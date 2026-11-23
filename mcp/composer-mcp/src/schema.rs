//! JSON Schema for command tools, derived from the daemon's `commands.describe`,
//! and a small validator for the subset of JSON Schema we emit.

use serde_json::{json, Map, Value};

/// Pattern of an id: 32 lowercase hex digits.
pub const ID_PATTERN: &str = "^[0-9a-f]{32}$";
/// Largest tick the model accepts (2^40).
const MAX_TICK: i64 = 1 << 40;
const MAX_NAME_LEN: u64 = 256;
/// Mirrors the daemon's bulk limit (MAX_BULK_NOTES in composer-commands).
const MAX_BULK_ITEMS: u64 = 100_000;

/// Schema for one described field. Unknown types stay open (any value) so a
/// newer daemon never makes a tool unusable.
fn field_schema(command: &str, name: &str, ty: &str, nullable: bool) -> Value {
    let (mut s, doc): (Map<String, Value>, String) = match ty {
        "id" => (
            obj(json!({ "type": "string", "pattern": ID_PATTERN })),
            "Id: 32 lowercase hex digits. New ids come from composer_new_id.".into(),
        ),
        "tick" => {
            let min = i64::from(matches!(name, "length" | "duration" | "grid"));
            let what = match (command, name) {
                (c, "start") if c.ends_with("_note") => {
                    "Start in ticks, relative to the start of the clip."
                }
                (_, "start") => "Start in ticks from the project start.",
                (_, "length" | "duration") => "Length in ticks, at least 1.",
                (_, "grid") => "Grid size in ticks, at least 1 (240 is a sixteenth note).",
                (_, "at") => "Absolute position in ticks from the project start.",
                _ => "Position in ticks from the project start.",
            };
            (
                obj(json!({ "type": "integer", "minimum": min, "maximum": MAX_TICK })),
                format!("{what} 960 ticks per quarter note, 3840 per 4/4 bar."),
            )
        }
        "u8" => match name {
            "pitch" => (
                obj(json!({ "type": "integer", "minimum": 0, "maximum": 127 })),
                "MIDI pitch 0..127 (60 is middle C).".into(),
            ),
            "velocity" => (
                obj(json!({ "type": "integer", "minimum": 1, "maximum": 127 })),
                "Velocity 1..127.".into(),
            ),
            "numerator" => (
                obj(json!({ "type": "integer", "minimum": 1, "maximum": 32 })),
                "Beats per bar, 1..32.".into(),
            ),
            "strength" => (
                obj(json!({ "type": "integer", "minimum": 1, "maximum": 100 })),
                "Percent of the way to the grid, 1..100.".into(),
            ),
            "denominator" => (
                obj(json!({ "type": "integer", "enum": [1, 2, 4, 8, 16, 32] })),
                "Beat unit: 1, 2, 4, 8, 16 or 32.".into(),
            ),
            _ => (
                obj(json!({ "type": "integer", "minimum": 0, "maximum": 255 })),
                "Integer 0..255.".into(),
            ),
        },
        "hash" => (
            obj(json!({ "type": "string", "pattern": "^[0-9a-f]{64}$" })),
            "Content hash (SHA-256, 64 lowercase hex digits) of an imported audio file. Use composer_audio_import first; the hash is in its reply."
                .into(),
        ),
        "u64" if name == "offset" => (
            obj(json!({ "type": "integer", "minimum": 0, "maximum": 1u64 << 44 })),
            "First source frame the clip plays (trims the start), 0 plays from the beginning."
                .into(),
        ),
        "u64" => (
            obj(json!({ "type": "integer", "minimum": 0 })),
            "Unsigned integer.".into(),
        ),
        "bool" => (
            obj(json!({ "type": "boolean" })),
            "true or false.".into(),
        ),
        "u16" if name == "volume" => (
            obj(json!({ "type": "integer", "minimum": 0, "maximum": 2000 })),
            "Volume in permille of unity gain: 1000 is unity, 2000 is about +6 dB, 0 is silent."
                .into(),
        ),
        "u16" if name == "gain" => (
            obj(json!({ "type": "integer", "minimum": 0, "maximum": 2000 })),
            "Clip gain in permille of unity: 1000 is unity, 2000 is about +6 dB, 0 is silent."
                .into(),
        ),
        "u16" => (
            obj(json!({ "type": "integer", "minimum": 0, "maximum": 65535 })),
            "Unsigned integer.".into(),
        ),
        "i16" if name == "pan" => (
            obj(json!({ "type": "integer", "minimum": -1000, "maximum": 1000 })),
            "Pan in permille: -1000 full left, 0 center, 1000 full right.".into(),
        ),
        "i16" if name == "semitones" => (
            obj(json!({ "type": "integer", "minimum": -127, "maximum": 127 })),
            "Semitones to shift, negative is down. A note may not leave 0..127.".into(),
        ),
        "i16" => (
            obj(json!({ "type": "integer", "minimum": -32768, "maximum": 32767 })),
            "Signed integer.".into(),
        ),
        "ids" => (
            obj(json!({
                "type": "array",
                "maxItems": MAX_BULK_ITEMS,
                "items": { "type": "string", "pattern": ID_PATTERN },
            })),
            "List of ids (32 lowercase hex digits each). New ids come from composer_new_id."
                .into(),
        ),
        "notes" => (
            obj(json!({
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_BULK_ITEMS,
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "pattern": ID_PATTERN },
                        "pitch": { "type": "integer", "minimum": 0, "maximum": 127 },
                        "velocity": { "type": "integer", "minimum": 1, "maximum": 127 },
                        "start": { "type": "integer", "minimum": 0, "maximum": MAX_TICK },
                        "duration": { "type": "integer", "minimum": 1, "maximum": MAX_TICK },
                    },
                    "required": ["id", "pitch", "velocity", "start", "duration"],
                    "additionalProperties": false,
                },
            })),
            "List of notes {id, pitch, velocity, start, duration}; start is in ticks relative to the clip."
                .into(),
        ),
        "u32" if name == "mbpm" => (
            obj(json!({ "type": "integer", "minimum": 20000, "maximum": 400000 })),
            "Tempo in thousandths of a BPM, 20000..400000 (120000 means 120 BPM).".into(),
        ),
        "u32" => (
            obj(json!({ "type": "integer", "minimum": 0, "maximum": u32::MAX })),
            "Unsigned integer.".into(),
        ),
        "index" => (
            obj(json!({ "type": "integer", "minimum": 0 })),
            "Zero based position in the list; past the end clamps or is rejected by the daemon."
                .into(),
        ),
        "text" => (
            obj(json!({ "type": "string", "maxLength": MAX_NAME_LEN })),
            format!("Text of at most {MAX_NAME_LEN} bytes."),
        ),
        t if t.contains('|') => {
            let vals: Vec<&str> = t.split('|').collect();
            (
                obj(json!({ "type": "string", "enum": vals })),
                format!("One of: {}.", vals.join(", ")),
            )
        }
        other => (Map::new(), format!("Type {other} (not described further).")),
    };
    if nullable {
        if let Some(Value::String(t)) = s.get("type").cloned() {
            s.insert("type".into(), json!([t, "null"]));
        }
        if let Some(Value::Array(e)) = s.get_mut("enum") {
            e.push(Value::Null);
        }
    }
    let null_note = if nullable {
        " Omit or null for none."
    } else {
        ""
    };
    s.insert("description".into(), json!(format!("{doc}{null_note}")));
    Value::Object(s)
}

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// The object schema of one command's fields, without the `command` tag.
pub fn command_fields_schema(info: &Value) -> Value {
    let command = info["name"].as_str().unwrap_or_default();
    let mut props = Map::new();
    let mut required = Vec::new();
    for f in info["fields"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let (Some(name), Some(ty)) = (f["name"].as_str(), f["ty"].as_str()) else {
            continue;
        };
        let nullable = f["nullable"].as_bool().unwrap_or(false);
        props.insert(name.into(), field_schema(command, name, ty, nullable));
        if !nullable {
            required.push(json!(name));
        }
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

/// The same schema with the `command` tag added, for items of a group.
pub fn tagged_schema(info: &Value) -> Value {
    let mut s = command_fields_schema(info);
    let name = info["name"].as_str().unwrap_or_default();
    s["properties"]["command"] = json!({ "const": name });
    if let Some(Value::Array(r)) = s.get_mut("required") {
        r.insert(0, json!("command"));
    }
    s["description"] = json!(info["summary"].as_str().unwrap_or_default());
    s
}

/// Validates `v` against the schema subset this crate emits. The error names
/// the offending path so a caller can fix the input.
pub fn validate(schema: &Value, v: &Value) -> Result<(), String> {
    check(schema, v, "$")
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        _ => true,
    }
}

fn is_id(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check(schema: &Value, v: &Value, path: &str) -> Result<(), String> {
    if let Some(c) = schema.get("const") {
        if c != v {
            return Err(format!("{path}: must be {c}"));
        }
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        // Group items are discriminated by their `command` tag.
        let tag = v.get("command");
        let hit = variants
            .iter()
            .find(|s| tag.is_some() && s["properties"]["command"]["const"] == *tag.unwrap());
        return match hit {
            Some(s) => check(s, v, path),
            None => Err(format!("{path}: unknown or missing \"command\" name")),
        };
    }
    // Output schemas use anyOf to allow either a success or an error shape.
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        let mut first = None;
        for s in variants {
            match check(s, v, path) {
                Ok(()) => return Ok(()),
                Err(e) => first = first.or(Some(e)),
            }
        }
        return Err(first.unwrap_or_else(|| format!("{path}: matches no anyOf variant")));
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|t| type_ok(t, v)) {
        return Err(format!("{path}: expected {}", types.join(" or ")));
    }
    if v.is_null() {
        return Ok(());
    }
    if let Some(e) = schema.get("enum").and_then(Value::as_array) {
        if !e.contains(v) {
            return Err(format!("{path}: not one of the allowed values"));
        }
    }
    if let Some(n) = v.as_i64() {
        if schema
            .get("minimum")
            .and_then(Value::as_i64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{path}: below the minimum"));
        }
        if schema
            .get("maximum")
            .and_then(Value::as_i64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{path}: above the maximum"));
        }
    }
    if let Some(s) = v.as_str() {
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| s.len() as u64 > m)
        {
            return Err(format!("{path}: text too long"));
        }
        if schema.get("pattern").and_then(Value::as_str) == Some(ID_PATTERN) && !is_id(s) {
            return Err(format!("{path}: must be 32 lowercase hex digits"));
        }
    }
    if let (Some(items), Some(a)) = (schema.get("items"), v.as_array()) {
        let max = schema.get("maxItems").and_then(Value::as_u64);
        let min = schema.get("minItems").and_then(Value::as_u64);
        if max.is_some_and(|m| a.len() as u64 > m) {
            return Err(format!("{path}: too many items"));
        }
        if min.is_some_and(|m| (a.len() as u64) < m) {
            return Err(format!("{path}: too few items"));
        }
        for (i, x) in a.iter().enumerate() {
            check(items, x, &format!("{path}[{i}]"))?;
        }
    }
    if let Some(m) = v.as_object() {
        let props = schema.get("properties").and_then(Value::as_object);
        for r in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(k) = r.as_str() {
                if !m.contains_key(k) {
                    return Err(format!("{path}: missing required field \"{k}\""));
                }
            }
        }
        for (k, x) in m {
            match props.and_then(|p| p.get(k)) {
                Some(s) => check(s, x, &format!("{path}.{k}"))?,
                None if schema.get("additionalProperties") == Some(&json!(false)) => {
                    return Err(format!("{path}: unknown field \"{k}\""));
                }
                None => {}
            }
        }
    }
    Ok(())
}
