//! Conformance rules for the MCP wire protocol. Each test names the spec rule it
//! encodes (notes/ref-mcp-spec.yaml has the sources), so a spec change that we
//! adopt shows up here as a failing test first.

use composer_mcp::backend::{Backend, CallError};
use composer_mcp::config::Config;
use composer_mcp::schema::validate;
use composer_mcp::server::{Server, VERSIONS};
use composer_mcp::tools::build_tools;
use serde_json::{json, Value};

/// Answers daemon calls with the same shapes composerd returns.
struct Fake;

impl Backend for Fake {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        let edit = |extra: Value| {
            let mut v =
                json!({ "state_hash": "00", "undo_depth": 1, "redo_depth": 0, "undo_label": "x" });
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            v
        };
        match method {
            "command.apply" if params["command"]["command"] == "delete_track" => {
                Err(CallError::Remote {
                    code: "not_found".into(),
                    message: "no such track".into(),
                    index: None,
                })
            }
            "command.apply" => Ok(edit(json!({ "changed": true }))),
            "command.group" | "session.new" => Ok(edit(json!({}))),
            "undo" | "redo" => Ok(edit(json!({ "applied": true }))),
            "state.hash" => Ok(json!({ "state_hash": "00" })),
            "state.get" => Ok(json!({ "project": {}, "state_hash": "00" })),
            other => panic!("unexpected daemon call {other}"),
        }
    }
    fn shutdown(&mut self) {}
}

fn described() -> Value {
    json!([
        { "name": "add_track", "summary": "Adds a track.", "fields": [
            { "name": "id", "ty": "id", "nullable": false },
            { "name": "name", "ty": "text", "nullable": false },
            { "name": "kind", "ty": "instrument|audio", "nullable": false } ] },
        { "name": "delete_track", "summary": "Deletes a track.", "fields": [
            { "name": "track", "ty": "id", "nullable": false } ] },
        { "name": "set_tempo", "summary": "Sets tempo.", "fields": [
            { "name": "tick", "ty": "tick", "nullable": false },
            { "name": "mbpm", "ty": "u32", "nullable": false } ] },
    ])
}

fn server() -> Server {
    let cfg = Config::from_lookup(|_| None).unwrap();
    Server::with_backend(Box::new(Fake), build_tools(&described(), &cfg))
}

/// A server that already completed the handshake at `version`.
fn ready(version: &str) -> Server {
    let mut s = server();
    let r = send(
        &mut s,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": version, "capabilities": {} } }),
    );
    assert_eq!(r["result"]["protocolVersion"], version);
    assert!(s
        .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .is_none());
    s
}

fn send(s: &mut Server, v: Value) -> Value {
    raw(s, &v.to_string())
}

fn raw(s: &mut Server, line: &str) -> Value {
    let out = s.handle_line(line).expect("a reply");
    assert!(!out.contains('\n'), "stdio framing: no embedded newline");
    serde_json::from_str(&out).unwrap()
}

