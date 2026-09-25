// FilePath: crates/sv-domain/src/error.rs
//! The one error type that crosses crate boundaries and reaches the UI.
//!
//! Crates convert their library errors with the constructor helpers (`AppError::database(e)`),
//! because the orphan rule forbids `impl From<sqlx::Error> for AppError` outside this crate and
//! this crate deliberately depends on no I/O library. The UI receives only the message string.

use std::fmt::Display;

use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Engine(String),
    #[error("{0}")]
    Audio(String),
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("File error: {0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn database(error: impl Display) -> Self {
        Self::Database(error.to_string())
    }

    pub fn network(error: impl Display) -> Self {
        Self::Network(error.to_string())
    }

    pub fn engine(error: impl Display) -> Self {
        Self::Engine(error.to_string())
    }

    pub fn audio(error: impl Display) -> Self {
        Self::Audio(error.to_string())
    }

    pub fn io(error: impl Display) -> Self {
        Self::Io(error.to_string())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    pub fn other(error: impl Display) -> Self {
        Self::Other(error.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error)
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_the_user_facing_message() {
        let json = serde_json::to_string(&AppError::NotFound("Dictation 4".into()));
        assert_eq!(json.ok().as_deref(), Some("\"Dictation 4 not found\""));
    }
}
