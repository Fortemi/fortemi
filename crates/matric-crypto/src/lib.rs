//! # matric-crypto
//!
//! Encryption primitives for matric-memory backup and sharing.
//!
//! This crate provides:
//!
//! - **Standard encryption** - Single-key encryption using passphrase or keyfile
//! - **E2E encryption** - Multi-recipient envelope encryption for secure sharing
//! - **Format detection** - Auto-detect encrypted vs unencrypted files
//!
//! ## Cryptographic Primitives
//!
//! - **Symmetric cipher**: AES-256-GCM (AEAD)
//! - **Key derivation**: Argon2id (memory-hard, GPU/ASIC resistant)
//! - **Random generation**: ChaCha20-based CSPRNG
//!
//! ## File Formats
//!
//! ### Standard Encrypted (MMENC01)
//!
//! Single-key encryption for backup archives and shards.
//!
//! ```text
//! +------------------+
//! | Magic: "MMENC01" | 8 bytes
//! +------------------+
//! | Header Length    | 4 bytes (little-endian)
//! +------------------+
//! | Header (JSON)    | Variable
//! +------------------+
//! | Encrypted Data   | Variable (includes 16-byte auth tag)
//! +------------------+
//! ```
//!
//! ### E2E Encrypted (MME2E01)
//!
//! Multi-recipient envelope encryption for secure shard sharing.
//!
//! ```text
//! +------------------+
//! | Magic: "MME2E01" | 8 bytes
//! +------------------+
//! | Header Length    | 4 bytes (little-endian)
//! +------------------+
//! | Header (JSON)    | Variable (includes encrypted DEKs)
//! +------------------+
//! | Encrypted Data   | Variable (includes 16-byte auth tag)
//! +------------------+
//! ```
//!
//! ## Examples
//!
//! ### Standard Encryption
//!
//! ```rust
//! use matric_crypto::{encrypt_with_passphrase, decrypt_standard, KeySource};
//!
//! // Encrypt
//! let data = b"My secret data";
//! let encrypted = encrypt_with_passphrase(data, "my-secure-passphrase", None).unwrap();
//!
//! // Decrypt
//! let (decrypted, header) = decrypt_standard(&encrypted, KeySource::Passphrase("my-secure-passphrase")).unwrap();
//! assert_eq!(data.as_slice(), decrypted.as_slice());
//! ```
//!
//! ### E2E Multi-Recipient Encryption
//!
//! ```rust
//! use matric_crypto::{encrypt_e2e, decrypt_e2e, RecipientInput};
//!
//! // Encrypt for multiple recipients
//! let data = b"Shared secret";
//! let recipients = vec![
//!     RecipientInput { id: "alice".into(), passphrase: "alice-secret-123".into() },
//!     RecipientInput { id: "bob".into(), passphrase: "bob-secret-456!!".into() },
//! ];
//! let encrypted = encrypt_e2e(data, &recipients, None).unwrap();
//!
//! // Either recipient can decrypt
//! let (decrypted, _) = decrypt_e2e(&encrypted, "alice", "alice-secret-123").unwrap();
//! assert_eq!(data.as_slice(), decrypted.as_slice());
//! ```
//!
//! ### Format Detection
//!
//! ```rust
//! use matric_crypto::{detect_format, FileFormat};
//!
//! let data = b"Some data";
//! match detect_format(data) {
//!     FileFormat::Standard => println!("Standard encrypted"),
//!     FileFormat::E2E => println!("E2E encrypted"),
//!     FileFormat::Unencrypted => println!("Not encrypted"),
//! }
//! ```

pub mod cipher;
pub mod detect;
pub mod e2e;
pub mod error;
pub mod format;
pub mod kdf;
pub mod standard;

// Re-export commonly used types
pub use detect::{detect_format, is_e2e_encrypted, is_encrypted, is_standard_encrypted};
pub use e2e::{decrypt_e2e, decrypt_e2e_auto, encrypt_e2e, get_recipient_ids, RecipientInput};
pub use error::{CryptoError, CryptoResult};
pub use format::{E2EHeader, EncryptedFile, FileFormat, Header, KeyType, Recipient};
pub use kdf::{
    derive_key, generate_keyfile, load_keyfile, validate_passphrase, DerivedKey, KdfParams,
};
pub use standard::{
    decrypt_standard, encrypt_standard, encrypt_with_keyfile, encrypt_with_passphrase,
    EncryptOptions, KeySource,
};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Verify that standard and E2E formats have distinct magic bytes.
    #[test]
    fn test_formats_are_distinct() {
        let plaintext = b"Test data for format distinction";

        let standard = encrypt_with_passphrase(plaintext, "standard-passphrase", None).unwrap();
        let e2e = encrypt_e2e(
            plaintext,
            &[RecipientInput {
                id: "test".into(),
                passphrase: "e2e-passphrase-123".into(),
            }],
            None,
        )
        .unwrap();

        // Different magic bytes
        assert_ne!(&standard[0..8], &e2e[0..8]);

        // Format detection works
        assert_eq!(detect_format(&standard), FileFormat::Standard);
        assert_eq!(detect_format(&e2e), FileFormat::E2E);
    }

    /// Test cross-decryption fails (E2E format with standard decrypt).
    #[test]
    fn test_cross_decryption_fails() {
        let plaintext = b"Test data";

        let e2e = encrypt_e2e(
            plaintext,
            &[RecipientInput {
                id: "test".into(),
                passphrase: "passphrase-12345".into(),
            }],
            None,
        )
        .unwrap();

        let result = decrypt_standard(&e2e, KeySource::Passphrase("passphrase-12345"));
        assert!(result.is_err());
    }

    /// Test full workflow: encrypt -> detect -> decrypt.
    #[test]
    fn test_full_workflow_standard() {
        let original = b"Important backup data that must be protected";
        let passphrase = "strong-passphrase-123";

        // Encrypt
        let encrypted =
            encrypt_with_passphrase(original, passphrase, Some("backup.tar.gz".into())).unwrap();

        // Detect
        assert!(is_encrypted(&encrypted));
        assert!(is_standard_encrypted(&encrypted));

        // Decrypt
        let (decrypted, header) =
            decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(original.as_slice(), decrypted.as_slice());
        assert_eq!(header.original_filename, Some("backup.tar.gz".to_string()));
    }

    /// Test full workflow: E2E encrypt -> detect -> auto-decrypt.
    #[test]
    fn test_full_workflow_e2e() {
        let original = b"Shared knowledge shard for team collaboration";
        let recipients = vec![
            RecipientInput {
                id: "alice@example.com".into(),
                passphrase: "alice-secure-phrase".into(),
            },
            RecipientInput {
                id: "bob@example.com".into(),
                passphrase: "bob-secure-phrase!!".into(),
            },
        ];

        // Encrypt
        let encrypted = encrypt_e2e(original, &recipients, Some("team.shard".into())).unwrap();

        // Detect
        assert!(is_encrypted(&encrypted));
        assert!(is_e2e_encrypted(&encrypted));

        // Get recipients without decrypting
        let ids = get_recipient_ids(&encrypted).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"alice@example.com".to_string()));
        assert!(ids.contains(&"bob@example.com".to_string()));

        // Auto-decrypt (Bob)
        let (decrypted, header, found_id) =
            decrypt_e2e_auto(&encrypted, "bob-secure-phrase!!").unwrap();

        assert_eq!(original.as_slice(), decrypted.as_slice());
        assert_eq!(header.original_filename, Some("team.shard".to_string()));
        assert_eq!(found_id, "bob@example.com");
    }
}
