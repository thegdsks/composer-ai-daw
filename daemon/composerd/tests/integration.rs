use composerd::{serve_stdio, Config, Hub, WsServer};
use serde_json::{json, Value};
use std::io::Cursor;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const ORIGIN: &str = "http://localhost:5173";

fn req(id: u64, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn add_track(id: &str, name: &str) -> Value {
    json!({ "command": "add_track", "id": id, "name": name, "kind": "instrument", "index": null })
}

/// Feeds the lines to a fresh stdio session and returns every output message.
fn stdio(lines: &[String], max: usize) -> Vec<Value> {
    let hub = Hub::new(4);
    let input = lines.join("\n") + "\n";
    let mut out = Vec::new();
    serve_stdio(&hub, max, Cursor::new(input.into_bytes()), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn response(out: &[Value], id: u64) -> &Value {
    out.iter().find(|m| m["id"] == json!(id)).expect("response")
}

#[test]
fn stdio_apply_undo_redo_with_hashes() {
    let out = stdio(
        &[
            req(1, "state.hash", json!({})),
            req(
                2,
                "command.apply",
                json!({ "command": add_track("a1", "Piano") }),
            ),
            req(3, "undo", json!({})),
            req(4, "redo", json!({})),
            req(5, "state.get", json!({})),
        ],
        1 << 20,
    );
    let h0 = response(&out, 1)["result"]["state_hash"].clone();
    let applied = &response(&out, 2)["result"];
    let h1 = applied["state_hash"].clone();
    assert_ne!(h0, h1);
    assert_eq!(applied["undo_depth"], 1);
    assert_eq!(applied["undo_label"], "add_track");
    assert_eq!(response(&out, 3)["result"]["state_hash"], h0);
    assert_eq!(response(&out, 3)["result"]["applied"], true);
    assert_eq!(response(&out, 4)["result"]["state_hash"], h1);
    let state = &response(&out, 5)["result"];
    assert_eq!(state["state_hash"], h1);
    assert_eq!(state["project"]["tracks"].as_array().unwrap().len(), 1);
    // Each real mutation was pushed to the stdio client itself.
    let pushes = out
        .iter()
        .filter(|m| m["method"] == "state.changed")
        .count();
    assert_eq!(pushes, 3);
}

#[test]
fn malformed_and_unknown_requests_get_proper_errors() {
    let out = stdio(
        &[
            "{not json".to_string(),
            json!({ "jsonrpc": "2.0", "id": 7, "method": "nope" }).to_string(),
            req(
                8,
                "command.apply",
                json!({ "command": { "command": "set_tempo" } }),
            ),
            req(
                9,
                "command.apply",
                json!({ "command": add_track("zz", "x"), "extra": 1 }),
            ),
            json!({ "id": 10, "method": "state.hash" }).to_string(),
            req(
                11,
                "command.apply",
                json!({ "command": { "command": "delete_track", "track": "ff" } }),
            ),
        ],
        1 << 20,
    );
    assert_eq!(out[0]["error"]["code"], -32700);
    assert!(out[0]["id"].is_null());
    assert_eq!(response(&out, 7)["error"]["code"], -32601);
    assert_eq!(response(&out, 8)["error"]["code"], -32602);
    assert_eq!(response(&out, 8)["error"]["data"]["code"], "invalid_params");
    assert_eq!(response(&out, 9)["error"]["code"], -32602);
    assert_eq!(response(&out, 10)["error"]["code"], -32600);
    let rejected = &response(&out, 11)["error"];
    assert_eq!(rejected["code"], -32000);
    assert_eq!(rejected["data"]["code"], "not_found");
}

#[test]
fn stdio_oversized_line_is_rejected_and_session_continues() {
    let big = req(1, "state.hash", json!({ "pad": "x".repeat(4000) }));
    let out = stdio(&[big, req(2, "state.hash", json!({}))], 512);
    assert_eq!(out[0]["error"]["code"], -32600);
    assert!(out[0]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("too large"));
    assert!(response(&out, 2)["result"]["state_hash"].is_string());
}

#[test]
fn dry_run_does_not_mutate() {
    let out = stdio(
        &[
            req(1, "state.hash", json!({})),
            req(
                2,
                "command.apply",
                json!({ "command": add_track("a1", "Piano"), "dry_run": true }),
            ),
            req(3, "state.hash", json!({})),
            req(4, "undo", json!({})),
            req(5, "undo", json!({ "dry_run": true })),
            req(
                6,
                "command.group",
                json!({ "label": "g", "dry_run": true,
                "commands": [add_track("b1", "A"), add_track("b2", "B")] }),
            ),
            req(7, "state.get", json!({})),
        ],
        1 << 20,
    );
    let h0 = &response(&out, 1)["result"]["state_hash"];
    let dry = &response(&out, 2)["result"];
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["would_change"], true);
    assert_eq!(&dry["state_hash"], h0);
    assert_ne!(&dry["state_hash_after"], h0);
    assert!(!dry["changes"].as_array().unwrap().is_empty());
    assert_eq!(&response(&out, 3)["result"]["state_hash"], h0);
    assert_eq!(response(&out, 4)["result"]["applied"], false);
    assert_eq!(response(&out, 5)["result"]["would_apply"], false);
    assert_eq!(response(&out, 6)["result"]["dry_run"], true);
    assert_eq!(response(&out, 7)["result"]["project"]["tracks"], json!([]));
    assert!(out.iter().all(|m| m["method"] != "state.changed"));
}

#[test]
fn group_is_atomic() {
    let out = stdio(
        &[
            req(1, "state.hash", json!({})),
            req(
                2,
                "command.group",
                json!({ "label": "bad", "commands": [
                add_track("a1", "One"), add_track("a1", "Two") ] }),
            ),
            req(3, "state.hash", json!({})),
            req(
                4,
                "command.group",
                json!({ "label": "two tracks", "commands": [
                add_track("a1", "One"), add_track("a2", "Two") ] }),
            ),
            req(5, "undo", json!({})),
            req(6, "state.hash", json!({})),
            req(
                7,
                "command.group",
                json!({ "label": "none", "commands": [] }),
            ),
        ],
        1 << 20,
    );
    let h0 = &response(&out, 1)["result"]["state_hash"];
    let failed = &response(&out, 2)["error"];
    assert_eq!(failed["data"]["code"], "duplicate_id");
    assert_eq!(failed["data"]["index"], 1);
    assert_eq!(&response(&out, 3)["result"]["state_hash"], h0);
    assert_eq!(response(&out, 4)["result"]["undo_depth"], 1);
    assert_eq!(response(&out, 4)["result"]["undo_label"], "two tracks");
    // One undo reverts both commands.
    assert_eq!(&response(&out, 6)["result"]["state_hash"], h0);
    assert_eq!(response(&out, 7)["error"]["code"], -32602);
}

fn start_ws(max: usize) -> (SocketAddr, Config) {
    let cfg = Config {
        port: 0,
        token: TOKEN.to_string(),
        allowed_origins: vec![ORIGIN.to_string()],
        max_message_bytes: max,
        max_clients: 3,
        poll_ms: 5,
        token_generated: false,
        ..Config::default()
    };
    let hub = Arc::new(Hub::new(cfg.max_clients));
    let server = WsServer::bind(hub, cfg.clone()).unwrap();
    let addr = server.local_addr().unwrap();
    assert!(addr.ip().is_loopback());
    server.spawn();
    (addr, cfg)
}

type Ws = WebSocket<TcpStream>;

fn try_connect(
    addr: SocketAddr,
    query: &str,
    origin: Option<&str>,
) -> Result<Ws, tungstenite::Error> {
    let mut r = format!("ws://{addr}/{query}")
        .into_client_request()
        .unwrap();
    if let Some(o) = origin {
        r.headers_mut().insert("origin", o.parse().unwrap());
    }
    let s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tungstenite::client(r, s)
        .map(|(ws, _)| ws)
        .map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => e,
            tungstenite::HandshakeError::Interrupted(_) => tungstenite::Error::ConnectionClosed,
        })
}

