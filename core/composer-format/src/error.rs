use crate::container::Tag;
use composer_model::Id;
use std::fmt;

/// Why a file was rejected. Nothing is returned partially: a failed read yields no project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The first bytes are not the composer magic.
    BadMagic,
    /// The input ends before the structure it announces.
    Truncated {
        what: &'static str,
    },
    /// The file was written by a newer format version than this build reads.
    UnsupportedVersion {
        found: u16,
        supported: u16,
    },
    /// The header holds a value this version does not allow.
    BadHeader(&'static str),
    /// The manifest (header and section table) fails its checksum.
    ManifestCorrupt,
    /// The section table is internally inconsistent.
    BadLayout(&'static str),
    /// One section fails its checksum. Other sections may be fine.
    SectionCorrupt {
        tag: Tag,
    },
    MissingSection {
        tag: Tag,
    },
    DuplicateSection {
        tag: Tag,
    },
    /// A section this build does not know is marked required.
    UnknownRequiredSection {
        tag: Tag,
    },
    /// Bytes remain after the last section.
    TrailingBytes,
    /// A section is larger than its limit.
    SectionTooLarge {
        tag: Tag,
        len: u64,
        limit: u64,
    },
    /// A count or total exceeds a configured limit.
    LimitExceeded {
        what: &'static str,
        limit: u64,
        found: u64,
    },
    /// A section passed its checksum but its content does not parse.
    Malformed {
        tag: Tag,
        reason: String,
    },
    DuplicateId(Id),
    /// A key or reference names something that is not there.
    DanglingReference(&'static str),
    OutOfRange(&'static str),
    /// Content that parses but contradicts itself (for example a note in an audio region).
    Inconsistent(&'static str),
    /// No upgrade step exists, or a step failed.
    MigrationFailed {
        from: u16,
        reason: String,
    },
}

impl FormatError {
    /// Stable machine readable code for CLI and MCP clients.
    pub fn code(&self) -> &'static str {
        match self {
            FormatError::BadMagic => "bad_magic",
            FormatError::Truncated { .. } => "truncated",
            FormatError::UnsupportedVersion { .. } => "unsupported_version",
            FormatError::BadHeader(_) => "bad_header",
            FormatError::ManifestCorrupt => "manifest_corrupt",
            FormatError::BadLayout(_) => "bad_layout",
            FormatError::SectionCorrupt { .. } => "section_corrupt",
            FormatError::MissingSection { .. } => "missing_section",
            FormatError::DuplicateSection { .. } => "duplicate_section",
            FormatError::UnknownRequiredSection { .. } => "unknown_required_section",
            FormatError::TrailingBytes => "trailing_bytes",
            FormatError::SectionTooLarge { .. } => "section_too_large",
            FormatError::LimitExceeded { .. } => "limit_exceeded",
            FormatError::Malformed { .. } => "malformed",
            FormatError::DuplicateId(_) => "duplicate_id",
            FormatError::DanglingReference(_) => "dangling_reference",
            FormatError::OutOfRange(_) => "out_of_range",
            FormatError::Inconsistent(_) => "inconsistent",
            FormatError::MigrationFailed { .. } => "migration_failed",
        }
    }

    /// The section a failure is attributed to, when it has one.
    pub fn section(&self) -> Option<Tag> {
        match self {
            FormatError::SectionCorrupt { tag }
            | FormatError::MissingSection { tag }
            | FormatError::DuplicateSection { tag }
            | FormatError::UnknownRequiredSection { tag }
            | FormatError::SectionTooLarge { tag, .. }
            | FormatError::Malformed { tag, .. } => Some(*tag),
            _ => None,
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::BadMagic => f.write_str("not a composer project file"),
            FormatError::Truncated { what } => write!(f, "file ends early ({what})"),
            FormatError::UnsupportedVersion { found, supported } => {
                write!(
                    f,
                    "format version {found} is newer than supported {supported}"
                )
            }
            FormatError::BadHeader(why) => write!(f, "bad header: {why}"),
            FormatError::ManifestCorrupt => f.write_str("manifest checksum mismatch"),
            FormatError::BadLayout(why) => write!(f, "bad section layout: {why}"),
            FormatError::SectionCorrupt { tag } => write!(f, "section {tag} is corrupt"),
            FormatError::MissingSection { tag } => write!(f, "section {tag} is missing"),
            FormatError::DuplicateSection { tag } => write!(f, "section {tag} appears twice"),
            FormatError::UnknownRequiredSection { tag } => {
                write!(f, "unknown required section {tag}")
            }
            FormatError::TrailingBytes => f.write_str("bytes after the last section"),
            FormatError::SectionTooLarge { tag, len, limit } => {
                write!(f, "section {tag} is {len} bytes, limit {limit}")
            }
            FormatError::LimitExceeded { what, limit, found } => {
                write!(f, "{what} is {found}, limit {limit}")
            }
            FormatError::Malformed { tag, reason } => {
                write!(f, "section {tag} does not parse: {reason}")
            }
            FormatError::DuplicateId(id) => write!(f, "id {id} is used twice"),
            FormatError::DanglingReference(what) => write!(f, "dangling reference: {what}"),
            FormatError::OutOfRange(what) => write!(f, "{what} is out of range"),
            FormatError::Inconsistent(what) => write!(f, "inconsistent content: {what}"),
            FormatError::MigrationFailed { from, reason } => {
                write!(f, "migration from version {from} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for FormatError {}
