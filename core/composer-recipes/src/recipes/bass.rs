//! bass_from_chords: a low line that follows the lowest note of each window of a chord clip.

use super::{
    midi_notes, new_track, note_spec, parse_params, pitch_in, ranged, target_region, Ctx,
    RecipePlan,
};
use crate::error::RecipeError;
use composer_commands::{Command, NoteSpec};
use composer_model::{Id, RegionKind, Tick, TICKS_PER_QUARTER};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    style: Option<String>,
    window_beats: Option<i64>,
    octave: Option<i64>,
    into_track: Option<Id>,
}

pub fn params() -> Value {
    json!([
        { "name": "style", "type": "choice", "choices": ["sustain", "root_fifth"], "default": "sustain",
          "description": "sustain holds one note per chord; root_fifth sometimes moves to the fifth halfway (chosen by the seed)." },
        { "name": "window_beats", "type": "integer", "min": 1, "max": 32, "default": 4,
          "description": "Length of one chord window in beats; the bass changes at most once per window." },
        { "name": "octave", "type": "integer", "min": 1, "max": 3, "default": 2,
          "description": "Octave of the bass notes, 4 holds middle C." },
        { "name": "into_track", "type": "id",
          "description": "Instrument track that gets the new clip. Default: a new Bass track." },
    ])
}

/// One held bass note: pitch class, velocity, start and end relative to the clip.
struct Run {
    pc: u8,
    velocity: u8,
    start: Tick,
    end: Tick,
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let root_fifth = match p.style.as_deref().unwrap_or("sustain") {
        "sustain" => false,
        "root_fifth" => true,
        _ => return Err(RecipeError::invalid("style must be sustain or root_fifth")),
    };
    let beats = ranged("window_beats", p.window_beats, 4, 1, 32)?;
    let octave = ranged("octave", p.octave, 2, 1, 3)?;
    let (_, r) = target_region(ctx.project, &ctx.scope)?;
    let notes = midi_notes(r);
    let window = beats * TICKS_PER_QUARTER;
    let windows = (r.length + window - 1) / window;

    // Lowest note of each window gives its root; empty windows keep the chord ringing.
    let mut runs: Vec<Run> = Vec::new();
    for w in 0..windows {
        let (lo, hi) = (w * window, ((w + 1) * window).min(r.length));
        let here: Vec<_> = notes
            .iter()
            .filter(|n| (lo..hi).contains(&n.start))
            .collect();
        let low = here.iter().min_by_key(|n| (n.pitch, n.start));
        match (low, runs.last_mut()) {
            (Some(n), Some(last)) if last.pc == n.pitch % 12 => last.end = hi,
            (Some(n), _) => {
                let mean =
                    here.iter().map(|n| u32::from(n.velocity)).sum::<u32>() / here.len() as u32;
                runs.push(Run {
                    pc: n.pitch % 12,
                    velocity: (mean * 4 / 5).clamp(30, 100) as u8,
                    start: lo,
                    end: hi,
                });
            }
            (None, Some(last)) => last.end = hi,
            (None, None) => {}
        }
    }
    if runs.is_empty() {
        return Err(RecipeError::new(
            "nothing_to_do",
            "the clip has no notes to follow",
        ));
    }

    let mut specs: Vec<NoteSpec> = Vec::new();
    for run in &runs {
        let Some(root) = pitch_in(run.pc, octave) else {
            continue;
        };
        let len = run.end - run.start;
        let half = (len / 2 / TICKS_PER_QUARTER) * TICKS_PER_QUARTER;
        if root_fifth && half >= TICKS_PER_QUARTER && ctx.rng.chance(60) {
            let fifth = root.saturating_add(7).min(127);
            specs.push(note_spec(&mut ctx.ids, root, run.velocity, run.start, half));
            specs.push(note_spec(
                &mut ctx.ids,
                fifth,
                run.velocity.saturating_sub(6),
                run.start + half,
                len - half,
            ));
        } else {
            specs.push(note_spec(&mut ctx.ids, root, run.velocity, run.start, len));
        }
    }

    let mut commands = Vec::new();
    let track = match p.into_track {
        Some(t) => t,
        None => {
            let (t, cmds) = new_track(&mut ctx.ids, "Bass", "builtin-sub-bass", 700, 0);
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
        label: "Bass from chords".to_string(),
        commands,
    })
}
