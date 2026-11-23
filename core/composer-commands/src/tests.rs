use super::*;
use composer_model::{Id, IdGen, Project, RegionContent, RegionKind, TrackKind};
use std::collections::BTreeSet;

fn id(n: u128) -> Id {
    Id(n)
}

fn samples() -> Vec<Command> {
    vec![
        Command::SetProjectName {
            name: "song".into(),
        },
        Command::SetTempo {
            tick: 3840,
            mbpm: 90_500,
        },
        Command::RemoveTempo { tick: 3840 },
        Command::SetTimeSignature {
            tick: 3840,
            numerator: 6,
            denominator: 8,
        },
        Command::RemoveTimeSignature { tick: 3840 },
        Command::SetKey {
            tick: 0,
            root: 9,
            scale: composer_model::Scale::Minor,
        },
        Command::RemoveKey { tick: 0 },
        Command::SetMarker {
            tick: 3840,
            name: "verse".into(),
        },
        Command::RemoveMarker { tick: 3840 },
        Command::SetRegionLoop {
            region: id(11),
            loop_length: 1920,
        },
        Command::AddAudioClip {
            id: id(12),
            track: id(10),
            start: 0,
            length: 960,
            asset: "ab".repeat(32),
            offset: Some(10),
            gain: Some(900),
        },
        Command::SetClipGain {
            region: id(12),
            gain: 800,
        },
        Command::SetClipOffset {
            region: id(12),
            offset: 100,
        },
        Command::AddTrack {
            id: id(10),
            name: "keys".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::RenameTrack {
            track: id(10),
            name: "piano".into(),
        },
        Command::SetInstrument {
            track: id(10),
            instrument: Some(id(99)),
        },
        Command::SetTrackSound {
            track: id(10),
            sound: Some("builtin-bell".into()),
        },
        Command::MoveTrack {
            track: id(10),
            index: 0,
        },
        Command::DeleteTrack { track: id(10) },
        Command::AddRegion {
            id: id(11),
            track: id(10),
            start: 0,
            length: 3840,
            kind: RegionKind::Midi,
        },
        Command::MoveRegion {
            region: id(11),
            start: 960,
        },
        Command::ResizeRegion {
            region: id(11),
            length: 7680,
        },
        Command::DeleteRegion { region: id(11) },
        Command::AddNote {
            id: id(12),
            region: id(11),
            pitch: 60,
            velocity: 100,
            start: 0,
            duration: 480,
        },
        Command::MoveNote {
            region: id(11),
            note: id(12),
            start: 480,
            pitch: 62,
        },
        Command::ResizeNote {
            region: id(11),
            note: id(12),
            duration: 960,
        },
        Command::SetNoteVelocity {
            region: id(11),
            note: id(12),
            velocity: 64,
        },
        Command::DeleteNote {
            region: id(11),
            note: id(12),
        },
        Command::MoveRegionToTrack {
            region: id(11),
            track: id(20),
        },
        Command::SetTrackVolume {
            track: id(10),
            volume: 800,
        },
        Command::SetTrackPan {
            track: id(10),
            pan: -300,
        },
        Command::SetTrackMute {
            track: id(10),
            mute: true,
        },
        Command::SetTrackSolo {
            track: id(10),
            solo: true,
        },
        Command::AddNotes {
            region: id(11),
            notes: vec![NoteSpec {
                id: id(13),
                pitch: 64,
                velocity: 90,
                start: 0,
                duration: 240,
            }],
        },
        Command::SplitRegion {
            region: id(11),
            at: 1920,
            new_region: id(14),
            note_ids: vec![id(15)],
        },
        Command::DuplicateRegion {
            region: id(11),
            new_region: id(16),
            start: 3840,
            track: None,
            note_ids: vec![id(17)],
        },
        Command::TransposeNotes {
            region: id(11),
            semitones: -12,
            notes: None,
        },
        Command::QuantizeNotes {
            region: id(11),
            grid: 480,
            strength: 100,
            notes: Some(vec![id(12)]),
        },
        Command::MoveNotes {
            region: id(11),
            notes: vec![id(12)],
            delta_tick: 10,
            delta_pitch: 1,
        },
        Command::ResizeNotes {
            region: id(11),
            notes: vec![id(12)],
            delta: -10,
        },
        Command::SetNotesVelocity {
            region: id(11),
            notes: vec![id(12)],
            value: None,
            ramp: Some(VelocityRamp { from: 20, to: 100 }),
        },
        Command::DeleteNotes {
            region: id(11),
            notes: vec![id(12)],
        },
        Command::ConformNotesToScale {
            region: id(11),
            notes: None,
        },
        Command::SetMasterVolume { volume: 800 },
        Command::SetMasterReverb {
            size: 600,
            mix: 250,
        },
        Command::SetTrackReverbSend {
            track: id(10),
            send: 300,
        },
        Command::AddAutomationPoint {
            target: composer_model::AutomationTarget::TrackVolume { track: id(10) },
            tick: 960,
            value: 500,
            curve: Some(composer_model::AutomationCurve::Smooth),
        },
        Command::MoveAutomationPoint {
            target: composer_model::AutomationTarget::TrackPan { track: id(10) },
            tick: 960,
            new_tick: 1920,
            value: -200,
        },
        Command::SetAutomationCurve {
            target: composer_model::AutomationTarget::MasterVolume,
            tick: 0,
            curve: composer_model::AutomationCurve::Hold,
        },
        Command::DeleteAutomationPoint {
            target: composer_model::AutomationTarget::TrackReverbSend { track: id(10) },
            tick: 0,
        },
        Command::ClearAutomationLane {
            target: composer_model::AutomationTarget::MasterVolume,
        },
        Command::AddEffect {
            id: id(60),
            owner: composer_model::EffectOwner::Master,
            kind: composer_model::EffectKind::Delay,
            index: None,
        },
        Command::RemoveEffect { effect: id(60) },
        Command::MoveEffect {
            effect: id(60),
            index: 0,
        },
        Command::SetEffectBypass {
            effect: id(60),
            bypass: true,
        },
        Command::SetEffectParam {
            effect: id(60),
            param: "time_ms".into(),
            value: 120,
        },
    ]
}

fn editor() -> Editor {
    Editor::new(Project::new(id(1), "test"))
}

fn run(ed: &mut Editor, cmds: &[Command]) {
    for c in cmds {
        ed.execute(c)
            .unwrap_or_else(|e| panic!("{} failed: {e}", c.name()));
    }
}

fn song() -> Vec<Command> {
    vec![
        Command::AddTrack {
            id: id(10),
            name: "keys".into(),
            kind: TrackKind::Instrument,
            index: None,
        },
        Command::AddRegion {
            id: id(11),
            track: id(10),
            start: 0,
            length: 3840,
            kind: RegionKind::Midi,
        },
        Command::AddNote {
            id: id(12),
            region: id(11),
            pitch: 60,
            velocity: 100,
            start: 0,
            duration: 480,
        },
    ]
}

#[test]
fn every_command_round_trips_through_json() {
    for c in samples() {
        let json = serde_json::to_string(&c).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c, "{json}");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], c.name());
    }
}

