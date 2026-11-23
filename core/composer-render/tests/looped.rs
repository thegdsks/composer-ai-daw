//! Looped clips render like the same notes written out by hand (D24).

use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, RenderSettings};
use std::collections::BTreeMap;
use std::time::Instant;

const BAR: i64 = 3840;

fn pattern() -> Vec<(i64, i64, u8)> {
    vec![
        (0, 960, 60),
        (BAR, 480, 64),
        (3 * BAR + 960, 1920, 67),
        (7 * BAR, 3840, 55),
    ]
}

fn project(notes: Vec<(i64, i64, u8)>, length: i64, loop_length: i64) -> Project {
    let map: BTreeMap<Id, Note> = notes
        .into_iter()
        .enumerate()
        .map(|(i, (start, duration, pitch))| {
            let id = Id(100 + i as u128);
            (
                id,
                Note {
                    id,
                    pitch,
                    velocity: 100,
                    start,
                    duration,
                },
            )
        })
        .collect();
    let region = Region {
        id: Id(11),
        start: 0,
        length,
        loop_length,
        content: RegionContent::Midi { notes: map },
    };
    let mut p = Project::new(Id(1), "loop");
    p.tracks.push(Track {
        id: Id(10),
        name: "keys".into(),
        kind: TrackKind::Instrument,
        instrument: None,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    });
    p
}

#[test]
fn looped_clip_matches_hand_written_repeats() {
    let settings = RenderSettings::new(24_000, 0.5);
    let length = 8 * BAR * 2 + 2 * BAR;
    let looped = project(pattern(), length, 8 * BAR);
    let mut written = Vec::new();
    for pass in 0..2 {
        for (s, d, p) in pattern() {
            written.push((s + pass * 8 * BAR, d, p));
        }
    }
    // The third pass is cut at the clip end: only the first two bars exist.
    for (s, d, p) in pattern() {
        if s < 2 * BAR {
            written.push((s + 16 * BAR, d.min(2 * BAR - s), p));
        }
    }
    let by_hand = project(written, length, 0);
    let a = render_project(&looped, &settings);
    let b = render_project(&by_hand, &settings);
    assert_eq!(a.frames, b.frames);
    assert_eq!(a.samples, b.samples);
    assert_eq!(a.samples, render_project(&looped, &settings).samples);
}

#[test]
fn loop_longer_than_clip_changes_nothing() {
    let settings = RenderSettings::new(24_000, 0.5);
    let plain = render_project(&project(pattern(), 8 * BAR, 0), &settings);
    let long = render_project(&project(pattern(), 8 * BAR, 16 * BAR), &settings);
    assert_eq!(plain.samples, long.samples);
}

/// 60 minutes from one 8 bar looped clip. Run with
/// `COMPOSER_BENCH_MAX_RENDER_RTF=0.05 cargo test --release -p composer-render --test looped -- --ignored`.
#[test]
#[ignore = "long: run in release"]
fn sixty_minute_looped_clip_stays_under_the_render_budget() {
    // 120 bpm: 2 beats per second, 960 ticks per beat.
    let length = 3600 * 2 * 960;
    let p = project(pattern(), length, 8 * BAR);
    let settings = RenderSettings::new(44_100, 1.0);
    let t = Instant::now();
    let r = render_project(&p, &settings);
    let secs = t.elapsed().as_secs_f64();
    let audio = r.frames as f64 / f64::from(r.sample_rate);
    let rtf = secs / audio;
    eprintln!("60 min looped render: {secs:.2}s for {audio:.0}s of audio, rtf {rtf:.5}");
    assert!(audio >= 3600.0);
    if let Ok(v) = std::env::var("COMPOSER_BENCH_MAX_RENDER_RTF") {
        let max: f64 = v
            .parse()
            .expect("COMPOSER_BENCH_MAX_RENDER_RTF is a number");
        assert!(rtf <= max, "rtf {rtf} over budget {max}");
    }
}
