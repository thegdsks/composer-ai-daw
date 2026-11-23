//! Scripted session replays against a real composerd child over stdio, plus
//! the `composer` binary end to end. Scripts live in tests/scripts.

use composer_cli::plan::{plan, Input, UsageError};
use composer_cli::transport::{Reply, StdioTransport, Transport, WsTransport};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Builds composerd once and returns its path (it lands next to the composer binary).
fn daemon() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Command::new(cargo)
            .args(["build", "-q", "-p", "composerd"])
            .status()
            .expect("run cargo");
        assert!(status.success(), "cargo build -p composerd failed");
        let exe = PathBuf::from(env!("CARGO_BIN_EXE_composer"));
        let dir = exe.parent().expect("profile dir");
        let bin = dir.join(format!("composerd{}", std::env::consts::EXE_SUFFIX));
        assert!(bin.exists(), "missing {}", bin.display());
        bin
    })
}

fn daemon_str() -> &'static str {
    daemon().to_str().expect("utf8 path")
}

/// Subset match: every key in `want` must match in `got`; "@any" needs only presence.
fn matches(want: &Value, got: &Value) -> bool {
    match (want, got) {
        (Value::String(s), _) if s == "@any" => true,
        (Value::Object(w), Value::Object(g)) => w
            .iter()
            .all(|(k, v)| g.get(k).is_some_and(|x| matches(v, x))),
        _ => want == got,
    }
}

/// Replays a script and asserts each step's `expect` (result) or `expect_error`.
fn replay(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scripts")
        .join(name);
    let src = std::fs::read_to_string(&path).expect("script");
    let mut t = StdioTransport::spawn(daemon_str()).expect("spawn composerd");
    let mut results = Vec::new();
    for (n, line) in src.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let step: Value = serde_json::from_str(line).expect("script line");
        let method = step["method"].as_str().expect("method");
        let params = step.get("params").cloned().unwrap_or_else(|| json!({}));
        let reply = t.call(method, params).expect("transport");
        let ctx = format!("{name}:{} {method}", n + 1);
        match (&reply, step.get("expect"), step.get("expect_error")) {
            (Reply::Result(r), Some(want), _) => {
                assert!(matches(want, r), "{ctx}: wanted {want}, got {r}")
            }
            (Reply::Error(e), _, Some(want)) => {
                assert!(matches(want, e), "{ctx}: wanted {want}, got {e}")
            }
            (Reply::Result(_), None, None) => {}
            (other, _, _) => panic!("{ctx}: unexpected reply {other:?}"),
        }
        if let Reply::Result(r) = reply {
            results.push(r);
        }
    }
    results
}

#[test]
fn replay_build_and_undo() {
    let r = replay("build_and_undo.jsonl");
    let project = &r.last().expect("state")["project"];
    assert_eq!(project["tracks"].as_array().expect("tracks").len(), 1);
}

#[test]
fn replay_parity_methods() {
    let r = replay("parity.jsonl");
    assert!(r.len() >= 18);
}

#[test]
fn replay_history_undo_steps_three() {
    let r = replay("history_steps.jsonl");
    let hash = |i: usize| r[i]["state_hash"].clone();
    // r[1] is the empty project, r[3], r[5], ... the hash after edit 1, 2, ...
    let (h0, h2, h4) = (hash(1), hash(5), hash(9));
    // Each move is followed by a state.hash call in the script.
    let at = |steps: u64, depth: u64| {
        r.iter()
            .position(|v| v["steps_applied"] == steps && v["undo_depth"] == depth)
            .expect("move result")
            + 1
    };
    assert_eq!(hash(at(3, 2)), h2);
    assert_eq!(hash(at(2, 4)), h4);
    assert_eq!(hash(at(4, 0)), h0);
}

#[test]
fn replay_proposals_channel() {
    let r = replay("proposals.jsonl");
    let entry = &r[7]["entries"][0];
    assert_eq!(entry["source"], "ai");
    assert_eq!(entry["label"], "Bass line");
    // Hash before the post equals the hash after the undo of the accepted proposal.
    assert_eq!(r[1]["state_hash"], r.last().expect("hash")["state_hash"]);
}

#[test]
fn propose_words_carry_note_author_and_dry_run() {
    let steps = plan(
        &words("propose Lead [] --note hello --author bot"),
        true,
        &NoInput,
    )
    .expect("plan");
    assert_eq!(steps[0].method, "proposal.post");
    let p = &steps[0].params;
    assert_eq!(p["note"], "hello");
    assert_eq!(p["author"], "bot");
    assert_eq!(p["dry_run"], true);
    assert!(plan(&words("propose Lead [] --bogus x"), false, &NoInput).is_err());
    assert!(plan(&words("resolve p1"), false, &NoInput).is_err());
}

