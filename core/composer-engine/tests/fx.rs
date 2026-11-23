//! Effect chains inside the engine (D45): bypass equals no chain, devices change the sound,
//! the block size never changes the result, and the master chain runs before the volume.

use composer_engine::{
    ChainSpec, Engine, EngineConfig, Event, EventKind, FxSpec, Instrument, InstrumentDef,
    MasterBus, SynthParams, TrackSetup, MAX_BLOCK,
};
use composer_model::EffectKind;

fn engine() -> Engine {
    let tracks = (0..2)
        .map(|i| TrackSetup {
            instrument: Instrument::new(&InstrumentDef::Synth(SynthParams::default()), 48_000),
            gain: 0.4,
            pan: i as f32 - 0.5,
        })
        .collect();
    let mut events = Vec::new();
    for i in 0..40u64 {
        let track = (i % 2) as u32;
        let pitch = 50 + (i % 9) as u8;
        events.push(Event {
            frame: i * 1500,
            track,
            kind: EventKind::NoteOn {
                pitch,
                velocity: 90,
            },
        });
        events.push(Event {
            frame: i * 1500 + 2200,
            track,
            kind: EventKind::NoteOff { pitch },
        });
    }
    Engine::new(
        EngineConfig {
            voices_per_track: 4,
        },
        tracks,
        events,
    )
}

fn render(e: &mut Engine, frames: usize, block: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let (mut l, mut r) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);
    let mut left = frames;
    while left > 0 {
        let n = left.min(block);
        e.process(&mut l[..n], &mut r[..n]);
        for i in 0..n {
            out.push(l[i]);
            out.push(r[i]);
        }
        left -= n;
    }
    out
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn all_chain() -> Vec<FxSpec> {
    EffectKind::ALL.iter().map(|k| FxSpec::new(*k)).collect()
}

#[test]
fn bypassed_chains_are_bit_identical_to_no_chains() {
    let plain = render(&mut engine(), 60_000, 128);
    let mut e = engine();
    let bypassed: Vec<FxSpec> = all_chain()
        .into_iter()
        .map(|mut s| {
            s.bypass = true;
            s
        })
        .collect();
    e.set_effects(&[
        ChainSpec {
            track: None,
            effects: bypassed.clone(),
        },
        ChainSpec {
            track: Some(0),
            effects: bypassed.clone(),
        },
        ChainSpec {
            track: Some(1),
            effects: bypassed,
        },
    ]);
    assert_eq!(bits(&render(&mut e, 60_000, 128)), bits(&plain));
}

#[test]
fn every_device_changes_the_sound_and_repeats_exactly() {
    let plain = render(&mut engine(), 30_000, 128);
    for kind in EffectKind::ALL {
        let run = || {
            let mut e = engine();
            let mut s = FxSpec::new(kind);
            // Make the neutral looking defaults audible.
            match kind {
                EffectKind::StereoTool => assert!(s.set(0, 500)),
                EffectKind::Gate => assert!(s.set(0, 900)),
                EffectKind::Crusher => assert!(s.set(0, 3)),
                _ => {}
            }
            e.set_effects(&[ChainSpec {
                track: Some(0),
                effects: vec![s],
            }]);
            render(&mut e, 30_000, 128)
        };
        let (a, b) = (run(), run());
        assert_eq!(bits(&a), bits(&b), "{kind:?}");
        assert_ne!(bits(&a), bits(&plain), "{kind:?} did nothing");
    }
}

#[test]
fn block_size_does_not_change_the_result() {
    let build = || {
        let mut e = engine();
        e.set_effects(&[
            ChainSpec {
                track: None,
                effects: all_chain(),
            },
            ChainSpec {
                track: Some(1),
                effects: all_chain(),
            },
        ]);
        e.set_master(MasterBus::new(0.9, 0.6, 0.3));
        e
    };
    let a = render(&mut build(), 40_000, 128);
    let b = render(&mut build(), 40_000, 37);
    assert_eq!(bits(&a), bits(&b));
}

#[test]
fn master_chain_runs_before_the_master_volume() {
    // A stereo tool at half volume on the master, then volume 0.5: a quarter of the sum.
    let plain = render(&mut engine(), 20_000, 128);
    let mut e = engine();
    let mut tool = FxSpec::new(EffectKind::StereoTool);
    assert!(tool.set(0, 500));
    e.set_effects(&[ChainSpec {
        track: None,
        effects: vec![tool],
    }]);
    e.set_master(MasterBus::new(0.5, 0.5, 0.0));
    let out = render(&mut e, 20_000, 128);
    for (a, b) in plain.iter().zip(&out) {
        assert!((a * 0.25 - b).abs() < 1e-6);
    }
}
