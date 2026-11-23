//! In-memory multi sample instrument (D34). Plain data, validated once in [`SampleBank::new`].

use crate::instrument::{Adsr, SampleData};
use std::sync::Arc;

/// How a zone loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// Plays to the end of the sample, then stops.
    None,
    /// Loops until the voice has fully released.
    Continuous,
    /// Loops while the key is down, then plays on past the loop end.
    Sustain,
    /// Plays the whole sample and ignores note off.
    OneShot,
}

/// One playable region of a bank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// Index into the bank's samples.
    pub sample: u32,
    pub key_lo: u8,
    pub key_hi: u8,
    pub vel_lo: u8,
    pub vel_hi: u8,
    /// Key that plays the sample at its own pitch.
    pub root_key: u8,
    /// Extra tuning in cents.
    pub tune_cents: i32,
    /// Cents per key step away from the root (100 is equal temperament).
    pub scale_cents: i16,
    pub loop_mode: LoopMode,
    /// Loop start frame and the first frame after the loop, relative to the sample.
    pub loop_start: u32,
    pub loop_end: u32,
    pub adsr: Adsr,
    /// Linear gain of the zone.
    pub gain: f32,
    /// -1.0 left to 1.0 right, relative: 0.0 leaves both channels at unity.
    pub pan: f32,
}

/// Why a bank was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankError {
    Empty,
    SampleIndex,
    Range,
    Loop,
    Value,
}

/// Samples plus the zones that play them.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleBank {
    samples: Vec<SampleData>,
    zones: Vec<Zone>,
}

/// Most zones that sound together for one note (a stereo pair needs two).
pub const MAX_LAYERS: usize = 4;

/// Bound on tuning so the pitch ratio stays finite.
pub const MAX_TUNE_CENTS: i32 = 48_000;

impl SampleBank {
    pub fn new(samples: Vec<SampleData>, zones: Vec<Zone>) -> Result<Self, BankError> {
        if samples.is_empty() || zones.is_empty() {
            return Err(BankError::Empty);
        }
        for z in &zones {
            let Some(s) = samples.get(z.sample as usize) else {
                return Err(BankError::SampleIndex);
            };
            if z.key_lo > z.key_hi
                || z.key_hi > 127
                || z.vel_lo > z.vel_hi
                || z.vel_hi > 127
                || z.root_key > 127
            {
                return Err(BankError::Range);
            }
            let a = z.adsr;
            let finite = [a.attack, a.decay, a.sustain, a.release, z.gain, z.pan]
                .iter()
                .all(|x| x.is_finite());
            if !finite
                || a.attack < 0.0
                || a.decay < 0.0
                || a.release < 0.0
                || z.gain < 0.0
                || !(-1.0..=1.0).contains(&z.pan)
                || z.tune_cents.abs() > MAX_TUNE_CENTS
                || !(-1200..=1200).contains(&z.scale_cents)
            {
                return Err(BankError::Value);
            }
            if matches!(z.loop_mode, LoopMode::Continuous | LoopMode::Sustain) {
                let n = s.len_frames() as u64;
                if z.loop_start >= z.loop_end || u64::from(z.loop_end) > n {
                    return Err(BankError::Loop);
                }
            }
        }
        Ok(Self { samples, zones })
    }

    pub fn samples(&self) -> &[SampleData] {
        &self.samples
    }

    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// The `nth` zone in order that covers the key and velocity (stereo pairs stack).
    pub fn find_zone(&self, pitch: u8, velocity: u8, nth: usize) -> Option<usize> {
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| {
                (z.key_lo..=z.key_hi).contains(&pitch) && (z.vel_lo..=z.vel_hi).contains(&velocity)
            })
            .nth(nth)
            .map(|(i, _)| i)
    }
}

/// Bank plus the linear gain of the whole instrument.
#[derive(Debug, Clone, PartialEq)]
pub struct BankParams {
    pub bank: Arc<SampleBank>,
    pub gain: f32,
}
