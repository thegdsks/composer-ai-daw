//! Sample bank playback: pitch shift, loops, velocity layers, stacking.

use composer_engine::{
    Adsr, BankError, BankParams, Engine, EngineConfig, Event, EventKind, Instrument, InstrumentDef,
    LoopMode, SampleBank, SampleData, TrackSetup, Zone,
};
use std::sync::Arc;

const RATE: u32 = 48_000;

fn ramp(n: usize, scale: f32) -> SampleData {
    let data = (0..n).map(|i| i as f32 / scale).collect();
    SampleData::new(RATE, 1, 0, data).unwrap()
}

fn flat(n: usize, v: f32) -> SampleData {
    SampleData::new(RATE, 1, 0, vec![v; n]).unwrap()
}

fn zone(sample: u32) -> Zone {
    Zone {
        sample,
        key_lo: 0,
        key_hi: 127,
        vel_lo: 0,
        vel_hi: 127,
        root_key: 60,
        tune_cents: 0,
        scale_cents: 100,
        loop_mode: LoopMode::None,
        loop_start: 0,
        loop_end: 0,
        adsr: Adsr {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 10.0,
        },
        gain: 1.0,
        pan: 0.0,
    }
}

/// Hard left track so the left output is the voice itself.
fn engine(samples: Vec<SampleData>, zones: Vec<Zone>, events: Vec<Event>) -> Engine {
    let bank = Arc::new(SampleBank::new(samples, zones).unwrap());
    let instrument = Instrument::new(&InstrumentDef::Bank(BankParams { bank, gain: 1.0 }), RATE);
    Engine::new(
        EngineConfig {
            voices_per_track: 8,
        },
        vec![TrackSetup {
            instrument,
            gain: 1.0,
            pan: -1.0,
        }],
        events,
    )
}

fn on(frame: u64, pitch: u8, velocity: u8) -> Event {
    Event {
        frame,
        track: 0,
        kind: EventKind::NoteOn { pitch, velocity },
    }
}

fn off(frame: u64, pitch: u8) -> Event {
    Event {
        frame,
        track: 0,
        kind: EventKind::NoteOff { pitch },
    }
}

fn run(e: &mut Engine, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (Vec::new(), Vec::new());
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    let mut left = frames;
    while left > 0 {
        let n = left.min(128);
        e.process(&mut l[..n], &mut r[..n]);
        ol.extend_from_slice(&l[..n]);
        or.extend_from_slice(&r[..n]);
        left -= n;
    }
    (ol, or)
}

#[test]
fn octave_up_plays_at_double_speed() {
    let mut e = engine(vec![ramp(400, 1000.0)], vec![zone(0)], vec![on(0, 72, 127)]);
    let (l, _) = run(&mut e, 100);
    for (i, v) in l.iter().enumerate() {
        let want = (2 * i) as f32 / 1000.0;
        assert!((v - want).abs() < 1e-5, "frame {i}: {v} vs {want}");
    }
}

#[test]
fn tuning_in_cents_shifts_pitch() {
    // +1200 cents on the root key is one octave.
    let mut z = zone(0);
    z.tune_cents = 1200;
    let mut e = engine(vec![ramp(400, 1000.0)], vec![z], vec![on(0, 60, 127)]);
    let (l, _) = run(&mut e, 50);
    assert!((l[10] - 0.02).abs() < 1e-5);
}

#[test]
fn sample_rate_difference_is_compensated() {
    let s = SampleData::new(24_000, 1, 0, (0..400).map(|i| i as f32 / 1000.0).collect()).unwrap();
    let mut e = engine(vec![s], vec![zone(0)], vec![on(0, 60, 127)]);
    let (l, _) = run(&mut e, 50);
    // Half rate sample at the root key advances half a frame per output frame.
    assert!((l[10] - 0.005).abs() < 1e-5);
}

#[test]
fn continuous_loop_wraps_and_keeps_sounding() {
    let mut z = zone(0);
    z.loop_mode = LoopMode::Continuous;
    z.loop_start = 20;
    z.loop_end = 60;
    let mut e = engine(
        vec![ramp(100, 100.0)],
        vec![z],
        vec![on(0, 60, 127), off(200, 60)],
    );
    let (l, _) = run(&mut e, 400);
    // Frame 130 sits at 20 + (130 - 20) % 40 = 50.
    assert!((l[130] - 0.5).abs() < 1e-5);
    // Continuous loops keep going while the release (10 s) is running.
    assert_eq!(e.voice_pitches(0), vec![60]);
}

