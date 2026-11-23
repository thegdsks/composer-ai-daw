//! Automation lanes in the frame domain (D36). A lane is a sorted list of points, and its value
//! at a frame is a pure function of that frame, so the offline render and the browser engine
//! agree bit for bit whatever block size the host uses.

use crate::mix::{any_solo, is_audible};
use crate::tempo::TempoClock;
use composer_model::{AutomationCurve, AutomationTarget, Project};

/// The engine re-reads every lane on frames that are a multiple of this, so the result never
/// depends on how the host splits the stream into blocks.
pub const CONTROL_PERIOD: u64 = 32;

/// One point of a compiled lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoPoint {
    pub frame: u64,
    /// In the unit of the target (permille).
    pub value: i16,
    pub curve: AutomationCurve,
}

/// What a compiled lane drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneParam {
    Volume,
    Pan,
    ReverbSend,
    MasterVolume,
}

impl LaneParam {
    pub fn code(self) -> u8 {
        match self {
            LaneParam::MasterVolume => 0,
            LaneParam::Volume => 1,
            LaneParam::Pan => 2,
            LaneParam::ReverbSend => 3,
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            0 => LaneParam::MasterVolume,
            1 => LaneParam::Volume,
            2 => LaneParam::Pan,
            3 => LaneParam::ReverbSend,
            _ => return None,
        })
    }
}

/// A lane ready for the engine: points sorted by frame, never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoLane {
    /// Index in `Project::tracks`; None for the master.
    pub track: Option<u32>,
    pub param: LaneParam,
    pub points: Vec<AutoPoint>,
    /// Linear gain a volume lane of 1000 permille gives (0.0 for a muted or unsoloed track), or
    /// the pan added to a pan lane. Unused by the other lanes.
    pub base: f32,
}

impl AutoLane {
    /// Value at a frame. Before the first point it is the first value, after the last it holds.
    /// Allocation free; the f32 arithmetic is plain add, multiply and divide, so it is the same
    /// on every target.
    pub fn value_at(&self, frame: u64) -> f32 {
        value_at(&self.points, frame)
    }
}

pub(crate) fn value_at(points: &[AutoPoint], frame: u64) -> f32 {
    let after = points.partition_point(|p| p.frame <= frame);
    let Some(a) = after.checked_sub(1).and_then(|i| points.get(i)) else {
        return points.first().map_or(0.0, |p| f32::from(p.value));
    };
    let Some(b) = points.get(after) else {
        return f32::from(a.value);
    };
    let (va, vb) = (f32::from(a.value), f32::from(b.value));
    let t = (frame - a.frame) as f32 / (b.frame - a.frame) as f32;
    match a.curve {
        AutomationCurve::Hold => va,
        AutomationCurve::Linear => va + (vb - va) * t,
        AutomationCurve::Smooth => va + (vb - va) * (t * t * (3.0 - 2.0 * t)),
    }
}

/// Compiles the lanes of a project to frames with the project tempo map. A lane of a track that
/// is not in the project is dropped. Lane order follows the model, so it is deterministic.
pub fn compile_automation(project: &Project, sample_rate: u32) -> Vec<AutoLane> {
    if project.automation.is_empty() {
        return Vec::new();
    }
    let clock = TempoClock::new(&project.tempo_map, sample_rate);
    let solo = any_solo(project);
    project
        .automation
        .iter()
        .filter_map(|lane| {
            let (track, param) = match lane.target {
                AutomationTarget::MasterVolume => (None, LaneParam::MasterVolume),
                AutomationTarget::TrackVolume { track } => (Some(track), LaneParam::Volume),
                AutomationTarget::TrackPan { track } => (Some(track), LaneParam::Pan),
                AutomationTarget::TrackReverbSend { track } => (Some(track), LaneParam::ReverbSend),
            };
            let track = match track {
                Some(id) => Some(project.track_index(id)? as u32),
                None => None,
            };
            let points = lane
                .points
                .iter()
                .map(|(tick, p)| AutoPoint {
                    frame: clock.frame_at(*tick),
                    value: p.value,
                    curve: p.curve,
                })
                .collect();
            let base = match (param, track) {
                (LaneParam::Volume, Some(i)) => {
                    let t = &project.tracks[i as usize];
                    f32::from(u8::from(is_audible(&t.mix, solo)))
                }
                (LaneParam::MasterVolume, _) => 1.0,
                _ => 0.0,
            };
            Some(AutoLane {
                track,
                param,
                points,
                base,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(points: &[(u64, i16, AutomationCurve)]) -> AutoLane {
        AutoLane {
            track: Some(0),
            param: LaneParam::Volume,
            base: 1.0,
            points: points
                .iter()
                .map(|&(frame, value, curve)| AutoPoint {
                    frame,
                    value,
                    curve,
                })
                .collect(),
        }
    }

    #[test]
    fn holds_before_and_after_and_interpolates() {
        use AutomationCurve::{Hold, Linear, Smooth};
        let l = lane(&[(100, 0, Linear), (200, 1000, Hold), (300, 500, Smooth)]);
        assert_eq!(l.value_at(0), 0.0);
        assert_eq!(l.value_at(150), 500.0);
        assert_eq!(l.value_at(200), 1000.0);
        assert_eq!(l.value_at(299), 1000.0);
        assert_eq!(l.value_at(5000), 500.0);
        let l = lane(&[(0, 0, Smooth), (100, 1000, Hold)]);
        assert_eq!(l.value_at(50), 500.0);
        assert!(l.value_at(25) < 250.0);
    }
}
