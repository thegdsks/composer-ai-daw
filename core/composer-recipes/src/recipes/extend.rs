//! extend_to_length: loop a clip and grow it to a bar count.

use super::{bar_ticks, parse_params, ranged, target_region, Ctx, RecipePlan};
use crate::error::RecipeError;
use composer_commands::Command;
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_BARS: i64 = 32;
const MAX_BARS: i64 = 4000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    bars: Option<i64>,
}

pub fn params() -> Value {
    json!([
        { "name": "bars", "type": "integer", "min": 1, "max": MAX_BARS, "default": DEFAULT_BARS,
          "description": "Total length of the clip in bars." },
    ])
}

pub fn plan(ctx: &mut Ctx<'_>, raw: Value) -> Result<RecipePlan, RecipeError> {
    let p: Params = parse_params(raw)?;
    let bars = ranged("bars", p.bars, DEFAULT_BARS, 1, MAX_BARS)?;
    let (_, r) = target_region(ctx.project, &ctx.scope)?;
    let target = bars * bar_ticks(ctx.project, r.start);
    if target <= r.length {
        return Err(RecipeError::new(
            "nothing_to_do",
            "the clip is already that long or longer",
        ));
    }
    let mut commands = Vec::new();
    // The current length becomes the repeating unit unless the clip already loops.
    if r.loop_length == 0 {
        commands.push(Command::SetRegionLoop {
            region: r.id,
            loop_length: r.length,
        });
    }
    commands.push(Command::ResizeRegion {
        region: r.id,
        length: target,
    });
    Ok(RecipePlan {
        label: format!("Extend clip to {bars} bars"),
        commands,
    })
}
