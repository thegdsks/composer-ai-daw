//! Compact project summary computed client side from a `state.get` result.

use serde_json::{json, Value};

const TICKS_PER_QUARTER: i64 = 960;

fn keyed_i64(m: Option<&Value>) -> Vec<(i64, &Value)> {
    let mut v: Vec<(i64, &Value)> = m
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .filter_map(|(k, x)| k.parse::<i64>().ok().map(|t| (t, x)))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|(t, _)| *t);
    v
}

/// Length of `span` ticks in bars, walking the meter changes.
fn bars(span: i64, meters: &[(i64, &Value)]) -> f64 {
    let mut total = 0.0;
    for (i, (at, m)) in meters.iter().enumerate() {
        let next = meters.get(i + 1).map_or(i64::MAX, |(t, _)| *t);
        let (n, d) = (
            m["numerator"].as_i64().unwrap_or(4),
            m["denominator"].as_i64().unwrap_or(4).max(1),
        );
        let bar_ticks = (n * 4 * TICKS_PER_QUARTER / d).max(1);
        let covered = span.min(next) - *at;
        if covered > 0 {
            total += covered as f64 / bar_ticks as f64;
        }
    }
    (total * 1000.0).round() / 1000.0
}

/// Builds the summary from the `state.get` result (`project` and `state_hash`).
pub fn summarize(state: &Value) -> Result<Value, String> {
    let p = state
        .get("project")
        .ok_or("state.get result has no project")?;
    let tempos = keyed_i64(p["tempo_map"].get("tempos"));
    let meters = keyed_i64(p["tempo_map"].get("meters"));
    let mut tracks = Vec::new();
    let (mut regions_total, mut notes_total, mut span) = (0u64, 0u64, 0i64);
    for t in p["tracks"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let (mut regions, mut notes) = (0u64, 0u64);
        for r in t["regions"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
        {
            regions += 1;
            notes += r["content"]["midi"]["notes"]
                .as_object()
                .map_or(0, |n| n.len() as u64);
            let end = r["start"].as_i64().unwrap_or(0) + r["length"].as_i64().unwrap_or(0);
            span = span.max(end);
        }
        regions_total += regions;
        notes_total += notes;
        tracks.push(json!({
            "id": t["id"], "name": t["name"], "kind": t["kind"],
            "regions": regions, "notes": notes,
        }));
    }
    let first_tempo = tempos.first().and_then(|(_, v)| v.as_u64()).unwrap_or(0);
    let first_meter = meters.first().map_or("4/4".to_string(), |(_, m)| {
        format!("{}/{}", m["numerator"], m["denominator"])
    });
    Ok(json!({
        "name": p["name"],
        "state_hash": state["state_hash"],
        "track_count": tracks.len(),
        "region_count": regions_total,
        "note_count": notes_total,
        "span_ticks": span,
        "span_bars": bars(span, &meters),
        "tempo_bpm": first_tempo as f64 / 1000.0,
        "tempo_changes": tempos.len().saturating_sub(1),
        "time_signature": first_meter,
        "time_signature_changes": meters.len().saturating_sub(1),
        "tracks": tracks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_span() {
        let state = json!({
            "state_hash": "ab",
            "project": {
                "name": "x",
                "tempo_map": { "tempos": { "0": 90000 }, "meters": { "0": { "numerator": 4, "denominator": 4 } } },
                "tracks": [{ "id": "t", "name": "T", "kind": "instrument", "regions": {
                    "r": { "start": 0, "length": 7680, "content": { "midi": { "notes": { "n": {}, "m": {} } } } }
                } }]
            }
        });
        let s = summarize(&state).unwrap();
        assert_eq!(s["span_bars"], json!(2.0));
        assert_eq!(s["note_count"], 2);
        assert_eq!(s["tempo_bpm"], json!(90.0));
    }

    #[test]
    fn six_eight_bar_is_2880_ticks() {
        let meters = [(0, &json!({ "numerator": 6, "denominator": 8 }))];
        assert_eq!(bars(5760, &meters), 2.0);
    }
}