#[test]
fn describe_matches_command_json() {
    let samples = samples();
    assert_eq!(describe().len(), samples.len());
    for c in &samples {
        let info = describe()
            .iter()
            .find(|i| i.name == c.name())
            .expect("described");
        let v = serde_json::to_value(c).unwrap();
        let keys: BTreeSet<&str> = v
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|k| *k != "command")
            .collect();
        let listed: BTreeSet<&str> = info.fields.iter().map(|f| f.name).collect();
        assert_eq!(keys, listed, "{}", c.name());
    }
    let json = serde_json::to_string(describe()).unwrap();
    assert!(json.contains("add_note"));
}

#[test]
fn apply_then_undo_restores_hash_and_redo_repeats() {
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    let mut hashes = vec![h0];
    for c in song() {
        ed.execute(&c).unwrap();
        hashes.push(ed.project().state_hash());
    }
    assert_ne!(hashes[0], hashes[3]);
    for expected in hashes.iter().rev().skip(1) {
        assert!(ed.undo().unwrap());
        assert_eq!(ed.project().state_hash(), *expected);
    }
    assert!(!ed.undo().unwrap());
    for expected in hashes.iter().skip(1) {
        assert!(ed.redo().unwrap());
        assert_eq!(ed.project().state_hash(), *expected);
    }
    assert!(!ed.redo().unwrap());
}

#[test]
fn new_edit_clears_redo() {
    let mut ed = editor();
    run(&mut ed, &song());
    ed.undo().unwrap();
    assert_eq!(ed.redo_depth(), 1);
    ed.execute(&Command::SetProjectName { name: "x".into() })
        .unwrap();
    assert_eq!(ed.redo_depth(), 0);
}

