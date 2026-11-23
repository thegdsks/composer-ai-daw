//! pad_under_melody: soft held chords of the key chosen to fit the melody above them.

use super::{
    bar_ticks, concat, key_for, key_params, midi_notes, new_track, note_spec, parse_params, ranged,
    target_region, Ctx, RecipePlan,
};
use crate::error::RecipeError;
use composer_commands::{Command, NoteSpec};
use composer_model::{Id, RegionKind, Tick};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    bars_per_chord: Option<i64>,
    octave: Option<i64>,
    color: Option<i64>,
    root: Option<u8>,
    scale: Option<composer_model::Scale>,
    into_track: Option<Id>,
}

pub fn params() -> Value {
    concat(
        json!([
            { "name": "bars_per_chord", "type": "integer", "min": 1, "max": 8, "default": 2,
              "description": "How many bars each chord is held." },
            { "name": "octave", "type": "integer", "min": 2, "max": 5, "default": 3,
              "description": "Octave of the chord root, 4 holds middle C." },
            { "name": "color", "type": "integer", "min": 0, "max": 100, "default": 20,
              "description": "Percent of chords that add a seventh (chosen by the seed)." },
            { "name": "into_track", "type": "id",
              "description": "Instrument track that gets the new clip. Default: a new Pad track." },
        ]),
        key_params(),
    )
}

/// Order to prefer chords on a tie: tonic, relative, subdominant, dominant, then the rest.
const PREFERENCE: [u8; 7] = [1, 6, 4, 5, 2, 3, 7];

struct Held {
    start: Tick,
    end: Tick,
    pitches: Vec<u8>,
    velocity: u8,
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let bars = ranged("bars_per_chord", p.bars_per_chord, 2, 1, 8)?;
    let octave = ranged("octave", p.octave, 3, 2, 5)? as i8;
    let color = ranged("color", p.color, 20, 0, 100)? as u64;
    let (_, r) = target_region(ctx.project, &ctx.scope)?;
    let key = key_for(ctx.project, p.root, p.scale, r.start)?
        .ok_or_else(|| RecipeError::new("no_key", "set a key first, or give root and scale"))?;
    let triads = composer_theory::chords(&key, false, octave);
    let sevenths = composer_theory::chords(&key, true, octave);
    let notes = midi_notes(r);
    let window = bars * bar_ticks(ctx.project, r.start);

    let mut held: Vec<Held> = Vec::new();
    let mut prev_degree: Option<u8> = None;
    let mut prev_tones: Vec<u8> = Vec::new();
    let mut at = 0;
    while at < r.length {
        let end = (at + window).min(r.length);
        // How long each pitch class sounds inside this window.
        let mut weight = [0i64; 12];
        for n in &notes {
            let (s, e) = (n.start.max(at), (n.start + n.duration).min(end));
            if e > s {
                weight[usize::from(n.pitch % 12)] += e - s;
            }
        }
        let heard = weight.iter().any(|w| *w > 0);
        let degree = if heard {
            let score = |c: &composer_theory::Chord| -> i64 {
                c.pitch_classes
                    .iter()
                    .map(|pc| weight[usize::from(*pc)])
                    .sum()
            };
            let best = triads.iter().map(score).max().unwrap_or(0);
            let tied = |c: &&composer_theory::Chord| score(c) == best;
            // Prefer to stay on the previous chord, else the preference list.
            triads
                .iter()
                .filter(tied)
                .find(|c| Some(c.degree) == prev_degree)
                .or_else(|| {
                    PREFERENCE
                        .iter()
                        .filter_map(|d| triads.iter().find(|c| c.degree == *d))
                        .find(|c| score(c) == best)
                })
                .map(|c| c.degree)
        } else {
            prev_degree.or(Some(1))
        };
        let Some(degree) = degree else { break };
        let seventh = degree_has_seventh(&sevenths, degree) && ctx.rng.chance(color);
        let chord = if seventh { &sevenths } else { &triads }
            .iter()
            .find(|c| c.degree == degree);
        let Some(chord) = chord else { break };
        // Pick the inversion that moves the least from the last voicing.
        let tones = (0..chord.intervals.len() as u8)
            .map(|inv| chord.voicing(octave, inv))
            .filter(|v| !v.is_empty())
            .min_by_key(|v| movement(&prev_tones, v))
            .unwrap_or_default();
        if tones.is_empty() {
            break;
        }
        let velocity = 44 + ctx.rng.below(9) as u8;
        match held.last_mut() {
            Some(last) if last.pitches == tones && last.end == at => last.end = end,
            _ => held.push(Held {
                start: at,
                end,
                pitches: tones.clone(),
                velocity,
            }),
        }
        prev_degree = Some(degree);
        prev_tones = tones;
        at = end;
    }
    if held.is_empty() {
        return Err(RecipeError::new(
            "nothing_to_do",
            "no chord could be placed",
        ));
    }

    let mut specs: Vec<NoteSpec> = Vec::new();
    for h in &held {
        for pitch in &h.pitches {
            specs.push(note_spec(
                &mut ctx.ids,
                *pitch,
                h.velocity,
                h.start,
                h.end - h.start,
            ));
        }
    }
    let mut commands = Vec::new();
    let track = match p.into_track {
        Some(t) => t,
        None => {
            let (t, cmds) = new_track(&mut ctx.ids, "Pad", "builtin-warm-pad", 560, -80);
            commands.extend(cmds);
            t
        }
    };
    let region = ctx.ids.next();
    commands.push(Command::AddRegion {
        id: region,
        track,
        start: r.start,
        length: r.length,
        kind: RegionKind::Midi,
    });
    commands.push(Command::AddNotes {
        region,
        notes: specs,
    });
    Ok(RecipePlan {
        label: "Pad under melody".to_string(),
        commands,
    })
}

fn degree_has_seventh(sevenths: &[composer_theory::Chord], degree: u8) -> bool {
    sevenths.iter().any(|c| c.degree == degree)
}

/// Total distance between two voicings, tone by tone from the bottom.
fn movement(prev: &[u8], next: &[u8]) -> i32 {
    if prev.is_empty() {
        return 0;
    }
    prev.iter()
        .zip(next)
        .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
        .sum()
}
