//! Channel mix resolution shared by the offline render and the browser engine (D19).

use composer_model::{ChannelMix, Project, MAX_PAN, MAX_REVERB, UNITY_VOLUME};

/// True when at least one track is soloed.
pub fn any_solo(project: &Project) -> bool {
    project.tracks.iter().any(|t| t.mix.solo)
}

/// True when mute and solo let the channel sound.
pub fn is_audible(mix: &ChannelMix, any_solo: bool) -> bool {
    !mix.mute && (!any_solo || mix.solo)
}

/// Linear gain and pan (-1.0..1.0) of one channel. A muted channel, or an unsoloed one while
/// another channel is soloed, gets zero gain. The pan law itself lives in the engine strip.
pub fn resolve_mix(mix: &ChannelMix, any_solo: bool) -> (f32, f32) {
    let audible = is_audible(mix, any_solo);
    let gain = if audible {
        f32::from(mix.volume) / f32::from(UNITY_VOLUME)
    } else {
        0.0
    };
    (gain, f32::from(mix.pan) / f32::from(MAX_PAN))
}

/// Largest master volume (linear), the same ceiling as a channel volume.
pub const MAX_MASTER_VOLUME: f32 = 2.0;

/// Master bus settings (D26). Defaults leave the sum untouched and the reverb off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MasterBus {
    /// Linear gain on the final sum, 0.0..=[`MAX_MASTER_VOLUME`].
    pub volume: f32,
    /// Room size, 0.0..=1.0 (longer decay).
    pub reverb_size: f32,
    /// Level of the reverb return, 0.0..=1.0. Zero switches the reverb off.
    pub reverb_mix: f32,
}

impl Default for MasterBus {
    fn default() -> Self {
        Self {
            volume: 1.0,
            reverb_size: 0.5,
            reverb_mix: 0.0,
        }
    }
}

fn clamp_unit(x: f32, max: f32, fallback: f32) -> f32 {
    if x.is_finite() {
        x.clamp(0.0, max)
    } else {
        fallback
    }
}

impl MasterBus {
    /// Clamps every field into range; a non finite value takes the default.
    pub fn new(volume: f32, reverb_size: f32, reverb_mix: f32) -> Self {
        let d = Self::default();
        Self {
            volume: clamp_unit(volume, MAX_MASTER_VOLUME, d.volume),
            reverb_size: clamp_unit(reverb_size, 1.0, d.reverb_size),
            reverb_mix: clamp_unit(reverb_mix, 1.0, d.reverb_mix),
        }
    }

    /// True when every field is in range (what [`MasterBus::new`] would keep).
    pub fn is_valid(&self) -> bool {
        *self == Self::new(self.volume, self.reverb_size, self.reverb_mix)
    }
}

/// Clamps a per track reverb send to 0.0..=1.0 (non finite becomes 0).
pub fn clamp_send(send: f32) -> f32 {
    clamp_unit(send, 1.0, 0.0)
}

/// Master bus of a project as the engine takes it (D26). The model stores permille integers.
pub fn master_from_project(project: &Project) -> MasterBus {
    let m = &project.master;
    MasterBus::new(
        f32::from(m.volume) / f32::from(UNITY_VOLUME),
        f32::from(m.reverb_size) / f32::from(MAX_REVERB),
        f32::from(m.reverb_mix) / f32::from(MAX_REVERB),
    )
}

/// Reverb send of one track as a linear 0.0..=1.0 amount (D26).
pub fn send_from_project(track: &composer_model::Track) -> f32 {
    clamp_send(f32::from(track.reverb_send) / f32::from(MAX_REVERB))
}

// Delay lengths in frames at 48 kHz. Distinct primes keep the echoes from lining up; the right
// channel is offset for stereo width.
const COMBS_L: [usize; 4] = [1051, 1181, 1303, 1433];
const COMBS_R: [usize; 4] = [1087, 1217, 1327, 1471];
const ALLPASS_L: [usize; 2] = [347, 229];
const ALLPASS_R: [usize; 2] = [359, 241];
const DAMP: f32 = 0.3;
const ALLPASS_GAIN: f32 = 0.5;
/// Keeps the four combs from stacking into a loud return.
const INPUT_GAIN: f32 = 0.25;

fn scaled(len: usize, sample_rate: u32) -> usize {
    ((len as u64 * u64::from(sample_rate) + 24_000) / 48_000).max(2) as usize
}

struct Delay {
    buf: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len],
            at: 0,
        }
    }

    fn advance(&mut self) {
        self.at += 1;
        if self.at == self.buf.len() {
            self.at = 0;
        }
    }
}

struct Comb {
    delay: Delay,
    low: f32,
}

