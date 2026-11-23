use crate::bank::{BankParams, LoopMode, SampleBank, MAX_LAYERS};
use crate::math::{cents_ratio, pitch_hz, semitone_ratio, sin_turns};
use crate::voices::{self, BuiltinVoice, Dsp};
use std::sync::Arc;

/// Attack, decay and release in seconds, sustain as a 0..=1 level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adsr {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self {
            attack: 0.005,
            decay: 0.1,
            sustain: 0.7,
            release: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    /// Naive (aliasing) saw, fine for a first synth.
    Saw,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthParams {
    pub waveform: Waveform,
    pub adsr: Adsr,
    /// Linear gain applied per voice before the track gain.
    pub gain: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: Waveform::Sine,
            adsr: Adsr::default(),
            gain: 0.25,
        }
    }
}

/// Why a sample was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleError {
    Channels,
    Length,
    Rate,
    Empty,
}

/// Mono or stereo f32 audio held in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    rate: u32,
    channels: u8,
    root_note: u8,
    frames: Vec<f32>,
}

impl SampleData {
    /// `frames` is interleaved. `root_note` is the pitch played at the original speed.
    pub fn new(
        rate: u32,
        channels: u8,
        root_note: u8,
        frames: Vec<f32>,
    ) -> Result<Self, SampleError> {
        if channels != 1 && channels != 2 {
            return Err(SampleError::Channels);
        }
        if rate == 0 {
            return Err(SampleError::Rate);
        }
        if frames.is_empty() {
            return Err(SampleError::Empty);
        }
        if !frames.len().is_multiple_of(usize::from(channels)) {
            return Err(SampleError::Length);
        }
        Ok(Self {
            rate,
            channels,
            root_note,
            frames,
        })
    }

    pub fn len_frames(&self) -> usize {
        self.frames.len() / usize::from(self.channels)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Interleaved frames.
    pub fn data(&self) -> &[f32] {
        &self.frames
    }

    fn at(&self, frame: usize, channel: usize) -> f32 {
        self.frames[frame * usize::from(self.channels) + channel]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SamplerParams {
    pub sample: Arc<SampleData>,
    pub adsr: Adsr,
    pub gain: f32,
}

impl SamplerParams {
    /// Plays the whole sample, then releases quickly.
    pub fn new(sample: Arc<SampleData>) -> Self {
        Self {
            sample,
            adsr: Adsr {
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                release: 0.02,
            },
            gain: 0.5,
        }
    }
}

/// A built in voice (D29) with its linear gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceParams {
    pub voice: BuiltinVoice,
    pub gain: f32,
}

impl VoiceParams {
    pub fn new(voice: BuiltinVoice) -> Self {
        Self {
            voice,
            gain: voice.default_gain(),
        }
    }
}

/// A user facing instrument description.
#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentDef {
    Synth(SynthParams),
    Sampler(SamplerParams),
    /// Multi sample bank (SF2 or SFZ), see D34.
    Bank(BankParams),
    /// One of the eight built in voices.
    Voice(VoiceParams),
}

/// Per zone values resolved for one sample rate.
#[derive(Debug, Clone, Copy)]
struct ZoneRt {
    attack: u32,
    decay: u32,
    sustain: f32,
    release: u32,
    gain: f32,
    pan_l: f32,
    pan_r: f32,
    rate_base: f64,
    loop_start: f64,
    loop_end: f64,
}

#[derive(Debug, Clone)]
enum Kind {
    Synth(Waveform),
    Sampler(Arc<SampleData>),
    Bank(Arc<SampleBank>, Vec<ZoneRt>),
    Voice(BuiltinVoice),
}

/// An instrument resolved for one sample rate (envelope times in frames).
#[derive(Debug, Clone)]
pub struct Instrument {
    kind: Kind,
    gain: f32,
    sample_rate: u32,
    attack: u32,
    decay: u32,
    sustain: f32,
    release: u32,
}

fn frames_of(seconds: f32, rate: u32) -> u32 {
    let f = (f64::from(seconds.max(0.0)) * f64::from(rate)).round();
    f.min(f64::from(u32::MAX)) as u32
}

impl Instrument {
    pub fn new(def: &InstrumentDef, sample_rate: u32) -> Self {
        let (kind, adsr, gain) = match def {
            InstrumentDef::Synth(p) => (Kind::Synth(p.waveform), p.adsr, p.gain),
            InstrumentDef::Sampler(p) => (Kind::Sampler(Arc::clone(&p.sample)), p.adsr, p.gain),
            InstrumentDef::Voice(p) => (Kind::Voice(p.voice), p.voice.adsr(), p.gain),
            InstrumentDef::Bank(p) => {
                let rt = p
                    .bank
                    .zones()
                    .iter()
                    .map(|z| {
                        // Constant power law scaled so that center is unity on both sides.
                        let angle = (z.pan + 1.0) * 0.125;
                        let s = &p.bank.samples()[z.sample as usize];
                        ZoneRt {
                            attack: frames_of(z.adsr.attack, sample_rate),
                            decay: frames_of(z.adsr.decay, sample_rate),
                            sustain: z.adsr.sustain.clamp(0.0, 1.0),
                            release: frames_of(z.adsr.release, sample_rate).max(1),
                            gain: z.gain,
                            pan_l: sin_turns(0.25 - angle) * core::f32::consts::SQRT_2,
                            pan_r: sin_turns(angle) * core::f32::consts::SQRT_2,
                            rate_base: f64::from(s.rate()) / f64::from(sample_rate),
                            loop_start: f64::from(z.loop_start),
                            loop_end: f64::from(z.loop_end),
                        }
                    })
                    .collect();
                return Self {
                    kind: Kind::Bank(Arc::clone(&p.bank), rt),
                    gain: p.gain,
                    sample_rate,
                    attack: 0,
                    decay: 0,
                    sustain: 1.0,
                    release: 1,
                };
            }
        };
        Self {
            kind,
            gain,
            sample_rate,
            attack: frames_of(adsr.attack, sample_rate),
            decay: frames_of(adsr.decay, sample_rate),
            sustain: adsr.sustain.clamp(0.0, 1.0),
            release: frames_of(adsr.release, sample_rate).max(1),
        }
    }

