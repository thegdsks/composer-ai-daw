//! MCP protocol over stdio: newline delimited JSON-RPC, tools only.

use crate::backend::{connect, read_bounded, Backend, Line};
use crate::config::Config;
use crate::tools::{build_tools, call_tool, find, Tool, ToolError};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// Protocol versions we speak, newest first. The client's pick is echoed when
/// supported, otherwise we answer with the newest.
pub const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Revisions served statelessly from per request `_meta`, no `initialize`.
pub const MODERN_VERSIONS: &[&str] = &["2026-07-28"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CAPS: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// Default `ttlMs` hint for `tools/list` and `server/discover`; the tool set is
/// fixed for the life of the process. Override with COMPOSER_MCP_CACHE_TTL_MS.
pub const DEFAULT_CACHE_TTL_MS: u64 = 60_000;

const INSTRUCTIONS: &str = "Composer AI DAW drives one live music project. Time is in ticks, 960 per quarter note (3840 per 4/4 bar). Tempo is in thousandths of a BPM (120000 = 120 BPM). Ids are 32 lowercase hex digits and you supply them: call composer_new_id, never invent ids. Pitch is 0..127, velocity 1..127. Notes start relative to their clip and must lie inside it. Every edit returns state_hash, undo_depth and undo_label, so verify with those or composer_project_summary instead of guessing. Use composer_group for many edits (one atomic undo step) and composer_dry_run to preview.";

pub struct Server {
    backend: Box<dyn Backend>,
    tools: Vec<Tool>,
    /// Revision agreed in `initialize`; shapes what we put on the wire.
    version: Option<&'static str>,
    cache_ttl_ms: u64,
}

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

type RpcError = (i64, String);

fn rpc_error_value(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    rpc_error_value(id, code, message).to_string()
}

/// `structuredContent` exists from 2025-06-18 on, older revisions get text only.
fn tool_result(r: Result<Value, ToolError>, version: &str) -> Value {
    let (text, structured, is_error) = match r {
        Ok(v) => (v.to_string(), v, false),
        Err(e) => (e.to_text(), e.to_value(), true),
    };
    let mut out = json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    });
    if version >= "2025-06-18" {
        out["structuredContent"] = structured;
    }
    out
}

/// JSON-RPC ids in MCP are strings or integers, never null.
fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

impl Server {
    /// Connects to composerd and builds the tool list from `commands.describe`.
    pub fn start(cfg: &Config) -> Result<Self, String> {
        let mut backend = connect(cfg)?;
        // Pushes are not used here, so ask the daemon not to send them.
        let _ = backend.call("subscribe", json!({ "enabled": false }));
        let described = backend
            .call("commands.describe", json!({}))
            .map_err(|e| format!("commands.describe failed: {e:?}"))?;
        let tools = build_tools(&described["commands"], cfg);
        Ok(Self::with_backend(backend, tools).with_cache_ttl_ms(cfg.cache_ttl_ms))
    }

    pub fn with_backend(backend: Box<dyn Backend>, tools: Vec<Tool>) -> Self {
        Self {
            backend,
            tools,
            version: None,
            cache_ttl_ms: DEFAULT_CACHE_TTL_MS,
        }
    }

