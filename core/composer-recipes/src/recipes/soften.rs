//! soften_velocity: pull loud notes down toward a ceiling.

use super::{midi_notes, parse_params, ranged, target_region, Ctx, RecipePlan};
use crate::error::RecipeError;
use composer_commands::Command;
use composer_model::{Id, Region};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    amount: Option<i64>,
    ceiling: Option<i64>,
}

pub fn params() -> Value {
    json!([
        { "name": "amount", "type": "integer", "min": 1, "max": 100, "default": 40,
          "description": "Percent of the way each loud note moves down to the ceiling." },
        { "name": "ceiling", "type": "integer", "min": 1, "max": 127, "default": 64,
          "description": "Notes at or below this velocity are left alone." },
    ])
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let amount = ranged("amount", p.amount, 40, 1, 100)?;
    let ceiling = ranged("ceiling", p.ceiling, 64, 1, 127)?;
    let regions: Vec<&Region> = if ctx.scope.region.is_some() {
        vec![target_region(ctx.project, &ctx.scope)?.1]
    } else if let Some(t) = ctx.scope.track {
        ctx.project
            .track(t)
            .ok_or_else(|| RecipeError::new("no_target", "that track does not exist"))?
            .regions
            .values()
            .collect()
    } else {
        ctx.project
            .tracks
            .iter()
            .flat_map(|t| t.regions.values())
            .collect()
    };
    let mut commands = Vec::new();
    for r in regions {
        // One command per new velocity value keeps the list short.
        let mut by_value: BTreeMap<u8, Vec<Id>> = BTreeMap::new();
        for n in midi_notes(r) {
            let v = i64::from(n.velocity);
            if v > ceiling {
                let soft = v - ((v - ceiling) * amount + 50) / 100;
                by_value
                    .entry(soft.clamp(1, 127) as u8)
                    .or_default()
                    .push(n.id);
            }
        }
        for (value, notes) in by_value {
            commands.push(Command::SetNotesVelocity {
                region: r.id,
                notes,
                value: Some(value),
                ramp: None,
            });
        }
    }
    if commands.is_empty() {
        return Err(RecipeError::new(
            "nothing_to_do",
            "no note is louder than the ceiling",
        ));
    }
    Ok(RecipePlan {
        label: "Soften velocity".to_string(),
        commands,
    })
}
