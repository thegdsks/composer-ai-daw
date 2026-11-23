//! Bounded memory audio for big projects (D41): a chunked WAV reader, a peak cache and a
//! directory store addressed by content hash. Host side only.
//!
//! Nothing here loads a whole file. Every size read from a file is checked against
//! [`AudioLimits`] before a buffer is sized from it.
#![forbid(unsafe_code)]

mod error;
mod limits;
pub mod peaks;
mod store;
mod wav;

pub use error::AudioError;
pub use limits::AudioLimits;
pub use peaks::{Peak, PeakLevels};
pub use store::{AudioProvider, DirStore, PeakBins, Stored};
pub use wav::{AudioStream, StreamInfo, WavStream};
