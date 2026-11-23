use composer_engine::{InstrumentDef, SampleData, SamplerParams, BUILTIN_SAW};
use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{
    read_wav, render_project, write_wav, Fallback, RenderSettings, TrackMix, WavFormat,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const SR: u32 = 48_000;

fn note(id: u128, pitch: u8, start: i64, duration: i64) -> (Id, Note) {
    (
        Id(id),
        Note {
            id: Id(id),
            pitch,
            velocity: 100,
            start,
            duration,
        },
    )
}

fn track(id: u128, instrument: Option<Id>, notes: Vec<(Id, Note)>) -> Track {
    let region = Region {
        loop_length: 0,
        id: Id(id + 1000),
        start: 0,
        length: 20_000,
        content: RegionContent::Midi {
            notes: notes.into_iter().collect(),
        },
    };
    Track {
        id: Id(id),
        name: format!("t{id}"),
        kind: TrackKind::Instrument,
        instrument,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    }
}

/// Two tracks; 120 bpm, then 60 bpm from tick 3840 (frame 96000).
fn demo() -> Project {
    let mut p = Project::new(Id(1), "demo");
    p.tempo_map.tempos.insert(3840, 60_000);
    p.tracks.push(track(
        10,
        Some(BUILTIN_SAW),
        vec![note(100, 60, 0, 960), note(101, 64, 3840, 960)],
    ));
    p.tracks.push(track(
        20,
        None,
        vec![note(200, 48, 960, 1920), note(201, 55, 4800, 960)],
    ));
    p
}

fn fnv(samples: &[f32]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn first_nonzero(r: &composer_render::Rendered, from: usize) -> Option<usize> {
    (from..r.frames as usize).find(|i| r.samples[i * 2] != 0.0 || r.samples[i * 2 + 1] != 0.0)
}

#[test]
fn render_is_deterministic_and_pinned() {
    let settings = RenderSettings::new(SR, 0.25);
    let a = render_project(&demo(), &settings);
    let b = render_project(&demo(), &settings);
    assert_eq!(a, b);
    assert_eq!(fnv(&a.samples), fnv(&b.samples));
    // Last off: tick 5760 at 60 bpm = 96000 + 96000 frames, plus 12000 tail.
    assert_eq!(a.frames, 204_000);
    assert_eq!(a.samples.len(), 408_000);
    assert_eq!(a.clipped_frames, 0);
    assert!(a.fallbacks.is_empty());
    // Pinned digest: if this changes, the render output changed.
    assert_eq!(
        fnv(&a.samples),
        PINNED_HASH,
        "hash was {:#x}",
        fnv(&a.samples)
    );
}

const PINNED_HASH: u64 = 0x9234f99f24d6c39d;

#[test]
fn note_on_and_off_land_on_exact_frames() {
    // One note, tick 960..1920 at 120 bpm: on at frame 24000, off at 48000.
    let mut p = Project::new(Id(1), "one");
    p.tracks.push(track(10, None, vec![note(1, 69, 960, 960)]));
    let r = render_project(&p, &RenderSettings::new(SR, 0.2));
    assert_eq!(first_nonzero(&r, 0), Some(24_001));
    assert_eq!(r.frames, 48_000 + 9_600);
    // Frame 24000 is the onset: envelope level is 0 there, so sound begins one frame later.
    assert!(r.samples[24_000 * 2] == 0.0 && r.samples[24_001 * 2] != 0.0);
    // Release starts at frame 48000 and ends 0.1 s later (4800 frames), then silence.
    assert!(r.samples[(48_000 + 100) * 2] != 0.0);
    let end = 48_000 + 4_800;
    assert!(r.samples[(end + 2) * 2..].iter().all(|s| *s == 0.0));
}

#[test]
fn tempo_change_moves_the_second_note() {
    let mut p = Project::new(Id(1), "t");
    p.tempo_map.tempos.insert(3840, 60_000);
    p.tracks.push(track(10, None, vec![note(1, 69, 4800, 960)]));
    let r = render_project(&p, &RenderSettings::new(SR, 0.0));
    // tick 4800 = 96000 + 48000 frames.
    assert_eq!(first_nonzero(&r, 0), Some(144_001));
}

#[test]
fn zero_notes_is_silence_of_tail_length() {
    let p = Project::new(Id(1), "empty");
    let r = render_project(&p, &RenderSettings::new(SR, 1.5));
    assert_eq!(r.frames, 72_000);
    assert_eq!(r.samples.len(), 144_000);
    assert!(r.samples.iter().all(|s| *s == 0.0));
    let r = render_project(&p, &RenderSettings::new(SR, 0.0));
    assert!(r.samples.is_empty());
}

#[test]
fn unknown_instrument_falls_back_and_is_reported() {
    let mut p = Project::new(Id(1), "x");
    p.tracks
        .push(track(10, Some(Id(0xdead)), vec![note(1, 69, 0, 960)]));
    let r = render_project(&p, &RenderSettings::new(SR, 0.1));
    assert_eq!(
        r.fallbacks,
        vec![Fallback {
            track: Id(10),
            instrument: Id(0xdead)
        }]
    );
    assert!(r.samples.iter().any(|s| *s != 0.0));
}

#[test]
fn registered_sampler_plays_and_pans() {
    let sample: Vec<f32> = (0..4800)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    let data = SampleData::new(SR, 1, 60, sample).unwrap();
    let mut settings = RenderSettings::new(SR, 0.1);
    settings.instruments.insert(
        Id(77),
        InstrumentDef::Sampler(SamplerParams::new(Arc::new(data))),
    );
    settings.mix.insert(
        Id(10),
        TrackMix {
            gain: 1.0,
            pan: -1.0,
        },
    );
    let mut p = Project::new(Id(1), "s");
    p.tracks
        .push(track(10, Some(Id(77)), vec![note(1, 60, 0, 960)]));
    let r = render_project(&p, &settings);
    assert!(r.fallbacks.is_empty());
    assert!(r.samples.iter().step_by(2).any(|s| *s != 0.0));
    assert!(r.samples.iter().skip(1).step_by(2).all(|s| s.abs() < 1e-6));
}

#[test]
fn overs_are_counted_not_clamped() {
    let mut p = Project::new(Id(1), "loud");
    let notes = (0..40).map(|i| note(i + 1, 60, 0, 960)).collect();
    p.tracks.push(track(10, Some(BUILTIN_SAW), notes));
    let mut s = RenderSettings::new(SR, 0.0);
    s.voices_per_track = 40;
    s.mix.insert(
        Id(10),
        TrackMix {
            gain: 4.0,
            pan: 0.0,
        },
    );
    let r = render_project(&p, &s);
    assert!(r.clipped_frames > 0);
    assert!(r.samples.iter().any(|s| s.abs() > 1.0));
}

#[test]
fn rendered_audio_survives_a_wav_round_trip() {
    let r = render_project(&demo(), &RenderSettings::new(SR, 0.1));
    let bytes = write_wav(WavFormat::Float32, 2, SR, &r.samples);
    let w = read_wav(&bytes).unwrap();
    assert_eq!(w.samples, r.samples);
    let w16 = read_wav(&write_wav(WavFormat::Pcm16, 2, SR, &r.samples)).unwrap();
    assert_eq!(w16.samples.len(), r.samples.len());
    let worst = r
        .samples
        .iter()
        .zip(&w16.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(worst < 1.0 / 16_000.0);
}
