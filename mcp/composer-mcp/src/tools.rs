//! The MCP tool set: one tool per described command plus fixed helpers.

use crate::backend::{Backend, CallError};
use crate::config::Config;
use crate::schema::{command_fields_schema, tagged_schema, validate, ID_PATTERN};
use crate::summary::summarize;
use serde_json::{json, Map, Value};

/// Recipe names the daemon accepts (composer-recipes); a test keeps this list honest.
pub const RECIPE_NAMES: [&str; 6] = [
    "extend_to_length",
    "repeat_with_variation",
    "bass_from_chords",
    "pad_under_melody",
    "soften_velocity",
    "calm_sections",
];

/// Template ids the daemon accepts (composer-recipes); a test keeps this list honest.
pub const TEMPLATE_IDS: [&str; 4] = ["recreate", "cinematic", "calm-hour", "blank"];

/// Scale names the daemon accepts (composer-theory); a daemon test keeps this list honest.
pub const SCALE_NAMES: [&str; 7] = [
    "major",
    "minor",
    "dorian",
    "mixolydian",
    "pentatonic_major",
    "pentatonic_minor",
    "harmonic_minor",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// One described command, by its stable name.
    Command(String),
    StateGet,
    Summary,
    StateHash,
    Undo,
    Redo,
    Group,
    SessionNew,
    DryRun,
    NewId,
    /// A read or file method passed straight to the daemon (D16, D17).
    Rpc(&'static str),
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub hints: Hints,
    pub kind: Kind,
}

/// Behaviour hints, reported honestly. No tool reaches outside the session, so
/// `openWorldHint` is always false.
#[derive(Debug, Clone, Copy)]
pub struct Hints {
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: bool,
}

impl Hints {
    const READ: Self = Self {
        read_only: true,
        destructive: false,
        idempotent: true,
    };
    /// Overwrites or removes existing content; a repeat with the same ids is a no-op.
    const EDIT: Self = Self {
        read_only: false,
        destructive: true,
        idempotent: true,
    };
    /// Only adds; a repeat with the same id is refused, so nothing more happens.
    const ADD: Self = Self {
        read_only: false,
        destructive: false,
        idempotent: true,
    };
    /// Each call moves along the undo history.
    const STEP: Self = Self {
        read_only: false,
        destructive: false,
        idempotent: false,
    };
    /// Writes a generated list as one undo step: may overwrite name, tempo or notes, and a
    /// repeat adds more material.
    const APPLY: Self = Self {
        read_only: false,
        destructive: true,
        idempotent: false,
    };
    /// Replaces the whole session; every call makes a fresh project id.
    const RESET: Self = Self {
        read_only: false,
        destructive: true,
        idempotent: false,
    };
}

impl Tool {
    /// The `tools/list` entry for the negotiated protocol revision. Fields a
    /// revision does not define are left out so old clients see only their own.
    pub fn listing(&self, version: &str) -> Value {
        let mut t = json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        });
        if version >= "2025-06-18" {
            t["title"] = json!(self.title);
            t["outputSchema"] = self.output_schema.clone();
        }
        if version >= "2025-03-26" {
            t["annotations"] = json!({
                "title": self.title,
                "readOnlyHint": self.hints.read_only,
                "destructiveHint": self.hints.destructive,
                "idempotentHint": self.hints.idempotent,
                "openWorldHint": false,
            });
        }
        t
    }
}

/// "add_note" becomes "Add Note".
fn title_of(name: &str) -> String {
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every tool failure carries this shape as `structuredContent`.
fn error_shape() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": { "type": "string" },
            "message": { "type": "string" },
            "hint": { "type": "string" },
            "index": { "type": "integer", "minimum": 0 },
        },
        "required": ["code", "message", "hint"],
    })
}

/// The output schema: the success shape, or the shared error shape (a failed
/// call still returns structuredContent, so both must be allowed).
fn output_schema(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "anyOf": [
            { "type": "object", "properties": props, "required": required },
            error_shape(),
        ],
    })
}

/// The shape of an edit reply, plus any extra fields the tool adds.
fn edit_output(extra: &[(&str, Value)], extra_required: &[&str]) -> Value {
    let mut props = json!({
        "state_hash": { "type": "string" },
        "undo_depth": { "type": "integer", "minimum": 0 },
        "redo_depth": { "type": "integer", "minimum": 0 },
        "undo_label": { "type": ["string", "null"] },
    });
    for (k, v) in extra {
        props[*k] = v.clone();
    }
    let mut req = vec!["state_hash", "undo_depth", "redo_depth", "undo_label"];
    req.extend_from_slice(extra_required);
    output_schema(props, &req)
}

