use crate::import::rescale;
use crate::*;
use composer_commands::{Command, Editor};
use composer_model::{
    Id, IdGen, Note, Project, Region, RegionContent, RegionKind, Track, TrackKind,
};
use std::collections::BTreeMap;

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = id.to_vec();
    v.extend_from_slice(&(body.len() as u32).to_be_bytes());
    v.extend_from_slice(body);
    v
}

fn smf(format: u16, ppq: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&format.to_be_bytes());
    head.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    head.extend_from_slice(&ppq.to_be_bytes());
    let mut out = chunk(b"MThd", &head);
    for t in tracks {
        out.extend(chunk(b"MTrk", t));
    }
    out
}

/// Applies a plan as one group on a fresh project.
fn apply(plan: &ImportPlan) -> Editor {
    let mut ed = Editor::new(Project::new(Id(1), ""));
    run(&mut ed, plan).unwrap();
    ed
}

fn run(ed: &mut Editor, plan: &ImportPlan) -> Result<(), composer_commands::CommandError> {
    ed.group("import midi", |e| {
        for c in &plan.commands {
            e.execute(c)?;
        }
        Ok(())
    })
}

fn import(bytes: &[u8]) -> Result<ImportPlan, MidiError> {
    import_smf(bytes, &ImportOptions::default())
}

/// Canonical form: ids renumbered, one region per track spanning its notes,
/// audio tracks and instrument slots dropped, notes sorted by position.
fn normalize(p: &Project) -> Project {
    let mut out = Project::new(Id(1), p.name.clone());
    out.tempo_map = p.tempo_map.clone();
    let mut next = 2u128;
    let mut id = || {
        next += 1;
        Id(next)
    };
    for t in p.tracks.iter().filter(|t| t.kind == TrackKind::Instrument) {
        let mut abs: Vec<(i64, u8, i64, u8)> = Vec::new();
        for r in t.regions.values() {
            if let RegionContent::Midi { notes } = &r.content {
                abs.extend(
                    notes
                        .values()
                        .map(|n| (r.start + n.start, n.pitch, n.duration, n.velocity)),
                );
            }
        }
        abs.sort();
        let track_id = id();
        let mut regions = BTreeMap::new();
        if let Some(first) = abs.first().map(|n| n.0) {
            let last = abs.iter().map(|n| n.0 + n.2).max().unwrap();
            let region_id = id();
            let notes = abs
                .iter()
                .map(|&(start, pitch, duration, velocity)| {
                    let nid = id();
                    (
                        nid,
                        Note {
                            id: nid,
                            pitch,
                            velocity,
                            start: start - first,
                            duration,
                        },
                    )
                })
                .collect();
            regions.insert(
                region_id,
                Region {
                    loop_length: 0,
                    id: region_id,
                    start: first,
                    length: last - first,
                    content: RegionContent::Midi { notes },
                },
            );
        }
        out.tracks.push(Track {
            id: track_id,
            name: t.name.clone(),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions,
        });
    }
    out
}

fn sample_project() -> Project {
    let mut ed = Editor::new(Project::new(Id(1), "round trip"));
    let mut ids = IdGen::new(7);
    let mut go = |c: Command| ed.execute(&c).unwrap();
    go(Command::SetTempo {
        tick: 0,
        mbpm: 90_000,
    });
    go(Command::SetTempo {
        tick: 3840,
        mbpm: 140_000,
    });
    go(Command::SetTimeSignature {
        tick: 7680,
        numerator: 6,
        denominator: 8,
    });
    let mut tracks = Vec::new();
    for (name, kind) in [
        ("lead", TrackKind::Instrument),
        ("tape", TrackKind::Audio),
        ("bass", TrackKind::Instrument),
        ("empty", TrackKind::Instrument),
    ] {
        let id = ids.next_id();
        go(Command::AddTrack {
            id,
            name: name.into(),
            kind,
            index: None,
        });
        tracks.push(id);
    }
    // Two regions on lead, plus a same pitch overlap that is not nested.
    for (start, notes) in [
        (960, vec![(60u8, 100u8, 0i64, 480i64), (64, 90, 240, 480)]),
        (
            7680,
            vec![(60, 80, 0, 100), (60, 70, 50, 100), (72, 127, 960, 1)],
        ),
    ] {
        let region = ids.next_id();
        go(Command::AddRegion {
            id: region,
            track: tracks[0],
            start,
            length: 4000,
            kind: RegionKind::Midi,
        });
        for (pitch, velocity, s, d) in notes {
            go(Command::AddNote {
                id: ids.next_id(),
                region,
                pitch,
                velocity,
                start: s,
                duration: d,
            });
        }
    }
    let region = ids.next_id();
    go(Command::AddRegion {
        id: region,
        track: tracks[2],
        start: 0,
        length: 3840,
        kind: RegionKind::Midi,
    });
    go(Command::AddNote {
        id: ids.next_id(),
        region,
        pitch: 36,
        velocity: 64,
        start: 0,
        duration: 3840,
    });
    ed.into_project()
}

