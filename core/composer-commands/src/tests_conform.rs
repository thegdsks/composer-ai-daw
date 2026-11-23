//! conform_notes_to_scale (D23b).
use super::*;
use composer_model::{Id, Project, RegionContent, RegionKind, Scale, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

fn ids(v: &[u128]) -> Vec<Id> {
    v.iter().map(|n| Id(*n)).collect()
}

fn note(n: u128, pitch: u8, start: i64) -> NoteSpec {
    NoteSpec {
        id: id(n),
        pitch,
        velocity: 100,
        start,
        duration: 100,
    }
}

/// A clip (11) at tick 960 holding in key notes 20..22 and strays 23..25 of C major.
fn base() -> Editor {
    let mut ed = Editor::new(Project::new(id(1), "t"));
    for c in [
        Command::AddTrack {
            id: id(10),
            name: "keys".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::AddRegion {
            id: id(11),
            track: id(10),
            start: 960,
            length: 3840,
            kind: RegionKind::Midi,
        },
        Command::AddNotes {
            region: id(11),
            notes: vec![
                note(20, 60, 0),
                note(21, 62, 1000),
                note(22, 64, 2400),
                note(23, 61, 100),
                note(24, 66, 1500),
                note(25, 70, 3000),
            ],
        },
    ] {
        ed.execute(&c).unwrap();
    }
    ed
}

fn pitches(ed: &Editor) -> Vec<u8> {
    let (_, r) = ed.project().region(id(11)).unwrap();
    let RegionContent::Midi { notes } = &r.content else {
        panic!("not midi")
    };
    notes.values().map(|n| n.pitch).collect()
}

fn set_key(ed: &mut Editor, tick: i64, root: u8, scale: Scale) {
    ed.execute(&Command::SetKey { tick, root, scale }).unwrap();
}

fn rejected(ed: &mut Editor, c: Command, code: &str) {
    let h = ed.project().state_hash();
    let e = ed.execute(&c).unwrap_err();
    assert_eq!(e.code(), code, "{c:?}");
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn needs_a_key_and_moves_strays_in_one_undo_step() {
    let mut ed = base();
    let all = Command::ConformNotesToScale {
        region: id(11),
        notes: None,
    };
    rejected(&mut ed, all.clone(), "invalid");
    set_key(&mut ed, 0, 0, Scale::Major);
    let before = ed.project().state_hash();
    let depth = ed.undo_depth();
    ed.execute(&all).unwrap();
    assert_eq!(ed.undo_depth(), depth + 1);
    // 61 goes down to 60, the 66 tie goes down to 65, 70 goes down to 69.
    assert_eq!(pitches(&ed), [60, 62, 64, 60, 65, 69]);
    let after = ed.project().state_hash();
    ed.execute(&all).unwrap();
    assert_eq!(ed.project().state_hash(), after);
    // The second run was a no-op, so one undo returns to the start.
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), before);
    ed.redo().unwrap();
    assert_eq!(ed.project().state_hash(), after);
}

#[test]
fn picks_notes_and_follows_the_key_at_each_start() {
    let mut ed = base();
    // The key starts at tick 2000: note 23 (tick 1060) has no key yet and stays.
    set_key(&mut ed, 2000, 0, Scale::Major);
    ed.execute(&Command::ConformNotesToScale {
        region: id(11),
        notes: Some(ids(&[23, 24])),
    })
    .unwrap();
    assert_eq!(pitches(&ed), [60, 62, 64, 61, 65, 70]);
    // A later key change applies from its own tick onward.
    set_key(&mut ed, 3500, 1, Scale::PentatonicMajor);
    ed.execute(&Command::ConformNotesToScale {
        region: id(11),
        notes: Some(ids(&[25])),
    })
    .unwrap();
    // 70 at tick 3960 is in C# pentatonic major (1 3 5 8 10)? 70 % 12 = 10: yes, unchanged.
    assert_eq!(pitches(&ed)[5], 70);
    for (notes, code) in [
        (Some(ids(&[999])), "not_found"),
        (Some(Vec::new()), "invalid"),
        (Some(ids(&[23, 23])), "invalid"),
    ] {
        let c = Command::ConformNotesToScale {
            region: id(11),
            notes,
        };
        rejected(&mut ed, c, code);
    }
}

#[test]
fn has_metadata_and_a_reason_without_a_key() {
    use crate::{command_meta, commands_enabled, EnabledContext};
    assert!(command_meta("conform_notes_to_scale").is_some());
    let mut ed = base();
    let get = |ed: &Editor| {
        commands_enabled(ed.project(), EnabledContext::default())
            .into_iter()
            .find(|e| e.name == "conform_notes_to_scale")
            .unwrap()
    };
    let e = get(&ed);
    assert!(!e.enabled);
    assert_eq!(e.reason, Some("Set a key first."));
    set_key(&mut ed, 0, 0, Scale::Minor);
    assert!(get(&ed).enabled);
}
