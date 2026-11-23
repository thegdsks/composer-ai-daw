use crate::error::FormatError;
use composer_model::{Tick, MAX_TICK};
use serde::{Deserialize, Serialize};

pub const MAX_APP_VERSION_LEN: usize = 64;

/// Default ceiling for `count_in_bars`; hosts override it from their own config (D21).
pub const DEFAULT_MAX_COUNT_IN_BARS: u32 = 4;

/// File level facts. Stamps come from the caller; the crate never reads a clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// Version of the app that wrote the file, for diagnostics.
    pub app_version: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: i64,
    pub modified_ms: i64,
    /// Transport preferences (D21). Absent when default, so older files read and hash alike.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<TransportPrefs>,
}

impl Meta {
    pub(crate) fn check(&self) -> Result<(), FormatError> {
        if self.app_version.len() > MAX_APP_VERSION_LEN {
            return Err(FormatError::OutOfRange("app version"));
        }
        if let Some(t) = &self.transport {
            t.check(u32::MAX)
                .map_err(|_| FormatError::OutOfRange("transport"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRange {
    pub on: bool,
    pub start: Tick,
    pub end: Tick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metronome {
    pub on: bool,
}

/// Transport session preferences (D21). Saved in the meta section, never in the
/// project, so they cannot change `state_hash`. Not undoable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportPrefs {
    #[serde(rename = "loop")]
    pub loop_range: LoopRange,
    pub metronome: Metronome,
    pub count_in_bars: u32,
    /// The view follows the playhead.
    pub follow: bool,
}

impl Default for TransportPrefs {
    fn default() -> Self {
        Self {
            loop_range: LoopRange {
                on: false,
                start: 0,
                end: 0,
            },
            metronome: Metronome { on: false },
            count_in_bars: 0,
            follow: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopPatch {
    pub on: Option<bool>,
    pub start: Option<Tick>,
    pub end: Option<Tick>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetronomePatch {
    pub on: Option<bool>,
}

/// `transport.set` parameters. Every field is optional and merges onto the current value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportPatch {
    pub tick: Option<Tick>,
    #[serde(rename = "loop")]
    pub loop_range: Option<LoopPatch>,
    pub metronome: Option<MetronomePatch>,
    pub count_in_bars: Option<u32>,
    pub follow: Option<bool>,
    #[serde(default)]
    pub dry_run: bool,
}

/// A rejected transport value: the field and a stable reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefsError {
    pub field: &'static str,
    pub reason: &'static str,
}

fn tick_ok(t: Tick) -> bool {
    (0..=MAX_TICK).contains(&t)
}

impl TransportPatch {
    pub fn is_empty(&self) -> bool {
        self.tick.is_none()
            && self.loop_range.is_none()
            && self.metronome.is_none()
            && self.count_in_bars.is_none()
            && self.follow.is_none()
    }

    /// Checks the playhead tick when present.
    pub fn check_tick(&self) -> Result<(), PrefsError> {
        match self.tick {
            Some(t) if !tick_ok(t) => Err(PrefsError {
                field: "tick",
                reason: "out of range",
            }),
            _ => Ok(()),
        }
    }
}

impl TransportPrefs {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The prefs after merging `patch`, validated; `self` is untouched.
    pub fn patched(
        &self,
        patch: &TransportPatch,
        max_count_in_bars: u32,
    ) -> Result<Self, PrefsError> {
        let mut n = *self;
        if let Some(l) = patch.loop_range {
            n.loop_range.on = l.on.unwrap_or(n.loop_range.on);
            n.loop_range.start = l.start.unwrap_or(n.loop_range.start);
            n.loop_range.end = l.end.unwrap_or(n.loop_range.end);
        }
        if let Some(m) = patch.metronome {
            n.metronome.on = m.on.unwrap_or(n.metronome.on);
        }
        n.count_in_bars = patch.count_in_bars.unwrap_or(n.count_in_bars);
        n.follow = patch.follow.unwrap_or(n.follow);
        n.check(max_count_in_bars)?;
        Ok(n)
    }

    pub fn check(&self, max_count_in_bars: u32) -> Result<(), PrefsError> {
        let l = &self.loop_range;
        if !tick_ok(l.start) || !tick_ok(l.end) {
            return Err(PrefsError {
                field: "loop",
                reason: "out of range",
            });
        }
        if l.on && l.start >= l.end {
            return Err(PrefsError {
                field: "loop",
                reason: "start must be before end",
            });
        }
        if self.count_in_bars > max_count_in_bars {
            return Err(PrefsError {
                field: "count_in_bars",
                reason: "out of range",
            });
        }
        Ok(())
    }
}