#[test]
fn replay_transport_session_state() {
    let r = replay("transport.jsonl");
    // The hash before any transport.set is the hash at the end of the replay.
    assert_eq!(r[1]["state_hash"], r.last().expect("undo")["state_hash"]);
}

#[test]
fn parity_words_map_to_daemon_methods() {
    let cases = [
        ("tree", "project.tree"),
        ("selection", "selection.get"),
        ("selection {}", "selection.set"),
        ("playhead", "transport.get"),
        ("playhead 960", "transport.set"),
        ("transport", "transport.get"),
        ("transport {\"metronome\":{\"on\":true}}", "transport.set"),
        ("meters", "meters.measure"),
        ("sounds", "sounds.list"),
        ("templates", "templates.list"),
        ("template calm-hour", "template.apply"),
        ("template calm-hour 7", "template.apply"),
        ("recipes", "recipes.list"),
        ("recipe calm_sections", "recipe.plan"),
        ("recipe calm_sections {\"seed\":3}", "recipe.plan"),
        ("recipe-apply calm_sections", "recipe.apply"),
        ("scale 0 major", "theory.scale"),
        ("chords 0 major", "theory.chords"),
        ("chords 0 major 3", "theory.chords"),
        ("pitch 0 major 61", "theory.pitch"),
        ("save a.cmpf", "project.save"),
        ("load a.cmpf", "project.load"),
        ("export-midi a.mid", "export.midi"),
        ("import-midi a.mid", "import.midi"),
        ("render a.wav", "render.wav"),
        (
            "propose Lead [{\"command\":\"set_tempo\",\"tick\":0,\"bpm\":90}]",
            "proposal.post",
        ),
        ("proposals", "proposal.list"),
        ("proposals all", "proposal.list"),
        ("resolve p1 accept", "proposal.resolve"),
    ];
    for (w, method) in cases {
        let steps = plan(&words(w), false, &NoInput).expect(w);
        assert_eq!(steps[0].method, method, "{w}");
    }
    let s = plan(&words("save a.cmpf --overwrite"), true, &NoInput).expect("plan");
    assert_eq!(
        s[0].params,
        json!({ "path": "a.cmpf", "overwrite": true, "dry_run": true })
    );
    assert!(plan(&words("playhead x"), false, &NoInput).is_err());
    assert!(plan(&words("pitch 0 major"), false, &NoInput).is_err());
    assert!(plan(&words("scale zero major"), false, &NoInput).is_err());
    let c = plan(&words("chords 7 minor 3"), false, &NoInput).expect("plan");
    assert_eq!(
        c[0].params,
        json!({ "root": 7, "scale": "minor", "octave": 3 })
    );
}

#[test]
fn recipe_words_carry_name_seed_and_dry_run() {
    let word = "recipe-apply calm_sections {\"seed\":3,\"params\":{\"minutes\":2}}";
    let p = plan(&words(word), true, &NoInput).expect("plan");
    assert_eq!(p[0].method, "recipe.apply");
    assert_eq!(
        p[0].params,
        json!({ "recipe": "calm_sections", "seed": 3, "params": { "minutes": 2 }, "dry_run": true })
    );
    let t = plan(&words("template blank 4"), true, &NoInput).expect("plan");
    assert_eq!(
        t[0].params,
        json!({ "template": "blank", "seed": 4, "dry_run": true })
    );
    assert!(plan(&words("template blank x"), false, &NoInput).is_err());
    assert!(plan(&words("recipe"), false, &NoInput).is_err());
    assert!(plan(&words("recipe a [1]"), false, &NoInput).is_err());
}

#[test]
fn replay_templates_and_recipes() {
    let r = replay("recipes.jsonl");
    // Lists and plans leave the hash alone; each apply is one undo step.
    assert_eq!(r[1]["state_hash"], r.last().expect("hash")["state_hash"]);
}

#[test]
fn replay_theory_reads_do_not_touch_the_project() {
    let r = replay("theory.jsonl");
    assert_eq!(r[1]["state_hash"], r.last().expect("hash")["state_hash"]);
}

#[test]
fn replay_dry_run_and_errors() {
    replay("dry_run_and_errors.jsonl");
}

