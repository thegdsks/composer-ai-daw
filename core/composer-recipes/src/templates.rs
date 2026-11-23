//! Starter templates as data (U03), loaded here so the browser editor, the daemon, the CLI
//! and MCP all read the same files. A template is a list of commands with `$name`
//! placeholders in id fields; planning gives each placeholder a seeded id.

use crate::error::RecipeError;
use crate::rng::{salt, IdSource};
use composer_commands::Command;
use composer_model::{Key, Project, Scale, TICKS_PER_QUARTER};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const SOURCES: [(&str, &str); 4] = [
    ("recreate", include_str!("../../../templates/recreate.json")),
    (
        "cinematic",
        include_str!("../../../templates/cinematic.json"),
    ),
    (
        "calm-hour",
        include_str!("../../../templates/calm-hour.json"),
    ),
    ("blank", include_str!("../../../templates/blank.json")),
];

#[derive(Deserialize)]
struct RawLoop {
    start: i64,
    end: i64,
}

#[derive(Deserialize)]
struct Raw {
    name: String,
    description: String,
    tempo: f64,
    key: String,
    ticks_per_quarter: i64,
    #[serde(rename = "loop")]
    loop_range: RawLoop,
    commands: Vec<Value>,
}

/// A parsed template file.
#[derive(Debug, Clone)]
pub struct Template {
    pub id: &'static str,
    pub name: String,
    pub description: String,
    pub tempo: f64,
    pub key_text: String,
    pub key: Key,
    pub loop_start: i64,
    pub loop_end: i64,
    commands: Vec<Value>,
}

impl Template {
    pub fn command_count(&self) -> usize {
        self.commands.len() + 1
    }

    pub fn info(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "tempo": self.tempo,
            "key": self.key_text,
            "command_count": self.command_count(),
            "loop": { "start": self.loop_start, "end": self.loop_end },
        })
    }
}

/// Parses text such as "F major" or "Bb minor". Pitch classes follow `note_name`.
pub fn parse_key(text: &str) -> Option<Key> {
    let (root, scale) = text.trim().split_once(' ')?;
    let flats = (0..12u8).find(|pc| composer_theory::note_name(*pc, true) == root);
    let sharps = (0..12u8).find(|pc| composer_theory::note_name(*pc, false) == root);
    let scale: Scale =
        serde_json::from_value(Value::String(scale.trim().replace(' ', "_"))).ok()?;
    Some(Key {
        root: flats.or(sharps)?,
        scale,
    })
}

fn parse(id: &'static str, text: &str) -> Result<Template, String> {
    let raw: Raw = serde_json::from_str(text).map_err(|e| format!("{id}: {e}"))?;
    if raw.ticks_per_quarter != TICKS_PER_QUARTER {
        return Err(format!("{id}: ticks_per_quarter does not match the engine"));
    }
    if raw.loop_range.end <= raw.loop_range.start || raw.tempo <= 0.0 {
        return Err(format!("{id}: bad loop or tempo"));
    }
    let key = parse_key(&raw.key).ok_or_else(|| format!("{id}: bad key {}", raw.key))?;
    Ok(Template {
        id,
        name: raw.name,
        description: raw.description,
        tempo: raw.tempo,
        key_text: raw.key,
        key,
        loop_start: raw.loop_range.start,
        loop_end: raw.loop_range.end,
        commands: raw.commands,
    })
}

/// Every bundled template, in a fixed order. A file that does not parse is a build defect
/// and is caught by the tests, so it is skipped here rather than panicking in a page.
pub fn all() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(|| {
        SOURCES
            .iter()
            .filter_map(|(id, text)| parse(id, text).ok())
            .collect()
    })
}

/// Number of bundled template files, for tests.
pub fn source_count() -> usize {
    SOURCES.len()
}

pub fn get(id: &str) -> Option<&'static Template> {
    all().iter().find(|t| t.id == id)
}

/// Replaces `$name` strings with ids; the same name always gets the same id.
fn substitute(v: &Value, ids: &mut BTreeMap<String, String>, source: &mut IdSource<'_>) -> Value {
    match v {
        Value::String(s) if is_placeholder(s) => {
            let id = ids
                .entry(s.clone())
                .or_insert_with(|| source.next().to_string());
            Value::String(id.clone())
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, ids, source)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), substitute(x, ids, source)))
                .collect::<Map<String, Value>>(),
        ),
        other => other.clone(),
    }
}

fn is_placeholder(s: &str) -> bool {
    let mut it = s.chars();
    it.next() == Some('$')
        && it.next().is_some_and(|c| c.is_ascii_lowercase())
        && it.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A template resolved against one project and seed.
#[derive(Debug, Clone)]
pub struct TemplatePlan {
    pub template: &'static Template,
    pub seed: u64,
    pub commands: Vec<Command>,
    /// Placeholder name (without the `$`) to the id it received.
    pub ids: BTreeMap<String, String>,
}

/// Commands that build the template on top of the current project. Ids are seeded and skip
/// any id the project already uses, so the same seed on the same project always gives the
/// same list. A `set_key` at tick 0 is added from the template key.
pub fn plan(
    project: &Project,
    template: &'static Template,
    seed: u64,
) -> Result<TemplatePlan, RecipeError> {
    let mut source = IdSource::new(project, seed, salt(template.id));
    let mut ids = BTreeMap::new();
    let mut commands = Vec::with_capacity(template.commands.len() + 1);
    for raw in &template.commands {
        let v = substitute(raw, &mut ids, &mut source);
        let c: Command = serde_json::from_value(v)
            .map_err(|e| RecipeError::new("invalid_template", format!("{}: {e}", template.id)))?;
        commands.push(c);
    }
    commands.push(Command::SetKey {
        tick: 0,
        root: template.key.root,
        scale: template.key.scale,
    });
    Ok(TemplatePlan {
        template,
        seed,
        commands,
        ids: ids
            .into_iter()
            .map(|(k, v)| (k.trim_start_matches('$').to_string(), v))
            .collect(),
    })
}
