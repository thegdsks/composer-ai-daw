//! Integration tests: the real composer-mcp binary talking to the real composerd.

use composer_mcp::schema::validate;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::Once;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(20);

fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_composer-mcp"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Makes sure target/<profile>/composerd exists next to composer-mcp.
fn composerd() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut cmd = Command::new(cargo);
        cmd.args(["build", "-q", "-p", "composerd"]);
        if bin_dir().ends_with("release") {
            cmd.arg("--release");
        }
        assert!(cmd.status().unwrap().success(), "building composerd failed");
    });
    bin_dir().join("composerd")
}

struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    out: Receiver<Value>,
    err: Receiver<String>,
    next: u64,
}

impl Mcp {
    fn start(env: &[(&str, &str)], with_path: bool) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_composer-mcp"));
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if with_path {
            cmd.env("COMPOSER_COMPOSERD", composerd());
        } else {
            composerd();
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take();
        let (tx, out) = mpsc::channel();
        let so = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for l in BufReader::new(so).lines().map_while(Result::ok) {
                // Every stdout line must be protocol JSON.
                tx.send(serde_json::from_str(&l).expect("stdout line is JSON"))
                    .ok();
            }
        });
        let (etx, err) = mpsc::channel();
        let se = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            for l in BufReader::new(se).lines().map_while(Result::ok) {
                etx.send(l).ok();
            }
        });
        let mut m = Self {
            child,
            stdin,
            out,
            err,
            next: 1,
        };
        m.handshake();
        m
    }

    fn send(&mut self, v: Value) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{v}").unwrap();
        s.flush().unwrap();
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let r = self.out.recv_timeout(WAIT).expect("reply in time");
        assert_eq!(r["id"], json!(id));
        r
    }

    fn handshake(&mut self) {
        let r = self.rpc(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" },
            }),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "composer-mcp");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    }

    /// Calls a tool and returns (isError, structuredContent).
    fn tool(&mut self, name: &str, args: Value) -> (bool, Value) {
        let r = self.rpc("tools/call", json!({ "name": name, "arguments": args }));
        let res = &r["result"];
        assert!(res["content"][0]["text"].is_string(), "{r}");
        (
            res["isError"].as_bool().unwrap(),
            res["structuredContent"].clone(),
        )
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, v) = self.tool(name, args);
        assert!(!err, "{name} failed: {v}");
        v
    }

    fn new_id(&mut self) -> String {
        self.ok("composer_new_id", json!({}))["ids"][0]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        let st = wait_exit(&mut self.child);
        assert!(st.success(), "composer-mcp exit status {st:?}");
    }
}

