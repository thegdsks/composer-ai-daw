use composer_engine::{
    Engine, EngineConfig, Event, EventKind, Instrument, InstrumentDef, SynthParams, TrackSetup,
};

fn engine(voices: usize, events: Vec<Event>) -> Engine {
    let inst = Instrument::new(&InstrumentDef::Synth(SynthParams::default()), 48_000);
    let track = TrackSetup {
        instrument: inst,
        gain: 1.0,
        pan: 0.0,
    };
    Engine::new(
        EngineConfig {
            voices_per_track: voices,
        },
        vec![track],
        events,
    )
}

fn on(frame: u64, pitch: u8) -> Event {
    Event {
        frame,
        track: 0,
        kind: EventKind::NoteOn {
            pitch,
            velocity: 100,
        },
    }
}

fn off(frame: u64, pitch: u8) -> Event {
    Event {
        frame,
        track: 0,
        kind: EventKind::NoteOff { pitch },
    }
}

fn run(e: &mut Engine, frames: usize) {
    let (mut l, mut r) = ([0.0; 128], [0.0; 128]);
    let mut left = frames;
    while left > 0 {
        let n = left.min(128);
        e.process(&mut l[..n], &mut r[..n]);
        left -= n;
    }
}

#[test]
fn steals_oldest_held_voice_when_pool_is_full() {
    let mut e = engine(2, vec![on(0, 60), on(1, 62), on(2, 64)]);
    run(&mut e, 2);
    assert_eq!(e.voice_pitches(0), vec![60, 62]);
    run(&mut e, 1);
    // 60 was oldest, its slot now holds 64.
    assert_eq!(e.voice_pitches(0), vec![64, 62]);
}

#[test]
fn prefers_stealing_a_releasing_voice() {
    // 62 is released (still ringing) and younger than 60, yet it is taken first.
    let mut e = engine(2, vec![on(0, 60), on(1, 62), off(2, 62), on(3, 64)]);
    run(&mut e, 4);
    assert_eq!(e.voice_pitches(0), vec![60, 64]);
}

#[test]
fn note_off_releases_oldest_held_voice_of_that_pitch() {
    let mut e = engine(4, vec![on(0, 60), on(1, 60), off(2, 60)]);
    run(&mut e, 3);
    assert_eq!(e.voice_pitches(0), vec![60, 60]);
}

