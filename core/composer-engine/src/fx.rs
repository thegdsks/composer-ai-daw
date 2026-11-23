//! Audio effect devices (D45). A chain is a list of devices dispatched through an enum, so the
//! audio path has no trait objects. All memory is allocated when a chain is built; `process`
//! uses only add, multiply, divide, floor and `sin_turns`, so native and wasm give the same bits.

use crate::math::sin_turns;
use composer_model::{EffectKind, Project, MAX_EFFECT_PARAMS};

/// Longest delay time in milliseconds (the model range of `time_ms`).
const MAX_DELAY_MS: u64 = 2000;

/// One device as the engine takes it: kind, bypass flag and the integer parameters of the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxSpec {
    pub kind: EffectKind,
    pub bypass: bool,
    /// In the order of `EffectKind::params`; slots past the count of the kind are zero.
    pub params: [i16; MAX_EFFECT_PARAMS],
}

impl FxSpec {
    /// A device with every parameter at its default.
    pub fn new(kind: EffectKind) -> Self {
        let mut params = [0; MAX_EFFECT_PARAMS];
        for (slot, s) in params.iter_mut().zip(kind.params()) {
            *slot = s.default;
        }
        Self {
            kind,
            bypass: false,
            params,
        }
    }

    /// Replaces one parameter, clamped to its range. False for an unknown parameter.
    pub fn set(&mut self, param: usize, value: i16) -> bool {
        match kind_spec(self.kind, param) {
            Some((lo, hi)) => {
                self.params[param] = value.clamp(lo, hi);
                true
            }
            None => false,
        }
    }
}

fn kind_spec(kind: EffectKind, param: usize) -> Option<(i16, i16)> {
    kind.params().get(param).map(|s| (s.min, s.max))
}

/// The chain of one track (by index in the project) or of the master (`None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainSpec {
    pub track: Option<u32>,
    pub effects: Vec<FxSpec>,
}

/// Chains of a project as the engine takes them, in model order. Chains of unknown tracks are
/// dropped (a valid project has none).
pub fn compile_effects(project: &Project) -> Vec<ChainSpec> {
    let mut out = Vec::new();
    for c in &project.effects {
        let track = match c.owner.track() {
            None => None,
            Some(t) => match project.track_index(t) {
                Some(i) => Some(i as u32),
                None => continue,
            },
        };
        let effects = c
            .effects
            .iter()
            .map(|e| {
                let mut s = FxSpec::new(e.kind);
                s.bypass = e.bypass;
                for (i, v) in e.params.iter().enumerate().take(MAX_EFFECT_PARAMS) {
                    s.set(i, *v);
                }
                s
            })
            .collect();
        out.push(ChainSpec { track, effects });
    }
    out
}

fn p(spec: &FxSpec, i: usize) -> f32 {
    f32::from(spec.params[i])
}

/// Flushes values too small to matter so they never reach the denormal range.
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

struct StereoTool {
    gain_l: f32,
    gain_r: f32,
    sign_l: f32,
    sign_r: f32,
    mono: bool,
}

impl StereoTool {
    fn new(s: &FxSpec) -> Self {
        let mut t = Self {
            gain_l: 1.0,
            gain_r: 1.0,
            sign_l: 1.0,
            sign_r: 1.0,
            mono: false,
        };
        t.set(s);
        t
    }

    fn set(&mut self, s: &FxSpec) {
        let volume = p(s, 0) / 1000.0;
        let pan = p(s, 1) / 1000.0;
        // Balance: the far side is attenuated, the near side keeps unity.
        self.gain_l = volume * if pan > 0.0 { 1.0 - pan } else { 1.0 };
        self.gain_r = volume * if pan < 0.0 { 1.0 + pan } else { 1.0 };
        self.sign_l = if s.params[2] != 0 { -1.0 } else { 1.0 };
        self.sign_r = if s.params[3] != 0 { -1.0 } else { 1.0 };
        self.mono = s.params[4] != 0;
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut l, mut r) = (l * self.sign_l, r * self.sign_r);
        if self.mono {
            let m = (l + r) * 0.5;
            l = m;
            r = m;
        }
        (l * self.gain_l, r * self.gain_r)
    }
}

