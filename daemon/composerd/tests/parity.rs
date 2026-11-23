//! Daemon integration tests for the T17 parity methods (D14 to D17), driven
//! through the stdio transport like any client.

use composerd::{serve_stdio, FileConfig, Hub};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;

const T: &str = "00000000000000000000000000000010";
const R: &str = "00000000000000000000000000000011";
const N1: &str = "00000000000000000000000000000012";
const N2: &str = "00000000000000000000000000000013";

fn temp_dir(tag: &str) -> PathBuf {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).unwrap();
    let name: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let d = std::env::temp_dir().join(format!("composerd-it-{tag}-{name}"));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(&d).unwrap()
}

fn files(dir: &std::path::Path) -> FileConfig {
    FileConfig {
        dir: dir.to_path_buf(),
        sounds_file: PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sounds/catalogue.yaml"
        )),
        ..FileConfig::default()
    }
}

/// Runs the calls in order in one fresh session; returns the reply of each.
fn run(cfg: FileConfig, calls: &[(&str, Value)]) -> Vec<Value> {
    let hub = Hub::with_files(4, cfg);
    let input: String = calls
        .iter()
        .enumerate()
        .map(|(i, (m, p))| {
            json!({ "jsonrpc": "2.0", "id": i + 1, "method": m, "params": p }).to_string() + "\n"
        })
        .collect();
    let mut out = Vec::new();
    serve_stdio(&hub, 1 << 20, Cursor::new(input.into_bytes()), &mut out).unwrap();
    let msgs: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (1..=calls.len())
        .map(|i| {
            msgs.iter()
                .find(|m| m["id"] == json!(i))
                .cloned()
                .expect("reply")
        })
        .collect()
}

fn apply(c: Value) -> (&'static str, Value) {
    ("command.apply", json!({ "command": c }))
}

fn song() -> Vec<(&'static str, Value)> {
    vec![
        apply(
            json!({ "command": "add_track", "id": T, "name": "keys", "kind": "instrument", "index": null }),
        ),
        apply(
            json!({ "command": "add_region", "id": R, "track": T, "start": 0, "length": 3840, "kind": "midi" }),
        ),
        apply(json!({ "command": "add_notes", "region": R, "notes": [
            { "id": N1, "pitch": 60, "velocity": 100, "start": 0, "duration": 960 },
            { "id": N2, "pitch": 64, "velocity": 100, "start": 960, "duration": 960 },
        ] })),
    ]
}

fn ok(v: &Value) -> &Value {
    assert!(v.get("error").is_none(), "unexpected error: {v}");
    &v["result"]
}

fn code(v: &Value) -> &str {
    v["error"]["data"]["code"].as_str().expect("error code")
}