/// A failed tool call: a stable `code`, what happened and what to do next.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub hint: String,
    pub index: Option<u64>,
}

impl ToolError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: hint(code).into(),
            index: None,
        }
    }

    pub fn to_value(&self) -> Value {
        let mut v = json!({ "code": self.code, "message": self.message, "hint": self.hint });
        if let Some(i) = self.index {
            v["index"] = json!(i);
        }
        v
    }

    pub fn to_text(&self) -> String {
        let at = self
            .index
            .map(|i| format!(" (command {i} of the list)"))
            .unwrap_or_default();
        format!("{}: {}{at}. Hint: {}", self.code, self.message, self.hint)
    }
}

/// Plain advice for each stable error code. Nothing is changed on any error.
fn hint(code: &str) -> &'static str {
    match code {
        "not_found" => "An id does not exist. Read it from composer_state_get or create it first.",
        "duplicate_id" => "That id is taken. Call composer_new_id for a fresh one.",
        "invalid" => "A value is out of range. Check the units and limits in the tool schema.",
        "wrong_kind" => "The command does not fit this track or clip kind (midi clips on instrument tracks, audio on audio tracks).",
        "out_of_bounds" => "A note must lie inside its clip. Resize the clip first or shorten the note.",
        "group_already_open" | "no_open_group" => "Group state is managed for you. Retry the call.",
        "apply_failed" => "The change could not be applied. Read state with composer_state_get and retry.",
        "invalid_params" | "invalid_request" | "parse_error" => "The request shape was wrong. Match the tool schema exactly.",
        "invalid_arguments" => "Fix the arguments to match the tool schema. Nothing was changed.",
        "too_many_commands" | "too_many_ids" => "Split the work into smaller calls.",
        "method_not_found" => "The daemon does not support this call. Versions may differ.",
        "daemon_unavailable" => "The composerd connection is gone. Restart this MCP server.",
        "timeout" => "composerd did not answer in time. Check state with composer_state_hash before retrying, the edit may have applied.",
        "reply_too_large" => "The reply was over the size limit. Ask for less, for example composer_project_summary.",
        _ => "Unexpected daemon error. Check composer_state_hash, then retry or restart.",
    }
}

fn from_call(e: CallError) -> ToolError {
    match e {
        CallError::Remote {
            code,
            message,
            index,
        } => ToolError {
            hint: hint(&code).into(),
            code,
            message,
            index,
        },
        CallError::Transport { code, message } => ToolError::new(code, message),
    }
}

const UNITS: &str = "Units: time in ticks (960 per quarter note, 3840 per 4/4 bar), tempo in thousandths of a BPM, ids are 32 lowercase hex digits (get new ones from composer_new_id), pitch 0..127.";