#[test]
fn round_trip_keeps_canonical_state_hash() {
    let original = sample_project();
    let bytes = export_smf(&original);
    let plan = import(&bytes).unwrap();
    let back = apply(&plan).into_project();
    assert_eq!(back.tracks.len(), 3, "audio track is not exported");
    assert_eq!(
        normalize(&original).state_hash(),
        normalize(&back).state_hash()
    );
    assert_eq!(plan.stats.unmatched_note_offs, 0);
    assert_eq!(plan.stats.unterminated_notes, 0);
}

#[test]
fn export_uses_distinct_channels_and_skips_drums() {
    let mut p = Project::new(Id(1), "");
    for i in 0..12u128 {
        p.tracks.push(Track {
            id: Id(i + 10),
            name: format!("t{i}"),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions: BTreeMap::from([(
                Id(i + 100),
                Region {
                    loop_length: 0,
                    id: Id(i + 100),
                    start: 0,
                    length: 10,
                    content: RegionContent::Midi {
                        notes: BTreeMap::from([(
                            Id(i + 200),
                            Note {
                                id: Id(i + 200),
                                pitch: 60,
                                velocity: 64,
                                start: 0,
                                duration: 10,
                            },
                        )]),
                    },
                },
            )]),
        });
    }
    let plan = import(&export_smf(&p)).unwrap();
    assert_eq!(plan.commands.len(), 12 * 3 + 2);
    let bytes = export_smf(&p);
    // A note on at channel 9 (status 0x99) must never appear.
    assert!(!bytes.windows(2).any(|w| w[0] == 0x99 && w[1] == 60));
    assert!(bytes.windows(2).any(|w| w[0] == 0x9C && w[1] == 60));
}