#[test]
fn new_commands_apply_dry_run_and_show_in_the_tree() {
    let mut calls = song();
    calls.push((
        "command.apply",
        json!({ "command": { "command": "set_track_volume", "track": T, "volume": 700 }, "dry_run": true }),
    ));
    calls.push(apply(
        json!({ "command": "set_track_volume", "track": T, "volume": 700 }),
    ));
    calls.push(apply(
        json!({ "command": "set_track_pan", "track": T, "pan": -250 }),
    ));
    calls.push(apply(
        json!({ "command": "set_track_mute", "track": T, "mute": true }),
    ));
    calls.push(apply(
        json!({ "command": "transpose_notes", "region": R, "semitones": 5, "notes": null }),
    ));
    calls.push(("project.tree", json!({})));
    calls.push(("project.tree", json!({ "notes": true, "track": T })));
    calls.push((
        "project.tree",
        json!({ "track": "00000000000000000000000000000099" }),
    ));
    let out = run(files(&temp_dir("tree")), &calls);
    let dry = ok(&out[3]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["would_change"], true);
    assert_ne!(dry["state_hash"], dry["state_hash_after"]);
    let tree = &ok(&out[8])["tree"];
    assert_eq!(tree["track_count"], 1);
    let t = &tree["tracks"][0];
    assert_eq!(
        t["mix"],
        json!({ "volume": 700, "pan": -250, "mute": true, "solo": false })
    );
    assert_eq!(
        (t["region_count"].as_u64(), t["note_count"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(t["regions"][0]["pitch_min"], 65);
    assert!(t["regions"][0].get("notes").is_none());
    let full = &ok(&out[9])["tree"]["tracks"][0]["regions"][0]["notes"];
    assert_eq!(full.as_array().unwrap().len(), 2);
    assert_eq!(full[0]["pitch"], 65);
    assert_eq!(code(&out[10]), "not_found");
    // The dry run changed nothing before the real edit: three adds, then four edits.
    assert_eq!(ok(&out[7])["undo_depth"], 7);
}

#[test]
fn selection_is_validated_pruned_and_reset_with_the_session() {
    let mut calls = song();
    calls.push((
        "selection.set",
        json!({ "tracks": [T], "regions": [R], "notes": [{ "region": R, "note": N1 }] }),
    ));
    calls.push(("selection.get", json!({})));
    calls.push(("selection.set", json!({ "tracks": [R] })));
    calls.push(("selection.set", json!({ "tracks": [T], "dry_run": true })));
    calls.push(apply(
        json!({ "command": "delete_note", "region": R, "note": N1 }),
    ));
    calls.push(("selection.get", json!({})));
    calls.push(("project.tree", json!({})));
    calls.push(("session.new", json!({ "name": "fresh" })));
    calls.push(("selection.get", json!({})));
    calls.push(("selection.set", json!({ "bogus": 1 })));
    let out = run(files(&temp_dir("sel")), &calls);
    assert_eq!(ok(&out[3])["selection"]["notes"][0]["note"], N1);
    assert_eq!(ok(&out[4])["selection"]["regions"][0], R);
    assert_eq!(code(&out[5]), "not_found");
    assert_eq!(ok(&out[6])["valid"], true);
    // The deleted note drops out of the selection on the next read.
    let pruned = &ok(&out[8])["selection"];
    assert_eq!(pruned["notes"].as_array().unwrap().len(), 0);
    assert_eq!(pruned["tracks"][0], T);
    assert_eq!(ok(&out[9])["selection"]["tracks"][0], T);
    assert_eq!(
        ok(&out[11])["selection"]["tracks"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(code(&out[12]), "invalid_params");
}

#[test]
fn transport_playhead_reads_back_in_ticks_and_seconds() {
    let out = run(
        files(&temp_dir("transport")),
        &[
            ("transport.get", json!({})),
            ("transport.set", json!({ "tick": 3840 })),
            ("transport.get", json!({})),
            ("transport.set", json!({ "tick": -1 })),
            ("transport.set", json!({ "tick": 960, "dry_run": true })),
            ("transport.get", json!({})),
            ("session.new", json!({ "name": "x" })),
            ("transport.get", json!({})),
        ],
    );
    assert_eq!(ok(&out[0])["playhead_tick"], 0);
    assert_eq!(ok(&out[0])["playing"], false);
    // One 4/4 bar at 120 BPM lasts two seconds.
    assert_eq!(ok(&out[2])["playhead_seconds"], 2.0);
    assert_eq!(code(&out[3]), "invalid");
    assert_eq!(ok(&out[5])["playhead_tick"], 3840);
    assert_eq!(ok(&out[7])["playhead_tick"], 0);
}

#[test]
fn save_then_load_restores_the_exact_state_and_resets_history() {
    let dir = temp_dir("saveload");
    let mut calls = song();
    calls.push(("state.hash", json!({})));
    calls.push(("project.save", json!({ "path": "a.cmpf", "dry_run": true })));
    calls.push(("project.save", json!({ "path": "a.cmpf" })));
    calls.push(("project.save", json!({ "path": "a.cmpf" })));
    calls.push((
        "project.save",
        json!({ "path": "a.cmpf", "overwrite": true }),
    ));
    calls.push(("session.new", json!({ "name": "other" })));
    calls.push(("project.load", json!({ "path": "a.cmpf", "dry_run": true })));
    calls.push(("state.hash", json!({})));
    calls.push(("project.load", json!({ "path": "a.cmpf" })));
    calls.push(("state.hash", json!({})));
    calls.push(("project.load", json!({ "path": "missing.cmpf" })));
    let out = run(files(&dir), &calls);
    let before = &ok(&out[3])["state_hash"];
    assert_eq!(ok(&out[4])["dry_run"], true);
    assert!(ok(&out[5])["bytes"].as_u64().unwrap() > 40);
    assert_eq!(code(&out[6]), "file_exists");
    assert_eq!(ok(&out[7])["path"], "a.cmpf");
    // A dry run load leaves the other session alone.
    assert_ne!(&ok(&out[10])["state_hash"], before);
    let loaded = ok(&out[11]);
    assert_eq!(loaded["tracks"], 1);
    assert_eq!(loaded["undo_depth"], 0);
    assert_eq!(&ok(&out[12])["state_hash"], before);
    assert_eq!(code(&out[13]), "file_not_found");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn file_methods_refuse_paths_outside_the_base_directory() {
    let dir = temp_dir("paths");
    let bad = [
        "../x.cmpf",
        "/tmp/x.cmpf",
        "sub/../../x.cmpf",
        "x.txt",
        "",
        "sub/x.cmpf",
    ];
    let mut calls = Vec::new();
    for p in bad {
        calls.push(("project.save", json!({ "path": p })));
        calls.push(("project.load", json!({ "path": p })));
    }
    calls.push(("export.midi", json!({ "path": "../a.mid" })));
    calls.push(("import.midi", json!({ "path": "../a.mid" })));
    calls.push(("render.wav", json!({ "path": "../a.wav" })));
    let out = run(files(&dir), &calls);
    for (i, o) in out.iter().enumerate() {
        let c = code(o);
        assert!(
            ["path_invalid", "path_outside_base", "file_not_found"].contains(&c),
            "call {i}: {o}"
        );
    }
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "nothing was written"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn midi_export_then_import_makes_one_undo_step() {
    let dir = temp_dir("midi");
    let mut calls = song();
    calls.push(("export.midi", json!({ "path": "song.mid" })));
    calls.push(("export.midi", json!({ "path": "song.mid" })));
    calls.push(("session.new", json!({ "name": "blank" })));
    calls.push((
        "import.midi",
        json!({ "path": "song.mid", "dry_run": true }),
    ));
    calls.push(("import.midi", json!({ "path": "song.mid", "seed": 7 })));
    calls.push(("project.tree", json!({})));
    calls.push(("undo", json!({})));
    calls.push(("project.tree", json!({})));
    let out = run(files(&dir), &calls);
    assert!(ok(&out[3])["bytes"].as_u64().unwrap() > 30);
    assert_eq!(code(&out[4]), "file_exists");
    let dry = ok(&out[6]);
    assert_eq!(dry["would_change"], true);
    assert_eq!(dry["stats"]["notes"], 2);
    let imported = ok(&out[7]);
    assert_eq!(imported["undo_depth"], 1);
    assert_eq!(imported["stats"]["notes"], 2);
    assert_eq!(ok(&out[8])["tree"]["tracks"][0]["note_count"], 2);
    assert_eq!(ok(&out[9])["applied"], true);
    assert_eq!(ok(&out[10])["tree"]["track_count"], 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn render_wav_writes_a_valid_file_and_respects_limits() {
    let dir = temp_dir("render");
    let mut calls = song();
    calls.push((
        "render.wav",
        json!({ "path": "out.wav", "sample_rate": 8000, "tail_seconds": 0.5, "dry_run": true }),
    ));
    calls.push((
        "render.wav",
        json!({ "path": "out.wav", "sample_rate": 8000, "tail_seconds": 0.5 }),
    ));
    calls.push((
        "render.wav",
        json!({ "path": "out.wav", "sample_rate": 8000 }),
    ));
    calls.push((
        "render.wav",
        json!({ "path": "f.wav", "format": "float32", "tail_seconds": 0.0 }),
    ));
    calls.push(("render.wav", json!({ "path": "g.wav", "format": "mp3" })));
    calls.push(("render.wav", json!({ "path": "h.wav", "sample_rate": 100 })));
    calls.push(("render.wav", json!({ "path": "h.wav", "tail_seconds": -1 })));
    let out = run(files(&dir), &calls);
    let dry = ok(&out[3]);
    let r = ok(&out[4]);
    assert_eq!(dry["frames"], r["frames"]);
    // The last note ends at tick 1920 (1 s at 120 BPM); with 0.5 s of tail that is 1.5 s.
    assert_eq!(r["frames"], 12_000);
    let wav = std::fs::read(dir.join("out.wav")).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(wav.len() as u64, r["bytes"].as_u64().unwrap());
    assert_eq!(wav.len(), 44 + 12_000 * 2 * 2);
    assert_eq!(code(&out[5]), "file_exists");
    assert!(std::fs::read(dir.join("f.wav")).unwrap().len() > 44);
    assert_eq!(code(&out[7]), "invalid");
    assert_eq!(code(&out[8]), "invalid");
    assert_eq!(code(&out[9]), "invalid");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn render_is_capped_by_the_env_limit() {
    let dir = temp_dir("cap");
    let mut cfg = files(&dir);
    cfg.max_render_seconds = 1;
    let mut calls = song();
    calls.push((
        "render.wav",
        json!({ "path": "out.wav", "tail_seconds": 0.0 }),
    ));
    calls.push(("meters.measure", json!({ "tail_seconds": 0.0 })));
    let out = run(cfg, &calls);
    // The song lasts 1 s plus nothing: exactly at the cap, so it passes.
    assert!(out[3].get("error").is_none(), "{}", out[3]);
    assert!(out[4].get("error").is_none());
    let mut cfg = files(&dir);
    cfg.max_render_seconds = 1;
    let out = run(
        cfg,
        &[
            apply(
                json!({ "command": "add_track", "id": T, "name": "k", "kind": "instrument", "index": null }),
            ),
            apply(
                json!({ "command": "add_region", "id": R, "track": T, "start": 0, "length": 100000, "kind": "midi" }),
            ),
            apply(
                json!({ "command": "add_note", "id": N1, "region": R, "pitch": 60, "velocity": 90, "start": 90000, "duration": 1000 }),
            ),
            (
                "render.wav",
                json!({ "path": "long.wav", "tail_seconds": 0 }),
            ),
            ("meters.measure", json!({ "tail_seconds": 0 })),
        ],
    );
    assert_eq!(code(&out[3]), "render_too_long");
    assert_eq!(code(&out[4]), "render_too_long");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn meters_read_levels_per_track_and_follow_mute() {
    let mut calls = song();
    calls.push((
        "meters.measure",
        json!({ "sample_rate": 8000, "tail_seconds": 0.1 }),
    ));
    calls.push(apply(
        json!({ "command": "set_track_mute", "track": T, "mute": true }),
    ));
    calls.push((
        "meters.measure",
        json!({ "sample_rate": 8000, "tail_seconds": 0.1 }),
    ));
    let out = run(files(&temp_dir("meters")), &calls);
    let loud = ok(&out[3]);
    assert_eq!(loud["offline"], true);
    assert!(loud["master"]["peak"].as_f64().unwrap() > 0.0);
    assert!(loud["tracks"][0]["rms"].as_f64().unwrap() > 0.0);
    assert!(loud["master"]["peak_db"].is_number());
    let quiet = ok(&out[5]);
    assert_eq!(quiet["master"]["peak"], 0.0);
    assert!(quiet["master"]["peak_db"].is_null());
    assert_eq!(quiet["tracks"][0]["name"], "keys");
}

#[test]
fn sounds_list_reads_the_catalogue_and_filters() {
    let mut cfg = files(&temp_dir("sounds"));
    let out = run(
        cfg.clone(),
        &[
            ("sounds.list", json!({})),
            ("sounds.list", json!({ "bundle": "yes" })),
            ("sounds.list", json!({ "bundle": "nope" })),
        ],
    );
    let all = ok(&out[0]);
    assert!(all["count"].as_u64().unwrap() >= 19);
    assert!(!all["gaps"].as_array().unwrap().is_empty());
    assert_eq!(all["entries"][0]["id"], "freepats-upright-piano-kw");
    let yes = ok(&out[1])["count"].as_u64().unwrap();
    assert!(yes >= 1 && yes < all["count"].as_u64().unwrap());
    assert_eq!(ok(&out[2])["count"], 0);
    cfg.sounds_file = PathBuf::from("/definitely/not/here.yaml");
    let out = run(cfg, &[("sounds.list", json!({}))]);
    assert_eq!(code(&out[0]), "catalogue_unavailable");
}

#[test]
fn split_duplicate_and_quantize_through_the_daemon_group() {
    let mut calls = song();
    let id = |n: u32| format!("{n:032x}");
    calls.push((
        "command.group",
        json!({ "label": "edit", "commands": [
            { "command": "split_region", "region": R, "at": 1000, "new_region": id(40), "note_ids": [id(41)] },
            { "command": "duplicate_region", "region": id(40), "new_region": id(50), "start": 8000, "track": null, "note_ids": [id(51)] },
            { "command": "quantize_notes", "region": R, "grid": 960, "strength": 100, "notes": null },
        ] }),
    ));
    calls.push(("project.tree", json!({ "notes": true })));
    calls.push(("undo", json!({})));
    calls.push(("project.tree", json!({})));
    let out = run(files(&temp_dir("group")), &calls);
    assert_eq!(ok(&out[3])["undo_depth"], 4);
    let regions = &ok(&out[4])["tree"]["tracks"][0]["regions"];
    assert_eq!(regions.as_array().unwrap().len(), 3);
    let after = ok(&out[6])["tree"]["tracks"][0].clone();
    assert_eq!(after["region_count"], 1);
    assert_eq!(after["regions"][0]["length"], 3840);
}

#[test]
fn transport_prefs_merge_validate_persist_and_never_touch_the_hash() {
    let dir = temp_dir("prefs");
    let mut calls = song();
    let base = calls.len();
    calls.extend([
        ("state.hash", json!({})),
        ("transport.get", json!({})),
        (
            "transport.set",
            json!({ "loop": { "on": true, "start": 0, "end": 7680 }, "metronome": { "on": true }, "count_in_bars": 2 }),
        ),
        ("transport.set", json!({ "loop": { "end": 3840 }, "dry_run": true })),
        ("transport.set", json!({ "follow": false })),
        ("transport.get", json!({})),
        ("transport.set", json!({ "loop": { "start": 9000 } })),
        ("transport.set", json!({ "count_in_bars": 99 })),
        ("transport.set", json!({})),
        ("state.hash", json!({})),
        ("project.save", json!({ "path": "p.cmpf" })),
        ("session.new", json!({ "name": "other" })),
        ("transport.get", json!({})),
        ("project.load", json!({ "path": "p.cmpf" })),
        ("transport.get", json!({})),
        ("state.hash", json!({})),
    ]);
    let out = run(files(&dir), &calls);
    let r = |i: usize| &out[base + i];
    assert_eq!(
        ok(r(1))["loop"],
        json!({ "on": false, "start": 0, "end": 0 })
    );
    assert_eq!(ok(r(1))["follow"], true);
    assert_eq!(ok(r(2))["loop"]["end"], 7680);
    assert_eq!(ok(r(3))["dry_run"], true);
    assert_eq!(ok(r(3))["loop"]["end"], 3840);
    let g = ok(r(5));
    assert_eq!(g["loop"], json!({ "on": true, "start": 0, "end": 7680 }));
    assert_eq!(g["metronome"], json!({ "on": true }));
    assert_eq!(g["count_in_bars"], 2);
    assert_eq!(g["follow"], false);
    for i in [6, 7, 8] {
        assert_eq!(code(r(i)), "invalid");
    }
    assert_eq!(ok(r(0))["state_hash"], ok(r(9))["state_hash"]);
    // A new session starts from defaults; loading the file restores the saved values.
    assert_eq!(ok(r(12))["count_in_bars"], 0);
    assert_eq!(ok(r(14))["loop"]["end"], 7680);
    assert_eq!(ok(r(14))["metronome"]["on"], true);
    assert_eq!(ok(r(0))["state_hash"], ok(r(15))["state_hash"]);
}

#[test]
fn files_saved_with_default_prefs_carry_no_transport_key() {
    let dir = temp_dir("noprefs");
    let mut calls = song();
    calls.push(("project.save", json!({ "path": "d.cmpf" })));
    run(files(&dir), &calls);
    let bytes = std::fs::read(dir.join("d.cmpf")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("count_in_bars"));
}
