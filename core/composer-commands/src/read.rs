//! Read side shared by every client (D17): session selection and the compact
//! project tree. Nothing here edits the project.

use crate::command::MAX_BULK_NOTES;
use crate::error::CommandError;
use composer_model::{
    ChannelMix, Id, Project, RegionContent, RegionKind, Tick, TimeSignature, TrackKind,
    DEFAULT_MBPM,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A note named together with its clip, since note ids are only found through a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteRef {
    pub region: Id,
    pub note: Id,
}

/// What the user has selected. Session state: not undoable, not in `state_hash`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    pub tracks: Vec<Id>,
    pub regions: Vec<Id>,
    pub notes: Vec<NoteRef>,
}

fn bad(field: &'static str, reason: &'static str) -> CommandError {
    CommandError::Invalid { field, reason }
}

impl Selection {
    /// Checks every id against the project: it must exist, be of the right kind,
    /// and appear once. Fails without side effects.
    pub fn validate(&self, p: &Project) -> Result<(), CommandError> {
        if self.tracks.len() + self.regions.len() + self.notes.len() > MAX_BULK_NOTES {
            return Err(bad("selection", "too many ids"));
        }
        let mut seen = HashSet::new();
        for id in &self.tracks {
            if !seen.insert(*id) {
                return Err(bad("tracks", "repeated id"));
            }
            p.track(*id).ok_or(CommandError::NotFound {
                what: "track",
                id: *id,
            })?;
        }
        for id in &self.regions {
            if !seen.insert(*id) {
                return Err(bad("regions", "repeated id"));
            }
            p.region(*id).ok_or(CommandError::NotFound {
                what: "region",
                id: *id,
            })?;
        }
        for n in &self.notes {
            if !seen.insert(n.note) {
                return Err(bad("notes", "repeated id"));
            }
            p.note(n.region, n.note).ok_or(CommandError::NotFound {
                what: "note",
                id: n.note,
            })?;
        }
        Ok(())
    }

    /// The same selection without ids that no longer exist (after a delete or undo).
    pub fn pruned(&self, p: &Project) -> Selection {
        Selection {
            tracks: self
                .tracks
                .iter()
                .copied()
                .filter(|id| p.track(*id).is_some())
                .collect(),
            regions: self
                .regions
                .iter()
                .copied()
                .filter(|id| p.region(*id).is_some())
                .collect(),
            notes: self
                .notes
                .iter()
                .copied()
                .filter(|n| p.note(n.region, n.note).is_some())
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.regions.is_empty() && self.notes.is_empty()
    }
}

/// What the tree includes.
#[derive(Debug, Clone, Copy, Default)]
pub struct TreeOptions {
    /// List every note of every shown clip.
    pub notes: bool,
    /// Show only this track.
    pub track: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteNode {
    pub id: Id,
    pub pitch: u8,
    pub velocity: u8,
    pub start: Tick,
    pub duration: Tick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionNode {
    pub id: Id,
    pub start: Tick,
    pub length: Tick,
    pub kind: RegionKind,
    pub note_count: usize,
    pub pitch_min: Option<u8>,
    pub pitch_max: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<NoteNode>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackNode {
    pub id: Id,
    pub name: String,
    pub kind: TrackKind,
    pub instrument: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    pub mix: ChannelMix,
    pub region_count: usize,
    pub note_count: usize,
    /// In start order.
    pub regions: Vec<RegionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectTree {
    pub id: Id,
    pub name: String,
    /// Tempo at tick 0, thousandths of a BPM.
    pub tempo_mbpm: u32,
    pub tempo_changes: usize,
    pub time_signature: String,
    pub meter_changes: usize,
    /// End tick of the last clip.
    pub span_ticks: Tick,
    pub track_count: usize,
    pub tracks: Vec<TrackNode>,
}

/// The compact read of the project: tracks and clips with counts, notes on request.
pub fn project_tree(p: &Project, opts: TreeOptions) -> Result<ProjectTree, CommandError> {
    if let Some(id) = opts.track {
        p.track(id)
            .ok_or(CommandError::NotFound { what: "track", id })?;
    }
    let mut span = 0;
    let tracks = p
        .tracks
        .iter()
        .map(|t| {
            let mut regions: Vec<RegionNode> = t
                .regions
                .values()
                .map(|r| {
                    span = span.max(r.start + r.length);
                    let (count, lo, hi, notes) = match &r.content {
                        RegionContent::Audio | RegionContent::Clip { .. } => (0, None, None, None),
                        RegionContent::Midi { notes } => (
                            notes.len(),
                            notes.values().map(|n| n.pitch).min(),
                            notes.values().map(|n| n.pitch).max(),
                            opts.notes.then(|| {
                                let mut v: Vec<NoteNode> = notes
                                    .values()
                                    .map(|n| NoteNode {
                                        id: n.id,
                                        pitch: n.pitch,
                                        velocity: n.velocity,
                                        start: n.start,
                                        duration: n.duration,
                                    })
                                    .collect();
                                v.sort_by_key(|n| (n.start, n.pitch, n.id));
                                v
                            }),
                        ),
                    };
                    RegionNode {
                        id: r.id,
                        start: r.start,
                        length: r.length,
                        kind: r.kind(),
                        note_count: count,
                        pitch_min: lo,
                        pitch_max: hi,
                        notes,
                    }
                })
                .collect();
            regions.sort_by_key(|r| (r.start, r.id));
            let note_count = regions.iter().map(|r| r.note_count).sum();
            TrackNode {
                id: t.id,
                name: t.name.clone(),
                kind: t.kind,
                instrument: t.instrument,
                sound: t.sound.clone(),
                mix: t.mix,
                region_count: regions.len(),
                note_count,
                regions: if opts.track.is_none_or(|id| id == t.id) {
                    regions
                } else {
                    Vec::new()
                },
            }
        })
        .filter(|n| opts.track.is_none_or(|id| id == n.id))
        .collect();
    let tm = &p.tempo_map;
    let sig = tm.meters.get(&0).copied().unwrap_or(TimeSignature::COMMON);
    Ok(ProjectTree {
        id: p.id,
        name: p.name.clone(),
        tempo_mbpm: tm.tempos.get(&0).copied().unwrap_or(DEFAULT_MBPM),
        tempo_changes: tm.tempos.len().saturating_sub(1),
        time_signature: format!("{}/{}", sig.numerator, sig.denominator),
        meter_changes: tm.meters.len().saturating_sub(1),
        span_ticks: span,
        track_count: p.tracks.len(),
        tracks,
    })
}
