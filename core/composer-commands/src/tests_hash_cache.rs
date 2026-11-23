//! Property test for D47: the editor's incremental state hash always equals the from scratch
//! `Project::state_hash`, through random edits, undo, redo, groups and rollbacks.

use super::*;
use composer_model::{
    EffectKind, EffectOwner, HashCache, Id, Project, RegionContent, RegionKind, TrackKind,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    /// Wide ids; one in three is forced to be a run boundary so run edges are exercised.
    fn id(&mut self) -> Id {
        let force = self.below(3) == 0;
        loop {
            let id = Id((u128::from(self.next()) << 64) | u128::from(self.next()));
            if !force || HashCache::starts_run(id) {
                return id;
            }
        }
    }
}

struct Ids {
    tracks: Vec<Id>,
    regions: Vec<(Id, Id)>,
    notes: Vec<(Id, Id)>,
}

fn ids_of(p: &Project) -> Ids {
    let mut out = Ids {
        tracks: Vec::new(),
        regions: Vec::new(),
        notes: Vec::new(),
    };
    for t in &p.tracks {
        out.tracks.push(t.id);
        for r in t.regions.values() {
            out.regions.push((t.id, r.id));
            if let RegionContent::Midi { notes } = &r.content {
                out.notes.extend(notes.keys().map(|n| (r.id, *n)));
            }
        }
    }
    out
}

fn pick<T: Copy>(rng: &mut Rng, v: &[T]) -> Option<T> {
    (!v.is_empty()).then(|| v[rng.below(v.len())])
}

/// One random command, valid most of the time and rejected otherwise (both are fine).
fn random_command(rng: &mut Rng, p: &Project) -> Command {
    let ids = ids_of(p);
    let track = pick(rng, &ids.tracks).unwrap_or(Id(1));
    let (rtrack, region) = pick(rng, &ids.regions).unwrap_or((track, Id(2)));
    let (nregion, note) = pick(rng, &ids.notes).unwrap_or((region, Id(3)));
    let tick = rng.below(40_000) as i64;
    let effects: Vec<_> = p
        .effects
        .iter()
        .flat_map(|c| c.effects.iter())
        .map(|e| (e.id, e.kind))
        .collect();
    let (eid, ekind) = pick(rng, &effects).unwrap_or((Id(5), EffectKind::Delay));
    let kinds = [
        EffectKind::StereoTool,
        EffectKind::Delay,
        EffectKind::Crusher,
        EffectKind::Gate,
        EffectKind::Waveshaper,
        EffectKind::Tidal,
    ];
    match rng.below(40) {
        34 => Command::AddEffect {
            id: rng.id(),
            owner: if rng.below(3) == 0 {
                EffectOwner::Master
            } else {
                EffectOwner::Track { track }
            },
            kind: kinds[rng.below(kinds.len())],
            index: None,
        },
        35 => Command::RemoveEffect { effect: eid },
        36 => Command::MoveEffect {
            effect: eid,
            index: rng.below(3),
        },
        37 => Command::SetEffectBypass {
            effect: eid,
            bypass: rng.below(2) == 0,
        },
        38 => {
            let specs = ekind.params();
            let spec = specs[rng.below(specs.len())];
            Command::SetEffectParam {
                effect: eid,
                param: spec.name.to_string(),
                value: i32::from(spec.min) + rng.below(5) as i32,
            }
        }
        0 => Command::AddTrack {
            id: rng.id(),
            name: format!("t{}", rng.below(99)),
            kind: TrackKind::Instrument,
            index: None,
        },
        1..=3 => Command::AddRegion {
            id: rng.id(),
            track,
            start: tick,
            length: 60_000,
            kind: RegionKind::Midi,
        },
        4..=14 => Command::AddNote {
            id: rng.id(),
            region,
            pitch: 30 + rng.below(60) as u8,
            velocity: 1 + rng.below(120) as u8,
            start: rng.below(50_000) as i64,
            duration: 10 + rng.below(100) as i64,
        },
        15 => Command::MoveNote {
            region: nregion,
            note,
            start: rng.below(50_000) as i64,
            pitch: 30 + rng.below(60) as u8,
        },
        16 => Command::ResizeNote {
            region: nregion,
            note,
            duration: 5 + rng.below(200) as i64,
        },
        17 => Command::SetNoteVelocity {
            region: nregion,
            note,
            velocity: 1 + rng.below(120) as u8,
        },
        18..=20 => Command::DeleteNote {
            region: nregion,
            note,
        },
        21 => Command::DeleteRegion { region },
        22 => Command::DeleteTrack { track },
        23 => Command::MoveRegion {
            region,
            start: tick,
        },
        24 => Command::ResizeRegion {
            region,
            length: 1_000 + rng.below(70_000) as i64,
        },
        25 => Command::MoveRegionToTrack {
            region,
            track: pick(rng, &ids.tracks).unwrap_or(rtrack),
        },
        26 => Command::SetTrackVolume {
            track,
            volume: rng.below(2000) as u16,
        },
        27 => Command::SetTempo {
            tick: (rng.below(8) as i64) * 3840,
            mbpm: 60_000 + rng.below(100_000) as u32,
        },
        28 => Command::SetMarker {
            tick,
            name: format!("m{}", rng.below(9)),
        },
        29 => Command::SetProjectName {
            name: format!("p{}", rng.below(9)),
        },
        30 => Command::AddNotes {
            region,
            notes: (0..1 + rng.below(40))
                .map(|_| composer_commands_note_spec(rng))
                .collect(),
        },
        31 => Command::DuplicateRegion {
            region,
            new_region: rng.id(),
            start: tick,
            track: None,
            note_ids: (0..400).map(|_| rng.id()).collect(),
        },
        32 => Command::DeleteNotes {
            region: nregion,
            notes: ids
                .notes
                .iter()
                .filter(|(r, _)| *r == nregion)
                .take(1 + rng.below(6))
                .map(|(_, n)| *n)
                .collect(),
        },
        _ => Command::SetMasterVolume {
            volume: rng.below(2000) as u16,
        },
    }
}

