//! Safe editor wrapper behind the `ed_*` C ABI. JSON shapes follow composerd rpc.rs.

use crate::blob::compile_project_with_sounds;
use crate::proposals::{self, app_error, Proposals};
use composer_commands::{
    command_meta, commands_enabled, describe, Command, CommandError, Editor, EnabledContext, Source,
};
use composer_engine::SampleBank;
use composer_engine::TempoClock;
use composer_format::{TransportPatch, TransportPrefs, DEFAULT_MAX_COUNT_IN_BARS};
use composer_model::{Id, Key, Project, Scale, Tick, ValidationError};
use composer_recipes::api;
use composer_sampler::{load_sf2, Limits};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Most history entries one `history.get` returns (no env in a page, D20).
pub const DEFAULT_MAX_HISTORY_PAGE: usize = 200;
const GROUP_TOKEN_MISMATCH: i64 = -32001;
const INVALID_PARAMS: i64 = -32602;
const COMMAND_REJECTED: i64 = -32000;
/// Project id of a fresh editor; callers pick other ids for their own objects.
const NEW_PROJECT_ID: Id = Id(u128::MAX);
/// Release tail after the last note in a compiled blob, in seconds.
const BLOB_TAIL_SECONDS: f64 = 0.25;
/// Rate used only to turn the playhead tick into seconds, same default as composerd.
const SECONDS_RATE: u32 = 48_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupParams {
    label: String,
    commands: Vec<Command>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct HistoryParams {
    limit: Option<usize>,
    #[serde(default)]
    offset: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupBeginParams {
    label: String,
    source: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenParams {
    token: Id,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct EnabledContextParams {
    track: Option<Id>,
    region: Option<Id>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct EnabledParams {
    #[serde(default)]
    context: EnabledContextParams,
}

/// Empty input means no params, so clients can call the read exports bare.
fn parse_or_default<T: serde::de::DeserializeOwned + Default>(json: &[u8]) -> Result<T, Value> {
    if json.is_empty() {
        return Ok(T::default());
    }
    serde_json::from_slice(json).map_err(|e| bad_params(e.to_string()))
}

pub struct EditorSession {
    editor: Editor,
    /// Token of the single open group (D32). The browser owns the timeout.
    open_token: Option<Id>,
    token_counter: u128,
    out: Vec<u8>,
    /// Session state outside the document (D21): not undoable, not in the state hash.
    playhead: Tick,
    prefs: TransportPrefs,
    /// D27: AI proposals waiting for a person; not in the state hash.
    proposals: Proposals,
    /// D25: when `Some(max)`, edit results carry a `changes` feed; over `max` changes it is `full`.
    feed_max: Option<usize>,
    /// Sampled sounds loaded by the page, by catalogue sound id (D37). Not in the document.
    sound_banks: BTreeMap<String, Arc<SampleBank>>,
}

pub(crate) fn hex(h: u64) -> String {
    format!("{h:016x}")
}

pub(crate) fn rejected(e: &CommandError, index: Option<usize>) -> Value {
    let mut data = json!({ "code": e.code() });
    if let Some(i) = index {
        data["index"] = json!(i);
    }
    json!({ "error": { "code": COMMAND_REJECTED, "message": e.to_string(), "data": data } })
}

/// Maps a recipe error onto the wire the same way composerd does.
fn recipe_error(e: composer_recipes::RecipeError) -> Value {
    if e.is_invalid_params() {
        bad_params(e.message)
    } else {
        app_error(e.code, &e.message)
    }
}

pub(crate) fn bad_params(msg: String) -> Value {
    json!({ "error": { "code": INVALID_PARAMS, "message": msg, "data": { "code": "invalid_params" } } })
}

fn invalid(e: &ValidationError) -> Value {
    let at = e.at;
    json!({ "error": { "code": COMMAND_REJECTED, "message": e.to_string(), "data": {
        "code": e.code(),
        "at": { "track_index": at.track_index, "region": at.region, "note": at.note },
    } } })
}

impl EditorSession {
    /// Fresh empty project; `None` when the name fails the command rules.
    pub fn new(name: &str) -> Option<Self> {
        let probe = Project::new(NEW_PROJECT_ID, "");
        Command::SetProjectName { name: name.into() }
            .plan(&probe)
            .ok()?;
        Some(Self {
            editor: Editor::new(Project::new(NEW_PROJECT_ID, name)),
            open_token: None,
            token_counter: 0,
            out: Vec::new(),
            playhead: 0,
            prefs: TransportPrefs::default(),
            proposals: Proposals::default(),
            feed_max: None,
            sound_banks: BTreeMap::new(),
        })
    }

    /// Turns the change feed on (`max_changes` above zero) or off (zero). Results then carry
    /// `changes {base_hash, hash, list}` or `{base_hash, hash, full: true}`.
    pub fn set_feed(&mut self, max_changes: usize) {
        self.feed_max = (max_changes > 0).then_some(max_changes);
        self.editor.track_changes(self.feed_max.is_some());
    }

    /// Hash at the start of an edit, computed only when a feed will be built (it walks the project).
    fn feed_base(&self) -> Option<u64> {
        self.feed_max.map(|_| self.state_hash())
    }

    /// Feed of the edit that started at hash `base`; drains the journal either way.
    fn feed(&mut self, base: Option<u64>) -> Option<Value> {
        let (max, base) = (self.feed_max?, base?);
        let list = self.editor.take_journal();
        let mut v = json!({ "base_hash": hex(base), "hash": hex(self.state_hash()) });
        if list.len() > max {
            v["full"] = json!(true);
        } else {
            v["list"] = json!(list);
        }
        Some(v)
    }

    /// Adds the feed to a success result and drops the journal after a rejection.
    fn with_feed(&mut self, mut v: Value, base: Option<u64>) -> Value {
        if v.get("result").is_none() {
            self.editor.take_journal();
            return v;
        }
        let feed = self.feed(base);
        if let (Some(f), Some(r)) = (feed, v.get_mut("result")) {
            r["changes"] = f;
        }
        v
    }

    pub fn state_hash(&self) -> u64 {
        self.editor.state_hash()
    }

    /// The result buffer of the latest call.
    pub fn result(&self) -> &[u8] {
        &self.out
    }

    fn summary(&self) -> Value {
        json!({
            "state_hash": hex(self.state_hash()),
            "undo_depth": self.editor.undo_depth(),
            "redo_depth": self.editor.redo_depth(),
            "undo_label": self.editor.undo_label(),
        })
    }

    fn with(&self, key: &str, v: Value) -> Value {
        let mut s = self.summary();
        s[key] = v;
        json!({ "result": s })
    }

    fn put(&mut self, v: &Value) {
        self.out = v.to_string().into_bytes();
    }

    /// One command object, e.g. `{"command":"add_track",...}`.
    pub fn apply(&mut self, json: &[u8]) {
        let base = self.feed_base();
        let v = match serde_json::from_slice::<Command>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(cmd) => {
                let before = self.state_hash();
                match self.editor.execute(&cmd) {
                    Err(e) => rejected(&e, None),
                    Ok(()) => self.with("changed", json!(before != self.state_hash())),
                }
            }
        };
        let v = self.with_feed(v, base);
        self.put(&v);
    }

    /// `{"label":"...","commands":[...]}`, atomic and one undo step.
    pub fn group(&mut self, json: &[u8]) {
        let base = self.feed_base();
        let v = match serde_json::from_slice::<GroupParams>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) if p.label.trim().is_empty() => bad_params("label must not be empty".into()),
            Ok(p) if p.commands.is_empty() => bad_params("commands must not be empty".into()),
            Ok(p) => {
                let mut failed = None;
                let run = self.editor.group(&p.label, |ed| {
                    for (i, c) in p.commands.iter().enumerate() {
                        if let Err(e) = ed.execute(c) {
                            failed = Some(i);
                            return Err(e);
                        }
                    }
                    Ok(())
                });
                match run {
                    Err(e) => rejected(&e, failed),
                    Ok(()) => json!({ "result": self.summary() }),
                }
            }
        };
        let v = self.with_feed(v, base);
        self.put(&v);
    }

    pub fn undo(&mut self) {
        let base = self.feed_base();
        let r = self.editor.undo();
        self.step_result(r, base);
    }

    pub fn redo(&mut self) {
        let base = self.feed_base();
        let r = self.editor.redo();
        self.step_result(r, base);
    }

    /// Undo or redo up to `steps` steps (D20); reports `steps_applied`.
    pub fn jump(&mut self, undo: bool, steps: u32) {
        let base = self.feed_base();
        let v = if steps == 0 {
            bad_params("steps must be at least 1".into())
        } else {
            let n = steps as usize;
            let r = if undo {
                self.editor.undo_steps(n)
            } else {
                self.editor.redo_steps(n)
            };
            match r {
                Err(e) => rejected(&e, None),
                Ok(done) => {
                    let mut v = self.with("applied", json!(done > 0));
                    v["result"]["steps_applied"] = json!(done);
                    v
                }
            }
        };
        let v = self.with_feed(v, base);
        self.put(&v);
    }

    /// `history.get` (D20): `{limit?, offset?}`, oldest first, with the cursor.
    pub fn history(&mut self, json: &[u8]) {
        let v = match parse_or_default::<HistoryParams>(json) {
            Err(v) => v,
            Ok(p) => {
                let max = DEFAULT_MAX_HISTORY_PAGE;
                let page = self
                    .editor
                    .history(p.offset, p.limit.unwrap_or(max).min(max));
                let entries: Vec<Value> = page
                    .entries
                    .iter()
                    .map(|e| {
                        json!({
                            "index": e.index,
                            "label": e.label,
                            "source": e.source.as_str(),
                            "command_count": e.command_count,
                        })
                    })
                    .collect();
                json!({ "result": {
                    "entries": entries,
                    "cursor": page.cursor,
                    "total": page.total,
                    "offset": p.offset,
                    "state_hash": hex(self.state_hash()),
                } })
            }
        };
        self.put(&v);
    }

    /// `group.begin` (D32): `{label, source?}`. One open group per session.
    pub fn group_begin(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<GroupBeginParams>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) if p.label.trim().is_empty() => bad_params("label must not be empty".into()),
            Ok(p) => match p
                .source
                .as_deref()
                .map_or(Some(Source::User), Source::parse)
            {
                None => bad_params("source must be user, ai, template or recipe".into()),
                Some(src) => match self.editor.begin_group_as(&p.label, src) {
                    Err(e) => rejected(&e, None),
                    Ok(()) => {
                        // No randomness in a page: a counter is enough to catch stale tokens.
                        self.token_counter += 1;
                        let token = Id(self.token_counter);
                        self.open_token = Some(token);
                        self.with("token", json!(token))
                    }
                },
            },
        };
        self.put(&v);
    }

    /// `group.commit` or `group.rollback`: `{token}`.
    pub fn group_close(&mut self, json: &[u8], commit: bool) {
        let base = self.feed_base();
        let v = match serde_json::from_slice::<TokenParams>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) => match self.open_token {
                None => rejected(&CommandError::NoOpenGroup, None),
                Some(t) if t != p.token => json!({ "error": {
                    "code": GROUP_TOKEN_MISMATCH,
                    "message": "the token does not match the open group",
                    "data": { "code": "group_token_mismatch" },
                } }),
                Some(_) => {
                    let r = if commit {
                        self.editor.commit_group()
                    } else {
                        self.editor.rollback_group()
                    };
                    self.open_token = None;
                    match r {
                        Err(e) => rejected(&e, None),
                        Ok(()) => json!({ "result": self.summary() }),
                    }
                }
            },
        };
        let v = self.with_feed(v, base);
        self.put(&v);
    }

    /// `proposal.post` (D27): `{label, commands, note?, author?, dry_run?}`.
    pub fn proposal_post(&mut self, json: &[u8]) {
        let v = proposals::post(&self.editor, &mut self.proposals, json);
        self.put(&v);
    }

    /// `proposal.list` (D27): `{status?}`, default pending.
    pub fn proposal_list(&mut self, json: &[u8]) {
        let v = proposals::list(&self.editor, &self.proposals, json);
        self.put(&v);
    }

    /// `proposal.resolve` (D27): `{id, action: accept or reject, dry_run?}`.
    pub fn proposal_resolve(&mut self, json: &[u8]) {
        let v = match proposals::resolve(&mut self.editor, &mut self.proposals, json) {
            Err(e) => e,
            Ok(extra) if extra["dry_run"] == true => json!({ "result": extra }),
            Ok(extra) => {
                let mut s = self.summary();
                s["id"] = extra["id"].clone();
                s["status"] = extra["status"].clone();
                json!({ "result": s })
            }
        };
        self.put(&v);
    }

    /// `templates.list` (D28): the bundled starter templates.
    pub fn templates_list(&mut self) {
        let v = json!({ "result": api::templates_list() });
        self.put(&v);
    }

    /// `recipes.list` (D28): recipes with their parameters.
    pub fn recipes_list(&mut self) {
        let v = json!({ "result": api::recipes_list() });
        self.put(&v);
    }

    /// `template.apply` (D28): `{template, seed?, dry_run?}`, one group with source template.
    pub fn template_apply(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<api::TemplateRequest>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(req) => match api::plan_template(self.editor.project(), &req) {
                Err(e) => recipe_error(e),
                Ok(planned) => self.run_planned(&planned, req.dry_run),
            },
        };
        self.put(&v);
    }

    /// `recipe.plan` (D28): `{recipe, params?, scope?, seed?}` returns the commands, applies nothing.
    pub fn recipe_plan(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<api::RecipeRequest>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(req) => match api::plan_recipe(self.editor.project(), &req) {
                Err(e) => recipe_error(e),
                Ok(planned) => match api::report(self.editor.project(), &planned) {
                    Ok(r) => json!({ "result": r }),
                    Err(f) => rejected(&f.error, f.index),
                },
            },
        };
        self.put(&v);
    }

    /// `recipe.apply` (D28): the same list as `recipe.plan`, applied as one group with source recipe.
    pub fn recipe_apply(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<api::RecipeRequest>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(req) => match api::plan_recipe(self.editor.project(), &req) {
                Err(e) => recipe_error(e),
                Ok(planned) => self.run_planned(&planned, req.dry_run),
            },
        };
        self.put(&v);
    }

    /// Dry run reports; otherwise applies the planned list as one group and adds the summary.
    fn run_planned(&mut self, planned: &api::Planned, dry_run: bool) -> Value {
        if dry_run {
            return match api::report(self.editor.project(), planned) {
                Ok(mut r) => {
                    r["dry_run"] = json!(true);
                    json!({ "result": r })
                }
                Err(f) => rejected(&f.error, f.index),
            };
        }
        let base = self.feed_base();
        let v = match api::apply(&mut self.editor, planned) {
            Err(f) => rejected(&f.error, f.index),
            Ok(()) => {
                let mut s = self.summary();
                if let (Some(o), Value::Object(extra)) =
                    (s.as_object_mut(), api::applied_fields(planned))
                {
                    o.extend(extra);
                }
                json!({ "result": s })
            }
        };
        self.with_feed(v, base)
    }

    /// `commands.enabled` (D33): `{context?: {track?, region?}}`.
    pub fn enabled(&mut self, json: &[u8]) {
        let v = match parse_or_default::<EnabledParams>(json) {
            Err(v) => v,
            Ok(p) => {
                let ctx = EnabledContext {
                    track: p.context.track,
                    region: p.context.region,
                };
                json!({ "result": { "commands": commands_enabled(self.editor.project(), ctx) } })
            }
        };
        self.put(&v);
    }

    fn step_result(&mut self, r: Result<bool, CommandError>, base: Option<u64>) {
        let v = match r {
            Ok(applied) => self.with("applied", json!(applied)),
            Err(e) => rejected(&e, None),
        };
        let v = self.with_feed(v, base);
        self.put(&v);
    }

    pub fn state(&mut self) {
        let v = json!({ "result": {
            "state_hash": hex(self.state_hash()),
            "project": self.editor.project(),
        } });
        self.put(&v);
    }

    pub fn describe(&mut self) {
        let list: Vec<Value> = describe()
            .iter()
            .map(|c| {
                let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
                if let (Some(o), Some(m)) = (v.as_object_mut(), command_meta(c.name)) {
                    o.insert("verb".into(), json!(m.verb));
                    o.insert("label".into(), json!(m.label));
                    o.insert("keywords".into(), json!(m.keywords));
                }
                v
            })
            .collect();
        self.put(&json!({ "result": { "commands": list } }));
    }

    /// Replaces the project (history is cleared) after parse and a full `Project::validate`.
    pub fn load(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<Project>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) => match p.validate() {
                Err(e) => invalid(&e),
                Ok(()) => {
                    self.editor = Editor::new(p);
                    self.editor.track_changes(self.feed_max.is_some());
                    self.open_token = None;
                    self.playhead = 0;
                    self.prefs = TransportPrefs::default();
                    self.proposals = Proposals::default();
                    json!({ "result": self.summary() })
                }
            },
        };
        self.put(&v);
    }

    fn transport_json(&self, prefs: &TransportPrefs, tick: Tick) -> Value {
        let project = self.editor.project();
        let frames = TempoClock::new(&project.tempo_map, SECONDS_RATE).frame_at(tick);
        let end = project
            .tracks
            .iter()
            .flat_map(|t| t.regions.values())
            .map(|r| r.start + r.length)
            .max()
            .unwrap_or(0);
        let mut v = json!({
            "playhead_tick": tick,
            "playhead_seconds": frames as f64 / f64::from(SECONDS_RATE),
            "playing": false,
            "end_tick": end,
        });
        if let (Some(o), Ok(Value::Object(p))) = (v.as_object_mut(), serde_json::to_value(prefs)) {
            o.extend(p);
        }
        v
    }

    /// `transport.get`: playhead plus the D21 prefs.
    pub fn transport_get(&mut self) {
        let v = json!({ "result": self.transport_json(&self.prefs, self.playhead) });
        self.put(&v);
    }

    /// `transport.set`: every field optional, merged and validated like composerd.
    pub fn transport_set(&mut self, json: &[u8]) {
        let v = match serde_json::from_slice::<TransportPatch>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) if p.is_empty() => rejected(
                &CommandError::Invalid {
                    field: "params",
                    reason: "no transport field given",
                },
                None,
            ),
            Ok(p) => match p
                .check_tick()
                .and_then(|()| self.prefs.patched(&p, DEFAULT_MAX_COUNT_IN_BARS))
            {
                Err(e) => rejected(
                    &CommandError::Invalid {
                        field: e.field,
                        reason: e.reason,
                    },
                    None,
                ),
                Ok(next) if p.dry_run => {
                    let mut r = self.transport_json(&next, p.tick.unwrap_or(self.playhead));
                    r["dry_run"] = json!(true);
                    json!({ "result": r })
                }
                Ok(next) => {
                    self.playhead = p.tick.unwrap_or(self.playhead);
                    self.prefs = next;
                    json!({ "result": self.transport_json(&self.prefs, self.playhead) })
                }
            },
        };
        self.put(&v);
    }

    /// `theory.scale` (D23): `{"root":0..11,"scale":name}` to the scale pitch classes in
    /// degree order. Reads nothing from the project.
    pub fn scale_info(&mut self, json: &[u8]) {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Params {
            root: u8,
            scale: Scale,
        }
        let v = match serde_json::from_slice::<Params>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) => {
                let key = Key {
                    root: p.root,
                    scale: p.scale,
                };
                if key.is_valid() {
                    json!({ "result": {
                        "root": key.root,
                        "scale": key.scale.name(),
                        "pitch_classes": key.pitch_classes(),
                        "mask": key.mask(),
                    } })
                } else {
                    bad_params("root must be 0 (C) to 11 (B)".into())
                }
            }
        };
        self.put(&v);
    }

    /// `theory.chords` (D23b): diatonic triads and sevenths of a key. Params are
    /// `{"key":{"root","scale"}}` or the flat `{"root","scale"}`, plus an optional
    /// `octave` (-1..8, default 4). Reads nothing from the project.
    pub fn chords_info(&mut self, json: &[u8]) {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct KeyParams {
            root: u8,
            scale: Scale,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Params {
            key: Option<KeyParams>,
            root: Option<u8>,
            scale: Option<Scale>,
            octave: Option<i8>,
        }
        let v = match serde_json::from_slice::<Params>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) => {
                let picked = match (p.key, p.root, p.scale) {
                    (Some(k), None, None) => Some((k.root, k.scale)),
                    (None, Some(r), Some(s)) => Some((r, s)),
                    _ => None,
                };
                let octave = p.octave.unwrap_or(4);
                match picked {
                    None => bad_params("give key {root, scale} or root and scale".into()),
                    Some((root, scale)) => {
                        let key = Key { root, scale };
                        if !key.is_valid() {
                            bad_params("root must be 0 (C) to 11 (B)".into())
                        } else if !(-1..=8).contains(&octave) {
                            bad_params("octave must be -1 to 8".into())
                        } else {
                            json!({ "result": {
                                "root": key.root,
                                "scale": key.scale.name(),
                                "root_name": composer_theory::key_root_name(&key),
                                "octave": octave,
                                "triads": composer_theory::chords(&key, false, octave),
                                "sevenths": composer_theory::chords(&key, true, octave),
                            } })
                        }
                    }
                }
            }
        };
        self.put(&v);
    }

    /// `theory.pitch` (D23b): `{"root","scale","pitch"}` to whether the pitch is in key, its
    /// 1 based scale degree (null outside the key) and the nearest in key pitch.
    pub fn pitch_info(&mut self, json: &[u8]) {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Params {
            root: u8,
            scale: Scale,
            pitch: u8,
        }
        let v = match serde_json::from_slice::<Params>(json) {
            Err(e) => bad_params(e.to_string()),
            Ok(p) => {
                let key = Key {
                    root: p.root,
                    scale: p.scale,
                };
                if !key.is_valid() {
                    bad_params("root must be 0 (C) to 11 (B)".into())
                } else if p.pitch > 127 {
                    bad_params("pitch must be 0 to 127".into())
                } else {
                    json!({ "result": {
                        "pitch": p.pitch,
                        "in_key": key.contains(p.pitch),
                        "degree": composer_theory::degree_of(&key, p.pitch),
                        "nearest": composer_theory::nearest_in_key(&key, p.pitch),
                    } })
                }
            }
        };
        self.put(&v);
    }

    /// Engine blob v1 bytes of the current project; false when the rate is zero.
    pub fn engine_blob(&mut self, sample_rate: u32) -> bool {
        if sample_rate == 0 {
            return false;
        }
        self.out = compile_project_with_sounds(
            self.editor.project(),
            sample_rate,
            BLOB_TAIL_SECONDS,
            &self.sound_banks,
        );
        true
    }

    /// Reads a SoundFont 2 file and keeps it as the bank of catalogue sound `id`. Returns the
    /// stable sampler error code on failure; a bad file leaves earlier banks untouched.
    pub fn load_sound_bank(&mut self, id: &str, sf2: &[u8]) -> Result<(), &'static str> {
        let ok = !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !ok {
            return Err("sound.bad_id");
        }
        let bank = load_sf2(sf2, &Limits::default(), None).map_err(|e| e.code())?;
        self.sound_banks.insert(id.to_string(), Arc::new(bank));
        Ok(())
    }
}
