//! repeat_with_variation: copies of a clip placed after it, each nudged a little.

use super::{concat, key_for, key_params, parse_params, ranged, target_region, Ctx, RecipePlan};
use crate::error::RecipeError;
use composer_commands::Command;
use composer_model::Key;
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_COPIES: i64 = 32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    copies: Option<i64>,
    variation: Option<i64>,
    root: Option<u8>,
    scale: Option<composer_model::Scale>,
}

pub fn params() -> Value {
    concat(
        json!([
            { "name": "copies", "type": "integer", "min": 1, "max": MAX_COPIES, "default": 3,
              "description": "How many copies to add after the clip." },
            { "name": "variation", "type": "integer", "min": 0, "max": 100, "default": 30,
              "description": "How much each copy differs, 0 is an exact repeat." },
        ]),
        key_params(),
    )
}

/// The next pitch above or below `pitch` that belongs to the key.
fn key_step(key: &Key, pitch: u8, up: bool) -> Option<u8> {
    (1..=12i32).find_map(|d| {
        let p = i32::from(pitch) + if up { d } else { -d };
        u8::try_from(p)
            .ok()
            .filter(|p| *p <= 127 && key.contains(*p))
    })
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let copies = ranged("copies", p.copies, 3, 1, MAX_COPIES)?;
    let variation = ranged("variation", p.variation, 30, 0, 100)? as u64;
    let (_, r) = target_region(ctx.project, &ctx.scope)?;
    let notes = super::midi_notes(r);
    // Pitch steps need a key; without one only velocities change.
    let key = key_for(ctx.project, p.root, p.scale, r.start)?;
    let (region, start, length) = (r.id, r.start, r.length);
    let mut commands = Vec::new();
    for k in 0..copies {
        let new_region = ctx.ids.next();
        let note_ids: Vec<_> = notes.iter().map(|_| ctx.ids.next()).collect();
        commands.push(Command::DuplicateRegion {
            region,
            new_region,
            start: start + length * (k + 1),
            track: None,
            note_ids: note_ids.clone(),
        });
        for (n, id) in notes.iter().zip(&note_ids) {
            if ctx.rng.chance(variation / 2) {
                let swing = 2 + (variation / 5) as i64;
                let delta = ctx.rng.between(-swing, swing);
                let v = (i64::from(n.velocity) + delta).clamp(1, 127) as u8;
                if v != n.velocity {
                    commands.push(Command::SetNoteVelocity {
                        region: new_region,
                        note: *id,
                        velocity: v,
                    });
                }
            }
            if let Some(key) = key.as_ref().filter(|_| ctx.rng.chance(variation / 3)) {
                let up = ctx.rng.chance(50);
                if let Some(pitch) = key_step(key, n.pitch, up) {
                    commands.push(Command::MoveNote {
                        region: new_region,
                        note: *id,
                        start: n.start,
                        pitch,
                    });
                }
            }
        }
    }
    Ok(RecipePlan {
        label: format!("Repeat clip {copies} times with variation"),
        commands,
    })
}
