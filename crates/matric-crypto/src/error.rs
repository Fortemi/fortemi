//! Error types for cryptographic operations.

use thiserror::Error;

/// Cryptographic operation errors.
#[derive(Error, Debug)]
pub enum CryptoError {
    /// Invalid magic bytes - not an encrypted file.
    #[error("Invalid magic bytes - not an encrypted file")]
    InvalidMagic,

    /// Unsupported format version.
    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u32),

    /// Header parsing failed.
    #[error("Header parsing failed: {0}")]
    HeaderParse(String),

    /// Key derivation failed.
    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),

    /// Encryption failed.
    #[error("Encryption failed: {0}")]
    Encryption(String),

    /// Decryption failed - wrong key or corrupted data.
    #[error("Decryption failed: {0}")]
    Decryption(String),

    /// Authentication failed - data may be tampered.
    #[error("Authentication failed - data may be tampered")]
    Authentication,

    /// No matching recipient found in E2E encrypted file.
    #[error("No matching recipient found")]
    NoMatchingRecipient,

    /// Invalid keyfile format or size.
    #[error("Invalid keyfile: {0}")]
    InvalidKeyfile(String),

    /// Passphrase too short.
    #[error("Passphrase too short (minimum {0} characters required)")]
    PassphraseTooShort(usize),

    /// Invalid recipient ID.
    #[error("Invalid recipient ID: {0}")]
    InvalidRecipientId(String),

    /// Invalid address format.
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    /// Invalid input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Invalid format.
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for cryptographic operations.
pub type CryptoResult<T> = Result<T, CryptoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = CryptoError::InvalidMagic;
        assert!(err.to_string().contains("magic bytes"));
    }

    #[test]
    fn test_unsupported_version_display() {
        let err = CryptoError::UnsupportedVersion(99);
        assert!(err.to_string().contains("99"));
    }

    #[test]
    fn test_passphrase_too_short_display() {
        let err = CryptoError::PassphraseTooShort(12);
        assert!(err.to_string().contains("12"));
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let crypto_err: CryptoError = io_err.into();
        assert!(matches!(crypto_err, CryptoError::Io(_)));
    }
}
