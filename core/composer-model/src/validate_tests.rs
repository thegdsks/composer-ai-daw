//! Tests for `Project::validate`: one failing input per rule, shared rules with
//! `apply`, random valid projects, and a single pass at scale.

use crate::{
    Change, FieldKey, FieldValue, Id, IdGen, Note, Project, Region, RegionContent, TimeSignature,
    Track, TrackKind, ValidationError, MAX_NAME_LEN, MAX_TICK,
};

fn id(n: u128) -> Id {
    Id(n)
}

fn note(n: u128) -> Note {
    Note {
        id: id(n),
        pitch: 60,
        velocity: 100,
        start: 0,
        duration: 100,
    }
}

fn midi(rid: u128, notes: Vec<Note>) -> Region {
    Region {
        loop_length: 0,
        id: id(rid),
        start: 0,
        length: 1000,
        content: RegionContent::Midi {
            notes: notes.into_iter().map(|n| (n.id, n)).collect(),
        },
    }
}

fn track(tid: u128, kind: TrackKind, regions: Vec<Region>) -> Track {
    Track {
        id: id(tid),
        name: "t".into(),
        kind,
        instrument: None,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: regions.into_iter().map(|r| (r.id, r)).collect(),
    }
}

fn small() -> Project {
    let mut p = Project::new(id(1), "p");
    let t = track(2, TrackKind::Instrument, vec![midi(3, vec![note(4)])]);
    p.apply(&Change::NewTrack { index: 0, track: t }).unwrap();
    p
}

/// Edits the document directly, then rebuilds the index so only the rule under test fails.
fn broken(f: impl FnOnce(&mut Project)) -> ValidationError {
    let mut p = small();
    f(&mut p);
    p.reindex();
    p.validate().unwrap_err()
}

fn region_mut(p: &mut Project) -> &mut Region {
    p.tracks[0].regions.get_mut(&id(3)).unwrap()
}

fn notes_mut(p: &mut Project) -> &mut std::collections::BTreeMap<Id, Note> {
    let RegionContent::Midi { notes } = &mut region_mut(p).content else {
        unreachable!()
    };
    notes
}

fn note_mut(p: &mut Project) -> &mut Note {
    notes_mut(p).get_mut(&id(4)).unwrap()
}

#[test]
fn valid_project_passes() {
    assert_eq!(small().validate(), Ok(()));
    assert_eq!(Project::new(id(1), "").validate(), Ok(()));
}

#[test]
fn every_rule_has_a_failing_input_and_a_stable_code() {
    type Edit = fn(&mut Project);
    let cases: Vec<(&str, Edit)> = vec![
        ("name_length", |p| p.name = "x".repeat(MAX_NAME_LEN + 1)),
        ("name_length", |p| {
            p.tracks[0].name = "x".repeat(MAX_NAME_LEN + 1)
        }),
        ("pitch_range", |p| note_mut(p).pitch = 128),
        ("velocity_range", |p| note_mut(p).velocity = 0),
        ("velocity_range", |p| note_mut(p).velocity = 128),
        ("note_start_range", |p| note_mut(p).start = -1),
        ("note_start_range", |p| note_mut(p).start = MAX_TICK + 1),
        ("note_duration_range", |p| note_mut(p).duration = 0),
        ("note_outside_region", |p| note_mut(p).start = 950),
        ("region_start_range", |p| region_mut(p).start = -5),
        ("region_length_range", |p| region_mut(p).length = 0),
        ("tempo_range", |p| {
            p.tempo_map.tempos.insert(960, 5);
        }),
        ("tempo_tick_range", |p| {
            p.tempo_map.tempos.insert(-1, 120_000);
        }),
        ("tempo_tick_range", |p| {
            p.tempo_map
                .meters
                .insert(MAX_TICK + 1, TimeSignature::COMMON);
        }),
        ("meter_invalid", |p| {
            p.tempo_map.meters.insert(
                960,
                TimeSignature {
                    numerator: 4,
                    denominator: 3,
                },
            );
        }),
        ("tempo_origin_missing", |p| {
            p.tempo_map.tempos.remove(&0);
        }),
        ("tempo_origin_missing", |p| {
            p.tempo_map.meters.remove(&0);
        }),
        ("content_kind_mismatch", |p| {
            p.tracks[0].kind = TrackKind::Audio
        }),
        ("content_kind_mismatch", |p| {
            let r = Region {
                loop_length: 0,
                id: id(9),
                start: 0,
                length: 10,
                content: RegionContent::Audio,
            };
            p.tracks[0].regions.insert(r.id, r);
        }),
        ("region_key_mismatch", |p| {
            let r = p.tracks[0].regions.remove(&id(3)).unwrap();
            p.tracks[0].regions.insert(id(77), r);
        }),
        ("note_key_mismatch", |p| {
            let n = notes_mut(p).remove(&id(4)).unwrap();
            notes_mut(p).insert(id(88), n);
        }),
        ("duplicate_id", |p| p.tracks[0].id = id(1)),
        ("duplicate_id", |p| {
            let mut t = p.tracks[0].clone();
            t.regions.clear();
            p.tracks.push(t);
        }),
        ("duplicate_id", |p| {
            let t = track(5, TrackKind::Instrument, vec![midi(6, vec![note(4)])]);
            p.tracks.push(t);
        }),
    ];
    for (i, (code, f)) in cases.into_iter().enumerate() {
        let e = broken(f);
        assert_eq!(e.code(), code, "case {i}: {e}");
    }
}