    /// Voices one note on needs: 1 for synths, the matching zone count (capped) for a bank,
    /// so a note no zone covers steals nothing.
    pub(crate) fn layers(&self, pitch: u8, velocity: u8) -> usize {
        match &self.kind {
            Kind::Bank(bank, _) => (0..MAX_LAYERS)
                .take_while(|&n| bank.find_zone(pitch, velocity, n).is_some())
                .count(),
            _ => 1,
        }
    }

    pub(crate) fn start(&self, v: &mut Voice, pitch: u8, velocity: u8, age: u64, layer: usize) {
        let mut attack = self.attack;
        *v = Voice {
            active: true,
            held: true,
            pitch,
            amp: f32::from(velocity.min(127)) / 127.0 * self.gain,
            age,
            stage: Stage::Attack,
            level: 0.0,
            step: 0.0,
            phase: 0.0,
            inc: 0.0,
            pos: 0.0,
            rate: 0.0,
            zone: 0,
            sample: 0,
            loop_mode: LoopMode::None,
            looping: false,
            loop_start: 0.0,
            loop_end: 0.0,
            decay: self.decay,
            sustain: self.sustain,
            release: self.release,
            pan_l: 1.0,
            pan_r: 1.0,
            dsp: Dsp::ZERO,
        };
        match &self.kind {
            Kind::Synth(_) => {
                v.inc = (pitch_hz(pitch) / f64::from(self.sample_rate)) as f32;
            }
            Kind::Voice(vc) => {
                voices::start(*vc, &mut v.dsp, pitch, velocity, self.sample_rate);
            }
            Kind::Sampler(s) => {
                let ratio = semitone_ratio(i32::from(pitch) - i32::from(s.root_note));
                v.rate = f64::from(s.rate) / f64::from(self.sample_rate) * ratio;
            }
            Kind::Bank(bank, rt) => {
                let Some(zi) = bank.find_zone(pitch, velocity, layer) else {
                    v.active = false;
                    return;
                };
                let (z, r) = (&bank.zones()[zi], &rt[zi]);
                let cents = (i32::from(pitch) - i32::from(z.root_key)) * i32::from(z.scale_cents)
                    + z.tune_cents;
                v.rate = r.rate_base * cents_ratio(cents);
                v.zone = zi as u32;
                v.sample = z.sample;
                v.loop_mode = z.loop_mode;
                v.looping = matches!(z.loop_mode, LoopMode::Continuous | LoopMode::Sustain);
                v.loop_start = r.loop_start;
                v.loop_end = r.loop_end;
                v.amp = f32::from(velocity.min(127)) / 127.0 * self.gain * r.gain;
                v.decay = r.decay;
                v.sustain = r.sustain;
                v.release = r.release;
                v.pan_l = r.pan_l;
                v.pan_r = r.pan_r;
                attack = r.attack;
            }
        }
        if attack == 0 {
            self.enter_decay(v);
        } else {
            v.step = 1.0 / attack as f32;
        }
    }

