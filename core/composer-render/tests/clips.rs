use composer_audio::{AudioLimits, DirStore};
use composer_engine::pan_gains;
use composer_model::{Change, Id, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, render_project_stream, RenderSettings};
use std::path::PathBuf;
use std::sync::Arc;

fn id(n: u128) -> Id {
    Id(n)
}

/// 16 bit PCM WAV, `value(k, c)` in -1..1.
fn wav16(channels: u16, rate: u32, frames: usize, value: impl Fn(usize, u16) -> f32) -> Vec<u8> {
    let mut data = Vec::new();
    for k in 0..frames {
        for c in 0..channels {
            data.extend_from_slice(&((value(k, c) * 32767.0).round() as i16).to_le_bytes());
        }
    }
    let align = 2 * channels;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    out.extend_from_slice(&align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

struct Fixture {
    dir: PathBuf,
    store: DirStore,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("composer-render-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = DirStore::new(dir.join("assets"), AudioLimits::default());
        Self { dir, store }
    }

    fn import(&self, bytes: &[u8]) -> String {
        let src = self.dir.join("src.wav");
        std::fs::write(&src, bytes).unwrap();
        self.store.import_file(&src).unwrap().hash.to_hex()
    }

    fn settings(&self) -> RenderSettings {
        let mut s = RenderSettings::new(48_000, 0.0);
        s.audio = Some(Arc::new(self.store.clone()));
        s
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn clip(rid: u128, start: i64, length: i64, asset: &str, offset: u64, gain: u16) -> Region {
    Region {
        loop_length: 0,
        id: id(rid),
        start,
        length,
        content: RegionContent::Clip {
            asset: asset.into(),
            offset,
            gain,
        },
    }
}

fn project(regions: Vec<Region>) -> Project {
    let mut p = Project::new(id(1), "audio");
    let track = Track {
        id: id(10),
        name: "voice".into(),
        kind: TrackKind::Audio,
        instrument: None,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: regions.into_iter().map(|r| (r.id, r)).collect(),
    };
    p.apply(&Change::NewTrack { index: 0, track }).unwrap();
    p
}

fn ramp(k: usize, c: u16) -> f32 {
    ((k % 1000) as f32 / 1000.0) * if c == 0 { 0.5 } else { -0.5 }
}

// 120 BPM: 960 ticks per quarter is 24000 frames at 48 kHz.
const SECOND: i64 = 1920;

#[test]
fn clip_plays_the_source_through_the_strip_and_ends_with_its_length() {
    let fx = Fixture::new("play");
    let hash = fx.import(&wav16(2, 48_000, 96_000, ramp));
    let p = project(vec![clip(11, SECOND, SECOND, &hash, 100, 1000)]);
    let r = render_project(&p, &fx.settings());
    assert!(r.audio_issues.is_empty(), "{:?}", r.audio_issues);
    // Starts at 1 s, runs 1 s, so the output is 2 s long.
    assert_eq!(r.frames, 96_000);
    let (gl, gr) = pan_gains(1.0, 0.0);
    for f in [0usize, 47_999] {
        assert_eq!(
            (r.samples[f * 2], r.samples[f * 2 + 1]),
            (0.0, 0.0),
            "frame {f}"
        );
    }
    for k in [0usize, 1, 500, 40_000, 47_999] {
        let src = 100 + k;
        let want_l = (ramp(src, 0) * 32767.0).round() / 32768.0 * gl;
        let want_r = (ramp(src, 1) * 32767.0).round() / 32768.0 * gr;
        let f = 48_000 + k;
        assert!((r.samples[f * 2] - want_l).abs() < 1e-6, "k {k}");
        assert!((r.samples[f * 2 + 1] - want_r).abs() < 1e-6, "k {k}");
    }
    assert!(r.audio_resident_high_water > 0);
}

#[test]
fn clip_gain_scales_and_mono_fills_both_sides() {
    let fx = Fixture::new("mono");
    let hash = fx.import(&wav16(1, 48_000, 48_000, |_, _| 0.5));
    let loud = render_project(
        &project(vec![clip(11, 0, SECOND, &hash, 0, 1000)]),
        &fx.settings(),
    );
    let soft = render_project(
        &project(vec![clip(11, 0, SECOND, &hash, 0, 500)]),
        &fx.settings(),
    );
    assert!((loud.samples[100 * 2] - loud.samples[100 * 2 + 1]).abs() < 1e-7);
    assert!((soft.samples[100 * 2] * 2.0 - loud.samples[100 * 2]).abs() < 1e-6);
}

#[test]
fn chunk_budget_never_changes_the_bits() {
    let fx = Fixture::new("budget");
    let hash = fx.import(&wav16(2, 44_100, 70_000, ramp));
    let p = project(vec![
        clip(11, 0, 2 * SECOND, &hash, 0, 1000),
        clip(12, 500, SECOND, &hash, 5000, 700),
    ]);
    let wide = render_project(&p, &fx.settings());
    let mut small = fx.settings();
    small.audio_limits.max_resident_bytes = 16 * 1024;
    let tight = render_project(&p, &small);
    assert!(wide.audio_issues.is_empty() && tight.audio_issues.is_empty());
    assert_eq!(wide.samples, tight.samples);
    assert!(tight.audio_resident_high_water <= 16 * 1024);
    assert!(wide.audio_resident_high_water > tight.audio_resident_high_water);
}

#[test]
fn rate_conversion_interpolates_exactly_between_source_frames() {
    let fx = Fixture::new("rate");
    // 24 kHz source played at 48 kHz: every second output frame is the midpoint.
    let hash = fx.import(&wav16(1, 24_000, 24_000, |k, _| (k % 100) as f32 / 200.0));
    let r = render_project(
        &project(vec![clip(11, 0, SECOND, &hash, 0, 1000)]),
        &fx.settings(),
    );
    let (g, _) = pan_gains(1.0, 0.0);
    let q = |k: usize| ((((k % 100) as f32) / 200.0) * 32767.0).round() / 32768.0;
    for k in [10usize, 11, 50, 51] {
        let want = if k % 2 == 0 {
            q(k / 2)
        } else {
            (q(k / 2) + q(k / 2 + 1)) / 2.0
        } * g;
        assert!((r.samples[k * 2] - want).abs() < 1e-5, "frame {k}");
    }
}

#[test]
fn bad_clips_stay_silent_and_are_reported_without_failing_the_render() {
    let fx = Fixture::new("issues");
    let hash = fx.import(&wav16(1, 48_000, 48_000, |_, _| 0.5));
    let missing = "ab".repeat(32);
    let p = project(vec![
        clip(11, 0, SECOND, &hash, 0, 1000),
        clip(12, SECOND, SECOND, &missing, 0, 1000),
    ]);
    let r = render_project(&p, &fx.settings());
    assert_eq!(r.frames, 96_000);
    assert_eq!(r.audio_issues.len(), 1);
    assert_eq!(
        (r.audio_issues[0].region, r.audio_issues[0].code),
        (id(12), "asset_missing")
    );
    assert!(r.samples[100 * 2] != 0.0);
    assert!(r.samples[(50_000 * 2) as usize..].iter().all(|v| *v == 0.0));
    // No provider at all.
    let bare = RenderSettings::new(48_000, 0.0);
    let r = render_project(&p, &bare);
    assert_eq!(r.audio_issues.len(), 2);
    assert!(r.audio_issues.iter().all(|i| i.code == "no_audio_source"));
    // A clip longer than its source goes quiet after the source ends.
    let long = project(vec![clip(13, 0, 3 * SECOND, &hash, 24_000, 1000)]);
    let r = render_project(&long, &fx.settings());
    assert_eq!(r.frames, 144_000);
    assert!(r.samples[10 * 2] != 0.0);
    assert!(r.samples[30_000 * 2..].iter().all(|v| *v == 0.0));
}

#[test]
fn overlapping_clips_over_budget_are_skipped_and_reported() {
    let fx = Fixture::new("over");
    let hash = fx.import(&wav16(2, 48_000, 48_000, ramp));
    let p = project(vec![
        clip(11, 0, SECOND, &hash, 0, 1000),
        clip(12, 0, SECOND, &hash, 0, 1000),
        clip(13, 0, SECOND, &hash, 0, 1000),
    ]);
    let mut s = fx.settings();
    // Room for two minimum chunks (256 frames * 2 channels * 4 bytes each) but not three.
    s.audio_limits.max_resident_bytes = 2 * 256 * 2 * 4 + 100;
    let r = render_project(&p, &s);
    assert_eq!(r.audio_issues.len(), 1);
    assert_eq!(r.audio_issues[0].code, "over_budget");
    assert!(r.audio_resident_high_water <= s.audio_limits.max_resident_bytes);
}

#[test]
fn stream_and_collect_agree_and_projects_without_audio_are_unchanged() {
    let fx = Fixture::new("stream");
    let hash = fx.import(&wav16(2, 48_000, 48_000, ramp));
    let p = project(vec![clip(11, 0, SECOND, &hash, 0, 1000)]);
    let whole = render_project(&p, &fx.settings());
    let mut seen = Vec::new();
    let mut max_block = 0;
    let sum = render_project_stream(&p, &fx.settings(), &mut |b| {
        max_block = max_block.max(b.len());
        seen.extend_from_slice(b);
    });
    assert_eq!(seen, whole.samples);
    assert_eq!(sum.frames, whole.frames);
    assert!(max_block <= 256);
    // A project with no clips never touches the audio path.
    let plain = Project::new(id(2), "plain");
    let r = render_project(&plain, &fx.settings());
    assert_eq!((r.frames, r.audio_resident_high_water), (0, 0));
}