struct DelayFx {
    left: Vec<f32>,
    right: Vec<f32>,
    at: usize,
    frames: usize,
    feedback: f32,
    damp: f32,
    mix: f32,
    low_l: f32,
    low_r: f32,
    rate: u32,
}

impl DelayFx {
    fn new(s: &FxSpec, rate: u32) -> Self {
        let cap = (MAX_DELAY_MS * u64::from(rate) / 1000) as usize + 2;
        let mut d = Self {
            left: vec![0.0; cap],
            right: vec![0.0; cap],
            at: 0,
            frames: 1,
            feedback: 0.0,
            damp: 0.0,
            mix: 0.0,
            low_l: 0.0,
            low_r: 0.0,
            rate,
        };
        d.set(s);
        d
    }

    fn set(&mut self, s: &FxSpec) {
        let ms = u64::from(s.params[0].max(1).unsigned_abs());
        let frames = (ms * u64::from(self.rate) / 1000) as usize;
        self.frames = frames.clamp(1, self.left.len() - 1);
        self.feedback = p(s, 1) / 1000.0;
        self.damp = p(s, 2) / 1000.0 * 0.9;
        self.mix = p(s, 3) / 1000.0;
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let n = self.left.len();
        let read = (self.at + n - self.frames) % n;
        let (wl, wr) = (self.left[read], self.right[read]);
        self.low_l = flush(self.low_l * self.damp + wl * (1.0 - self.damp));
        self.low_r = flush(self.low_r * self.damp + wr * (1.0 - self.damp));
        self.left[self.at] = l + self.low_l * self.feedback;
        self.right[self.at] = r + self.low_r * self.feedback;
        self.at += 1;
        if self.at == n {
            self.at = 0;
        }
        let dry = 1.0 - self.mix;
        (l * dry + wl * self.mix, r * dry + wr * self.mix)
    }
}

struct CrusherFx {
    levels: f32,
    hold: u32,
    count: u32,
    held_l: f32,
    held_r: f32,
    mix: f32,
}

impl CrusherFx {
    fn new(s: &FxSpec) -> Self {
        let mut c = Self {
            levels: 128.0,
            hold: 1,
            count: 0,
            held_l: 0.0,
            held_r: 0.0,
            mix: 1.0,
        };
        c.set(s);
        c
    }

    fn set(&mut self, s: &FxSpec) {
        let bits = u32::from(s.params[0].clamp(1, 16).unsigned_abs());
        self.levels = (1u32 << (bits - 1)) as f32;
        self.hold = u32::from(s.params[1].clamp(1, 64).unsigned_abs());
        self.mix = p(s, 2) / 1000.0;
    }

    fn quantize(&self, x: f32) -> f32 {
        (x * self.levels + 0.5).floor() / self.levels
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.count == 0 {
            self.held_l = self.quantize(l);
            self.held_r = self.quantize(r);
        }
        self.count += 1;
        if self.count >= self.hold {
            self.count = 0;
        }
        let dry = 1.0 - self.mix;
        (
            l * dry + self.held_l * self.mix,
            r * dry + self.held_r * self.mix,
        )
    }
}

struct GateFx {
    threshold: f32,
    close_at: f32,
    attack: f32,
    release: f32,
    detect: f32,
    env: f32,
    open: bool,
    gain: f32,
}

impl GateFx {
    fn new(s: &FxSpec, rate: u32) -> Self {
        let mut g = Self {
            threshold: 0.0,
            close_at: 0.0,
            attack: 1.0,
            release: 1.0,
            // The level follower falls over about ten milliseconds.
            detect: 1.0 / (rate as f32 * 0.01).max(1.0),
            env: 0.0,
            open: false,
            gain: 0.0,
        };
        g.set(s, rate);
        g
    }

