//! The measurements. Each returns plain numbers; `report` turns them into YAML.

use crate::gen::{empty_editor, generate, run, Ids, Picker, REGION_TICKS};
use crate::spec::{RenderSpec, Spec};
use composer_commands::{Command, Editor};
use composer_format::{peek, read_project, write_project, AssetTable, Limits, Meta};
use composer_model::{IdGen, Project, RegionKind, TrackKind};
use composer_render::{render_project, RenderSettings};
use std::hint::black_box;
use std::time::Instant;

/// Minimum of three runs, which filters out scheduler noise on a busy machine.
fn best_of(mut f: impl FnMut() -> u128) -> u128 {
    (0..3).map(|_| f()).min().unwrap_or(0)
}

fn ns_per(total_ns: u128, n: u64) -> f64 {
    total_ns as f64 / n.max(1) as f64
}

#[derive(Debug, Clone, Default)]
pub struct Counts {
    pub tracks: u64,
    pub regions: u64,
    pub notes: u64,
    pub commands: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Apply {
    pub counts: Counts,
    pub total_ms: f64,
    pub ns_per_command: f64,
}

#[derive(Debug, Clone, Default)]
pub struct UndoRedo {
    pub depth: u64,
    pub undo_ns_per_step: f64,
    pub redo_ns_per_step: f64,
    pub full_history_steps: u64,
    /// Undo cost of the newest decile of the history versus the oldest decile.
    pub full_undo_ns_first_decile: f64,
    pub full_undo_ns_last_decile: f64,
    pub full_redo_ms: f64,
    pub hash_restored: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Persist {
    pub bytes: u64,
    pub save_ms: f64,
    pub load_ms: f64,
    pub peek_ms: f64,
    pub roundtrip_equal: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Render {
    pub tracks: u64,
    pub notes: u64,
    pub audio_seconds: f64,
    pub render_ms: f64,
    /// Render time over audio time. Below 1.0 is faster than real time.
    pub rtf: f64,
    pub sample_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ScalePoint {
    pub size: u64,
    pub lookup_ns: f64,
    pub apply_ns: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Scaling {
    /// Swept by region count, one note per region.
    pub by_regions: Vec<ScalePoint>,
    /// Swept by track count: track lookup and front insert plus delete.
    pub by_tracks: Vec<ScalePoint>,
}

impl Scaling {
    fn ratio(points: &[ScalePoint], f: fn(&ScalePoint) -> f64) -> f64 {
        match (points.first(), points.last()) {
            (Some(a), Some(b)) if f(a) > 0.0 => f(b) / f(a),
            _ => 0.0,
        }
    }
    pub fn lookup_ratio(&self) -> f64 {
        Self::ratio(&self.by_regions, |p| p.lookup_ns)
    }
    pub fn apply_ratio(&self) -> f64 {
        Self::ratio(&self.by_regions, |p| p.apply_ns)
    }
    pub fn track_lookup_ratio(&self) -> f64 {
        Self::ratio(&self.by_tracks, |p| p.lookup_ns)
    }
    pub fn track_apply_ratio(&self) -> f64 {
        Self::ratio(&self.by_tracks, |p| p.apply_ns)
    }
}

/// Builds the stress project and times the command path.
pub fn apply(spec: &Spec) -> (Editor, Ids, Apply) {
    let mut gen = IdGen::new(spec.seed);
    let mut ed = empty_editor(&mut gen);
    let t = Instant::now();
    let ids = generate(&mut ed, &mut gen, spec);
    let el = t.elapsed().as_nanos();
    let commands = ed.undo_depth() as u64;
    let counts = Counts {
        tracks: ids.tracks.len() as u64,
        regions: ids.regions.len() as u64,
        notes: ids.notes.len() as u64,
        commands,
    };
    let a = Apply {
        total_ms: el as f64 / 1e6,
        ns_per_command: ns_per(el, commands),
        counts,
    };
    (ed, ids, a)
}

/// Cycles the newest `depth` steps through undo and redo, then undoes and
/// redoes the whole history. The editor ends where it started.
pub fn undo_redo(ed: &mut Editor, depth: u64) -> UndoRedo {
    let before = ed.project().state_hash();
    let total = ed.undo_depth() as u64;
    let depth = depth.min(total);
    let t = Instant::now();
    for _ in 0..depth {
        assert!(ed.undo().expect("undo"));
    }
    let undo_ns = t.elapsed().as_nanos();
    let t = Instant::now();
    for _ in 0..depth {
        assert!(ed.redo().expect("redo"));
    }
    let redo_ns = t.elapsed().as_nanos();

    let decile = (total / 10).max(1);
    let (mut first, mut last, mut first_n, mut last_n) = (0u128, 0u128, 0u64, 0u64);
    for i in 0..total {
        let t = Instant::now();
        assert!(ed.undo().expect("undo"));
        let e = t.elapsed().as_nanos();
        if i < decile {
            first += e;
            first_n += 1;
        } else if i >= total - decile {
            last += e;
            last_n += 1;
        }
    }
    let t = Instant::now();
    for _ in 0..total {
        assert!(ed.redo().expect("redo"));
    }
    let full_redo = t.elapsed().as_nanos();
    UndoRedo {
        depth,
        undo_ns_per_step: ns_per(undo_ns, depth),
        redo_ns_per_step: ns_per(redo_ns, depth),
        full_history_steps: total,
        full_undo_ns_first_decile: ns_per(first, first_n),
        full_undo_ns_last_decile: ns_per(last, last_n),
        full_redo_ms: full_redo as f64 / 1e6,
        hash_restored: ed.project().state_hash() == before,
    }
}

/// Median of `reps` timings of `state_hash`, in milliseconds.
pub fn state_hash_ms(p: &Project, reps: usize) -> f64 {
    let mut v = Vec::new();
    for _ in 0..reps.max(1) {
        let t = Instant::now();
        black_box(p.state_hash());
        v.push(t.elapsed().as_nanos() as f64 / 1e6);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub fn persist(p: &Project) -> Persist {
    let meta = Meta {
        app_version: "bench".into(),
        created_ms: 0,
        modified_ms: 0,
        transport: None,
    };
    let assets = AssetTable::new();
    let t = Instant::now();
    let bytes = write_project(p, &assets, &meta);
    let save = t.elapsed();
    let limits = Limits {
        max_tracks: u64::MAX,
        max_regions: u64::MAX,
        max_notes: u64::MAX,
        ..Limits::default()
    };
    let t = Instant::now();
    let loaded = read_project(&bytes, &limits).expect("read_project");
    let load = t.elapsed();
    let t = Instant::now();
    black_box(peek(&bytes).expect("peek"));
    let peeked = t.elapsed();
    Persist {
        bytes: bytes.len() as u64,
        save_ms: save.as_secs_f64() * 1e3,
        load_ms: load.as_secs_f64() * 1e3,
        peek_ms: peeked.as_secs_f64() * 1e3,
        roundtrip_equal: loaded.project.state_hash() == p.state_hash(),
    }
}

/// Renders a project of `spec.tracks` tracks covering `spec.seconds` at the
/// default 120 bpm, one note per beat on every track.
pub fn render(spec: &RenderSpec, seed: u64) -> Render {
    let mut gen = IdGen::new(seed);
    let mut ed = empty_editor(&mut gen);
    // 120 bpm: a region of REGION_TICKS lasts 8 seconds.
    let regions = spec.seconds.div_ceil(8).max(1);
    let beats_per_region = 16;
    let mut notes = 0;
    for t in 0..spec.tracks {
        let track = gen.next_id();
        run(
            &mut ed,
            Command::AddTrack {
                id: track,
                name: format!("r{t}"),
                kind: TrackKind::Instrument,
                index: None,
            },
        );
        for r in 0..regions {
            let region = gen.next_id();
            run(
                &mut ed,
                Command::AddRegion {
                    id: region,
                    track,
                    start: r as i64 * REGION_TICKS,
                    length: REGION_TICKS,
                    kind: RegionKind::Midi,
                },
            );
            for b in 0..beats_per_region {
                run(
                    &mut ed,
                    Command::AddNote {
                        id: gen.next_id(),
                        region,
                        pitch: 40 + ((b + t) % 36) as u8,
                        velocity: 80,
                        start: b as i64 * 960,
                        duration: 480,
                    },
                );
                notes += 1;
            }
        }
    }
    let settings = RenderSettings::new(spec.sample_rate, 1.0);
    let t = Instant::now();
    let out = render_project(ed.project(), &settings);
    let el = t.elapsed().as_secs_f64();
    let audio = out.frames as f64 / f64::from(out.sample_rate);
    Render {
        tracks: spec.tracks,
        notes,
        audio_seconds: audio,
        render_ms: el * 1e3,
        rtf: el / audio.max(f64::MIN_POSITIVE),
        sample_bytes: out.samples.len() as u64 * 4,
    }
}

fn sized_by_regions(regions: u64, seed: u64) -> (Editor, Ids) {
    let spec = Spec {
        tracks: regions.div_ceil(100),
        regions_per_track: regions.min(100),
        notes_per_region: 1,
        undo_depth: 0,
        seed,
    };
    let mut gen = IdGen::new(seed);
    let mut ed = empty_editor(&mut gen);
    let ids = generate(&mut ed, &mut gen, &spec);
    (ed, ids)
}

/// Sweeps project size and times id lookups and one applied command at each size.
/// Flat curves mean the index is doing its job; a rising curve means a scan.
pub fn scaling(sizes_regions: &[u64], sizes_tracks: &[u64], lookups: u64) -> Scaling {
    let mut out = Scaling::default();
    for &n in sizes_regions {
        let (mut ed, ids) = sized_by_regions(n, 7);
        let mut pick = Picker::new(99);
        let lookup_el = best_of(|| {
            let t = Instant::now();
            let mut hits = 0u64;
            for _ in 0..lookups {
                let (tr, rg) = ids.regions[pick.below(ids.regions.len())];
                let (nr, nn) = ids.notes[pick.below(ids.notes.len())];
                hits += u64::from(ed.project().track(tr).is_some())
                    + u64::from(ed.project().region(rg).is_some())
                    + u64::from(ed.project().note(nr, nn).is_some());
            }
            assert_eq!(hits, lookups * 3);
            t.elapsed().as_nanos()
        });
        let (_, region) = ids.regions[0];
        let mut gen = IdGen::new(5);
        let reps = lookups.min(20_000);
        let t = Instant::now();
        for i in 0..reps {
            run(
                &mut ed,
                Command::AddNote {
                    id: gen.next_id(),
                    region,
                    pitch: 60,
                    velocity: 90,
                    start: (i % 8) as i64 * 100,
                    duration: 50,
                },
            );
        }
        let apply_el = t.elapsed().as_nanos();
        out.by_regions.push(ScalePoint {
            size: ids.regions.len() as u64,
            lookup_ns: ns_per(lookup_el, lookups * 3),
            apply_ns: ns_per(apply_el, reps),
        });
    }
    for &n in sizes_tracks {
        let mut gen = IdGen::new(11);
        let mut ed = empty_editor(&mut gen);
        let mut tracks = Vec::new();
        for i in 0..n {
            let id = gen.next_id();
            run(
                &mut ed,
                Command::AddTrack {
                    id,
                    name: format!("t{i}"),
                    kind: TrackKind::Instrument,
                    index: None,
                },
            );
            tracks.push(id);
        }
        let mut pick = Picker::new(3);
        let lookup_el = best_of(|| {
            let t = Instant::now();
            let mut hits = 0u64;
            for _ in 0..lookups {
                let id = tracks[pick.below(tracks.len())];
                hits += u64::from(ed.project().track_index(id).is_some());
            }
            assert_eq!(hits, lookups);
            t.elapsed().as_nanos()
        });
        // Insert at the front shifts every position; delete it again to keep the size.
        let reps = 200u64;
        let apply_el = best_of(|| {
            let t = Instant::now();
            for _ in 0..reps {
                let id = gen.next_id();
                run(
                    &mut ed,
                    Command::AddTrack {
                        id,
                        name: "front".into(),
                        kind: TrackKind::Instrument,
                        index: Some(0),
                    },
                );
                run(&mut ed, Command::DeleteTrack { track: id });
            }
            t.elapsed().as_nanos()
        });
        out.by_tracks.push(ScalePoint {
            size: n,
            lookup_ns: ns_per(lookup_el, lookups),
            apply_ns: ns_per(apply_el, reps * 2),
        });
    }
    out
}
