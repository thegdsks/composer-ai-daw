use crate::hub::{random_id, ClientHandle, Feed, Hub, Inner};
use composer_commands::{command_meta, describe, Command, CommandError, Editor, Source};
use composer_model::{Change, Id, Project};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// Application error: the command was understood but rejected.
const COMMAND_REJECTED: i64 = -32000;

/// A JSON-RPC error. `data.code` is the stable machine readable reason.
#[derive(Debug)]
pub(crate) struct RpcError {
    code: i64,
    message: String,
    data: Value,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>, reason: &str) -> Self {
        Self {
            code,
            message: message.into(),
            data: json!({ "code": reason }),
        }
    }
    /// An application error with its own stable reason code.
    pub(crate) fn app(reason: &str, message: impl Into<String>) -> Self {
        Self::new(COMMAND_REJECTED, message, reason)
    }
    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(PARSE_ERROR, message, "parse_error")
    }
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(INVALID_REQUEST, message, "invalid_request")
    }
    fn method_not_found(method: &str) -> Self {
        Self::new(
            METHOD_NOT_FOUND,
            format!("unknown method {method}"),
            "method_not_found",
        )
    }
    pub(crate) fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message, "invalid_params")
    }
    pub(crate) fn command(e: &CommandError, index: Option<usize>) -> Self {
        let mut data = json!({ "code": e.code() });
        if let Some(i) = index {
            data["index"] = json!(i);
        }
        Self {
            code: COMMAND_REJECTED,
            message: e.to_string(),
            data,
        }
    }
}

pub(crate) fn error_line(id: Value, e: &RpcError) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": e.code, "message": e.message, "data": e.data },
    })
    .to_string()
}

fn ok_line(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

pub(crate) fn state_changed_line(hash: u64, cause: &str, feed: Option<Value>) -> String {
    notification(hash, cause, feed)
}

/// D25: the wire shape of one feed. `list` is absent and `full` true when the client must
/// fetch the whole state.
pub(crate) fn feed_value(f: &Feed) -> Value {
    let mut v = json!({ "base_hash": hex(f.base_hash), "hash": hex(f.hash) });
    match &f.changes {
        Some(list) => v["list"] = json!(list),
        None => v["full"] = json!(true),
    }
    v
}

/// D25 `changes.since {hash}`: every feed after the state `hash`, joined, or `full` when the
/// log no longer reaches back that far or a session replacement lies in between.
pub(crate) fn changes_since(g: &Inner, hash: u64) -> Value {
    let head = g.editor.project().state_hash();
    let full = || json!({ "base_hash": hex(hash), "hash": hex(head), "full": true });
    if hash == head {
        return json!({ "base_hash": hex(hash), "hash": hex(head), "list": [] });
    }
    // The latest match wins: equal hashes mean equal states.
    let Some(start) = g.feeds.iter().rposition(|f| f.base_hash == hash) else {
        return full();
    };
    let mut list: Vec<&Change> = Vec::new();
    for f in g.feeds.iter().skip(start) {
        match &f.changes {
            Some(c) => list.extend(c.iter()),
            None => return full(),
        }
    }
    if list.len() > g.files.max_feed_changes {
        return full();
    }
    json!({ "base_hash": hex(hash), "hash": hex(head), "list": list })
}

/// `commands.describe` entries with the palette metadata of D33 merged in.
pub(crate) fn describe_value() -> Value {
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
    Value::Array(list)
}

/// Parses an optional source tag; absent means the user.
pub(crate) fn parse_source(s: &Option<String>) -> Result<Source, RpcError> {
    match s {
        None => Ok(Source::User),
        Some(x) => Source::parse(x)
            .ok_or_else(|| RpcError::invalid_params("source must be user, ai, template or recipe")),
    }
}

fn notification(hash: u64, cause: &str, feed: Option<Value>) -> String {
    let mut params = json!({ "hash": hex(hash), "cause": cause });
    if let Some(f) = feed {
        params["changes"] = f;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "state.changed",
        "params": params,
    })
    .to_string()
}

/// Hashes go over JSON as 16 hex digits so JavaScript clients never lose bits.
pub(crate) fn hex(h: u64) -> String {
    format!("{h:016x}")
}

fn fingerprint(e: &Editor) -> (u64, usize, usize) {
    (e.project().state_hash(), e.undo_depth(), e.redo_depth())
}

pub(crate) fn summary(e: &Editor) -> Map<String, Value> {
    let Value::Object(m) = json!({
        "state_hash": hex(e.project().state_hash()),
        "undo_depth": e.undo_depth(),
        "redo_depth": e.redo_depth(),
        "undo_label": e.undo_label(),
    }) else {
        unreachable!("object literal")
    };
    m
}