fn wait_exit(c: &mut Child) -> std::process::ExitStatus {
    for _ in 0..1000 {
        if let Some(s) = c.try_wait().unwrap() {
            return s;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = c.kill();
    panic!("process did not exit");
}

fn tool_list(m: &mut Mcp) -> Vec<Value> {
    m.rpc("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .clone()
}

/// Reads the daemon's own command list through a plain stdio session.
fn described_commands() -> Vec<Value> {
    let mut c = Command::new(composerd())
        .args(["--stdio", "--no-ws"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut i = c.stdin.take().unwrap();
    writeln!(
        i,
        r#"{{"jsonrpc":"2.0","id":1,"method":"commands.describe"}}"#
    )
    .unwrap();
    drop(i);
    let mut line = String::new();
    BufReader::new(c.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    c.wait().unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    v["result"]["commands"].as_array().unwrap().clone()
}

#[test]
fn handshake_negotiates_version_and_zero_config_spawn_works() {
    // No COMPOSER_COMPOSERD: the daemon is found next to our own binary.
    let mut m = Mcp::start(&[], false);
    let r = m.rpc(
        "initialize",
        json!({ "protocolVersion": "1999-01-01", "capabilities": {} }),
    );
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    assert!(r["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("960"));
    assert_eq!(m.rpc("ping", json!({}))["result"], json!({}));
    let unknown = m.rpc("nope/nope", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    m.finish();
}

#[test]
fn tools_cover_every_described_command_and_schemas_validate() {
    let mut m = Mcp::start(&[], true);
    let tools = tool_list(&mut m);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let described = described_commands();
    assert!(described.len() >= 19);
    for c in &described {
        let n = format!("composer_{}", c["name"].as_str().unwrap());
        assert!(names.contains(&n.as_str()), "missing tool {n}");
    }
    for fixed in [
        "composer_state_get",
        "composer_project_summary",
        "composer_state_hash",
        "composer_undo",
        "composer_redo",
        "composer_group",
        "composer_session_new",
        "composer_dry_run",
        "composer_new_id",
        "composer_project_tree",
        "composer_selection_get",
        "composer_selection_set",
        "composer_transport_get",
        "composer_transport_set",
        "composer_meters_measure",
        "composer_sounds_list",
        "composer_theory_scale",
        "composer_theory_chords",
        "composer_theory_pitch",
        "composer_templates_list",
        "composer_template_apply",
        "composer_recipes_list",
        "composer_recipe_plan",
        "composer_recipe_apply",
        "composer_project_save",
        "composer_project_load",
        "composer_export_midi",
        "composer_import_midi",
        "composer_audio_import",
        "composer_audio_assets",
        "composer_audio_peaks",
        "composer_render_wav",
        "composer_changes_since",
    ] {
        assert!(names.contains(&fixed), "missing {fixed}");
    }
    assert_eq!(names.len(), described.len() + 40);
    for t in &tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(t["description"].as_str().unwrap().len() > 20);
    }

    let by = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap()["inputSchema"].clone();
    let id = "00000000000000000000000000000001";
    let note = by("composer_add_note");
    let good =
        json!({ "id": id, "region": id, "pitch": 60, "velocity": 90, "start": 0, "duration": 960 });
    assert_eq!(validate(&note, &good), Ok(()));
    for (patch, why) in [
        (json!({ "pitch": 128 }), "pitch range"),
        (json!({ "velocity": 0 }), "velocity range"),
        (json!({ "id": "XYZ" }), "id format"),
        (json!({ "duration": 0 }), "duration minimum"),
        (json!({ "extra": 1 }), "unknown field"),
        (json!({ "pitch": "60" }), "type"),
    ] {
        let mut bad = good.clone();
        bad.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(validate(&note, &bad).is_err(), "should reject: {why}");
    }
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("region");
    assert!(validate(&note, &missing).is_err());
    // Nullable index may be omitted or null.
    let track = by("composer_add_track");
    let t = json!({ "id": id, "name": "T", "kind": "instrument" });
    assert_eq!(validate(&track, &t), Ok(()));
    let mut with_null = t.clone();
    with_null["index"] = Value::Null;
    assert_eq!(validate(&track, &with_null), Ok(()));
    let mut bad_kind = t.clone();
    bad_kind["kind"] = json!("drums");
    assert!(validate(&track, &bad_kind).is_err());
    // Group items are discriminated by command name.
    let group = by("composer_group");
    let mut item = good.clone();
    item["command"] = json!("add_note");
    let g = json!({ "label": "x", "commands": [item] });
    assert_eq!(validate(&group, &g), Ok(()));
    let g_bad = json!({ "label": "x", "commands": [{ "command": "nope" }] });
    assert!(validate(&group, &g_bad).is_err());
    m.finish();
}

fn hash(m: &mut Mcp) -> String {
    m.ok("composer_state_hash", json!({}))["state_hash"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn scripted_session_with_undo_redo_and_hash_checks() {
    let mut m = Mcp::start(&[], true);
    let s = m.ok("composer_session_new", json!({ "name": "Test song" }));
    assert_eq!(s["undo_depth"], 0);
    let empty = hash(&mut m);

    let (track, region) = (m.new_id(), m.new_id());
    let added = m.ok(
        "composer_add_track",
        json!({ "id": track, "name": "Piano", "kind": "instrument" }),
    );
    assert_eq!(added["undo_depth"], 1);
    assert_eq!(added["undo_label"], "add_track");
    assert_eq!(added["changed"], true);
    let h_track = added["state_hash"].as_str().unwrap().to_string();
    assert_ne!(h_track, empty);

    m.ok(
        "composer_add_region",
        json!({ "id": region, "track": track, "start": 0, "length": 7680, "kind": "midi" }),
    );
    let ids = m.ok("composer_new_id", json!({ "count": 3 }));
    let ids: Vec<String> = ids["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 3);
    let notes: Vec<Value> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            json!({
                "command": "add_note", "id": id, "region": region,
                "pitch": 60 + 4 * i, "velocity": 96, "start": 960 * i, "duration": 960,
            })
        })
        .collect();
    let g = m.ok(
        "composer_group",
        json!({ "label": "chord run", "commands": notes }),
    );
    assert_eq!(g["undo_depth"], 3);
    assert_eq!(g["undo_label"], "chord run");
    let h_notes = g["state_hash"].as_str().unwrap().to_string();

    let sum = m.ok("composer_project_summary", json!({}));
    assert_eq!(sum["state_hash"], json!(h_notes));
    assert_eq!(sum["track_count"], 1);
    assert_eq!(sum["region_count"], 1);
    assert_eq!(sum["note_count"], 3);
    assert_eq!(sum["span_ticks"], 7680);
    assert_eq!(sum["span_bars"], json!(2.0));
    assert_eq!(sum["tempo_bpm"], json!(120.0));
    assert_eq!(sum["tracks"][0]["notes"], 3);

    // One undo removes the whole group.
    let u = m.ok("composer_undo", json!({}));
    assert_eq!(u["applied"], true);
    assert_eq!(u["undo_depth"], 2);
    assert_eq!(m.ok("composer_project_summary", json!({}))["note_count"], 0);
    let r = m.ok("composer_redo", json!({}));
    assert_eq!(r["state_hash"], json!(h_notes));
    assert_eq!(hash(&mut m), h_notes);
    assert_eq!(
        m.ok("composer_project_summary", json!({}))["state_hash"],
        json!(h_notes)
    );

    let full = m.ok("composer_state_get", json!({}));
    assert_eq!(full["state_hash"], json!(h_notes));
    assert_eq!(full["project"]["tracks"][0]["name"], "Piano");

    // Back to the very start restores the empty hash exactly.
    for _ in 0..3 {
        m.ok("composer_undo", json!({}));
    }
    assert_eq!(hash(&mut m), empty);
    m.finish();
}

#[test]
fn invalid_input_is_a_tool_error_with_a_code() {
    let mut m = Mcp::start(&[], true);
    let (e, v) = m.tool("composer_set_tempo", json!({ "tick": 0, "mbpm": 5 }));
    assert!(e);
    assert_eq!(v["code"], "invalid_arguments");
    assert!(v["hint"].as_str().unwrap().len() > 10);

    // Well formed but rejected by the daemon: stable daemon code comes through.
    let ghost = m.new_id();
    let (e, v) = m.tool("composer_delete_track", json!({ "track": ghost }));
    assert!(e);
    assert_eq!(v["code"], "not_found");

    // A failing item reports its index and changes nothing.
    let h = hash(&mut m);
    let (t, bad) = (m.new_id(), m.new_id());
    let (e, v) = m.tool(
        "composer_group",
        json!({ "label": "oops", "commands": [
            { "command": "add_track", "id": t, "name": "A", "kind": "instrument" },
            { "command": "delete_track", "track": bad },
        ] }),
    );
    assert!(e);
    assert_eq!(v["code"], "not_found");
    assert_eq!(v["index"], 1);
    assert_eq!(hash(&mut m), h);

    // Unknown tool is a protocol error, malformed JSON gets a parse error.
    let r = m.rpc(
        "tools/call",
        json!({ "name": "composer_nope", "arguments": {} }),
    );
    assert_eq!(r["error"]["code"], -32602);
    m.send(json!("garbage"));
    let r = m.out.recv_timeout(WAIT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    m.finish();
}

#[test]
fn dry_run_reports_without_mutating() {
    let mut m = Mcp::start(&[], true);
    let before = hash(&mut m);
    let id = m.new_id();
    let d = m.ok(
        "composer_dry_run",
        json!({ "commands": [{ "command": "add_track", "id": id, "name": "Ghost", "kind": "audio" }] }),
    );
    assert_eq!(d["dry_run"], true);
    assert_eq!(d["would_change"], true);
    assert_eq!(d["state_hash"], json!(before));
    assert_ne!(d["state_hash_after"], json!(before));
    assert_eq!(hash(&mut m), before);
    assert_eq!(
        m.ok("composer_project_summary", json!({}))["track_count"],
        0
    );
    // A dry run of a bad command reports the error, still no mutation.
    let (e, v) = m.tool(
        "composer_dry_run",
        json!({ "commands": [{ "command": "delete_track", "track": id }] }),
    );
    assert!(e);
    assert_eq!(v["code"], "not_found");
    // Two commands plan in order on a copy.
    let (a, b) = (m.new_id(), m.new_id());
    let d = m.ok(
        "composer_dry_run",
        json!({ "commands": [
            { "command": "add_track", "id": a, "name": "A", "kind": "instrument" },
            { "command": "add_region", "id": b, "track": a, "start": 0, "length": 960, "kind": "midi" },
        ] }),
    );
    assert_eq!(d["would_change"], true);
    assert_eq!(hash(&mut m), before);
    m.finish();
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn child_is_reaped_after_stdin_closes() {
    let mut m = Mcp::start(&[], true);
    let line = m.err.recv_timeout(WAIT).expect("start log on stderr");
    let pid: u32 = line
        .rsplit(' ')
        .next()
        .unwrap()
        .parse()
        .expect("pid in log");
    assert!(pid_alive(pid));
    m.ok("composer_state_hash", json!({}));
    drop(m.stdin.take());
    let st = wait_exit(&mut m.child);
    assert!(st.success());
    // kill -0 fails once the child is gone and reaped, a zombie would still pass.
    assert!(!pid_alive(pid), "composerd child still present");
}

#[test]
fn oversized_line_is_refused_and_server_keeps_working() {
    let mut m = Mcp::start(&[("COMPOSER_MCP_MAX_MESSAGE_BYTES", "4096")], true);
    let big = "x".repeat(10_000);
    m.send(json!({ "jsonrpc": "2.0", "id": 99, "method": "ping", "params": { "pad": big } }));
    let r = m.out.recv_timeout(WAIT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(m.rpc("ping", json!({}))["result"], json!({}));
    let (e, v) = m.tool("composer_new_id", json!({ "count": 100000 }));
    assert!(e);
    assert_eq!(v["code"], "invalid_arguments");
    m.finish();
}

#[test]
fn remote_mode_over_websocket_with_token() {
    let token = "0123456789abcdef0123456789abcdef";
    let mut d = Command::new(composerd())
        .args(["--port", "0"])
        .env("COMPOSERD_TOKEN", token)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut log = BufReader::new(d.stderr.take().unwrap());
    let mut line = String::new();
    log.read_line(&mut line).unwrap();
    let url = line.trim().rsplit(' ').next().unwrap().to_string();
    assert!(url.starts_with("ws://"), "{line}");
    let mut m = Mcp::start(
        &[("COMPOSER_WS_URL", &url), ("COMPOSER_TOKEN", token)],
        false,
    );
    let id = m.new_id();
    let r = m.ok(
        "composer_add_track",
        json!({ "id": id, "name": "Remote", "kind": "audio" }),
    );
    assert_eq!(r["undo_depth"], 1);
    assert_eq!(
        m.ok("composer_project_summary", json!({}))["track_count"],
        1
    );
    m.finish();
    // A wrong token is refused at startup, without echoing the secret.
    let out = Command::new(env!("CARGO_BIN_EXE_composer-mcp"))
        .env("COMPOSER_WS_URL", &url)
        .env("COMPOSER_TOKEN", "ffffffffffffffffffffffffffffffff")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("ffffffff"));
    d.kill().unwrap();
    d.wait().unwrap();
}

#[test]
fn transport_loop_set_through_mcp_reads_back_through_the_daemon_without_touching_the_hash() {
    use composer_mcp::backend::{Backend, WsBackend};
    let token = "0123456789abcdef0123456789abcdef";
    let mut d = Command::new(composerd())
        .args(["--port", "0"])
        .env("COMPOSERD_TOKEN", token)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut log = BufReader::new(d.stderr.take().unwrap());
    let mut line = String::new();
    log.read_line(&mut line).unwrap();
    let url = line.trim().rsplit(' ').next().unwrap().to_string();
    let mut m = Mcp::start(
        &[("COMPOSER_WS_URL", &url), ("COMPOSER_TOKEN", token)],
        false,
    );
    let before = m.ok("composer_state_hash", json!({}))["state_hash"].clone();
    let tools = tool_list(&mut m);
    let schema = tools
        .iter()
        .find(|t| t["name"] == "composer_transport_set")
        .unwrap()["inputSchema"]
        .clone();
    let args = json!({
        "loop": { "on": true, "start": 0, "end": 7680 },
        "metronome": { "on": true },
        "count_in_bars": 2,
        "follow": false,
    });
    assert_eq!(validate(&schema, &args), Ok(()));
    assert!(validate(&schema, &json!({ "loop": { "bogus": 1 } })).is_err());
    m.ok("composer_transport_set", args);
    let mut daemon = WsBackend::connect(&url, token, WAIT).unwrap();
    let got = daemon.call("transport.get", json!({})).unwrap();
    assert_eq!(got["loop"], json!({ "on": true, "start": 0, "end": 7680 }));
    assert_eq!(got["metronome"], json!({ "on": true }));
    assert_eq!(got["count_in_bars"], 2);
    assert_eq!(got["follow"], false);
    let hash = daemon.call("state.hash", json!({})).unwrap();
    assert_eq!(hash["state_hash"], before);
    assert_eq!(m.ok("composer_state_hash", json!({}))["state_hash"], before);
    let via_mcp = m.ok("composer_transport_get", json!({}));
    assert_eq!(via_mcp["loop"], got["loop"]);
    m.finish();
    d.kill().unwrap();
    d.wait().unwrap();
}

#[test]
fn real_daemon_results_conform_to_each_output_schema() {
    // outputSchema: servers MUST return structured results that conform to it.
    let mut m = Mcp::start(&[], true);
    let tools = tool_list(&mut m);
    let check = |tools: &[Value], name: &str, v: &Value| {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(validate(&t["outputSchema"], v), Ok(()), "{name}: {v}");
    };
    let v = m.ok("composer_session_new", json!({ "name": "S" }));
    check(&tools, "composer_session_new", &v);
    let (a, b) = (m.new_id(), m.new_id());
    let v = m.ok("composer_new_id", json!({ "count": 2 }));
    check(&tools, "composer_new_id", &v);
    let v = m.ok(
        "composer_add_track",
        json!({ "id": a, "name": "T", "kind": "instrument" }),
    );
    check(&tools, "composer_add_track", &v);
    let v = m.ok(
        "composer_group",
        json!({ "label": "g", "commands": [
            { "command": "add_region", "id": b, "track": a, "start": 0, "length": 960, "kind": "midi" } ] }),
    );
    check(&tools, "composer_group", &v);
    for name in [
        "composer_state_get",
        "composer_project_summary",
        "composer_state_hash",
        "composer_undo",
        "composer_redo",
    ] {
        let v = m.ok(name, json!({}));
        check(&tools, name, &v);
    }
    let fresh = m.new_id();
    let v = m.ok(
        "composer_dry_run",
        json!({ "commands": [{ "command": "add_track", "id": fresh, "name": "Q", "kind": "audio" }] }),
    );
    check(&tools, "composer_dry_run", &v);
    let (c, d) = (m.new_id(), m.new_id());
    let v = m.ok(
        "composer_dry_run",
        json!({ "commands": [
            { "command": "add_track", "id": c, "name": "A", "kind": "audio" },
            { "command": "add_track", "id": d, "name": "B", "kind": "audio" } ] }),
    );
    check(&tools, "composer_dry_run", &v);
    // Failures keep the shared error shape.
    let (e, v) = m.tool("composer_delete_track", json!({ "track": fresh }));
    assert!(e);
    check(&tools, "composer_delete_track", &v);
    m.finish();
}

#[test]
fn text_with_newlines_stays_on_one_stdout_line() {
    // stdio: messages MUST NOT contain embedded newlines; the harness reads by line.
    let mut m = Mcp::start(&[], true);
    let name = "line one\nline two\r\u{2028}end";
    m.ok("composer_session_new", json!({ "name": name }));
    let sum = m.ok("composer_project_summary", json!({}));
    assert_eq!(sum["name"], name);
    // One reply per request: the next line is the next reply, not a leftover.
    assert_eq!(m.rpc("ping", json!({}))["result"], json!({}));
    m.finish();
}

#[test]
fn stderr_carries_logs_and_stdout_only_protocol() {
    // stdio: logs go to stderr; every stdout line is a JSON-RPC message (the
    // harness parses each one and fails on anything else).
    let mut m = Mcp::start(&[], true);
    let log = m.err.recv_timeout(WAIT).expect("a log line on stderr");
    assert!(log.starts_with("composer-mcp:"), "{log}");
    m.ok("composer_state_hash", json!({}));
    m.finish();
}

#[test]
fn closing_stdin_ends_the_server_with_success_even_without_a_handshake() {
    // lifecycle (stdio): the server exits when its input stream closes.
    let mut child = Command::new(env!("CARGO_BIN_EXE_composer-mcp"))
        .env("COMPOSER_COMPOSERD", composerd())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    assert!(wait_exit(&mut child).success());
}

#[test]
fn parity_tools_edit_read_save_and_render_through_the_daemon() {
    let dir = std::env::temp_dir().join(format!("composer-mcp-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir_s = dir.to_str().unwrap().to_string();
    let mut m = Mcp::start(&[("COMPOSERD_FILES_DIR", &dir_s)], true);
    m.handshake();
    let (t, r) = (m.new_id(), m.new_id());
    let (n1, n2) = (m.new_id(), m.new_id());
    m.ok(
        "composer_add_track",
        json!({ "id": t, "name": "keys", "kind": "instrument", "index": null }),
    );
    m.ok(
        "composer_add_region",
        json!({ "id": r, "track": t, "start": 0, "length": 3840, "kind": "midi" }),
    );
    m.ok(
        "composer_add_notes",
        json!({ "region": r, "notes": [
            { "id": n1, "pitch": 60, "velocity": 90, "start": 10, "duration": 400 },
            { "id": n2, "pitch": 64, "velocity": 90, "start": 970, "duration": 400 },
        ] }),
    );
    m.ok(
        "composer_quantize_notes",
        json!({ "region": r, "grid": 960, "strength": 100, "notes": null }),
    );
    m.ok(
        "composer_set_track_volume",
        json!({ "track": t, "volume": 500 }),
    );
    let tree = m.ok("composer_project_tree", json!({ "notes": true }));
    let notes = &tree["tree"]["tracks"][0]["regions"][0]["notes"];
    assert_eq!(notes[0]["start"], 0);
    assert_eq!(notes[1]["start"], 960);
    assert_eq!(tree["tree"]["tracks"][0]["mix"]["volume"], 500);
    m.ok("composer_selection_set", json!({ "tracks": [t] }));
    assert_eq!(
        m.ok("composer_selection_get", json!({}))["selection"]["tracks"][0],
        t
    );
    m.ok("composer_transport_set", json!({ "tick": 3840 }));
    assert_eq!(
        m.ok("composer_transport_get", json!({}))["playhead_seconds"],
        2.0
    );
    let h = m.ok("composer_state_hash", json!({}))["state_hash"].clone();
    m.ok("composer_project_save", json!({ "path": "s.cmpf" }));
    m.ok(
        "composer_render_wav",
        json!({ "path": "s.wav", "tail_seconds": 0.1 }),
    );
    m.ok("composer_export_midi", json!({ "path": "s.mid" }));
    assert!(dir.join("s.wav").exists() && dir.join("s.mid").exists());
    m.ok("composer_session_new", json!({ "name": "other" }));
    m.ok("composer_project_load", json!({ "path": "s.cmpf" }));
    assert_eq!(m.ok("composer_state_hash", json!({}))["state_hash"], h);
    let (is_err, e) = m.tool("composer_project_save", json!({ "path": "../evil.cmpf" }));
    assert!(is_err);
    assert_eq!(e["code"], "path_invalid");
    let meters = m.ok("composer_meters_measure", json!({ "tail_seconds": 0.1 }));
    assert!(meters["master"]["peak"].as_f64().unwrap() > 0.0);
    let sc = m.ok(
        "composer_theory_scale",
        json!({ "root": 2, "scale": "dorian" }),
    );
    assert_eq!(sc["pitch_classes"], json!([2, 4, 5, 7, 9, 11, 0]));
    for name in composer_mcp::tools::SCALE_NAMES {
        let r = m.ok("composer_theory_scale", json!({ "root": 0, "scale": name }));
        assert_eq!(r["scale"], name);
    }
    let ch = m.ok(
        "composer_theory_chords",
        json!({ "key": { "root": 0, "scale": "major" }, "octave": 3 }),
    );
    assert_eq!(ch["triads"][1]["name"], "Dm");
    assert_eq!(ch["octave"], 3);
    let pi = m.ok(
        "composer_theory_pitch",
        json!({ "root": 0, "scale": "major", "pitch": 61 }),
    );
    assert_eq!(pi["in_key"], false);
    assert_eq!(pi["nearest"], 60);
    let (bad, e) = m.tool(
        "composer_theory_pitch",
        json!({ "root": 12, "scale": "major", "pitch": 60 }),
    );
    assert!(bad);
    assert_eq!(e["code"], "invalid_arguments");

    // D28: the schema enums match what the daemon bundles.
    let t = m.ok("composer_templates_list", json!({}));
    let mut ids: Vec<&str> = t["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    let mut want = composer_mcp::tools::TEMPLATE_IDS.to_vec();
    want.sort_unstable();
    assert_eq!(ids, want);
    let r = m.ok("composer_recipes_list", json!({}));
    let names: Vec<&str> = r["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, composer_mcp::tools::RECIPE_NAMES);
    // Plan reads only; apply lands on the hash the plan promised and is one recipe step.
    let before = m.ok("composer_state_hash", json!({}))["state_hash"].clone();
    let plan = m.ok(
        "composer_recipe_plan",
        json!({ "recipe": "calm_sections", "seed": 4, "params": { "minutes": 1 } }),
    );
    assert_eq!(m.ok("composer_state_hash", json!({}))["state_hash"], before);
    let done = m.ok(
        "composer_recipe_apply",
        json!({ "recipe": "calm_sections", "seed": 4, "params": { "minutes": 1 } }),
    );
    assert_eq!(done["state_hash"], plan["state_hash_after"]);
    let hist = m.ok("composer_history", json!({}));
    let last = hist["entries"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["source"], "recipe");
    let tpl = m.ok(
        "composer_template_apply",
        json!({ "template": "blank", "seed": 2 }),
    );
    assert_eq!(tpl["template"], "blank");
    let (bad, e) = m.tool("composer_recipe_plan", json!({ "recipe": "nope" }));
    assert!(bad);
    assert_eq!(e["code"], "invalid_arguments");
    let (bad, e) = m.tool(
        "composer_recipe_plan",
        json!({ "recipe": "extend_to_length" }),
    );
    assert!(bad);
    assert_eq!(e["code"], "no_target");
    m.finish();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn proposal_posted_through_mcp_is_resolved_through_the_daemon() {
    use composer_mcp::backend::{Backend, WsBackend};
    let token = "0123456789abcdef0123456789abcdef";
    let mut d = Command::new(composerd())
        .args(["--port", "0"])
        .env("COMPOSERD_TOKEN", token)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut log = BufReader::new(d.stderr.take().unwrap());
    let mut line = String::new();
    log.read_line(&mut line).unwrap();
    let url = line.trim().rsplit(' ').next().unwrap().to_string();
    let mut m = Mcp::start(
        &[("COMPOSER_WS_URL", &url), ("COMPOSER_TOKEN", token)],
        false,
    );
    let tools = tool_list(&mut m);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"composer_propose"));
    assert!(names.contains(&"composer_proposals"));
    assert!(
        !names.iter().any(|n| n.contains("resolve")),
        "an agent must not approve its own proposal"
    );
    let before = m.ok("composer_state_hash", json!({}))["state_hash"].clone();
    let id = m.new_id();
    let posted = m.ok(
        "composer_propose",
        json!({
            "label": "Add a lead",
            "note": "One instrument track.",
            "author": "test",
            "commands": [{ "command": "add_track", "id": id, "name": "Lead", "kind": "instrument" }],
        }),
    );
    assert_eq!(posted["status"], "pending");
    assert_eq!(posted["change_count"], 1);
    assert_eq!(posted["state_hash"], before);
    // Posting changed nothing.
    assert_eq!(m.ok("composer_state_hash", json!({}))["state_hash"], before);
    let listed = m.ok("composer_proposals", json!({}));
    assert_eq!(listed["pending"], 1);
    assert_eq!(listed["proposals"][0]["applies"], true);

    let mut daemon = WsBackend::connect(&url, token, WAIT).unwrap();
    let pid = posted["id"].clone();
    let dry = daemon
        .call(
            "proposal.resolve",
            json!({ "id": pid, "action": "accept", "dry_run": true }),
        )
        .unwrap();
    assert_eq!(dry["state_hash"], before);
    let done = daemon
        .call("proposal.resolve", json!({ "id": pid, "action": "accept" }))
        .unwrap();
    assert_eq!(done["status"], "accepted");
    assert_eq!(done["undo_depth"], 1);
    assert_eq!(done["state_hash"], dry["state_hash_after"]);
    let hist = daemon.call("history.get", json!({})).unwrap();
    assert_eq!(hist["entries"][0]["source"], "ai");
    assert_eq!(hist["entries"][0]["label"], "Add a lead");
    assert_eq!(
        m.ok("composer_project_summary", json!({}))["track_count"],
        1
    );
    assert_eq!(m.ok("composer_proposals", json!({}))["pending"], 0);
    m.finish();
    d.kill().unwrap();
    d.wait().unwrap();
}

#[test]
fn change_feed_through_mcp_chains_on_the_state_hash() {
    let mut m = Mcp::start(&[], true);
    m.ok("composer_session_new", json!({ "name": "Feed" }));
    let base = hash(&mut m);
    let (t1, t2) = (m.new_id(), m.new_id());
    let g = m.ok(
        "composer_group",
        json!({
            "label": "two tracks", "changes": true,
            "commands": [
                { "command": "add_track", "id": t1, "name": "A", "kind": "instrument" },
                { "command": "add_track", "id": t2, "name": "B", "kind": "instrument" },
            ],
        }),
    );
    assert_eq!(g["changes"]["base_hash"], base);
    assert_eq!(g["changes"]["hash"], g["state_hash"]);
    assert_eq!(g["changes"]["list"].as_array().unwrap().len(), 2);
    // Without the flag the reply is the old shape.
    let u = m.ok("composer_undo", json!({}));
    assert!(u.get("changes").is_none());
    let since = m.ok("composer_changes_since", json!({ "hash": base }));
    assert_eq!(since["hash"], base);
    assert_eq!(since["list"], json!([]));
    let r = m.ok("composer_redo", json!({ "changes": true }));
    assert_eq!(r["changes"]["list"][0]["op"], "new_track");
    let since = m.ok("composer_changes_since", json!({ "hash": base }));
    assert_eq!(since["list"].as_array().unwrap().len(), 2);
    assert_eq!(since["hash"], r["state_hash"]);
    let stale = m.ok(
        "composer_changes_since",
        json!({ "hash": "0123456789abcdef" }),
    );
    assert_eq!(stale["full"], true);
    m.finish();
}
