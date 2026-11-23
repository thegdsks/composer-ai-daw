use composer_audio::peaks::{bins_from_source, block_frames, LEVEL_COUNT};
use composer_audio::{
    AudioError, AudioLimits, AudioProvider, AudioStream, DirStore, PeakLevels, WavStream,
};
use composer_format::ContentHash;
use std::io::Cursor;
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Fmt {
    P8,
    P16,
    P24,
    P32,
    F32,
}

impl Fmt {
    const ALL: [Fmt; 5] = [Fmt::P8, Fmt::P16, Fmt::P24, Fmt::P32, Fmt::F32];
    fn bytes(self) -> usize {
        match self {
            Fmt::P8 => 1,
            Fmt::P16 => 2,
            Fmt::P24 => 3,
            Fmt::P32 | Fmt::F32 => 4,
        }
    }
}

/// A WAV whose sample k of channel c is `value(k, c)` in -1..1.
fn wav(
    fmt: Fmt,
    channels: u16,
    rate: u32,
    frames: usize,
    value: impl Fn(usize, u16) -> f32,
) -> Vec<u8> {
    let w = fmt.bytes();
    let align = w * usize::from(channels);
    let mut data = Vec::with_capacity(frames * align);
    for k in 0..frames {
        for c in 0..channels {
            let v = value(k, c);
            match fmt {
                Fmt::P8 => data.push((v * 127.0 + 128.0).round() as u8),
                Fmt::P16 => data.extend_from_slice(&((v * 32767.0).round() as i16).to_le_bytes()),
                Fmt::P24 => {
                    let x = (v * 8_388_607.0).round() as i32;
                    data.extend_from_slice(&x.to_le_bytes()[..3]);
                }
                Fmt::P32 => data.extend_from_slice(&((v * 2_147_483_647.0) as i32).to_le_bytes()),
                Fmt::F32 => data.extend_from_slice(&v.to_le_bytes()),
            }
        }
    }
    let tag: u16 = if matches!(fmt, Fmt::F32) { 3 } else { 1 };
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * align as u32).to_le_bytes());
    out.extend_from_slice(&(align as u16).to_le_bytes());
    out.extend_from_slice(&((w * 8) as u16).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

fn tri(k: usize, c: u16) -> f32 {
    let p = (k * 7 + usize::from(c) * 3) % 200;
    (p as f32 - 100.0) / 100.0
}

fn read_all(s: &mut dyn AudioStream) -> Result<Vec<f32>, AudioError> {
    let info = s.info();
    let ch = usize::from(info.channels);
    let mut out = vec![0.0; info.frames as usize * ch];
    let mut at = 0usize;
    while at < info.frames as usize {
        let n = s.read(
            at as u64,
            &mut out[at * ch..(at + 513.min(info.frames as usize - at)) * ch],
        )?;
        if n == 0 {
            break;
        }
        at += n;
    }
    Ok(out)
}

fn open(bytes: Vec<u8>) -> Result<WavStream<Cursor<Vec<u8>>>, AudioError> {
    WavStream::open(Cursor::new(bytes), &AudioLimits::default())
}

#[test]
fn decodes_every_format_within_quantisation() {
    for fmt in Fmt::ALL {
        let bytes = wav(fmt, 2, 44_100, 3000, tri);
        let mut s = open(bytes).unwrap();
        let info = s.info();
        assert_eq!(
            (info.sample_rate, info.channels, info.frames),
            (44_100, 2, 3000)
        );
        let got = read_all(&mut s).unwrap();
        let tol = match fmt {
            Fmt::P8 => 0.02,
            Fmt::P16 => 0.0001,
            _ => 0.00001,
        };
        for k in 0..3000 {
            for c in 0..2u16 {
                let d = (got[k * 2 + usize::from(c)] - tri(k, c)).abs();
                assert!(d <= tol, "frame {k} ch {c} off by {d}");
            }
        }
    }
}

#[test]
fn random_access_equals_sequential() {
    let mut s = open(wav(Fmt::P24, 1, 48_000, 5000, tri)).unwrap();
    let all = read_all(&mut s).unwrap();
    let mut piece = vec![0.0; 100];
    for start in [4900u64, 0, 2500, 17, 4999] {
        let n = s.read(start, &mut piece).unwrap();
        assert_eq!(n as u64, 100.min(5000 - start));
        assert_eq!(&piece[..n], &all[start as usize..start as usize + n]);
    }
    assert_eq!(s.read(5000, &mut piece).unwrap(), 0);
}

#[test]
fn every_prefix_of_every_format_is_safe() {
    for fmt in Fmt::ALL {
        for channels in [1u16, 2] {
            let full = wav(fmt, channels, 22_050, 40, tri);
            for n in 0..full.len() {
                // A cut file declares more data than it has: always an error, never a panic.
                assert!(open(full[..n].to_vec()).is_err(), "prefix {n}");
            }
            assert!(open(full.clone()).is_ok());
            // A streaming writer leaves the data size at zero: any prefix with a whole frame opens
            // and reads to its own end.
            let mut open_ended = full.clone();
            open_ended[40..44].copy_from_slice(&0u32.to_le_bytes());
            for n in 0..open_ended.len() {
                if let Ok(mut s) = open(open_ended[..n].to_vec()) {
                    let info = s.info();
                    assert!(info.frames <= 40);
                    read_all(&mut s).unwrap();
                }
            }
        }
    }
}

#[test]
fn damaged_headers_never_panic() {
    let base = wav(Fmt::P16, 2, 48_000, 64, tri);
    let mut seed = 0x1234_5678_9abc_def1u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..4000 {
        let mut b = base.clone();
        for _ in 0..1 + next() % 4 {
            let i = (next() % 64) as usize;
            b[i] = next() as u8;
        }
        if let Ok(mut s) = open(b) {
            let _ = read_all(&mut s);
        }
    }
}

#[test]
fn caps_are_checked_before_allocating() {
    let bytes = wav(Fmt::P16, 2, 48_000, 1000, tri);
    let with = |f: &dyn Fn(&mut AudioLimits)| {
        let mut l = AudioLimits::default();
        f(&mut l);
        WavStream::open(Cursor::new(bytes.clone()), &l).err()
    };
    assert_eq!(
        with(&|l| l.max_frames = 999),
        Some(AudioError::TooLarge("frames"))
    );
    assert_eq!(
        with(&|l| l.max_file_bytes = 100),
        Some(AudioError::TooLarge("file bytes"))
    );
    assert_eq!(
        with(&|l| l.max_channels = 1),
        Some(AudioError::Unsupported("channel count"))
    );
    // A header that claims four billion bytes of data in a tiny file is Truncated, nothing is allocated.
    let mut lie = bytes.clone();
    lie[40..44].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    assert_eq!(open(lie).err(), Some(AudioError::Truncated));
    let mut junk = bytes.clone();
    junk[0..4].copy_from_slice(b"RF64");
    assert!(matches!(open(junk).err(), Some(AudioError::Unsupported(_))));
    assert_eq!(open(vec![1, 2, 3]).err(), Some(AudioError::NotWav));
}

#[test]
fn limits_read_the_environment_names() {
    let l = AudioLimits::from_lookup(|k| match k {
        "COMPOSER_AUDIO_MAX_RESIDENT_BYTES" => Some(" 1234 ".into()),
        "COMPOSER_AUDIO_MAX_CHANNELS" => Some("0".into()),
        "COMPOSER_AUDIO_MAX_PEAK_BINS" => Some("77".into()),
        _ => None,
    });
    assert_eq!(l.max_resident_bytes, 1234);
    assert_eq!(l.max_peak_bins, 77);
    assert_eq!(l.max_channels, AudioLimits::default().max_channels);
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("composer-audio-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn brute(frames: &[f32], ch: usize, from: usize, to: usize) -> (f32, f32) {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for f in from..to {
        for c in 0..ch {
            lo = lo.min(frames[f * ch + c]);
            hi = hi.max(frames[f * ch + c]);
        }
    }
    (lo, hi)
}

#[test]
fn peaks_match_a_brute_force_scan_at_every_level() {
    let frames = 300_000usize;
    let mut s = open(wav(Fmt::F32, 2, 48_000, frames, |k, c| {
        ((k as f32 * 0.001 + f32::from(c)).sin()) * 0.9
    }))
    .unwrap();
    let all = read_all(&mut s).unwrap();
    let levels = PeakLevels::build(&mut s).unwrap();
    for l in 0..LEVEL_COUNT {
        let bs = block_frames(l) as usize;
        for (i, p) in levels.level(l).iter().enumerate().step_by(7) {
            let to = ((i + 1) * bs).min(frames);
            let (lo, hi) = brute(&all, 2, i * bs, to);
            assert!(
                (f32::from(p.min) / 32767.0 - lo).abs() < 0.0001,
                "level {l} block {i}"
            );
            assert!(
                (f32::from(p.max) / 32767.0 - hi).abs() < 0.0001,
                "level {l} block {i}"
            );
        }
    }
    let bytes = levels.encode();
    assert_eq!(
        PeakLevels::decode(&bytes, levels.frames, 2).unwrap(),
        levels
    );
    assert!(PeakLevels::decode(&bytes[..bytes.len() - 1], levels.frames, 2).is_err());
    assert!(PeakLevels::decode(&bytes, levels.frames + 1, 2).is_err());
    assert!(PeakLevels::decode(&bytes, levels.frames, 1).is_err());
    // Fine ranges come from the source and agree with the scan.
    let fine = bins_from_source(&mut s, 1000, 1100, 4).unwrap();
    let (lo, hi) = brute(&all, 2, 1000, 1025);
    assert!((f32::from(fine[0].min) / 32767.0 - lo).abs() < 0.0001);
    assert!((f32::from(fine[0].max) / 32767.0 - hi).abs() < 0.0001);
}

#[test]
fn store_imports_by_hash_dedupes_and_rebuilds_a_bad_sidecar() {
    let dir = temp_dir("store");
    let src = dir.join("in.wav");
    let bytes = wav(Fmt::P16, 2, 48_000, 100_000, tri);
    std::fs::write(&src, &bytes).unwrap();
    let store = DirStore::new(dir.join("assets"), AudioLimits::default());
    let a = store.import_file(&src).unwrap();
    assert_eq!(a.hash, ContentHash::of(&bytes));
    assert_eq!(a.bytes, bytes.len() as u64);
    assert_eq!((a.info.channels, a.info.frames), (2, 100_000));
    let b = store.import_file(&src).unwrap();
    assert_eq!(a, b);
    let names: Vec<_> = std::fs::read_dir(store.dir()).unwrap().collect();
    assert_eq!(names.len(), 1, "one copy and no temp files");
    let hash = a.hash.to_hex();
    assert!(store.contains(&hash));
    assert!(!store.contains(&"0".repeat(64)));
    assert!(store.asset_path("../etc/passwd").is_err());

    let p1 = store.peaks(&hash, 0, None, 100).unwrap();
    assert_eq!(p1.min.len(), 100);
    assert!(p1.level_frames >= 256);
    let side = store.dir().join(format!("{hash}.peaks"));
    assert!(side.is_file());
    std::fs::write(&side, b"garbage").unwrap();
    let p2 = store.peaks(&hash, 0, None, 100).unwrap();
    assert_eq!(p1, p2, "a bad sidecar is rebuilt to the same answer");
    std::fs::remove_file(&side).unwrap();
    assert_eq!(store.peaks(&hash, 0, None, 100).unwrap(), p1);
    // Fine range reads the source, bins capped by the limit.
    let fine = store.peaks(&hash, 10, Some(50), 1000).unwrap();
    assert_eq!((fine.level_frames, fine.bins), (0, 40));
    assert!(store.peaks(&hash, 50, Some(50), 10).is_err());
    assert!(store.peaks(&hash, 0, Some(100_001), 10).is_err());
    // Provider opens streams by hash.
    let mut s = store.open(&hash).unwrap();
    assert_eq!(s.info().frames, 100_000);
    assert!(store.open(&"1".repeat(64)).is_err());

    // Not audio: nothing is left behind.
    std::fs::write(&src, b"hello world").unwrap();
    assert_eq!(store.import_file(&src).err(), Some(AudioError::NotWav));
    assert_eq!(std::fs::read_dir(store.dir()).unwrap().count(), 2);
    let mut buf = [0.0f32; 4];
    assert_eq!(s.read(99_999, &mut buf).unwrap(), 1);
    assert_eq!(s.read(100_000, &mut buf).unwrap(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