pub(crate) fn with(mut m: Map<String, Value>, extra: Value) -> Value {
    if let Value::Object(x) = extra {
        m.extend(x);
    }
    Value::Object(m)
}

pub(crate) fn parse_params<T: DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    let v = if v.is_null() { json!({}) } else { v };
    if !v.is_object() {
        return Err(RpcError::invalid_params("params must be an object"));
    }
    serde_json::from_value(v).map_err(|e| RpcError::invalid_params(e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewParams {
    name: String,
    id: Option<Id>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyParams {
    command: Command,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupParams {
    label: String,
    commands: Vec<Command>,
    source: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DryParams {
    /// How many steps to move (D20), default 1.
    steps: Option<usize>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubParams {
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

/// Plans `cmds` one after another on a copy of the project; nothing is mutated.
pub(crate) fn plan_on_copy(
    p: &Project,
    cmds: &[Command],
) -> Result<(Project, Vec<Value>), RpcError> {
    let mut copy = p.clone();
    let mut all = Vec::new();
    for (i, c) in cmds.iter().enumerate() {
        let at = (cmds.len() > 1).then_some(i);
        let changes = c.plan(&copy).map_err(|e| RpcError::command(&e, at))?;
        for ch in &changes {
            copy.apply(ch)
                .map_err(|e| RpcError::command(&CommandError::from(e), at))?;
        }
        all.extend(changes.iter().filter_map(|c| serde_json::to_value(c).ok()));
    }
    Ok((copy, all))
}

pub(crate) fn dry_report(current: &Project, after: &Project, changes: Vec<Value>) -> Value {
    json!({
        "dry_run": true,
        "would_change": !changes.is_empty(),
        "changes": changes,
        "state_hash": hex(current.state_hash()),
        "state_hash_after": hex(after.state_hash()),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SinceParams {
    #[serde(deserialize_with = "hash_from_hex")]
    hash: u64,
}

fn hash_from_hex<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let s = String::deserialize(d)?;
    if s.len() != 16 {
        return Err(serde::de::Error::custom("hash must be 16 hex digits"));
    }
    u64::from_str_radix(&s, 16).map_err(|_| serde::de::Error::custom("hash must be 16 hex digits"))
}

/// D25: `changes: true` on an edit asks for the feed in the reply. Removed before parsing
/// because the parameter structs reject unknown fields.
fn take_changes_flag(method: &str, params: &mut Value) -> bool {
    if !matches!(method, "command.apply" | "command.group" | "undo" | "redo") {
        return false;
    }
    params
        .as_object_mut()
        .and_then(|o| o.remove("changes"))
        .is_some_and(|v| v == json!(true))
}

fn dispatch(g: &mut Inner, client: u64, method: &str, params: Value) -> Result<Value, RpcError> {
    match method {
        "session.new" => {
            let p: NewParams = parse_params(params)?;
            let id = p.id.unwrap_or_else(random_id);
            // Reuse the command rules for names so both paths agree.
            let probe = Project::new(id, "");
            Command::SetProjectName {
                name: p.name.clone(),
            }
            .plan(&probe)
            .map_err(|e| RpcError::command(&e, None))?;
            let fresh = Project::new(id, p.name);
            if p.dry_run {
                return Ok(dry_report(
                    g.editor.project(),
                    &fresh,
                    vec![json!("replace_session")],
                ));
            }
            g.replace_project(fresh);
            Ok(Value::Object(summary(&g.editor)))
        }
        "command.apply" => {
            let p: ApplyParams = parse_params(params)?;
            crate::audio::check_assets(g, std::slice::from_ref(&p.command))?;
            if p.dry_run {
                let (after, changes) = plan_on_copy(g.editor.project(), &[p.command])?;
                return Ok(dry_report(g.editor.project(), &after, changes));
            }
            let before = g.editor.project().state_hash();
            g.editor
                .execute(&p.command)
                .map_err(|e| RpcError::command(&e, None))?;
            let changed = before != g.editor.project().state_hash();
            Ok(with(summary(&g.editor), json!({ "changed": changed })))
        }
        "command.group" => {
            let p: GroupParams = parse_params(params)?;
            if p.label.trim().is_empty() {
                return Err(RpcError::invalid_params("label must not be empty"));
            }
            if p.commands.is_empty() {
                return Err(RpcError::invalid_params("commands must not be empty"));
            }
            crate::audio::check_assets(g, &p.commands)?;
            if p.dry_run {
                let (after, changes) = plan_on_copy(g.editor.project(), &p.commands)?;
                return Ok(dry_report(g.editor.project(), &after, changes));
            }
            let source = parse_source(&p.source)?;
            let mut failed = None;
            let run = g.editor.group_as(&p.label, source, |ed| {
                for (i, c) in p.commands.iter().enumerate() {
                    if let Err(e) = ed.execute(c) {
                        failed = Some(i);
                        return Err(e);
                    }
                }
                Ok(())
            });
            run.map_err(|e| RpcError::command(&e, failed))?;
            Ok(Value::Object(summary(&g.editor)))
        }
        "undo" | "redo" => {
            let p: DryParams = parse_params(params)?;
            let is_undo = method == "undo";
            let steps = p.steps.unwrap_or(1);
            if steps == 0 {
                return Err(RpcError::invalid_params("steps must be at least 1"));
            }
            let depth = if is_undo {
                g.editor.undo_depth()
            } else {
                g.editor.redo_depth()
            };
            if p.dry_run {
                return Ok(with(
                    summary(&g.editor),
                    json!({ "dry_run": true, "would_apply": depth > 0, "steps_available": depth.min(steps) }),
                ));
            }
            let n = if is_undo {
                g.editor.undo_steps(steps)
            } else {
                g.editor.redo_steps(steps)
            }
            .map_err(|e| RpcError::command(&e, None))?;
            Ok(with(
                summary(&g.editor),
                json!({ "applied": n > 0, "steps_applied": n }),
            ))
        }
        "state.get" => Ok(json!({
            "state_hash": hex(g.editor.project().state_hash()),
            "project": g.editor.project(),
        })),
        "changes.since" => {
            let p: SinceParams = parse_params(params)?;
            Ok(changes_since(g, p.hash))
        }
        "state.hash" => Ok(json!({ "state_hash": hex(g.editor.project().state_hash()) })),
        "commands.describe" => Ok(json!({ "commands": describe_value() })),
        "subscribe" => {
            let p: SubParams = parse_params(params)?;
            g.set_subscribed(client, p.enabled);
            Ok(json!({
                "subscribed": p.enabled,
                "state_hash": hex(g.editor.project().state_hash()),
            }))
        }
        other => crate::methods::dispatch(g, client, other, params)
            .unwrap_or_else(|| Err(RpcError::method_not_found(other))),
    }
}

/// Handles one JSON-RPC message from `client`. The reply and any `state.changed`
/// push are queued while the session lock is held, so every client sees one order.
pub fn handle_line(hub: &Hub, client: &ClientHandle, line: &str) {
    let send = |s: String| {
        let _ = client.tx.send(s);
    };
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return send(error_line(Value::Null, &RpcError::parse(e.to_string()))),
    };
    let Some(obj) = v.as_object() else {
        let msg = if v.is_array() {
            "batch requests are not supported"
        } else {
            "request must be an object"
        };
        return send(error_line(Value::Null, &RpcError::invalid_request(msg)));
    };
    let id = obj.get("id").cloned();
    if matches!(
        &id,
        Some(Value::Array(_) | Value::Object(_) | Value::Bool(_))
    ) {
        return send(error_line(
            Value::Null,
            &RpcError::invalid_request("bad id"),
        ));
    }
    let reply_id = id.clone().unwrap_or(Value::Null);
    if obj.get("jsonrpc") != Some(&json!("2.0")) {
        return send(error_line(
            reply_id,
            &RpcError::invalid_request("jsonrpc must be \"2.0\""),
        ));
    }
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return send(error_line(
            reply_id,
            &RpcError::invalid_request("method must be a string"),
        ));
    };
    let mut params = obj.get("params").cloned().unwrap_or(Value::Null);
    let want_changes = take_changes_flag(method, &mut params);

    let mut g = hub.lock();
    g.expire_group();
    let before = fingerprint(&g.editor);
    let result = dispatch(&mut g, client.id, method, params);
    let after = fingerprint(&g.editor);
    let feed = if result.is_err() && before.0 == after.0 {
        // A rejected edit is atomic: whatever was recorded was undone again.
        g.editor.take_journal();
        None
    } else {
        g.seal_feed(before.0)
    };
    let result = match result {
        Ok(mut r) if want_changes => {
            r["changes"] = feed.clone().unwrap_or_else(
                || json!({ "base_hash": hex(before.0), "hash": hex(after.0), "list": [] }),
            );
            Ok(r)
        }
        other => other,
    };
    if id.is_some() {
        send(match &result {
            Ok(r) => ok_line(reply_id, r.clone()),
            Err(e) => error_line(reply_id, e),
        });
    }
    if result.is_ok() && before != after {
        g.broadcast(&notification(after.0, method, feed));
    }
}
