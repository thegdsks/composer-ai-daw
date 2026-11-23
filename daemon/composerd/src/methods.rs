//! The T17 methods (D16, D17): file operations and reads. They hold no music
//! logic: bytes come from composer-format, composer-midi and composer-render,
//! the tree and selection rules from composer-commands.

use crate::files::{self, FileConfig, FileError, Kind};
use crate::hub::{random_id, Inner, OpenGroupInfo};
use crate::rpc::{
    dry_report, hex, parse_params, parse_source, plan_on_copy, summary, with, RpcError,
};
use composer_commands::{
    commands_enabled, project_tree, CommandError, EnabledContext, NoteRef, Selection, TreeOptions,
};
use composer_engine::TempoClock;
use composer_format::{read_project, write_project, Limits, Meta, TransportPatch, TransportPrefs};
use composer_midi::{export_smf, import_smf, ImportOptions};
use composer_model::{Id, Project, Tick};
use composer_render::{
    content_end_frame, measure, render_project, write_wav, RenderSettings, WavFormat,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

impl From<FileError> for RpcError {
    fn from(e: FileError) -> Self {
        RpcError::app(e.code, e.message)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeParams {
    #[serde(default)]
    notes: bool,
    track: Option<Id>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionParams {
    #[serde(default)]
    tracks: Vec<Id>,
    #[serde(default)]
    regions: Vec<Id>,
    #[serde(default)]
    notes: Vec<NoteRef>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeterParams {
    sample_rate: Option<u32>,
    tail_seconds: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SoundsParams {
    bundle: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveParams {
    path: String,
    #[serde(default)]
    overwrite: bool,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadParams {
    path: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    path: String,
    seed: Option<u64>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderParams {
    path: String,
    sample_rate: Option<u32>,
    tail_seconds: Option<f64>,
    format: Option<String>,
    #[serde(default)]
    overwrite: bool,
    #[serde(default)]
    dry_run: bool,
}

fn invalid(field: &'static str, reason: &'static str) -> RpcError {
    RpcError::command(&CommandError::Invalid { field, reason }, None)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn span_ticks(p: &Project) -> Tick {
    p.tracks
        .iter()
        .flat_map(|t| t.regions.values())
        .map(|r| r.start + r.length)
        .max()
        .unwrap_or(0)
}

/// Render settings from the request and the env defaults, with bounds on both.
fn render_settings(
    p: &Project,
    cfg: &FileConfig,
    rate: Option<u32>,
    tail: Option<f64>,
) -> Result<RenderSettings, RpcError> {
    let sr = rate.unwrap_or(cfg.render_sample_rate);
    if !(8_000..=192_000).contains(&sr) {
        return Err(invalid("sample_rate", "must be 8000 to 192000"));
    }
    let tail = tail.unwrap_or(f64::from(cfg.render_tail_seconds));
    if !tail.is_finite() || tail < 0.0 || tail > cfg.max_render_seconds as f64 {
        return Err(invalid("tail_seconds", "out of range"));
    }
    let mut s = RenderSettings::new(sr, tail).with_project_mix(p);
    s.audio = Some(crate::audio::store(cfg)?.into_provider());
    s.audio_limits = cfg.audio;
    let frames = content_end_frame(p, sr) + (tail * f64::from(sr)).round() as u64;
    if frames > cfg.max_render_seconds.saturating_mul(u64::from(sr)) {
        return Err(RpcError::app(
            "render_too_long",
            format!(
                "longer than the {} second render limit",
                cfg.max_render_seconds
            ),
        ));
    }
    Ok(s)
}

fn db(level: f32) -> Value {
    if level > 0.0 {
        json!((f64::from(20.0 * level.log10()) * 100.0).round() / 100.0)
    } else {
        Value::Null
    }
}

fn meta(prefs: &TransportPrefs) -> Meta {
    let now = now_ms();
    Meta {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        created_ms: now,
        modified_ms: now,
        transport: (!prefs.is_default()).then_some(*prefs),
    }
}

/// Routes the T17 methods. `None` means the method is not one of them.
pub(crate) fn dispatch(
    g: &mut Inner,
    client: u64,
    method: &str,
    params: Value,
) -> Option<Result<Value, RpcError>> {
    Some(match method {
        "history.get" => history_get(g, params),
        "templates.list" => crate::recipes::templates_list(g, params),
        "template.apply" => crate::recipes::template_apply(g, params),
        "recipes.list" => crate::recipes::recipes_list(g, params),
        "recipe.plan" => crate::recipes::recipe_plan(g, params),
        "recipe.apply" => crate::recipes::recipe_apply(g, params),
        "proposal.post" => crate::proposals::post(g, params),
        "proposal.list" => crate::proposals::list(g, params),
        "proposal.resolve" => crate::proposals::resolve(g, params),
        "group.begin" => group_begin(g, client, params),
        "group.commit" => group_close(g, params, true),
        "group.rollback" => group_close(g, params, false),
        "commands.enabled" => enabled(g, params),
        "project.tree" => tree(g, params),
        "selection.get" => selection_get(g),
        "selection.set" => selection_set(g, params),
        "transport.get" => transport_get(g),
        "transport.set" => transport_set(g, params),
        "meters.measure" => meters(g, params),
        "sounds.list" => sounds(g, params),
        "theory.scale" => theory_scale(params),
        "theory.chords" => theory_chords(params),
        "theory.pitch" => theory_pitch(params),
        "project.save" => save(g, params),
        "project.load" => load(g, params),
        "export.midi" => export_midi(g, params),
        "import.midi" => import_midi(g, params),
        "render.wav" => render_wav(g, params),
        "audio.import" => crate::audio::import(g, params),
        "audio.assets" => crate::audio::assets(g, params),
        "audio.peaks" => crate::audio::peaks(g, params),
        _ => return None,
    })
}

#[derive(Deserialize)]
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

/// D20: a page of steps, oldest first, with the cursor between applied and undone.
fn history_get(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: HistoryParams = parse_params(params)?;
    let max = g.files.max_history_page;
    let limit = p.limit.unwrap_or(max).min(max);
    let page = g.editor.history(p.offset, limit);
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
    Ok(json!({
        "entries": entries,
        "cursor": page.cursor,
        "total": page.total,
        "offset": p.offset,
        "state_hash": hex(g.editor.state_hash()),
    }))
}

/// D32: open a group; commands applied afterwards are live and join it.
fn group_begin(g: &mut Inner, client: u64, params: Value) -> Result<Value, RpcError> {
    let p: GroupBeginParams = parse_params(params)?;
    if p.label.trim().is_empty() {
        return Err(RpcError::invalid_params("label must not be empty"));
    }
    let source = parse_source(&p.source)?;
    g.editor
        .begin_group_as(&p.label, source)
        .map_err(|e| RpcError::command(&e, None))?;
    let token = random_id();
    g.open_group = Some(OpenGroupInfo {
        token,
        owner: client,
        opened: Instant::now(),
    });
    Ok(with(
        summary(&g.editor),
        json!({ "token": token, "timeout_ms": g.files.group_timeout_ms }),
    ))
}

fn group_close(g: &mut Inner, params: Value, commit: bool) -> Result<Value, RpcError> {
    let p: TokenParams = parse_params(params)?;
    match &g.open_group {
        None => {
            return Err(RpcError::command(&CommandError::NoOpenGroup, None));
        }
        Some(o) if o.token != p.token => {
            return Err(RpcError::app(
                "group_token_mismatch",
                "the token does not match the open group",
            ));
        }
        Some(_) => {}
    }
    let r = if commit {
        g.editor.commit_group()
    } else {
        g.editor.rollback_group()
    };
    g.open_group = None;
    r.map_err(|e| RpcError::command(&e, None))?;
    Ok(summary(&g.editor).into())
}

/// D33: which commands could run now, with a plain reason when not.
fn enabled(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: EnabledParams = parse_params(params)?;
    let ctx = EnabledContext {
        track: p.context.track,
        region: p.context.region,
    };
    Ok(json!({ "commands": commands_enabled(g.editor.project(), ctx) }))
}

fn tree(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: TreeParams = parse_params(params)?;
    let project = g.editor.project();
    let tree = project_tree(
        project,
        TreeOptions {
            notes: p.notes,
            track: p.track,
        },
    )
    .map_err(|e| RpcError::command(&e, None))?;
    Ok(json!({
        "state_hash": hex(g.editor.state_hash()),
        "tree": tree,
        "selection": g.selection.pruned(project),
    }))
}

fn selection_get(g: &mut Inner) -> Result<Value, RpcError> {
    let project = g.editor.project();
    let sel = g.selection.pruned(project);
    Ok(json!({ "state_hash": hex(g.editor.state_hash()), "selection": sel }))
}

fn selection_set(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: SelectionParams = parse_params(params)?;
    let sel = Selection {
        tracks: p.tracks,
        regions: p.regions,
        notes: p.notes,
    };
    sel.validate(g.editor.project())
        .map_err(|e| RpcError::command(&e, None))?;
    if p.dry_run {
        return Ok(json!({ "dry_run": true, "valid": true, "selection": sel }));
    }
    g.selection = sel.clone();
    Ok(json!({ "selection": sel }))
}

fn playhead_json(g: &Inner) -> Value {
    let project = g.editor.project();
    let sr = g.files.render_sample_rate;
    let frames = TempoClock::new(&project.tempo_map, sr).frame_at(g.playhead);
    let mut v = json!({
        "playhead_tick": g.playhead,
        "playhead_seconds": frames as f64 / f64::from(sr),
        "playing": false,
        "end_tick": span_ticks(project),
    });
    merge_prefs(&mut v, &g.prefs);
    v
}

fn merge_prefs(v: &mut Value, prefs: &TransportPrefs) {
    if let (Some(o), Ok(Value::Object(p))) = (v.as_object_mut(), serde_json::to_value(prefs)) {
        o.extend(p);
    }
}

fn transport_get(g: &mut Inner) -> Result<Value, RpcError> {
    Ok(playhead_json(g))
}

fn transport_set(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: TransportPatch = parse_params(params)?;
    if p.is_empty() {
        return Err(invalid("params", "no transport field given"));
    }
    p.check_tick().map_err(|e| invalid(e.field, e.reason))?;
    let next = g
        .prefs
        .patched(&p, g.files.max_count_in_bars)
        .map_err(|e| invalid(e.field, e.reason))?;
    if p.dry_run {
        let mut v = json!({ "dry_run": true, "playhead_tick": p.tick.unwrap_or(g.playhead) });
        merge_prefs(&mut v, &next);
        return Ok(v);
    }
    if let Some(t) = p.tick {
        g.playhead = t;
    }
    g.prefs = next;
    Ok(playhead_json(g))
}

fn meters(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: MeterParams = parse_params(params)?;
    let project = g.editor.project();
    let settings = render_settings(project, &g.files, p.sample_rate, p.tail_seconds)?;
    let r = measure(project, &settings);
    let tracks: Vec<Value> = r
        .tracks
        .iter()
        .map(|(id, l)| {
            let name = project.track(*id).map(|t| t.name.clone());
            json!({ "track": id, "name": name, "peak": l.peak, "rms": l.rms, "peak_db": db(l.peak) })
        })
        .collect();
    Ok(json!({
        "state_hash": hex(g.editor.state_hash()),
        "offline": true,
        "sample_rate": r.sample_rate,
        "frames": r.frames,
        "seconds": r.frames as f64 / f64::from(r.sample_rate),
        "clipped_frames": r.clipped_frames,
        "master": { "peak": r.master.peak, "rms": r.master.rms, "peak_db": db(r.master.peak) },
        "tracks": tracks,
    }))
}

fn sounds(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: SoundsParams = parse_params(params)?;
    let cfg = &g.files;
    let meta = std::fs::metadata(&cfg.sounds_file).map_err(|_| {
        RpcError::app(
            "catalogue_unavailable",
            "the sound catalogue file is missing",
        )
    })?;
    if meta.len() > cfg.max_file_bytes {
        return Err(RpcError::app(
            "file_too_large",
            "the sound catalogue is too large",
        ));
    }
    let text = std::fs::read_to_string(&cfg.sounds_file).map_err(|_| {
        RpcError::app(
            "catalogue_unavailable",
            "the sound catalogue cannot be read",
        )
    })?;
    let cat =
        composer_catalogue::parse(&text).map_err(|e| RpcError::app(e.code(), e.to_string()))?;
    let entries: Vec<_> = cat
        .entries
        .iter()
        .filter(|e| p.bundle.as_ref().is_none_or(|b| *b == e.bundle))
        .collect();
    Ok(json!({ "count": entries.len(), "entries": entries, "gaps": cat.gaps }))
}

fn save(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: SaveParams = parse_params(params)?;
    let path = files::resolve(&g.files, &p.path, Kind::Project)?;
    let project = g.editor.project();
    let bytes = write_project(project, &g.assets, &meta(&g.prefs));
    if p.dry_run {
        return Ok(json!({
            "dry_run": true,
            "path": p.path,
            "bytes": bytes.len(),
            "would_overwrite": files::exists(&path),
        }));
    }
    files::write(&path, &bytes, p.overwrite)?;
    Ok(json!({
        "path": p.path,
        "bytes": bytes.len(),
        "state_hash": hex(g.editor.state_hash()),
    }))
}

fn load(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: LoadParams = parse_params(params)?;
    let path = files::resolve(&g.files, &p.path, Kind::Project)?;
    let bytes = files::read(&g.files, &path)?;
    let limits = Limits {
        max_file_bytes: g.files.max_file_bytes,
        ..Limits::default()
    };
    let loaded =
        read_project(&bytes, &limits).map_err(|e| RpcError::app(e.code(), e.to_string()))?;
    let (tracks, name) = (loaded.project.tracks.len(), loaded.project.name.clone());
    if p.dry_run {
        return Ok(dry_report(
            g.editor.project(),
            &loaded.project,
            vec![json!("replace_session")],
        ));
    }
    g.replace_project(loaded.project);
    g.assets = loaded.assets;
    g.prefs = loaded.meta.transport.unwrap_or_default();
    Ok(with(
        summary(&g.editor),
        json!({ "path": p.path, "name": name, "tracks": tracks, "format_version": loaded.version }),
    ))
}

fn export_midi(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: SaveParams = parse_params(params)?;
    let path = files::resolve(&g.files, &p.path, Kind::Midi)?;
    let bytes = export_smf(g.editor.project());
    if p.dry_run {
        return Ok(json!({
            "dry_run": true,
            "path": p.path,
            "bytes": bytes.len(),
            "would_overwrite": files::exists(&path),
        }));
    }
    files::write(&path, &bytes, p.overwrite)?;
    Ok(json!({ "path": p.path, "bytes": bytes.len() }))
}

fn import_midi(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: ImportParams = parse_params(params)?;
    let path = files::resolve(&g.files, &p.path, Kind::Midi)?;
    let bytes = files::read(&g.files, &path)?;
    // A random seed by default so a second import never collides with the first one's ids.
    let seed = p.seed.unwrap_or_else(|| random_id().0 as u64);
    let plan = import_smf(
        &bytes,
        &ImportOptions {
            seed,
            ..ImportOptions::default()
        },
    )
    .map_err(|e| RpcError::app("midi_invalid", e.to_string()))?;
    let stats = json!({
        "notes": plan.stats.notes,
        "unmatched_note_offs": plan.stats.unmatched_note_offs,
        "unterminated_notes": plan.stats.unterminated_notes,
        "commands": plan.commands.len(),
    });
    if p.dry_run {
        let (after, changes) = plan_on_copy(g.editor.project(), &plan.commands)?;
        let mut report = dry_report(g.editor.project(), &after, changes);
        report["stats"] = stats;
        return Ok(report);
    }
    let mut failed = None;
    g.editor
        .group("import midi", |ed| {
            for (i, c) in plan.commands.iter().enumerate() {
                if let Err(e) = ed.execute(c) {
                    failed = Some(i);
                    return Err(e);
                }
            }
            Ok(())
        })
        .map_err(|e| RpcError::command(&e, failed))?;
    Ok(with(summary(&g.editor), json!({ "stats": stats })))
}

fn render_wav(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: RenderParams = parse_params(params)?;
    let format = match p.format.as_deref() {
        None | Some("pcm16") => WavFormat::Pcm16,
        Some("pcm24") => WavFormat::Pcm24,
        Some("float32") => WavFormat::Float32,
        Some(_) => return Err(invalid("format", "must be pcm16, pcm24 or float32")),
    };
    let path = files::resolve(&g.files, &p.path, Kind::Wav)?;
    let project = g.editor.project();
    let settings = render_settings(project, &g.files, p.sample_rate, p.tail_seconds)?;
    if p.dry_run {
        let sr = settings.sample_rate;
        let frames =
            content_end_frame(project, sr) + (settings.tail_seconds * f64::from(sr)).round() as u64;
        return Ok(json!({
            "dry_run": true,
            "path": p.path,
            "sample_rate": sr,
            "frames": frames,
            "seconds": frames as f64 / f64::from(sr),
            "would_overwrite": files::exists(&path),
        }));
    }
    let r = render_project(project, &settings);
    let bytes = write_wav(format, 2, r.sample_rate, &r.samples);
    files::write(&path, &bytes, p.overwrite)?;
    let fallbacks: Vec<Value> = r
        .fallbacks
        .iter()
        .map(|f| json!({ "track": f.track, "instrument": f.instrument }))
        .collect();
    Ok(json!({
        "path": p.path,
        "bytes": bytes.len(),
        "sample_rate": r.sample_rate,
        "frames": r.frames,
        "seconds": r.frames as f64 / f64::from(r.sample_rate),
        "clipped_frames": r.clipped_frames,
        "fallbacks": fallbacks,
        "audio_issues": r.audio_issues.iter().map(|i| json!({ "region": i.region, "code": i.code })).collect::<Vec<_>>(),
        "audio_resident_high_water": r.audio_resident_high_water,
        "state_hash": hex(g.editor.state_hash()),
    }))
}

// Theory reads (D35): pure calls into composer-theory, no session state.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScaleParams {
    root: u8,
    scale: composer_theory::Scale,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChordsParams {
    key: Option<ScaleParams>,
    root: Option<u8>,
    scale: Option<composer_theory::Scale>,
    octave: Option<i8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PitchParams {
    key: Option<ScaleParams>,
    root: Option<u8>,
    scale: Option<composer_theory::Scale>,
    pitch: u8,
}

fn theory_value<T: Serialize>(r: Result<T, &'static str>) -> Result<Value, RpcError> {
    let v = r.map_err(RpcError::invalid_params)?;
    serde_json::to_value(v).map_err(|e| RpcError::invalid_params(e.to_string()))
}

/// Accepts `key: {root, scale}` or the flat `root` and `scale`, never both.
fn pick_key(
    key: Option<ScaleParams>,
    root: Option<u8>,
    scale: Option<composer_theory::Scale>,
) -> Result<composer_theory::Key, RpcError> {
    match (key, root, scale) {
        (Some(k), None, None) => Ok(composer_theory::Key {
            root: k.root,
            scale: k.scale,
        }),
        (None, Some(root), Some(scale)) => Ok(composer_theory::Key { root, scale }),
        _ => Err(RpcError::invalid_params(
            "give key {root, scale} or root and scale",
        )),
    }
}

fn theory_scale(params: Value) -> Result<Value, RpcError> {
    let p: ScaleParams = parse_params(params)?;
    let key = composer_theory::Key {
        root: p.root,
        scale: p.scale,
    };
    theory_value(composer_theory::scale_info(&key))
}

fn theory_chords(params: Value) -> Result<Value, RpcError> {
    let p: ChordsParams = parse_params(params)?;
    let key = pick_key(p.key, p.root, p.scale)?;
    theory_value(composer_theory::chords_info(&key, p.octave.unwrap_or(4)))
}

fn theory_pitch(params: Value) -> Result<Value, RpcError> {
    let p: PitchParams = parse_params(params)?;
    let key = pick_key(p.key, p.root, p.scale)?;
    theory_value(composer_theory::pitch_info(&key, p.pitch))
}
