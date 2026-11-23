//! Safe engine wrapper that owns the output buffers handed to JS.

use crate::blob::{decode, BlobError, Decoded};
use composer_engine::{
    resolve_mix, BankParams, BuiltinVoice, Engine, EngineConfig, EventKind, Instrument,
    InstrumentDef, LaneParam, SynthParams, TrackSetup, VoiceParams, Waveform, MAX_BLOCK,
};
use std::sync::Arc;

pub struct Session {
    engine: Engine,
    pub left: [f32; MAX_BLOCK],
    pub right: [f32; MAX_BLOCK],
    pub length_frames: u64,
}

impl Session {
    pub fn from_blob(bytes: &[u8]) -> Result<Self, BlobError> {
        Ok(Self::new(decode(bytes)?))
    }

    pub fn new(d: Decoded) -> Self {
        let solo = d.tracks.iter().any(|t| t.mix.solo);
        let tracks = d
            .tracks
            .iter()
            .map(|t| {
                let waveform = if t.waveform == 1 {
                    Waveform::Saw
                } else {
                    Waveform::Sine
                };
                // Bank instruments run at unit gain, the zone gain carries the level (D34).
                let def = match t.bank.and_then(|i| d.banks.get(usize::from(i))) {
                    Some(bank) => InstrumentDef::Bank(BankParams {
                        bank: Arc::clone(bank),
                        gain: 1.0,
                    }),
                    None => match BuiltinVoice::from_wave_code(t.waveform) {
                        Some(v) => InstrumentDef::Voice(VoiceParams::new(v)),
                        None => InstrumentDef::Synth(SynthParams {
                            waveform,
                            ..SynthParams::default()
                        }),
                    },
                };
                // Same resolution as the offline render; v1 blobs carry the default mix.
                let (mix_gain, mix_pan) = resolve_mix(&t.mix, solo);
                TrackSetup {
                    instrument: Instrument::new(&def, d.sample_rate),
                    gain: t.gain * mix_gain,
                    pan: (t.pan + mix_pan).clamp(-1.0, 1.0),
                }
            })
            .collect();
        let config = EngineConfig {
            voices_per_track: d.voices_per_track as usize,
        };
        let mut engine = Engine::with_rate(config, tracks, d.events, d.sample_rate);
        engine.set_master(d.master);
        for (i, t) in d.tracks.iter().enumerate() {
            engine.set_send(i, t.send);
        }
        engine.set_effects(&d.effects);
        // Lanes go in last so they win over the static master volume and sends (D36).
        for mut lane in d.lanes {
            if let Some(t) = lane.track.and_then(|i| d.tracks.get(i as usize)) {
                match lane.param {
                    LaneParam::Volume => lane.base *= t.gain,
                    LaneParam::Pan => lane.base += t.pan,
                    _ => {}
                }
            }
            engine.set_lane(lane);
        }
        Self {
            engine,
            left: [0.0; MAX_BLOCK],
            right: [0.0; MAX_BLOCK],
            length_frames: d.length_frames,
        }
    }

    /// Renders up to 128 frames into `left` and `right`; returns the count rendered.
    pub fn process(&mut self, frames: usize) -> usize {
        let n = frames.min(MAX_BLOCK);
        self.engine
            .process(&mut self.left[..n], &mut self.right[..n]);
        n
    }

    pub fn position(&self) -> u64 {
        self.engine.position()
    }

    /// Per track peaks of the last block, in blob track order.
    pub fn track_peaks(&self) -> &[f32] {
        self.engine.track_peaks()
    }

    /// Enables the master soft clip at a linear ceiling, or disables it for 0 (D44).
    pub fn set_ceiling(&mut self, ceiling: f32) {
        self.engine.set_ceiling((ceiling > 0.0).then_some(ceiling));
    }

    pub fn clipped_frames(&self) -> u64 {
        self.engine.clipped_frames()
    }

    pub fn note(&mut self, track: u32, pitch: u8, velocity: u8, on: bool) -> bool {
        let kind = if on {
            EventKind::NoteOn { pitch, velocity }
        } else {
            EventKind::NoteOff { pitch }
        };
        self.engine.push_live(track, kind)
    }
}
