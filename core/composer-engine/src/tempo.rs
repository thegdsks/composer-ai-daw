use composer_model::{TempoMap, Tick, DEFAULT_MBPM, MIN_MBPM, TICKS_PER_QUARTER};

#[derive(Debug, Clone, Copy)]
struct Segment {
    start_tick: Tick,
    mbpm: u32,
    start_frame: u64,
}

/// Converts ticks to sample frames with integer math.
///
/// Rule: inside a tempo segment the offset is
/// `dt * rate * 60000 / (mbpm * 960)` rounded half up to a whole frame. Each
/// segment start is the previous start plus the rounded length of the previous
/// segment, so a tempo change adds at most half a frame of error and a fixed
/// tempo adds none. Negative ticks count as 0.
#[derive(Debug, Clone)]
pub struct TempoClock {
    sample_rate: u32,
    segments: Vec<Segment>,
}

impl TempoClock {
    pub fn new(map: &TempoMap, sample_rate: u32) -> Self {
        let mut points: Vec<(Tick, u32)> = map
            .tempos
            .iter()
            .map(|(t, m)| ((*t).max(0), (*m).max(MIN_MBPM)))
            .collect();
        if points.first().map(|p| p.0) != Some(0) {
            points.insert(0, (0, DEFAULT_MBPM));
        }
        let mut segments: Vec<Segment> = Vec::with_capacity(points.len());
        for (tick, mbpm) in points {
            let start_frame = match segments.last() {
                Some(prev) => {
                    prev.start_frame + span(tick - prev.start_tick, prev.mbpm, sample_rate)
                }
                None => 0,
            };
            segments.push(Segment {
                start_tick: tick,
                mbpm,
                start_frame,
            });
        }
        Self {
            sample_rate,
            segments,
        }
    }

    /// Frame index of a tick.
    pub fn frame_at(&self, tick: Tick) -> u64 {
        let tick = tick.max(0);
        let i = self
            .segments
            .partition_point(|s| s.start_tick <= tick)
            .saturating_sub(1);
        let s = &self.segments[i];
        s.start_frame + span(tick - s.start_tick, s.mbpm, self.sample_rate)
    }
}

fn span(dt: Tick, mbpm: u32, sample_rate: u32) -> u64 {
    let num = dt.max(0) as u128 * u128::from(sample_rate) * 60_000;
    let den = u128::from(mbpm) * TICKS_PER_QUARTER as u128;
    ((num * 2 + den) / (den * 2)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_tempo_is_exact_and_rounds_half_up() {
        let c = TempoClock::new(&TempoMap::default(), 48_000);
        assert_eq!(c.frame_at(960), 24_000);
        assert_eq!(c.frame_at(7), 175);
        // 44.1 kHz, 120 bpm: one tick is 22.96875 frames.
        let c = TempoClock::new(&TempoMap::default(), 44_100);
        assert_eq!(c.frame_at(1), 23);
        assert_eq!(c.frame_at(960), 22_050);
    }

    #[test]
    fn tempo_change_is_honored() {
        let mut m = TempoMap::default();
        m.tempos.insert(3840, 60_000);
        let c = TempoClock::new(&m, 48_000);
        assert_eq!(c.frame_at(3840), 96_000);
        assert_eq!(c.frame_at(4800), 144_000);
    }
}