fn req(id: Value, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn tools(s: &mut Server, version_params: Value) -> Vec<Value> {
    send(s, req(json!(2), "tools/list", version_params))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone()
}

// ---- lifecycle ----

#[test]
fn supported_revisions_are_exactly_the_four_we_verified() {
    // lifecycle: servers answer with a version they support, newest preferred.
    assert_eq!(
        VERSIONS,
        &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"]
    );
}

#[test]
fn initialize_echoes_each_supported_version_and_has_required_fields() {
    for v in VERSIONS {
        let mut s = server();
        let r = send(
            &mut s,
            req(
                json!(1),
                "initialize",
                json!({ "protocolVersion": v, "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }),
            ),
        );
        let res = &r["result"];
        // initialize: protocolVersion, capabilities, serverInfo are required.
        assert_eq!(res["protocolVersion"], *v);
        assert!(res["capabilities"].is_object());
        assert!(res["serverInfo"]["name"].is_string());
        assert!(res["serverInfo"]["version"].is_string());
        assert!(res["instructions"].is_string());
    }
}

#[test]
fn unsupported_version_is_answered_with_our_newest() {
    // 2025-11-25 lifecycle: otherwise respond with another supported version, the latest.
    let mut s = server();
    let r = send(
        &mut s,
        req(
            json!(1),
            "initialize",
            json!({ "protocolVersion": "1999-01-01" }),
        ),
    );
    assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);
    assert!(r.get("error").is_none());
}

#[test]
fn initialize_without_a_version_is_invalid_params() {
    let mut s = server();
    let r = send(&mut s, req(json!(1), "initialize", json!({})));
    assert_eq!(r["error"]["code"], -32602);
}

#[test]
fn capabilities_declare_tools_only_and_listchanged_is_false() {
    // tools: servers MUST declare the capability; we never emit list_changed.
    let mut s = server();
    let r = send(
        &mut s,
        req(
            json!(1),
            "initialize",
            json!({ "protocolVersion": "2025-11-25" }),
        ),
    );
    let caps = r["result"]["capabilities"].as_object().unwrap();
    assert_eq!(caps.len(), 1);
    assert_eq!(caps["tools"]["listChanged"], false);
}

#[test]
fn ping_returns_an_empty_result_in_any_phase() {
    // ping: receiver MUST respond promptly with an empty result.
    let mut s = server();
    assert_eq!(
        send(&mut s, req(json!("p"), "ping", json!({})))["result"],
        json!({})
    );
    let mut s = ready("2025-11-25");
    assert_eq!(
        send(&mut s, req(json!(7), "ping", Value::Null))["result"],
        json!({})
    );
}

#[test]
fn notifications_never_get_a_reply() {
    // JSON-RPC: the receiver MUST NOT answer a notification, known or not.
    let mut s = ready("2025-11-25");
    for m in [
        "notifications/initialized",
        "notifications/cancelled",
        "notifications/progress",
        "notifications/roots/list_changed",
        "ping",
        "tools/list",
        "no/such/method",
    ] {
        let line =
            json!({ "jsonrpc": "2.0", "method": m, "params": { "requestId": 1 } }).to_string();
        assert!(s.handle_line(&line).is_none(), "{m}");
    }
}

#[test]
fn cancelling_an_unknown_or_finished_request_is_ignored() {
    // cancellation: unknown ids and completed requests MUST be tolerated.
    let mut s = ready("2025-11-25");
    let n = json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                    "params": { "requestId": "never-sent", "reason": "test" } });
    assert!(s.handle_line(&n.to_string()).is_none());
    assert_eq!(
        send(&mut s, req(json!(3), "ping", json!({})))["result"],
        json!({})
    );
}

#[test]
fn a_progress_token_on_a_request_is_accepted_without_progress_messages() {
    // progress: receivers MAY ignore the token; the reply is the only message.
    let mut s = ready("2025-11-25");
    let r = send(
        &mut s,
        req(
            json!(4),
            "tools/call",
            json!({ "name": "composer_state_hash", "arguments": {}, "_meta": { "progressToken": "t1" } }),
        ),
    );
    assert_eq!(r["result"]["isError"], false);
}

// ---- JSON-RPC 2.0 ----

#[test]
fn parse_error_has_null_id_and_code_32700() {
    let mut s = server();
    let r = raw(&mut s, "{not json");
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(r["id"], Value::Null);
    assert_eq!(r["jsonrpc"], "2.0");
}

#[test]
fn non_object_message_is_invalid_request() {
    let mut s = server();
    for line in ["1", "\"x\"", "null", "true"] {
        let r = raw(&mut s, line);
        assert_eq!(r["error"]["code"], -32600, "{line}");
        assert_eq!(r["id"], Value::Null);
    }
}

#[test]
fn request_ids_are_strings_or_integers_and_never_null() {
    // basic: requests MUST include a string or integer id; unlike JSON-RPC, never null.
    let mut s = server();
    for bad in [json!(null), json!(1.5), json!(true), json!({}), json!([1])] {
        let r = send(&mut s, req(bad.clone(), "ping", json!({})));
        assert_eq!(r["error"]["code"], -32600, "{bad}");
        assert_eq!(r["id"], Value::Null);
    }
    for good in [
        json!(0),
        json!(-5),
        json!(9007199254740993_i64),
        json!("abc"),
        json!(""),
    ] {
        let r = send(&mut s, req(good.clone(), "ping", json!({})));
        assert_eq!(r["id"], good);
        assert_eq!(r["result"], json!({}));
    }
}

#[test]
fn version_field_must_be_2_0() {
    let mut s = server();
    let r = send(&mut s, json!({ "id": 1, "method": "ping" }));
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], 1);
    let r = send(
        &mut s,
        json!({ "jsonrpc": "1.0", "id": 2, "method": "ping" }),
    );
    assert_eq!(r["error"]["code"], -32600);
}

