//! Audio effect chains (D45): an ordered list of devices on a track or on the master bus.
//! Parameters are small integers with a range per kind, so the model stays free of floats.

use crate::id::Id;
use serde::{Deserialize, Serialize};

/// Most devices one chain may hold.
pub const MAX_CHAIN_EFFECTS: usize = 16;
/// Most parameters any kind has (the blob record reserves this many slots).
pub const MAX_EFFECT_PARAMS: usize = 8;

/// Where a chain sits: after one track's strip, or on the master bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EffectOwner {
    Master,
    Track { track: Id },
}

impl EffectOwner {
    pub fn track(&self) -> Option<Id> {
        match self {
            EffectOwner::Master => None,
            EffectOwner::Track { track } => Some(*track),
        }
    }
}

/// The built in device types. Codes are stable and used by hashes and the blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    StereoTool,
    Delay,
    Crusher,
    Gate,
    Waveshaper,
    Tidal,
}

/// Name, range and default of one parameter. Every range fits an i16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: i16,
    pub max: i16,
    pub default: i16,
}

const fn ps(name: &'static str, min: i16, max: i16, default: i16) -> ParamSpec {
    ParamSpec {
        name,
        min,
        max,
        default,
    }
}

static STEREO_TOOL: [ParamSpec; 5] = [
    ps("volume", 0, 2000, 1000),
    ps("pan", -1000, 1000, 0),
    ps("invert_left", 0, 1, 0),
    ps("invert_right", 0, 1, 0),
    ps("mono", 0, 1, 0),
];
static DELAY: [ParamSpec; 4] = [
    ps("time_ms", 1, 2000, 250),
    ps("feedback", 0, 950, 350),
    ps("filter", 0, 1000, 300),
    ps("mix", 0, 1000, 300),
];
static CRUSHER: [ParamSpec; 3] = [
    ps("bits", 1, 16, 8),
    ps("rate_reduction", 1, 64, 1),
    ps("mix", 0, 1000, 1000),
];
static GATE: [ParamSpec; 3] = [
    ps("threshold", 0, 1000, 100),
    ps("attack_ms", 0, 200, 1),
    ps("release_ms", 1, 2000, 100),
];
static WAVESHAPER: [ParamSpec; 3] = [
    ps("drive", 0, 1000, 300),
    ps("shape", 0, 1000, 0),
    ps("mix", 0, 1000, 1000),
];
static TIDAL: [ParamSpec; 4] = [
    ps("rate_centihz", 1, 2000, 200),
    ps("depth", 0, 1000, 500),
    ps("shape", 0, 1000, 0),
    ps("pan_depth", 0, 1000, 0),
];

impl EffectKind {
    pub const ALL: [EffectKind; 6] = [
        EffectKind::StereoTool,
        EffectKind::Delay,
        EffectKind::Crusher,
        EffectKind::Gate,
        EffectKind::Waveshaper,
        EffectKind::Tidal,
    ];

    pub fn code(self) -> u8 {
        match self {
            EffectKind::StereoTool => 0,
            EffectKind::Delay => 1,
            EffectKind::Crusher => 2,
            EffectKind::Gate => 3,
            EffectKind::Waveshaper => 4,
            EffectKind::Tidal => 5,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// Plain snake case name, the same as the JSON form.
    pub fn name(self) -> &'static str {
        match self {
            EffectKind::StereoTool => "stereo_tool",
            EffectKind::Delay => "delay",
            EffectKind::Crusher => "crusher",
            EffectKind::Gate => "gate",
            EffectKind::Waveshaper => "waveshaper",
            EffectKind::Tidal => "tidal",
        }
    }

    pub fn params(self) -> &'static [ParamSpec] {
        match self {
            EffectKind::StereoTool => &STEREO_TOOL,
            EffectKind::Delay => &DELAY,
            EffectKind::Crusher => &CRUSHER,
            EffectKind::Gate => &GATE,
            EffectKind::Waveshaper => &WAVESHAPER,
            EffectKind::Tidal => &TIDAL,
        }
    }

    /// Index of a parameter by name.
    pub fn param_index(self, name: &str) -> Option<usize> {
        self.params().iter().position(|p| p.name == name)
    }

    pub fn default_params(self) -> Vec<i16> {
        self.params().iter().map(|p| p.default).collect()
    }
}

/// One device of a chain. `params` follows [`EffectKind::params`] in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effect {
    pub id: Id,
    pub kind: EffectKind,
    /// A bypassed device passes audio through untouched. Skipped in JSON while false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bypass: bool,
    pub params: Vec<i16>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Effect {
    /// A device with every parameter at its default.
    pub fn new(id: Id, kind: EffectKind) -> Self {
        Self {
            id,
            kind,
            bypass: false,
            params: kind.default_params(),
        }
    }

    /// True when the parameter count and every value match the kind.
    pub fn is_valid(&self) -> bool {
        let specs = self.kind.params();
        self.params.len() == specs.len()
            && self
                .params
                .iter()
                .zip(specs)
                .all(|(v, s)| (s.min..=s.max).contains(v))
    }
}

/// The chain of one owner, in processing order. A chain is never stored empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectChain {
    pub owner: EffectOwner,
    pub effects: Vec<Effect>,
}
