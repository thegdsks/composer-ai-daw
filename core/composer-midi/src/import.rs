use crate::error::MidiError;
use crate::reader::{parse, Limits, RawNote, RawTrack};
use composer_commands::Command;
use composer_model::{
    IdGen, RegionKind, Tick, TimeSignature, TrackKind, MAX_MBPM, MAX_NAME_LEN, MAX_TICK, MIN_MBPM,
    TICKS_PER_QUARTER,
};
use std::collections::BTreeMap;

/// Settings for [`import_smf`].
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Seed for the ids of new tracks, regions and notes (same seed, same ids).
    pub seed: u64,
    pub max_tracks: usize,
    pub max_notes: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            max_tracks: 4096,
            max_notes: 1 << 20,
        }
    }
}

/// What the importer had to repair or skip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportStats {
    pub notes: usize,
    /// Note offs with no open note, dropped.
    pub unmatched_note_offs: usize,
    /// Notes still open at the end of their track, closed there.
    pub unterminated_notes: usize,
}

/// Commands to apply as ONE group (for example `Editor::group`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    pub commands: Vec<Command>,
    pub stats: ImportStats,
}

/// `round_half_up(t * 960 / ppq)`, exact. Fails when the result passes the model limit.
pub(crate) fn rescale(t: u64, ppq: u16) -> Result<Tick, MidiError> {
    let p = u128::from(ppq);
    let v = (u128::from(t) * TICKS_PER_QUARTER as u128 * 2 + p) / (2 * p);
    if v >= MAX_TICK as u128 {
        return Err(MidiError::TickOverflow);
    }
    Ok(v as Tick)
}

/// Microseconds per quarter to milli bpm, rounded half up and clamped to the model range.
fn mbpm_from_us(us: u32) -> u32 {
    let us = u64::from(us);
    let v = (60_000_000_000u64 * 2 + us) / (2 * us);
    v.clamp(u64::from(MIN_MBPM), u64::from(MAX_MBPM)) as u32
}

fn clean_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    let mut end = s.len().min(MAX_NAME_LEN);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].trim_matches('\0').to_string()
}

/// Parses a Standard MIDI File (format 0 or 1) into commands.
///
/// Tracks: each MTrk gives one instrument track per channel that has notes. In a
/// format 1 file, a first MTrk without notes is the conductor and its name becomes
/// the project name; later MTrks without notes still make an empty track. Each
/// track with notes gets one midi region spanning them. Tempo and meter events
/// from every MTrk go to one map; at the same tick the later event wins. Notes
/// pair on and off per channel and pitch, first on first off. Unknown meta,
/// sysex and controller events are skipped; known meta with a wrong length is ignored.
pub fn import_smf(bytes: &[u8], opts: &ImportOptions) -> Result<ImportPlan, MidiError> {
    let smf = parse(
        bytes,
        &Limits {
            max_tracks: opts.max_tracks,
            max_notes: opts.max_notes,
        },
    )?;
    let mut ids = IdGen::new(opts.seed);
    let mut cmds = Vec::new();
    let mut stats = ImportStats::default();

    let mut tempos: BTreeMap<Tick, u32> = BTreeMap::new();
    let mut meters: BTreeMap<Tick, (u8, u8)> = BTreeMap::new();
    for t in &smf.tracks {
        for &(tick, us) in &t.tempos {
            tempos.insert(rescale(tick, smf.ppq)?, mbpm_from_us(us));
        }
        for &(tick, nn, dd) in &t.meters {
            if dd >= 8 {
                continue;
            }
            let sig = TimeSignature {
                numerator: nn,
                denominator: 1u8 << dd,
            };
            if sig.is_valid() {
                meters.insert(rescale(tick, smf.ppq)?, (sig.numerator, sig.denominator));
            }
        }
        stats.unmatched_note_offs += t.unmatched_offs;
        stats.unterminated_notes += t.unterminated;
    }

    if let Some(first) = smf.tracks.first() {
        if smf.format == 1 && first.notes.is_empty() {
            if let Some(name) = first
                .name
                .as_deref()
                .map(clean_name)
                .filter(|n| !n.is_empty())
            {
                cmds.push(Command::SetProjectName { name });
            }
        }
    }
    cmds.extend(
        tempos
            .into_iter()
            .map(|(tick, mbpm)| Command::SetTempo { tick, mbpm }),
    );
    cmds.extend(meters.into_iter().map(|(tick, (numerator, denominator))| {
        Command::SetTimeSignature {
            tick,
            numerator,
            denominator,
        }
    }));

    let mut made = 0usize;
    for (i, t) in smf.tracks.iter().enumerate() {
        let skip = t.notes.is_empty() && !(smf.format == 1 && i > 0);
        if skip {
            continue;
        }
        let base = track_base_name(t, i);
        if t.notes.is_empty() {
            push_track(
                &mut cmds,
                &mut ids,
                &mut made,
                opts,
                base.clone(),
                Vec::new(),
                smf.ppq,
            )?;
        }
        let multi = t.notes.len() > 1;
        for (ch, notes) in &t.notes {
            let name = if multi {
                format!("{base} ch{}", ch + 1)
            } else {
                base.clone()
            };
            stats.notes += notes.len();
            push_track(
                &mut cmds,
                &mut ids,
                &mut made,
                opts,
                name,
                sorted(notes),
                smf.ppq,
            )?;
        }
    }
    Ok(ImportPlan {
        commands: cmds,
        stats,
    })
}

fn track_base_name(t: &RawTrack, index: usize) -> String {
    t.name
        .as_deref()
        .map(clean_name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("Track {}", index + 1))
}

fn sorted(notes: &[RawNote]) -> Vec<&RawNote> {
    let mut v: Vec<&RawNote> = notes.iter().collect();
    v.sort_by_key(|n| (n.start, n.pitch, n.end, n.velocity));
    v
}

fn push_track(
    cmds: &mut Vec<Command>,
    ids: &mut IdGen,
    made: &mut usize,
    opts: &ImportOptions,
    name: String,
    notes: Vec<&RawNote>,
    ppq: u16,
) -> Result<(), MidiError> {
    if *made >= opts.max_tracks {
        return Err(MidiError::TooManyTracks);
    }
    *made += 1;
    let track = ids.next_id();
    cmds.push(Command::AddTrack {
        id: track,
        name,
        kind: TrackKind::Instrument,
        index: None,
    });
    if notes.is_empty() {
        return Ok(());
    }
    let mut spans = Vec::with_capacity(notes.len());
    for n in &notes {
        let start = rescale(n.start, ppq)?;
        let end = rescale(n.end, ppq)?;
        spans.push((start, (end - start).max(1)));
    }
    let first = spans.iter().map(|s| s.0).min().unwrap_or(0);
    let last = spans.iter().map(|s| s.0 + s.1).max().unwrap_or(first + 1);
    let region = ids.next_id();
    cmds.push(Command::AddRegion {
        id: region,
        track,
        start: first,
        length: last - first,
        kind: RegionKind::Midi,
    });
    for (n, (start, duration)) in notes.iter().zip(spans) {
        cmds.push(Command::AddNote {
            id: ids.next_id(),
            region,
            pitch: n.pitch,
            velocity: n.velocity,
            start: start - first,
            duration,
        });
    }
    Ok(())
}
