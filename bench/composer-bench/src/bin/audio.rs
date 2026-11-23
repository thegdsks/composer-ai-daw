//! Long audio file scenario (D41, D42): generates a WAV in code, imports it, builds the peak
//! cache and streams a render of one clip, reporting time and memory. Nothing is committed.
//!
//! Env: COMPOSER_BENCH_AUDIO_SECONDS (default 7200), COMPOSER_BENCH_AUDIO_RATE (48000),
//! COMPOSER_BENCH_AUDIO_DIR (default the system temp dir), and budgets
//! COMPOSER_BENCH_MAX_AUDIO_RTF, COMPOSER_BENCH_MAX_AUDIO_PEAKS_MS,
//! COMPOSER_BENCH_MAX_AUDIO_RSS_BYTES. The decode cap is COMPOSER_AUDIO_MAX_RESIDENT_BYTES.

use composer_audio::{AudioLimits, DirStore};
use composer_commands::{Command, Editor};
use composer_model::{Id, Project, TrackKind};
use composer_render::{render_project_stream, RenderSettings};
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::process::{Command as Proc, ExitCode};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn env_u64(name: &str, fallback: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(name: &str) -> Option<f64> {
    env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Resident set size of this process in bytes, from `ps` (std only, no libc).
fn rss_bytes() -> u64 {
    Proc::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map_or(0, |kb| kb * 1024)
}

fn write_wav(path: &std::path::Path, frames: u64, rate: u32) -> std::io::Result<()> {
    let data_len = frames * 4;
    let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&((36 + data_len) as u32).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&rate.to_le_bytes())?;
    w.write_all(&(rate * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&(data_len as u32).to_le_bytes())?;
    // Two sawtooth ramps of different period: cheap, deterministic, not silent.
    for k in 0..frames {
        let a = ((k % 480) as i32 - 240) * 100;
        let b = ((k % 733) as i32 - 366) * 60;
        w.write_all(&(a as i16).to_le_bytes())?;
        w.write_all(&(b as i16).to_le_bytes())?;
    }
    w.flush()
}

fn main() -> ExitCode {
    let seconds = env_u64("COMPOSER_BENCH_AUDIO_SECONDS", 7200);
    let rate = env_u64("COMPOSER_BENCH_AUDIO_RATE", 48_000) as u32;
    let frames = seconds * u64::from(rate);
    // The RIFF size field is 32 bit, so one WAV holds at most about 4 GiB of data.
    if frames * 4 > u64::from(u32::MAX) - 64 {
        eprintln!("too long for a 32 bit WAV: lower COMPOSER_BENCH_AUDIO_SECONDS or the rate");
        return ExitCode::from(2);
    }
    let base = env::var("COMPOSER_BENCH_AUDIO_DIR")
        .map(Into::into)
        .unwrap_or_else(|_| env::temp_dir());
    let dir = base.join(format!("composer-bench-audio-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("bench dir");

    let peak_rss = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (p, s) = (peak_rss.clone(), stop.clone());
        std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                p.fetch_max(rss_bytes(), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };

    let src = dir.join("long.wav");
    let t = Instant::now();
    write_wav(&src, frames, rate).expect("generate wav");
    let generate_ms = t.elapsed().as_secs_f64() * 1e3;

    let limits = AudioLimits::from_env();
    let store = DirStore::new(dir.join("assets"), limits);
    let t = Instant::now();
    let stored = store.import_file(&src).expect("import");
    let import_ms = t.elapsed().as_secs_f64() * 1e3;
    let _ = fs::remove_file(&src);
    let hash = stored.hash.to_hex();

    let t = Instant::now();
    let levels = store.peak_levels(&hash).expect("peaks");
    let peaks_build_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let _ = store.peak_levels(&hash).expect("peaks cached");
    let peaks_cached_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let q = store.peaks(&hash, 0, None, 2000).expect("query");
    let peaks_query_ms = t.elapsed().as_secs_f64() * 1e3;

    let mut ed = Editor::new(Project::new(Id(1), "long audio"));
    ed.execute(&Command::AddTrack {
        id: Id(2),
        name: "long".into(),
        kind: TrackKind::Audio,
        index: None,
    })
    .expect("track");
    // 120 bpm: 1920 ticks per second.
    ed.execute(&Command::AddAudioClip {
        id: Id(3),
        track: Id(2),
        start: 0,
        length: seconds as i64 * 1920,
        asset: hash.clone(),
        offset: None,
        gain: None,
    })
    .expect("clip");
    let mut settings = RenderSettings::new(rate, 0.0);
    settings.audio = Some(store.clone().into_provider());
    settings.audio_limits = limits;
    let (mut peak, mut sum_sq, mut n) = (0.0f32, 0.0f64, 0u64);
    let t = Instant::now();
    let r = render_project_stream(ed.project(), &settings, &mut |b| {
        for v in b {
            peak = peak.max(v.abs());
            sum_sq += f64::from(*v) * f64::from(*v);
        }
        n += b.len() as u64;
    });
    let render_ms = t.elapsed().as_secs_f64() * 1e3;
    stop.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    let rss = peak_rss.load(Ordering::Relaxed);
    let rtf = render_ms / 1e3 / seconds as f64;
    let _ = fs::remove_dir_all(&dir);

    println!("audio_long_file:");
    println!("  audio_seconds: {seconds}");
    println!("  sample_rate: {rate}");
    println!("  file_bytes: {}", stored.bytes);
    println!("  frames: {}", r.frames);
    println!("  generate_ms: {generate_ms:.1}");
    println!("  import_ms: {import_ms:.1}");
    println!("  peaks_build_ms: {peaks_build_ms:.1}");
    println!("  peaks_cached_load_ms: {peaks_cached_ms:.1}");
    println!("  peaks_query_ms: {peaks_query_ms:.2}");
    println!("  peak_blocks_level0: {}", levels.level(0).len());
    println!("  peak_bins_returned: {}", q.bins);
    println!("  render_ms: {render_ms:.1}");
    println!("  render_rtf: {rtf:.5}");
    println!("  render_peak: {peak:.4}");
    println!("  render_rms: {:.4}", (sum_sq / n.max(1) as f64).sqrt());
    println!("  audio_issues: {}", r.audio_issues.len());
    println!(
        "  decode_resident_high_water_bytes: {}",
        r.audio_resident_high_water
    );
    println!("  decode_resident_cap_bytes: {}", limits.max_resident_bytes);
    println!("  process_rss_high_water_bytes: {rss}");

    let mut bad = !r.audio_issues.is_empty();
    let mut check = |name: &str, limit: Option<f64>, actual: f64| {
        if let Some(l) = limit.filter(|l| actual > *l) {
            eprintln!("BUDGET EXCEEDED {name}: limit {l} actual {actual:.2}");
            bad = true;
        }
    };
    check(
        "COMPOSER_BENCH_MAX_AUDIO_RTF",
        env_f64("COMPOSER_BENCH_MAX_AUDIO_RTF"),
        rtf,
    );
    check(
        "COMPOSER_BENCH_MAX_AUDIO_PEAKS_MS",
        env_f64("COMPOSER_BENCH_MAX_AUDIO_PEAKS_MS"),
        peaks_build_ms,
    );
    check(
        "COMPOSER_BENCH_MAX_AUDIO_RSS_BYTES",
        env_f64("COMPOSER_BENCH_MAX_AUDIO_RSS_BYTES"),
        rss as f64,
    );
    if bad {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
