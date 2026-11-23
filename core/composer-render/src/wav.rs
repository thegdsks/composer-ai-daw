//! Minimal RIFF/WAVE writer and reader: 16 and 24 bit PCM and 32 bit float.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavFormat {
    Pcm16,
    Pcm24,
    Float32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wav {
    pub format: WavFormat,
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples in -1.0..=1.0 (float files may exceed it).
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavError {
    NotRiff,
    Truncated,
    MissingChunk,
    Unsupported,
    BadHeader,
}

/// Encodes interleaved samples. PCM clamps to full scale and rounds
/// `x * (2^(bits-1) - 1)` to the nearest integer; floats are stored as is.
pub fn write_wav(format: WavFormat, channels: u16, sample_rate: u32, samples: &[f32]) -> Vec<u8> {
    let (bytes, tag, bits) = match format {
        WavFormat::Pcm16 => (2usize, 1u16, 16u16),
        WavFormat::Pcm24 => (3, 1, 24),
        WavFormat::Float32 => (4, 3, 32),
    };
    let data_len = samples.len() * bytes;
    let mut out = Vec::with_capacity(44 + data_len + 1);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len + data_len % 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    let block = channels as usize * bytes;
    out.extend_from_slice(&(sample_rate * block as u32).to_le_bytes());
    out.extend_from_slice(&(block as u16).to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &s in samples {
        match format {
            WavFormat::Pcm16 => {
                let v = (s.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
                out.extend_from_slice(&v.to_le_bytes());
            }
            WavFormat::Pcm24 => {
                let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                out.extend_from_slice(&v.to_le_bytes()[..3]);
            }
            WavFormat::Float32 => out.extend_from_slice(&s.to_le_bytes()),
        }
    }
    if data_len % 2 == 1 {
        out.push(0);
    }
    out
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Decodes PCM16, PCM24 and float32 files. PCM divides by `2^(bits-1)`.
pub fn read_wav(bytes: &[u8]) -> Result<Wav, WavError> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotRiff);
    }
    let mut pos = 12;
    let mut fmt: Option<(WavFormat, u16, u32)> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(bytes, pos + 4).ok_or(WavError::Truncated)? as usize;
        let body = pos + 8;
        let end = body.checked_add(len).ok_or(WavError::Truncated)?;
        if end > bytes.len() {
            return Err(WavError::Truncated);
        }
        if id == b"fmt " {
            if len < 16 {
                return Err(WavError::BadHeader);
            }
            let tag = u16_at(bytes, body).ok_or(WavError::Truncated)?;
            let channels = u16_at(bytes, body + 2).ok_or(WavError::Truncated)?;
            let rate = u32_at(bytes, body + 4).ok_or(WavError::Truncated)?;
            let bits = u16_at(bytes, body + 14).ok_or(WavError::Truncated)?;
            let format = match (tag, bits) {
                (1, 16) => WavFormat::Pcm16,
                (1, 24) => WavFormat::Pcm24,
                (3, 32) => WavFormat::Float32,
                _ => return Err(WavError::Unsupported),
            };
            if channels == 0 {
                return Err(WavError::BadHeader);
            }
            fmt = Some((format, channels, rate));
        } else if id == b"data" {
            let (format, channels, sample_rate) = fmt.ok_or(WavError::MissingChunk)?;
            let data = &bytes[body..end];
            let samples = match format {
                WavFormat::Pcm16 => data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| f32::from(i16::from_le_bytes(*c)) / 32_768.0)
                    .collect(),
                WavFormat::Pcm24 => data
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| {
                        let v = i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8;
                        v as f32 / 8_388_608.0
                    })
                    .collect(),
                WavFormat::Float32 => data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| f32::from_le_bytes(*c))
                    .collect(),
            };
            return Ok(Wav {
                format,
                channels,
                sample_rate,
                samples,
            });
        }
        pos = end + len % 2;
    }
    Err(WavError::MissingChunk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Vec<f32> {
        (0..200).map(|i| (i as f32 / 100.0 - 1.0) * 0.99).collect()
    }

    #[test]
    fn float_round_trip_is_exact() {
        let mut s = ramp();
        s.push(1.5);
        let w = read_wav(&write_wav(WavFormat::Float32, 2, 48_000, &s)).unwrap();
        assert_eq!(w.samples, s);
        assert_eq!((w.channels, w.sample_rate), (2, 48_000));
    }

    #[test]
    fn pcm_round_trip_is_within_one_step() {
        for (f, step) in [
            (WavFormat::Pcm16, 1.0 / 32_768.0),
            (WavFormat::Pcm24, 1.0 / 8_388_608.0),
        ] {
            let s = ramp();
            let w = read_wav(&write_wav(f, 1, 44_100, &s)).unwrap();
            assert_eq!(w.format, f);
            assert_eq!(w.samples.len(), s.len());
            for (a, b) in s.iter().zip(&w.samples) {
                assert!((a - b).abs() <= step * 2.0, "{a} {b}");
            }
        }
    }

    #[test]
    fn pcm_clamps_overs_and_odd_length_pads() {
        let bytes = write_wav(WavFormat::Pcm24, 1, 8_000, &[2.0, -2.0, 0.0]);
        assert_eq!(bytes.len() % 2, 0);
        let w = read_wav(&bytes).unwrap();
        assert!(w.samples[0] > 0.999 && w.samples[1] < -0.999 && w.samples[2] == 0.0);
    }

    #[test]
    fn malformed_input_errors_without_panic() {
        assert_eq!(read_wav(b"nope"), Err(WavError::NotRiff));
        let good = write_wav(WavFormat::Pcm16, 1, 8_000, &[0.1, 0.2]);
        assert!(read_wav(&good[..30]).is_err());
        assert_eq!(read_wav(&good[..12]), Err(WavError::MissingChunk));
    }
}
