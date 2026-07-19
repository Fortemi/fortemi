//! Error types for matric-memory.

use std::error::Error as StdError;
use std::fmt;

/// Result type alias using matric-memory's Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for matric-memory operations.
pub enum Error {
    /// Database operation failed (wraps sqlx::Error)
    Database(sqlx::Error),

    /// Resource not found
    NotFound(String),

    /// Note not found
    NoteNotFound(uuid::Uuid),

    /// Collection not found
    CollectionNotFound(uuid::Uuid),

    /// Embedding generation failed
    Embedding(String),

    /// Inference/generation failed
    Inference(String),

    /// Search operation failed
    Search(String),

    /// Job queue error
    Job(String),

    /// A persisted job row uses a type or status this binary cannot decode.
    IncompatibleJobRow {
        /// Persisted job identifier.
        job_id: uuid::Uuid,
        /// Rejected enum field (`job_type` or `status`).
        field: &'static str,
        /// Character count of the rejected value.
        value_len: usize,
    },

    /// Serialization/deserialization error
    Serialization(String),

    /// Configuration error
    Config(String),

    /// Invalid input
    InvalidInput(String),

    /// HTTP/network request failed
    Request(String),

    /// Internal error
    Internal(String),

    /// Authentication/authorization failed
    Unauthorized(String),

    /// Forbidden (authenticated but not authorized)
    Forbidden(String),

