//! # matric-crypto
//!
//! Cryptographic primitives for matric-memory.
//!
//! This crate provides public-key encryption (PKE) for secure data sharing
//! using wallet-style addresses. Users share their public key address (`mm:...`)
//! and senders can encrypt data without needing to exchange passphrases.
//!
//! ## Cryptographic Primitives
//!
//! - **Key exchange**: X25519 (Curve25519 ECDH)
//! - **Symmetric cipher**: AES-256-GCM (AEAD)
//! - **Key derivation**: HKDF-SHA256 (for KEK), Argon2id (for private key storage)
//! - **Address format**: BLAKE3 hash with Base58Check encoding
//! - **Random generation**: ChaCha20-based CSPRNG
//!
//! ## File Format (MMPKE01)
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │ Magic: "MMPKE01\n" (8 bytes)                    │
//! ├─────────────────────────────────────────────────┤
//! │ Header Length: u32 LE (4 bytes)                 │
//! ├─────────────────────────────────────────────────┤
//! │ Header (JSON with ephemeral key, recipients)   │
//! ├─────────────────────────────────────────────────┤
//! │ Encrypted Data (AES-256-GCM)                   │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Examples
//!
//! ### Generate a Keypair
//!
//! ```rust
//! use matric_crypto::pke::{Keypair, save_private_key, save_public_key};
//!
//! let keypair = Keypair::generate();
//! let my_address = keypair.public.to_address();
//! println!("My address: {}", my_address);  // mm:1abc...xyz
//!
//! # let temp = tempfile::tempdir().unwrap();
//! # let private_path = temp.path().join("private.key");
//! # let public_path = temp.path().join("public.key");
//! save_private_key(&keypair.private, &private_path, "my-passphrase!").unwrap();
//! save_public_key(&keypair.public, &public_path, Some("My Key")).unwrap();
//! ```
//!
//! ### Encrypt for Recipients
//!
//! ```rust
//! use matric_crypto::pke::{encrypt_pke, Keypair};
//!
//! let alice = Keypair::generate();
//! let bob = Keypair::generate();
//!
//! let secret_data = b"Confidential information";
//! let encrypted = encrypt_pke(
//!     secret_data,
//!     &[alice.public.clone(), bob.public.clone()],
//!     Some("data.json".into())
//! ).unwrap();
//! ```
//!
//! ### Decrypt with Private Key
//!
//! ```rust
//! use matric_crypto::pke::{encrypt_pke, decrypt_pke, Keypair};
//!
//! let alice = Keypair::generate();
//! let encrypted = encrypt_pke(b"Secret", &[alice.public.clone()], None).unwrap();
//!
//! let (plaintext, header) = decrypt_pke(&encrypted, &alice.private).unwrap();
//! assert_eq!(plaintext, b"Secret");
//! ```
//!
//! ### Format Detection
//!
//! ```rust
//! use matric_crypto::{detect_format, is_encrypted, is_pke_encrypted, FileFormat};
//!
//! let data = b"Some data";
//! match detect_format(data) {
//!     FileFormat::Pke => println!("PKE encrypted"),
//!     FileFormat::Unencrypted => println!("Not encrypted"),
//!     _ => println!("Unknown format"),
//! }
//! ```

pub mod cipher;
pub mod detect;
pub mod error;
pub mod format;
pub mod kdf;
pub mod pke;

// Re-export commonly used types
pub use detect::{detect_format, is_encrypted, is_pke_encrypted};
pub use error::{CryptoError, CryptoResult};
pub use format::{base64_decode, base64_encode, FileFormat};
pub use kdf::{derive_key, validate_passphrase, DerivedKey, KdfParams};

// Re-export PKE types at crate level for convenience
pub use pke::{
    can_decrypt_pke, decrypt_pke, encrypt_pke, get_pke_recipients, load_private_key,
    load_public_key, save_private_key, save_public_key, Address, Keypair, PkeHeader, PrivateKey,
    PublicKey,
};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test full PKE workflow: generate -> encrypt -> detect -> decrypt.
    #[test]
    fn test_full_pke_workflow() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let original = b"Shared secret data for the team";

        // Encrypt for both recipients
        let encrypted = encrypt_pke(
            original,
            &[alice.public.clone(), bob.public.clone()],
            Some("secret.txt".into()),
        )
        .unwrap();

        // Detect format
        assert!(is_encrypted(&encrypted));
        assert!(is_pke_encrypted(&encrypted));
        assert_eq!(detect_format(&encrypted), FileFormat::Pke);

        // Get recipients
        let recipients = get_pke_recipients(&encrypted).unwrap();
        assert_eq!(recipients.len(), 2);
        assert!(recipients.contains(&alice.public.to_address()));
        assert!(recipients.contains(&bob.public.to_address()));

        // Both can decrypt
        let (decrypted_alice, header_alice) = decrypt_pke(&encrypted, &alice.private).unwrap();
        let (decrypted_bob, _) = decrypt_pke(&encrypted, &bob.private).unwrap();

        assert_eq!(original.as_slice(), decrypted_alice.as_slice());
        assert_eq!(original.as_slice(), decrypted_bob.as_slice());
        assert_eq!(header_alice.original_filename, Some("secret.txt".into()));

        // Eve cannot decrypt
        let eve = Keypair::generate();
        assert!(decrypt_pke(&encrypted, &eve.private).is_err());
    }

    /// Test key persistence workflow.
    #[test]
    fn test_key_persistence() {
        let temp = tempfile::tempdir().unwrap();
        let private_path = temp.path().join("test.key.enc");
        let public_path = temp.path().join("test.pub");

        // Generate and save
        let original = Keypair::generate();
        save_private_key(&original.private, &private_path, "secure-pass-123").unwrap();
        save_public_key(&original.public, &public_path, Some("Test Key")).unwrap();

        // Load back
        let loaded_private = load_private_key(&private_path, "secure-pass-123").unwrap();
        let loaded_public = load_public_key(&public_path).unwrap();

        // Verify
        assert_eq!(original.private.as_bytes(), loaded_private.as_bytes());
        assert_eq!(original.public.as_bytes(), loaded_public.as_bytes());

        // Use loaded keys for encryption
        let message = b"Test message";
        let encrypted = encrypt_pke(message, &[loaded_public], None).unwrap();
        let (decrypted, _) = decrypt_pke(&encrypted, &loaded_private).unwrap();
        assert_eq!(message.as_slice(), decrypted.as_slice());
    }

    /// Test address format.
    #[test]
    fn test_address_format() {
        let keypair = Keypair::generate();
        let address = keypair.public.to_address();

        // Address starts with mm: prefix
        let addr_str = address.to_string();
        assert!(addr_str.starts_with("mm:"));
        assert!(addr_str.len() > 10); // Should be a reasonable length

        // Parse back
        let parsed: Address = addr_str.parse().unwrap();
        assert_eq!(address, parsed);
    }
}
