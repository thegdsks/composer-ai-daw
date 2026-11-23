//! Tests for the T17 additions (D14, D15): mix commands, bulk notes, split,
//! duplicate, transpose and quantize.
use super::*;
use composer_model::{Id, Project, RegionContent, RegionKind, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

fn note(n: u128, pitch: u8, start: i64, duration: i64) -> NoteSpec {
    NoteSpec {
        id: id(n),
        pitch,
        velocity: 100,
        start,
        duration,
    }
}

/// One instrument track (10) with a 3840 tick clip (11) holding three notes.
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
                note(20, 60, 0, 480),
                note(21, 62, 1000, 1000),
                note(22, 64, 2400, 480),
            ],
        },
    ] {
        ed.execute(&c).unwrap();
    }
    ed
}

/// (id, pitch, start, duration) per note, in id order.
fn notes_of(ed: &Editor, region: u128) -> Vec<(u128, u8, i64, i64)> {
    let (_, r) = ed.project().region(id(region)).unwrap();
    let RegionContent::Midi { notes } = &r.content else {
        panic!("not midi")
    };
    notes
        .values()
        .map(|n| (n.id.0, n.pitch, n.start, n.duration))
        .collect()
}

fn rejected(ed: &mut Editor, c: Command, code: &str) {
    let h = ed.project().state_hash();
    let e = ed.execute(&c).unwrap_err();
    assert_eq!(e.code(), code, "{c:?}");
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn mix_commands_edit_validate_and_undo() {
    let mut ed = base();
    ed.execute(&Command::SetTrackVolume {
        track: id(10),
        volume: 1500,
    })
    .unwrap();
    ed.execute(&Command::SetTrackPan {
        track: id(10),
        pan: -500,
    })
    .unwrap();
    ed.execute(&Command::SetTrackMute {
        track: id(10),
        mute: true,
    })
    .unwrap();
    ed.execute(&Command::SetTrackSolo {
        track: id(10),
        solo: true,
    })
    .unwrap();
    let m = ed.project().track(id(10)).unwrap().mix;
    assert_eq!((m.volume, m.pan, m.mute, m.solo), (1500, -500, true, true));
    // Setting the same value again is a no-op and adds no undo step.
    let depth = ed.undo_depth();
    ed.execute(&Command::SetTrackMute {
        track: id(10),
        mute: true,
    })
    .unwrap();
    assert_eq!(ed.undo_depth(), depth);
    rejected(
        &mut ed,
        Command::SetTrackVolume {
            track: id(10),
            volume: 2001,
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::SetTrackPan {
            track: id(10),
            pan: 1001,
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::SetTrackMute {
            track: id(99),
            mute: true,
        },
        "not_found",
    );
    for _ in 0..4 {
        assert!(ed.undo().unwrap());
    }
    assert!(ed.project().track(id(10)).unwrap().mix.is_default());
}

#[test]
fn add_notes_is_atomic_and_one_undo_step() {
    let mut ed = base();
    let depth = ed.undo_depth();
    ed.execute(&Command::AddNotes {
        region: id(11),
        notes: vec![note(30, 70, 0, 10), note(31, 71, 10, 10)],
    })
    .unwrap();
    assert_eq!(ed.undo_depth(), depth + 1);
    assert_eq!(notes_of(&ed, 11).len(), 5);
    // One bad note rejects the whole batch.
    rejected(
        &mut ed,
        Command::AddNotes {
            region: id(11),
            notes: vec![note(32, 70, 0, 10), note(33, 70, 3839, 10)],
        },
        "out_of_bounds",
    );
    rejected(
        &mut ed,
        Command::AddNotes {
            region: id(11),
            notes: vec![note(34, 70, 0, 10), note(34, 70, 20, 10)],
        },
        "duplicate_id",
    );
    rejected(
        &mut ed,
        Command::AddNotes {
            region: id(11),
            notes: vec![],
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::AddNotes {
            region: id(11),
            notes: vec![note(20, 70, 0, 10)],
        },
        "duplicate_id",
    );
    ed.undo().unwrap();
    assert_eq!(notes_of(&ed, 11).len(), 3);
}

#[test]
fn split_region_cuts_notes_and_undoes() {
    let mut ed = base();
    let h = ed.project().state_hash();
    // The clip starts at 960, so the cut 1500 in is at 2460: note 21 (1000..2000) crosses it.
    ed.execute(&Command::SplitRegion {
        region: id(11),
        at: 960 + 1500,
        new_region: id(40),
        note_ids: vec![id(41)],
    })
    .unwrap();
    let (_, left) = ed.project().region(id(11)).unwrap();
    let (_, right) = ed.project().region(id(40)).unwrap();
    assert_eq!((left.start, left.length), (960, 1500));
    assert_eq!((right.start, right.length), (2460, 2340));
    assert_eq!(
        notes_of(&ed, 11),
        vec![(20, 60, 0, 480), (21, 62, 1000, 500)]
    );
    assert_eq!(
        notes_of(&ed, 40),
        vec![(22, 64, 900, 480), (41, 62, 0, 500)]
    );
    assert!(ed.project().validate().is_ok());
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), h);
    assert!(ed.redo().unwrap());
    assert!(ed.project().region(id(40)).is_some());
}

#[test]
fn split_region_rejects_bad_cuts_and_id_counts() {
    let mut ed = base();
    for at in [960, 4800, 100, 99_999] {
        rejected(
            &mut ed,
            Command::SplitRegion {
                region: id(11),
                at,
                new_region: id(40),
                note_ids: vec![],
            },
            "invalid",
        );
    }
    // A crossing note without an id, then a spare id nobody needs.
    rejected(
        &mut ed,
        Command::SplitRegion {
            region: id(11),
            at: 2460,
            new_region: id(40),
            note_ids: vec![],
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::SplitRegion {
            region: id(11),
            at: 960 + 700,
            new_region: id(40),
            note_ids: vec![id(41)],
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::SplitRegion {
            region: id(11),
            at: 2460,
            new_region: id(10),
            note_ids: vec![id(41)],
        },
        "duplicate_id",
    );
}

#[test]
fn duplicate_region_copies_notes_with_new_ids() {
    let mut ed = base();
    ed.execute(&Command::DuplicateRegion {
        region: id(11),
        new_region: id(50),
        start: 9600,
        track: None,
        note_ids: vec![id(51), id(52), id(53)],
    })
    .unwrap();
    let (t, r) = ed.project().region(id(50)).unwrap();
    assert_eq!((t.id, r.start, r.length), (id(10), 9600, 3840));
    let shape = |v: Vec<(u128, u8, i64, i64)>| -> Vec<_> {
        v.into_iter().map(|n| (n.1, n.2, n.3)).collect()
    };
    assert_eq!(shape(notes_of(&ed, 50)), shape(notes_of(&ed, 11)));
    assert!(ed.project().validate().is_ok());
    rejected(
        &mut ed,
        Command::DuplicateRegion {
            region: id(11),
            new_region: id(60),
            start: 0,
            track: None,
            note_ids: vec![id(61)],
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::DuplicateRegion {
            region: id(11),
            new_region: id(60),
            start: 0,
            track: Some(id(99)),
            note_ids: vec![id(61), id(62), id(63)],
        },
        "not_found",
    );
    ed.undo().unwrap();
    assert!(ed.project().region(id(50)).is_none());
}

#[test]
fn duplicate_region_to_another_track_and_wrong_kind() {
    let mut ed = base();
    for c in [
        Command::AddTrack {
            id: id(70),
            name: "bass".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::AddTrack {
            id: id(71),
            name: "audio".into(),
            kind: TrackKind::Audio,
            index: None,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    ed.execute(&Command::DuplicateRegion {
        region: id(11),
        new_region: id(80),
        start: 0,
        track: Some(id(70)),
        note_ids: vec![id(81), id(82), id(83)],
    })
    .unwrap();
    assert_eq!(ed.project().region(id(80)).unwrap().0.id, id(70));
    rejected(
        &mut ed,
        Command::DuplicateRegion {
            region: id(11),
            new_region: id(90),
            start: 0,
            track: Some(id(71)),
            note_ids: vec![id(91), id(92), id(93)],
        },
        "wrong_kind",
    );
}

#[test]
fn transpose_notes_all_selected_and_bounds() {
    let mut ed = base();
    ed.execute(&Command::TransposeNotes {
        region: id(11),
        semitones: 12,
        notes: None,
    })
    .unwrap();
    assert_eq!(
        notes_of(&ed, 11).iter().map(|n| n.1).collect::<Vec<_>>(),
        vec![72, 74, 76]
    );
    ed.execute(&Command::TransposeNotes {
        region: id(11),
        semitones: -2,
        notes: Some(vec![id(20)]),
    })
    .unwrap();
    assert_eq!(notes_of(&ed, 11)[0].1, 70);
    rejected(
        &mut ed,
        Command::TransposeNotes {
            region: id(11),
            semitones: 60,
            notes: None,
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::TransposeNotes {
            region: id(11),
            semitones: 1,
            notes: Some(vec![id(999)]),
        },
        "not_found",
    );
    rejected(
        &mut ed,
        Command::TransposeNotes {
            region: id(11),
            semitones: 1,
            notes: Some(vec![id(20), id(20)]),
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::TransposeNotes {
            region: id(11),
            semitones: 200,
            notes: None,
        },
        "invalid",
    );
    // A zero shift changes nothing and adds no undo step.
    let depth = ed.undo_depth();
    ed.execute(&Command::TransposeNotes {
        region: id(11),
        semitones: 0,
        notes: None,
    })
    .unwrap();
    assert_eq!(ed.undo_depth(), depth);
}

#[test]
fn quantize_notes_full_partial_and_clamped() {
    let mut ed = base();
    ed.execute(&Command::QuantizeNotes {
        region: id(11),
        grid: 480,
        strength: 100,
        notes: None,
    })
    .unwrap();
    // 0 stays, 1000 rounds to 960, 2400 stays.
    assert_eq!(
        notes_of(&ed, 11).iter().map(|n| n.2).collect::<Vec<_>>(),
        vec![0, 960, 2400]
    );
    let mut ed = base();
    ed.execute(&Command::QuantizeNotes {
        region: id(11),
        grid: 480,
        strength: 50,
        notes: Some(vec![id(21)]),
    })
    .unwrap();
    assert_eq!(notes_of(&ed, 11)[1].2, 980);
    // A note near the clip end is pulled back inside instead of rejected.
    ed.execute(&Command::AddNotes {
        region: id(11),
        notes: vec![note(23, 50, 3700, 100)],
    })
    .unwrap();
    ed.execute(&Command::QuantizeNotes {
        region: id(11),
        grid: 3840,
        strength: 100,
        notes: Some(vec![id(23)]),
    })
    .unwrap();
    assert_eq!(notes_of(&ed, 11).last().unwrap().2, 3740);
    assert!(ed.project().validate().is_ok());
    rejected(
        &mut ed,
        Command::QuantizeNotes {
            region: id(11),
            grid: 0,
            strength: 100,
            notes: None,
        },
        "invalid",
    );
    rejected(
        &mut ed,
        Command::QuantizeNotes {
            region: id(11),
            grid: 480,
            strength: 0,
            notes: None,
        },
        "invalid",
    );
}

#[test]
fn new_commands_roll_back_inside_a_failed_group() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let r = ed.group("edit", |ed| {
        ed.execute(&Command::TransposeNotes {
            region: id(11),
            semitones: 1,
            notes: None,
        })?;
        ed.execute(&Command::SplitRegion {
            region: id(11),
            at: 2460,
            new_region: id(40),
            note_ids: vec![],
        })
    });
    assert!(r.is_err());
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn audio_regions_split_and_reject_note_commands() {
    let mut ed = base();
    for c in [
        Command::AddTrack {
            id: id(71),
            name: "a".into(),
            kind: TrackKind::Audio,
            index: None,
        },
        Command::AddRegion {
            id: id(72),
            track: id(71),
            start: 0,
            length: 1000,
            kind: RegionKind::Audio,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    ed.execute(&Command::SplitRegion {
        region: id(72),
        at: 400,
        new_region: id(73),
        note_ids: vec![],
    })
    .unwrap();
    assert_eq!(ed.project().region(id(73)).unwrap().1.length, 600);
    rejected(
        &mut ed,
        Command::TransposeNotes {
            region: id(72),
            semitones: 1,
            notes: None,
        },
        "wrong_kind",
    );
}

// D22: bulk note commands.

fn ids(v: &[u128]) -> Vec<Id> {
    v.iter().map(|n| id(*n)).collect()
}

#[test]
fn move_notes_moves_all_and_undoes_in_one_step() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let depth = ed.undo_depth();
    ed.execute(&Command::MoveNotes {
        region: id(11),
        notes: ids(&[20, 22]),
        delta_tick: 100,
        delta_pitch: -2,
    })
    .unwrap();
    assert_eq!(
        notes_of(&ed, 11),
        vec![
            (20, 58, 100, 480),
            (21, 62, 1000, 1000),
            (22, 62, 2500, 480)
        ]
    );
    assert_eq!(ed.undo_depth(), depth + 1);
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), h);
    assert!(ed.redo().unwrap());
    assert_eq!(notes_of(&ed, 11)[0].2, 100);
}

#[test]
fn move_notes_rejects_whole_command() {
    let mut ed = base();
    let mv = |notes: &[u128], t: i64, p: i16| Command::MoveNotes {
        region: id(11),
        notes: ids(notes),
        delta_tick: t,
        delta_pitch: p,
    };
    // Second note would pass the clip end; the first must not move either.
    rejected(&mut ed, mv(&[20, 22], 1000, 0), "out_of_bounds");
    rejected(&mut ed, mv(&[20], -1, 0), "invalid");
    rejected(&mut ed, mv(&[20], 0, -61), "invalid");
    rejected(&mut ed, mv(&[20], 0, 128), "invalid");
    rejected(&mut ed, mv(&[], 0, 0), "invalid");
    rejected(&mut ed, mv(&[20, 20], 0, 0), "invalid");
    rejected(&mut ed, mv(&[99], 0, 0), "not_found");
    rejected(
        &mut ed,
        Command::MoveNotes {
            region: id(98),
            notes: ids(&[20]),
            delta_tick: 0,
            delta_pitch: 0,
        },
        "not_found",
    );
}

#[test]
fn resize_notes_changes_durations_and_rejects_bad_ones() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let rs = |notes: &[u128], d: i64| Command::ResizeNotes {
        region: id(11),
        notes: ids(notes),
        delta: d,
    };
    ed.execute(&rs(&[20, 21], 20)).unwrap();
    assert_eq!(notes_of(&ed, 11)[0].3, 500);
    assert_eq!(notes_of(&ed, 11)[1].3, 1020);
    rejected(&mut ed, rs(&[20], -500), "invalid");
    rejected(&mut ed, rs(&[22], 2000), "out_of_bounds");
    rejected(&mut ed, rs(&[], 1), "invalid");
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn set_notes_velocity_value_and_ramp() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let vel = |ed: &Editor| -> Vec<u8> {
        let (_, r) = ed.project().region(id(11)).unwrap();
        let RegionContent::Midi { notes } = &r.content else {
            panic!()
        };
        notes.values().map(|n| n.velocity).collect()
    };
    let sv = |value: Option<u8>, ramp: Option<VelocityRamp>| Command::SetNotesVelocity {
        region: id(11),
        notes: ids(&[20, 21, 22]),
        value,
        ramp,
    };
    ed.execute(&sv(Some(40), None)).unwrap();
    assert_eq!(vel(&ed), vec![40, 40, 40]);
    // Starts are 0, 1000, 2400: ramp 20..120 gives 20, 61, 120.
    ed.execute(&sv(None, Some(VelocityRamp { from: 20, to: 120 })))
        .unwrap();
    assert_eq!(vel(&ed), vec![20, 61, 120]);
    ed.execute(&Command::SetNotesVelocity {
        region: id(11),
        notes: ids(&[21]),
        value: None,
        ramp: Some(VelocityRamp { from: 90, to: 10 }),
    })
    .unwrap();
    assert_eq!(vel(&ed)[1], 90);
    rejected(&mut ed, sv(None, None), "invalid");
    rejected(
        &mut ed,
        sv(Some(5), Some(VelocityRamp { from: 1, to: 2 })),
        "invalid",
    );
    rejected(&mut ed, sv(Some(0), None), "invalid");
    rejected(&mut ed, sv(Some(128), None), "invalid");
    rejected(
        &mut ed,
        sv(None, Some(VelocityRamp { from: 1, to: 128 })),
        "invalid",
    );
    for _ in 0..3 {
        assert!(ed.undo().unwrap());
    }
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn delete_notes_removes_listed_and_undoes() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let dn = |notes: &[u128]| Command::DeleteNotes {
        region: id(11),
        notes: ids(notes),
    };
    ed.execute(&dn(&[20, 22])).unwrap();
    assert_eq!(notes_of(&ed, 11).len(), 1);
    assert!(ed.project().index_is_consistent());
    rejected(&mut ed, dn(&[20]), "not_found");
    rejected(&mut ed, dn(&[]), "invalid");
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), h);
    assert!(ed.project().index_is_consistent());
}

#[test]
fn bulk_note_commands_reject_audio_clips_and_oversized_lists() {
    let mut ed = base();
    for c in [
        Command::AddTrack {
            id: id(71),
            name: "a".into(),
            kind: TrackKind::Audio,
            index: None,
        },
        Command::AddRegion {
            id: id(72),
            track: id(71),
            start: 0,
            length: 1000,
            kind: RegionKind::Audio,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    rejected(
        &mut ed,
        Command::DeleteNotes {
            region: id(72),
            notes: ids(&[20]),
        },
        "wrong_kind",
    );
    rejected(
        &mut ed,
        Command::DeleteNotes {
            region: id(11),
            notes: (0..=MAX_BULK_NOTES as u128).map(id).collect(),
        },
        "invalid",
    );
}

#[test]
fn bulk_note_commands_roll_back_inside_a_failed_group() {
    let mut ed = base();
    let h = ed.project().state_hash();
    let r = ed.group("edit", |ed| {
        ed.execute(&Command::MoveNotes {
            region: id(11),
            notes: ids(&[20]),
            delta_tick: 5,
            delta_pitch: 1,
        })?;
        ed.execute(&Command::DeleteNotes {
            region: id(11),
            notes: ids(&[21]),
        })?;
        ed.execute(&Command::DeleteNotes {
            region: id(11),
            notes: ids(&[21]),
        })
    });
    assert!(r.is_err());
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn bulk_note_commands_survive_json_and_have_metadata() {
    let c = Command::SetNotesVelocity {
        region: id(11),
        notes: ids(&[20]),
        value: None,
        ramp: Some(VelocityRamp { from: 10, to: 100 }),
    };
    let s = serde_json::to_string(&c).unwrap();
    assert!(s.contains("\"command\":\"set_notes_velocity\""), "{s}");
    assert_eq!(serde_json::from_str::<Command>(&s).unwrap(), c);
    for n in [
        "move_notes",
        "resize_notes",
        "set_notes_velocity",
        "delete_notes",
    ] {
        assert!(describe().iter().any(|d| d.name == n), "{n}");
    }
}

/// Random bulk commands: every outcome is either rejected with no effect or
/// undoable back to the start hash.
#[test]
fn bulk_note_fuzz_undoes_to_initial_hash() {
    let mut s: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move |n: u64| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s % n
    };
    let mut ed = base();
    let h0 = ed.project().state_hash();
    let d0 = ed.undo_depth();
    let (mut ok, mut bad) = (0, 0);
    for fresh in 0..600u128 {
        let pool = [20u128, 21, 22, 100 + fresh.saturating_sub(1)];
        let mut notes: Vec<Id> = pool
            .iter()
            .filter(|_| next(2) == 0)
            .map(|n| id(*n))
            .collect();
        if next(12) == 0 {
            notes.push(id(99));
        }
        if notes.is_empty() {
            notes.push(id(20 + next(3) as u128));
        }
        let region = id(11);
        let c = match next(4) {
            0 => Command::MoveNotes {
                region,
                notes,
                delta_tick: next(300) as i64 - 30,
                delta_pitch: next(7) as i16 - 3,
            },
            1 => Command::ResizeNotes {
                region,
                notes,
                delta: next(300) as i64 - 100,
            },
            2 => Command::SetNotesVelocity {
                region,
                notes,
                value: (next(2) == 0).then(|| next(129) as u8),
                ramp: (next(2) == 0).then(|| VelocityRamp {
                    from: next(128) as u8,
                    to: next(128) as u8,
                }),
            },
            _ if next(3) == 0 => Command::DeleteNotes { region, notes },
            _ => Command::AddNotes {
                region,
                notes: vec![note(100 + fresh, 70, next(3000) as i64, 100)],
            },
        };
        let before = ed.project().state_hash();
        match ed.execute(&c) {
            Ok(()) => ok += 1,
            Err(_) => {
                bad += 1;
                assert_eq!(ed.project().state_hash(), before, "{c:?}");
            }
        }
        assert!(ed.project().index_is_consistent());
        assert_eq!(ed.project().validate(), Ok(()), "{c:?}");
    }
    assert!(ok > 50 && bad > 50, "ok {ok} bad {bad}");
    while ed.undo_depth() > d0 {
        assert!(ed.undo().unwrap());
    }
    assert_eq!(ed.project().state_hash(), h0);
}

/// 10000 notes moved by one command stay within the per command budget times
/// the note count (the budget comes from the env, unset means not enforced).
#[test]
fn move_ten_thousand_notes_within_budget() {
    const N: u128 = 10_000;
    let mut ed = Editor::new(Project::new(id(1), "big"));
    ed.execute(&Command::AddTrack {
        id: id(10),
        name: "k".into(),
        kind: TrackKind::Instrument,
        index: None,
    })
    .unwrap();
    ed.execute(&Command::AddRegion {
        id: id(11),
        track: id(10),
        start: 0,
        length: 4_000_000,
        kind: RegionKind::Midi,
    })
    .unwrap();
    let specs: Vec<NoteSpec> = (0..N)
        .map(|i| note(1000 + i, 40 + (i % 40) as u8, i as i64 * 100, 50))
        .collect();
    ed.execute(&Command::AddNotes {
        region: id(11),
        notes: specs,
    })
    .unwrap();
    let c = Command::MoveNotes {
        region: id(11),
        notes: (0..N).map(|i| id(1000 + i)).collect(),
        delta_tick: 10,
        delta_pitch: 3,
    };
    let t = std::time::Instant::now();
    ed.execute(&c).unwrap();
    let ns = t.elapsed().as_nanos() as f64;
    if let Some(limit) = std::env::var("COMPOSER_BENCH_MAX_APPLY_NS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        assert!(ns <= limit * N as f64, "{ns} ns over {limit} * {N}");
    }
    assert!(ed.undo().unwrap());
}
