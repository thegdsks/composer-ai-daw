//! Deterministic offline render: a project in, interleaved stereo f32 out.
#![forbid(unsafe_code)]

mod clips;
mod meter;
mod wav;

pub use clips::AudioIssue;
pub use composer_audio::{AudioLimits, AudioProvider};
pub use meter::{measure, mix_from_project, Level, MeterReport};

pub use wav::{read_wav, write_wav, Wav, WavError, WavFormat};

use composer_engine::{
    compile_automation, compile_effects, compile_events, BuiltinVoice, Engine, EngineConfig,
    Instrument, InstrumentDef, MasterBus, SynthParams, TrackSetup, VoiceParams, Waveform,
    BUILTIN_SAW, BUILTIN_SINE, MAX_BLOCK,
};
use composer_model::{Id, Project};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Per track mixer settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackMix {
    pub gain: f32,
    /// -1.0 left to 1.0 right.
    pub pan: f32,
}

impl Default for TrackMix {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pan: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderSettings {
    pub sample_rate: u32,
    /// Silence appended after the last note off so releases can ring out.
    pub tail_seconds: f64,
    /// Registered instruments by id. Built in ids [`BUILTIN_SINE`] and [`BUILTIN_SAW`] need no entry.
    pub instruments: BTreeMap<Id, InstrumentDef>,
    /// Mixer settings by track id. Missing tracks use unity gain, centered.
    pub mix: BTreeMap<Id, TrackMix>,
    pub voices_per_track: usize,
    /// Master volume and reverb (D26). The default leaves the sum untouched.
    pub master: MasterBus,
    /// Reverb send by track id, 0.0..=1.0. Missing tracks send nothing.
    pub sends: BTreeMap<Id, f32>,
    /// Where audio clips find their bytes (D42). Without it every clip stays silent and is reported.
    pub audio: Option<Arc<dyn AudioProvider>>,
    /// Caps for clip streaming, including the resident decode buffer budget.
    pub audio_limits: AudioLimits,
}

impl RenderSettings {
    pub fn new(sample_rate: u32, tail_seconds: f64) -> Self {
        Self {
            sample_rate,
            tail_seconds,
            instruments: BTreeMap::new(),
            mix: BTreeMap::new(),
            voices_per_track: 16,
            master: MasterBus::default(),
            sends: BTreeMap::new(),
            audio: None,
            audio_limits: AudioLimits::default(),
        }
    }
}

/// A track whose instrument id was not known, so the default synth played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fallback {
    pub track: Id,
    pub instrument: Id,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// Interleaved stereo, `frames * 2` values.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub frames: u64,
    /// Frames whose master sum went past full scale (the samples are not clamped).
    pub clipped_frames: u64,
    pub fallbacks: Vec<Fallback>,
    /// Audio clips that stayed silent (D42).
    pub audio_issues: Vec<AudioIssue>,
    /// Most bytes of audio decode buffers held at once.
    pub audio_resident_high_water: u64,
}

/// What a streamed render reports once it is done (see [`render_project_stream`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSummary {
    pub sample_rate: u32,
    pub frames: u64,
    pub clipped_frames: u64,
    pub fallbacks: Vec<Fallback>,
    pub audio_issues: Vec<AudioIssue>,
    pub audio_resident_high_water: u64,
}

fn default_def() -> InstrumentDef {
    InstrumentDef::Synth(SynthParams::default())
}

/// Renders the whole project in 128 frame blocks.
///
/// Length is the frame of the last note off plus `tail_seconds` rounded to the
/// nearest frame (0 for negative or non finite), so a project without notes is
/// pure silence of exactly the tail length. A track without an instrument uses
/// the default sine synth silently; an id that is neither registered nor built
/// in also uses it and is listed in `fallbacks`.
pub fn render_project(project: &Project, settings: &RenderSettings) -> Rendered {
    let mut samples = Vec::new();
    let summary = render_project_stream(project, settings, &mut |block| {
        samples.extend_from_slice(block);
    });
    Rendered {
        samples,
        sample_rate: summary.sample_rate,
        frames: summary.frames,
        clipped_frames: summary.clipped_frames,
        fallbacks: summary.fallbacks,
        audio_issues: summary.audio_issues,
        audio_resident_high_water: summary.audio_resident_high_water,
    }
}

