//! Shared format utilities.

use base64::Engine;

use crate::error::{CryptoError, CryptoResult};

/// Magic bytes for PKE (public key) encrypted format.
pub const MAGIC_PKE: &[u8; 8] = b"MMPKE01\n";

/// File format type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Public-key encryption (MMPKE01) - wallet-style.
    Pke,
    /// Unencrypted file.
    Unencrypted,
}

/// Encode bytes as base64.
pub fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decode base64 string to bytes.
pub fn base64_decode(data: &str) -> CryptoResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| CryptoError::HeaderParse(format!("Invalid base64: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_constant() {
        assert_eq!(MAGIC_PKE.len(), 8);
        assert!(MAGIC_PKE.starts_with(b"MMPKE"));
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = [42u8; 32];
        let encoded = base64_encode(&original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.as_slice(), decoded.as_slice());
    }

    #[test]
    fn test_base64_decode_invalid() {
        let result = base64_decode("not valid base64!!!");
        assert!(matches!(result, Err(CryptoError::HeaderParse(_))));
    }

    #[test]
    fn test_file_format_debug() {
        assert_eq!(format!("{:?}", FileFormat::Pke), "Pke");
        assert_eq!(format!("{:?}", FileFormat::Unencrypted), "Unencrypted");
    }
}
