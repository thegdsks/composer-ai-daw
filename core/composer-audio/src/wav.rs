//! Chunked WAV reader (D41): only the header is parsed up front, frames are decoded on request.

use crate::{AudioError, AudioLimits};
use std::io::{Read, Seek, SeekFrom};

/// Shape of an opened stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
}

/// A source of decoded audio that is read in chunks. Reads may start anywhere; reading
/// sequentially is the cheap case.
pub trait AudioStream: Send {
    fn info(&self) -> StreamInfo;
    /// Decodes up to `out.len() / channels` frames starting at frame `start` into `out`
    /// (interleaved f32). Returns the number of frames written, fewer than asked only at
    /// the end of the stream.
    fn read(&mut self, start: u64, out: &mut [f32]) -> Result<usize, AudioError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sample {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
}

/// Most chunk headers walked before giving up on finding `data`.
const MAX_CHUNKS: u32 = 4096;
/// Frames decoded per inner read, which bounds the byte scratch buffer.
const SCRATCH_FRAMES: usize = 8192;

pub struct WavStream<R: Read + Seek> {
    r: R,
    info: StreamInfo,
    kind: Sample,
    bytes_per_frame: u64,
    data_start: u64,
    /// Frame the reader is positioned at.
    pos: u64,
    scratch: Vec<u8>,
}

fn u16_le(b: &[u8], at: usize) -> Result<u16, AudioError> {
    let s = b.get(at..at + 2).ok_or(AudioError::Truncated)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_le(b: &[u8], at: usize) -> Result<u32, AudioError> {
    let s = b.get(at..at + 4).ok_or(AudioError::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

impl<R: Read + Seek + Send> WavStream<R> {
    /// Parses the header. Reads a few hundred bytes at most; nothing is sized from the file
    /// before the caps are checked.
    pub fn open(mut r: R, limits: &AudioLimits) -> Result<Self, AudioError> {
        let len = r.seek(SeekFrom::End(0))?;
        if len > limits.max_file_bytes {
            return Err(AudioError::TooLarge("file bytes"));
        }
        r.seek(SeekFrom::Start(0))?;
        let mut head = [0u8; 12];
        read_exact_or(&mut r, &mut head, AudioError::NotWav)?;
        match &head[0..4] {
            b"RIFF" => {}
            b"RF64" | b"BW64" => return Err(AudioError::Unsupported("RF64 files")),
            _ => return Err(AudioError::NotWav),
        }
        if &head[8..12] != b"WAVE" {
            return Err(AudioError::NotWav);
        }
        let mut fmt: Option<(Sample, u16, u32, u64)> = None;
        let mut pos = 12u64;
        for _ in 0..MAX_CHUNKS {
            if pos == len {
                break;
            }
            let mut ch = [0u8; 8];
            r.seek(SeekFrom::Start(pos))?;
            read_exact_or(&mut r, &mut ch, AudioError::Truncated)?;
            let size = u64::from(u32_le(&ch, 4)?);
            let body = pos + 8;
            match &ch[0..4] {
                b"fmt " => {
                    if !(16..=256).contains(&size) || body + size > len {
                        return Err(AudioError::Malformed("fmt chunk size"));
                    }
                    let mut buf = [0u8; 256];
                    let n = size as usize;
                    read_exact_or(&mut r, &mut buf[..n], AudioError::Truncated)?;
                    fmt = Some(parse_fmt(&buf[..n], limits)?);
                }
                b"data" => {
                    let (kind, channels, rate, align) =
                        fmt.ok_or(AudioError::Malformed("data before fmt"))?;
                    let avail = len - body;
                    // Writers that stream leave the size at 0 or all ones.
                    let size = if size == 0 || size == 0xFFFF_FFFF {
                        avail
                    } else if size > avail {
                        return Err(AudioError::Truncated);
                    } else {
                        size
                    };
                    let frames = size / align;
                    if frames == 0 {
                        return Err(AudioError::Malformed("no audio frames"));
                    }
                    if frames > limits.max_frames {
                        return Err(AudioError::TooLarge("frames"));
                    }
                    return Ok(Self {
                        r,
                        info: StreamInfo {
                            sample_rate: rate,
                            channels,
                            frames,
                        },
                        kind,
                        bytes_per_frame: align,
                        data_start: body,
                        pos: 0,
                        scratch: Vec::new(),
                    });
                }
                _ => {}
            }
            // Chunks are padded to an even size.
            let next = body
                .checked_add(size)
                .and_then(|v| v.checked_add(size & 1))
                .ok_or(AudioError::Malformed("chunk size"))?;
            if next > len {
                return Err(AudioError::Truncated);
            }
            pos = next;
        }
        if fmt.is_none() {
            return Err(AudioError::Malformed("no fmt chunk"));
        }
        Err(AudioError::Malformed("no data chunk"))
    }
}

fn read_exact_or<R: Read>(r: &mut R, buf: &mut [u8], on_eof: AudioError) -> Result<(), AudioError> {
    match r.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(on_eof),
        Err(e) => Err(e.into()),
    }
}

fn parse_fmt(b: &[u8], limits: &AudioLimits) -> Result<(Sample, u16, u32, u64), AudioError> {
    let mut tag = u16_le(b, 0)?;
    let channels = u16_le(b, 2)?;
    let rate = u32_le(b, 4)?;
    let align = u16_le(b, 12)?;
    let bits = u16_le(b, 14)?;
    if tag == 0xFFFE {
        // Extensible: the real format tag opens the sub format GUID.
        tag = u16_le(b, 24)?;
    }
    if channels == 0 || channels > limits.max_channels {
        return Err(AudioError::Unsupported("channel count"));
    }
    if rate == 0 || rate > 768_000 {
        return Err(AudioError::Malformed("sample rate"));
    }
    let kind = match (tag, bits) {
        (1, 8) => Sample::Pcm8,
        (1, 16) => Sample::Pcm16,
        (1, 24) => Sample::Pcm24,
        (1, 32) => Sample::Pcm32,
        (3, 32) => Sample::Float32,
        _ => return Err(AudioError::Unsupported("sample format")),
    };
    let width = u64::from(bits / 8);
    if u64::from(align) != width * u64::from(channels) {
        return Err(AudioError::Malformed("block align"));
    }
    Ok((kind, channels, rate, u64::from(align)))
}

fn decode(kind: Sample, raw: &[u8]) -> f32 {
    match kind {
        Sample::Pcm8 => (f32::from(raw[0]) - 128.0) / 128.0,
        Sample::Pcm16 => f32::from(i16::from_le_bytes([raw[0], raw[1]])) / 32768.0,
        Sample::Pcm24 => {
            (i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8) as f32 / 8_388_608.0
        }
        Sample::Pcm32 => {
            (f64::from(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])) / 2_147_483_648.0)
                as f32
        }
        Sample::Float32 => {
            let x = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            if x.is_finite() {
                x
            } else {
                0.0
            }
        }
    }
}

impl<R: Read + Seek + Send> AudioStream for WavStream<R> {
    fn info(&self) -> StreamInfo {
        self.info
    }