    /// File I/O operation failed
    Io(std::io::Error),
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(err) => redacted_error_debug(f, "Database", &err.to_string()),
            Self::NotFound(value) => redacted_error_debug(f, "NotFound", value),
            Self::NoteNotFound(_) => f
                .debug_struct("NoteNotFound")
                .field("id_present", &true)
                .finish(),
            Self::CollectionNotFound(_) => f
                .debug_struct("CollectionNotFound")
                .field("id_present", &true)
                .finish(),
            Self::Embedding(value) => redacted_error_debug(f, "Embedding", value),
            Self::Inference(value) => redacted_error_debug(f, "Inference", value),
            Self::Search(value) => redacted_error_debug(f, "Search", value),
            Self::Job(value) => redacted_error_debug(f, "Job", value),
            Self::IncompatibleJobRow {
                job_id,
                field,
                value_len,
            } => f
                .debug_struct("IncompatibleJobRow")
                .field("job_id_present", &(!job_id.is_nil()))
                .field("field", field)
                .field("value_len", value_len)
                .finish(),
            Self::Serialization(value) => redacted_error_debug(f, "Serialization", value),
            Self::Config(value) => redacted_error_debug(f, "Config", value),
            Self::InvalidInput(value) => redacted_error_debug(f, "InvalidInput", value),
            Self::Request(value) => redacted_error_debug(f, "Request", value),
            Self::Internal(value) => redacted_error_debug(f, "Internal", value),
            Self::Unauthorized(value) => redacted_error_debug(f, "Unauthorized", value),
            Self::Forbidden(value) => redacted_error_debug(f, "Forbidden", value),
            Self::Io(err) => redacted_error_debug(f, "Io", &err.to_string()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(err) => write_redacted_error(f, "Database error", &err.to_string()),
            Self::NotFound(value) => write_redacted_error(f, "Not found", value),
            Self::NoteNotFound(_) => f.write_str("Note not found: id_present=true"),
            Self::CollectionNotFound(_) => f.write_str("Collection not found: id_present=true"),
            Self::Embedding(value) => write_redacted_error(f, "Embedding error", value),
            Self::Inference(value) => write_redacted_error(f, "Inference error", value),
            Self::Search(value) => write_redacted_error(f, "Search error", value),
            Self::Job(value) => write_redacted_error(f, "Job error", value),
            Self::IncompatibleJobRow {
                job_id,
                field,
                value_len,
            } => write!(
                f,
                "Incompatible job row: job_id_present={} field={field} value_len={value_len}",
                !job_id.is_nil()
            ),
            Self::Serialization(value) => write_redacted_error(f, "Serialization error", value),
            Self::Config(value) => write_redacted_error(f, "Configuration error", value),
            Self::InvalidInput(value) => write_redacted_error(f, "Invalid input", value),
            Self::Request(value) => write_redacted_error(f, "Request error", value),
            Self::Internal(value) => write_redacted_error(f, "Internal error", value),
            Self::Unauthorized(value) => write_redacted_error(f, "Unauthorized", value),
            Self::Forbidden(value) => write_redacted_error(f, "Forbidden", value),
            Self::Io(err) => write_redacted_error(f, "I/O error", &err.to_string()),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Database(err) => Some(err),
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Database(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
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

fn redacted_error_debug(f: &mut fmt::Formatter<'_>, variant: &str, message: &str) -> fmt::Result {
    f.debug_struct(variant)
        .field("message_len", &message.chars().count())
        .field("message_class", &error_message_class(message))
        .finish()
}

fn write_redacted_error(f: &mut fmt::Formatter<'_>, label: &str, message: &str) -> fmt::Result {
    write!(
        f,
        "{label}: message_class={} message_len={}",
        error_message_class(message),
        message.chars().count()
    )
}

fn error_message_class(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if value.is_empty() {
        "empty"
    } else if lower.starts_with("bearer ")
        || lower.starts_with("mm_key_")
        || lower.starts_with("mm_at_")
        || lower.contains("-----begin private key-----")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("api_key")
        || lower.contains("authorization")
        || lower.contains("client_secret=")
        || lower.contains("postgres://")
        || lower.contains("postgresql://")
    {
        "secret_candidate"
    } else if lower.contains("://") {
        "url_like"
    } else if value.contains('/') || value.contains('\\') {
        "path_like"
    } else if value.chars().any(|ch| ch.is_control()) {
        "control_chars"
    } else {
        "text"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_error_display_not_found() {
        let err = Error::NotFound("test resource".to_string());
        assert_eq!(
            err.to_string(),
            "Not found: message_class=text message_len=13"
        );
    }

    #[test]
    fn test_error_display_note_not_found() {
        let id = Uuid::nil();
        let err = Error::NoteNotFound(id);
        assert_eq!(err.to_string(), "Note not found: id_present=true");
    }

    #[test]
    fn test_error_display_collection_not_found() {
        let id = Uuid::nil();
        let err = Error::CollectionNotFound(id);
        assert_eq!(err.to_string(), "Collection not found: id_present=true");
    }

    #[test]
    fn test_error_display_embedding() {
        let err = Error::Embedding("failed to generate".to_string());
        assert_eq!(
            err.to_string(),
            "Embedding error: message_class=text message_len=18"
        );
    }

    #[test]
    fn test_error_display_inference() {
        let err = Error::Inference("model timeout".to_string());
        assert_eq!(
            err.to_string(),
            "Inference error: message_class=text message_len=13"
        );
    }

    #[test]
    fn test_error_display_search() {
        let err = Error::Search("index unavailable".to_string());
        assert_eq!(
            err.to_string(),
            "Search error: message_class=text message_len=17"
        );
    }

    #[test]
    fn test_error_display_job() {
        let err = Error::Job("queue full".to_string());
        assert_eq!(
            err.to_string(),
            "Job error: message_class=text message_len=10"
        );
    }

    #[test]
    fn incompatible_job_row_error_exposes_only_bounded_diagnostics() {
        let job_id = Uuid::new_v4();
        let err = Error::IncompatibleJobRow {
            job_id,
            field: "job_type",
            value_len: 23,
        };

        let display = err.to_string();
        let debug = format!("{err:?}");
        assert_eq!(
            display,
            "Incompatible job row: job_id_present=true field=job_type value_len=23"
        );
        assert!(debug.contains("IncompatibleJobRow"));
        assert!(debug.contains("job_id_present: true"));
        assert!(!display.contains(&job_id.to_string()));
        assert!(!debug.contains(&job_id.to_string()));
    }

    #[test]
    fn test_error_display_serialization() {
        let err = Error::Serialization("invalid JSON".to_string());
        assert_eq!(
            err.to_string(),
            "Serialization error: message_class=text message_len=12"
        );
    }

    #[test]
    fn test_error_display_config() {
        let err = Error::Config("missing API key".to_string());
        assert_eq!(
            err.to_string(),
            "Configuration error: message_class=text message_len=15"
        );
    }

    #[test]
    fn test_error_display_invalid_input() {
        let err = Error::InvalidInput("negative count".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid input: message_class=text message_len=14"
        );
    }

    #[test]
    fn test_error_display_request() {
        let err = Error::Request("network unreachable".to_string());
        assert_eq!(
            err.to_string(),
            "Request error: message_class=text message_len=19"
        );
    }

    #[test]
    fn test_error_display_internal() {
        let err = Error::Internal("unexpected state".to_string());
        assert_eq!(
            err.to_string(),
            "Internal error: message_class=text message_len=16"
        );
    }

    #[test]
    fn test_error_display_unauthorized() {
        let err = Error::Unauthorized("invalid token".to_string());
        assert_eq!(
            err.to_string(),
            "Unauthorized: message_class=secret_candidate message_len=13"
        );
    }

    #[test]
    fn test_error_display_forbidden() {
        let err = Error::Forbidden("insufficient permissions".to_string());
        assert_eq!(
            err.to_string(),
            "Forbidden: message_class=text message_len=24"
        );
    }

    #[test]
    fn test_error_display_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::Io(io_err);
        assert!(err.to_string().contains("I/O error:"));
        assert!(err.to_string().contains("message_class=text"));
        assert!(!err.to_string().contains("file not found"));
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
        assert!(debug_str.contains("message_len: 4"));
        assert!(!debug_str.contains("test"));
    }

    #[test]
    fn test_note_not_found_with_random_uuid() {
        let id = Uuid::new_v4();
        let err = Error::NoteNotFound(id);
        assert!(!err.to_string().contains(&id.to_string()));
        assert!(err.to_string().contains("id_present=true"));
    }

    #[test]
    fn test_collection_not_found_with_random_uuid() {
        let id = Uuid::new_v4();
        let err = Error::CollectionNotFound(id);
        assert!(!err.to_string().contains(&id.to_string()));
        assert!(err.to_string().contains("id_present=true"));
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

    #[test]
    fn error_display_and_debug_redact_secret_path_and_url_messages() {
        let raw = "failed for postgres://user:secret@localhost/db at /srv/fortemi/private.log"
            .to_string();
        let errors = [
            Error::Inference(raw.clone()),
            Error::Search(raw.clone()),
            Error::Config(raw.clone()),
            Error::InvalidInput(raw.clone()),
            Error::Request(raw.clone()),
            Error::Internal(raw.clone()),
            Error::Unauthorized(raw.clone()),
            Error::Forbidden(raw.clone()),
        ];

        for err in errors {
            let display = err.to_string();
            let debug = format!("{err:?}");

            assert!(display.contains("message_class=secret_candidate"));
            assert!(display.contains("message_len="));
            assert!(debug.contains("message_class: \"secret_candidate\""));
            assert!(debug.contains("message_len:"));
            assert!(!display.contains(&raw));
            assert!(!debug.contains(&raw));
            assert!(!display.contains("postgres://"));
            assert!(!debug.contains("postgres://"));
            assert!(!display.contains("/srv/fortemi/private.log"));
            assert!(!debug.contains("/srv/fortemi/private.log"));
        }
    }
}
