//! Conformance for the 2026-07-28 stateless era and its coexistence with the
//! legacy handshake. Sources: notes/ref-mcp-spec.yaml (modern_* and dual_era facts).

use composer_mcp::backend::{Backend, CallError};
use composer_mcp::config::Config;
use composer_mcp::server::{Server, MODERN_VERSIONS};
use composer_mcp::tools::build_tools;
use serde_json::{json, Value};
use std::io::Cursor;

struct Fake;

impl Backend for Fake {
    fn call(&mut self, method: &str, _params: Value) -> Result<Value, CallError> {
        match method {
            "command.apply" => Ok(json!({
                "state_hash": "00", "undo_depth": 1, "redo_depth": 0,
                "undo_label": "x", "changed": true
            })),
            other => panic!("unexpected daemon call {other}"),
        }
    }
    fn shutdown(&mut self) {}
}

fn server() -> Server {
    let cfg = Config::from_lookup(|_| None).unwrap();
    let described = json!([
        { "name": "set_tempo", "summary": "Sets tempo.", "fields": [
            { "name": "tick", "ty": "tick", "nullable": false },
            { "name": "mbpm", "ty": "u32", "nullable": false } ] },
    ]);
    Server::with_backend(Box::new(Fake), build_tools(&described, &cfg))
}

fn send(s: &mut Server, v: Value) -> Value {
    let out = s.handle_line(&v.to_string()).expect("a reply");
    assert!(!out.contains('\n'));
    serde_json::from_str(&out).unwrap()
}

fn meta(version: &str) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": { "name": "t", "version": "1" },
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

fn modern(id: i64, method: &str, mut params: Value) -> Value {
    params["_meta"] = meta("2026-07-28");
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

const INFO: &str = "io.modelcontextprotocol/serverInfo";

#[test]
fn discover_returns_versions_capabilities_info_and_cache_hints() {
    // server/discover: resultType complete, supportedVersions, capabilities, serverInfo in _meta, ttlMs, cacheScope.
    let mut s = server();
    let r = send(&mut s, modern(1, "server/discover", json!({})));
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["supportedVersions"], json!(MODERN_VERSIONS));
    assert!(res["capabilities"]["tools"].is_object());
    assert_eq!(res["_meta"][INFO]["name"], "composer-mcp");
    assert!(res["instructions"].is_string());
    assert!(res["ttlMs"].as_u64().is_some());
    assert_eq!(res["cacheScope"], "public");
}

#[test]
fn modern_requests_need_no_initialize_and_leave_no_state() {
    // Stateless: tools/list works on a fresh server, and a later legacy initialize is unaffected.
    let mut s = server();
    let r = send(&mut s, modern(1, "tools/list", json!({})));
    assert_eq!(r["result"]["resultType"], "complete");
    assert!(r["result"]["ttlMs"].as_u64().is_some());
    assert_eq!(r["result"]["cacheScope"], "public");
    let tools = r["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    assert!(tools[0]["outputSchema"].is_object() && tools[0]["annotations"].is_object());
    // Legacy handshake afterwards still negotiates normally.
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05", "capabilities": {} } }),
    );
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    assert!(
        r["result"].get("resultType").is_none(),
        "legacy results are unchanged"
    );
}

#[test]
fn modern_tools_call_has_result_type_and_structured_content() {
    let mut s = server();
    let r = send(&mut s, modern(1, "tools/list", json!({})));
    let name = r["result"]["tools"][0]["name"]
        .as_str()
        .unwrap()
        .to_string();
    let r = send(
        &mut s,
        modern(
            2,
            "tools/call",
            json!({ "name": name, "arguments": { "tick": 0, "mbpm": 120000 } }),
        ),
    );
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["_meta"][INFO]["name"], "composer-mcp");
    assert!(res["content"][0]["text"].is_string());
    assert!(res["structuredContent"].is_object());
    assert!(res["isError"].is_boolean());
    // Unknown tool is still a protocol error.
    let r = send(&mut s, modern(3, "tools/call", json!({ "name": "nope" })));
    assert_eq!(r["error"]["code"], -32602);
}

#[test]
fn unsupported_version_is_32022_with_supported_and_requested() {
    // basic/versioning: UnsupportedProtocolVersionError with data.supported and data.requested.
    let mut s = server();
    for method in ["server/discover", "tools/list", "tools/call"] {
        let mut m = modern(1, method, json!({ "name": "x" }));
        m["params"]["_meta"] = meta("1900-01-01");
        let r = send(&mut s, m);
        assert_eq!(r["error"]["code"], -32022, "{method}");
        assert_eq!(r["error"]["data"]["supported"], json!(MODERN_VERSIONS));
        assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
        assert_eq!(r["id"], 1);
    }
    // A legacy revision named in modern _meta is not served statelessly either.
    let mut m = modern(2, "tools/list", json!({}));
    m["params"]["_meta"] = meta("2025-11-25");
    assert_eq!(send(&mut s, m)["error"]["code"], -32022);
}

#[test]
fn missing_required_meta_fields_are_invalid_params() {
    // basic _meta: protocolVersion and clientCapabilities are required, a missing one is -32602.
    let mut s = server();
    let mut m = modern(1, "tools/list", json!({}));
    m["params"]["_meta"] = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28" });
    assert_eq!(send(&mut s, m)["error"]["code"], -32602);
    let mut m = modern(2, "tools/list", json!({}));
    m["params"]["_meta"] = json!({ "io.modelcontextprotocol/protocolVersion": 7 });
    assert_eq!(send(&mut s, m)["error"]["code"], -32602);
}

#[test]
fn modern_removed_and_unknown_methods_are_32601_and_notifications_get_no_reply() {
    // ping was removed in 2026-07-28; initialize is legacy only.
    let mut s = server();
    assert_eq!(
        send(&mut s, modern(1, "ping", json!({})))["error"]["code"],
        -32601
    );
    assert_eq!(
        send(&mut s, modern(2, "resources/list", json!({})))["error"]["code"],
        -32601
    );
    let n = json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                    "params": { "requestId": 1, "_meta": meta("2026-07-28") } });
    assert!(s.handle_line(&n.to_string()).is_none());
}

#[test]
fn legacy_fallback_probe_is_a_non_modern_error_and_does_not_break_the_session() {
    // stdio backward compatibility: a probe without modern _meta gets a plain error (not -32022),
    // so the client falls back to initialize, which then works.
    let mut s = server();
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 0, "method": "server/discover", "params": {} }),
    );
    assert_eq!(r["error"]["code"], -32601);
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-11-25", "capabilities": {} } }),
    );
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    assert!(s
        .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .is_none());
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    );
    assert!(r["result"]["tools"].is_array());
    assert!(r["result"].get("resultType").is_none());
}

#[test]
fn replayed_modern_session_over_stdio_framing_ends_when_stdin_closes() {
    // stdio: servers SHOULD exit promptly when stdin reaches EOF.
    let mut s = server();
    let input = [
        modern(1, "server/discover", json!({})).to_string(),
        modern(2, "tools/list", json!({})).to_string(),
    ]
    .join("\n")
        + "\n";
    let mut out = Vec::new();
    s.run(Cursor::new(input), &mut out, 1 << 20).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], 2);
}
