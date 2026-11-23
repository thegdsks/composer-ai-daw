use serde::{Deserialize, Serialize};

/// Ticks per quarter note (PPQ). 960 divides evenly by 2 to 6, 8, 10, 12, 16
/// and 32, so triplets and 1/64 notes stay exact. One 4/4 bar is 3840 ticks.
pub const TICKS_PER_QUARTER: i64 = 960;

/// Position or length in ticks. Signed so differences are easy; stored values are never negative.
pub type Tick = i64;

/// Upper bound for any stored tick value, far above any real project and low
/// enough that sums cannot overflow.
pub const MAX_TICK: Tick = 1 << 40;

/// Time signature as numerator over a power of two denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const COMMON: TimeSignature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };

    /// True for 1..=32 beats over 1, 2, 4, 8, 16 or 32.
    pub fn is_valid(&self) -> bool {
        (1..=32).contains(&self.numerator) && matches!(self.denominator, 1 | 2 | 4 | 8 | 16 | 32)
    }

    /// Length of one bar in ticks (exact for every valid signature).
    pub fn ticks_per_bar(&self) -> Tick {
        TICKS_PER_QUARTER * 4 * i64::from(self.numerator) / i64::from(self.denominator)
    }
}
