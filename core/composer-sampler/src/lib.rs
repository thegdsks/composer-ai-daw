//! Readers that turn SoundFont 2 and SFZ files into an engine [`SampleBank`] (D34).
//!
//! Host side only: the browser engine receives the finished bank inside the blob. Every
//! reader treats its input as untrusted: sizes are checked before allocating, indices are
//! bounds checked, and nothing panics on malformed bytes (see the truncation tests).
#![forbid(unsafe_code)]

mod limits;
mod sf2;
mod sfz;
mod wav;

pub use composer_engine::SampleBank;
pub use limits::Limits;
pub use sf2::{load_sf2, Sf2Select};
pub use sfz::{load_sfz, load_sfz_file};
pub use wav::{decode_wav, WavSample};

/// Why a file was rejected. Codes are stable strings for clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerError {
    /// Input or a derived size is over a configured cap.
    TooLarge,
    /// A chunk or table runs past the end of the data.
    Truncated,
    /// Wrong magic, bad chunk layout or an impossible value.
    Malformed,
    /// A table index points outside its table.
    BadIndex,
    /// A loop point lies outside its sample.
    BadLoop,
    /// A valid file using a feature we do not read (for example 24 bit SF2 only).
    Unsupported,
    /// The file parsed but yields no playable zone.
    NoZones,
    /// A referenced sample file could not be read.
    MissingSample,
}

impl SamplerError {
    pub fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "sampler.too_large",
            Self::Truncated => "sampler.truncated",
            Self::Malformed => "sampler.malformed",
            Self::BadIndex => "sampler.bad_index",
            Self::BadLoop => "sampler.bad_loop",
            Self::Unsupported => "sampler.unsupported",
            Self::NoZones => "sampler.no_zones",
            Self::MissingSample => "sampler.missing_sample",
        }
    }
}

impl std::fmt::Display for SamplerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for SamplerError {}

impl From<composer_engine::BankError> for SamplerError {
    fn from(_: composer_engine::BankError) -> Self {
        Self::Malformed
    }
}

pub(crate) fn u16_at(b: &[u8], i: usize) -> Result<u16, SamplerError> {
    let s = b.get(i..i + 2).ok_or(SamplerError::Truncated)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

pub(crate) fn u32_at(b: &[u8], i: usize) -> Result<u32, SamplerError> {
    let s = b.get(i..i + 4).ok_or(SamplerError::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Walks RIFF style chunks (4 byte id, u32 size, body padded to even) with bounds checks.
pub(crate) struct Chunks<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Chunks<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<([u8; 4], &'a [u8]), SamplerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let rest = &self.data[self.pos..];
        if rest.len() < 8 {
            self.pos = self.data.len();
            return Some(Err(SamplerError::Truncated));
        }
        let id = [rest[0], rest[1], rest[2], rest[3]];
        let size = u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
        let Some(body) = rest.get(8..8 + size) else {
            self.pos = self.data.len();
            return Some(Err(SamplerError::Truncated));
        };
        self.pos += 8 + size + (size & 1);
        Some(Ok((id, body)))
    }
}