    fn set(&mut self, s: &FxSpec, rate: u32) {
        self.threshold = p(s, 0) / 1000.0;
        self.close_at = self.threshold * 0.8;
        let per_ms = rate as f32 / 1000.0;
        self.attack = 1.0 / (p(s, 1) * per_ms + 1.0);
        self.release = 1.0 / (p(s, 2) * per_ms + 1.0);
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level = l.abs().max(r.abs());
        if level > self.env {
            self.env = level;
        } else {
            self.env = flush(self.env + (level - self.env) * self.detect);
        }
        if self.env >= self.threshold {
            self.open = true;
        } else if self.env < self.close_at {
            self.open = false;
        }
        if self.open {
            self.gain += (1.0 - self.gain) * self.attack;
        } else {
            self.gain += (0.0 - self.gain) * self.release;
            // Below about -80 dB the gate counts as shut.
            if self.gain < 1e-4 {
                self.gain = 0.0;
            }
        }
        (l * self.gain, r * self.gain)
    }
}

struct WaveshaperFx {
    drive: f32,
    shape: f32,
    mix: f32,
}

impl WaveshaperFx {
    fn new(s: &FxSpec) -> Self {
        let mut w = Self {
            drive: 1.0,
            shape: 0.0,
            mix: 1.0,
        };
        w.set(s);
        w
    }

    fn set(&mut self, s: &FxSpec) {
        self.drive = 1.0 + p(s, 0) * 0.02;
        self.shape = p(s, 1) / 1000.0;
        self.mix = p(s, 2) / 1000.0;
    }

    fn shape_one(&self, x: f32) -> f32 {
        let v = x * self.drive;
        let soft = v / (1.0 + v.abs());
        let hard = v.clamp(-1.0, 1.0);
        let wet = soft + (hard - soft) * self.shape;
        x * (1.0 - self.mix) + wet * self.mix
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.shape_one(l), self.shape_one(r))
    }
}

struct TidalFx {
    phase: f32,
    inc: f32,
    depth: f32,
    shape: f32,
    pan_depth: f32,
}

impl TidalFx {
    fn new(s: &FxSpec, rate: u32) -> Self {
        let mut t = Self {
            phase: 0.0,
            inc: 0.0,
            depth: 0.0,
            shape: 0.0,
            pan_depth: 0.0,
        };
        t.set(s, rate);
        t
    }

    fn set(&mut self, s: &FxSpec, rate: u32) {
        self.inc = p(s, 0) / 100.0 / rate as f32;
        self.depth = p(s, 1) / 1000.0;
        self.shape = p(s, 2) / 1000.0;
        self.pan_depth = p(s, 3) / 1000.0;
    }

    /// Waveform in -1.0..=1.0 for a phase in 0.0..1.0: sine at shape 0, triangle at half,
    /// a steep sided square at 1.
    fn wave(&self) -> f32 {
        let ph = self.phase;
        let tri = if ph < 0.25 {
            ph * 4.0
        } else if ph < 0.75 {
            2.0 - ph * 4.0
        } else {
            ph * 4.0 - 4.0
        };
        let smooth = sin_turns(ph);
        if self.shape < 0.5 {
            smooth + (tri - smooth) * (self.shape * 2.0)
        } else {
            let square = (tri * 6.0).clamp(-1.0, 1.0);
            tri + (square - tri) * ((self.shape - 0.5) * 2.0)
        }
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let lfo = self.wave();
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let up = (lfo + 1.0) * 0.5;
        let gain = 1.0 - self.depth * (1.0 - up);
        let pan = self.pan_depth * lfo;
        let gl = if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let gr = if pan < 0.0 { 1.0 + pan } else { 1.0 };
        (l * gain * gl, r * gain * gr)
    }
}

enum Fx {
    StereoTool(StereoTool),
    Delay(DelayFx),
    Crusher(CrusherFx),
    Gate(GateFx),
    Waveshaper(WaveshaperFx),
    Tidal(TidalFx),
}

struct Device {
    bypass: bool,
    fx: Fx,
}