#[test]
fn group_undoes_as_one_step() {
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    ed.group("write phrase", |e| {
        for c in song() {
            e.execute(&c)?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(ed.undo_depth(), 1);
    assert_eq!(ed.undo_label(), Some("write phrase"));
    let h1 = ed.project().state_hash();
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), h0);
    assert!(ed.redo().unwrap());
    assert_eq!(ed.project().state_hash(), h1);
}

#[test]
fn failed_group_rolls_back_everything() {
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    let r = ed.group("bad", |e| {
        for c in song() {
            e.execute(&c)?;
        }
        e.execute(&Command::DeleteTrack { track: id(777) })
    });
    assert!(matches!(r, Err(CommandError::NotFound { .. })));
    assert_eq!(ed.project().state_hash(), h0);
    assert_eq!(ed.undo_depth(), 0);
    assert!(ed.begin_group("again").is_ok());
    assert_eq!(
        ed.begin_group("nested"),
        Err(CommandError::GroupAlreadyOpen)
    );
    assert_eq!(ed.undo(), Err(CommandError::GroupAlreadyOpen));
}

#[test]
fn invalid_commands_leave_state_unchanged() {
    let mut ed = editor();
    run(&mut ed, &song());
    run(
        &mut ed,
        &[Command::AddTrack {
            id: id(20),
            name: "drums".into(),
            kind: TrackKind::Audio,
            index: None,
        }],
    );
    let h = ed.project().state_hash();
    let depth = ed.undo_depth();
    let bad = vec![
        Command::AddTrack {
            id: id(10),
            name: "dup".into(),
            kind: TrackKind::Audio,
            index: None,
        },
        Command::AddTrack {
            id: id(30),
            name: "x".into(),
            kind: TrackKind::Audio,
            index: Some(99),
        },
        Command::AddRegion {
            id: id(31),
            track: id(20),
            start: 0,
            length: 10,
            kind: RegionKind::Midi,
        },
        Command::AddRegion {
            id: id(31),
            track: id(10),
            start: -1,
            length: 10,
            kind: RegionKind::Midi,
        },
        Command::AddRegion {
            id: id(31),
            track: id(10),
            start: 0,
            length: 0,
            kind: RegionKind::Midi,
        },
        Command::AddNote {
            id: id(32),
            region: id(11),
            pitch: 128,
            velocity: 100,
            start: 0,
            duration: 1,
        },
        Command::AddNote {
            id: id(32),
            region: id(11),
            pitch: 60,
            velocity: 0,
            start: 0,
            duration: 1,
        },
        Command::AddNote {
            id: id(12),
            region: id(11),
            pitch: 60,
            velocity: 1,
            start: 0,
            duration: 1,
        },
        Command::MoveNote {
            region: id(11),
            note: id(999),
            start: 0,
            pitch: 1,
        },
        Command::SetInstrument {
            track: id(20),
            instrument: Some(id(5)),
        },
        Command::SetTempo { tick: 0, mbpm: 1 },
        Command::RemoveTempo { tick: 0 },
        Command::SetTimeSignature {
            tick: 0,
            numerator: 4,
            denominator: 3,
        },
        Command::MoveTrack {
            track: id(10),
            index: 2,
        },
        Command::SetProjectName {
            name: "n".repeat(300),
        },
    ];
    for c in &bad {
        assert!(ed.execute(c).is_err(), "{c:?}");
        assert_eq!(ed.project().state_hash(), h, "{c:?}");
    }
    assert_eq!(ed.undo_depth(), depth);
}

#[test]
fn tempo_map_edits_and_deleting_a_track_undo_cleanly() {
    let mut ed = editor();
    run(&mut ed, &song());
    let h = ed.project().state_hash();
    run(
        &mut ed,
        &[
            Command::SetTempo {
                tick: 3840,
                mbpm: 90_000,
            },
            Command::SetTimeSignature {
                tick: 3840,
                numerator: 3,
                denominator: 4,
            },
            Command::DeleteTrack { track: id(10) },
        ],
    );
    assert!(ed.project().tracks.is_empty());
    for _ in 0..3 {
        ed.undo().unwrap();
    }
    assert_eq!(ed.project().state_hash(), h);
    assert!(matches!(
        ed.project().region(id(11)).unwrap().1.content,
        RegionContent::Midi { .. }
    ));
}

#[test]
fn state_hash_is_stable_across_clones_and_distinguishes_edits() {
    let mut ed = editor();
    run(&mut ed, &song());
    assert_eq!(ed.project().clone().state_hash(), ed.project().state_hash());
    // Pinned value: a change here means the hash format changed and clients must be told.
    assert_eq!(
        Project::new(id(1), "test").state_hash(),
        17_432_832_799_248_192_433
    );
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len() as u64) as usize])
        }
    }
}