    fn enter_decay(&self, v: &mut Voice) {
        if v.decay == 0 {
            v.level = v.sustain;
            v.stage = Stage::Sustain;
        } else if v.sustain >= 1.0 {
            v.level = 1.0;
            v.stage = Stage::Sustain;
        } else {
            v.level = 1.0;
            v.stage = Stage::Decay;
            v.step = (1.0 - v.sustain) / v.decay as f32;
        }
    }

    pub(crate) fn release(&self, v: &mut Voice) {
        if !v.active || !v.held || v.loop_mode == LoopMode::OneShot {
            return;
        }
        v.held = false;
        if v.loop_mode == LoopMode::Sustain {
            v.looping = false;
        }
        if v.level <= 0.0 {
            v.active = false;
            return;
        }
        v.stage = Stage::Release;
        v.step = v.level / v.release as f32;
    }

    /// Next stereo frame of the voice before track gain and pan. Frees the voice at its end.
    pub(crate) fn render(&self, v: &mut Voice) -> (f32, f32) {
        let env = v.level;
        match v.stage {
            Stage::Attack => {
                v.level += v.step;
                if v.level >= 1.0 {
                    self.enter_decay(v);
                }
            }
            Stage::Decay => {
                v.level -= v.step;
                if v.level <= v.sustain {
                    v.level = v.sustain;
                    v.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                v.level -= v.step;
                if v.level <= 0.0 {
                    v.level = 0.0;
                    v.active = false;
                }
            }
        }
        let gain = env * v.amp;
        match &self.kind {
            Kind::Synth(w) => {
                let s = match w {
                    Waveform::Sine => sin_turns(v.phase),
                    Waveform::Saw => 2.0 * v.phase - 1.0,
                };
                v.phase += v.inc;
                if v.phase >= 1.0 {
                    v.phase -= 1.0;
                }
                (s * gain, s * gain)
            }
            Kind::Voice(vc) => {
                let (l, r) = voices::render(*vc, &mut v.dsp);
                (l * gain, r * gain)
            }
            Kind::Sampler(s) => {
                let i = v.pos as usize;
                let n = s.len_frames();
                if i >= n {
                    v.active = false;
                    return (0.0, 0.0);
                }
                let frac = (v.pos - i as f64) as f32;
                let j = (i + 1).min(n - 1);
                let l = s.at(i, 0) + (s.at(j, 0) - s.at(i, 0)) * frac;
                let r = if s.channels == 2 {
                    s.at(i, 1) + (s.at(j, 1) - s.at(i, 1)) * frac
                } else {
                    l
                };
                v.pos += v.rate;
                (l * gain, r * gain)
            }
            Kind::Bank(bank, _) => {
                let s = &bank.samples()[v.sample as usize];
                if v.looping && v.pos >= v.loop_end {
                    v.pos = v.loop_start + (v.pos - v.loop_start) % (v.loop_end - v.loop_start);
                }
                let i = v.pos as usize;
                let n = s.len_frames();
                if i >= n {
                    v.active = false;
                    return (0.0, 0.0);
                }
                let frac = (v.pos - i as f64) as f32;
                let mut j = i + 1;
                if v.looping && (j as f64) >= v.loop_end {
                    j = v.loop_start as usize;
                }
                let j = j.min(n - 1);
                let l = s.at(i, 0) + (s.at(j, 0) - s.at(i, 0)) * frac;
                let r = if s.channels == 2 {
                    s.at(i, 1) + (s.at(j, 1) - s.at(i, 1)) * frac
                } else {
                    l
                };
                v.pos += v.rate;
                (l * gain * v.pan_l, r * gain * v.pan_r)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Voice {
    pub active: bool,
    /// False once the note off arrived (voice is in its release).
    pub held: bool,
    pub pitch: u8,
    pub amp: f32,
    pub age: u64,
    stage: Stage,
    level: f32,
    step: f32,
    phase: f32,
    inc: f32,
    pos: f64,
    rate: f64,
    zone: u32,
    sample: u32,
    loop_mode: LoopMode,
    looping: bool,
    loop_start: f64,
    loop_end: f64,
    decay: u32,
    sustain: f32,
    release: u32,
    pan_l: f32,
    pan_r: f32,
    dsp: Dsp,
}

impl Voice {
    pub(crate) const IDLE: Voice = Voice {
        active: false,
        held: false,
        pitch: 0,
        amp: 0.0,
        age: 0,
        stage: Stage::Release,
        level: 0.0,
        step: 0.0,
        phase: 0.0,
        inc: 0.0,
        pos: 0.0,
        rate: 0.0,
        zone: 0,
        sample: 0,
        loop_mode: LoopMode::None,
        looping: false,
        loop_start: 0.0,
        loop_end: 0.0,
        decay: 0,
        sustain: 1.0,
        release: 1,
        pan_l: 1.0,
        pan_r: 1.0,
        dsp: Dsp::ZERO,
    };
}