#[test]
fn stale_index_is_reported_and_locator_names_the_spot() {
    let mut p = small();
    p.tracks.push(track(7, TrackKind::Instrument, vec![]));
    assert_eq!(p.validate().unwrap_err().code(), "index_inconsistent");
    p.reindex();
    assert_eq!(p.validate(), Ok(()));

    let e = broken(|p| note_mut(p).pitch = 200);
    assert_eq!(e.at.track_index, Some(0));
    assert_eq!(e.at.region, Some(id(3)));
    assert_eq!(e.at.note, Some(id(4)));
    assert!(e.to_string().starts_with("project.tracks[0].regions["));
}

#[test]
fn apply_and_validate_agree_on_the_same_values() {
    let (r, n) = (id(3), id(4));
    let rejected = [
        (
            FieldKey::NotePitch { region: r, note: n },
            FieldValue::Byte(128),
            "pitch_range",
        ),
        (
            FieldKey::NoteVelocity { region: r, note: n },
            FieldValue::Byte(0),
            "velocity_range",
        ),
        (
            FieldKey::NoteStart { region: r, note: n },
            FieldValue::Tick(990),
            "note_outside_region",
        ),
        (
            FieldKey::NoteDuration { region: r, note: n },
            FieldValue::Tick(0),
            "note_duration_range",
        ),
        (
            FieldKey::RegionLength { region: r },
            FieldValue::Tick(50),
            "note_outside_region",
        ),
        (
            FieldKey::RegionStart { region: r },
            FieldValue::Tick(-1),
            "region_start_range",
        ),
    ];
    for (key, new, code) in rejected {
        let mut q = small();
        let old = q.get(&key).unwrap();
        let c = Change::Set {
            key: key.clone(),
            old,
            new: new.clone(),
        };
        assert!(q.apply(&c).is_err(), "{key:?}");
        // Forcing the value in below the rules must fail validate with the matching code.
        q.put(&key, new).unwrap();
        assert_eq!(q.validate().unwrap_err().code(), code, "{key:?}");
    }
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

#[test]
fn projects_built_by_changes_always_validate() {
    for seed in [3u64, 11, 0xBEEF] {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut ids = IdGen::new(seed);
        let mut p = Project::new(ids.next_id(), "fuzz");
        let mut accepted = 0;
        for _ in 0..1500 {
            let tracks: Vec<Id> = p.tracks.iter().map(|t| t.id).collect();
            let regions: Vec<Id> = p
                .tracks
                .iter()
                .flat_map(|t| t.regions.keys().copied())
                .collect();
            let region = regions.get(rng.below(regions.len().max(1) as u64) as usize);
            let c = match rng.below(5) {
                0 => Change::NewTrack {
                    index: rng.below(tracks.len() as u64 + 1) as usize,
                    track: track(ids.next_id().0, TrackKind::Instrument, vec![]),
                },
                1 if !tracks.is_empty() => Change::NewRegion {
                    track: tracks[rng.below(tracks.len() as u64) as usize],
                    region: midi(ids.next_id().0, vec![]),
                },
                2 if region.is_some() => Change::NewNote {
                    region: *region.unwrap(),
                    note: Note {
                        id: ids.next_id(),
                        pitch: rng.below(130) as u8,
                        velocity: rng.below(130) as u8,
                        start: rng.below(1100) as i64,
                        duration: rng.below(300) as i64,
                    },
                },
                3 => {
                    let tick = (rng.below(8) * 960) as i64;
                    Change::SetTempo {
                        tick,
                        old: p.tempo_map.tempos.get(&tick).copied(),
                        new: Some(rng.below(500_000) as u32),
                    }
                }
                4 if region.is_some() => {
                    let key = FieldKey::RegionLength {
                        region: *region.unwrap(),
                    };
                    Change::Set {
                        old: p.get(&key).unwrap(),
                        key,
                        new: FieldValue::Tick(rng.below(1200) as i64),
                    }
                }
                _ => continue,
            };
            if p.apply(&c).is_ok() {
                accepted += 1;
                assert_eq!(p.validate(), Ok(()), "{c:?}");
            }
        }
        assert!(accepted > 300, "accepted {accepted}");
        let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.validate(), Ok(()));
    }
}

