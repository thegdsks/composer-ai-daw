//! Sixteen built in voices generated in code (D29, U06, S01). Textbook DSP only: band limited saw and
//! pulse by a polynomial step correction, additive partials with a decay each, one pole and
//! state variable filters. Everything uses add, multiply, divide and [`sin_turns`], so native and
//! wasm output the same bits. No allocation: all state lives in [`Dsp`], a plain `Copy` value.

use crate::instrument::Adsr;
use crate::math::{cents_ratio, pitch_hz, sin_turns};

/// The built in voices, in catalogue order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinVoice {
    SoftPiano,
    WarmPad,
    WarmStrings,
    Pluck,
    SubBass,
    ChoirPad,
    Bell,
    SoftPulse,
    ElectricPiano,
    Mallet,
    GuitarPluck,
    BrassPad,
    Flute,
    Organ,
    BassPluck,
    Lead,
}

impl BuiltinVoice {
    pub const ALL: [BuiltinVoice; 16] = [
        BuiltinVoice::SoftPiano,
        BuiltinVoice::WarmPad,
        BuiltinVoice::WarmStrings,
        BuiltinVoice::Pluck,
        BuiltinVoice::SubBass,
        BuiltinVoice::ChoirPad,
        BuiltinVoice::Bell,
        BuiltinVoice::SoftPulse,
        BuiltinVoice::ElectricPiano,
        BuiltinVoice::Mallet,
        BuiltinVoice::GuitarPluck,
        BuiltinVoice::BrassPad,
        BuiltinVoice::Flute,
        BuiltinVoice::Organ,
        BuiltinVoice::BassPluck,
        BuiltinVoice::Lead,
    ];