/// Builds the tool list from the daemon's `commands.describe` array.
pub fn build_tools(commands: &Value, cfg: &Config) -> Vec<Tool> {
    let list = commands.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut tools = Vec::new();
    let mut variants = Vec::new();
    for info in list {
        let Some(name) = info["name"].as_str() else {
            continue;
        };
        variants.push(tagged_schema(info));
        tools.push(Tool {
            name: format!("composer_{name}"),
            title: title_of(name),
            output_schema: edit_output(
                &[("changed", json!({ "type": "boolean" }))],
                &["changed"],
            ),
            hints: if name.starts_with("add_") {
                Hints::ADD
            } else {
                Hints::EDIT
            },
            description: format!(
                "{} Edits the project as one undo step and returns state_hash, undo_depth, redo_depth and undo_label. {UNITS}",
                info["summary"].as_str().unwrap_or_default()
            ),
            input_schema: command_fields_schema(info),
            kind: Kind::Command(name.to_string()),
        });
    }
    let no_args = json!({ "type": "object", "properties": {}, "additionalProperties": false });
    let fixed = |name: &str,
                 title: &str,
                 desc: &str,
                 schema: Value,
                 out: Value,
                 hints: Hints,
                 kind: Kind| Tool {
        name: name.into(),
        title: title.into(),
        description: desc.into(),
        input_schema: schema,
        output_schema: out,
        hints,
        kind,
    };
    let step_out = edit_output(
        &[
            ("applied", json!({ "type": "boolean" })),
            ("steps_applied", json!({ "type": "integer", "minimum": 0 })),
        ],
        &["applied"],
    );
    let steps_args = json!({
        "type": "object",
        "properties": {
            "steps": { "type": "integer", "minimum": 1, "description": "How many steps to move, default 1. Moves fewer when history runs out; steps_applied says how many." },
            "changes": changes_prop(),
        },
        "additionalProperties": false,
    });
    let edit_out = edit_output(&[], &[]);
    tools.extend([
        fixed(
            "composer_state_get",
            "Get Project State",
            "Returns the full project as JSON (tempo map, tracks, clips, notes) with its state_hash. Large for big projects, prefer composer_project_summary.",
            no_args.clone(),
            output_schema(
                json!({ "project": { "type": "object" }, "state_hash": { "type": "string" } }),
                &["project", "state_hash"],
            ),
            Hints::READ,
            Kind::StateGet,
        ),
        fixed(
            "composer_project_summary",
            "Project Summary",
            "Compact view: track, clip and note counts per track, time span in ticks and bars, tempo, time signature and state_hash.",
            no_args.clone(),
            output_schema(
                json!({
                    "name": { "type": "string" },
                    "state_hash": { "type": "string" },
                    "track_count": { "type": "integer", "minimum": 0 },
                    "region_count": { "type": "integer", "minimum": 0 },
                    "note_count": { "type": "integer", "minimum": 0 },
                    "span_ticks": { "type": "integer", "minimum": 0 },
                    "span_bars": { "type": "number" },
                    "tempo_bpm": { "type": "number" },
                    "tempo_changes": { "type": "integer", "minimum": 0 },
                    "time_signature": { "type": "string" },
                    "time_signature_changes": { "type": "integer", "minimum": 0 },
                    "tracks": { "type": "array", "items": { "type": "object" } },
                }),
                &[
                    "name",
                    "state_hash",
                    "track_count",
                    "region_count",
                    "note_count",
                    "span_ticks",
                    "span_bars",
                    "tempo_bpm",
                    "time_signature",
                    "tracks",
                ],
            ),
            Hints::READ,
            Kind::Summary,
        ),
        fixed(
            "composer_state_hash",
            "State Hash",
            "Returns the 16 hex digit hash of the whole project. Equal hashes mean equal projects, use it to verify an edit or an undo.",
            no_args.clone(),
            output_schema(
                json!({ "state_hash": { "type": "string" } }),
                &["state_hash"],
            ),
            Hints::READ,
            Kind::StateHash,
        ),
        fixed(
            "composer_undo",
            "Undo",
            "Undoes the last edit step, or steps of them (a group counts as one step). Returns applied, steps_applied, state_hash and depths.",
            steps_args.clone(),
            step_out.clone(),
            Hints::STEP,
            Kind::Undo,
        ),
        fixed(
            "composer_redo",
            "Redo",
            "Redoes the last undone step, or steps of them. Returns applied, steps_applied, state_hash and depths.",
            steps_args,
            step_out,
            Hints::STEP,
            Kind::Redo,
        ),
        fixed(
            "composer_group",
            "Apply Command Group",
            &format!(
                "Runs a list of commands as one atomic undo step: if any fails nothing changes and the error carries its index. Each item is a command object with a \"command\" name and that command's fields. {UNITS}"
            ),
            json!({
                "type": "object",
                "properties": {
                    "label": { "type": "string", "maxLength": 256, "description": "Name of the undo step." },
                    "source": source_prop(),
                    "changes": changes_prop(),
                    "commands": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": cfg.max_group_commands,
                        "items": { "oneOf": variants.clone() },
                    },
                },
                "required": ["label", "commands"],
                "additionalProperties": false,
            }),
            edit_out.clone(),
            Hints::EDIT,
            Kind::Group,
        ),
        fixed(
            "composer_session_new",
            "New Session",
            "Replaces the whole session with a fresh empty project and clears undo history. Everything unsaved is lost.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "maxLength": 256, "description": "Project name." },
                    "id": { "type": ["string", "null"], "pattern": ID_PATTERN, "description": "Optional project id, random when omitted." },
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            edit_out,
            Hints::RESET,
            Kind::SessionNew,
        ),
        fixed(
            "composer_dry_run",
            "Dry Run Commands",
            "Reports what commands would change without changing anything: would_change, the primitive changes, state_hash and state_hash_after. Items are command objects as in composer_group.",
            json!({
                "type": "object",
                "properties": {
                    "commands": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": cfg.max_group_commands,
                        "items": { "oneOf": variants.clone() },
                    },
                },
                "required": ["commands"],
                "additionalProperties": false,
            }),
            output_schema(
                json!({
                    "dry_run": { "type": "boolean" },
                    "would_change": { "type": "boolean" },
                    "changes": { "type": "array", "items": { "type": "object" } },
                    "state_hash": { "type": "string" },
                    "state_hash_after": { "type": "string" },
                }),
                &["dry_run", "would_change", "changes", "state_hash", "state_hash_after"],
            ),
            Hints::READ,
            Kind::DryRun,
        ),
        fixed(
            "composer_new_id",
            "New Ids",
            "Returns fresh random ids (32 hex digits) for new tracks, clips and notes. Never invent ids by hand.",
            json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer", "minimum": 1, "maximum": cfg.max_new_ids, "description": "How many ids, default 1." },
                },
                "additionalProperties": false,
            }),
            output_schema(
                json!({ "ids": { "type": "array", "items": { "type": "string", "pattern": ID_PATTERN } } }),
                &["ids"],
            ),
            Hints::READ,
            Kind::NewId,
        ),
    ]);
    tools.extend(rpc_tools());
    tools.extend(proposal_tools(&variants, cfg));
    tools
}

