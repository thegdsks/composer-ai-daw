//! Recipes (D28): deterministic generators that turn parameters, a seed and the current
//! project into an ordinary command list. A recipe never touches the project; the caller
//! plans the list on a copy, applies it as one undo group, or posts it as a proposal.

mod bass;
mod calm;
mod extend;
mod pad;
mod repeat;
mod soften;

use crate::error::RecipeError;
use crate::rng::{salt, IdSource, Rng};
use composer_commands::{Command, NoteSpec};
use composer_model::{
    Id, Key, Project, Region, RegionContent, Scale, Tick, Track, TICKS_PER_QUARTER,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

/// Instrument id the starter templates use for the built in synth.
pub const BUILTIN_INSTRUMENT: Id = Id(1);

/// What a recipe works on. Which fields it reads is part of its description.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub track: Option<Id>,
    pub region: Option<Id>,
}

/// A planned edit: a label for the undo history and the commands to run, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipePlan {
    pub label: String,
    pub commands: Vec<Command>,
}

/// Everything a recipe may read.
pub struct Ctx<'a> {
    pub project: &'a Project,
    pub scope: Scope,
    pub rng: Rng,
    pub ids: IdSource<'a>,
}

type PlanFn = fn(&mut Ctx<'_>, Value) -> Result<RecipePlan, RecipeError>;

struct Entry {
    name: &'static str,
    title: &'static str,
    summary: &'static str,
    /// Which scope fields the recipe reads.
    scope: &'static str,
    params: fn() -> Value,
    plan: PlanFn,
}

const ENTRIES: [Entry; 6] = [
    Entry {
        name: "extend_to_length",
        title: "Extend to length",
        summary: "Makes a clip loop and grows it to a target number of bars.",
        scope: "region, or track (its latest clip)",
        params: extend::params,
        plan: extend::plan,
    },
    Entry {
        name: "repeat_with_variation",
        title: "Repeat with variation",
        summary: "Repeats a clip after itself, each copy with small changes to velocity and in key pitch steps.",
        scope: "region, or track (its latest clip)",
        params: repeat::params,
        plan: repeat::plan,
    },
    Entry {
        name: "bass_from_chords",
        title: "Bass from chords",
        summary: "Writes a low bass line under a chord clip from the lowest note of each window.",
        scope: "region, or track (its latest clip)",
        params: bass::params,
        plan: bass::plan,
    },
    Entry {
        name: "pad_under_melody",
        title: "Pad under melody",
        summary: "Adds soft held chords of the key that follow a melody clip.",
        scope: "region, or track (its latest clip)",
        params: pad::params,
        plan: pad::plan,
    },
    Entry {
        name: "soften_velocity",
        title: "Soften velocity",
        summary: "Pulls loud notes down toward a ceiling.",
        scope: "region, track, or the whole project when empty",
        params: soften::params,
        plan: soften::plan,
    },
    Entry {
        name: "calm_sections",
        title: "Calm sections",
        summary: "A long, slow piece in sections: a drone, a pad and a sparse piano line, with markers.",
        scope: "none (adds new tracks)",
        params: calm::params,
        plan: calm::plan,
    },
];

/// The recipe list with parameter descriptions, as sent by `recipes.list`.
pub fn list() -> Value {
    Value::Array(
        ENTRIES
            .iter()
            .map(|e| {
                json!({
                    "name": e.name,
                    "title": e.title,
                    "summary": e.summary,
                    "scope": e.scope,
                    "params": (e.params)(),
                })
            })
            .collect(),
    )
}

pub fn names() -> Vec<&'static str> {
    ENTRIES.iter().map(|e| e.name).collect()
}

/// Plans one recipe. Deterministic: the same project, params, scope and seed give the same
/// commands.
pub fn plan(
    project: &Project,
    recipe: &str,
    params: Value,
    scope: Scope,
    seed: u64,
) -> Result<RecipePlan, RecipeError> {
    let Some(e) = ENTRIES.iter().find(|e| e.name == recipe) else {
        return Err(RecipeError::new(
            "unknown_recipe",
            format!("no recipe named {recipe}"),
        ));
    };
    let s = salt(e.name);
    let mut ctx = Ctx {
        project,
        scope,
        rng: Rng::new(seed, s),
        ids: IdSource::new(project, seed, s),
    };
    let params = if params.is_null() { json!({}) } else { params };
    (e.plan)(&mut ctx, params)
}

// Shared helpers.

pub(crate) fn parse_params<T: DeserializeOwned>(v: Value) -> Result<T, RecipeError> {
    if !v.is_object() {
        return Err(RecipeError::invalid("params must be an object"));
    }
    serde_json::from_value(v).map_err(|e| RecipeError::invalid(e.to_string()))
}