    /// Catalogue id (the `id` of the `builtin` entry in sounds/catalogue.yaml).
    pub fn sound_id(self) -> &'static str {
        match self {
            BuiltinVoice::SoftPiano => "builtin-soft-piano",
            BuiltinVoice::WarmPad => "builtin-warm-pad",
            BuiltinVoice::WarmStrings => "builtin-warm-strings",
            BuiltinVoice::Pluck => "builtin-pluck",
            BuiltinVoice::SubBass => "builtin-sub-bass",
            BuiltinVoice::ChoirPad => "builtin-choir-pad",
            BuiltinVoice::Bell => "builtin-bell",
            BuiltinVoice::SoftPulse => "builtin-soft-pulse",
            BuiltinVoice::ElectricPiano => "builtin-electric-piano",
            BuiltinVoice::Mallet => "builtin-mallet",
            BuiltinVoice::GuitarPluck => "builtin-guitar-pluck",
            BuiltinVoice::BrassPad => "builtin-brass-pad",
            BuiltinVoice::Flute => "builtin-flute",
            BuiltinVoice::Organ => "builtin-organ",
            BuiltinVoice::BassPluck => "builtin-bass-pluck",
            BuiltinVoice::Lead => "builtin-lead",
        }
    }

    pub fn from_sound_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.sound_id() == id)
    }

    /// Waveform byte of the browser blob. 0 and 1 stay sine and saw.
    pub fn wave_code(self) -> u8 {
        match self {
            BuiltinVoice::SoftPiano => 2,
            BuiltinVoice::WarmPad => 3,
            BuiltinVoice::WarmStrings => 4,
            BuiltinVoice::Pluck => 5,
            BuiltinVoice::SubBass => 6,
            BuiltinVoice::ChoirPad => 7,
            BuiltinVoice::Bell => 8,
            BuiltinVoice::SoftPulse => 9,
            BuiltinVoice::ElectricPiano => 10,
            BuiltinVoice::Mallet => 11,
            BuiltinVoice::GuitarPluck => 12,
            BuiltinVoice::BrassPad => 13,
            BuiltinVoice::Flute => 14,
            BuiltinVoice::Organ => 15,
            BuiltinVoice::BassPluck => 16,
            BuiltinVoice::Lead => 17,
        }
    }

    pub fn from_wave_code(code: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.wave_code() == code)
    }

    /// Linear voice gain that keeps a velocity 100 note near the level of the default synth.
    pub fn default_gain(self) -> f32 {
        match self {
            BuiltinVoice::SoftPiano => 0.22,
            BuiltinVoice::WarmPad => 0.26,
            BuiltinVoice::WarmStrings => 0.4,
            BuiltinVoice::Pluck => 0.34,
            BuiltinVoice::SubBass => 0.42,
            BuiltinVoice::ChoirPad => 0.36,
            BuiltinVoice::Bell => 0.26,
            BuiltinVoice::SoftPulse => 0.30,
            BuiltinVoice::ElectricPiano => 0.26,
            BuiltinVoice::Mallet => 0.3,
            BuiltinVoice::GuitarPluck => 0.3,
            BuiltinVoice::BrassPad => 0.3,
            BuiltinVoice::Flute => 0.4,
            BuiltinVoice::Organ => 0.2,
            BuiltinVoice::BassPluck => 0.4,
            BuiltinVoice::Lead => 0.26,
        }
    }

    pub(crate) fn adsr(self) -> Adsr {
        let (attack, decay, sustain, release) = match self {
            BuiltinVoice::SoftPiano => (0.003, 14.0, 0.0, 0.35),
            BuiltinVoice::WarmPad => (0.9, 0.6, 0.85, 2.2),
            BuiltinVoice::WarmStrings => (0.22, 0.3, 0.9, 0.6),
            BuiltinVoice::Pluck => (0.001, 1.8, 0.0, 0.12),
            BuiltinVoice::SubBass => (0.006, 0.2, 0.85, 0.15),
            BuiltinVoice::ChoirPad => (0.7, 0.5, 0.9, 1.4),
            BuiltinVoice::Bell => (0.001, 9.0, 0.0, 1.0),
            BuiltinVoice::SoftPulse => (0.01, 0.3, 0.75, 0.25),
            BuiltinVoice::ElectricPiano => (0.003, 12.0, 0.0, 0.3),
            BuiltinVoice::Mallet => (0.001, 9.0, 0.0, 0.3),
            BuiltinVoice::GuitarPluck => (0.001, 12.0, 0.0, 0.18),
            BuiltinVoice::BrassPad => (0.12, 0.4, 0.8, 0.5),
            BuiltinVoice::Flute => (0.07, 0.2, 0.9, 0.15),
            BuiltinVoice::Organ => (0.008, 0.05, 1.0, 0.06),
            BuiltinVoice::BassPluck => (0.002, 0.9, 0.0, 0.12),
            BuiltinVoice::Lead => (0.008, 0.15, 0.8, 0.2),
        };
        Adsr {
            attack,
            decay,
            sustain,
            release,
        }
    }
}

const TAU: f32 = core::f32::consts::TAU;
const SLOTS: usize = 8;

/// Per voice DSP state. Fixed size, so a voice slot is reused without allocating.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Dsp {
    dt: f32,
    sr: f32,
    /// Seconds since the note started.
    t: f32,
    ph: [f32; SLOTS],
    inc: [f32; SLOTS],
    amp: [f32; SLOTS],
    dec: [f32; SLOTS],
    /// Filter memory: two poles per channel for the pads, formant filters for the choir.
    st: [f32; 12],
    cut: f32,
    cut_mul: f32,
    cut_floor: f32,
    lfo: f32,
    lfo_inc: f32,
    rng: u32,
}

impl Dsp {
    pub(crate) const ZERO: Dsp = Dsp {
        dt: 0.0,
        sr: 1.0,
        t: 0.0,
        ph: [0.0; SLOTS],
        inc: [0.0; SLOTS],
        amp: [0.0; SLOTS],
        dec: [1.0; SLOTS],
        st: [0.0; 12],
        cut: 0.0,
        cut_mul: 1.0,
        cut_floor: 0.0,
        lfo: 0.0,
        lfo_inc: 0.0,
        rng: 1,
    };
}

