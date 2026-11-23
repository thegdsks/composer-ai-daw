//! Peak cache (D41): min and max per block at several zoom levels, kept as a sidecar file.

use crate::{AudioError, AudioStream};

/// Frames per block at level 0.
pub const BASE_BLOCK: u64 = 256;
/// Each level is this many times coarser than the one before.
pub const LEVEL_FACTOR: u64 = 4;
/// Number of levels (256, 1024, 4096, 16384, 65536 frames per block).
pub const LEVEL_COUNT: usize = 5;

const MAGIC: &[u8; 4] = b"CMPK";
const VERSION: u16 = 1;
const HEADER_LEN: usize = 4 + 2 + 2 + 8 + 4 + 4;
/// Frames decoded per read while building.
const BUILD_CHUNK_FRAMES: usize = 16_384;

/// Frames per block of a level.
pub const fn block_frames(level: usize) -> u64 {
    BASE_BLOCK * LEVEL_FACTOR.pow(level as u32)
}

fn blocks_in(frames: u64, level: usize) -> u64 {
    frames.div_ceil(block_frames(level))
}

/// Min and max of one block, scaled to i16 (full scale is 32767).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peak {
    pub min: i16,
    pub max: i16,
}

impl Peak {
    const EMPTY: Peak = Peak {
        min: i16::MAX,
        max: i16::MIN,
    };

    fn add(&mut self, other: Peak) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

fn quantize(v: f32) -> i16 {
    // NaN compares false and falls to the lower arm, 0.
    if v >= 1.0 {
        i16::MAX
    } else if v <= -1.0 {
        -i16::MAX
    } else if v > -1.0 && v < 1.0 {
        (v * 32767.0).round() as i16
    } else {
        0
    }
}

/// All levels of one asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeakLevels {
    pub frames: u64,
    pub channels: u16,
    levels: Vec<Vec<Peak>>,
}

impl PeakLevels {
    /// One streaming pass over the source: bounded memory apart from the levels themselves.
    pub fn build(stream: &mut dyn AudioStream) -> Result<Self, AudioError> {
        let info = stream.info();
        let ch = usize::from(info.channels);
        let mut buf = vec![0.0f32; BUILD_CHUNK_FRAMES * ch];
        let mut base: Vec<Peak> = Vec::with_capacity(blocks_in(info.frames, 0) as usize);
        let mut cur = Peak::EMPTY;
        let mut in_block = 0u64;
        let mut at = 0u64;
        while at < info.frames {
            let n = stream.read(at, &mut buf)?;
            if n == 0 {
                return Err(AudioError::Truncated);
            }
            for frame in buf[..n * ch].chunks_exact(ch) {
                for &s in frame {
                    let q = quantize(s);
                    cur.min = cur.min.min(q);
                    cur.max = cur.max.max(q);
                }
                in_block += 1;
                if in_block == BASE_BLOCK {
                    base.push(cur);
                    cur = Peak::EMPTY;
                    in_block = 0;
                }
            }
            at += n as u64;
        }
        if in_block > 0 {
            base.push(cur);
        }
        let mut levels = vec![base];
        for l in 1..LEVEL_COUNT {
            let prev = &levels[l - 1];
            let next: Vec<Peak> = prev
                .chunks(LEVEL_FACTOR as usize)
                .map(|c| {
                    let mut p = Peak::EMPTY;
                    for q in c {
                        p.add(*q);
                    }
                    p
                })
                .collect();
            levels.push(next);
        }
        Ok(Self {
            frames: info.frames,
            channels: info.channels,
            levels,
        })
    }

    pub fn level(&self, level: usize) -> &[Peak] {
        self.levels.get(level).map_or(&[], Vec::as_slice)
    }

