#![allow(dead_code)]

use composer_commands::{Command, Editor};
use composer_model::{Id, Project, RegionKind, Scale, TrackKind};
use serde_json::Value;
use std::path::PathBuf;

pub const CHORDS_TRACK: Id = Id(0x100);
pub const CHORDS_REGION: Id = Id(0x101);
pub const MELODY_TRACK: Id = Id(0x200);
pub const MELODY_REGION: Id = Id(0x201);
const BAR: i64 = 3840;

/// A small project in C major: a four bar chord clip (C, Am, F, G) and a melody clip.
pub fn fixture() -> Project {
    let mut ed = Editor::new(Project::new(Id(1), "Fixture"));
    let mut run = |c: Command| ed.execute(&c).expect("fixture command");
    run(Command::SetKey {
        tick: 0,
        root: 0,
        scale: Scale::Major,
    });
    run(Command::AddTrack {
        id: CHORDS_TRACK,
        name: "Chords".into(),
        kind: TrackKind::Instrument,
        index: None,
    });
    run(Command::AddRegion {
        id: CHORDS_REGION,
        track: CHORDS_TRACK,
        start: 0,
        length: 4 * BAR,
        kind: RegionKind::Midi,
    });
    let chords: [[u8; 3]; 4] = [[60, 64, 67], [57, 60, 64], [53, 57, 60], [55, 59, 62]];
    let mut n = 0x1000u128;
    for (bar, chord) in chords.iter().enumerate() {
        for pitch in chord {
            n += 1;
            run(Command::AddNote {
                id: Id(n),
                region: CHORDS_REGION,
                pitch: *pitch,
                velocity: 96,
                start: bar as i64 * BAR,
                duration: BAR,
            });
        }
    }
    run(Command::AddTrack {
        id: MELODY_TRACK,
        name: "Melody".into(),
        kind: TrackKind::Instrument,
        index: None,
    });
    run(Command::AddRegion {
        id: MELODY_REGION,
        track: MELODY_TRACK,
        start: 0,
        length: 4 * BAR,
        kind: RegionKind::Midi,
    });
    let melody: [(u8, i64); 8] = [
        (72, 0),
        (74, 1),
        (76, 2),
        (72, 3),
        (69, 4),
        (72, 5),
        (74, 6),
        (71, 7),
    ];
    for (pitch, beat) in melody {
        n += 1;
        run(Command::AddNote {
            id: Id(n),
            region: MELODY_REGION,
            pitch,
            velocity: 110,
            start: beat * 1920,
            duration: 1920,
        });
    }
    ed.into_project()
}

pub fn golden(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(value).expect("json") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, text).expect("write golden");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {name}, run with UPDATE_GOLDEN=1"));
    assert!(want == text, "golden {name} differs");
}