#[test]
fn hand_built_file_with_running_status_and_tempo_change() {
    let conductor = vec![
        0x00, 0xFF, 0x03, 0x04, b'c', b'o', b'n', b'd', //
        0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, //
        0x00, 0xFF, 0x58, 0x04, 0x03, 0x02, 0x18, 0x08, //
        0x83, 0x60, 0xFF, 0x51, 0x03, 0x06, 0x1A, 0x80, //
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let lead = vec![
        0x00, 0xFF, 0x03, 0x04, b'l', b'e', b'a', b'd', //
        0x00, 0x90, 0x3C, 0x64, // note on 60
        0x83, 0x60, 0x3E, 0x50, // running status, note on 62
        0x83, 0x60, 0x3C, 0x00, // running status, velocity 0 is off for 60
        0x00, 0xFF, 0x7F, 0x03, 0xAA, 0xBB, 0xCC, // unknown meta
        0x00, 0xF0, 0x03, 0x01, 0x02, 0xF7, // sysex
        0x83, 0x60, 0x80, 0x3E, 0x40, // explicit off for 62
        0x00, 0xFF, 0x2F, 0x00,
    ];
    let plan = import(&smf(1, 480, &[conductor, lead])).unwrap();
    let p = apply(&plan).into_project();
    assert_eq!(p.name, "cond");
    assert_eq!(
        p.tempo_map.tempos,
        BTreeMap::from([(0, 120_000), (960, 150_000)])
    );
    let sig = p.tempo_map.meters[&0];
    assert_eq!((sig.numerator, sig.denominator), (3, 4));
    assert_eq!(p.tracks.len(), 1);
    let t = &p.tracks[0];
    assert_eq!(t.name, "lead");
    let r = t.regions.values().next().unwrap();
    assert_eq!((r.start, r.length), (0, 2880));
    let RegionContent::Midi { notes } = &r.content else {
        panic!("midi region expected");
    };
    let mut got: Vec<_> = notes
        .values()
        .map(|n| (n.pitch, n.velocity, n.start, n.duration))
        .collect();
    got.sort();
    assert_eq!(got, vec![(60, 100, 0, 1920), (62, 80, 960, 1920)]);
}

#[test]
fn rescale_rounds_half_up_exactly() {
    assert_eq!(rescale(1, 96).unwrap(), 10);
    assert_eq!(rescale(480, 480).unwrap(), 960);
    assert_eq!(rescale(1, 1920).unwrap(), 1); // 0.5 rounds up
    assert_eq!(rescale(1, 100).unwrap(), 10); // 9.6
    assert_eq!(rescale(3, 7).unwrap(), 411); // 411.43
    assert_eq!(rescale(0, 1).unwrap(), 0);
    assert_eq!(rescale(u64::MAX, 1), Err(MidiError::TickOverflow));
}

#[test]
fn ppq_96_and_480_rescale_in_files() {
    for (ppq, unit) in [(96u16, 10i64), (480, 2)] {
        // One quarter note (ppq ticks) after an eighth rest.
        let half = u32::from(ppq / 2);
        let mut track = vec![0x00, 0xFF, 0x03, 0x01, b'x'];
        vlq(&mut track, half);
        track.extend_from_slice(&[0x90, 0x40, 0x60]);
        vlq(&mut track, u32::from(ppq));
        track.extend_from_slice(&[0x90, 0x40, 0x00]);
        let plan = import(&smf(0, ppq, &[track])).unwrap();
        let p = apply(&plan).into_project();
        let r = p.tracks[0].regions.values().next().unwrap();
        let RegionContent::Midi { notes } = &r.content else {
            panic!()
        };
        let n = notes.values().next().unwrap();
        assert_eq!(r.start, i64::from(half) * unit);
        assert_eq!(n.duration, i64::from(ppq) * unit);
        assert_eq!(n.duration, 960);
    }
}

fn vlq(out: &mut Vec<u8>, v: u32) {
    let mut groups = vec![(v & 0x7f) as u8];
    let mut v = v >> 7;
    while v > 0 {
        groups.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    out.extend(groups.iter().rev());
}

#[test]
fn unmatched_off_overlap_and_open_note_are_deterministic() {
    let track = vec![
        0x00, 0x90, 0x3C, 0x40, // on 60 at 0
        0x00, 0x80, 0x3E, 0x40, // off 62 with nothing open
        0x60, 0x90, 0x3C, 0x50, // second on 60 at 96
        0x60, 0x80, 0x3C, 0x00, // off at 192 pairs with the first on
        0x60, 0x80, 0x3C, 0x00, // off at 288 pairs with the second on
        0x60, 0x90, 0x40, 0x30, // on 64 at 384, never closed
        0x60, 0xFF, 0x2F, 0x00, // track ends at 480
    ];
    let bytes = smf(0, 960, &[track]);
    let plan = import(&bytes).unwrap();
    assert_eq!(plan, import(&bytes).unwrap());
    assert_eq!(plan.stats.unmatched_note_offs, 1);
    assert_eq!(plan.stats.unterminated_notes, 1);
    let notes: Vec<_> = plan
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::AddNote {
                pitch,
                velocity,
                start,
                duration,
                ..
            } => Some((*pitch, *velocity, *start, *duration)),
            _ => None,
        })
        .collect();
    assert_eq!(
        notes,
        vec![(60, 64, 0, 192), (60, 80, 96, 192), (64, 48, 384, 96)]
    );
    apply(&plan);
}

