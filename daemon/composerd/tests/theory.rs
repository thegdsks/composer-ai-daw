//! D35: theory reads are pure calls into composer-theory and must equal the wasm editor.

use composer_wasm::EditorSession;
use composerd::{serve_stdio, Hub};
use serde_json::{json, Value};
use std::io::Cursor;

const SCALES: [&str; 7] = [
    "major",
    "minor",
    "dorian",
    "mixolydian",
    "pentatonic_major",
    "pentatonic_minor",
    "harmonic_minor",
];

fn run(calls: &[(&str, Value)]) -> Vec<Value> {
    let hub = Hub::new(4);
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

fn wasm(f: fn(&mut EditorSession, &[u8]), params: &Value) -> Value {
    let mut s = EditorSession::new("t").unwrap();
    f(&mut s, params.to_string().as_bytes());
    serde_json::from_slice(s.result()).unwrap()
}

fn code(v: &Value) -> &str {
    v["error"]["data"]["code"].as_str().expect("error code")
}

#[test]
fn theory_methods_equal_the_wasm_editor() {
    let mut calls: Vec<(&str, Value)> = Vec::new();
    let mut want: Vec<Value> = Vec::new();
    for scale in SCALES {
        for root in [0u8, 5, 11, 12] {
            let p = json!({ "root": root, "scale": scale });
            want.push(wasm(EditorSession::scale_info, &p));
            calls.push(("theory.scale", p));
            for octave in [None, Some(3), Some(9)] {
                let mut p = json!({ "root": root, "scale": scale });
                if let Some(o) = octave {
                    p["octave"] = json!(o);
                }
                want.push(wasm(EditorSession::chords_info, &p));
                calls.push(("theory.chords", p));
            }
            for pitch in [0, 61, 127, 128] {
                let p = json!({ "root": root, "scale": scale, "pitch": pitch });
                want.push(wasm(EditorSession::pitch_info, &p));
                calls.push(("theory.pitch", p));
            }
        }
    }
    let out = run(&calls);
    let mut errors = 0;
    for ((reply, (m, p)), w) in out.iter().zip(&calls).zip(&want) {
        match w.get("result") {
            Some(r) => assert_eq!(&reply["result"], r, "{m} {p}"),
            None => {
                errors += 1;
                assert_eq!(code(reply), "invalid_params", "{m} {p}");
            }
        }
    }
    assert!(errors > 0 && errors < calls.len());
}

#[test]
fn theory_accepts_nested_key_and_rejects_bad_input() {
    let key = json!({ "root": 0, "scale": "major" });
    let out = run(&[
        ("theory.chords", json!({ "key": key })),
        ("theory.chords", json!({ "root": 0, "scale": "major" })),
        (
            "theory.chords",
            json!({ "key": key, "root": 0, "scale": "major" }),
        ),
        ("theory.chords", json!({})),
        ("theory.pitch", json!({ "key": key, "pitch": 60 })),
        ("theory.scale", json!({ "root": 0, "scale": "locrian" })),
        (
            "theory.scale",
            json!({ "root": 0, "scale": "major", "x": 1 }),
        ),
        ("state.hash", json!({})),
        ("theory.scale", json!({ "root": 0, "scale": "major" })),
        ("state.hash", json!({})),
    ]);
    assert_eq!(out[0]["result"], out[1]["result"]);
    assert_eq!(code(&out[2]), "invalid_params");
    assert_eq!(code(&out[3]), "invalid_params");
    assert_eq!(out[4]["result"]["in_key"], true);
    assert_eq!(code(&out[5]), "invalid_params");
    assert_eq!(code(&out[6]), "invalid_params");
    assert_eq!(out[7]["result"], out[9]["result"]);
}

#[test]
fn every_scale_name_is_accepted() {
    let calls: Vec<(&str, Value)> = composer_theory::Scale::ALL
        .iter()
        .map(|s| ("theory.scale", json!({ "root": 0, "scale": s.name() })))
        .collect();
    assert_eq!(calls.len(), SCALES.len());
    for (r, s) in run(&calls).iter().zip(SCALES) {
        assert_eq!(r["result"]["scale"], s);
    }
}
