//! D27: the proposals channel. An AI client posts a labelled command list; it
//! changes nothing until a person accepts it, which applies it as one group
//! tagged `ai`. No music logic lives here: commands are planned and applied by
//! composer-commands.

use crate::hub::Inner;
use crate::rpc::{dry_report, hex, parse_params, plan_on_copy, summary, with, RpcError};
use composer_commands::{Command, Source};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_MAX_PROPOSALS: usize = 32;
const DEFAULT_MAX_COMMANDS: usize = 2000;
const MAX_TEXT: usize = 512;

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
pub(crate) struct Proposals {
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

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn max_proposals() -> usize {
    env_usize("COMPOSERD_MAX_PROPOSALS", DEFAULT_MAX_PROPOSALS)
}

fn max_commands() -> usize {
    env_usize("COMPOSERD_MAX_PROPOSAL_COMMANDS", DEFAULT_MAX_COMMANDS)
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

fn text(field: &'static str, v: &str) -> Result<(), RpcError> {
    if v.chars().count() > MAX_TEXT {
        return Err(RpcError::invalid_params(format!(
            "{field} is longer than {MAX_TEXT} characters"
        )));
    }
    Ok(())
}

pub(crate) fn post(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: PostParams = parse_params(params)?;
    if p.label.trim().is_empty() {
        return Err(RpcError::invalid_params("label must not be empty"));
    }
    text("label", &p.label)?;
    if let Some(n) = &p.note {
        text("note", n)?;
    }
    if let Some(a) = &p.author {
        text("author", a)?;
    }
    if p.commands.is_empty() {
        return Err(RpcError::invalid_params("commands must not be empty"));
    }
    if p.commands.len() > max_commands() {
        return Err(RpcError::app(
            "proposal_too_large",
            "the proposal has more commands than COMPOSERD_MAX_PROPOSAL_COMMANDS allows",
        ));
    }
    // A proposal that cannot apply now is refused up front, with the failing index.
    let (after, changes) = plan_on_copy(g.editor.project(), &p.commands)?;
    let n = changes.len();
    let mut out = dry_report(g.editor.project(), &after, changes);
    out["change_count"] = json!(n);
    if p.dry_run {
        return Ok(out);
    }
    if g.proposals.pending() >= max_proposals() {
        return Err(RpcError::app(
            "proposals_full",
            "too many pending proposals, resolve some first",
        ));
    }
    g.proposals.next += 1;
    let id = format!("p{}", g.proposals.next);
    let base_hash = g.editor.project().state_hash();
    g.proposals.items.push(Proposal {
        id: id.clone(),
        label: p.label,
        note: p.note,
        author: p.author,
        commands: p.commands,
        base_hash,
        status: Status::Pending,
    });
    // Resolved entries are history only: drop the oldest beyond twice the cap.
    while g.proposals.items.len() > max_proposals() * 2 {
        let Some(i) = g
            .proposals
            .items
            .iter()
            .position(|x| x.status != Status::Pending)
        else {
            break;
        };
        g.proposals.items.remove(i);
    }
    notify(g);
    out["dry_run"] = json!(false);
    out["id"] = json!(id);
    out["status"] = json!("pending");
    Ok(out)
}

/// Tells subscribed clients the pending count moved, so a badge can refresh.
fn notify(g: &mut Inner) {
    let line = json!({
        "jsonrpc": "2.0",
        "method": "proposals.changed",
        "params": { "pending": g.proposals.pending() },
    })
    .to_string();
    g.broadcast(&line);
}

pub(crate) fn list(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: ListParams = parse_params(params)?;
    let want = p.status.as_deref().unwrap_or("pending");
    if !matches!(want, "pending" | "accepted" | "rejected" | "all") {
        return Err(RpcError::invalid_params(
            "status must be pending, accepted, rejected or all",
        ));
    }
    let current = g.editor.project().state_hash();
    let entries: Vec<Value> = g
        .proposals
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
                // Whether it still applies to the project as it is now.
                let planned = plan_on_copy(g.editor.project(), &x.commands);
                v["applies"] = json!(planned.is_ok());
                if let Ok((_, ch)) = planned {
                    v["change_count"] = json!(ch.len());
                }
            }
            v
        })
        .collect();
    Ok(json!({
        "proposals": entries,
        "pending": g.proposals.pending(),
        "state_hash": hex(current),
    }))
}

pub(crate) fn resolve(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: ResolveParams = parse_params(params)?;
    if !matches!(p.action.as_str(), "accept" | "reject") {
        return Err(RpcError::invalid_params("action must be accept or reject"));
    }
    let Some(i) = g.proposals.items.iter().position(|x| x.id == p.id) else {
        return Err(RpcError::app(
            "proposal_not_found",
            "no proposal has that id",
        ));
    };
    if g.proposals.items[i].status != Status::Pending {
        return Err(RpcError::app(
            "proposal_resolved",
            "the proposal was already resolved",
        ));
    }
    let accept = p.action == "accept";
    if accept {
        let cmds = g.proposals.items[i].commands.clone();
        if p.dry_run {
            let (after, changes) = plan_on_copy(g.editor.project(), &cmds)?;
            let mut r = dry_report(g.editor.project(), &after, changes);
            r["id"] = json!(p.id);
            return Ok(r);
        }
        let label = g.proposals.items[i].label.clone();
        let mut failed = None;
        let run = g.editor.group_as(&label, Source::Ai, |ed| {
            for (k, c) in cmds.iter().enumerate() {
                if let Err(e) = ed.execute(c) {
                    failed = Some(k);
                    return Err(e);
                }
            }
            Ok(())
        });
        // On failure the group rolled back and the proposal stays pending.
        run.map_err(|e| RpcError::command(&e, failed))?;
    } else if p.dry_run {
        return Ok(json!({ "dry_run": true, "valid": true, "id": p.id }));
    }
    let status = if accept {
        Status::Accepted
    } else {
        Status::Rejected
    };
    g.proposals.items[i].status = status;
    notify(g);
    Ok(with(
        summary(&g.editor),
        json!({ "id": p.id, "status": status.as_str() }),
    ))
}