fn random_command(rng: &mut Rng, ids: &mut IdGen, p: &Project) -> Command {
    let tracks: Vec<Id> = p.tracks.iter().map(|t| t.id).collect();
    let regions: Vec<Id> = p
        .tracks
        .iter()
        .flat_map(|t| t.regions.keys().copied())
        .collect();
    let notes: Vec<(Id, Id)> = p
        .tracks
        .iter()
        .flat_map(|t| t.regions.values())
        .flat_map(|r| match &r.content {
            RegionContent::Midi { notes } => notes.keys().map(|n| (r.id, *n)).collect(),
            RegionContent::Audio | RegionContent::Clip { .. } => Vec::new(),
        })
        .collect();
    let ghost = ids.next_id();
    let tick = |rng: &mut Rng| (rng.below(64) * 240) as i64;
    // A tenth of the time aim at a missing target to exercise rejection.
    let miss = rng.below(10) == 0;
    let track = if miss { Some(ghost) } else { rng.pick(&tracks) };
    let region = if miss {
        Some(ghost)
    } else {
        rng.pick(&regions)
    };
    let note = if miss {
        Some((ghost, ghost))
    } else {
        rng.pick(&notes)
    };
    let fresh = ids.next_id();
    let kind = if rng.below(2) == 0 {
        TrackKind::Instrument
    } else {
        TrackKind::Audio
    };
    let rkind = if rng.below(2) == 0 {
        RegionKind::Midi
    } else {
        RegionKind::Audio
    };
    match rng.below(20) {
        0 => Command::SetProjectName {
            name: format!("p{}", rng.below(5)),
        },
        1 => Command::SetTempo {
            tick: tick(rng),
            mbpm: 40_000 + rng.below(200) as u32 * 1000,
        },
        2 => Command::RemoveTempo { tick: tick(rng) },
        3 => Command::SetTimeSignature {
            tick: tick(rng),
            numerator: 1 + rng.below(8) as u8,
            denominator: 1 << rng.below(6),
        },
        4 => Command::RemoveTimeSignature { tick: tick(rng) },
        5 => Command::AddTrack {
            id: fresh,
            name: "t".into(),
            kind,
            index: Some(rng.below(tracks.len() as u64 + 2) as usize),
        },
        6 => Command::RenameTrack {
            track: track.unwrap_or(ghost),
            name: format!("n{}", rng.below(4)),
        },
        7 => Command::SetInstrument {
            track: track.unwrap_or(ghost),
            instrument: if rng.below(3) == 0 { None } else { Some(fresh) },
        },
        8 => Command::MoveTrack {
            track: track.unwrap_or(ghost),
            index: rng.below(tracks.len() as u64 + 1) as usize,
        },
        9 => Command::DeleteTrack {
            track: track.unwrap_or(ghost),
        },
        10 => Command::AddRegion {
            id: fresh,
            track: track.unwrap_or(ghost),
            start: tick(rng),
            length: 1 + tick(rng),
            kind: rkind,
        },
        11 => Command::MoveRegion {
            region: region.unwrap_or(ghost),
            start: tick(rng),
        },
        12 => Command::ResizeRegion {
            region: region.unwrap_or(ghost),
            length: tick(rng),
        },
        13 => Command::DeleteRegion {
            region: region.unwrap_or(ghost),
        },
        14 => Command::AddNote {
            id: fresh,
            region: region.unwrap_or(ghost),
            pitch: rng.below(130) as u8,
            velocity: rng.below(129) as u8,
            start: tick(rng),
            duration: 1 + tick(rng),
        },
        15 => {
            let (r, n) = note.unwrap_or((ghost, ghost));
            Command::MoveNote {
                region: r,
                note: n,
                start: tick(rng),
                pitch: rng.below(128) as u8,
            }
        }
        16 => {
            let (r, n) = note.unwrap_or((ghost, ghost));
            Command::ResizeNote {
                region: r,
                note: n,
                duration: 1 + tick(rng),
            }
        }
        17 => {
            let (r, n) = note.unwrap_or((ghost, ghost));
            Command::SetNoteVelocity {
                region: r,
                note: n,
                velocity: 1 + rng.below(127) as u8,
            }
        }
        18 => {
            let (r, n) = note.unwrap_or((ghost, ghost));
            Command::DeleteNote { region: r, note: n }
        }
        _ => Command::MoveRegionToTrack {
            region: region.unwrap_or(ghost),
            track: track.unwrap_or(ghost),
        },
    }
}

#[test]
fn random_commands_undo_back_to_initial_hash() {
    for seed in [1u64, 7, 0xC0FFEE] {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut ids = IdGen::new(seed);
        let mut ed = editor();
        let h0 = ed.project().state_hash();
        let (mut ok, mut rejected) = (0, 0);
        for _ in 0..1000 {
            let c = random_command(&mut rng, &mut ids, ed.project());
            let before = ed.project().state_hash();
            // Every command also survives a JSON trip before running.
            let c: Command = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
            match ed.execute(&c) {
                Ok(()) => ok += 1,
                Err(_) => {
                    rejected += 1;
                    assert_eq!(ed.project().state_hash(), before, "{c:?}");
                }
            }
            assert!(ed.project().index_is_consistent(), "{c:?}");
            assert_eq!(ed.project().validate(), Ok(()), "{c:?}");
        }
        assert!(ok > 200 && rejected > 20, "ok {ok} rejected {rejected}");
        let h_end = ed.project().state_hash();
        let json = serde_json::to_string(ed.project()).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state_hash(), h_end);
        assert!(back.index_is_consistent());
        let depth = ed.undo_depth();
        let mut undone = 0;
        while ed.undo().unwrap() {
            undone += 1;
            assert!(ed.project().index_is_consistent(), "seed {seed} undo");
        }
        assert_eq!(undone, depth);
        assert_eq!(ed.project().state_hash(), h0, "seed {seed}");
        while ed.redo().unwrap() {
            assert!(ed.project().index_is_consistent(), "seed {seed} redo");
        }
        assert_eq!(ed.project().state_hash(), h_end, "seed {seed}");
    }
}