fn connect(addr: SocketAddr) -> Ws {
    try_connect(addr, &format!("?token={TOKEN}"), None).unwrap()
}

fn next_json(ws: &mut Ws) -> Value {
    loop {
        if let Message::Text(t) = ws.read().unwrap() {
            return serde_json::from_str(t.as_str()).unwrap();
        }
    }
}

fn call(ws: &mut Ws, id: u64, method: &str, params: Value) -> Value {
    ws.send(Message::text(req(id, method, params))).unwrap();
    loop {
        let m = next_json(ws);
        if m["id"] == json!(id) {
            return m;
        }
    }
}

fn status_of(e: tungstenite::Error) -> u16 {
    match e {
        tungstenite::Error::Http(r) => r.status().as_u16(),
        other => panic!("expected an HTTP rejection, got {other:?}"),
    }
}

#[test]
fn websocket_two_clients_second_receives_state_changed() {
    let (addr, _) = start_ws(1 << 20);
    let mut a = connect(addr);
    let mut b = connect(addr);
    let r = call(
        &mut a,
        1,
        "command.apply",
        json!({ "command": add_track("a1", "Piano") }),
    );
    let hash = r["result"]["state_hash"].clone();
    let note = next_json(&mut b);
    assert_eq!(note["method"], "state.changed");
    assert_eq!(note["params"]["hash"], hash);
    assert_eq!(note["params"]["cause"], "command.apply");
    assert!(note.get("id").is_none());
    // The editor also gets the push, after its own response.
    assert_eq!(next_json(&mut a)["method"], "state.changed");
    // B unsubscribes; the next edit by A is not pushed to B.
    assert_eq!(
        call(&mut b, 2, "subscribe", json!({ "enabled": false }))["result"]["subscribed"],
        false
    );
    call(&mut a, 3, "undo", json!({}));
    let seen = call(&mut b, 4, "state.hash", json!({}));
    assert_eq!(
        seen["id"], 4,
        "no state.changed was queued ahead of the reply"
    );
}

