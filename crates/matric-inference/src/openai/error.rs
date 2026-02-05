//! OpenAI-specific error handling.

use matric_core::Error;

/// OpenAI-specific error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAIErrorCode {
    /// Invalid authentication credentials.
    AuthenticationError,
    /// Rate limit exceeded.
    RateLimitExceeded,
    /// Model not found or not available.
    ModelNotFound,
    /// Request too large.
    ContextLengthExceeded,
    /// Server error.
    ServerError,
    /// Unknown error.
    Unknown,
}

impl OpenAIErrorCode {
    /// Determine error code from HTTP status and error type.
    pub fn from_response(status: u16, error_type: &str) -> Self {
        match (status, error_type) {
            (401, _) => Self::AuthenticationError,
            (429, _) => Self::RateLimitExceeded,
            (404, _) | (_, "model_not_found") => Self::ModelNotFound,
            (400, _) if error_type.contains("context_length") => Self::ContextLengthExceeded,
            (500..=599, _) => Self::ServerError,
            _ => Self::Unknown,
        }
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimitExceeded | Self::ServerError)
    }
}

/// Convert OpenAI error to matric Error.
pub fn to_matric_error(code: OpenAIErrorCode, message: &str) -> Error {
    match code {
        OpenAIErrorCode::AuthenticationError => {
            Error::Config(format!("Authentication failed: {}", message))
        }
        OpenAIErrorCode::RateLimitExceeded => {
            Error::Inference(format!("Rate limit exceeded: {}", message))
        }
        OpenAIErrorCode::ModelNotFound => Error::Config(format!("Model not found: {}", message)),
        OpenAIErrorCode::ContextLengthExceeded => {
            Error::Inference(format!("Context too long: {}", message))
        }
        OpenAIErrorCode::ServerError => Error::Inference(format!("Server error: {}", message)),
        OpenAIErrorCode::Unknown => Error::Inference(message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_from_401() {
        let code = OpenAIErrorCode::from_response(401, "invalid_api_key");
        assert_eq!(code, OpenAIErrorCode::AuthenticationError);
    }

    #[test]
    fn test_error_code_from_429() {
        let code = OpenAIErrorCode::from_response(429, "rate_limit_exceeded");
        assert_eq!(code, OpenAIErrorCode::RateLimitExceeded);
    }

    #[test]
    fn test_error_code_from_404() {
        let code = OpenAIErrorCode::from_response(404, "model_not_found");
        assert_eq!(code, OpenAIErrorCode::ModelNotFound);
    }

    #[test]
    fn test_error_code_from_500() {
        let code = OpenAIErrorCode::from_response(500, "server_error");
        assert_eq!(code, OpenAIErrorCode::ServerError);
    }

    #[test]
    fn test_error_code_from_502() {
        let code = OpenAIErrorCode::from_response(502, "bad_gateway");
        assert_eq!(code, OpenAIErrorCode::ServerError);
    }

    #[test]
    fn test_error_code_from_unknown() {
        let code = OpenAIErrorCode::from_response(418, "im_a_teapot");
        assert_eq!(code, OpenAIErrorCode::Unknown);
    }

    #[test]
    fn test_retryable_rate_limit() {
        assert!(OpenAIErrorCode::RateLimitExceeded.is_retryable());
    }

    #[test]
    fn test_retryable_server_error() {
        assert!(OpenAIErrorCode::ServerError.is_retryable());
    }

    #[test]
    fn test_not_retryable_auth() {
        assert!(!OpenAIErrorCode::AuthenticationError.is_retryable());
    }

    #[test]
    fn test_not_retryable_model_not_found() {
        assert!(!OpenAIErrorCode::ModelNotFound.is_retryable());
    }

    #[test]
    fn test_to_matric_error_auth() {
        let err = to_matric_error(OpenAIErrorCode::AuthenticationError, "Invalid key");
        assert!(err.to_string().contains("Authentication failed"));
    }

    #[test]
    fn test_to_matric_error_rate_limit() {
        let err = to_matric_error(OpenAIErrorCode::RateLimitExceeded, "Too many requests");
        assert!(err.to_string().contains("Rate limit exceeded"));
    }
}
