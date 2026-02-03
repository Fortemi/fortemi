//! Public Key Encryption (PKE) module for wallet-style E2E encryption.
//!
//! This module provides public-key-based encryption that allows users to
//! share only their public key addresses (like cryptocurrency wallets)
//! instead of having to exchange passphrases.
//!
//! # Overview
//!
//! This system uses X25519 keypairs where:
//!
//! - **Public key address** (`mm:...`) - Shareable identifier, like a wallet address
//! - **Private key** - Secret, stored encrypted on disk
//! - **No passphrase sharing** - Senders only need recipient's public address
//!
//! # Security Model
//!
//! - **X25519** - Curve25519 Diffie-Hellman for key exchange
//! - **HKDF-SHA256** - Key derivation with domain separation
//! - **AES-256-GCM** - Authenticated encryption
//! - **Forward secrecy** - Ephemeral keys per encryption
//!
//! # File Format (MMPKE01)
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
//! # Usage Example
//!
//! ## Generate a Keypair
//!
//! ```rust
//! use matric_crypto::pke::{Keypair, save_private_key, save_public_key};
//! use std::path::Path;
//!
//! // Generate new keypair
//! let keypair = Keypair::generate();
//!
//! // Get your address to share with others
//! let my_address = keypair.public.to_address();
//! println!("My address: {}", my_address);
//!
//! // Save keys to disk
//! # let temp = tempfile::tempdir().unwrap();
//! # let private_path = temp.path().join("private.key");
//! # let public_path = temp.path().join("public.key");
//! save_private_key(&keypair.private, &private_path, "my-passphrase!").unwrap();
//! save_public_key(&keypair.public, &public_path, Some("My Key")).unwrap();
//! ```
//!
//! ## Encrypt for Recipients
//!
//! ```rust
//! use matric_crypto::pke::{encrypt_pke, Keypair};
//!
//! let alice = Keypair::generate();
//! let bob = Keypair::generate();
//!
//! // Sender only needs public keys (or can use addresses to look them up)
//! let recipients = vec![alice.public.clone(), bob.public.clone()];
//!
//! let secret_data = b"Confidential information";
//! let encrypted = encrypt_pke(secret_data, &recipients, Some("data.json".into())).unwrap();
//! ```
//!
//! ## Decrypt with Private Key
//!
//! ```rust
//! use matric_crypto::pke::{encrypt_pke, decrypt_pke, Keypair};
//!
//! let alice = Keypair::generate();
//! let encrypted = encrypt_pke(b"Secret", &[alice.public.clone()], None).unwrap();
//!
//! // Recipient decrypts with their private key
//! let (plaintext, header) = decrypt_pke(&encrypted, &alice.private).unwrap();
//! assert_eq!(plaintext, b"Secret");
//! ```
//!
//! ## Check Recipients Without Decrypting
//!
//! ```rust
//! use matric_crypto::pke::{encrypt_pke, get_pke_recipients, Keypair};
//!
//! let alice = Keypair::generate();
//! let encrypted = encrypt_pke(b"data", &[alice.public.clone()], None).unwrap();
//!
//! let recipients = get_pke_recipients(&encrypted).unwrap();
//! println!("Recipients: {:?}", recipients);
//! ```

pub mod address;
pub mod ecdh;
pub mod encrypt;
pub mod format;
pub mod key_storage;
pub mod keys;

// Re-export commonly used types
pub use address::Address;
pub use encrypt::{can_decrypt_pke, decrypt_pke, encrypt_pke, get_pke_recipients};
pub use format::{is_pke_format, PkeHeader, RecipientBlock, MAGIC_BYTES};
pub use keys::{
    load_private_key, load_public_key, save_private_key, save_public_key, Keypair, PrivateKey,
    PublicKey,
};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Full workflow test: generate keys, encrypt, decrypt.
    #[test]
    fn test_full_pke_workflow() {
        // Alice generates a keypair
        let alice = Keypair::generate();
        let alice_address = alice.public.to_address();
        println!("Alice's address: {}", alice_address);

        // Bob generates a keypair
        let bob = Keypair::generate();
        let bob_address = bob.public.to_address();
        println!("Bob's address: {}", bob_address);

        // Carol wants to send a message to both
        let message = b"Hello Alice and Bob! This is a secret message.";
        let encrypted = encrypt_pke(
            message,
            &[alice.public.clone(), bob.public.clone()],
            Some("greeting.txt".into()),
        )
        .unwrap();

        // Verify it's in PKE format
        assert!(is_pke_format(&encrypted));

        // Both can see they're recipients
        let recipients = get_pke_recipients(&encrypted).unwrap();
        assert_eq!(recipients.len(), 2);
        assert!(recipients.contains(&alice_address));
        assert!(recipients.contains(&bob_address));

        // Alice decrypts
        let (plaintext_alice, header_alice) = decrypt_pke(&encrypted, &alice.private).unwrap();
        assert_eq!(plaintext_alice, message);
        assert_eq!(header_alice.original_filename, Some("greeting.txt".into()));

        // Bob decrypts
        let (plaintext_bob, _) = decrypt_pke(&encrypted, &bob.private).unwrap();
        assert_eq!(plaintext_bob, message);

        // Eve cannot decrypt (she's not a recipient)
        let eve = Keypair::generate();
        let result = decrypt_pke(&encrypted, &eve.private);
        assert!(result.is_err());
    }

    /// Test key persistence workflow.
    #[test]
    fn test_key_persistence_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let private_path = temp.path().join("test.key.enc");
        let public_path = temp.path().join("test.pub");

        // Generate and save keys
        let original = Keypair::generate();
        save_private_key(&original.private, &private_path, "secure-pass-123").unwrap();
        save_public_key(&original.public, &public_path, Some("Test Key")).unwrap();

        // Load keys back
        let loaded_private = load_private_key(&private_path, "secure-pass-123").unwrap();
        let loaded_public = load_public_key(&public_path).unwrap();

        // Verify they match
        assert_eq!(original.private.as_bytes(), loaded_private.as_bytes());
        assert_eq!(original.public.as_bytes(), loaded_public.as_bytes());

        // Verify encryption still works with loaded keys
        let message = b"Test message";
        let encrypted = encrypt_pke(message, &[loaded_public], None).unwrap();
        let (decrypted, _) = decrypt_pke(&encrypted, &loaded_private).unwrap();
        assert_eq!(message.as_slice(), decrypted.as_slice());
    }

    /// Test address round-trip.
    #[test]
    fn test_address_roundtrip() {
        let keypair = Keypair::generate();
        let address = keypair.public.to_address();

        // Parse the address string
        let parsed: Address = address.as_str().parse().unwrap();

        // Should be equal
        assert_eq!(address, parsed);
        assert!(parsed.verify_checksum());
    }
}