#[test]
fn sustain_loop_plays_on_after_note_off() {
    let mut z = zone(0);
    z.loop_mode = LoopMode::Sustain;
    z.loop_start = 20;
    z.loop_end = 60;
    let mut e = engine(
        vec![ramp(100, 100.0)],
        vec![z],
        vec![on(0, 60, 127), off(200, 60)],
    );
    run(&mut e, 200);
    assert_eq!(e.voice_pitches(0), vec![60]);
    // After the key is up the sample runs out within 80 frames despite a 10 s release.
    run(&mut e, 120);
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn one_shot_ignores_note_off() {
    let mut z = zone(0);
    z.loop_mode = LoopMode::OneShot;
    let mut e = engine(
        vec![flat(300, 0.5)],
        vec![z],
        vec![on(0, 60, 127), off(10, 60)],
    );
    let (l, _) = run(&mut e, 250);
    assert!((l[249] - 0.5).abs() < 1e-6);
    run(&mut e, 100);
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn velocity_layers_pick_their_sample() {
    let mut soft = zone(0);
    soft.vel_hi = 63;
    let mut loud = zone(1);
    loud.vel_lo = 64;
    let samples = vec![flat(100, 0.25), flat(100, 0.5)];
    let mut e = engine(samples.clone(), vec![soft, loud], vec![on(0, 60, 100)]);
    let (l, _) = run(&mut e, 10);
    assert!((l[5] - 0.5 * 100.0 / 127.0).abs() < 1e-5);
    let mut e = engine(samples, vec![soft, loud], vec![on(0, 60, 30)]);
    let (l, _) = run(&mut e, 10);
    assert!((l[5] - 0.25 * 30.0 / 127.0).abs() < 1e-5);
}

#[test]
fn uncovered_note_is_silent_and_takes_no_voice() {
    let mut z = zone(0);
    z.key_lo = 50;
    z.key_hi = 60;
    let mut e = engine(vec![flat(100, 0.5)], vec![z], vec![on(0, 90, 100)]);
    let (l, r) = run(&mut e, 10);
    assert!(l.iter().chain(r.iter()).all(|v| *v == 0.0));
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn stacked_zones_sound_together_and_release_together() {
    let mut a = zone(0);
    a.pan = -1.0;
    a.adsr.release = 0.01;
    let mut b = zone(1);
    b.pan = 1.0;
    b.adsr.release = 0.01;
    let samples = vec![flat(5000, 0.5), flat(5000, 0.25)];
    let mut e = engine(samples, vec![a, b], vec![on(0, 60, 127), off(100, 60)]);
    run(&mut e, 50);
    assert_eq!(e.voice_pitches(0), vec![60, 60]);
    run(&mut e, 100);
    // 10 ms release is 480 frames: both layers are releasing, none left held.
    assert_eq!(e.voice_pitches(0).len(), 2);
    run(&mut e, 600);
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn bank_validation_fails_closed() {
    let s = vec![flat(10, 0.0)];
    let mut z = zone(0);
    z.sample = 3;
    assert_eq!(
        SampleBank::new(s.clone(), vec![z]),
        Err(BankError::SampleIndex)
    );
    let mut z = zone(0);
    z.loop_mode = LoopMode::Continuous;
    z.loop_start = 2;
    z.loop_end = 11;
    assert_eq!(SampleBank::new(s.clone(), vec![z]), Err(BankError::Loop));
    let mut z = zone(0);
    z.key_lo = 90;
    z.key_hi = 10;
    assert_eq!(SampleBank::new(s.clone(), vec![z]), Err(BankError::Range));
    let mut z = zone(0);
    z.gain = f32::NAN;
    assert_eq!(SampleBank::new(s.clone(), vec![z]), Err(BankError::Value));
    assert_eq!(SampleBank::new(s, vec![]), Err(BankError::Empty));
}