/// A built chain: per device state, ready for the audio thread.
pub(crate) struct Chain {
    devices: Vec<Device>,
    specs: Vec<FxSpec>,
    /// True while at least one device is not bypassed.
    active: bool,
    rate: u32,
}

impl Chain {
    /// Allocates every device. Setup time only.
    pub(crate) fn new(specs: &[FxSpec], rate: u32) -> Self {
        let devices = specs
            .iter()
            .map(|s| Device {
                bypass: s.bypass,
                fx: match s.kind {
                    EffectKind::StereoTool => Fx::StereoTool(StereoTool::new(s)),
                    EffectKind::Delay => Fx::Delay(DelayFx::new(s, rate)),
                    EffectKind::Crusher => Fx::Crusher(CrusherFx::new(s)),
                    EffectKind::Gate => Fx::Gate(GateFx::new(s, rate)),
                    EffectKind::Waveshaper => Fx::Waveshaper(WaveshaperFx::new(s)),
                    EffectKind::Tidal => Fx::Tidal(TidalFx::new(s, rate)),
                },
            })
            .collect();
        Self {
            devices,
            specs: specs.to_vec(),
            active: specs.iter().any(|s| !s.bypass),
            rate,
        }
    }

    /// False when every device is bypassed, so the host can keep its plain signal path.
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn len(&self) -> usize {
        self.devices.len()
    }

    /// Sets one parameter of one device, keeping its state. Never allocates.
    pub(crate) fn set_param(&mut self, at: usize, param: usize, value: i16) -> bool {
        let Some(mut s) = self.specs.get(at).copied() else {
            return false;
        };
        if !s.set(param, value) {
            return false;
        }
        self.specs[at] = s;
        self.update(at, &s)
    }

    /// Switches one device on or off. Never allocates.
    pub(crate) fn set_bypass(&mut self, at: usize, bypass: bool) -> bool {
        match self.devices.get_mut(at) {
            Some(d) => {
                d.bypass = bypass;
                self.specs[at].bypass = bypass;
                self.active = self.devices.iter().any(|d| !d.bypass);
                true
            }
            None => false,
        }
    }

    /// Re-reads the parameters of one device, keeping its state. Never allocates. False when
    /// the position or the kind does not match.
    pub(crate) fn update(&mut self, at: usize, s: &FxSpec) -> bool {
        let rate = self.rate;
        let Some(d) = self.devices.get_mut(at) else {
            return false;
        };
        d.bypass = s.bypass;
        match (&mut d.fx, s.kind) {
            (Fx::StereoTool(x), EffectKind::StereoTool) => x.set(s),
            (Fx::Delay(x), EffectKind::Delay) => x.set(s),
            (Fx::Crusher(x), EffectKind::Crusher) => x.set(s),
            (Fx::Gate(x), EffectKind::Gate) => x.set(s, rate),
            (Fx::Waveshaper(x), EffectKind::Waveshaper) => x.set(s),
            (Fx::Tidal(x), EffectKind::Tidal) => x.set(s, rate),
            _ => return false,
        }
        self.active = self.devices.iter().any(|d| !d.bypass);
        true
    }

