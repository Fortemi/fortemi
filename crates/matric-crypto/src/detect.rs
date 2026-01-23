//! Format detection for encrypted files.
//!
//! Automatically detects if a file is PKE encrypted (MMPKE01).

use crate::format::{FileFormat, MAGIC_PKE};

/// Detect the format of a file from its bytes.
///
/// Returns `FileFormat::Pke` if the file is PKE encrypted,
/// `FileFormat::Unencrypted` otherwise.
pub fn detect_format(data: &[u8]) -> FileFormat {
    if data.len() < 8 {
        return FileFormat::Unencrypted;
    }

    let magic = &data[0..8];

    if magic == MAGIC_PKE {
        FileFormat::Pke
    } else {
        FileFormat::Unencrypted
    }
}

/// Check if a file is encrypted.
pub fn is_encrypted(data: &[u8]) -> bool {
    !matches!(detect_format(data), FileFormat::Unencrypted)
}

/// Check if a file is PKE encrypted (MMPKE01).
///
/// PKE (Public Key Encryption) uses wallet-style addresses
/// and X25519 key exchange.
pub fn is_pke_encrypted(data: &[u8]) -> bool {
    matches!(detect_format(data), FileFormat::Pke)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pke::{encrypt_pke, Keypair};

    #[test]
    fn test_detect_pke() {
        let alice = Keypair::generate();
        let plaintext = b"test data";
        let encrypted = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();

        assert_eq!(detect_format(&encrypted), FileFormat::Pke);
        assert!(is_encrypted(&encrypted));
        assert!(is_pke_encrypted(&encrypted));
    }

    #[test]
    fn test_detect_unencrypted() {
        let data = b"Just plain text data";

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
        assert!(!is_encrypted(data));
        assert!(!is_pke_encrypted(data));
    }

    #[test]
    fn test_detect_tar_gz() {
        // tar.gz magic bytes (gzip)
        let data = [0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];

        assert_eq!(detect_format(&data), FileFormat::Unencrypted);
        assert!(!is_encrypted(&data));
    }

    #[test]
    fn test_detect_too_short() {
        let data = b"short";

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
        assert!(!is_encrypted(data));
    }

    #[test]
    fn test_detect_empty() {
        let data: &[u8] = &[];

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
        assert!(!is_encrypted(data));
    }

    #[test]
    fn test_detect_partial_magic() {
        // Partial magic bytes
        let data = b"MMPKE0";

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
    }
}