fn two_tracks() -> Editor {
    let mut ed = editor();
    run(&mut ed, &song());
    run(
        &mut ed,
        &[
            Command::AddTrack {
                id: id(20),
                name: "pads".into(),
                kind: TrackKind::Instrument,
                index: None,
            },
            Command::AddTrack {
                id: id(30),
                name: "audio".into(),
                kind: TrackKind::Audio,
                index: None,
            },
            Command::AddRegion {
                id: id(31),
                track: id(30),
                start: 0,
                length: 100,
                kind: RegionKind::Audio,
            },
        ],
    );
    ed
}

#[test]
fn move_region_to_track_moves_notes_and_undoes() {
    let mut ed = two_tracks();
    let h = ed.project().state_hash();
    ed.execute(&Command::MoveRegionToTrack {
        region: id(11),
        track: id(20),
    })
    .unwrap();
    let p = ed.project();
    assert!(p.track(id(10)).unwrap().regions.is_empty());
    let (t, r) = p.region(id(11)).unwrap();
    assert_eq!(t.id, id(20));
    assert_eq!(r.start, 0);
    assert!(p.note(id(11), id(12)).is_some());
    assert!(p.index_is_consistent());
    assert_ne!(p.state_hash(), h);
    assert!(ed.undo().unwrap());
    assert_eq!(ed.project().state_hash(), h);
    assert!(ed.project().index_is_consistent());
    assert!(ed.redo().unwrap());
    assert_eq!(ed.project().region(id(11)).unwrap().0.id, id(20));

    // Same track is a no-op, wrong kind and missing targets are rejected.
    let depth = ed.undo_depth();
    ed.execute(&Command::MoveRegionToTrack {
        region: id(11),
        track: id(20),
    })
    .unwrap();
    assert_eq!(ed.undo_depth(), depth);
    let cases = [
        (id(11), id(30), "wrong_kind"),
        (id(31), id(20), "wrong_kind"),
        (id(11), id(999), "not_found"),
        (id(999), id(20), "not_found"),
    ];
    for (region, track, code) in cases {
        let err = ed
            .execute(&Command::MoveRegionToTrack { region, track })
            .unwrap_err();
        assert_eq!(err.code(), code, "{err}");
    }
}

#[test]
fn notes_must_lie_inside_their_region() {
    let mut ed = two_tracks();
    // Region 11 is 3840 long. Note 12 sits at 0 with duration 480.
    let h = ed.project().state_hash();
    let add = |start, duration| Command::AddNote {
        id: id(40),
        region: id(11),
        pitch: 60,
        velocity: 90,
        start,
        duration,
    };
    let bad = [
        add(3840, 1),
        add(3800, 100),
        Command::MoveNote {
            region: id(11),
            note: id(12),
            start: 3600,
            pitch: 60,
        },
        Command::ResizeNote {
            region: id(11),
            note: id(12),
            duration: 3841,
        },
        Command::ResizeRegion {
            region: id(11),
            length: 479,
        },
    ];
    for c in &bad {
        let err = ed.execute(c).unwrap_err();
        assert_eq!(err.code(), "out_of_bounds", "{c:?}");
        assert_eq!(ed.project().state_hash(), h);
    }
    // Negative starts are plain range errors.
    let err = ed.execute(&add(-1, 10)).unwrap_err();
    assert_eq!(err.code(), "invalid");
    // Exactly touching the end is allowed, and shrinking below the last note end is not.
    run(&mut ed, &[add(3840 - 100, 100)]);
    run(
        &mut ed,
        &[Command::ResizeRegion {
            region: id(11),
            length: 3840,
        }],
    );
    let err = ed
        .execute(&Command::ResizeRegion {
            region: id(11),
            length: 3839,
        })
        .unwrap_err();
    assert_eq!(err.code(), "out_of_bounds");
}

#[test]
fn overlapping_regions_on_one_track_are_allowed() {
    let mut ed = two_tracks();
    run(
        &mut ed,
        &[Command::AddRegion {
            id: id(50),
            track: id(10),
            start: 100,
            length: 3840,
            kind: RegionKind::Midi,
        }],
    );
    assert_eq!(ed.project().track(id(10)).unwrap().regions.len(), 2);
}

