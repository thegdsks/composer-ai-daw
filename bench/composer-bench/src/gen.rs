//! Stress project generator. Every edit goes through `Editor::execute`, the
//! same path the CLI, MCP server and UI use.

use crate::spec::Spec;
use composer_commands::{Command, Editor};
use composer_model::{Id, IdGen, Project, RegionKind, Tick, TrackKind, TICKS_PER_QUARTER};

/// Four 4/4 bars.
pub const REGION_TICKS: Tick = 16 * TICKS_PER_QUARTER;

/// Ids of everything created, in creation order, for later lookups and edits.
#[derive(Debug, Default, Clone)]
pub struct Ids {
    pub tracks: Vec<Id>,
    /// `(track, region)`.
    pub regions: Vec<(Id, Id)>,
    /// `(region, note)`.
    pub notes: Vec<(Id, Id)>,
}

pub fn empty_editor(gen: &mut IdGen) -> Editor {
    Editor::new(Project::new(gen.next_id(), "bench"))
}

/// Fills `ed` with `spec.tracks` tracks, each holding `regions_per_track`
/// back to back regions of `notes_per_region` notes. Returns every id created.
/// Panics on a command error: the generator only issues valid commands.
pub fn generate(ed: &mut Editor, gen: &mut IdGen, spec: &Spec) -> Ids {
    let mut ids = Ids::default();
    let step = REGION_TICKS / spec.notes_per_region.max(1) as Tick;
    for t in 0..spec.tracks {
        let track = gen.next_id();
        run(
            ed,
            Command::AddTrack {
                id: track,
                name: format!("track {t}"),
                kind: TrackKind::Instrument,
                index: None,
            },
        );
        ids.tracks.push(track);
        for r in 0..spec.regions_per_track {
            let region = gen.next_id();
            run(
                ed,
                Command::AddRegion {
                    id: region,
                    track,
                    start: r as Tick * REGION_TICKS,
                    length: REGION_TICKS,
                    kind: RegionKind::Midi,
                },
            );
            ids.regions.push((track, region));
            for n in 0..spec.notes_per_region {
                let note = gen.next_id();
                let start = n as Tick * step;
                run(
                    ed,
                    Command::AddNote {
                        id: note,
                        region,
                        pitch: 36 + ((n + t) % 48) as u8,
                        velocity: 1 + ((n * 7) % 127) as u8,
                        start,
                        duration: step.max(1),
                    },
                );
                ids.notes.push((region, note));
            }
        }
    }
    ids
}

pub fn run(ed: &mut Editor, cmd: Command) {
    if let Err(e) = ed.execute(&cmd) {
        panic!("generator issued a rejected command {}: {e:?}", cmd.name());
    }
}

/// Seeded xorshift index picker so lookups hit scattered entries.
pub struct Picker(u64);

impl Picker {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}
