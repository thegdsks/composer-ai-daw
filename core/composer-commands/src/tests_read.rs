//! Tests for the shared read side (D17): selection and the compact tree.
use super::*;
use composer_model::{Id, Project, RegionKind, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

fn editor() -> Editor {
    let mut ed = Editor::new(Project::new(id(1), "tree"));
    for c in [
        Command::AddTrack {
            id: id(10),
            name: "keys".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::AddTrack {
            id: id(11),
            name: "bass".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::AddRegion {
            id: id(20),
            track: id(10),
            start: 3840,
            length: 3840,
            kind: RegionKind::Midi,
        },
        Command::AddRegion {
            id: id(21),
            track: id(10),
            start: 0,
            length: 1920,
            kind: RegionKind::Midi,
        },
        Command::AddNotes {
            region: id(20),
            notes: vec![
                NoteSpec {
                    id: id(30),
                    pitch: 64,
                    velocity: 90,
                    start: 480,
                    duration: 480,
                },
                NoteSpec {
                    id: id(31),
                    pitch: 60,
                    velocity: 90,
                    start: 0,
                    duration: 480,
                },
            ],
        },
        Command::SetTrackMute {
            track: id(11),
            mute: true,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    ed
}

#[test]
fn tree_is_compact_sorted_and_counts_notes() {
    let ed = editor();
    let t = project_tree(ed.project(), TreeOptions::default()).unwrap();
    assert_eq!(
        (t.track_count, t.span_ticks, t.tempo_mbpm),
        (2, 7680, 120_000)
    );
    assert_eq!(t.time_signature, "4/4");
    let keys = &t.tracks[0];
    assert_eq!((keys.region_count, keys.note_count), (2, 2));
    // Regions come in start order, not id order.
    assert_eq!(keys.regions[0].id, id(21));
    assert_eq!(keys.regions[1].pitch_min, Some(60));
    assert_eq!(keys.regions[1].pitch_max, Some(64));
    assert!(keys.regions[1].notes.is_none());
    assert!(t.tracks[1].mix.mute);
    let json = serde_json::to_string(&t).unwrap();
    assert!(!json.contains("\"notes\""), "{json}");
}

#[test]
fn tree_lists_notes_in_time_order_and_filters_by_track() {
    let ed = editor();
    let t = project_tree(
        ed.project(),
        TreeOptions {
            notes: true,
            track: Some(id(10)),
        },
    )
    .unwrap();
    assert_eq!(t.tracks.len(), 1);
    let notes = t.tracks[0].regions[1].notes.as_ref().unwrap();
    assert_eq!(
        notes.iter().map(|n| n.id).collect::<Vec<_>>(),
        [id(31), id(30)]
    );
    let err = project_tree(
        ed.project(),
        TreeOptions {
            notes: false,
            track: Some(id(99)),
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn selection_validates_kinds_and_prunes_stale_ids() {
    let mut ed = editor();
    let sel = Selection {
        tracks: vec![id(10)],
        regions: vec![id(20)],
        notes: vec![NoteRef {
            region: id(20),
            note: id(30),
        }],
    };
    sel.validate(ed.project()).unwrap();
    // A region id is not a track id, and a repeated id is refused.
    let wrong = Selection {
        tracks: vec![id(20)],
        ..Selection::default()
    };
    assert_eq!(
        wrong.validate(ed.project()).unwrap_err().code(),
        "not_found"
    );
    let twice = Selection {
        tracks: vec![id(10), id(10)],
        ..Selection::default()
    };
    assert_eq!(twice.validate(ed.project()).unwrap_err().code(), "invalid");
    let wrong_clip = Selection {
        notes: vec![NoteRef {
            region: id(21),
            note: id(30),
        }],
        ..Selection::default()
    };
    assert!(wrong_clip.validate(ed.project()).is_err());
    ed.execute(&Command::DeleteRegion { region: id(20) })
        .unwrap();
    let pruned = sel.pruned(ed.project());
    assert_eq!(pruned.tracks, vec![id(10)]);
    assert!(pruned.regions.is_empty() && pruned.notes.is_empty());
    assert!(!pruned.is_empty());
}
