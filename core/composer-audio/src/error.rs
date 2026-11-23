use std::fmt;

/// Why audio could not be opened, read or stored. Each variant has a stable [`AudioError::code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    Io(String),
    /// The bytes are not a RIFF WAVE file.
    NotWav,
    /// The file ends before the structure it announces.
    Truncated,
    /// A valid file of a kind this build does not read (format tag, bit depth, RF64).
    Unsupported(&'static str),
    /// A size or count is over a configured cap.
    TooLarge(&'static str),
    /// The header contradicts itself.
    Malformed(&'static str),
    /// No such asset in the store.
    NotFound,
    /// The asked range or hash is not valid.
    BadRequest(&'static str),
}

impl AudioError {
    pub fn code(&self) -> &'static str {
        match self {
            AudioError::Io(_) => "io_error",
            AudioError::NotWav => "not_wav",
            AudioError::Truncated => "truncated",
            AudioError::Unsupported(_) => "unsupported",
            AudioError::TooLarge(_) => "too_large",
            AudioError::Malformed(_) => "malformed",
            AudioError::NotFound => "asset_missing",
            AudioError::BadRequest(_) => "bad_request",
        }
    }
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::Io(e) => write!(f, "i/o error: {e}"),
            AudioError::NotWav => f.write_str("not a RIFF WAVE file"),
            AudioError::Truncated => f.write_str("the file ends early"),
            AudioError::Unsupported(w) => write!(f, "unsupported: {w}"),
            AudioError::TooLarge(w) => write!(f, "too large: {w}"),
            AudioError::Malformed(w) => write!(f, "malformed: {w}"),
            AudioError::NotFound => f.write_str("the asset file is not in the store"),
            AudioError::BadRequest(w) => write!(f, "bad request: {w}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<std::io::Error> for AudioError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::UnexpectedEof => AudioError::Truncated,
            std::io::ErrorKind::NotFound => AudioError::NotFound,
            _ => AudioError::Io(e.to_string()),
        }
    }
}