#[test]
fn apply_enforces_ranges_without_commands() {
    use composer_model::{Change, ChangeError, FieldKey, FieldValue, Note, MAX_TICK};
    let mut ed = two_tracks();
    let mut p = ed.project().clone();
    let h = p.state_hash();
    let note = |n: u128, pitch, velocity, start, duration| Note {
        id: id(n),
        pitch,
        velocity,
        start,
        duration,
    };
    let new_note = |note| Change::NewNote {
        region: id(11),
        note,
    };
    let set = |key, old, new| Change::Set { key, old, new };
    let key = |f: fn(Id, Id) -> FieldKey| f(id(11), id(12));
    let bad = [
        (new_note(note(60, 128, 90, 0, 10)), "pitch"),
        (new_note(note(60, 60, 0, 0, 10)), "velocity"),
        (new_note(note(60, 60, 128, 0, 10)), "velocity"),
        (new_note(note(60, 60, 90, -1, 10)), "note start"),
        (new_note(note(60, 60, 90, 0, 0)), "note duration"),
        (new_note(note(60, 60, 90, MAX_TICK + 1, 1)), "note start"),
        (
            set(
                key(|region, note| FieldKey::NotePitch { region, note }),
                FieldValue::Byte(60),
                FieldValue::Byte(200),
            ),
            "pitch",
        ),
        (
            set(
                key(|region, note| FieldKey::NoteVelocity { region, note }),
                FieldValue::Byte(100),
                FieldValue::Byte(0),
            ),
            "velocity",
        ),
        (
            set(
                FieldKey::RegionStart { region: id(11) },
                FieldValue::Tick(0),
                FieldValue::Tick(-5),
            ),
            "region start",
        ),
        (
            set(
                FieldKey::RegionLength { region: id(11) },
                FieldValue::Tick(3840),
                FieldValue::Tick(0),
            ),
            "region length",
        ),
        (
            Change::SetTempo {
                tick: 960,
                old: None,
                new: Some(1),
            },
            "tempo",
        ),
        (
            Change::SetTempo {
                tick: -1,
                old: None,
                new: Some(120_000),
            },
            "tick",
        ),
    ];
    for (c, what) in bad {
        assert_eq!(p.apply(&c), Err(ChangeError::OutOfRange(what)), "{c:?}");
    }
    // The inside-the-region rule also holds for direct changes.
    let out = set(
        key(|region, note| FieldKey::NoteStart { region, note }),
        FieldValue::Tick(0),
        FieldValue::Tick(3800),
    );
    assert_eq!(p.apply(&out), Err(ChangeError::OutOfBounds("note")));
    let out = new_note(note(61, 60, 90, 3800, 100));
    assert_eq!(p.apply(&out), Err(ChangeError::OutOfBounds("note")));
    assert_eq!(p.state_hash(), h);
    assert!(p.index_is_consistent());
    // And it surfaces through the editor with a stable code.
    let err = ed
        .execute(&Command::SetTempo { tick: 0, mbpm: 5 })
        .unwrap_err();
    assert_eq!(err.code(), "invalid");
}

#[test]
fn project_json_load_rebuilds_the_index() {
    let ed = two_tracks();
    let json = serde_json::to_string(ed.project()).unwrap();
    assert!(!json.contains("index"));
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, ed.project());
    assert!(back.contains_id(id(12)) && back.region(id(31)).is_some());
    assert!(back.index_is_consistent());
    let mut again = back.clone();
    again.reindex();
    assert!(again.index_is_consistent());
}

#[test]
fn scale_lookups_use_the_index_and_undo_returns_to_start() {
    const TRACKS: u128 = 200;
    const REGIONS: u128 = 50;
    const NOTES: u128 = 20;
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    let mut n = 1000u128;
    let mut next = || {
        n += 1;
        Id(n)
    };
    let mut notes = Vec::new();
    let mut regions = Vec::new();
    let mut commands = 0u64;
    let probes0 = ed.project().index_probes();
    for ti in 0..TRACKS {
        let t = next();
        let mut batch = vec![Command::AddTrack {
            id: t,
            name: format!("t{ti}"),
            kind: TrackKind::Instrument,
            index: None,
        }];
        for ri in 0..REGIONS {
            let r = next();
            regions.push((t, r));
            batch.push(Command::AddRegion {
                id: r,
                track: t,
                start: (ri as i64) * 4000,
                length: 4000,
                kind: RegionKind::Midi,
            });
            for ni in 0..NOTES {
                let nid = next();
                notes.push((r, nid));
                batch.push(Command::AddNote {
                    id: nid,
                    region: r,
                    pitch: 40 + (ni % 40) as u8,
                    velocity: 90,
                    start: (ni as i64) * 200,
                    duration: 100,
                });
            }
        }
        commands += batch.len() as u64;
        run(&mut ed, &batch);
    }
    assert_eq!(notes.len() as u128, TRACKS * REGIONS * NOTES);
    // Building does a small constant number of probes per command, never one per element.
    let build_probes = ed.project().index_probes() - probes0;
    assert!(
        build_probes <= commands * 12,
        "{build_probes} for {commands}"
    );

    // Lookups: contains_id and track cost one probe, region and note two.
    let p = ed.project();
    let before = p.index_probes();
    for (r, nid) in &notes {
        assert!(p.contains_id(*nid));
        assert!(p.note(*r, *nid).is_some());
    }
    assert_eq!(p.index_probes() - before, 3 * notes.len() as u64);
    let before = p.index_probes();
    for (t, r) in &regions {
        assert_eq!(p.region(*r).unwrap().0.id, *t);
        assert!(p.track(*t).is_some());
    }
    assert_eq!(p.index_probes() - before, 3 * regions.len() as u64);
    assert!(!p.contains_id(Id(7)));
    assert!(p.index_is_consistent());

    // Cross-track move at scale, then undo everything.
    let (_, r0) = regions[0];
    let (t1, _) = regions[REGIONS as usize];
    run(
        &mut ed,
        &[Command::MoveRegionToTrack {
            region: r0,
            track: t1,
        }],
    );
    assert_eq!(ed.project().region(r0).unwrap().0.id, t1);
    let mut undone = 0u64;
    while ed.undo().unwrap() {
        undone += 1;
    }
    assert_eq!(undone, commands + 1);
    assert_eq!(ed.project().state_hash(), h0);
    assert!(ed.project().index_is_consistent());
    assert!(!ed.project().contains_id(notes[0].1));
}