#[test]
fn ids_follow_the_seed() {
    let bytes = export_smf(&sample_project());
    let a = import_smf(
        &bytes,
        &ImportOptions {
            seed: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let b = import_smf(
        &bytes,
        &ImportOptions {
            seed: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let c = import_smf(
        &bytes,
        &ImportOptions {
            seed: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn multi_channel_track_splits_per_channel() {
    let track = vec![
        0x00, 0xFF, 0x03, 0x01, b'm', //
        0x00, 0x90, 0x3C, 0x40, //
        0x00, 0x91, 0x40, 0x40, //
        0x10, 0x80, 0x3C, 0x00, //
        0x00, 0x81, 0x40, 0x00,
    ];
    let p = apply(&import(&smf(0, 960, &[track])).unwrap()).into_project();
    let names: Vec<_> = p.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["m ch1", "m ch2"]);
}

#[test]
fn malformed_inputs_give_typed_errors() {
    let body = |b: &[u8]| smf(0, 960, &[b.to_vec()]);
    assert!(matches!(import(&[]), Err(MidiError::BadHeader(_))));
    assert!(matches!(
        import(b"RIFFxxxxxxxx"),
        Err(MidiError::BadHeader(_))
    ));
    assert!(matches!(
        import(&chunk(b"MThd", &[0, 0, 0, 1, 0])),
        Err(MidiError::BadHeader(_))
    ));
    assert_eq!(
        import(&smf(2, 960, &[vec![]])),
        Err(MidiError::UnsupportedFormat(2))
    );
    assert_eq!(
        import(&smf(1, 0xE728, &[vec![]])),
        Err(MidiError::UnsupportedDivision)
    );
    assert!(matches!(
        import(&smf(1, 0, &[vec![]])),
        Err(MidiError::BadHeader(_))
    ));
    assert_eq!(import(&smf(1, 960, &[])), Err(MidiError::NoTracks));
    assert!(matches!(
        import(&body(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00])),
        Err(MidiError::BadVarLen { .. })
    ));
    assert!(matches!(
        import(&body(&[0x00, 0xF1])),
        Err(MidiError::BadStatus { status: 0xF1, .. })
    ));
    assert!(matches!(
        import(&body(&[0x00, 0x3C, 0x40])),
        Err(MidiError::MissingStatus { .. })
    ));
    assert!(matches!(
        import(&body(&[0x00, 0x90, 0x3C, 0x80])),
        Err(MidiError::BadData { .. })
    ));
    // A meta event that claims more bytes than the chunk holds.
    assert!(matches!(
        import(&body(&[0x00, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0x7F, 0x01])),
        Err(MidiError::Truncated { .. })
    ));
    // A chunk that claims 4 GiB.
    let mut huge = smf(0, 960, &[vec![]]);
    huge[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(import(&huge), Err(MidiError::Truncated { .. })));
    // Declared tracks that are not there.
    let mut few = smf(1, 960, &[vec![0x00, 0xFF, 0x2F, 0x00]]);
    few[10..12].copy_from_slice(&5u16.to_be_bytes());
    assert!(matches!(import(&few), Err(MidiError::Truncated { .. })));
}

#[test]
fn limits_are_enforced() {
    let track = vec![
        0x00, 0x90, 0x3C, 0x40, 0x00, 0x90, 0x3E, 0x40, 0x10, 0x80, 0x3C, 0x00,
    ];
    let bytes = smf(0, 960, &[track]);
    let opts = ImportOptions {
        max_notes: 1,
        ..Default::default()
    };
    assert_eq!(import_smf(&bytes, &opts), Err(MidiError::TooManyNotes));
    let opts = ImportOptions {
        max_tracks: 0,
        ..Default::default()
    };
    assert_eq!(import_smf(&bytes, &opts), Err(MidiError::TooManyTracks));
    // A far away event overflows the model's tick range.
    let mut far = Vec::new();
    for _ in 0..5 {
        far.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0x7F, 0x00]);
    }
    far.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
    assert_eq!(import(&smf(0, 1, &[far])), Err(MidiError::TickOverflow));
}

#[test]
fn every_truncation_of_a_valid_file_is_rejected() {
    let bytes = export_smf(&sample_project());
    for n in 0..bytes.len() {
        assert!(import(&bytes[..n]).is_err(), "prefix of {n} bytes accepted");
    }
    assert!(import(&bytes).is_ok());
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Any accepted file must produce a plan the editor accepts, and nothing may panic.
fn check_bytes(bytes: &[u8]) {
    if let Ok(plan) = import(bytes) {
        let mut ed = Editor::new(Project::new(Id(1), ""));
        run(&mut ed, &plan).expect("plan from an accepted file must apply");
    }
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..2000 {
        let len = (rng.next() % 200) as usize;
        let junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        check_bytes(&junk);
        // Valid header, random track body.
        check_bytes(&smf(1, 480, &[junk.clone(), junk]));
    }
    let valid = export_smf(&sample_project());
    for _ in 0..3000 {
        let mut b = valid.clone();
        for _ in 0..1 + rng.next() % 4 {
            let i = (rng.next() % b.len() as u64) as usize;
            b[i] = rng.next() as u8;
        }
        if rng.next().is_multiple_of(4) {
            let cut = (rng.next() % b.len() as u64) as usize;
            b.truncate(cut);
        }
        check_bytes(&b);
    }
}

#[test]
fn import_is_one_undo_step() {
    let mut ed = Editor::new(sample_project());
    let before = ed.project().state_hash();
    let depth = ed.undo_depth();
    let plan = import(&export_smf(&sample_project())).unwrap();
    run(&mut ed, &plan).unwrap();
    assert_eq!(ed.undo_depth(), depth + 1);
    let after = ed.project().state_hash();
    assert_ne!(before, after);
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), before);
    assert!(ed.redo().unwrap());
    assert_eq!(ed.project().state_hash(), after);
}

#[test]
fn import_into_empty_project_undoes_to_initial_hash() {
    let mut ed = Editor::new(Project::new(Id(1), ""));
    let initial = ed.project().state_hash();
    let plan = import(&export_smf(&sample_project())).unwrap();
    run(&mut ed, &plan).unwrap();
    assert_eq!(ed.undo_depth(), 1);
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), initial);
}
