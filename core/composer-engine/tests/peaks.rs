//! Per track peaks (T27) and the opt-in master soft clip (D44).
use composer_engine::{
    Engine, EngineConfig, Event, EventKind, Instrument, InstrumentDef, SynthParams, TrackSetup,
    Waveform, MAX_BLOCK,
};

fn engine(gains: &[f32], loud: bool) -> Engine {
    let def = InstrumentDef::Synth(SynthParams {
        waveform: Waveform::Saw,
        ..SynthParams::default()
    });
    let tracks = gains
        .iter()
        .map(|g| TrackSetup {
            instrument: Instrument::new(&def, 48_000),
            gain: *g,
            pan: 0.0,
        })
        .collect();
    let reps = if loud { 8 } else { 1 };
    let events = (0..reps)
        .flat_map(|k| {
            (0..gains.len() as u32).map(move |t| Event {
                frame: 0,
                track: t,
                kind: EventKind::NoteOn {
                    pitch: 60 + k as u8,
                    velocity: 127,
                },
            })
        })
        .collect();
    Engine::with_rate(
        EngineConfig {
            voices_per_track: 8,
        },
        tracks,
        events,
        48_000,
    )
}

#[test]
fn peaks_follow_track_order_and_gain() {
    let mut e = engine(&[1.0, 0.0, 0.25], false);
    let (mut l, mut r) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);
    for _ in 0..4 {
        e.process(&mut l, &mut r);
    }
    let p = e.track_peaks().to_vec();
    assert_eq!(p.len(), 3);
    assert!(p[0] > 0.0 && p[1] == 0.0 && p[2] > 0.0);
    assert!(p[2] < p[0]);
}

#[test]
fn soft_clip_is_off_by_default_and_bounded_when_on() {
    let run = |ceiling: Option<f32>| {
        let mut e = engine(&[2.0, 2.0], true);
        e.set_ceiling(ceiling);
        let (mut l, mut r) = ([0.0f32; MAX_BLOCK], [0.0f32; MAX_BLOCK]);
        let mut out = Vec::new();
        for _ in 0..8 {
            e.process(&mut l, &mut r);
            out.extend_from_slice(&l);
        }
        out
    };
    let plain = run(None);
    assert!(plain.iter().any(|s| s.abs() > 1.0));
    let clipped = run(Some(0.9));
    assert!(clipped.iter().all(|s| s.abs() < 1.0));
    // Below the ceiling nothing changes.
    for (a, b) in plain.iter().zip(&clipped) {
        if a.abs() <= 0.9 {
            assert_eq!(a, b);
        }
    }
}
