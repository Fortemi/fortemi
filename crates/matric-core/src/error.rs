//! Error types for matric-memory.

use thiserror::Error;

/// Result type alias using matric-memory's Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for matric-memory operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(String),

    /// Note not found
    #[error("Note not found: {0}")]
    NoteNotFound(uuid::Uuid),

    /// Collection not found
    #[error("Collection not found: {0}")]
    CollectionNotFound(uuid::Uuid),

    /// Embedding generation failed
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Inference/generation failed
    #[error("Inference error: {0}")]
    Inference(String),

    /// Search operation failed
    #[error("Search error: {0}")]
    Search(String),

    /// Job queue error
    #[error("Job error: {0}")]
    Job(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}