pub(crate) fn ranged(
    name: &str,
    v: Option<i64>,
    default: i64,
    lo: i64,
    hi: i64,
) -> Result<i64, RecipeError> {
    let v = v.unwrap_or(default);
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(RecipeError::invalid(format!("{name} must be {lo} to {hi}")))
    }
}

/// The midi clip a recipe works on: the scoped region, else the latest ending midi clip of
/// the scoped track.
pub(crate) fn target_region<'a>(
    p: &'a Project,
    scope: &Scope,
) -> Result<(&'a Track, &'a Region), RecipeError> {
    if let Some(id) = scope.region {
        let (t, r) = p
            .region(id)
            .ok_or_else(|| RecipeError::new("no_target", "that clip does not exist"))?;
        if !matches!(r.content, RegionContent::Midi { .. }) {
            return Err(RecipeError::new(
                "no_target",
                "this recipe needs a midi clip",
            ));
        }
        return Ok((t, r));
    }
    if let Some(id) = scope.track {
        let t = p
            .track(id)
            .ok_or_else(|| RecipeError::new("no_target", "that track does not exist"))?;
        return t
            .regions
            .values()
            .filter(|r| matches!(r.content, RegionContent::Midi { .. }))
            .max_by_key(|r| (r.start + r.length, r.start))
            .map(|r| (t, r))
            .ok_or_else(|| RecipeError::new("no_target", "that track has no midi clip"));
    }
    Err(RecipeError::new(
        "no_target",
        "give scope.region or scope.track",
    ))
}

pub(crate) fn midi_notes(r: &Region) -> Vec<&composer_model::Note> {
    match &r.content {
        RegionContent::Midi { notes } => notes.values().collect(),
        RegionContent::Audio | RegionContent::Clip { .. } => Vec::new(),
    }
}

/// Length of the bar that starts at or before `tick`.
pub(crate) fn bar_ticks(p: &Project, tick: Tick) -> Tick {
    p.tempo_map
        .meters
        .range(..=tick)
        .next_back()
        .map_or(TICKS_PER_QUARTER * 4, |(_, m)| m.ticks_per_bar())
}

/// Key from `root` and `scale` when both are given, else the key in effect at `tick`.
pub(crate) fn key_for(
    p: &Project,
    root: Option<u8>,
    scale: Option<Scale>,
    tick: Tick,
) -> Result<Option<Key>, RecipeError> {
    match (root, scale) {
        (Some(root), Some(scale)) => {
            let k = Key { root, scale };
            if k.is_valid() {
                Ok(Some(k))
            } else {
                Err(RecipeError::invalid("root must be 0 (C) to 11 (B)"))
            }
        }
        (None, None) => Ok(p.tempo_map.keys.range(..=tick).next_back().map(|(_, k)| *k)),
        _ => Err(RecipeError::invalid("give both root and scale, or neither")),
    }
}

pub(crate) fn key_params() -> Value {
    json!([
        { "name": "root", "type": "integer", "min": 0, "max": 11, "description": "Key root, 0 = C. Give with scale; leave both out to use the project key." },
        { "name": "scale", "type": "choice", "choices": composer_theory::Scale::ALL.iter().map(|s| s.name()).collect::<Vec<_>>(), "description": "Key scale." },
    ])
}

pub(crate) fn concat(a: Value, b: Value) -> Value {
    let mut out = a.as_array().cloned().unwrap_or_default();
    out.extend(b.as_array().cloned().unwrap_or_default());
    Value::Array(out)
}

/// The commands that create a new instrument track with a built in voice and a mix level.
pub(crate) fn new_track(
    ids: &mut IdSource<'_>,
    name: &str,
    sound: &str,
    volume: u16,
    pan: i16,
) -> (Id, Vec<Command>) {
    let id = ids.next();
    let mut cmds = vec![
        Command::AddTrack {
            id,
            name: name.to_string(),
            kind: composer_model::TrackKind::Instrument,
            index: None,
        },
        Command::SetInstrument {
            track: id,
            instrument: Some(BUILTIN_INSTRUMENT),
        },
        Command::SetTrackSound {
            track: id,
            sound: Some(sound.to_string()),
        },
        Command::SetTrackVolume { track: id, volume },
    ];
    if pan != 0 {
        cmds.push(Command::SetTrackPan { track: id, pan });
    }
    (id, cmds)
}

pub(crate) fn note_spec(
    ids: &mut IdSource<'_>,
    pitch: u8,
    velocity: u8,
    start: Tick,
    duration: Tick,
) -> NoteSpec {
    NoteSpec {
        id: ids.next(),
        pitch,
        velocity: velocity.clamp(1, 127),
        start,
        duration,
    }
}

/// MIDI pitch of pitch class `pc` in `octave` (4 holds middle C).
pub(crate) fn pitch_in(pc: u8, octave: i64) -> Option<u8> {
    u8::try_from((octave + 1) * 12 + i64::from(pc % 12))
        .ok()
        .filter(|p| *p <= 127)
}
