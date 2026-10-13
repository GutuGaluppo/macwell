use serde::{Serialize, Serializer};

use crate::filesystem::safety::SafetyViolation;

/// Error type returned by every Tauri command. It serializes to a small tagged object so the UI can
/// show a readable message and, for safety violations, explain which rule blocked the action.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("blocked by safety rule: {0}")]
    Safety(#[from] SafetyViolation),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The backup gate refused: the action needs an acknowledgement that was not given.
    #[error("{0}")]
    BackupGate(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::Safety(_) => "safety",
            AppError::NotFound(_) => "notFound",
            AppError::Invalid(_) => "invalid",
            AppError::BackupGate(_) => "backupGate",
            AppError::Cancelled => "cancelled",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<trash::Error> for AppError {
    fn from(err: trash::Error) -> Self {
        AppError::Other(format!("could not move to Trash: {err}"))
    }
}