struct NoInput;
impl Input for NoInput {
    fn file(&self, p: &str) -> Result<String, UsageError> {
        Err(UsageError(format!("no file {p}")))
    }
    fn stdin(&self) -> Result<String, UsageError> {
        Err(UsageError("no stdin".into()))
    }
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn every_word_maps_to_one_daemon_method() {
    let cases = [
        ("describe", "commands.describe"),
        ("state", "state.get"),
        ("hash", "state.hash"),
        ("undo", "undo"),
        ("redo", "redo"),
        ("undo 3", "undo"),
        ("history", "history.get"),
        ("history 10 20", "history.get"),
        ("enabled", "commands.enabled"),
        ("group L [] --source ai", "command.group"),
        ("new Song", "session.new"),
        ("apply {}", "command.apply"),
        ("group L []", "command.group"),
        ("rpc subscribe", "subscribe"),
    ];
    for (w, method) in cases {
        let steps = plan(&words(w), false, &NoInput).expect(w);
        assert_eq!(steps.len(), 1, "{w}");
        assert_eq!(steps[0].method, method, "{w}");
    }
}

#[test]
fn dry_run_flag_sets_dry_run_param_and_usage_errors_are_reported() {
    let s = plan(&words("undo"), true, &NoInput).expect("plan");
    assert_eq!(s[0].params, json!({ "dry_run": true }));
    assert!(plan(&words("undo 0"), false, &NoInput).is_err());
    assert!(plan(&words("history x"), false, &NoInput).is_err());
    let s = plan(&words("undo 3"), false, &NoInput).expect("plan");
    assert_eq!(s[0].params, json!({ "steps": 3 }));
    assert!(plan(&words("apply"), false, &NoInput).is_err());
    assert!(plan(&words("apply {bad"), false, &NoInput).is_err());
    assert!(plan(&words("frobnicate"), false, &NoInput).is_err());
    assert!(plan(&[], false, &NoInput).is_err());
}

fn composer(args: &[&str]) -> (Vec<Value>, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_composer"))
        .args(["--daemon", daemon_str()])
        .args(args)
        .output()
        .expect("run composer");
    let lines = String::from_utf8(out.stdout)
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("json line"))
        .collect();
    (lines, out.status.code().unwrap_or(-1))
}

#[test]
fn binary_prints_json_envelopes_and_exit_codes() {
    let (out, code) = composer(&["hash"]);
    assert_eq!(code, 0);
    assert_eq!(out[0]["method"], "state.hash");
    assert_eq!(
        out[0]["result"]["state_hash"].as_str().map(str::len),
        Some(16)
    );

    let (out, code) = composer(&[
        "--dry-run",
        "apply",
        r#"{"command":"set_project_name","name":"X"}"#,
    ]);
    assert_eq!(code, 0);
    assert_eq!(out[0]["result"]["dry_run"], true);

    let (out, code) = composer(&[
        "apply",
        r#"{"command":"delete_track","track":"00000000000000000000000000000001"}"#,
    ]);
    assert_eq!(code, 1);
    assert!(out[0]["error"]["data"]["code"].is_string());

    let (_, code) = composer(&["nonsense"]);
    assert_eq!(code, 2);
}

#[test]
fn run_subcommand_keeps_state_across_steps() {
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scripts/build_and_undo.jsonl");
    let arg = format!("@{}", script.display());
    let (out, code) = composer(&["run", &arg]);
    assert_eq!(code, 0);
    assert_eq!(out.len(), 10);
    assert_eq!(out[9]["result"]["project"]["tracks"][0]["name"], "Piano");
}

#[test]
fn websocket_transport_shares_one_session_between_invocations() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        l.local_addr().expect("addr").port()
    };
    let token = "0123456789abcdef0123456789abcdef";
    let mut child = Command::new(daemon())
        .env("COMPOSERD_PORT", port.to_string())
        .env("COMPOSERD_TOKEN", token)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn composerd");
    // The daemon announces the socket on stderr once it is listening.
    let mut err = BufReader::new(child.stderr.take().expect("stderr"));
    let mut line = String::new();
    err.read_line(&mut line).expect("banner");
    assert!(line.contains("websocket on"), "banner: {line}");

    let url = format!("ws://127.0.0.1:{port}");
    let mut a = WsTransport::connect(&url, token).expect("connect");
    a.call(
        "command.apply",
        json!({ "command": { "command": "set_project_name", "name": "Shared" } }),
    )
    .expect("apply");
    drop(a);
    let out = Command::new(env!("CARGO_BIN_EXE_composer"))
        .args(["--ws", &url, "state"])
        .env("COMPOSER_TOKEN", token)
        .output()
        .expect("run composer");
    let bad = WsTransport::connect(&url, "wrong-wrong-wrong-wrong").is_err();
    let _ = child.kill();
    let _ = child.wait();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["result"]["project"]["name"], "Shared");
    assert!(bad, "wrong token must be refused");
}