/// D27: the AI side of the proposals channel. There is no resolve tool on
/// purpose: accepting or rejecting is the person's act (UI or CLI), so an
/// agent cannot approve its own work.
fn proposal_tools(variants: &[Value], cfg: &Config) -> Vec<Tool> {
    let propose_schema = json!({
        "type": "object",
        "properties": {
            "label": { "type": "string", "maxLength": 256, "description": "Name shown to the person, and the undo step name once accepted." },
            "note": { "type": "string", "maxLength": 512, "description": "One or two plain sentences on what this changes and why." },
            "author": { "type": "string", "maxLength": 64, "description": "Who is proposing, shown next to the card." },
            "commands": {
                "type": "array",
                "minItems": 1,
                "maxItems": cfg.max_group_commands,
                "items": { "oneOf": variants },
            },
            "dry_run": dry_prop(),
        },
        "required": ["label", "commands"],
        "additionalProperties": false,
    });
    let status_schema = obj_schema(
        json!({
            "status": { "type": "string", "enum": ["pending", "accepted", "rejected", "all"], "description": "Default pending." },
        }),
        &[],
    );
    vec![
        Tool {
            name: "composer_propose".into(),
            title: "Propose Edits".into(),
            description: "Posts a labelled command list for the person to review. The project does not change until they accept it, which applies the list as one undo step tagged ai. A list that would fail now is refused with the failing index. Returns id, change_count and the hash the project would have after. Prefer this over composer_group when the person has not asked for the edit directly.".into(),
            input_schema: propose_schema,
            output_schema: json!({ "type": "object" }),
            // Each call queues one more card, so it is not idempotent.
            hints: Hints::STEP,
            kind: Kind::Rpc("proposal.post"),
        },
        Tool {
            name: "composer_proposals".into(),
            title: "List Proposals".into(),
            description: "Lists proposals with status (pending, accepted, rejected), their commands, whether a pending one still applies and whether the project moved on since it was posted (stale).".into(),
            input_schema: status_schema,
            output_schema: json!({ "type": "object" }),
            hints: Hints::READ,
            kind: Kind::Rpc("proposal.list"),
        },
    ]
}

fn seed_prop() -> Value {
    json!({ "type": "integer", "minimum": 0, "description": "Mood seed, default 0. The same seed gives the same music." })
}

fn recipe_schema(with_dry_run: bool) -> Value {
    let mut props = json!({
        "recipe": { "type": "string", "enum": RECIPE_NAMES },
        "params": { "type": "object", "description": "Recipe parameters; composer_recipes_list gives the names, ranges and defaults. Unknown names are refused." },
        "scope": obj_schema(
            json!({
                "track": { "type": "string", "pattern": ID_PATTERN },
                "region": { "type": "string", "pattern": ID_PATTERN, "description": "The clip to work on. Wins over track." },
            }),
            &[],
        ),
        "seed": seed_prop(),
    });
    if with_dry_run {
        props["dry_run"] = dry_prop();
    }
    obj_schema(props, &["recipe"])
}

