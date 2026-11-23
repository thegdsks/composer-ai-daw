//! Automation lanes (D36): curves of (tick, value) points bound to one mix parameter.
//! Values are integers in the unit of the parameter they drive, so the model stays free of floats.

use crate::id::Id;
use crate::project::{MAX_PAN, MAX_REVERB, MAX_VOLUME};
use crate::time::Tick;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most points one lane may hold.
pub const MAX_AUTOMATION_POINTS: usize = 10_000;

/// How the value travels from a point to the next one. After the last point the value holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCurve {
    /// Keeps the point value until the next point (a step).
    Hold,
    #[default]
    Linear,
    /// Eases in and out between the two values.
    Smooth,
}

impl AutomationCurve {
    /// Stable byte used by hashes and the blob.
    pub fn code(self) -> u8 {
        match self {
            AutomationCurve::Hold => 0,
            AutomationCurve::Linear => 1,
            AutomationCurve::Smooth => 2,
        }
    }
}

/// One point of a lane. `value` is in the unit of the target: permille of unity for volumes,
/// permille from -1000 to 1000 for pan, permille of the reverb send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub value: i16,
    #[serde(default)]
    pub curve: AutomationCurve,
}

/// What a lane drives. Ordered so lanes keep a fixed order in the file and the hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationTarget {
    MasterVolume,
    TrackVolume { track: Id },
    TrackPan { track: Id },
    TrackReverbSend { track: Id },
}

impl AutomationTarget {
    /// The track the lane belongs to, None for the master.
    pub fn track(&self) -> Option<Id> {
        match self {
            AutomationTarget::MasterVolume => None,
            AutomationTarget::TrackVolume { track }
            | AutomationTarget::TrackPan { track }
            | AutomationTarget::TrackReverbSend { track } => Some(*track),
        }
    }

    /// Stable byte used by hashes and the blob.
    pub fn code(&self) -> u8 {
        match self {
            AutomationTarget::MasterVolume => 0,
            AutomationTarget::TrackVolume { .. } => 1,
            AutomationTarget::TrackPan { .. } => 2,
            AutomationTarget::TrackReverbSend { .. } => 3,
        }
    }

    /// Smallest and largest accepted point value.
    pub fn range(&self) -> (i16, i16) {
        match self {
            AutomationTarget::MasterVolume | AutomationTarget::TrackVolume { .. } => {
                (0, MAX_VOLUME as i16)
            }
            AutomationTarget::TrackPan { .. } => (-MAX_PAN, MAX_PAN),
            AutomationTarget::TrackReverbSend { .. } => (0, MAX_REVERB as i16),
        }
    }

    pub fn accepts(&self, value: i16) -> bool {
        let (lo, hi) = self.range();
        (lo..=hi).contains(&value)
    }
}

/// A lane: its target and the points by tick. A lane is never stored empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationLane {
    pub target: AutomationTarget,
    pub points: BTreeMap<Tick, AutomationPoint>,
}