#[test]
fn unknown_method_is_32601_and_echoes_the_id() {
    let mut s = server();
    let r = send(&mut s, req(json!("m"), "resources/list", json!({})));
    assert_eq!(r["error"]["code"], -32601);
    assert_eq!(r["id"], "m");
    // 2026-07-28 probe: a modern client sending server/discover must fall back cleanly.
    let r = send(&mut s, req(json!(2), "server/discover", json!({})));
    assert_eq!(r["error"]["code"], -32601);
}

#[test]
fn a_message_without_method_is_invalid_request_but_a_client_response_is_ignored() {
    let mut s = server();
    let r = send(&mut s, json!({ "jsonrpc": "2.0", "id": 5 }));
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], 5);
    // We never send requests, so a response from the client has nothing to match.
    assert!(s
        .handle_line(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#)
        .is_none());
    assert!(s
        .handle_line(r#"{"jsonrpc":"2.0","id":9,"error":{"code":-1,"message":"x"}}"#)
        .is_none());
}

#[test]
fn params_must_be_an_object_when_present() {
    let mut s = server();
    let r = send(&mut s, req(json!(1), "ping", json!([1, 2])));
    assert_eq!(r["error"]["code"], -32602);
}

#[test]
fn batches_exist_only_in_2025_03_26() {
    // 2025-03-26: receivers MUST support batches. 2025-06-18 removed them.
    let batch = json!([
        req(json!(1), "ping", json!({})),
        { "jsonrpc": "2.0", "method": "notifications/initialized" },
        req(json!(2), "ping", json!({})),
    ]);
    let mut s = ready("2025-03-26");
    let r = send(&mut s, batch.clone());
    let items = r.as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], 1);
    assert_eq!(items[1]["id"], 2);
    // A batch of notifications gets no reply at all.
    let only_notes = json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }]);
    assert!(s.handle_line(&only_notes.to_string()).is_none());
    // An empty batch is an invalid request.
    assert_eq!(send(&mut s, json!([]))["error"]["code"], -32600);
    for v in ["2025-06-18", "2025-11-25", "2024-11-05"] {
        let mut s = ready(v);
        let r = send(&mut s, batch.clone());
        assert_eq!(r["error"]["code"], -32600, "{v}");
        assert_eq!(r["id"], Value::Null);
    }
}

// ---- tools/list ----

#[test]
fn every_tool_has_the_required_fields_and_a_valid_name() {
    for v in VERSIONS {
        let mut s = ready(v);
        let list = tools(&mut s, json!({}));
        assert!(!list.is_empty());
        let mut seen = std::collections::HashSet::new();
        for t in &list {
            let name = t["name"].as_str().unwrap();
            // tools: 1..=128 chars of A-Z a-z 0-9 _ - . and unique.
            assert!((1..=128).contains(&name.len()), "{name}");
            assert!(name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)));
            assert!(seen.insert(name.to_string()), "duplicate {name}");
            assert!(t["description"].as_str().unwrap().len() > 10);
            assert_eq!(t["inputSchema"]["type"], "object", "{name}");
            // The schema must be an object, never null; no-arg tools forbid extras.
            assert!(t["inputSchema"].is_object());
        }
        for n in [
            "composer_state_hash",
            "composer_undo",
            "composer_redo",
            "composer_state_get",
        ] {
            let t = list.iter().find(|t| t["name"] == n).unwrap();
            assert_eq!(t["inputSchema"]["additionalProperties"], false, "{n}");
        }
    }
}

#[test]
fn tool_fields_follow_the_negotiated_revision() {
    // title and outputSchema arrived in 2025-06-18, annotations in 2025-03-26.
    for (v, annotations, title, output) in [
        ("2024-11-05", false, false, false),
        ("2025-03-26", true, false, false),
        ("2025-06-18", true, true, true),
        ("2025-11-25", true, true, true),
    ] {
        let mut s = ready(v);
        for t in tools(&mut s, json!({})) {
            assert_eq!(t.get("annotations").is_some(), annotations, "{v}");
            assert_eq!(t.get("title").is_some(), title, "{v}");
            assert_eq!(t.get("outputSchema").is_some(), output, "{v}");
        }
    }
}