fn obj_schema(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

fn path_prop(ext: &str) -> Value {
    json!({ "type": "string", "maxLength": 255, "description": format!("Relative file name inside the daemon's base directory (COMPOSERD_FILES_DIR), ending in {ext}. No absolute paths or .. parts.") })
}

fn source_prop() -> Value {
    json!({ "type": "string", "enum": ["user", "ai", "template", "recipe"], "description": "Who made the edit, shown in the history list. Default user." })
}

fn token_schema() -> Value {
    obj_schema(
        json!({ "token": { "type": "string", "pattern": ID_PATTERN, "description": "Token returned by composer_group_begin." } }),
        &["token"],
    )
}

fn id_prop() -> Value {
    json!({ "type": "string", "pattern": ID_PATTERN, "description": "Id: 32 lowercase hex digits. New ids come from composer_new_id." })
}

fn dry_prop() -> Value {
    json!({ "type": "boolean", "description": "Report what would happen without doing it." })
}

/// Tools that map one to one onto a daemon method (D16, D17). No logic here.
fn rpc_tools() -> Vec<Tool> {
    let ids = json!({ "type": "array", "items": { "type": "string", "pattern": ID_PATTERN } });
    let note_refs = json!({
        "type": "array",
        "items": obj_schema(
            json!({
                "region": { "type": "string", "pattern": ID_PATTERN },
                "note": { "type": "string", "pattern": ID_PATTERN },
            }),
            &["region", "note"],
        ),
    });
    let any =
        |name: &str, title: &str, desc: &str, method: &'static str, schema: Value, hints: Hints| {
            Tool {
                name: name.into(),
                title: title.into(),
                description: desc.into(),
                input_schema: schema,
                output_schema: json!({ "type": "object" }),
                hints,
                kind: Kind::Rpc(method),
            }
        };
    let none = || obj_schema(json!({}), &[]);
    let root_prop = || json!({ "type": "integer", "minimum": 0, "maximum": 11, "description": "Pitch class of the key root, 0 = C." });
    let scale_prop = || json!({ "type": "string", "enum": SCALE_NAMES });
    let file_hints = Hints::EDIT;
    vec![
        any(
            "composer_history",
            "Edit History",
            "Lists undo steps oldest first with label, source (user, ai, template, recipe) and command_count, plus cursor (applied steps) and total. Entries before the cursor are applied, the rest can be redone. Page with limit and offset.",
            "history.get",
            obj_schema(
                json!({
                    "limit": { "type": "integer", "minimum": 1, "description": "Most entries to return, capped by the daemon." },
                    "offset": { "type": "integer", "minimum": 0, "description": "First step to return." },
                }),
                &[],
            ),
            Hints::READ,
        ),
        any(
            "composer_changes_since",
            "Changes Since",
            "Returns the primitive changes (new and deleted tracks, clips and notes, field sets) that took the project from the state with the given hash to now: {base_hash, hash, list}. Returns full true instead of a list when that state is too old, a project was opened in between or the list is too large; then call composer_state_get. Cheaper than composer_state_get after a few edits.",
            "changes.since",
            obj_schema(
                json!({ "hash": { "type": "string", "pattern": "^[0-9a-f]{16}$", "description": "A state_hash you saw earlier." } }),
                &["hash"],
            ),
            Hints::READ,
        ),
        any(
            "composer_group_begin",
            "Begin Trial Group",
            "Opens a group: commands you apply next take effect right away and undo as one step when you commit. Returns a token. Roll back to discard them. One group at a time; the daemon rolls it back after a timeout or when you disconnect.",
            "group.begin",
            obj_schema(
                json!({
                    "label": { "type": "string", "maxLength": 256, "description": "Name of the undo step." },
                    "source": source_prop(),
                }),
                &["label"],
            ),
            Hints::STEP,
        ),
        any(
            "composer_group_commit",
            "Commit Trial Group",
            "Closes the open group as one undo step (nothing is added when it changed nothing).",
            "group.commit",
            token_schema(),
            Hints::STEP,
        ),
        any(
            "composer_group_rollback",
            "Roll Back Trial Group",
            "Closes the open group and reverts everything it did. The state_hash returns to its value before group begin.",
            "group.rollback",
            token_schema(),
            Hints::STEP,
        ),
        any(
            "composer_commands_enabled",
            "Which Commands Can Run",
            "For every described command says whether it can run now, with a plain reason when it cannot. Optional context narrows to a track or clip id.",
            "commands.enabled",
            obj_schema(
                json!({
                    "context": obj_schema(
                        json!({
                            "track": { "type": "string", "pattern": ID_PATTERN },
                            "region": { "type": "string", "pattern": ID_PATTERN },
                        }),
                        &[],
                    ),
                }),
                &[],
            ),
            Hints::READ,
        ),
        any(
            "composer_project_tree",
            "Project Tree",
            "Compact read of the project: tracks with mixer state, clips with counts and pitch range, plus the current selection. Notes only when notes is true.",
            "project.tree",
            obj_schema(
                json!({
                    "notes": { "type": "boolean", "description": "List every note of the shown clips." },
                    "track": { "type": ["string", "null"], "pattern": ID_PATTERN, "description": "Show only this track." },
                }),
                &[],
            ),
            Hints::READ,
        ),
        any(
            "composer_selection_get",
            "Get Selection",
            "Returns the selected tracks, clips and notes (stale ids are dropped).",
            "selection.get",
            none(),
            Hints::READ,
        ),
        any(
            "composer_selection_set",
            "Set Selection",
            "Replaces the selection. Ids are checked against the project. Not undoable, not part of the state hash.",
            "selection.set",
            obj_schema(
                json!({ "tracks": ids.clone(), "regions": ids, "notes": note_refs, "dry_run": dry_prop() }),
                &[],
            ),
            Hints::ADD,
        ),
        any(
            "composer_transport_get",
            "Get Transport",
            "Returns the playhead in ticks and seconds, playing (always false, the daemon has no audio device), the end tick, and the session settings loop {on, start, end}, metronome {on}, count_in_bars and follow.",
            "transport.get",
            none(),
            Hints::READ,
        ),
        any(
            "composer_transport_set",
            "Set Transport",
            "Moves the playhead and or changes the loop range, metronome, count in bars (the daemon caps it) and follow. Every field is optional and merges onto the current value, at least one is required. Saved with the project file, not undoable, never part of the state hash.",
            "transport.set",
            obj_schema(
                json!({
                    "tick": { "type": "integer", "minimum": 0, "maximum": 1099511627776i64 },
                    "loop": obj_schema(
                        json!({
                            "on": { "type": "boolean" },
                            "start": { "type": "integer", "minimum": 0, "maximum": 1099511627776i64 },
                            "end": { "type": "integer", "minimum": 0, "maximum": 1099511627776i64 },
                        }),
                        &[],
                    ),
                    "metronome": obj_schema(json!({ "on": { "type": "boolean" } }), &[]),
                    "count_in_bars": { "type": "integer", "minimum": 0 },
                    "follow": { "type": "boolean" },
                    "dry_run": dry_prop(),
                }),
                &[],
            ),
            Hints::ADD,
        ),
        any(
            "composer_meters_measure",
            "Measure Meters",
            "Renders offline and returns peak, rms and peak_db for the master and each track. Not a live meter.",
            "meters.measure",
            obj_schema(
                json!({
                    "sample_rate": { "type": "integer", "minimum": 8000, "maximum": 192000 },
                    "tail_seconds": { "type": "number", "minimum": 0 },
                }),
                &[],
            ),
            Hints::READ,
        ),
        any(
            "composer_sounds_list",
            "List Sounds",
            "Lists the sound source catalogue with licences and the known gaps. Optional bundle filter: yes, maybe or no.",
            "sounds.list",
            obj_schema(json!({ "bundle": { "type": "string", "maxLength": 16 } }), &[]),
            Hints::READ,
        ),
        any(
            "composer_theory_scale",
            "Theory Scale",
            "Returns the pitch classes (0 = C) of a scale from a root, in degree order, plus a 12 bit mask. Pure lookup, reads no project state.",
            "theory.scale",
            obj_schema(
                json!({ "root": root_prop(), "scale": scale_prop() }),
                &["root", "scale"],
            ),
            Hints::READ,
        ),
        any(
            "composer_theory_chords",
            "Theory Chords",
            "Returns the diatonic triads and sevenths of a key with plain names, roman numerals, pitch classes and root position MIDI pitches. Pass key {root, scale} or flat root and scale. Pure lookup, reads no project state.",
            "theory.chords",
            obj_schema(
                json!({
                    "key": obj_schema(json!({ "root": root_prop(), "scale": scale_prop() }), &["root", "scale"]),
                    "root": root_prop(),
                    "scale": scale_prop(),
                    "octave": { "type": "integer", "minimum": -1, "maximum": 8, "description": "Octave of the voicings, default 4." },
                    "sevenths": { "type": "boolean", "description": "Accepted for symmetry; the result always holds both triads and sevenths." },
                }),
                &[],
            ),
            Hints::READ,
        ),
        any(
            "composer_theory_pitch",
            "Theory Pitch",
            "Says whether a MIDI pitch is in a key, its 1 based scale degree (null when outside) and the nearest in key pitch. Pass key {root, scale} or flat root and scale. Pure lookup, reads no project state.",
            "theory.pitch",
            obj_schema(
                json!({
                    "key": obj_schema(json!({ "root": root_prop(), "scale": scale_prop() }), &["root", "scale"]),
                    "root": root_prop(),
                    "scale": scale_prop(),
                    "pitch": { "type": "integer", "minimum": 0, "maximum": 127 },
                }),
                &["pitch"],
            ),
            Hints::READ,
        ),
        any(
            "composer_templates_list",
            "List Templates",
            "Lists the bundled starter templates with id, name, description, tempo, key, command count and the loop range to play first. Reads no project state.",
            "templates.list",
            none(),
            Hints::READ,
        ),
        any(
            "composer_template_apply",
            "Apply Template",
            "Adds a starter template (tracks, clips, mix, tempo, key and name) on top of the current project as one undo step with source template. Ids come from the seed, so the same seed on the same project gives the same result. Use dry_run to see the commands first. Returns the ids it created by placeholder name.",
            "template.apply",
            obj_schema(
                json!({
                    "template": { "type": "string", "enum": TEMPLATE_IDS },
                    "seed": seed_prop(),
                    "dry_run": dry_prop(),
                }),
                &["template"],
            ),
            Hints::APPLY,
        ),
        any(
            "composer_recipes_list",
            "List Recipes",
            "Lists the recipes (extend_to_length, repeat_with_variation, bass_from_chords, pad_under_melody, soften_velocity, calm_sections) with which scope each reads and every parameter with its range, default and choices. Reads no project state.",
            "recipes.list",
            none(),
            Hints::READ,
        ),
        any(
            "composer_recipe_plan",
            "Plan Recipe",
            "Returns the commands a recipe would run for these params, scope and seed, without changing the project. The list is checked against the current project and reports change_count and the state hash it would reach. Send the commands to composer_propose to let the person review them, or use composer_recipe_apply.",
            "recipe.plan",
            recipe_schema(false),
            Hints::READ,
        ),
        any(
            "composer_recipe_apply",
            "Apply Recipe",
            "Runs a recipe and applies its commands as one undo step with source recipe. Deterministic for the same project, params, scope and seed. Prefer composer_recipe_plan then composer_propose when the person has not asked for the edit directly.",
            "recipe.apply",
            recipe_schema(true),
            Hints::APPLY,
        ),
        any(
            "composer_project_save",
            "Save Project",
            "Saves the project to a file in the base directory. Refuses to replace a file unless overwrite is true.",
            "project.save",
            obj_schema(
                json!({ "path": path_prop(".cmpf"), "overwrite": { "type": "boolean" }, "dry_run": dry_prop() }),
                &["path"],
            ),
            file_hints,
        ),
        any(
            "composer_project_load",
            "Load Project",
            "Replaces the session with a project file and clears undo history. Everything unsaved is lost.",
            "project.load",
            obj_schema(json!({ "path": path_prop(".cmpf"), "dry_run": dry_prop() }), &["path"]),
            Hints::EDIT,
        ),
        any(
            "composer_export_midi",
            "Export MIDI",
            "Writes a Standard MIDI File (format 1, 960 PPQ) of the instrument tracks.",
            "export.midi",
            obj_schema(
                json!({ "path": path_prop(".mid"), "overwrite": { "type": "boolean" }, "dry_run": dry_prop() }),
                &["path"],
            ),
            file_hints,
        ),
        any(
            "composer_import_midi",
            "Import MIDI",
            "Adds the tracks and notes of a MIDI file as one undo step.",
            "import.midi",
            obj_schema(
                json!({ "path": path_prop(".mid"), "seed": { "type": "integer", "minimum": 0 }, "dry_run": dry_prop() }),
                &["path"],
            ),
            Hints::STEP,
        ),
        any(
            "composer_audio_import",
            "Import Audio",
            "Imports a WAV file from the base directory as an asset (stored by content hash, PCM 8, 16, 24, 32 bit or float, up to the configured size). With track (an audio track) it also places a clip as one undo step; length defaults to the file duration. Importing the same bytes again reuses the asset.",
            "audio.import",
            obj_schema(
                json!({
                    "path": path_prop(".wav"),
                    "track": id_prop(),
                    "start": { "type": "integer", "minimum": 0, "description": "Clip start in ticks, default 0." },
                    "length": { "type": "integer", "minimum": 1, "description": "Clip length in ticks, default the file duration. Needs track." },
                    "id": id_prop(),
                    "dry_run": dry_prop(),
                }),
                &["path"],
            ),
            Hints::STEP,
        ),
        any(
            "composer_audio_assets",
            "List Audio Assets",
            "Lists the imported audio assets of the session with hash, name, rate, channels, frames, whether the file is still in the store, and the clips that use each.",
            "audio.assets",
            none(),
            Hints::READ,
        ),
        any(
            "composer_audio_peaks",
            "Audio Peaks",
            "Min and max per bin (floats in -1..1) of an imported asset over a frame range, for drawing a waveform. asset is the content hash or the asset id. Builds a peak cache next to the file on first use.",
            "audio.peaks",
            obj_schema(
                json!({
                    "asset": { "type": "string", "minLength": 32, "maxLength": 64, "description": "Content hash (64 hex digits) or asset id (32 hex digits)." },
                    "from": { "type": "integer", "minimum": 0, "description": "First frame, default 0." },
                    "to": { "type": "integer", "minimum": 1, "description": "End frame (exclusive), default the end of the file." },
                    "bins": { "type": "integer", "minimum": 1, "description": "Number of bins, default 512, capped by the daemon." },
                }),
                &["asset"],
            ),
            Hints::READ,
        ),
        any(
            "composer_render_wav",
            "Render WAV",
            "Renders the project offline to a WAV file (stereo). Honors volume, pan, mute and solo.",
            "render.wav",
            obj_schema(
                json!({
                    "path": path_prop(".wav"),
                    "sample_rate": { "type": "integer", "minimum": 8000, "maximum": 192000 },
                    "tail_seconds": { "type": "number", "minimum": 0 },
                    "format": { "type": "string", "enum": ["pcm16", "pcm24", "float32"] },
                    "overwrite": { "type": "boolean" },
                    "dry_run": dry_prop(),
                }),
                &["path"],
            ),
            file_hints,
        ),
    ]
}

