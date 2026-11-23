#![forbid(unsafe_code)]
//! Standard MIDI File import and export, with no dependencies beyond the
//! workspace crates.
//!
//! Import turns bytes into an [`ImportPlan`]: a list of [`composer_commands::Command`]
//! that a client applies as one group, so the import is one undo step and works
//! the same from the UI, CLI and MCP. Export writes a format 1 file at 960 PPQ.
//!
//! Rounding: file ticks are rescaled to 960 per quarter as `round_half_up(t * 960 / ppq)`
//! in exact integer arithmetic, on absolute positions (never on deltas), so error
//! never accumulates. A note duration is the difference of its two rescaled ends,
//! at least 1 tick. Tempo is converted between microseconds per quarter and milli
//! beats per minute with the same round half up rule, and clamped to the model range.
//!
//! Export channels: the k-th instrument track uses channel k mod 15, counted over
//! 0 to 8 then 10 to 15, so channel 10 (index 9, drums) is never used. Audio
//! tracks are skipped. Positions above 0x0FFFFFFF ticks are clamped.

mod error;
mod export;
mod import;
mod reader;

pub use error::MidiError;
pub use export::export_smf;
pub use import::{import_smf, ImportOptions, ImportPlan, ImportStats};

#[cfg(test)]
mod tests;