#[test]
fn every_tool_has_a_title_and_a_root_object_output_schema() {
    let mut s = ready("2025-11-25");
    for t in tools(&mut s, json!({})) {
        let name = t["name"].as_str().unwrap();
        assert!(t["title"].as_str().unwrap().len() > 2, "{name}");
        assert_eq!(t["annotations"]["title"], t["title"]);
        // 2025-11-25 schema: outputSchema root is type object.
        assert_eq!(t["outputSchema"]["type"], "object", "{name}");
    }
}

#[test]
fn tool_list_is_one_page_in_a_stable_order_and_rejects_cursors() {
    // pagination: nextCursor absent means the end; invalid cursors are -32602.
    let mut s = ready("2025-11-25");
    let a = send(&mut s, req(json!(1), "tools/list", json!({})));
    assert!(a["result"].get("nextCursor").is_none());
    let b = send(&mut s, req(json!(2), "tools/list", json!({})));
    assert_eq!(
        a["result"]["tools"], b["result"]["tools"],
        "deterministic order"
    );
    let c = send(
        &mut s,
        req(json!(3), "tools/list", json!({ "cursor": "abc" })),
    );
    assert_eq!(c["error"]["code"], -32602);
    let c = send(&mut s, req(json!(4), "tools/list", json!({ "cursor": 5 })));
    assert_eq!(c["error"]["code"], -32602);
}

#[test]
fn annotations_are_honest_for_every_tool() {
    let mut s = ready("2025-11-25");
    let list = tools(&mut s, json!({}));
    let read_only = [
        "composer_state_get",
        "composer_project_summary",
        "composer_state_hash",
        "composer_dry_run",
        "composer_new_id",
        "composer_project_tree",
        "composer_selection_get",
        "composer_transport_get",
        "composer_meters_measure",
        "composer_sounds_list",
        "composer_theory_scale",
        "composer_theory_chords",
        "composer_theory_pitch",
        "composer_templates_list",
        "composer_recipes_list",
        "composer_recipe_plan",
        "composer_history",
        "composer_proposals",
        "composer_changes_since",
        "composer_commands_enabled",
        "composer_audio_assets",
        "composer_audio_peaks",
    ];
    for t in &list {
        let n = t["name"].as_str().unwrap();
        let a = &t["annotations"];
        assert_eq!(a["openWorldHint"], false, "{n}: nothing leaves the session");
        assert_eq!(a["readOnlyHint"], read_only.contains(&n), "{n}");
        if a["readOnlyHint"] == false {
            let additive = n.starts_with("composer_add_")
                || matches!(
                    n,
                    "composer_undo"
                        | "composer_redo"
                        | "composer_group_begin"
                        | "composer_group_commit"
                        | "composer_group_rollback"
                        | "composer_propose"
                        | "composer_selection_set"
                        | "composer_transport_set"
                        | "composer_import_midi"
                        | "composer_audio_import"
                );
            assert_eq!(a["destructiveHint"], !additive, "{n}");
            let idempotent = !matches!(
                n,
                "composer_undo"
                    | "composer_redo"
                    | "composer_session_new"
                    | "composer_import_midi"
                    | "composer_audio_import"
                    | "composer_group_begin"
                    | "composer_group_commit"
                    | "composer_group_rollback"
                    | "composer_propose"
                    | "composer_template_apply"
                    | "composer_recipe_apply"
            );
            assert_eq!(a["idempotentHint"], idempotent, "{n}");
        }
    }
    let by = |n: &str| list.iter().find(|t| t["name"] == n).unwrap();
    assert_eq!(
        by("composer_delete_track")["annotations"]["destructiveHint"],
        true
    );
    assert_eq!(
        by("composer_session_new")["annotations"]["destructiveHint"],
        true
    );
}

// ---- tools/call ----

#[test]
fn unknown_tool_and_malformed_call_are_protocol_errors() {
    // tools: unknown tool and malformed requests are JSON-RPC errors (-32602).
    let mut s = ready("2025-11-25");
    for params in [
        json!({ "name": "composer_nope", "arguments": {} }),
        json!({ "arguments": {} }),
        json!({ "name": 7 }),
        json!({ "name": "composer_state_hash", "arguments": [1] }),
        json!({ "name": "composer_state_hash", "arguments": "x" }),
    ] {
        let r = send(&mut s, req(json!(1), "tools/call", params.clone()));
        assert_eq!(r["error"]["code"], -32602, "{params}");
        assert!(r.get("result").is_none());
    }
}

