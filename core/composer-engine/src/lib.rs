//! Audio engine. Unsafe code stays forbidden until a decision allows it.
//!
//! Built to compile unchanged to wasm32 for an AudioWorklet: no files, threads,
//! clocks or global state, and `Engine::process` never allocates (everything is
//! sized in `Engine::new`). Block size is at most [`MAX_BLOCK`] frames.
#![forbid(unsafe_code)]

mod automation;
mod bank;
mod engine;
mod fx;
mod instrument;
mod math;
mod mix;
mod schedule;
mod tempo;
mod voices;

pub use automation::{compile_automation, AutoLane, AutoPoint, LaneParam, CONTROL_PERIOD};
pub use bank::{BankError, BankParams, LoopMode, SampleBank, Zone, MAX_LAYERS, MAX_TUNE_CENTS};
pub use engine::{
    pan_gains, Engine, EngineConfig, Event, EventKind, TrackSetup, DEFAULT_CEILING, LIVE_CAPACITY,
    MAX_BLOCK,
};
pub use fx::{compile_effects, ChainSpec, FxSpec};
pub use instrument::{
    Adsr, Instrument, InstrumentDef, SampleData, SampleError, SamplerParams, SynthParams,
    VoiceParams, Waveform,
};
pub use mix::{
    any_solo, clamp_send, is_audible, master_from_project, resolve_mix, send_from_project,
    MasterBus, Reverb, MAX_MASTER_VOLUME,
};
pub use schedule::{compile_events, LoopSpan, Schedule, MAX_LOOP_PASSES};
pub use tempo::TempoClock;
pub use voices::BuiltinVoice;

use composer_model::Id;

/// Built-in sine synth instrument id.
pub const BUILTIN_SINE: Id = Id(1);
/// Built-in saw synth instrument id.
pub const BUILTIN_SAW: Id = Id(2);

/// Sample rate in hertz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRate(pub u32);
