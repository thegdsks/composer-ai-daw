//! D27 proposals channel for the browser editor. Same shapes as composerd
//! `proposal.post`, `proposal.list` and `proposal.resolve`. Accepting applies the
//! commands as one group tagged `ai`.

use crate::editor_session::{bad_params, hex, rejected};
use composer_commands::{Command, CommandError, Editor, Source};
use composer_model::Project;
use serde::Deserialize;
use serde_json::{json, Value};

/// Pending proposals one session holds (no env in a page, D27).
pub const DEFAULT_MAX_PROPOSALS: usize = 32;
/// Most commands in one proposal.
pub const DEFAULT_MAX_PROPOSAL_COMMANDS: usize = 2000;
const MAX_TEXT: usize = 512;
const COMMAND_REJECTED: i64 = -32000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Accepted,
    Rejected,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Accepted => "accepted",
            Status::Rejected => "rejected",
        }
    }
}

struct Proposal {
    id: String,
    label: String,
    note: Option<String>,
    author: Option<String>,
    commands: Vec<Command>,
    base_hash: u64,
    status: Status,
}

/// Session state outside the document: not undoable, not in the state hash.
#[derive(Default)]
pub struct Proposals {
    items: Vec<Proposal>,
    next: u64,
}

impl Proposals {
    fn pending(&self) -> usize {
        self.items
            .iter()
            .filter(|x| x.status == Status::Pending)
            .count()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostParams {
    label: String,
    commands: Vec<Command>,
    note: Option<String>,
    author: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ListParams {
    status: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveParams {
    id: String,
    action: String,
    #[serde(default)]
    dry_run: bool,
}

pub(crate) fn app_error(code: &str, message: &str) -> Value {
    json!({ "error": { "code": COMMAND_REJECTED, "message": message, "data": { "code": code } } })
}

/// Plans `cmds` in order on a copy; returns the copy and the primitive changes.
fn plan_on_copy(p: &Project, cmds: &[Command]) -> Result<(Project, usize), Value> {
    let mut copy = p.clone();
    let mut count = 0;
    for (i, c) in cmds.iter().enumerate() {
        let at = (cmds.len() > 1).then_some(i);
        let changes = c.plan(&copy).map_err(|e| rejected(&e, at))?;
        for ch in &changes {
            copy.apply(ch)
                .map_err(|e| rejected(&CommandError::from(e), at))?;
        }
        count += changes.len();
    }
    Ok((copy, count))
}

fn too_long(field: &str, v: &str) -> Option<Value> {
    (v.chars().count() > MAX_TEXT)
        .then(|| bad_params(format!("{field} is longer than {MAX_TEXT} characters")))
}

pub fn post(editor: &Editor, store: &mut Proposals, json: &[u8]) -> Value {
    let p: PostParams = match serde_json::from_slice(json) {
        Ok(p) => p,
        Err(e) => return bad_params(e.to_string()),
    };
    if p.label.trim().is_empty() {
        return bad_params("label must not be empty".into());
    }
    let long = too_long("label", &p.label)
        .or_else(|| p.note.as_deref().and_then(|n| too_long("note", n)))
        .or_else(|| p.author.as_deref().and_then(|n| too_long("author", n)));
    if let Some(e) = long {
        return e;
    }
    if p.commands.is_empty() {
        return bad_params("commands must not be empty".into());
    }
    if p.commands.len() > DEFAULT_MAX_PROPOSAL_COMMANDS {
        return app_error("proposal_too_large", "the proposal has too many commands");
    }
    let (after, n) = match plan_on_copy(editor.project(), &p.commands) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let before = editor.state_hash();
    let mut out = json!({
        "dry_run": p.dry_run,
        "would_change": n > 0,
        "change_count": n,
        "state_hash": hex(before),
        "state_hash_after": hex(after.state_hash()),
    });
    if p.dry_run {
        return json!({ "result": out });
    }
    if store.pending() >= DEFAULT_MAX_PROPOSALS {
        return app_error(
            "proposals_full",
            "too many pending proposals, resolve some first",
        );
    }
    store.next += 1;
    let id = format!("p{}", store.next);
    store.items.push(Proposal {
        id: id.clone(),
        label: p.label,
        note: p.note,
        author: p.author,
        commands: p.commands,
        base_hash: before,
        status: Status::Pending,
    });
    while store.items.len() > DEFAULT_MAX_PROPOSALS * 2 {
        let Some(i) = store.items.iter().position(|x| x.status != Status::Pending) else {
            break;
        };
        store.items.remove(i);
    }
    out["id"] = json!(id);
    out["status"] = json!("pending");
    json!({ "result": out })
}

pub fn list(editor: &Editor, store: &Proposals, json: &[u8]) -> Value {
    let p: ListParams = if json.is_empty() {
        ListParams::default()
    } else {
        match serde_json::from_slice(json) {
            Ok(p) => p,
            Err(e) => return bad_params(e.to_string()),
        }
    };
    let want = p.status.as_deref().unwrap_or("pending");
    if !matches!(want, "pending" | "accepted" | "rejected" | "all") {
        return bad_params("status must be pending, accepted, rejected or all".into());
    }
    let current = editor.state_hash();
    let entries: Vec<Value> = store
        .items
        .iter()
        .filter(|x| want == "all" || x.status.as_str() == want)
        .map(|x| {
            let mut v = json!({
                "id": x.id,
                "label": x.label,
                "note": x.note,
                "author": x.author,
                "status": x.status.as_str(),
                "command_count": x.commands.len(),
                "commands": x.commands,
                "base_hash": hex(x.base_hash),
                "stale": x.base_hash != current,
            });
            if x.status == Status::Pending {
                let planned = plan_on_copy(editor.project(), &x.commands);
                v["applies"] = json!(planned.is_ok());
                if let Ok((_, n)) = planned {
                    v["change_count"] = json!(n);
                }
            }
            v
        })
        .collect();
    json!({ "result": {
        "proposals": entries,
        "pending": store.pending(),
        "state_hash": hex(current),
    } })
}

/// On success `Ok` carries `{id, status}` and the caller adds its summary.
pub fn resolve(editor: &mut Editor, store: &mut Proposals, json: &[u8]) -> Result<Value, Value> {
    let p: ResolveParams = serde_json::from_slice(json).map_err(|e| bad_params(e.to_string()))?;
    if !matches!(p.action.as_str(), "accept" | "reject") {
        return Err(bad_params("action must be accept or reject".into()));
    }
    let Some(i) = store.items.iter().position(|x| x.id == p.id) else {
        return Err(app_error("proposal_not_found", "no proposal has that id"));
    };
    if store.items[i].status != Status::Pending {
        return Err(app_error(
            "proposal_resolved",
            "the proposal was already resolved",
        ));
    }
    let accept = p.action == "accept";
    if accept {
        let cmds = store.items[i].commands.clone();
        if p.dry_run {
            let (after, n) = plan_on_copy(editor.project(), &cmds)?;
            return Ok(json!({
                "dry_run": true,
                "id": p.id,
                "would_change": n > 0,
                "change_count": n,
                "state_hash": hex(editor.state_hash()),
                "state_hash_after": hex(after.state_hash()),
            }));
        }
        let label = store.items[i].label.clone();
        let mut failed = None;
        let run = editor.group_as(&label, Source::Ai, |ed| {
            for (k, c) in cmds.iter().enumerate() {
                if let Err(e) = ed.execute(c) {
                    failed = Some(k);
                    return Err(e);
                }
            }
            Ok(())
        });
        run.map_err(|e| rejected(&e, failed))?;
    } else if p.dry_run {
        return Ok(json!({ "dry_run": true, "valid": true, "id": p.id }));
    }
    let status = if accept {
        Status::Accepted
    } else {
        Status::Rejected
    };
    store.items[i].status = status;
    Ok(json!({ "id": p.id, "status": status.as_str() }))
}
