//! Audio import, assets and peaks over the daemon methods (D43).

use composerd::{handle_line, ClientHandle, FileConfig, Hub};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

struct Client {
    h: ClientHandle,
    rx: Receiver<String>,
    n: u64,
}

impl Client {
    fn new(hub: &Hub) -> Self {
        let (h, rx) = hub.connect().expect("connect");
        Self { h, rx, n: 0 }
    }

    fn call(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        self.n += 1;
        let line = json!({ "jsonrpc": "2.0", "id": self.n, "method": method, "params": params });
        handle_line(hub, &self.h, &line.to_string());
        loop {
            let v: Value = serde_json::from_str(&self.rx.recv().expect("reply")).unwrap();
            if v["id"] == json!(self.n) {
                return v;
            }
        }
    }

    fn ok(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        let r = self.call(hub, method, params);
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }

    fn code(&mut self, hub: &Hub, method: &str, params: Value) -> String {
        let r = self.call(hub, method, params);
        r["error"]["data"]["code"]
            .as_str()
            .unwrap_or_else(|| panic!("{method} should fail: {r}"))
            .to_string()
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("composerd-audio-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(&d).unwrap()
}

fn wav(frames: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for k in 0..frames {
        let v = (((k % 200) as f32 / 200.0) - 0.5) * 0.8;
        let s = (v * 32767.0) as i16;
        data.extend_from_slice(&s.to_le_bytes());
        data.extend_from_slice(&s.to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&48_000u32.to_le_bytes());
    out.extend_from_slice(&(48_000u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

fn hub_in(d: &std::path::Path) -> Hub {
    Hub::with_files(
        4,
        FileConfig {
            dir: d.to_path_buf(),
            ..FileConfig::default()
        },
    )
}

fn audio_track(c: &mut Client, hub: &Hub) -> String {
    let id = format!("{:032x}", 0x77u32);
    c.ok(
        hub,
        "command.apply",
        json!({ "command": { "command": "add_track", "id": id, "name": "Voice", "kind": "audio", "index": null } }),
    );
    id
}

#[test]
fn import_places_a_clip_in_one_undo_step_and_keeps_the_asset() {
    let d = dir("import");
    std::fs::write(d.join("voice.wav"), wav(96_000)).unwrap();
    let hub = hub_in(&d);
    let mut c = Client::new(&hub);
    let track = audio_track(&mut c, &hub);
    let h0 = c.ok(&hub, "state.hash", json!({}))["state_hash"].clone();
    let undo0 = c.ok(&hub, "state.hash", json!({}));
    let _ = undo0;

    let dry = c.ok(
        &hub,
        "audio.import",
        json!({ "path": "voice.wav", "track": track, "dry_run": true }),
    );
    assert_eq!(dry["frames"], 96_000);
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h0);
    assert!(!d.join("assets").exists(), "a dry run stores nothing");

    let r = c.ok(
        &hub,
        "audio.import",
        json!({ "path": "voice.wav", "track": track, "start": 960 }),
    );
    assert_eq!(r["placed"], true);
    assert_eq!(r["asset"]["frames"], 96_000);
    // 2 seconds at 120 BPM is 3840 ticks.
    assert_eq!(r["length"], 3840);
    assert_eq!(r["start"], 960);
    assert_eq!(r["undo_depth"], 2);
    let hash = r["asset"]["hash"].as_str().unwrap().to_string();
    assert!(d.join("assets").join(format!("{hash}.wav")).is_file());

    let list = c.ok(&hub, "audio.assets", json!({}));
    assert_eq!(list["count"], 1);
    assert_eq!(list["assets"][0]["present"], true);
    assert_eq!(list["assets"][0]["clips"].as_array().unwrap().len(), 1);

    // Importing the same bytes again reuses the asset.
    let again = c.ok(&hub, "audio.import", json!({ "path": "voice.wav" }));
    assert_eq!(again["asset"]["id"], r["asset"]["id"]);
    assert_eq!(again["placed"], false);
    assert_eq!(c.ok(&hub, "audio.assets", json!({}))["count"], 1);

    // Undo removes the clip only.
    c.ok(&hub, "undo", json!({}));
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h0);
    let list = c.ok(&hub, "audio.assets", json!({}));
    assert_eq!(list["count"], 1);
    assert!(list["assets"][0]["clips"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn peaks_cover_the_file_and_validate_ranges() {
    let d = dir("peaks");
    std::fs::write(d.join("a.wav"), wav(96_000)).unwrap();
    let hub = hub_in(&d);
    let mut c = Client::new(&hub);
    let r = c.ok(&hub, "audio.import", json!({ "path": "a.wav" }));
    let hash = r["asset"]["hash"].as_str().unwrap();
    let id = r["asset"]["id"].as_str().unwrap();
    let p = c.ok(&hub, "audio.peaks", json!({ "asset": hash, "bins": 64 }));
    assert_eq!(p["bins"], 64);
    assert_eq!(p["min"].as_array().unwrap().len(), 64);
    assert!(p["max"][0].as_f64().unwrap() > 0.0 && p["min"][0].as_f64().unwrap() < 0.0);
    assert!(d.join("assets").join(format!("{hash}.peaks")).is_file());
    let by_id = c.ok(&hub, "audio.peaks", json!({ "asset": id, "bins": 64 }));
    assert_eq!(by_id, p);
    let part = c.ok(
        &hub,
        "audio.peaks",
        json!({ "asset": hash, "from": 100, "to": 140, "bins": 500 }),
    );
    assert_eq!(part["bins"], 40);
    assert_eq!(part["level_frames"], 0);
    assert_eq!(
        c.code(
            &hub,
            "audio.peaks",
            json!({ "asset": hash, "from": 5, "to": 5 })
        ),
        "invalid_params"
    );
    assert_eq!(
        c.code(&hub, "audio.peaks", json!({ "asset": "0".repeat(64) })),
        "asset_not_found"
    );
    assert_eq!(
        c.code(&hub, "audio.peaks", json!({ "asset": "nope" })),
        "asset_not_found"
    );
    // The file vanishing is reported as missing, not as a crash.
    std::fs::remove_file(d.join("assets").join(format!("{hash}.wav"))).unwrap();
    assert_eq!(
        c.code(&hub, "audio.peaks", json!({ "asset": hash })),
        "asset_missing"
    );
    let list = c.ok(&hub, "audio.assets", json!({}));
    assert_eq!(list["assets"][0]["present"], false);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn paths_follow_the_file_rules_and_bad_audio_is_refused() {
    let d = dir("paths");
    std::fs::write(d.join("junk.wav"), b"not audio at all").unwrap();
    std::fs::write(d.join("note.txt"), b"x").unwrap();
    let hub = hub_in(&d);
    let mut c = Client::new(&hub);
    for bad in [
        "../x.wav",
        "/etc/x.wav",
        "a/../../x.wav",
        "note.txt",
        "x\\y.wav",
    ] {
        assert_eq!(
            c.code(&hub, "audio.import", json!({ "path": bad })),
            "path_invalid",
            "{bad}"
        );
    }
    assert_eq!(
        c.code(&hub, "audio.import", json!({ "path": "gone.wav" })),
        "file_not_found"
    );
    assert_eq!(
        c.code(&hub, "audio.import", json!({ "path": "junk.wav" })),
        "audio_invalid"
    );
    assert_eq!(c.ok(&hub, "audio.assets", json!({}))["count"], 0);
    // A clip may only name an imported asset.
    let track = audio_track(&mut c, &hub);
    let cmd = json!({ "command": "add_audio_clip", "id": format!("{:032x}", 0x88u32), "track": track,
        "start": 0, "length": 960, "asset": "ab".repeat(32) });
    assert_eq!(
        c.code(&hub, "command.apply", json!({ "command": cmd })),
        "asset_not_found"
    );
    assert_eq!(
        c.code(
            &hub,
            "command.group",
            json!({ "label": "x", "commands": [cmd] })
        ),
        "asset_not_found"
    );
    // Tiny file cap.
    let small = Hub::with_files(
        2,
        FileConfig {
            dir: d.to_path_buf(),
            audio: composer_audio_limits(10),
            ..FileConfig::default()
        },
    );
    std::fs::write(d.join("big.wav"), wav(100)).unwrap();
    let mut c2 = Client::new(&small);
    assert_eq!(
        c2.code(&small, "audio.import", json!({ "path": "big.wav" })),
        "file_too_large"
    );
    std::fs::remove_dir_all(&d).unwrap();
}

fn composer_audio_limits(max_file_bytes: u64) -> composer_audio::AudioLimits {
    composer_audio::AudioLimits {
        max_file_bytes,
        ..composer_audio::AudioLimits::default()
    }
}

#[test]
fn clips_render_and_survive_save_and_load() {
    let d = dir("render");
    std::fs::write(d.join("voice.wav"), wav(48_000)).unwrap();
    let hub = hub_in(&d);
    let mut c = Client::new(&hub);
    let track = audio_track(&mut c, &hub);
    c.ok(
        &hub,
        "audio.import",
        json!({ "path": "voice.wav", "track": track }),
    );
    let hash = c.ok(&hub, "state.hash", json!({}))["state_hash"].clone();
    let r = c.ok(
        &hub,
        "render.wav",
        json!({ "path": "out.wav", "tail_seconds": 0.0, "format": "float32" }),
    );
    assert_eq!(r["frames"], 48_000);
    assert_eq!(r["audio_issues"], json!([]));
    assert!(r["audio_resident_high_water"].as_u64().unwrap() > 0);
    let out = std::fs::read(d.join("out.wav")).unwrap();
    assert!(out[44..]
        .as_chunks::<4>()
        .0
        .iter()
        .any(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) != 0.0));

    c.ok(&hub, "project.save", json!({ "path": "song.cmpf" }));
    c.ok(&hub, "session.new", json!({ "name": "other" }));
    assert_eq!(c.ok(&hub, "audio.assets", json!({}))["count"], 0);
    c.ok(&hub, "project.load", json!({ "path": "song.cmpf" }));
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], hash);
    assert_eq!(c.ok(&hub, "audio.assets", json!({}))["count"], 1);
    let again = c.ok(
        &hub,
        "render.wav",
        json!({ "path": "out.wav", "tail_seconds": 0.0, "format": "float32", "overwrite": true }),
    );
    assert_eq!(again["frames"], 48_000);
    std::fs::remove_dir_all(&d).unwrap();
}