#[test]
fn scale_project_validates_in_one_pass() {
    let (tracks, regions, notes) = (200u128, 50u128, 20u128);
    let mut next = 10u128;
    let mut p = Project::new(id(1), "big");
    for ti in 0..tracks {
        let mut rs = Vec::new();
        for _ in 0..regions {
            next += 1;
            let rid = next;
            let ns = (0..notes)
                .map(|k| {
                    next += 1;
                    let mut n = note(next);
                    n.start = (k * 40) as i64;
                    n
                })
                .collect();
            rs.push(midi(rid, ns));
        }
        next += 1;
        let t = track(next, TrackKind::Instrument, rs);
        p.apply(&Change::NewTrack {
            index: ti as usize,
            track: t,
        })
        .unwrap();
    }
    let (r, visits) = p.validate_counted();
    assert_eq!(r, Ok(()));
    // project + 1 tempo + 1 meter, then every track, region and note exactly once.
    let expected = 3 + tracks + tracks * regions + tracks * regions * notes;
    assert_eq!(visits, expected as u64);
}

#[test]
fn track_positions_stay_exact_after_inserts_and_deletes() {
    let mut p = Project::new(id(1), "p");
    let mut order: Vec<u128> = Vec::new();
    for k in 0..40u128 {
        let at = (k as usize * 7) % (order.len() + 1);
        let t = track(100 + k, TrackKind::Instrument, vec![]);
        p.apply(&Change::NewTrack {
            index: at,
            track: t,
        })
        .unwrap();
        order.insert(at, 100 + k);
        if k % 3 == 2 {
            let at = (k as usize * 5) % order.len();
            let gone = order.remove(at);
            let t = p.tracks[at].clone();
            assert_eq!(t.id, id(gone));
            p.apply(&Change::DeleteTrack {
                index: at,
                track: t,
            })
            .unwrap();
        }
        // Lookups interleaved with edits exercise the on demand repair.
        let probe = order[order.len() / 2];
        assert_eq!(p.track_index(id(probe)), Some(order.len() / 2));
    }
    for (i, t) in order.iter().enumerate() {
        assert_eq!(p.track_index(id(*t)), Some(i));
        assert_eq!(p.track(id(*t)).map(|t| t.id), Some(id(*t)));
    }
    assert!(p.index_is_consistent());
    assert_eq!(p.validate(), Ok(()));
}

#[test]
fn bus_values_out_of_range_fail_validation() {
    let mut p = Project::new(Id(1), "bus");
    p.master.reverb_mix = 1001;
    assert!(p.validate().is_err());
    p.master.reverb_mix = 0;
    assert!(p.validate().is_ok());
}

#[test]
fn default_bus_keeps_the_hash() {
    let mut p = Project::new(Id(1), "bus");
    let base = p.state_hash();
    p.master.reverb_mix = 100;
    assert_ne!(p.state_hash(), base);
    p.master.reverb_mix = 0;
    assert_eq!(p.state_hash(), base);
}