    fn read(&mut self, start: u64, out: &mut [f32]) -> Result<usize, AudioError> {
        let ch = usize::from(self.info.channels);
        let want = (out.len() / ch) as u64;
        if start >= self.info.frames || want == 0 {
            return Ok(0);
        }
        let total = want.min(self.info.frames - start) as usize;
        if self.pos != start {
            let at = start
                .checked_mul(self.bytes_per_frame)
                .and_then(|v| v.checked_add(self.data_start))
                .ok_or(AudioError::Malformed("offset"))?;
            self.r.seek(SeekFrom::Start(at))?;
            self.pos = start;
        }
        let width = (self.bytes_per_frame as usize) / ch;
        let mut done = 0usize;
        while done < total {
            let n = (total - done).min(SCRATCH_FRAMES);
            let bytes = n * self.bytes_per_frame as usize;
            self.scratch.resize(bytes, 0);
            if let Err(e) = self.r.read_exact(&mut self.scratch) {
                // The position is unknown after a failed read, so force a seek next time.
                self.pos = u64::MAX;
                return Err(e.into());
            }
            let dst = &mut out[done * ch..(done + n) * ch];
            for (o, raw) in dst.iter_mut().zip(self.scratch.chunks_exact(width)) {
                *o = decode(self.kind, raw);
            }
            done += n;
        }
        self.pos = start + total as u64;
        Ok(total)
    }
}