impl Comb {
    fn tick(&mut self, x: f32, feedback: f32) -> f32 {
        let y = self.delay.buf[self.delay.at];
        self.low = y * (1.0 - DAMP) + self.low * DAMP;
        if self.low.abs() < 1e-20 {
            self.low = 0.0;
        }
        self.delay.buf[self.delay.at] = x + self.low * feedback;
        self.delay.advance();
        y
    }
}

fn allpass(d: &mut Delay, x: f32) -> f32 {
    let v = d.buf[d.at];
    d.buf[d.at] = x + v * ALLPASS_GAIN;
    d.advance();
    v - x
}

struct Side {
    combs: [Comb; 4],
    allpasses: [Delay; 2],
}

impl Side {
    fn new(combs: [usize; 4], aps: [usize; 2], sample_rate: u32) -> Self {
        Self {
            combs: combs.map(|n| Comb {
                delay: Delay::new(scaled(n, sample_rate)),
                low: 0.0,
            }),
            allpasses: aps.map(|n| Delay::new(scaled(n, sample_rate))),
        }
    }

    fn tick(&mut self, x: f32, feedback: f32) -> f32 {
        let mut sum = 0.0;
        for c in &mut self.combs {
            sum += c.tick(x, feedback);
        }
        for a in &mut self.allpasses {
            sum = allpass(a, sum);
        }
        sum
    }
}

/// Stereo algorithmic reverb (D26). All memory is allocated in [`Reverb::new`]; `tick` only
/// adds and multiplies, so it never allocates and gives the same bits on every target.
pub struct Reverb {
    left: Side,
    right: Side,
    feedback: f32,
}

impl Reverb {
    pub fn new(sample_rate: u32) -> Self {
        let mut r = Self {
            left: Side::new(COMBS_L, ALLPASS_L, sample_rate),
            right: Side::new(COMBS_R, ALLPASS_R, sample_rate),
            feedback: 0.0,
        };
        r.set_size(MasterBus::default().reverb_size);
        r
    }

    /// Room size 0.0..=1.0 maps to a comb feedback of 0.70..=0.98.
    pub fn set_size(&mut self, size: f32) {
        self.feedback = 0.70 + 0.28 * clamp_unit(size, 1.0, 0.5);
    }

    /// One frame: the send bus in, the wet pair out.
    pub fn tick(&mut self, send_l: f32, send_r: f32) -> (f32, f32) {
        let x = (send_l + send_r) * INPUT_GAIN;
        (
            self.left.tick(x, self.feedback),
            self.right.tick(x, self.feedback),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(volume: u16, pan: i16, mute: bool, solo: bool) -> ChannelMix {
        ChannelMix {
            volume,
            pan,
            mute,
            solo,
        }
    }

    #[test]
    fn default_is_unity_center() {
        assert_eq!(resolve_mix(&ChannelMix::default(), false), (1.0, 0.0));
    }

    #[test]
    fn mute_and_solo_rules() {
        assert_eq!(resolve_mix(&m(1000, 0, true, false), false).0, 0.0);
        // Another track soloed: this one is silent, the soloed one plays.
        assert_eq!(resolve_mix(&m(1000, 0, false, false), true).0, 0.0);
        assert_eq!(resolve_mix(&m(1000, 0, false, true), true).0, 1.0);
        // Mute wins over solo.
        assert_eq!(resolve_mix(&m(1000, 0, true, true), true).0, 0.0);
    }

    #[test]
    fn volume_and_pan_scale() {
        assert_eq!(
            resolve_mix(&m(500, -1000, false, false), false),
            (0.5, -1.0)
        );
        assert_eq!(resolve_mix(&m(2000, 500, false, false), false), (2.0, 0.5));
    }

    #[test]
    fn master_clamps_and_reverb_is_deterministic_and_decays() {
        let m = MasterBus::new(9.0, f32::NAN, -1.0);
        assert_eq!((m.volume, m.reverb_size, m.reverb_mix), (2.0, 0.5, 0.0));
        assert!(m.is_valid());
        assert!(!MasterBus {
            volume: 3.0,
            ..MasterBus::default()
        }
        .is_valid());
        let run = || {
            let mut r = Reverb::new(48_000);
            r.set_size(0.2);
            let mut out = Vec::new();
            for i in 0..48_000 {
                let x = if i == 0 { 1.0 } else { 0.0 };
                out.push(r.tick(x, x));
            }
            out
        };
        let (a, b) = (run(), run());
        assert_eq!(a, b);
        assert!(a.iter().any(|(l, r)| *l != 0.0 && *r != 0.0));
        let late = a[40_000..].iter().fold(0.0f32, |m, (l, _)| m.max(l.abs()));
        let early = a[..8_000].iter().fold(0.0f32, |m, (l, _)| m.max(l.abs()));
        assert!(late < early * 0.1, "{late} {early}");
    }
}
