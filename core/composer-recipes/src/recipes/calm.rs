//! calm_sections: a long, slow piece built from sections. A drone, a pad and a sparse piano
//! line share one slow chord loop; the seed picks loops, rhythms and notes. All material is
//! generated from small tables of scale degrees and chord tones, nothing is quoted.

use super::{
    bar_ticks, concat, key_for, key_params, new_track, note_spec, parse_params, pitch_in, ranged,
    Ctx, RecipePlan,
};
use crate::error::RecipeError;
use composer_commands::{Command, NoteSpec};
use composer_model::{Id, Key, RegionKind, Scale, Tick, TICKS_PER_QUARTER};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_MINUTES: i64 = 8;
const MAX_MINUTES: i64 = 120;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    minutes: Option<i64>,
    tempo: Option<i64>,
    mood: Option<String>,
    section_bars: Option<i64>,
    root: Option<u8>,
    scale: Option<Scale>,
}

pub fn params() -> Value {
    concat(
        json!([
            { "name": "minutes", "type": "integer", "min": 1, "max": MAX_MINUTES, "default": DEFAULT_MINUTES,
              "description": "About how long the piece runs, rounded up to whole sections." },
            { "name": "tempo", "type": "integer", "min": 40, "max": 100, "default": 60,
              "description": "Beats per minute." },
            { "name": "mood", "type": "choice", "choices": ["still", "gentle", "drifting"], "default": "gentle",
              "description": "How busy the piano line is and how often the piece swells." },
            { "name": "section_bars", "type": "integer", "min": 8, "max": 32, "default": 16,
              "description": "Bars per section, a multiple of 4 (one chord lasts a quarter of a section)." },
        ]),
        key_params(),
    )
}

#[derive(Clone, Copy)]
enum Mood {
    Still,
    Gentle,
    Drifting,
}

/// Slow loops of four scale degrees (1 based), grouped by whether the key is minor.
const MAJOR_LOOPS: [[u8; 4]; 4] = [[1, 5, 6, 4], [1, 4, 6, 5], [1, 6, 4, 5], [4, 1, 5, 6]];
const MINOR_LOOPS: [[u8; 4]; 4] = [[1, 6, 3, 7], [1, 4, 7, 3], [1, 6, 7, 1], [6, 7, 1, 5]];
const MIDDLE_NAMES: [&str; 6] = ["Drift", "Bloom", "Hush", "Glow", "Turn", "Settle"];