/// The optional `changes` flag (D25): the reply then carries the primitive change list.
fn changes_prop() -> Value {
    json!({ "type": "boolean", "description": "When true the reply also carries changes: the primitive change list {base_hash, hash, list}, or full when the edit was too large to list. Default false." })
}

fn rpc(b: &mut dyn Backend, method: &str, params: Value) -> Result<Value, ToolError> {
    b.call(method, params).map_err(from_call)
}

fn steps_params(args: &Value) -> Value {
    let mut p = match args.get("steps").filter(|v| !v.is_null()) {
        Some(s) => json!({ "steps": s }),
        None => json!({}),
    };
    if args["changes"] == json!(true) {
        p["changes"] = json!(true);
    }
    p
}

fn random_id() -> Result<String, ToolError> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b)
        .map_err(|_| ToolError::new("internal", "operating system randomness is unavailable"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Runs one tool. `args` is the MCP `arguments` object (null counts as empty).
pub fn call_tool(tool: &Tool, args: Value, b: &mut dyn Backend) -> Result<Value, ToolError> {
    let args = if args.is_null() { json!({}) } else { args };
    validate(&tool.input_schema, &args).map_err(|m| ToolError::new("invalid_arguments", m))?;
    match &tool.kind {
        Kind::Command(name) => {
            let mut cmd: Map<String, Value> = args.as_object().cloned().unwrap_or_default();
            cmd.insert("command".into(), json!(name));
            rpc(b, "command.apply", json!({ "command": cmd }))
        }
        Kind::StateGet => rpc(b, "state.get", json!({})),
        Kind::StateHash => rpc(b, "state.hash", json!({})),
        Kind::Summary => {
            let state = rpc(b, "state.get", json!({}))?;
            summarize(&state).map_err(|m| ToolError::new("bad_reply", m))
        }
        Kind::Undo => rpc(b, "undo", steps_params(&args)),
        Kind::Redo => rpc(b, "redo", steps_params(&args)),
        Kind::SessionNew => {
            let mut p = Map::new();
            p.insert("name".into(), args["name"].clone());
            if let Some(id) = args.get("id").filter(|v| !v.is_null()) {
                p.insert("id".into(), id.clone());
            }
            rpc(b, "session.new", Value::Object(p))
        }
        Kind::Group => rpc(b, "command.group", {
            let mut p = json!({ "label": args["label"], "commands": args["commands"] });
            if let Some(s) = args.get("source").filter(|v| !v.is_null()) {
                p["source"] = s.clone();
            }
            if args["changes"] == json!(true) {
                p["changes"] = json!(true);
            }
            p
        }),
        Kind::DryRun => {
            let cmds = args["commands"].as_array().cloned().unwrap_or_default();
            if cmds.len() == 1 {
                rpc(
                    b,
                    "command.apply",
                    json!({ "command": cmds[0], "dry_run": true }),
                )
            } else {
                rpc(
                    b,
                    "command.group",
                    json!({ "label": "dry run", "commands": cmds, "dry_run": true }),
                )
            }
        }
        Kind::Rpc(method) => rpc(b, method, args),
        Kind::NewId => {
            let n = args["count"].as_u64().unwrap_or(1);
            let ids: Result<Vec<String>, ToolError> = (0..n).map(|_| random_id()).collect();
            Ok(json!({ "ids": ids? }))
        }
    }
}

/// Finds a tool by its public name.
pub fn find<'a>(tools: &'a [Tool], name: &str) -> Option<&'a Tool> {
    tools.iter().find(|t| t.name == name)
}