#[test]
fn released_voice_frees_itself() {
    let mut e = engine(1, vec![on(0, 60), off(100, 60)]);
    run(&mut e, 100);
    assert_eq!(e.voice_pitches(0).len(), 1);
    // Default release is 0.1 s = 4800 frames.
    run(&mut e, 5000);
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn live_note_sounds_on_next_block_and_releases() {
    let mut e = engine(4, vec![]);
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    e.process(&mut l, &mut r);
    assert!(l.iter().all(|s| *s == 0.0));
    assert!(e.push_live(
        0,
        EventKind::NoteOn {
            pitch: 69,
            velocity: 100
        }
    ));
    e.process(&mut l, &mut r);
    assert_eq!(e.voice_pitches(0), vec![69]);
    assert!(l.iter().any(|s| *s != 0.0));
    assert!(e.push_live(0, EventKind::NoteOff { pitch: 69 }));
    for _ in 0..60 {
        e.process(&mut l, &mut r);
    }
    assert!(e.voice_pitches(0).is_empty());
}

#[test]
fn live_queue_is_bounded_and_drains() {
    let mut e = engine(4, vec![]);
    let kind = EventKind::NoteOff { pitch: 1 };
    for _ in 0..composer_engine::LIVE_CAPACITY {
        assert!(e.push_live(0, kind));
    }
    assert!(!e.push_live(0, kind));
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    e.process(&mut l, &mut r);
    assert!(e.push_live(0, kind));
}

// ---- built in voices (D29, U06) ----

use composer_engine::{BuiltinVoice, VoiceParams};

const SR: u32 = 48_000;

/// One note of `voice`: on at frame 0, off at `off`, `frames` rendered in 128 frame blocks.
fn render_voice(voice: BuiltinVoice, pitch: u8, velocity: u8, off: u64, frames: usize) -> Vec<f32> {
    let inst = Instrument::new(&InstrumentDef::Voice(VoiceParams::new(voice)), SR);
    let mut e = Engine::new(
        EngineConfig {
            voices_per_track: 4,
        },
        vec![TrackSetup {
            instrument: inst,
            gain: 1.0,
            pan: 0.0,
        }],
        vec![
            Event {
                frame: 0,
                track: 0,
                kind: EventKind::NoteOn { pitch, velocity },
            },
            off_event(off, pitch),
        ],
    );
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    let mut out = Vec::with_capacity(frames * 2);
    let mut left = frames;
    while left > 0 {
        let n = left.min(128);
        e.process(&mut l[..n], &mut r[..n]);
        for i in 0..n {
            out.push(l[i]);
            out.push(r[i]);
        }
        left -= n;
    }
    out
}

fn off_event(frame: u64, pitch: u8) -> Event {
    Event {
        frame,
        track: 0,
        kind: EventKind::NoteOff { pitch },
    }
}

/// FNV-1a over the f32 bits: a golden fingerprint of a render.
fn fingerprint(samples: &[f32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

fn mono(s: &[f32]) -> Vec<f32> {
    s.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect()
}

/// Golden fingerprints of a 4 s render (note on at 0, off at 1.5 s) of A3 (pitch 57),
/// velocity 100, at 48 kHz. Change one only on purpose, and say why in the commit.
const GOLDEN: [(BuiltinVoice, u64); 16] = [
    (BuiltinVoice::SoftPiano, 0x7e41a2faad591e0a),
    (BuiltinVoice::WarmPad, 0x3f8b24125a6c5544),
    (BuiltinVoice::WarmStrings, 0xb9fe9a73ee3b37b5),
    (BuiltinVoice::Pluck, 0x2ab697c751c8635f),
    (BuiltinVoice::SubBass, 0xee2bd9b0492ccd69),
    (BuiltinVoice::ChoirPad, 0x7bc2369d4d3d2ffe),
    (BuiltinVoice::Bell, 0x09e6c97c90bc067c),
    (BuiltinVoice::SoftPulse, 0x9d704536e8b7f045),
    (BuiltinVoice::ElectricPiano, 0xf22fabacc3d0486c),
    (BuiltinVoice::Mallet, 0x5c74dae2d3a703c1),
    (BuiltinVoice::GuitarPluck, 0xf3f608436d46a593),
    (BuiltinVoice::BrassPad, 0xdf42fb7d6ca54936),
    (BuiltinVoice::Flute, 0x2936024af359db9d),
    (BuiltinVoice::Organ, 0x3a6c1f251af092d3),
    (BuiltinVoice::BassPluck, 0xaf5bed001f6cb8dd),
    (BuiltinVoice::Lead, 0x6e705413d8d116f1),
];

#[test]
fn every_voice_has_a_golden_render() {
    let mut wrong = Vec::new();
    for (v, want) in GOLDEN {
        let got = fingerprint(&render_voice(v, 57, 100, 72_000, 4 * 48_000));
        if got != want {
            wrong.push(format!("({v:?}, {got:#018x})"));
        }
    }
    assert!(wrong.is_empty(), "goldens changed: {}", wrong.join(", "));
}

#[test]
fn every_voice_is_clean_across_the_keyboard() {
    for v in BuiltinVoice::ALL {
        for pitch in [24u8, 36, 48, 60, 72, 84, 96, 108, 120] {
            for vel in [1u8, 64, 127] {
                let s = render_voice(v, pitch, vel, 24_000, 3 * 48_000);
                assert!(s.iter().all(|x| x.is_finite()), "{v:?} {pitch} {vel}");
                // Below full scale even at the loudest corner, so one voice never clips.
                assert!(peak(&s) < 0.9, "{v:?} p{pitch} v{vel} peak {}", peak(&s));
                // Releases to true silence by the end of the render.
                let tail = &s[s.len() - 2 * 4800..];
                assert!(
                    peak(tail) < 1e-4
                        || matches!(v, BuiltinVoice::WarmPad | BuiltinVoice::ChoirPad),
                    "{v:?} p{pitch} v{vel} tail {}",
                    peak(tail)
                );
            }
        }
    }
}

#[test]
fn louder_velocity_is_louder_and_every_voice_is_audible() {
    for v in BuiltinVoice::ALL {
        let soft = rms(&render_voice(v, 60, 30, 48_000, 96_000));
        let loud = rms(&render_voice(v, 60, 120, 48_000, 96_000));
        assert!(loud > soft * 1.5, "{v:?} soft {soft} loud {loud}");
        assert!(loud > 0.01, "{v:?} too quiet: {loud}");
    }
}

/// Power of one frequency in a Hann windowed block (Goertzel style sum).
fn power_at(block: &[f32], hz: f32) -> f32 {
    let n = block.len() as f32;
    let w = core::f32::consts::TAU * hz / SR as f32;
    let (mut re, mut im) = (0.0f32, 0.0f32);
    for (i, x) in block.iter().enumerate() {
        let hann = 0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / n).cos();
        re += x * hann * (w * i as f32).cos();
        im += x * hann * (w * i as f32).sin();
    }
    re * re + im * im
}

/// The played pitch must stand out: far more energy at the note than at two unrelated
/// frequencies (a fifth and a tritone away), measured over 0.5 to 1.0 s.
#[test]
fn pitched_voices_play_the_right_note() {
    for v in BuiltinVoice::ALL {
        if v == BuiltinVoice::Bell {
            continue;
        }
        for pitch in [45u8, 57, 69, 81] {
            let m = mono(&render_voice(v, pitch, 100, 96_000, 96_000));
            let w = &m[24_000..48_000];
            let f0 = 440.0 * 2f32.powf((f32::from(pitch) - 69.0) / 12.0);
            let on = power_at(w, f0);
            for off in [0.707, 1.414] {
                let other = power_at(w, f0 * off);
                assert!(
                    on > other * 4.0,
                    "{v:?} pitch {pitch}: {on} vs {other} at x{off}"
                );
            }
        }
    }
}

#[test]
fn envelopes_have_the_intended_shape() {
    let at = |s: &[f32], from: usize, to: usize| rms(&s[from * 2..to * 2]);
    // Pads and strings fade in, they do not start with a click.
    for v in [
        BuiltinVoice::WarmPad,
        BuiltinVoice::ChoirPad,
        BuiltinVoice::WarmStrings,
    ] {
        let s = render_voice(v, 60, 100, 144_000, 192_000);
        assert!(
            at(&s, 0, 480) * 4.0 < at(&s, 48_000, 96_000),
            "{v:?} attack"
        );
    }
    // Plucks, piano and bells die away on their own while the key is held.
    for v in [
        BuiltinVoice::Pluck,
        BuiltinVoice::SoftPiano,
        BuiltinVoice::Bell,
        BuiltinVoice::ElectricPiano,
        BuiltinVoice::Mallet,
        BuiltinVoice::GuitarPluck,
        BuiltinVoice::BassPluck,
    ] {
        let s = render_voice(v, 60, 100, 190_000, 192_000);
        assert!(
            at(&s, 100_000, 150_000) < at(&s, 0, 20_000) * 0.7,
            "{v:?} decay"
        );
    }
    // The pluck loses its brightness: the later part is smoother (fewer sign changes).
    let s = mono(&render_voice(BuiltinVoice::Pluck, 60, 100, 190_000, 96_000));
    let flips = |a: usize, b: usize| s[a..b].windows(2).filter(|w| w[0] * w[1] < 0.0).count();
    assert!(flips(20_000, 24_000) < flips(100, 4_100) * 2);
}

#[test]
fn no_dc_offset_and_no_clicks() {
    for v in BuiltinVoice::ALL {
        let s = mono(&render_voice(v, 60, 100, 48_000, 96_000));
        let mean = s.iter().sum::<f32>() / s.len() as f32;
        assert!(mean.abs() < 0.01, "{v:?} dc {mean}");
        // Largest step between neighbouring samples stays small (no hard edges).
        let step = s.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(step < 0.2, "{v:?} step {step}");
    }
}

#[test]
fn voices_are_deterministic_and_distinct() {
    let mut seen = Vec::new();
    for v in BuiltinVoice::ALL {
        let a = fingerprint(&render_voice(v, 60, 100, 48_000, 96_000));
        let b = fingerprint(&render_voice(v, 60, 100, 48_000, 96_000));
        assert_eq!(a, b, "{v:?}");
        assert!(!seen.contains(&a), "{v:?} sounds like another voice");
        seen.push(a);
    }
}

#[test]
fn sound_ids_and_wave_codes_round_trip() {
    for v in BuiltinVoice::ALL {
        assert_eq!(BuiltinVoice::from_sound_id(v.sound_id()), Some(v));
        assert_eq!(BuiltinVoice::from_wave_code(v.wave_code()), Some(v));
        assert!((2..=17).contains(&v.wave_code()));
    }
    assert_eq!(BuiltinVoice::from_sound_id("freepats-concert-harp"), None);
    assert_eq!(BuiltinVoice::from_wave_code(0), None);
    assert_eq!(BuiltinVoice::from_wave_code(18), None);
}

/// The catalogue file lists every voice as kind builtin with its engine id.
#[test]
fn catalogue_lists_every_builtin_voice() {
    let text = include_str!("../../../sounds/catalogue.yaml");
    for v in BuiltinVoice::ALL {
        let at = text
            .find(&format!("- id: {}\n", v.sound_id()))
            .unwrap_or_else(|| panic!("{} missing from the catalogue", v.sound_id()));
        let entry = &text[at..];
        let entry = &entry[..entry[2..].find("\n  - id:").map_or(entry.len(), |n| n + 2)];
        assert!(entry.contains("kind: builtin\n"), "{}", v.sound_id());
        assert!(entry.contains("bundle: \"yes\""), "{}", v.sound_id());
    }
}