    /// Sidecar bytes.
    pub fn encode(&self) -> Vec<u8> {
        let total: usize = self.levels.iter().map(Vec::len).sum();
        let mut out = Vec::with_capacity(HEADER_LEN + total * 4);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&self.frames.to_le_bytes());
        out.extend_from_slice(&(BASE_BLOCK as u32).to_le_bytes());
        out.extend_from_slice(&(LEVEL_COUNT as u32).to_le_bytes());
        for lv in &self.levels {
            for p in lv {
                out.extend_from_slice(&p.min.to_le_bytes());
                out.extend_from_slice(&p.max.to_le_bytes());
            }
        }
        out
    }

    /// Parses a sidecar. `frames` and `channels` are what the source says; a sidecar that
    /// disagrees, or whose size is not exactly what its header implies, is rejected so the
    /// caller rebuilds it.
    pub fn decode(bytes: &[u8], frames: u64, channels: u16) -> Result<Self, AudioError> {
        let bad = AudioError::Malformed("peak sidecar");
        if bytes.len() < HEADER_LEN || &bytes[0..4] != MAGIC {
            return Err(bad);
        }
        let ver = u16::from_le_bytes([bytes[4], bytes[5]]);
        let ch = u16::from_le_bytes([bytes[6], bytes[7]]);
        let fr = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| bad.clone())?);
        let base = u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| bad.clone())?);
        let count = u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| bad.clone())?);
        if ver != VERSION
            || ch != channels
            || fr != frames
            || u64::from(base) != BASE_BLOCK
            || count as usize != LEVEL_COUNT
        {
            return Err(bad);
        }
        let want: u64 = (0..LEVEL_COUNT).map(|l| blocks_in(frames, l)).sum();
        if (bytes.len() - HEADER_LEN) as u64 != want * 4 {
            return Err(bad);
        }
        let mut at = HEADER_LEN;
        let mut levels = Vec::with_capacity(LEVEL_COUNT);
        for l in 0..LEVEL_COUNT {
            let n = blocks_in(frames, l) as usize;
            let mut v = Vec::with_capacity(n);
            for c in bytes[at..at + n * 4].as_chunks::<4>().0.iter() {
                v.push(Peak {
                    min: i16::from_le_bytes([c[0], c[1]]),
                    max: i16::from_le_bytes([c[2], c[3]]),
                });
            }
            at += n * 4;
            levels.push(v);
        }
        Ok(Self {
            frames,
            channels,
            levels,
        })
    }

    /// Min and max per bin over `from..to` (frames), using the coarsest level that is not
    /// coarser than a bin. Returns None when a bin is finer than the base level; the caller
    /// then reads the source (see [`bins_from_source`]).
    pub fn bins(&self, from: u64, to: u64, bins: u32) -> Option<(usize, Vec<Peak>)> {
        let span = to.checked_sub(from)?;
        if bins == 0 || span == 0 || to > self.frames {
            return None;
        }
        let bin_frames = span / u64::from(bins);
        if bin_frames < BASE_BLOCK {
            return None;
        }
        let level = (0..LEVEL_COUNT)
            .rev()
            .find(|l| block_frames(*l) <= bin_frames)?;
        let bs = block_frames(level);
        let lv = &self.levels[level];
        let mut out = Vec::with_capacity(bins as usize);
        for i in 0..u64::from(bins) {
            let a = from + (u128::from(span) * u128::from(i) / u128::from(bins)) as u64;
            let b = from + (u128::from(span) * u128::from(i + 1) / u128::from(bins)) as u64;
            let first = (a / bs) as usize;
            let last = (b.div_ceil(bs) as usize).min(lv.len());
            let mut p = Peak::EMPTY;
            for q in &lv[first.min(last)..last] {
                p.add(*q);
            }
            if p == Peak::EMPTY {
                p = Peak { min: 0, max: 0 };
            }
            out.push(p);
        }
        Some((bs as usize, out))
    }
}

/// Min and max per bin computed straight from the source, for ranges finer than the cache.
/// Reads only `from..to`, in bounded chunks.
pub fn bins_from_source(
    stream: &mut dyn AudioStream,
    from: u64,
    to: u64,
    bins: u32,
) -> Result<Vec<Peak>, AudioError> {
    let info = stream.info();
    if to <= from || to > info.frames || bins == 0 {
        return Err(AudioError::BadRequest("range"));
    }
    let span = to - from;
    let ch = usize::from(info.channels);
    let mut buf = vec![0.0f32; BUILD_CHUNK_FRAMES * ch];
    let mut out = vec![Peak::EMPTY; bins as usize];
    let mut at = from;
    while at < to {
        let want = ((to - at) as usize).min(BUILD_CHUNK_FRAMES);
        let n = stream.read(at, &mut buf[..want * ch])?;
        if n == 0 {
            return Err(AudioError::Truncated);
        }
        for (k, frame) in buf[..n * ch].chunks_exact(ch).enumerate() {
            let rel = at + k as u64 - from;
            let bin = (u128::from(rel) * u128::from(bins) / u128::from(span)) as usize;
            let slot = &mut out[bin.min(bins as usize - 1)];
            for &s in frame {
                let q = quantize(s);
                slot.min = slot.min.min(q);
                slot.max = slot.max.max(q);
            }
        }
        at += n as u64;
    }
    for p in &mut out {
        if *p == Peak::EMPTY {
            *p = Peak { min: 0, max: 0 };
        }
    }
    Ok(out)
}
