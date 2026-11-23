use composer_model::{ChangeError, Id};
use std::fmt;

/// Why a command was rejected. The project is untouched when this is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    NotFound {
        what: &'static str,
        id: Id,
    },
    DuplicateId(Id),
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    WrongKind {
        reason: &'static str,
    },
    /// A note would fall outside its clip.
    OutOfBounds {
        field: &'static str,
        reason: &'static str,
    },
    GroupAlreadyOpen,
    NoOpenGroup,
    Apply(ChangeError),
}

impl CommandError {
    /// Stable machine readable code for CLI and MCP clients.
    pub fn code(&self) -> &'static str {
        match self {
            CommandError::NotFound { .. } => "not_found",
            CommandError::DuplicateId(_) => "duplicate_id",
            CommandError::Invalid { .. } => "invalid",
            CommandError::WrongKind { .. } => "wrong_kind",
            CommandError::GroupAlreadyOpen => "group_already_open",
            CommandError::NoOpenGroup => "no_open_group",
            CommandError::OutOfBounds { .. } => "out_of_bounds",
            CommandError::Apply(ChangeError::OutOfRange(_)) => "invalid",
            CommandError::Apply(ChangeError::OutOfBounds(_)) => "out_of_bounds",
            CommandError::Apply(_) => "apply_failed",
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NotFound { what, id } => write!(f, "{what} {id} not found"),
            CommandError::DuplicateId(id) => write!(f, "id {id} already in use"),
            CommandError::Invalid { field, reason } => write!(f, "invalid {field}: {reason}"),
            CommandError::WrongKind { reason } => f.write_str(reason),
            CommandError::OutOfBounds { field, reason } => {
                write!(f, "out of bounds {field}: {reason}")
            }
            CommandError::GroupAlreadyOpen => f.write_str("a transaction group is already open"),
            CommandError::NoOpenGroup => f.write_str("no transaction group is open"),
            CommandError::Apply(e) => write!(f, "apply failed: {e}"),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<ChangeError> for CommandError {
    fn from(e: ChangeError) -> Self {
        CommandError::Apply(e)
    }
}
