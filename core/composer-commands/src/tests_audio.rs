//! Audio clip commands (D40).
use super::*;
use composer_model::{Id, Project, RegionContent, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn base() -> Editor {
    let mut ed = Editor::new(Project::new(id(1), "t"));
    ed.execute(&Command::AddTrack {
        id: id(10),
        name: "voice".into(),
        kind: TrackKind::Audio,
        index: None,
    })
    .unwrap();
    ed
}

fn add(ed: &mut Editor, asset: &str) -> Result<(), CommandError> {
    ed.execute(&Command::AddAudioClip {
        id: id(11),
        track: id(10),
        start: 960,
        length: 7680,
        asset: asset.into(),
        offset: None,
        gain: None,
    })
}

fn clip(ed: &Editor) -> (String, u64, u16) {
    match &ed.project().region(id(11)).unwrap().1.content {
        RegionContent::Clip {
            asset,
            offset,
            gain,
        } => (asset.clone(), *offset, *gain),
        other => panic!("not a clip: {other:?}"),
    }
}

#[test]
fn add_clip_is_one_undo_step_with_exact_inverse() {
    let mut ed = base();
    let before = ed.project().state_hash();
    add(&mut ed, HASH).unwrap();
    assert_eq!(clip(&ed), (HASH.into(), 0, 1000));
    assert_ne!(ed.project().state_hash(), before);
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), before);
    assert!(ed.redo().unwrap());
    assert_eq!(clip(&ed).0, HASH);
}

#[test]
fn gain_and_offset_edits_undo_and_validate() {
    let mut ed = base();
    add(&mut ed, HASH).unwrap();
    let h = ed.project().state_hash();
    ed.execute(&Command::SetClipGain {
        region: id(11),
        gain: 500,
    })
    .unwrap();
    ed.execute(&Command::SetClipOffset {
        region: id(11),
        offset: 48_000,
    })
    .unwrap();
    assert_eq!(clip(&ed), (HASH.into(), 48_000, 500));
    assert!(ed
        .execute(&Command::SetClipGain {
            region: id(11),
            gain: 2001
        })
        .is_err());
    assert!(ed
        .execute(&Command::SetClipOffset {
            region: id(11),
            offset: u64::MAX
        })
        .is_err());
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h);
    ed.project().validate().unwrap();
}

#[test]
fn rejects_bad_hash_wrong_track_and_split() {
    let mut ed = base();
    for bad in ["", "abc", &HASH.to_uppercase(), &"g".repeat(64)] {
        assert!(add(&mut ed, bad).is_err(), "{bad:?}");
    }
    ed.execute(&Command::AddTrack {
        id: id(20),
        name: "keys".into(),
        kind: TrackKind::Instrument,
        index: None,
    })
    .unwrap();
    let e = ed
        .execute(&Command::AddAudioClip {
            id: id(12),
            track: id(20),
            start: 0,
            length: 10,
            asset: HASH.into(),
            offset: None,
            gain: None,
        })
        .unwrap_err();
    assert_eq!(e.code(), "wrong_kind");
    add(&mut ed, HASH).unwrap();
    let e = ed
        .execute(&Command::SplitRegion {
            region: id(11),
            at: 2000,
            new_region: id(13),
            note_ids: vec![],
        })
        .unwrap_err();
    assert_eq!(e.code(), "wrong_kind");
}

#[test]
fn duplicate_and_move_keep_the_source() {
    let mut ed = base();
    add(&mut ed, HASH).unwrap();
    ed.execute(&Command::DuplicateRegion {
        region: id(11),
        new_region: id(14),
        start: 20_000,
        track: None,
        note_ids: vec![],
    })
    .unwrap();
    ed.execute(&Command::MoveRegion {
        region: id(14),
        start: 30_000,
    })
    .unwrap();
    let (_, r) = ed.project().region(id(14)).unwrap();
    assert!(matches!(&r.content, RegionContent::Clip { asset, .. } if asset == HASH));
    ed.project().validate().unwrap();
}

#[test]
fn json_round_trip_and_optional_fields() {
    let c = Command::AddAudioClip {
        id: id(11),
        track: id(10),
        start: 0,
        length: 5,
        asset: HASH.into(),
        offset: Some(7),
        gain: Some(900),
    };
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c);
    let short = format!(
        r#"{{"command":"add_audio_clip","id":"{}","track":"{}","start":0,"length":5,"asset":"{HASH}"}}"#,
        id(11),
        id(10)
    );
    assert!(serde_json::from_str::<Command>(&short).is_ok());
}
