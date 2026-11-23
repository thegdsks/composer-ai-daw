//! Minimal WAV decoder for SFZ samples (see notes/ref-sampler-formats.yaml, wav).

use crate::{u16_at, u32_at, Chunks, Limits, SamplerError};

/// Decoded audio plus the first stored loop, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct WavSample {
    pub rate: u32,
    pub channels: u8,
    /// Interleaved.
    pub frames: Vec<f32>,
    /// Loop start and first frame after the loop, from the smpl chunk.
    pub loop_points: Option<(u32, u32)>,
}

impl WavSample {
    pub fn len_frames(&self) -> usize {
        self.frames.len() / usize::from(self.channels.max(1))
    }
}

/// Decodes PCM 8, 16, 24, 32 bit and float32, mono or stereo.
pub fn decode_wav(bytes: &[u8], limits: &Limits) -> Result<WavSample, SamplerError> {
    if bytes.len() > limits.max_file_bytes {
        return Err(SamplerError::TooLarge);
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SamplerError::Malformed);
    }
    let mut fmt: Option<(u16, u8, u32, u16, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut smpl: Option<&[u8]> = None;
    for chunk in Chunks::new(&bytes[12..]) {
        // A chunk cut short is a truncated file, not something to play half of.
        let (id, body) = chunk?;
        match &id {
            b"fmt " => {
                if body.len() < 16 {
                    return Err(SamplerError::Truncated);
                }
                let mut tag = u16_at(body, 0)?;
                let channels = u16_at(body, 2)?;
                let rate = u32_at(body, 4)?;
                let align = u16_at(body, 12)?;
                let bits = u16_at(body, 14)?;
                if tag == 0xFFFE {
                    // Extensible: the real format tag opens the sub format GUID.
                    tag = u16_at(body, 24)?;
                }
                if channels == 0 || channels > 2 {
                    return Err(SamplerError::Unsupported);
                }
                fmt = Some((tag, channels as u8, rate, align, bits));
            }
            b"data" => data = Some(body),
            b"smpl" => smpl = Some(body),
            _ => {}
        }
    }
    let (tag, channels, rate, align, bits) = fmt.ok_or(SamplerError::Malformed)?;
    let data = data.ok_or(SamplerError::Malformed)?;
    if rate == 0 || rate > 768_000 {
        return Err(SamplerError::Malformed);
    }
    let bytes_per = usize::from(bits / 8);
    if !matches!((tag, bits), (1, 8 | 16 | 24 | 32) | (3, 32)) {
        return Err(SamplerError::Unsupported);
    }
    if usize::from(align) != bytes_per * usize::from(channels) {
        return Err(SamplerError::Malformed);
    }
    let n_frames = data.len() / usize::from(align);
    if n_frames == 0 {
        return Err(SamplerError::Malformed);
    }
    if n_frames > limits.max_sample_frames {
        return Err(SamplerError::TooLarge);
    }
    let mut frames = Vec::with_capacity(n_frames * usize::from(channels));
    for raw in data[..n_frames * usize::from(align)].chunks_exact(bytes_per) {
        let v = match (tag, bits) {
            (1, 8) => (f32::from(raw[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([raw[0], raw[1]])) / 32768.0,
            (1, 24) => {
                let x = i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8;
                x as f32 / 8_388_608.0
            }
            (1, _) => {
                let x = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                (f64::from(x) / 2_147_483_648.0) as f32
            }
            _ => {
                let x = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                if x.is_finite() {
                    x
                } else {
                    0.0
                }
            }
        };
        frames.push(v);
    }
    let loop_points = smpl.and_then(|s| first_loop(s, n_frames));
    Ok(WavSample {
        rate,
        channels,
        frames,
        loop_points,
    })
}

fn first_loop(smpl: &[u8], n_frames: usize) -> Option<(u32, u32)> {
    let count = u32_at(smpl, 28).ok()?;
    if count == 0 {
        return None;
    }
    let start = u32_at(smpl, 36 + 8).ok()?;
    let end = u32_at(smpl, 36 + 12).ok()?;
    // The stored end is the last looped frame, ours is the first frame after it.
    let end = end.checked_add(1)?;
    (start < end && (end as usize) <= n_frames).then_some((start, end))
}