/// Frame of the end of the music or the last audio clip, whichever is later, without the tail.
/// What the render length is built from, so callers can check a length before rendering.
pub fn content_end_frame(project: &Project, sample_rate: u32) -> u64 {
    let notes = compile_events(project, sample_rate).end_frame;
    let clock = composer_engine::TempoClock::new(&project.tempo_map, sample_rate);
    let clips = project
        .tracks
        .iter()
        .flat_map(|t| t.regions.values())
        .filter(|r| matches!(r.content, composer_model::RegionContent::Clip { .. }))
        .map(|r| clock.frame_at(r.start + r.length))
        .max()
        .unwrap_or(0);
    notes.max(clips)
}

/// Same render as [`render_project`], handing the output to `sink` as it is produced:
/// interleaved stereo f32 in blocks of at most 128 frames. Memory stays flat however long
/// the project is, which is how a two hour audio file renders in a fixed budget (D42).
pub fn render_project_stream(
    project: &Project,
    settings: &RenderSettings,
    sink: &mut dyn FnMut(&[f32]),
) -> RenderSummary {
    let sr = settings.sample_rate;
    let schedule = compile_events(project, sr);
    let mut fallbacks = Vec::new();
    let tracks = project
        .tracks
        .iter()
        .map(|t| {
            // A built in sound id wins over the instrument slot (D29).
            let voice = t.sound.as_deref().and_then(BuiltinVoice::from_sound_id);
            let def = match (voice, t.instrument) {
                (Some(v), _) => InstrumentDef::Voice(VoiceParams::new(v)),
                (None, None) => default_def(),
                (None, Some(id)) => match settings.instruments.get(&id) {
                    Some(d) => d.clone(),
                    None if id == BUILTIN_SINE => default_def(),
                    None if id == BUILTIN_SAW => InstrumentDef::Synth(SynthParams {
                        waveform: Waveform::Saw,
                        ..SynthParams::default()
                    }),
                    None => {
                        fallbacks.push(Fallback {
                            track: t.id,
                            instrument: id,
                        });
                        default_def()
                    }
                },
            };
            let mix = settings.mix.get(&t.id).copied().unwrap_or_default();
            TrackSetup {
                instrument: Instrument::new(&def, sr),
                gain: mix.gain,
                pan: mix.pan,
            }
        })
        .collect();
    let tail = if settings.tail_seconds.is_finite() && settings.tail_seconds > 0.0 {
        (settings.tail_seconds * f64::from(sr)).round() as u64
    } else {
        0
    };
    let mut clips = clips::ClipMixer::new(project, settings, sr);
    let clips_end = clips.as_ref().map_or(0, |c| c.end_frame);
    let frames = schedule.end_frame.max(clips_end) + tail;
    let mut engine = Engine::with_rate(
        EngineConfig {
            voices_per_track: settings.voices_per_track,
        },
        tracks,
        schedule.events,
        sr,
    );
    engine.set_master(settings.master);
    for (i, t) in project.tracks.iter().enumerate() {
        if let Some(send) = settings.sends.get(&t.id) {
            engine.set_send(i, *send);
        }
    }
    engine.set_effects(&compile_effects(project));
    for lane in compile_automation(project, sr) {
        engine.set_lane(lane);
    }
    let (mut l, mut r) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);
    let (mut il, mut ir) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);
    let mut out = [0.0f32; MAX_BLOCK * 2];
    let mut at = 0u64;
    while at < frames {
        let n = (frames - at).min(MAX_BLOCK as u64) as usize;
        match clips.as_mut() {
            Some(c) => {
                c.mix_block(at, &mut il[..n], &mut ir[..n]);
                engine.process_with(&mut l[..n], &mut r[..n], &il[..n], &ir[..n]);
            }
            None => engine.process(&mut l[..n], &mut r[..n]),
        }
        for i in 0..n {
            out[i * 2] = l[i];
            out[i * 2 + 1] = r[i];
        }
        sink(&out[..n * 2]);
        at += n as u64;
    }
    RenderSummary {
        sample_rate: sr,
        frames,
        clipped_frames: engine.clipped_frames(),
        fallbacks,
        audio_issues: clips.as_ref().map_or_else(Vec::new, |c| c.issues.clone()),
        audio_resident_high_water: clips.as_ref().map_or(0, |c| c.high_water),
    }
}
