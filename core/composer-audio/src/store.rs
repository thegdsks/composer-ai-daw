//! Directory backed asset store (D41, D43): files named by content hash, peak sidecars next to them.
//! This is the only file code in the crate.

use crate::peaks::{bins_from_source, PeakLevels};
use crate::{AudioError, AudioLimits, AudioStream, StreamInfo, WavStream};
use composer_format::sha256::Sha256;
use composer_format::ContentHash;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Opens audio by content hash. The seam between the render and wherever bytes live.
pub trait AudioProvider: Send + Sync + std::fmt::Debug {
    fn open(&self, hash: &str) -> Result<Box<dyn AudioStream>, AudioError>;
}

/// A freshly stored asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub hash: ContentHash,
    pub bytes: u64,
    pub info: StreamInfo,
}

/// Peak query result: `min` and `max` per bin in -1..1, and the cache block size used
/// (0 when the source was read directly).
#[derive(Debug, Clone, PartialEq)]
pub struct PeakBins {
    pub from: u64,
    pub to: u64,
    pub bins: u32,
    pub level_frames: u64,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct DirStore {
    dir: PathBuf,
    limits: AudioLimits,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const COPY_BUF: usize = 1 << 20;

fn valid_hash(hash: &str) -> Result<ContentHash, AudioError> {
    ContentHash::from_hex(hash).ok_or(AudioError::BadRequest("asset hash"))
}

impl DirStore {
    pub fn new(dir: impl Into<PathBuf>, limits: AudioLimits) -> Self {
        Self {
            dir: dir.into(),
            limits,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn limits(&self) -> &AudioLimits {
        &self.limits
    }

    /// Path of an asset file. The name comes only from a validated hash.
    pub fn asset_path(&self, hash: &str) -> Result<PathBuf, AudioError> {
        let h = valid_hash(hash)?;
        Ok(self.dir.join(format!("{}.wav", h.to_hex())))
    }

    fn peaks_path(&self, hash: &str) -> Result<PathBuf, AudioError> {
        let h = valid_hash(hash)?;
        Ok(self.dir.join(format!("{}.peaks", h.to_hex())))
    }

    pub fn contains(&self, hash: &str) -> bool {
        self.asset_path(hash).is_ok_and(|p| p.is_file())
    }

    /// Opens the stored file as a stream.
    pub fn open_wav(&self, hash: &str) -> Result<WavStream<File>, AudioError> {
        let f = File::open(self.asset_path(hash)?)?;
        WavStream::open(f, &self.limits)
    }

    /// Hashes and copies a file into the store with bounded buffers, then checks that it
    /// decodes. Storing the same bytes again returns the same hash and keeps one copy.
    pub fn import_file(&self, src: &Path) -> Result<Stored, AudioError> {
        let mut f = File::open(src)?;
        let len = f.metadata()?.len();
        if len > self.limits.max_file_bytes {
            return Err(AudioError::TooLarge("file bytes"));
        }
        fs::create_dir_all(&self.dir)?;
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = self
            .dir
            .join(format!("import-{}-{n}.tmp", std::process::id()));
        let result = (|| {
            let mut out = File::create(&tmp)?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; COPY_BUF];
            let mut total = 0u64;
            loop {
                let got = f.read(&mut buf)?;
                if got == 0 {
                    break;
                }
                total += got as u64;
                if total > self.limits.max_file_bytes {
                    return Err(AudioError::TooLarge("file bytes"));
                }
                hasher.update(&buf[..got]);
                out.write_all(&buf[..got])?;
            }
            out.sync_all()?;
            let hash = ContentHash(hasher.finish());
            let info = WavStream::open(File::open(&tmp)?, &self.limits)?.info();
            Ok(Stored {
                hash,
                bytes: total,
                info,
            })
        })();
        match result {
            Ok(stored) => {
                let dest = self.dir.join(format!("{}.wav", stored.hash.to_hex()));
                if dest.exists() {
                    let _ = fs::remove_file(&tmp);
                } else if let Err(e) = fs::rename(&tmp, &dest) {
                    let _ = fs::remove_file(&tmp);
                    return Err(e.into());
                }
                Ok(stored)
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Peak levels from the sidecar, rebuilding and rewriting it when it is missing or invalid.
    pub fn peak_levels(&self, hash: &str) -> Result<PeakLevels, AudioError> {
        let mut wav = self.open_wav(hash)?;
        let info = wav.info();
        let side = self.peaks_path(hash)?;
        if let Ok(bytes) = fs::read(&side) {
            if let Ok(levels) = PeakLevels::decode(&bytes, info.frames, info.channels) {
                return Ok(levels);
            }
        }
        let levels = PeakLevels::build(&mut wav)?;
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = self
            .dir
            .join(format!("peaks-{}-{n}.tmp", std::process::id()));
        // A sidecar that cannot be written only costs a rebuild next time.
        if fs::write(&tmp, levels.encode()).is_ok() && fs::rename(&tmp, &side).is_err() {
            let _ = fs::remove_file(&tmp);
        }
        Ok(levels)
    }

    /// Min and max per bin over `from..to` frames (to defaults to the end), bins capped by
    /// the configured limit.
    pub fn peaks(
        &self,
        hash: &str,
        from: u64,
        to: Option<u64>,
        bins: u32,
    ) -> Result<PeakBins, AudioError> {
        let mut wav = self.open_wav(hash)?;
        let frames = wav.info().frames;
        let to = to.unwrap_or(frames);
        if from >= to || to > frames {
            return Err(AudioError::BadRequest("range"));
        }
        let bins = bins.clamp(1, self.limits.max_peak_bins);
        let bins = u32::try_from(u64::from(bins).min(to - from)).unwrap_or(bins);
        let cached = self.peak_levels(hash)?.bins(from, to, bins);
        let (level_frames, peaks) = match cached {
            Some((bs, p)) => (bs as u64, p),
            None => (0, bins_from_source(&mut wav, from, to, bins)?),
        };
        let scale = |v: i16| f32::from(v) / 32767.0;
        Ok(PeakBins {
            from,
            to,
            bins,
            level_frames,
            min: peaks.iter().map(|p| scale(p.min)).collect(),
            max: peaks.iter().map(|p| scale(p.max)).collect(),
        })
    }

    pub fn into_provider(self) -> Arc<dyn AudioProvider> {
        Arc::new(self)
    }
}

impl AudioProvider for DirStore {
    fn open(&self, hash: &str) -> Result<Box<dyn AudioStream>, AudioError> {
        Ok(Box::new(self.open_wav(hash)?))
    }
}
