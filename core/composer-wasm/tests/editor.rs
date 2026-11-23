//! Editor ABI tests: the JSON command sequence of web/spike/test/editor.mjs, rendered natively.

use composer_commands::{Command, Editor};
use composer_model::{Id, Project};
use composer_wasm::{compile_project, ffi, EditorSession, Session};
use serde_json::Value;

const SR: u32 = 48_000;
// Same value as PINNED_HASH in web/spike/test/editor.mjs.
const PINNED_HASH: u64 = 0x459668d8b032cecd;

const SEQUENCE: [&str; 5] = [
    r#"{"command":"add_track","id":"a","name":"lead","kind":"instrument","index":null}"#,
    r#"{"command":"set_instrument","track":"a","instrument":"2"}"#,
    r#"{"command":"add_region","id":"b","track":"a","start":0,"length":3840,"kind":"midi"}"#,
    r#"{"command":"add_note","id":"c","region":"b","pitch":60,"velocity":100,"start":0,"duration":960}"#,
    r#"{"command":"add_note","id":"d","region":"b","pitch":64,"velocity":90,"start":960,"duration":960}"#,
];

const MISSING_TRACK: &str = r#"{"command":"delete_track","track":"ff"}"#;

fn fnv(samples: &[f32]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn render_blob(blob: &[u8]) -> u64 {
    let mut s = Session::from_blob(blob).unwrap();
    let mut out = Vec::new();
    let mut left = s.length_frames;
    while left > 0 {
        let n = s.process(left.min(128) as usize);
        for i in 0..n {
            out.push(s.left[i]);
            out.push(s.right[i]);
        }
        left -= n as u64;
    }
    fnv(&out)
}

/// Copies the result buffer out after an ed_* call returned `p`.
unsafe fn take(h: *mut EditorSession, p: *const u8) -> Vec<u8> {
    assert!(!p.is_null());
    std::slice::from_raw_parts(p, ffi::ed_result_len(h)).to_vec()
}

unsafe fn json_of(h: *mut EditorSession, p: *const u8) -> Value {
    serde_json::from_slice(&take(h, p)).unwrap()
}

unsafe fn apply(h: *mut EditorSession, json: &str) -> Value {
    let p = ffi::ed_apply_json(h, json.as_ptr(), json.len());
    json_of(h, p)
}

fn native_editor() -> Editor {
    let mut e = Editor::new(Project::new(Id(u128::MAX), "song"));
    for line in SEQUENCE {
        e.execute(&serde_json::from_str::<Command>(line).unwrap())
            .unwrap();
    }
    e
}

#[test]
fn native_and_abi_paths_render_the_same_pinned_hash() {
    let e = native_editor();
    let native = render_blob(&compile_project(e.project(), SR, 0.25));
    assert_eq!(native, PINNED_HASH, "native hash {native:#x}");

    unsafe {
        let h = ffi::ed_new(b"song".as_ptr(), 4);
        assert!(!h.is_null());
        for line in SEQUENCE {
            assert!(apply(h, line)["result"].is_object(), "{line}");
        }
        assert_eq!(ffi::ed_state_hash(h), e.project().state_hash());
        let p = ffi::ed_engine_blob(h, SR);
        let blob = take(h, p);
        assert_eq!(render_blob(&blob), PINNED_HASH);
        ffi::ed_free(h);
    }
}

#[test]
fn invalid_command_returns_stable_code_and_keeps_hash() {
    unsafe {
        let h = ffi::ed_new(b"song".as_ptr(), 4);
        let before = ffi::ed_state_hash(h);
        let v = apply(h, MISSING_TRACK);
        assert_eq!(v["error"]["data"]["code"], "not_found");
        assert_eq!(ffi::ed_state_hash(h), before);
        let v = apply(h, r#"{"command":"nope"}"#);
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        assert_eq!(ffi::ed_state_hash(h), before);
        ffi::ed_free(h);
    }
}

#[test]
fn undo_redo_group_load_and_null_safety() {
    unsafe {
        let h = ffi::ed_new(b"song".as_ptr(), 4);
        let h0 = ffi::ed_state_hash(h);
        apply(h, SEQUENCE[0]);
        let h1 = ffi::ed_state_hash(h);
        assert_ne!(h0, h1);
        let p = ffi::ed_undo(h);
        assert_eq!(json_of(h, p)["result"]["applied"], true);
        assert_eq!(ffi::ed_state_hash(h), h0);
        ffi::ed_redo(h);
        assert_eq!(ffi::ed_state_hash(h), h1);

        // Second command fails, so the whole group rolls back and reports its index.
        let g = format!(
            r#"{{"label":"two","commands":[{},{}]}}"#,
            SEQUENCE[1], MISSING_TRACK
        );
        let p = ffi::ed_group_json(h, g.as_ptr(), g.len());
        assert_eq!(json_of(h, p)["error"]["data"]["index"], 1);
        assert_eq!(ffi::ed_state_hash(h), h1);

        // Save state, load into a fresh editor: same hash; duplicate ids are rejected.
        let p = ffi::ed_state_json(h);
        let state = json_of(h, p);
        let project = state["result"]["project"].to_string();
        let h2 = ffi::ed_new(b"x".as_ptr(), 1);
        let p = ffi::ed_load_json(h2, project.as_ptr(), project.len());
        assert_eq!(json_of(h2, p)["result"]["undo_depth"], 0);
        assert_eq!(ffi::ed_state_hash(h2), h1);
        let mut doc = state["result"]["project"].clone();
        let t = doc["tracks"][0].clone();
        doc["tracks"].as_array_mut().unwrap().push(t);
        let dup = doc.to_string();
        let p = ffi::ed_load_json(h2, dup.as_ptr(), dup.len());
        assert_eq!(json_of(h2, p)["error"]["data"]["code"], "duplicate_id");
        assert_eq!(ffi::ed_state_hash(h2), h1);

        let p = ffi::ed_describe_json(h);
        assert_eq!(
            json_of(h, p)["result"]["commands"]
                .as_array()
                .unwrap()
                .len(),
            56
        );

        assert!(ffi::ed_engine_blob(h, 0).is_null());
        assert!(ffi::ed_undo(std::ptr::null_mut()).is_null());
        assert_eq!(ffi::ed_state_hash(std::ptr::null()), 0);
        assert!(ffi::ed_new([0xffu8].as_ptr(), 1).is_null());
        ffi::ed_free(h);
        ffi::ed_free(h2);
        ffi::ed_free(std::ptr::null_mut());
    }
}

#[test]
fn load_json_runs_the_full_validator() {
    unsafe {
        let h = ffi::ed_new(b"song".as_ptr(), 4);
        for line in SEQUENCE {
            apply(h, line);
        }
        let before = ffi::ed_state_hash(h);
        let p = ffi::ed_state_json(h);
        let good = json_of(h, p)["result"]["project"].clone();
        // Region is 3840 long; note d starts at 960 with duration 960.
        #[allow(clippy::type_complexity)]
        let cases: [(&str, fn(&mut Value), &str); 5] = [
            (
                "outside",
                |d| {
                    d["tracks"][0]["regions"]["0000000000000000000000000000000b"]["content"]
                        ["midi"]["notes"]["0000000000000000000000000000000d"]["start"] = 3000.into()
                },
                "note_outside_region",
            ),
            (
                "pitch",
                |d| {
                    d["tracks"][0]["regions"]["0000000000000000000000000000000b"]["content"]
                        ["midi"]["notes"]["0000000000000000000000000000000c"]["pitch"] = 200.into()
                },
                "pitch_range",
            ),
            (
                "velocity",
                |d| {
                    d["tracks"][0]["regions"]["0000000000000000000000000000000b"]["content"]
                        ["midi"]["notes"]["0000000000000000000000000000000c"]["velocity"] = 0.into()
                },
                "velocity_range",
            ),
            (
                "tempo",
                |d| d["tempo_map"]["tempos"]["0"] = 5.into(),
                "tempo_range",
            ),
            (
                "name",
                |d| d["name"] = "x".repeat(300).into(),
                "name_length",
            ),
        ];
        for (label, edit, code) in cases {
            let mut doc = good.clone();
            edit(&mut doc);
            let bad = doc.to_string();
            let p = ffi::ed_load_json(h, bad.as_ptr(), bad.len());
            let v = json_of(h, p);
            assert_eq!(v["error"]["data"]["code"], code, "{label}: {v}");
            assert_eq!(ffi::ed_state_hash(h), before, "{label} must not replace");
        }
        let ok = good.to_string();
        let p = ffi::ed_load_json(h, ok.as_ptr(), ok.len());
        assert!(json_of(h, p)["result"].is_object());
        ffi::ed_free(h);
    }
}

#[test]
fn transport_session_state_matches_the_daemon_shapes_and_leaves_the_hash_alone() {
    let mut ed = EditorSession::new("t").unwrap();
    let h = ed.state_hash();
    let mut call = |json: &str| -> Value {
        ed.transport_set(json.as_bytes());
        serde_json::from_slice(ed.result()).unwrap()
    };
    let set = call(
        r#"{"loop":{"on":true,"start":0,"end":7680},"metronome":{"on":true},"count_in_bars":2,"tick":960}"#,
    );
    assert_eq!(set["result"]["loop"]["end"], 7680);
    assert_eq!(set["result"]["playhead_tick"], 960);
    let dry = call(r#"{"follow":false,"dry_run":true}"#);
    assert_eq!(dry["result"]["dry_run"], true);
    assert_eq!(dry["result"]["follow"], false);
    for bad in [
        r#"{}"#,
        r#"{"loop":{"start":9000}}"#,
        r#"{"count_in_bars":99}"#,
        r#"{"tick":-1}"#,
    ] {
        assert_eq!(call(bad)["error"]["data"]["code"], "invalid", "{bad}");
    }
    assert_eq!(
        call(r#"{"bogus":1}"#)["error"]["data"]["code"],
        "invalid_params"
    );
    ed.transport_get();
    let got: Value = serde_json::from_slice(ed.result()).unwrap();
    assert_eq!(got["result"]["metronome"]["on"], true);
    assert_eq!(got["result"]["follow"], true);
    assert_eq!(got["result"]["count_in_bars"], 2);
    assert_eq!(ed.state_hash(), h);
}

#[test]
fn key_commands_and_scale_pitch_classes() {
    let mut ed = EditorSession::new("k").unwrap();
    let h0 = ed.state_hash();
    ed.apply(br#"{"command":"set_key","tick":0,"root":9,"scale":"minor"}"#);
    let r: Value = serde_json::from_slice(ed.result()).unwrap();
    assert_eq!(r["result"]["changed"], true);
    assert_ne!(ed.state_hash(), h0);
    ed.apply(br#"{"command":"set_key","tick":0,"root":12,"scale":"minor"}"#);
    let r: Value = serde_json::from_slice(ed.result()).unwrap();
    assert_eq!(r["error"]["data"]["code"], "invalid");
    ed.undo();
    assert_eq!(ed.state_hash(), h0);
    ed.scale_info(br#"{"root":9,"scale":"minor"}"#);
    let r: Value = serde_json::from_slice(ed.result()).unwrap();
    assert_eq!(
        r["result"]["pitch_classes"],
        serde_json::json!([9, 11, 0, 2, 4, 5, 7])
    );
    ed.scale_info(br#"{"root":12,"scale":"major"}"#);
    let r: Value = serde_json::from_slice(ed.result()).unwrap();
    assert_eq!(r["error"]["data"]["code"], "invalid_params");
    assert_eq!(ed.state_hash(), h0);
}

#[test]
fn change_feed_is_off_by_default_and_chains_hashes() {
    let mut s = EditorSession::new("feed").unwrap();
    let json = |s: &EditorSession| serde_json::from_slice::<Value>(s.result()).unwrap();
    s.apply(SEQUENCE[0].as_bytes());
    assert!(json(&s)["result"].get("changes").is_none());
    s.set_feed(10);
    let base = format!("{:016x}", s.state_hash());
    s.apply(SEQUENCE[1].as_bytes());
    let r = json(&s);
    assert_eq!(r["result"]["changes"]["base_hash"], base.as_str());
    assert_eq!(r["result"]["changes"]["hash"], r["result"]["state_hash"]);
    assert_eq!(r["result"]["changes"]["list"][0]["op"], "set");
    // A rejected command leaves nothing behind for the next feed.
    s.apply(MISSING_TRACK.as_bytes());
    assert!(json(&s).get("error").is_some());
    s.undo();
    let u = json(&s);
    assert_eq!(u["result"]["changes"]["list"].as_array().unwrap().len(), 1);
    // Over the cap the feed says full instead of listing.
    s.set_feed(1);
    s.group(
        format!(
            r#"{{"label":"g","commands":[{},{}]}}"#,
            SEQUENCE[1], SEQUENCE[2]
        )
        .as_bytes(),
    );
    let g = json(&s);
    assert_eq!(g["result"]["changes"]["full"], true);
    assert!(g["result"]["changes"].get("list").is_none());
}

fn result_of(ed: &EditorSession) -> Value {
    serde_json::from_slice(ed.result()).unwrap()
}

#[test]
fn chords_and_pitch_helpers() {
    let mut ed = EditorSession::new("t").unwrap();
    let h0 = ed.state_hash();
    ed.chords_info(br#"{"key":{"root":0,"scale":"major"}}"#);
    let r = result_of(&ed);
    let names: Vec<&str> = r["result"]["triads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
    assert_eq!(r["result"]["sevenths"][4]["name"], "G7");
    assert_eq!(
        r["result"]["sevenths"][4]["pitches"],
        serde_json::json!([67, 71, 74, 77])
    );
    assert_eq!(r["result"]["sevenths"][4]["roman"], "V7");
    ed.chords_info(br#"{"root":5,"scale":"major","octave":3}"#);
    let r = result_of(&ed);
    assert_eq!(r["result"]["triads"][3]["name"], "Bb");
    assert_eq!(r["result"]["triads"][0]["pitches"][0], 53);
    for bad in [
        r#"{"key":{"root":12,"scale":"major"}}"#,
        r#"{"root":0,"scale":"major","octave":9}"#,
        r#"{"root":0}"#,
        r#"{"key":{"root":0,"scale":"major"},"root":1,"scale":"minor"}"#,
        r#"{"bogus":1}"#,
    ] {
        ed.chords_info(bad.as_bytes());
        assert_eq!(
            result_of(&ed)["error"]["data"]["code"],
            "invalid_params",
            "{bad}"
        );
    }
    ed.pitch_info(br#"{"root":0,"scale":"major","pitch":61}"#);
    let r = result_of(&ed);
    assert_eq!(r["result"]["in_key"], false);
    assert_eq!(r["result"]["degree"], Value::Null);
    assert_eq!(r["result"]["nearest"], 60);
    ed.pitch_info(br#"{"root":0,"scale":"major","pitch":67}"#);
    assert_eq!(result_of(&ed)["result"]["degree"], 5);
    ed.pitch_info(br#"{"root":0,"scale":"major","pitch":128}"#);
    assert_eq!(result_of(&ed)["error"]["data"]["code"], "invalid_params");
    assert_eq!(ed.state_hash(), h0);
}

#[test]
fn conform_notes_to_scale_runs_through_the_editor() {
    let mut ed = EditorSession::new("t").unwrap();
    for c in SEQUENCE {
        ed.apply(c.as_bytes());
    }
    ed.apply(br#"{"command":"conform_notes_to_scale","region":"b","notes":null}"#);
    assert_eq!(result_of(&ed)["error"]["data"]["code"], "invalid");
    ed.apply(br#"{"command":"set_key","tick":0,"root":2,"scale":"major"}"#);
    ed.apply(br#"{"command":"add_note","id":"e","region":"b","pitch":61,"velocity":90,"start":1920,"duration":480}"#);
    let before = ed.state_hash();
    ed.apply(br#"{"command":"conform_notes_to_scale","region":"b","notes":null}"#);
    assert_eq!(result_of(&ed)["result"]["changed"], true);
    assert_ne!(ed.state_hash(), before);
    ed.undo();
    assert_eq!(ed.state_hash(), before);
}
