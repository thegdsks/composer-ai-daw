//! Verifies `Engine::process` does not allocate. A counting global allocator
//! counts allocations made by the test thread only (thread local counter), and
//! this file holds a single test so nothing else shares the binary. The
//! allocator needs unsafe, which is fine here: the lib itself forbids it.

use composer_engine::{
    Adsr, AutoLane, AutoPoint, BankParams, BuiltinVoice, ChainSpec, Engine, EngineConfig, Event,
    EventKind, FxSpec, Instrument, InstrumentDef, LaneParam, LoopMode, MasterBus, SampleBank,
    SampleData, SamplerParams, SynthParams, TrackSetup, VoiceParams, Waveform, Zone, MAX_BLOCK,
};
use composer_model::{AutomationCurve, EffectKind};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn process_never_allocates() {
    let synth = |w| {
        Instrument::new(
            &InstrumentDef::Synth(SynthParams {
                waveform: w,
                ..SynthParams::default()
            }),
            48_000,
        )
    };
    let sample: Vec<f32> = (0..2000).map(|i| (i % 50) as f32 / 50.0).collect();
    let sampler = Instrument::new(
        &InstrumentDef::Sampler(SamplerParams::new(Arc::new(
            SampleData::new(44_100, 2, 60, sample).unwrap(),
        ))),
        48_000,
    );
    // A bank with a looped stereo pair layer and a one shot velocity layer.
    let pcm = |n: usize| {
        (0..n)
            .map(|i| ((i * 7) % 31) as f32 / 31.0)
            .collect::<Vec<f32>>()
    };
    let zone = |sample: u32, vel_lo: u8, vel_hi: u8, mode: LoopMode, pan: f32| Zone {
        sample,
        key_lo: 0,
        key_hi: 127,
        vel_lo,
        vel_hi,
        root_key: 60,
        tune_cents: 13,
        scale_cents: 100,
        loop_mode: mode,
        loop_start: 100,
        loop_end: 900,
        adsr: Adsr::default(),
        gain: 0.5,
        pan,
    };
    let bank = Arc::new(
        SampleBank::new(
            vec![
                SampleData::new(44_100, 1, 0, pcm(1000)).unwrap(),
                SampleData::new(22_050, 2, 0, pcm(2000)).unwrap(),
            ],
            vec![
                zone(0, 0, 90, LoopMode::Continuous, -0.5),
                zone(0, 0, 90, LoopMode::Sustain, 0.5),
                zone(1, 91, 127, LoopMode::OneShot, 0.0),
            ],
        )
        .unwrap(),
    );
    let banked = Instrument::new(&InstrumentDef::Bank(BankParams { bank, gain: 1.0 }), 48_000);
    let mut tracks = vec![
        TrackSetup {
            instrument: synth(Waveform::Sine),
            gain: 1.0,
            pan: -0.5,
        },
        TrackSetup {
            instrument: synth(Waveform::Saw),
            gain: 1.0,
            pan: 0.5,
        },
        TrackSetup {
            instrument: sampler,
            gain: 1.0,
            pan: 0.0,
        },
        TrackSetup {
            instrument: banked,
            gain: 1.0,
            pan: 0.0,
        },
    ];
    // The eight built in voices (D29) share the same hostile schedule.
    for v in BuiltinVoice::ALL {
        tracks.push(TrackSetup {
            instrument: Instrument::new(&InstrumentDef::Voice(VoiceParams::new(v)), 48_000),
            gain: 1.0,
            pan: 0.0,
        });
    }
    let track_count = tracks.len() as u64;
    let mut events = Vec::new();
    for i in 0..400u64 {
        let track = (i % track_count) as u32;
        let pitch = 40 + (i % 40) as u8;
        events.push(Event {
            frame: i * 300,
            track,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 100,
            },
        });
        events.push(Event {
            frame: i * 300 + 700,
            track,
            kind: EventKind::NoteOff { pitch },
        });
    }
    // Two voices per track forces stealing as well.
    let mut engine = Engine::new(
        EngineConfig {
            voices_per_track: 2,
        },
        tracks,
        events,
    );
    // Reverb on, with sends, so the whole bus is inside the measured loop (D26).
    engine.set_master(MasterBus::new(0.8, 0.7, 0.4));
    for t in 0..4 {
        assert!(engine.set_send(t, 0.3 + 0.1 * t as f32));
    }
    // Automation on the master and on every strip parameter, all curve kinds, so the lane
    // evaluation is inside the measured loop too (D36). Installing lanes allocates and is setup.
    let curves = [
        AutomationCurve::Linear,
        AutomationCurve::Smooth,
        AutomationCurve::Hold,
    ];
    let lane = |track: Option<u32>, param, lo: i16, hi: i16, base: f32| AutoLane {
        track,
        param,
        base,
        points: (0..50)
            .map(|i| AutoPoint {
                frame: i * 5_000,
                value: if i % 2 == 0 { lo } else { hi },
                curve: curves[i as usize % 3],
            })
            .collect(),
    };
    assert!(engine.set_lane(lane(None, LaneParam::MasterVolume, 200, 1500, 1.0)));
    for t in 0..4u32 {
        assert!(engine.set_lane(lane(Some(t), LaneParam::Volume, 0, 1800, 1.0)));
        assert!(engine.set_lane(lane(Some(t), LaneParam::Pan, -1000, 1000, 0.0)));
        assert!(engine.set_lane(lane(Some(t), LaneParam::ReverbSend, 0, 1000, 0.0)));
    }
    // Every effect kind runs inside the measured loop (D45): all six on the master and on
    // track 0, rotated sets on the other strips. Building the chains allocates and is setup.
    let all: Vec<FxSpec> = EffectKind::ALL.iter().map(|k| FxSpec::new(*k)).collect();
    let rotated = |n: usize| -> Vec<FxSpec> { (0..3).map(|i| all[(n + i) % 6]).collect() };
    assert!(engine.set_effects(&[
        ChainSpec {
            track: None,
            effects: all.clone()
        },
        ChainSpec {
            track: Some(0),
            effects: all.clone()
        },
        ChainSpec {
            track: Some(1),
            effects: rotated(1)
        },
        ChainSpec {
            track: Some(2),
            effects: rotated(3)
        },
        ChainSpec {
            track: Some(3),
            effects: rotated(5)
        },
    ]));
    let (mut l, mut r) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);

    // Sanity: the counter really sees allocations.
    let probe = COUNT.with(Cell::get);
    let v = std::hint::black_box(vec![1u8; 64]);
    assert!(COUNT.with(Cell::get) > probe);
    drop(v);

    let before = COUNT.with(Cell::get);
    for i in 0..2000u32 {
        // Live pushes are part of the audio path and must not allocate either.
        engine.push_live(
            i % track_count as u32,
            EventKind::NoteOn {
                pitch: 50 + (i % 7) as u8,
                velocity: 60 + (i % 60) as u8,
            },
        );
        engine.push_live(
            i % track_count as u32,
            EventKind::NoteOff {
                pitch: 50 + (i % 7) as u8,
            },
        );
        // Live parameter and bypass edits are allocation free as well.
        let k = (i % 6) as usize;
        assert!(engine.set_effect_param(Some(0), k, 0, EffectKind::ALL[k].params()[0].default));
        assert!(engine.set_effect_param(None, k, 0, EffectKind::ALL[k].params()[0].default));
        assert!(engine.set_effect_bypass(Some(0), k, i % 7 == 0));
        engine.process(&mut l, &mut r);
        // Reading the peaks is part of the audio path too.
        std::hint::black_box(engine.track_peaks().iter().fold(0.0f32, |a, p| a.max(*p)));
    }
    let after = COUNT.with(Cell::get);
    assert_eq!(after - before, 0, "process allocated");
    assert!(l.iter().chain(r.iter()).any(|s| *s != 0.0) || engine.position() > 0);
}