/// Step correction that turns the naive saw jump into a band limited one.
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

fn saw(ph: f32, inc: f32) -> f32 {
    2.0 * ph - 1.0 - blep(ph, inc)
}

/// Pulse of width `w` (0..1) as the difference of two saws, so it has no DC.
fn pulse(ph: f32, inc: f32, w: f32) -> f32 {
    let mut q = ph + w;
    if q >= 1.0 {
        q -= 1.0;
    }
    saw(ph, inc) - saw(q, inc)
}

fn wrap(ph: f32, inc: f32) -> f32 {
    let p = ph + inc;
    if p >= 1.0 {
        p - 1.0
    } else {
        p
    }
}

/// One pole low pass coefficient for a cutoff in hertz (x over 1 plus x, stable at any cutoff).
fn lp_k(fc: f32, sr: f32) -> f32 {
    let x = TAU * fc.min(0.45 * sr).max(1.0) / sr;
    x / (1.0 + x)
}

/// Two cascaded one pole filters: 12 dB per octave. `s` is the state slice of one channel.
fn lp2(s: &mut [f32], x: f32, k: f32) -> f32 {
    s[0] += k * (x - s[0]);
    s[1] += k * (s[0] - s[1]);
    s[1]
}

fn flush(x: f32) -> f32 {
    if x < 1e-9 {
        0.0
    } else {
        x
    }
}

fn hz(pitch: u8) -> f32 {
    pitch_hz(pitch) as f32
}

fn ratio(cents: i32) -> f32 {
    cents_ratio(cents) as f32
}