#[test]
fn set_track_sound_is_one_undo_step_and_keeps_old_hashes() {
    let mut ed = editor();
    run(&mut ed, &song());
    let plain = ed.project().state_hash();
    let json = serde_json::to_string(ed.project()).unwrap();
    assert!(!json.contains("sound"));
    ed.execute(&Command::SetTrackSound {
        track: id(10),
        sound: Some("builtin-warm-pad".into()),
    })
    .unwrap();
    assert_ne!(ed.project().state_hash(), plain);
    assert_eq!(
        ed.project().track(id(10)).unwrap().sound.as_deref(),
        Some("builtin-warm-pad")
    );
    let json = serde_json::to_string(ed.project()).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.state_hash(), ed.project().state_hash());
    // Clearing it returns to the exact old hash and the old JSON.
    ed.execute(&Command::SetTrackSound {
        track: id(10),
        sound: None,
    })
    .unwrap();
    assert_eq!(ed.project().state_hash(), plain);
    ed.undo().unwrap();
    assert_eq!(
        ed.project().track(id(10)).unwrap().sound.as_deref(),
        Some("builtin-warm-pad")
    );
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), plain);
}

#[test]
fn set_track_sound_rejects_bad_ids_and_wrong_tracks() {
    let mut ed = editor();
    run(&mut ed, &song());
    ed.execute(&Command::AddTrack {
        id: id(30),
        name: "audio".into(),
        kind: TrackKind::Audio,
        index: None,
    })
    .unwrap();
    let h = ed.project().state_hash();
    for bad in ["", "Upper", "has space", "under_score", &"a".repeat(65)] {
        let r = ed.execute(&Command::SetTrackSound {
            track: id(10),
            sound: Some(bad.to_string()),
        });
        assert!(r.is_err(), "{bad:?}");
    }
    assert!(ed
        .execute(&Command::SetTrackSound {
            track: id(30),
            sound: Some("builtin-bell".into()),
        })
        .is_err());
    assert!(ed
        .execute(&Command::SetTrackSound {
            track: id(777),
            sound: None,
        })
        .is_err());
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn key_commands_undo_and_keep_old_hash_when_unused() {
    use composer_model::{Key, Scale};
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    let set = |tick, root, scale| Command::SetKey { tick, root, scale };
    ed.execute(&set(0, 9, Scale::Minor)).unwrap();
    let h1 = ed.project().state_hash();
    assert_ne!(h0, h1);
    assert_eq!(
        ed.project().tempo_map.keys[&0],
        Key {
            root: 9,
            scale: Scale::Minor
        }
    );
    // Same value again plans nothing; bad root and unknown removal are rejected.
    assert!(set(0, 9, Scale::Minor)
        .plan(ed.project())
        .unwrap()
        .is_empty());
    assert!(set(0, 12, Scale::Major).plan(ed.project()).is_err());
    assert!(set(-1, 0, Scale::Major).plan(ed.project()).is_err());
    assert!(Command::RemoveKey { tick: 960 }.plan(ed.project()).is_err());
    ed.execute(&set(3840, 2, Scale::Dorian)).unwrap();
    ed.execute(&Command::RemoveKey { tick: 0 }).unwrap();
    assert_eq!(ed.project().tempo_map.keys.len(), 1);
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h1);
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h0);
    assert!(ed.project().tempo_map.keys.is_empty());
    // A project with no key serializes without a keys field and reloads equal.
    let json = serde_json::to_string(ed.project()).unwrap();
    assert!(!json.contains("keys"));
    ed.execute(&set(0, 0, Scale::Major)).unwrap();
    let json = serde_json::to_string(ed.project()).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.state_hash(), ed.project().state_hash());
    assert!(back.validate().is_ok());
}