    /// One stereo frame through every device in order. Bypassed devices are skipped.
    #[inline]
    pub(crate) fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut l, mut r) = (l, r);
        for d in &mut self.devices {
            if d.bypass {
                continue;
            }
            (l, r) = match &mut d.fx {
                Fx::StereoTool(x) => x.tick(l, r),
                Fx::Delay(x) => x.tick(l, r),
                Fx::Crusher(x) => x.tick(l, r),
                Fx::Gate(x) => x.tick(l, r),
                Fx::Waveshaper(x) => x.tick(l, r),
                Fx::Tidal(x) => x.tick(l, r),
            };
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn chain(kind: EffectKind, set: &[(usize, i16)]) -> Chain {
        let mut s = FxSpec::new(kind);
        for (i, v) in set {
            assert!(s.set(*i, *v));
        }
        Chain::new(&[s], SR)
    }

    /// Deterministic noise in -0.5..0.5 from an integer generator.
    fn noise(n: usize) -> Vec<(f32, f32)> {
        let mut x = 12345u32;
        let mut next = || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / 16_777_216.0 - 0.5
        };
        (0..n).map(|_| (next(), next())).collect()
    }

    fn run(c: &mut Chain, input: &[(f32, f32)]) -> Vec<(f32, f32)> {
        input.iter().map(|&(l, r)| c.process(l, r)).collect()
    }

    #[test]
    fn every_kind_is_deterministic_and_finite() {
        let input = noise(20_000);
        for kind in EffectKind::ALL {
            let a = run(&mut Chain::new(&[FxSpec::new(kind)], SR), &input);
            let b = run(&mut Chain::new(&[FxSpec::new(kind)], SR), &input);
            let bits = |v: &[(f32, f32)]| -> Vec<(u32, u32)> {
                v.iter().map(|(l, r)| (l.to_bits(), r.to_bits())).collect()
            };
            assert_eq!(bits(&a), bits(&b), "{kind:?}");
            assert!(a.iter().all(|(l, r)| l.is_finite() && r.is_finite()));
        }
    }

    #[test]
    fn bypassed_device_is_skipped_and_inactive() {
        let mut s = FxSpec::new(EffectKind::Waveshaper);
        s.bypass = true;
        let mut c = Chain::new(&[s], SR);
        assert!(!c.is_active());
        assert_eq!(c.process(0.3, -0.2), (0.3, -0.2));
        assert!(c.set_bypass(0, false) && c.is_active());
        assert_ne!(c.process(0.3, -0.2), (0.3, -0.2));
    }

    #[test]
    fn stereo_tool_volume_pan_invert_mono() {
        let mut c = chain(EffectKind::StereoTool, &[(0, 500)]);
        assert_eq!(c.process(1.0, -1.0), (0.5, -0.5));
        let mut c = chain(EffectKind::StereoTool, &[(1, 1000)]);
        assert_eq!(c.process(1.0, 1.0), (0.0, 1.0));
        let mut c = chain(EffectKind::StereoTool, &[(2, 1)]);
        assert_eq!(c.process(1.0, 1.0), (-1.0, 1.0));
        let mut c = chain(EffectKind::StereoTool, &[(4, 1)]);
        assert_eq!(c.process(1.0, 0.0), (0.5, 0.5));
        let mut c = Chain::new(&[FxSpec::new(EffectKind::StereoTool)], SR);
        assert_eq!(c.process(0.25, -0.75), (0.25, -0.75));
    }

    #[test]
    fn delay_repeats_at_the_set_time() {
        // 10 ms is 480 frames at 48 kHz; wet only, half feedback, no damping.
        let mut c = chain(EffectKind::Delay, &[(0, 10), (1, 500), (2, 0), (3, 1000)]);
        let mut input = vec![(0.0, 0.0); 2000];
        input[0] = (1.0, 0.5);
        let out = run(&mut c, &input);
        assert_eq!(out[480], (1.0, 0.5));
        assert_eq!(out[960], (0.5, 0.25));
        assert_eq!(out[1440], (0.25, 0.125));
        assert_eq!(out[1920], (0.125, 0.0625));
        for (i, o) in out.iter().enumerate() {
            if i % 480 != 0 || i == 0 || i > 1920 {
                assert_eq!(*o, (0.0, 0.0), "frame {i}");
            }
        }
    }

    #[test]
    fn crusher_quantizes_and_holds() {
        // 2 bits is steps of 0.5; hold every 4 frames.
        let mut c = chain(EffectKind::Crusher, &[(0, 2), (1, 4)]);
        let ramp: Vec<(f32, f32)> = (0..8).map(|i| (i as f32 * 0.1, 0.0)).collect();
        let out = run(&mut c, &ramp);
        assert_eq!(out[0].0, 0.0);
        assert!(out[..4].iter().all(|o| o.0 == out[0].0));
        assert!(out[4..].iter().all(|o| o.0 == out[4].0));
        assert_eq!(out[4].0, 0.5);
        let mut c = chain(EffectKind::Crusher, &[(0, 2)]);
        for (l, _) in run(&mut c, &noise(500)) {
            assert_eq!(l * 2.0, (l * 2.0).floor());
        }
    }

    #[test]
    fn gate_silences_below_threshold_and_passes_above() {
        let tone = |amp: f32, n: usize| -> Vec<(f32, f32)> {
            (0..n)
                .map(|i| {
                    let v = amp * sin_turns(i as f32 * 440.0 / SR as f32);
                    (v, v)
                })
                .collect()
        };
        // Threshold 0.1, fast release.
        let mut c = chain(EffectKind::Gate, &[(0, 100), (1, 1), (2, 10)]);
        let quiet = run(&mut c, &tone(0.05, 48_000));
        let tail = quiet[24_000..]
            .iter()
            .fold(0.0f32, |m, (l, r)| m.max(l.abs()).max(r.abs()));
        assert_eq!(tail, 0.0);
        let mut c = chain(EffectKind::Gate, &[(0, 100), (1, 1), (2, 10)]);
        let loud = run(&mut c, &tone(0.5, 48_000));
        let peak = loud[24_000..]
            .iter()
            .fold(0.0f32, |m, (l, _)| m.max(l.abs()));
        assert!(peak > 0.49, "{peak}");
        // Loud, then quiet: the gate closes again.
        let mut both = tone(0.5, 4_800);
        both.extend(tone(0.05, 48_000));
        let mut c = chain(EffectKind::Gate, &[(0, 100), (1, 1), (2, 10)]);
        let out = run(&mut c, &both);
        assert_eq!(out[out.len() - 1], (0.0, 0.0));
    }

    #[test]
    fn waveshaper_drives_and_hard_clips() {
        let mut c = chain(EffectKind::Waveshaper, &[(0, 1000), (1, 1000)]);
        assert_eq!(c.process(0.5, -0.5), (1.0, -1.0));
        let mut c = chain(EffectKind::Waveshaper, &[(0, 1000), (1, 0)]);
        let (l, _) = c.process(0.5, 0.0);
        assert!(l > 0.9 && l < 1.0);
        let mut c = chain(EffectKind::Waveshaper, &[(0, 0), (1, 0)]);
        let (l, _) = c.process(0.0, 0.0);
        assert_eq!(l, 0.0);
    }

    #[test]
    fn tidal_modulates_volume_and_pan_once_per_cycle() {
        // 1 Hz, full depth, sine: the gain sweeps through 0 and 1 in one second.
        let mut c = chain(EffectKind::Tidal, &[(0, 100), (1, 1000), (2, 0)]);
        let out = run(&mut c, &vec![(1.0, 1.0); 48_000]);
        let min = out.iter().fold(f32::MAX, |m, o| m.min(o.0));
        let max = out.iter().fold(f32::MIN, |m, o| m.max(o.0));
        assert!(min < 0.001 && max > 0.999, "{min} {max}");
        assert!(out.iter().all(|o| o.0 == o.1));
        // Pan only: volume untouched, channels differ.
        let mut c = chain(EffectKind::Tidal, &[(0, 100), (1, 0), (3, 1000)]);
        let out = run(&mut c, &vec![(1.0, 1.0); 48_000]);
        assert!(out.iter().any(|o| o.0 != o.1));
        assert!(out.iter().all(|o| o.0.max(o.1) == 1.0));
    }

    #[test]
    fn live_param_change_keeps_state_and_rejects_bad_input() {
        let mut c = chain(EffectKind::Delay, &[(0, 10), (3, 1000)]);
        c.process(1.0, 1.0);
        assert!(c.set_param(0, 0, 20));
        assert!(!c.set_param(0, 9, 20));
        assert!(!c.set_param(3, 0, 20));
        assert!(!c.set_bypass(3, true));
    }
}
