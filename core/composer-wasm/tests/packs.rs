//! Sampled sounds from the catalogue (D37): every entry with status `wired` loads through the
//! editor session and renders non silent audio through the browser engine path.
//! Packs are fetched by sounds/fetch.sh and are not in git. A missing file skips the check
//! unless COMPOSER_REQUIRE_PACKS is set (CI that fetched them sets it).

use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_wasm::{compile_project, compile_project_with_sounds, EditorSession, Session};
use std::collections::BTreeMap;
use std::path::PathBuf;

const SR: u32 = 48_000;

fn project(sound: &str) -> Project {
    let mut p = Project::new(Id(1), "packs");
    let notes: BTreeMap<Id, Note> = [
        (0i64, 40u8, 100u8),
        (240, 52, 90),
        (480, 64, 110),
        (720, 76, 100),
    ]
    .iter()
    .enumerate()
    .map(|(n, (start, pitch, velocity))| {
        let id = Id(100 + n as u128);
        (
            id,
            Note {
                id,
                pitch: *pitch,
                velocity: *velocity,
                start: *start,
                duration: 480,
            },
        )
    })
    .collect();
    let region = Region {
        loop_length: 0,
        id: Id(50),
        start: 0,
        length: 1920,
        content: RegionContent::Midi { notes },
    };
    p.tracks.push(Track {
        id: Id(10),
        name: "t".into(),
        kind: TrackKind::Instrument,
        instrument: None,
        mix: Default::default(),
        sound: Some(sound.to_string()),
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    });
    p
}

fn render(blob: &[u8]) -> Vec<f32> {
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
    out
}

fn packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sounds/packs")
}

#[test]
fn every_wired_pack_loads_and_renders_audio() {
    let text = include_str!("../../../sounds/catalogue.yaml");
    let cat = composer_catalogue::parse(text).unwrap();
    let wired: Vec<_> = cat
        .entries
        .iter()
        .filter(|e| e.status == "wired" && e.kind == "soundfont")
        .collect();
    assert!(
        wired.len() >= 5,
        "expected the five fetched packs to be wired"
    );
    let require = std::env::var("COMPOSER_REQUIRE_PACKS").is_ok();
    let mut checked = 0;
    for e in wired {
        let file = packs_dir().join(e.file.as_deref().expect("wired entry has a file"));
        let Ok(bytes) = std::fs::read(&file) else {
            assert!(!require, "{} missing, run sounds/fetch.sh", file.display());
            eprintln!("skip {}: not fetched", e.id);
            continue;
        };
        let mut s = EditorSession::new("packs").unwrap();
        s.load(&serde_json::to_vec(&project(&e.id)).unwrap());
        s.load_sound_bank(&e.id, &bytes)
            .unwrap_or_else(|c| panic!("{} refused: {c}", e.id));
        assert!(s.engine_blob(SR));
        let blob = s.result().to_vec();
        // A bank means blob v3 or later, so the plain compile is not what was produced.
        assert_ne!(blob, compile_project(&project(&e.id), SR, 0.25), "{}", e.id);
        let audio = render(&blob);
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(audio.iter().all(|x| x.is_finite()), "{}", e.id);
        assert!(peak > 0.01, "{} silent, peak {peak}", e.id);
        // Levels are reported, not capped: synth brass 2 is hot (about 1.5 with four notes).
        eprintln!("{} peak {peak:.3}", e.id);
        checked += 1;
    }
    eprintln!("rendered {checked} wired packs");
}

#[test]
fn a_sound_without_a_loaded_bank_compiles_like_before() {
    let p = project("freepats-ukulele");
    let none = BTreeMap::new();
    assert_eq!(
        compile_project_with_sounds(&p, SR, 0.25, &none),
        compile_project(&p, SR, 0.25)
    );
}

#[test]
fn bad_ids_and_bad_files_are_refused() {
    let mut s = EditorSession::new("packs").unwrap();
    assert_eq!(s.load_sound_bank("Bad Id", b"x"), Err("sound.bad_id"));
    assert_eq!(s.load_sound_bank("", b"x"), Err("sound.bad_id"));
    assert!(s.load_sound_bank("fine-id", b"not a soundfont").is_err());
    assert!(s.load_sound_bank("fine-id", &[]).is_err());
}