#[test]
fn input_validation_failures_are_tool_errors_not_protocol_errors() {
    // 2025-11-25 (SEP-1303): input validation errors are results with isError true.
    let mut s = ready("2025-11-25");
    for (name, args) in [
        ("composer_set_tempo", json!({ "tick": 0, "mbpm": 5 })),
        ("composer_set_tempo", json!({ "tick": 0 })),
        (
            "composer_add_track",
            json!({ "id": "XYZ", "name": "a", "kind": "audio" }),
        ),
        ("composer_state_hash", json!({ "extra": 1 })),
    ] {
        let r = send(
            &mut s,
            req(
                json!(1),
                "tools/call",
                json!({ "name": name, "arguments": args }),
            ),
        );
        assert!(r.get("error").is_none(), "{name}");
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(
            r["result"]["structuredContent"]["code"],
            "invalid_arguments"
        );
        assert_eq!(r["result"]["content"][0]["type"], "text");
    }
}

#[test]
fn daemon_failures_are_tool_errors_with_a_stable_code() {
    let mut s = ready("2025-11-25");
    let r = send(
        &mut s,
        req(
            json!(1),
            "tools/call",
            json!({ "name": "composer_delete_track",
            "arguments": { "track": "00000000000000000000000000000001" } }),
        ),
    );
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["structuredContent"]["code"], "not_found");
}

#[test]
fn success_carries_text_that_mirrors_structured_content() {
    // tools: structured content SHOULD also be returned serialized in a text block.
    let mut s = ready("2025-11-25");
    let r = send(
        &mut s,
        req(
            json!(1),
            "tools/call",
            json!({ "name": "composer_state_hash", "arguments": {} }),
        ),
    );
    let res = &r["result"];
    assert_eq!(res["isError"], false);
    assert_eq!(res["content"][0]["type"], "text");
    let text: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, res["structuredContent"]);
    // structuredContent is a JSON object in this revision.
    assert!(res["structuredContent"].is_object());
}

#[test]
fn structured_content_is_left_out_before_2025_06_18() {
    for v in ["2024-11-05", "2025-03-26"] {
        let mut s = ready(v);
        let r = send(
            &mut s,
            req(
                json!(1),
                "tools/call",
                json!({ "name": "composer_state_hash", "arguments": {} }),
            ),
        );
        assert!(r["result"].get("structuredContent").is_none(), "{v}");
        assert!(r["result"]["content"][0]["text"].is_string());
    }
}

#[test]
fn results_match_each_tools_output_schema_success_and_error() {
    // outputSchema: servers MUST return structured results that conform to it.
    let mut s = ready("2025-11-25");
    let list = tools(&mut s, json!({}));
    let id = "00000000000000000000000000000001";
    let calls = [
        (
            "composer_add_track",
            json!({ "id": id, "name": "T", "kind": "audio" }),
        ),
        ("composer_set_tempo", json!({ "tick": 0, "mbpm": 120000 })),
        ("composer_delete_track", json!({ "track": id })),
        ("composer_set_tempo", json!({ "tick": 0, "mbpm": 5 })),
        ("composer_state_get", json!({})),
        ("composer_state_hash", json!({})),
        ("composer_undo", json!({})),
        ("composer_redo", json!({})),
        ("composer_session_new", json!({ "name": "x" })),
        ("composer_new_id", json!({ "count": 2 })),
        (
            "composer_group",
            json!({ "label": "g", "commands": [{ "command": "set_tempo", "tick": 0, "mbpm": 120000 }] }),
        ),
    ];
    for (name, args) in calls {
        let r = send(
            &mut s,
            req(
                json!(1),
                "tools/call",
                json!({ "name": name, "arguments": args }),
            ),
        );
        let schema = &list.iter().find(|t| t["name"] == name).unwrap()["outputSchema"];
        let sc = &r["result"]["structuredContent"];
        assert_eq!(validate(schema, sc), Ok(()), "{name}: {sc}");
    }
}

#[test]
fn output_schema_rejects_a_wrong_shape() {
    // Guards the check above against a validator that accepts everything.
    let mut s = ready("2025-11-25");
    let list = tools(&mut s, json!({}));
    let schema = &list.iter().find(|t| t["name"] == "composer_undo").unwrap()["outputSchema"];
    assert!(validate(schema, &json!({ "state_hash": 5 })).is_err());
    assert!(validate(schema, &json!({})).is_err());
}