    pub fn with_cache_ttl_ms(mut self, ms: u64) -> Self {
        self.cache_ttl_ms = ms;
        self
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Handles one protocol line; returns the reply line, if any.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Some(rpc_error(Value::Null, PARSE_ERROR, "parse error")),
        };
        let Value::Array(items) = v else {
            return self.handle_message(v).map(|r| r.to_string());
        };
        // Batches were only part of 2025-03-26 and were removed again after it.
        if self.version != Some("2025-03-26") {
            return Some(rpc_error(
                Value::Null,
                INVALID_REQUEST,
                "batch requests are not supported in this protocol revision",
            ));
        }
        if items.is_empty() {
            return Some(rpc_error(Value::Null, INVALID_REQUEST, "empty batch"));
        }
        let replies: Vec<Value> = items
            .into_iter()
            .filter_map(|m| self.handle_message(m))
            .collect();
        (!replies.is_empty()).then(|| Value::Array(replies).to_string())
    }

    /// One JSON-RPC message in, the response object out (none for notifications
    /// and for responses sent by the client).
    fn handle_message(&mut self, v: Value) -> Option<Value> {
        let Some(obj) = v.as_object() else {
            return Some(rpc_error_value(
                Value::Null,
                INVALID_REQUEST,
                "message must be an object",
            ));
        };
        let id = obj.get("id");
        if let Some(i) = id {
            if !valid_id(i) {
                return Some(rpc_error_value(
                    Value::Null,
                    INVALID_REQUEST,
                    "id must be a string or an integer, never null",
                ));
            }
        }
        let reply_id = id.cloned().unwrap_or(Value::Null);
        if obj.get("jsonrpc") != Some(&json!("2.0")) {
            return Some(rpc_error_value(
                reply_id,
                INVALID_REQUEST,
                "jsonrpc must be \"2.0\"",
            ));
        }
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // A response from the client: we never send requests, so ignore it.
            if obj.contains_key("result") || obj.contains_key("error") {
                return None;
            }
            return Some(rpc_error_value(
                reply_id,
                INVALID_REQUEST,
                "method must be a string",
            ));
        };
        let params = match obj.get("params") {
            None | Some(Value::Null) => Value::Object(Default::default()),
            Some(p @ Value::Object(_)) => p.clone(),
            Some(_) => {
                return id.map(|i| {
                    rpc_error_value(i.clone(), INVALID_PARAMS, "params must be an object")
                })
            }
        };
        // Notifications (no id) never get a reply. Cancellation and progress
        // need no action: calls run to completion one at a time.
        let id = id?.clone();
        // Modern per request metadata selects the stateless era for this request
        // only; anything else is the legacy session path, unchanged.
        if params["_meta"].get(META_VERSION).is_some() {
            return Some(match self.dispatch_modern(method, &params) {
                Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
                Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
            });
        }
        Some(match self.dispatch(method, params) {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, msg)) => rpc_error_value(id, code, &msg),
        })
    }

    fn server_info() -> Value {
        json!({
            "name": "composer-mcp",
            "title": "Composer AI DAW",
            "description": "Drives one live Composer AI DAW music project with edit, undo and read-back tools.",
            "version": env!("CARGO_PKG_VERSION"),
        })
    }

    /// 2026-07-28 request: no session, version and capabilities come from `_meta`.
    /// Returns the JSON-RPC error object on failure.
    fn dispatch_modern(&mut self, method: &str, params: &Value) -> Result<Value, Value> {
        let err = |code: i64, msg: &str| json!({ "code": code, "message": msg });
        let meta = &params["_meta"];
        let Some(asked) = meta[META_VERSION].as_str() else {
            return Err(err(
                INVALID_PARAMS,
                "protocolVersion in _meta must be a string",
            ));
        };
        if !MODERN_VERSIONS.contains(&asked) {
            return Err(json!({
                "code": UNSUPPORTED_PROTOCOL_VERSION,
                "message": "Unsupported protocol version",
                "data": { "supported": MODERN_VERSIONS, "requested": asked },
            }));
        }
        if !meta[META_CAPS].is_object() {
            return Err(err(
                INVALID_PARAMS,
                "clientCapabilities in _meta must be an object",
            ));
        }
        let mut result = match method {
            "server/discover" => json!({
                "supportedVersions": MODERN_VERSIONS,
                "capabilities": { "tools": { "listChanged": false } },
                "instructions": INSTRUCTIONS,
                "ttlMs": self.cache_ttl_ms,
                "cacheScope": "public",
            }),
            "tools/list" => {
                if !matches!(params.get("cursor"), None | Some(Value::Null)) {
                    return Err(err(INVALID_PARAMS, "invalid cursor"));
                }
                json!({
                    "tools": self.tools.iter().map(|t| t.listing(asked)).collect::<Vec<_>>(),
                    "ttlMs": self.cache_ttl_ms,
                    "cacheScope": "public",
                })
            }
            // Same body as the newest legacy revision (text plus structuredContent).
            "tools/call" => self
                .dispatch("tools/call", params.clone())
                .map_err(|(c, m)| err(c, &m))?,
            // ping was removed in this revision, initialize belongs to the legacy era.
            other => return Err(err(METHOD_NOT_FOUND, &format!("method not found: {other}"))),
        };
        result["resultType"] = json!("complete");
        result["_meta"] = json!({ META_SERVER_INFO: Self::server_info() });
        Ok(result)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => {
                let Some(asked) = params["protocolVersion"].as_str() else {
                    return Err((INVALID_PARAMS, "protocolVersion must be a string".into()));
                };
                // An unsupported ask is answered with our newest, the client decides.
                let version = VERSIONS
                    .iter()
                    .find(|v| **v == asked)
                    .unwrap_or(&VERSIONS[0]);
                self.version = Some(version);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "composer-mcp",
                        "title": "Composer AI DAW",
                        "description": "Drives one live Composer AI DAW music project with edit, undo and read-back tools.",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => {
                // The whole list fits one page, so no cursor was ever issued.
                match params.get("cursor") {
                    None | Some(Value::Null) => {}
                    Some(_) => return Err((INVALID_PARAMS, "invalid cursor".into())),
                }
                let v = self.version.unwrap_or(VERSIONS[0]);
                Ok(json!({
                    "tools": self.tools.iter().map(|t| t.listing(v)).collect::<Vec<_>>(),
                }))
            }
            "tools/call" => {
                let Some(name) = params["name"].as_str() else {
                    return Err((INVALID_PARAMS, "name must be a string".into()));
                };
                let args = match params.get("arguments") {
                    None | Some(Value::Null) => Value::Null,
                    Some(a @ Value::Object(_)) => a.clone(),
                    Some(_) => return Err((INVALID_PARAMS, "arguments must be an object".into())),
                };
                let Some(tool) = find(&self.tools, name) else {
                    return Err((INVALID_PARAMS, format!("Unknown tool: {name}")));
                };
                let tool = tool.clone();
                let v = self.version.unwrap_or(VERSIONS[0]);
                Ok(tool_result(
                    call_tool(&tool, args, self.backend.as_mut()),
                    v,
                ))
            }
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    /// Serves until the reader ends, then shuts the backend down.
    pub fn run(
        &mut self,
        mut reader: impl BufRead,
        mut writer: impl Write,
        max_message_bytes: usize,
    ) -> std::io::Result<()> {
        let result = (|| loop {
            let reply = match read_bounded(&mut reader, max_message_bytes)? {
                Line::Eof => return Ok(()),
                Line::Text(t) if t.trim().is_empty() => continue,
                Line::Text(t) => self.handle_line(&t),
                Line::TooLong => Some(rpc_error(Value::Null, -32600, "message too large")),
            };
            if let Some(r) = reply {
                writeln!(writer, "{r}")?;
                writer.flush()?;
            }
        })();
        self.backend.shutdown();
        result
    }
}