#[test]
fn master_and_send_commands_undo_and_keep_default_hashes() {
    let mut ed = editor();
    run(&mut ed, &song());
    let base = ed.project().state_hash();
    run(
        &mut ed,
        &[
            Command::SetMasterVolume { volume: 800 },
            Command::SetMasterReverb {
                size: 700,
                mix: 300,
            },
            Command::SetTrackReverbSend {
                track: id(10),
                send: 400,
            },
        ],
    );
    let p = ed.project();
    assert_eq!(
        (p.master.volume, p.master.reverb_size, p.master.reverb_mix),
        (800, 700, 300)
    );
    assert_eq!(p.tracks[0].reverb_send, 400);
    assert_ne!(p.state_hash(), base);
    for _ in 0..3 {
        ed.undo().unwrap();
    }
    assert_eq!(ed.project().state_hash(), base);
    for _ in 0..3 {
        ed.redo().unwrap();
    }
    assert_eq!(ed.project().tracks[0].reverb_send, 400);
    // Setting the defaults back restores the original hash too.
    run(
        &mut ed,
        &[
            Command::SetMasterVolume { volume: 1000 },
            Command::SetMasterReverb { size: 500, mix: 0 },
            Command::SetTrackReverbSend {
                track: id(10),
                send: 0,
            },
        ],
    );
    assert_eq!(ed.project().state_hash(), base);
}

#[test]
fn master_and_send_commands_reject_bad_values() {
    let mut ed = editor();
    run(&mut ed, &song());
    let before = ed.project().state_hash();
    assert!(ed
        .execute(&Command::SetMasterVolume { volume: 2001 })
        .is_err());
    assert!(ed
        .execute(&Command::SetMasterReverb { size: 1001, mix: 0 })
        .is_err());
    assert!(ed
        .execute(&Command::SetMasterReverb { size: 0, mix: 1001 })
        .is_err());
    assert!(ed
        .execute(&Command::SetTrackReverbSend {
            track: id(10),
            send: 1001
        })
        .is_err());
    assert!(ed
        .execute(&Command::SetTrackReverbSend {
            track: id(99),
            send: 10
        })
        .is_err());
    assert_eq!(ed.project().state_hash(), before);
}

#[test]
fn loop_and_marker_commands_undo_and_skip_defaults() {
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
            start: 0,
            length: 7680,
            kind: RegionKind::Midi,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    let before = ed.project().state_hash();
    let json_before = serde_json::to_string(ed.project()).unwrap();
    assert!(!json_before.contains("loop_length") && !json_before.contains("markers"));
    ed.execute(&Command::SetRegionLoop {
        region: id(11),
        loop_length: 1920,
    })
    .unwrap();
    ed.execute(&Command::SetMarker {
        tick: 960,
        name: "intro".into(),
    })
    .unwrap();
    ed.execute(&Command::SetMarker {
        tick: 960,
        name: "verse".into(),
    })
    .unwrap();
    assert_ne!(ed.project().state_hash(), before);
    assert_eq!(ed.project().region(id(11)).unwrap().1.loop_length, 1920);
    assert_eq!(ed.project().markers[&960], "verse");
    let json = serde_json::to_string(ed.project()).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.state_hash(), ed.project().state_hash());
    ed.execute(&Command::RemoveMarker { tick: 960 }).unwrap();
    assert!(ed.execute(&Command::RemoveMarker { tick: 960 }).is_err());
    assert!(ed
        .execute(&Command::SetRegionLoop {
            region: id(11),
            loop_length: 10,
        })
        .is_err());
    ed.undo_steps(4).unwrap();
    assert_eq!(ed.project().state_hash(), before);
    ed.redo_steps(4).unwrap();
    ed.execute(&Command::SetRegionLoop {
        region: id(11),
        loop_length: 0,
    })
    .unwrap();
    assert_eq!(ed.project().region(id(11)).unwrap().1.loop_length, 0);
}

#[test]
fn looped_clip_cannot_be_split_and_duplicates_keep_the_loop() {
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
            start: 0,
            length: 7680,
            kind: RegionKind::Midi,
        },
        Command::SetRegionLoop {
            region: id(11),
            loop_length: 1920,
        },
    ] {
        ed.execute(&c).unwrap();
    }
    assert!(ed
        .execute(&Command::SplitRegion {
            region: id(11),
            at: 960,
            new_region: id(12),
            note_ids: vec![],
        })
        .is_err());
    ed.execute(&Command::DuplicateRegion {
        region: id(11),
        new_region: id(12),
        start: 8000,
        track: None,
        note_ids: vec![],
    })
    .unwrap();
    assert_eq!(ed.project().region(id(12)).unwrap().1.loop_length, 1920);
}