fn composer_commands_note_spec(rng: &mut Rng) -> NoteSpec {
    NoteSpec {
        id: rng.id(),
        pitch: 30 + rng.below(60) as u8,
        velocity: 1 + rng.below(120) as u8,
        start: rng.below(50_000) as i64,
        duration: 10 + rng.below(100) as i64,
    }
}

fn run_seed(seed: u64, steps: usize) -> bool {
    let mut saw_effects = false;
    let mut rng = Rng(seed | 1);
    let mut ed = Editor::new(Project::new(Id(1), "prop"));
    // A few tracks and regions up front so notes land somewhere.
    for _ in 0..3 {
        let t = rng.id();
        ed.execute(&Command::AddTrack {
            id: t,
            name: "t".into(),
            kind: TrackKind::Instrument,
            index: None,
        })
        .unwrap();
        for _ in 0..2 {
            ed.execute(&Command::AddRegion {
                id: rng.id(),
                track: t,
                start: 0,
                length: 60_000,
                kind: RegionKind::Midi,
            })
            .unwrap();
        }
    }
    for step in 0..steps {
        match rng.below(100) {
            0..=54 => {
                let c = random_command(&mut rng, ed.project());
                let _ = ed.execute(&c);
            }
            55..=64 => {
                let _ = ed.undo();
            }
            65..=71 => {
                let _ = ed.redo();
            }
            72..=78 => {
                // Group that fails half of the time (a missing target), so it rolls back.
                let fail = rng.below(2) == 0;
                let cmds: Vec<Command> = (0..1 + rng.below(4))
                    .map(|_| random_command(&mut rng, ed.project()))
                    .collect();
                let _ = ed.group("g", |e| {
                    for c in &cmds {
                        let _ = e.execute(c);
                    }
                    if fail {
                        e.execute(&Command::DeleteTrack { track: Id(999) })?;
                    }
                    Ok(())
                });
            }
            79..=84 => {
                ed.begin_group("manual").unwrap();
                for _ in 0..1 + rng.below(4) {
                    let c = random_command(&mut rng, ed.project());
                    let _ = ed.execute(&c);
                }
                if rng.below(2) == 0 {
                    ed.rollback_group().unwrap();
                } else {
                    ed.commit_group().unwrap();
                }
            }
            85..=89 => {
                let _ = ed.undo_steps(1 + rng.below(5));
            }
            _ => {
                let _ = ed.redo_steps(1 + rng.below(5));
            }
        }
        saw_effects |= !ed.project().effects.is_empty();
        // Skipping some checks lets dirty marks pile up between hash calls.
        if rng.below(10) < 6 || step + 1 == steps {
            assert_eq!(
                ed.state_hash(),
                ed.project().state_hash(),
                "seed {seed} step {step}"
            );
        }
    }
    assert_eq!(ed.state_hash(), ed.project().state_hash());
    saw_effects
}

#[test]
fn incremental_hash_equals_full_hash_after_random_runs() {
    let mut saw_effects = false;
    for seed in 1..=12u64 {
        saw_effects |= run_seed(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 700);
    }
    assert!(saw_effects, "the run never created an effect");
}

#[test]
fn incremental_hash_matches_after_undo_all_and_redo_all() {
    let mut rng = Rng(77);
    let mut ed = Editor::new(Project::new(Id(1), "prop"));
    let base = ed.state_hash();
    let t = rng.id();
    ed.execute(&Command::AddTrack {
        id: t,
        name: "t".into(),
        kind: TrackKind::Instrument,
        index: None,
    })
    .unwrap();
    let r = rng.id();
    ed.execute(&Command::AddRegion {
        id: r,
        track: t,
        start: 0,
        length: 60_000,
        kind: RegionKind::Midi,
    })
    .unwrap();
    for _ in 0..300 {
        let n = composer_commands_note_spec(&mut rng);
        ed.execute(&Command::AddNote {
            id: n.id,
            region: r,
            pitch: n.pitch,
            velocity: n.velocity,
            start: n.start,
            duration: n.duration,
        })
        .unwrap();
        let _ = ed.state_hash();
    }
    let full = ed.project().state_hash();
    assert_eq!(ed.state_hash(), full);
    while ed.undo().unwrap() {}
    assert_eq!(ed.state_hash(), base);
    assert_eq!(base, Project::new(Id(1), "prop").state_hash());
    while ed.redo().unwrap() {}
    assert_eq!(ed.state_hash(), full);
}
