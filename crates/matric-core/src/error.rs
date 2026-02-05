//! Error types for matric-memory.

use thiserror::Error;

/// Result type alias using matric-memory's Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for matric-memory operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Database operation failed (wraps sqlx::Error)
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

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

    /// HTTP/network request failed
    #[error("Request error: {0}")]
    Request(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Authentication/authorization failed
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Forbidden (authenticated but not authorized)
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// File I/O operation failed
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Request(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_error_display_not_found() {
        let err = Error::NotFound("test resource".to_string());
        assert_eq!(err.to_string(), "Not found: test resource");
    }

    #[test]
    fn test_error_display_note_not_found() {
        let id = Uuid::nil();
        let err = Error::NoteNotFound(id);
        assert_eq!(err.to_string(), format!("Note not found: {}", id));
    }

    #[test]
    fn test_error_display_collection_not_found() {
        let id = Uuid::nil();
        let err = Error::CollectionNotFound(id);
        assert_eq!(err.to_string(), format!("Collection not found: {}", id));
    }

    #[test]
    fn test_error_display_embedding() {
        let err = Error::Embedding("failed to generate".to_string());
        assert_eq!(err.to_string(), "Embedding error: failed to generate");
    }

    #[test]
    fn test_error_display_inference() {
        let err = Error::Inference("model timeout".to_string());
        assert_eq!(err.to_string(), "Inference error: model timeout");
    }

    #[test]
    fn test_error_display_search() {
        let err = Error::Search("index unavailable".to_string());
        assert_eq!(err.to_string(), "Search error: index unavailable");
    }

    #[test]
    fn test_error_display_job() {
        let err = Error::Job("queue full".to_string());
        assert_eq!(err.to_string(), "Job error: queue full");
    }

    #[test]
    fn test_error_display_serialization() {
        let err = Error::Serialization("invalid JSON".to_string());
        assert_eq!(err.to_string(), "Serialization error: invalid JSON");
    }

    #[test]
    fn test_error_display_config() {
        let err = Error::Config("missing API key".to_string());
        assert_eq!(err.to_string(), "Configuration error: missing API key");
    }

    #[test]
    fn test_error_display_invalid_input() {
        let err = Error::InvalidInput("negative count".to_string());
        assert_eq!(err.to_string(), "Invalid input: negative count");
    }

    #[test]
    fn test_error_display_request() {
        let err = Error::Request("network unreachable".to_string());
        assert_eq!(err.to_string(), "Request error: network unreachable");
    }

    #[test]
    fn test_error_display_internal() {
        let err = Error::Internal("unexpected state".to_string());
        assert_eq!(err.to_string(), "Internal error: unexpected state");
    }

    #[test]
    fn test_error_display_unauthorized() {
        let err = Error::Unauthorized("invalid token".to_string());
        assert_eq!(err.to_string(), "Unauthorized: invalid token");
    }

    #[test]
    fn test_error_display_forbidden() {
        let err = Error::Forbidden("insufficient permissions".to_string());
        assert_eq!(err.to_string(), "Forbidden: insufficient permissions");
    }

    #[test]
    fn test_error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::Io(io_err);
        assert!(err.to_string().contains("I/O error:"));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<i32>("not a number");
        assert!(json_err.is_err());

        let err: Error = json_err.unwrap_err().into();
        match err {
            Error::Serialization(msg) => {
                assert!(!msg.is_empty());
            }
            _ => panic!("Expected Serialization error"),
        }
    }

    #[test]
    fn test_from_serde_json_error_maintains_message() {
        let json_str = r#"{"invalid": json}"#;
        let json_err = serde_json::from_str::<serde_json::Value>(json_str);
        assert!(json_err.is_err());

        let err: Error = json_err.unwrap_err().into();
        assert!(err.to_string().contains("Serialization error:"));
    }

    #[test]
    fn test_result_type_ok() {
        fn get_result() -> Result<i32> {
            Ok(42)
        }
        let result = get_result();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_result_type_err() {
        let result: Result<i32> = Err(Error::Internal("test".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<Error>();
        assert_sync::<Error>();
    }

    #[test]
    fn test_error_debug_format() {
        let err = Error::NotFound("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("NotFound"));
    }

    #[test]
    fn test_note_not_found_with_random_uuid() {
        let id = Uuid::new_v4();
        let err = Error::NoteNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_collection_not_found_with_random_uuid() {
        let id = Uuid::new_v4();
        let err = Error::CollectionNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: Error = io_err.into();
        match err {
            Error::Io(_) => {} // Success
            _ => panic!("Expected Io error"),
        }
    }
}