pub(crate) fn start(voice: BuiltinVoice, d: &mut Dsp, pitch: u8, velocity: u8, sr: u32) {
    *d = Dsp::ZERO;
    let sr_f = sr as f32;
    d.sr = sr_f;
    d.dt = 1.0 / sr_f;
    let f0 = hz(pitch);
    let nyq = 0.45 * sr_f;
    let vel = f32::from(velocity.min(127)) / 127.0;
    let semis = f32::from(pitch) - 60.0;
    match voice {
        BuiltinVoice::SoftPiano => {
            let base = [1.0, 0.55, 0.34, 0.2, 0.12, 0.07];
            let bright = 0.4 + 0.6 * vel;
            let tau1 = (7.0 - 0.045 * semis).clamp(1.2, 12.0);
            let mut b = 1.0;
            for k in 0..6 {
                let n = (k + 1) as f32;
                // A touch of stiffness stretches the upper partials, as on a real string.
                let f = n * f0 * (1.0 + 0.00025 * n * n);
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = base[k] * b;
                    d.dec[k] = 1.0 - d.dt * (1.0 + 0.8 * k as f32) / tau1;
                }
                b *= bright;
            }
            // Second string of the first two partials, a few cents sharp: slow beating.
            let sharp = ratio(3);
            for (slot, k) in [(6usize, 0usize), (7, 1)] {
                d.inc[slot] = d.inc[k] * sharp;
                d.amp[slot] = d.amp[k] * 0.6;
                d.dec[slot] = d.dec[k];
            }
            d.cut = 2500.0 + 5500.0 * vel;
        }
        BuiltinVoice::WarmPad => {
            for (i, c) in [-8, 8, 0].into_iter().enumerate() {
                d.inc[i] = f0 * ratio(c) / sr_f;
                d.ph[i] = 0.33 * i as f32;
            }
            d.inc[3] = 0.5 * f0 / sr_f;
            d.lfo_inc = 0.17 * d.dt;
            d.cut = 450.0 + 1.1 * f0 + 500.0 * vel;
        }
        BuiltinVoice::WarmStrings => {
            for (i, c) in [-6, 2, -2, 6].into_iter().enumerate() {
                d.inc[i] = f0 * ratio(c) / sr_f;
                d.ph[i] = 0.21 * i as f32;
            }
            d.lfo_inc = 5.3 * d.dt;
            d.cut = 1500.0 + 2500.0 * vel + 1.5 * f0;
        }
        BuiltinVoice::Pluck => {
            d.inc[0] = f0 * ratio(-4) / sr_f;
            d.inc[1] = f0 * ratio(4) / sr_f;
            d.ph[1] = 0.4;
            d.inc[2] = f0 / sr_f;
            d.cut = 450.0 + 4500.0 * (0.3 + 0.7 * vel);
            d.cut_floor = 400.0;
            d.cut_mul = 1.0 - d.dt / 0.28;
        }
        BuiltinVoice::SubBass => {
            for (i, a) in [1.0, 0.22, 0.06].into_iter().enumerate() {
                let n = (i + 1) as f32;
                if n * f0 < nyq {
                    d.inc[i] = n * f0 / sr_f;
                    d.amp[i] = a;
                }
            }
        }
        BuiltinVoice::ChoirPad => {
            d.inc[0] = f0 * ratio(-6) / sr_f;
            d.inc[1] = f0 * ratio(6) / sr_f;
            d.ph[1] = 0.37;
            d.lfo_inc = 4.6 * d.dt;
            d.rng = 0x2F6E_2B1D ^ u32::from(pitch);
            d.cut = 1300.0;
        }
        BuiltinVoice::Bell => {
            let ratios = [0.5, 1.0, 1.19, 1.5, 2.0, 2.74, 3.76, 5.43];
            let amps = [0.35, 1.0, 0.45, 0.4, 0.5, 0.25, 0.15, 0.08];
            let taus = [3.5, 3.0, 2.0, 1.8, 1.5, 0.9, 0.6, 0.3];
            let scale = (1.8 - 0.02 * semis).clamp(0.35, 2.5);
            let bright = 0.55 + 0.45 * vel;
            let mut b = 1.0;
            for k in 0..SLOTS {
                let f = f0 * ratios[k];
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = amps[k] * b;
                    d.dec[k] = 1.0 - d.dt / (taus[k] * scale);
                }
                if k > 0 {
                    b *= bright;
                }
            }
        }
        BuiltinVoice::SoftPulse => {
            d.inc[0] = f0 * ratio(-5) / sr_f;
            d.inc[1] = f0 * ratio(5) / sr_f;
            d.ph[1] = 0.5;
            d.lfo_inc = 0.5 * d.dt;
            d.cut = 1800.0 + 2200.0 * vel;
        }
        BuiltinVoice::ElectricPiano => {
            // Tine and tone bar: a strong fundamental, a soft octave and a short bright bark.
            let ratios = [1.0, 2.0, 4.0, 7.0];
            let amps = [1.0, 0.3, 0.1, 0.32];
            let taus = [6.0, 3.0, 1.2, 0.25];
            let scale = (1.6 - 0.02 * semis).clamp(0.4, 2.5);
            for k in 0..4 {
                let f = f0 * ratios[k];
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = amps[k] * if k == 3 { vel * vel } else { 0.5 + 0.5 * vel };
                    d.dec[k] = 1.0 - d.dt / (taus[k] * scale);
                }
            }
            d.lfo_inc = 4.6 * d.dt;
            d.cut = 2800.0 + 4000.0 * vel;
        }
        BuiltinVoice::Mallet => {
            let ratios = [1.0, 3.98, 10.0];
            let amps = [1.0, 0.5, 0.2];
            let taus = [1.4, 0.35, 0.08];
            let scale = (1.5 - 0.02 * semis).clamp(0.3, 2.2);
            let bright = 0.4 + 0.6 * vel;
            for k in 0..3 {
                let f = f0 * ratios[k];
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = amps[k] * if k == 0 { 1.0 } else { bright };
                    d.dec[k] = 1.0 - d.dt / (taus[k] * scale);
                }
            }
        }
        BuiltinVoice::GuitarPluck => {
            // Pluck position near a fifth of the string leaves its mark on the partial levels.
            let scale = (1.6 - 0.025 * semis).clamp(0.4, 2.5);
            for k in 0..6 {
                let n = (k + 1) as f32;
                let f = n * f0 * (1.0 + 0.0001 * n * n);
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = (sin_turns(0.09 * n).abs() / (n * 0.8 + 0.2)) * (0.6 + 0.4 * vel);
                    d.dec[k] = 1.0 - d.dt * (1.0 + 0.9 * k as f32) / (3.0 * scale);
                }
            }
            // Pick noise: a few milliseconds of hiss.
            d.cut_floor = 0.2 * vel;
            d.cut_mul = 1.0 - d.dt / 0.005;
            d.rng = 0x1B87_3593 ^ u32::from(pitch);
            d.cut = 1800.0 + 3200.0 * vel;
        }
        BuiltinVoice::BrassPad => {
            for (i, c) in [-5, 5, 0].into_iter().enumerate() {
                d.inc[i] = f0 * ratio(c) / sr_f;
                d.ph[i] = 0.29 * i as f32;
            }
            d.cut = 1300.0 + 2200.0 * vel + 1.2 * f0;
        }
        BuiltinVoice::Flute => {
            d.inc[0] = f0 / sr_f;
            if 2.0 * f0 < nyq {
                d.inc[1] = 2.0 * f0 / sr_f;
            }
            if 3.0 * f0 < nyq {
                d.inc[2] = 3.0 * f0 / sr_f;
            }
            d.lfo_inc = 5.0 * d.dt;
            d.rng = 0x3C6E_F372 ^ u32::from(pitch);
            // Chiff at the start of the note.
            d.cut_floor = 0.1 + 0.1 * vel;
            d.cut_mul = 1.0 - d.dt / 0.05;
            d.cut = 2.0 * f0 + 600.0;
        }
        BuiltinVoice::Organ => {
            let ratios = [0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0];
            let amps = [0.8, 1.0, 0.45, 0.7, 0.35, 0.3, 0.12, 0.18];
            for k in 0..SLOTS {
                let f = f0 * ratios[k];
                if f < nyq {
                    d.inc[k] = f / sr_f;
                    d.amp[k] = amps[k];
                }
            }
            d.lfo_inc = 0.8 * d.dt;
            d.rng = 0x5851_F42D ^ u32::from(pitch);
            d.cut_floor = 0.12;
            d.cut_mul = 1.0 - d.dt / 0.004;
            d.cut = 7000.0;
        }
        BuiltinVoice::BassPluck => {
            d.inc[0] = f0 / sr_f;
            d.inc[1] = f0 * ratio(3) / sr_f;
            d.cut_floor = 90.0;
            d.cut = 150.0 + 1800.0 * (0.3 + 0.7 * vel);
            d.cut_mul = 1.0 - d.dt / 0.18;
        }
        BuiltinVoice::Lead => {
            d.inc[0] = f0 * ratio(-7) / sr_f;
            d.inc[1] = f0 * ratio(7) / sr_f;
            d.ph[1] = 0.3;
            d.inc[2] = f0 / sr_f;
            d.lfo_inc = 5.5 * d.dt;
            d.cut = 2200.0 + 3000.0 * vel + 2.0 * f0;
        }
    }
}