fn is_minor(s: Scale) -> bool {
    matches!(
        s,
        Scale::Minor | Scale::Dorian | Scale::HarmonicMinor | Scale::PentatonicMinor
    )
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let minutes = ranged("minutes", p.minutes, DEFAULT_MINUTES, 1, MAX_MINUTES)?;
    let bpm = ranged("tempo", p.tempo, 60, 40, 100)?;
    let section_bars = ranged("section_bars", p.section_bars, 16, 8, 32)?;
    if section_bars % 4 != 0 {
        return Err(RecipeError::invalid("section_bars must be a multiple of 4"));
    }
    let mood = match p.mood.as_deref().unwrap_or("gentle") {
        "still" => Mood::Still,
        "gentle" => Mood::Gentle,
        "drifting" => Mood::Drifting,
        _ => {
            return Err(RecipeError::invalid(
                "mood must be still, gentle or drifting",
            ))
        }
    };
    let key = key_for(ctx.project, p.root, p.scale, 0)?.unwrap_or(Key {
        root: 5,
        scale: Scale::Major,
    });
    let bar = bar_ticks(ctx.project, 0);
    let section_ticks = section_bars * bar;
    let chord_ticks = section_ticks / 4;
    let wanted = minutes * bpm * TICKS_PER_QUARTER;
    let sections = ((wanted + section_ticks - 1) / section_ticks).max(1);

    let triads = composer_theory::chords(&key, false, 3);
    let loops = if is_minor(key.scale) {
        &MINOR_LOOPS
    } else {
        &MAJOR_LOOPS
    };
    let scale_pcs = key.pitch_classes();

    let mut commands = vec![
        Command::SetTempo {
            tick: 0,
            mbpm: (bpm * 1000) as u32,
        },
        Command::SetKey {
            tick: 0,
            root: key.root,
            scale: key.scale,
        },
    ];
    let (drone, c) = new_track(&mut ctx.ids, "Low drone", "builtin-sub-bass", 480, 0);
    commands.extend(c);
    let (pad, c) = new_track(&mut ctx.ids, "Warm pad", "builtin-warm-pad", 620, -100);
    commands.extend(c);
    let (piano, c) = new_track(&mut ctx.ids, "Soft piano", "builtin-soft-piano", 700, 120);
    commands.extend(c);

    let edge = (sections / 8).max(1);
    let peak_percent = match mood {
        Mood::Still => 0,
        Mood::Gentle => 25,
        Mood::Drifting => 45,
    };
    let mut prog = ctx.rng.below(4) as usize;
    let mut prev_pitch: i64 = 69;
    let mut prev_voicing: Vec<u8> = Vec::new();

    for s in 0..sections {
        let start: Tick = s * section_ticks;
        let energy: u8 = if sections > 2 && (s < edge || s >= sections - edge) {
            0
        } else if ctx.rng.chance(peak_percent) {
            2
        } else {
            1
        };
        if s > 0 && ctx.rng.chance(45) {
            prog = ctx.rng.below(4) as usize;
        }
        let name = if s == 0 {
            "Arrival"
        } else if s == sections - 1 {
            "Return"
        } else {
            *ctx.rng.pick(&MIDDLE_NAMES)
        };
        commands.push(Command::SetMarker {
            tick: start,
            name: name.to_string(),
        });

        let mut drone_notes: Vec<NoteSpec> = Vec::new();
        let mut pad_notes: Vec<NoteSpec> = Vec::new();
        let mut piano_notes: Vec<NoteSpec> = Vec::new();
        let level = i64::from(energy);

        for (j, degree) in loops[prog].iter().copied().enumerate() {
            let at = j as Tick * chord_ticks;
            let Some(chord) = triads.iter().find(|c| c.degree == degree) else {
                continue;
            };
            // Drone: the key root alone while quiet, the chord root while fuller.
            if energy > 0 || j == 0 {
                let pc = if energy == 0 { key.root } else { chord.root };
                let len = if energy == 0 {
                    section_ticks
                } else {
                    chord_ticks
                };
                if let Some(pitch) = pitch_in(pc, 2) {
                    drone_notes.push(note_spec(
                        &mut ctx.ids,
                        pitch,
                        (38 + 6 * level) as u8,
                        at,
                        len,
                    ));
                }
            }
            // Pad: the inversion nearest the last voicing; bare root and fifth when quiet.
            let tones = (0..3u8)
                .map(|inv| chord.voicing(3, inv))
                .filter(|v| !v.is_empty())
                .min_by_key(|v| distance(&prev_voicing, v))
                .unwrap_or_default();
            let sounding: Vec<u8> = if energy == 0 {
                tones.iter().take(2).copied().collect()
            } else {
                tones.clone()
            };
            for pitch in &sounding {
                let vel = 36 + 6 * level + ctx.rng.below(6) as i64;
                pad_notes.push(note_spec(&mut ctx.ids, *pitch, vel as u8, at, chord_ticks));
            }
            if !tones.is_empty() {
                prev_voicing = tones;
            }
            // Piano: a few notes on whole beats, stepping near the last pitch.
            let (lo, hi) = match mood {
                Mood::Still => (0, 1),
                Mood::Gentle => (1, 2),
                Mood::Drifting => (1, 3),
            };
            let count = ctx.rng.between(lo + level / 2, hi + level);
            let beats = chord_ticks / TICKS_PER_QUARTER;
            let mut taken: Vec<i64> = Vec::new();
            while (taken.len() as i64) < count.min(beats) {
                let b = ctx.rng.below(beats as u64) as i64;
                if !taken.contains(&b) {
                    taken.push(b);
                }
            }
            taken.sort_unstable();
            for b in taken {
                let from_chord = ctx.rng.chance(70);
                let pool: &[u8] = if from_chord {
                    &chord.pitch_classes
                } else {
                    &scale_pcs
                };
                let mut near: Vec<i64> = (57..=81i64)
                    .filter(|m| pool.contains(&((*m % 12) as u8)))
                    .collect();
                near.sort_by_key(|m| ((m - prev_pitch).abs(), *m));
                near.truncate(3);
                if near.is_empty() {
                    continue;
                }
                let pitch = *ctx.rng.pick(&near);
                prev_pitch = pitch;
                let len_beats = ctx.rng.between(2, 6).min(beats - b);
                let vel = 40 + 8 * level + ctx.rng.below(11) as i64;
                piano_notes.push(note_spec(
                    &mut ctx.ids,
                    pitch as u8,
                    vel as u8,
                    at + b * TICKS_PER_QUARTER,
                    len_beats * TICKS_PER_QUARTER,
                ));
            }
        }

        for (track, notes) in [(drone, drone_notes), (pad, pad_notes), (piano, piano_notes)] {
            push_clip(ctx, &mut commands, track, start, section_ticks, notes);
        }
    }
    Ok(RecipePlan {
        label: format!("Calm sections, {minutes} min"),
        commands,
    })
}

fn push_clip(
    ctx: &mut Ctx<'_>,
    commands: &mut Vec<Command>,
    track: Id,
    start: Tick,
    length: Tick,
    notes: Vec<NoteSpec>,
) {
    if notes.is_empty() {
        return;
    }
    let region = ctx.ids.next();
    commands.push(Command::AddRegion {
        id: region,
        track,
        start,
        length,
        kind: RegionKind::Midi,
    });
    commands.push(Command::AddNotes { region, notes });
}

fn distance(prev: &[u8], next: &[u8]) -> i32 {
    prev.iter()
        .zip(next)
        .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
        .sum()
}