#[test]
fn bad_token_and_wrong_origin_are_rejected() {
    let (addr, _) = start_ws(1 << 20);
    assert_eq!(
        status_of(
            try_connect(addr, "?token=wrong-token-wrong-token", None)
                .err()
                .unwrap()
        ),
        401
    );
    assert_eq!(status_of(try_connect(addr, "", None).err().unwrap()), 401);
    let bad_origin = try_connect(
        addr,
        &format!("?token={TOKEN}"),
        Some("http://evil.example"),
    );
    assert_eq!(status_of(bad_origin.err().unwrap()), 403);
    // Right origin plus token works, and so does a header token.
    assert!(try_connect(addr, &format!("?token={TOKEN}"), Some(ORIGIN)).is_ok());
    let mut r = format!("ws://{addr}/").into_client_request().unwrap();
    r.headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    let s = TcpStream::connect(addr).unwrap();
    assert!(tungstenite::client(r, s).is_ok());
}

#[test]
fn websocket_oversized_message_is_rejected() {
    let (addr, _) = start_ws(1024);
    let mut ws = connect(addr);
    ws.send(Message::text("x".repeat(5000))).unwrap();
    loop {
        match ws.read() {
            Ok(Message::Close(Some(f))) => {
                assert_eq!(u16::from(f.code), 1009);
                break;
            }
            Ok(Message::Close(None)) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

#[test]
fn websocket_malformed_json_gets_parse_error_and_stays_open() {
    let (addr, _) = start_ws(1 << 20);
    let mut ws = connect(addr);
    ws.send(Message::text("{broken")).unwrap();
    let e = next_json(&mut ws);
    assert_eq!(e["error"]["code"], -32700);
    assert!(
        call(&mut ws, 1, "commands.describe", json!({}))["result"]["commands"]
            .as_array()
            .is_some_and(|c| c.len() == composer_commands::describe().len())
    );
}

#[test]
fn client_limit_is_enforced_on_sockets() {
    let (addr, _) = start_ws(1 << 20);
    let _held: Vec<Ws> = (0..3).map(|_| connect(addr)).collect();
    let extra = try_connect(addr, &format!("?token={TOKEN}"), None);
    assert!(extra.is_err());
}

#[test]
fn config_from_env_validates_and_hides_token() {
    let get = |k: &str| match k {
        "COMPOSERD_TOKEN" => Some("short".to_string()),
        _ => None,
    };
    assert!(Config::from_lookup(get).is_err());
    let c = Config::from_lookup(|k| match k {
        "COMPOSERD_TOKEN" => Some(TOKEN.to_string()),
        "COMPOSERD_PORT" => Some("9000".to_string()),
        "COMPOSERD_ALLOWED_ORIGINS" => Some("HTTP://A.test, http://b.test".to_string()),
        _ => None,
    })
    .unwrap();
    assert_eq!(c.port, 9000);
    assert_eq!(c.allowed_origins, vec!["http://a.test", "http://b.test"]);
    assert!(!format!("{c:?}").contains(TOKEN));
    assert!(
        Config::from_lookup(|k| (k == "COMPOSERD_MAX_CLIENTS").then(|| "0".to_string())).is_err()
    );
}