/// Next stereo frame of the voice before the envelope.
pub(crate) fn render(voice: BuiltinVoice, d: &mut Dsp) -> (f32, f32) {
    d.t += d.dt;
    d.lfo = wrap(d.lfo, d.lfo_inc);
    match voice {
        BuiltinVoice::SoftPiano => {
            let mut main = 0.0;
            let mut det = 0.0;
            for k in 0..SLOTS {
                if d.amp[k] > 0.0 {
                    let s = sin_turns(d.ph[k]) * d.amp[k];
                    if k < 6 {
                        main += s;
                    } else {
                        det += s;
                    }
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                    d.amp[k] = flush(d.amp[k] * d.dec[k]);
                }
            }
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], main + 0.25 * det, k);
            let r = lp2(&mut d.st[2..4], main + 0.75 * det, k);
            (l, r)
        }
        BuiltinVoice::WarmPad => {
            let mut o = [0.0; 3];
            for (i, s) in o.iter_mut().enumerate() {
                *s = saw(d.ph[i], d.inc[i]);
                d.ph[i] = wrap(d.ph[i], d.inc[i]);
            }
            let sub = sin_turns(d.ph[3]);
            d.ph[3] = wrap(d.ph[3], d.inc[3]);
            let breathe = 1.0 + 0.3 * sin_turns(d.lfo);
            let k = lp_k(d.cut * breathe, d.sr);
            let l = lp2(&mut d.st[0..2], o[0] + 0.7 * o[2] + 0.5 * sub, k);
            let r = lp2(&mut d.st[2..4], o[1] + 0.7 * o[2] + 0.5 * sub, k);
            (l, r)
        }
        BuiltinVoice::WarmStrings => {
            // Vibrato fades in over the first 0.7 s, as a player would bring it in.
            let ramp = (d.t / 0.7).min(1.0);
            let vib = 1.0 + 0.0023 * ramp * sin_turns(d.lfo);
            let mut o = [0.0; 4];
            for (i, s) in o.iter_mut().enumerate() {
                let inc = d.inc[i] * vib;
                *s = saw(d.ph[i], inc);
                d.ph[i] = wrap(d.ph[i], inc);
            }
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], o[0] + o[1], k);
            let r = lp2(&mut d.st[2..4], o[2] + o[3], k);
            (0.6 * l, 0.6 * r)
        }
        BuiltinVoice::Pluck => {
            let a = saw(d.ph[0], d.inc[0]);
            let b = saw(d.ph[1], d.inc[1]);
            let body = sin_turns(d.ph[2]);
            d.ph[0] = wrap(d.ph[0], d.inc[0]);
            d.ph[1] = wrap(d.ph[1], d.inc[1]);
            d.ph[2] = wrap(d.ph[2], d.inc[2]);
            let fc = d.cut_floor + d.cut;
            d.cut *= d.cut_mul;
            let k = lp_k(fc, d.sr);
            let l = lp2(&mut d.st[0..2], a + 0.4 * b, k);
            let r = lp2(&mut d.st[2..4], b + 0.4 * a, k);
            (l + 0.3 * body, r + 0.3 * body)
        }
        BuiltinVoice::SubBass => {
            let mut x = 0.0;
            for k in 0..3 {
                if d.amp[k] > 0.0 {
                    x += sin_turns(d.ph[k]) * d.amp[k];
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                }
            }
            // Gentle saturation adds a little upper warmth so the note reads on small speakers.
            let y = x / (1.0 + 0.5 * x.abs());
            (y, y)
        }
        BuiltinVoice::ChoirPad => {
            let ramp = (d.t / 1.0).min(1.0);
            let vib = 1.0 + 0.0035 * ramp * sin_turns(d.lfo);
            let a = saw(d.ph[0], d.inc[0] * vib);
            let b = saw(d.ph[1], d.inc[1] * vib);
            d.ph[0] = wrap(d.ph[0], d.inc[0] * vib);
            d.ph[1] = wrap(d.ph[1], d.inc[1] * vib);
            // Xorshift breath noise, integer only so it is identical everywhere.
            let mut x = d.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            d.rng = x;
            let breath = 0.05 * ((x >> 8) as f32 * (1.0 / 8_388_608.0) - 1.0);
            let src = [a + 0.5 * b + breath, b + 0.5 * a + breath];
            // Three resonances of an open "ah": state variable filters, band pass outputs.
            let formants = [(750.0, 0.12, 1.0), (1150.0, 0.1, 0.45), (2600.0, 0.08, 0.2)];
            let mut out = [0.0; 2];
            for (ch, o) in out.iter_mut().enumerate() {
                for (i, (fc, q, g)) in formants.iter().enumerate() {
                    let f = 2.0 * sin_turns(fc / (2.0 * d.sr));
                    let at = ch * 6 + i * 2;
                    let (lp, bp) = (d.st[at], d.st[at + 1]);
                    let lp = lp + f * bp;
                    let hp = src[ch] - lp - q * bp;
                    let bp = bp + f * hp;
                    d.st[at] = lp;
                    d.st[at + 1] = bp;
                    *o += bp * g;
                }
            }
            (0.35 * out[0], 0.35 * out[1])
        }
        BuiltinVoice::Bell => {
            const PAN: [f32; SLOTS] = [0.5, 0.5, 0.35, 0.65, 0.4, 0.6, 0.3, 0.7];
            let (mut l, mut r) = (0.0, 0.0);
            for k in 0..SLOTS {
                if d.amp[k] > 0.0 {
                    let s = sin_turns(d.ph[k]) * d.amp[k];
                    l += s * (1.0 - PAN[k]) * 1.4;
                    r += s * PAN[k] * 1.4;
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                    d.amp[k] = flush(d.amp[k] * d.dec[k]);
                }
            }
            (l, r)
        }
        BuiltinVoice::SoftPulse => {
            let w = 0.32 + 0.12 * sin_turns(d.lfo);
            let a = pulse(d.ph[0], d.inc[0], w);
            let b = pulse(d.ph[1], d.inc[1], w);
            d.ph[0] = wrap(d.ph[0], d.inc[0]);
            d.ph[1] = wrap(d.ph[1], d.inc[1]);
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], a, k);
            let r = lp2(&mut d.st[2..4], b, k);
            (l, r)
        }
        BuiltinVoice::ElectricPiano => {
            let mut x = 0.0;
            for k in 0..4 {
                if d.amp[k] > 0.0 {
                    x += sin_turns(d.ph[k]) * d.amp[k];
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                    d.amp[k] = flush(d.amp[k] * d.dec[k]);
                }
            }
            // Tremolo that swings between the two sides, as a suitcase piano does.
            let m = 0.2 * sin_turns(d.lfo);
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], x * (1.0 + m), k);
            let r = lp2(&mut d.st[2..4], x * (1.0 - m), k);
            (l, r)
        }
        BuiltinVoice::Mallet => {
            const PAN: [f32; 3] = [0.5, 0.4, 0.6];
            let (mut l, mut r) = (0.0, 0.0);
            for k in 0..3 {
                if d.amp[k] > 0.0 {
                    let s = sin_turns(d.ph[k]) * d.amp[k];
                    l += s * (1.0 - PAN[k]) * 1.6;
                    r += s * PAN[k] * 1.6;
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                    d.amp[k] = flush(d.amp[k] * d.dec[k]);
                }
            }
            (l, r)
        }
        BuiltinVoice::GuitarPluck => {
            let (mut a, mut b) = (0.0, 0.0);
            for k in 0..6 {
                if d.amp[k] > 0.0 {
                    let s = sin_turns(d.ph[k]) * d.amp[k];
                    if k % 2 == 0 {
                        a += s;
                    } else {
                        b += s;
                    }
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                    d.amp[k] = flush(d.amp[k] * d.dec[k]);
                }
            }
            let mut x = d.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            d.rng = x;
            let pick = d.cut_floor * ((x >> 8) as f32 * (1.0 / 8_388_608.0) - 1.0);
            d.cut_floor = flush(d.cut_floor * d.cut_mul);
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], 1.1 * a + 0.8 * b + pick, k);
            let r = lp2(&mut d.st[2..4], 0.8 * a + 1.1 * b + pick, k);
            (l, r)
        }
        BuiltinVoice::BrassPad => {
            let mut o = [0.0; 3];
            for (i, s) in o.iter_mut().enumerate() {
                *s = saw(d.ph[i], d.inc[i]);
                d.ph[i] = wrap(d.ph[i], d.inc[i]);
            }
            // The filter opens over about a third of a second, like a section swelling in.
            let swell = 0.25 + 0.75 * (d.t / 0.35).min(1.0);
            let k = lp_k(d.cut * swell, d.sr);
            let l = lp2(&mut d.st[0..2], o[0] + 0.8 * o[2], k);
            let r = lp2(&mut d.st[2..4], o[1] + 0.8 * o[2], k);
            (0.7 * l, 0.7 * r)
        }
        BuiltinVoice::Flute => {
            // Vibrato comes in after about a third of a second.
            let ramp = ((d.t - 0.35) / 0.6).clamp(0.0, 1.0);
            let vib = 1.0 + 0.004 * ramp * sin_turns(d.lfo);
            let mut y = 0.0;
            for (k, g) in [1.0, 0.22, 0.07].into_iter().enumerate() {
                if d.inc[k] > 0.0 {
                    y += g * sin_turns(d.ph[k]);
                    d.ph[k] = wrap(d.ph[k], d.inc[k] * vib);
                }
            }
            let mut x = d.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            d.rng = x;
            let n = (x >> 8) as f32 * (1.0 / 8_388_608.0) - 1.0;
            let breath = (0.025 + d.cut_floor) * n;
            d.cut_floor = flush(d.cut_floor * d.cut_mul);
            let k = lp_k(d.cut, d.sr);
            d.st[0] += k * (breath - d.st[0]);
            let o = 0.5 * y + d.st[0] * 2.0;
            (o, o)
        }
        BuiltinVoice::Organ => {
            let mut x = 0.0;
            for k in 0..SLOTS {
                if d.amp[k] > 0.0 {
                    x += sin_turns(d.ph[k]) * d.amp[k];
                    d.ph[k] = wrap(d.ph[k], d.inc[k]);
                }
            }
            let mut r = d.rng;
            r ^= r << 13;
            r ^= r >> 17;
            r ^= r << 5;
            d.rng = r;
            let click = d.cut_floor * ((r >> 8) as f32 * (1.0 / 8_388_608.0) - 1.0);
            d.cut_floor = flush(d.cut_floor * d.cut_mul);
            // Slow rotating speaker: a gentle swing of level between the two sides.
            let m = 0.12 * sin_turns(d.lfo);
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], 0.28 * x * (1.0 + m) + click, k);
            let rr = lp2(&mut d.st[2..4], 0.28 * x * (1.0 - m) + click, k);
            (l, rr)
        }
        BuiltinVoice::BassPluck => {
            let sub = sin_turns(d.ph[0]);
            let s = saw(d.ph[1], d.inc[1]);
            d.ph[0] = wrap(d.ph[0], d.inc[0]);
            d.ph[1] = wrap(d.ph[1], d.inc[1]);
            let fc = d.cut_floor + d.cut;
            d.cut *= d.cut_mul;
            let k = lp_k(fc, d.sr);
            let body = lp2(&mut d.st[0..2], s, k);
            let x = 0.75 * sub + 0.6 * body;
            let y = x / (1.0 + 0.5 * x.abs());
            (y, y)
        }
        BuiltinVoice::Lead => {
            let ramp = ((d.t - 0.25) / 0.5).clamp(0.0, 1.0);
            let vib = 1.0 + 0.004 * ramp * sin_turns(d.lfo);
            let a = saw(d.ph[0], d.inc[0] * vib);
            let b = saw(d.ph[1], d.inc[1] * vib);
            let q = pulse(d.ph[2], d.inc[2] * vib, 0.5);
            d.ph[0] = wrap(d.ph[0], d.inc[0] * vib);
            d.ph[1] = wrap(d.ph[1], d.inc[1] * vib);
            d.ph[2] = wrap(d.ph[2], d.inc[2] * vib);
            let k = lp_k(d.cut, d.sr);
            let l = lp2(&mut d.st[0..2], a + 0.5 * q, k);
            let r = lp2(&mut d.st[2..4], b + 0.5 * q, k);
            (0.6 * l, 0.6 * r)
        }
    }
}
