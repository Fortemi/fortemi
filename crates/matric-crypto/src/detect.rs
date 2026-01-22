//! Format detection for encrypted files.

use crate::format::{FileFormat, MAGIC_E2E, MAGIC_STANDARD};

/// Detect the format of a file from its bytes.
///
/// Returns `FileFormat::Unencrypted` if the file doesn't start with
/// a known magic byte sequence.
pub fn detect_format(data: &[u8]) -> FileFormat {
    if data.len() < 8 {
        return FileFormat::Unencrypted;
    }

    let magic = &data[0..8];

    if magic == MAGIC_STANDARD {
        FileFormat::Standard
    } else if magic == MAGIC_E2E {
        FileFormat::E2E
    } else {
        FileFormat::Unencrypted
    }
}

/// Check if a file is encrypted (either standard or E2E).
pub fn is_encrypted(data: &[u8]) -> bool {
    !matches!(detect_format(data), FileFormat::Unencrypted)
}

/// Check if a file is standard encrypted.
pub fn is_standard_encrypted(data: &[u8]) -> bool {
    matches!(detect_format(data), FileFormat::Standard)
}

/// Check if a file is E2E encrypted.
pub fn is_e2e_encrypted(data: &[u8]) -> bool {
    matches!(detect_format(data), FileFormat::E2E)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e2e::{encrypt_e2e, RecipientInput};
    use crate::standard::encrypt_with_passphrase;

    #[test]
    fn test_detect_standard() {
        let plaintext = b"test data";
        let encrypted = encrypt_with_passphrase(plaintext, "secure-passphrase-123", None).unwrap();

        assert_eq!(detect_format(&encrypted), FileFormat::Standard);
        assert!(is_encrypted(&encrypted));
        assert!(is_standard_encrypted(&encrypted));
        assert!(!is_e2e_encrypted(&encrypted));
    }

    #[test]
    fn test_detect_e2e() {
        let plaintext = b"test data";
        let recipients = vec![RecipientInput {
            id: "alice".to_string(),
            passphrase: "alice-passphrase-123".to_string(),
        }];
        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();

        assert_eq!(detect_format(&encrypted), FileFormat::E2E);
        assert!(is_encrypted(&encrypted));
        assert!(!is_standard_encrypted(&encrypted));
        assert!(is_e2e_encrypted(&encrypted));
    }

    #[test]
    fn test_detect_unencrypted() {
        let data = b"Just plain text data";

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
        assert!(!is_encrypted(data));
        assert!(!is_standard_encrypted(data));
        assert!(!is_e2e_encrypted(data));
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
        let data = b"MMENC0";

        assert_eq!(detect_format(data), FileFormat::Unencrypted);
    }
}
