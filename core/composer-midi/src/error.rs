use std::fmt;

/// Why a MIDI file was rejected. Offsets are byte positions in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiError {
    /// The input ended inside a header, chunk or event.
    Truncated {
        offset: usize,
    },
    /// Missing or invalid `MThd` header.
    BadHeader(&'static str),
    /// Format 2 (independent sequences) is not supported.
    UnsupportedFormat(u16),
    /// SMPTE time division is not supported.
    UnsupportedDivision,
    /// A variable length quantity longer than 4 bytes.
    BadVarLen {
        offset: usize,
    },
    /// A status byte that cannot appear in a file.
    BadStatus {
        offset: usize,
        status: u8,
    },
    /// A data byte with no running status to give it meaning.
    MissingStatus {
        offset: usize,
    },
    /// A data byte at or above 0x80 where a value was expected.
    BadData {
        offset: usize,
    },
    /// A position too large to represent after rescaling.
    TickOverflow,
    TooManyTracks,
    TooManyNotes,
    /// The file declares or holds no tracks.
    NoTracks,
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MidiError::Truncated { offset } => write!(f, "file truncated at byte {offset}"),
            MidiError::BadHeader(why) => write!(f, "bad header: {why}"),
            MidiError::UnsupportedFormat(n) => write!(f, "unsupported MIDI file format {n}"),
            MidiError::UnsupportedDivision => f.write_str("SMPTE time division is not supported"),
            MidiError::BadVarLen { offset } => {
                write!(f, "bad variable length value at byte {offset}")
            }
            MidiError::BadStatus { offset, status } => {
                write!(f, "bad status byte {status:#04x} at byte {offset}")
            }
            MidiError::MissingStatus { offset } => {
                write!(f, "data byte without running status at byte {offset}")
            }
            MidiError::BadData { offset } => write!(f, "data byte out of range at byte {offset}"),
            MidiError::TickOverflow => f.write_str("event position out of range"),
            MidiError::TooManyTracks => f.write_str("too many tracks"),
            MidiError::TooManyNotes => f.write_str("too many notes"),
            MidiError::NoTracks => f.write_str("file has no tracks"),
        }
    }
}

impl std::error::Error for MidiError {}
