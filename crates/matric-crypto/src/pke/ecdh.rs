//! X25519 Elliptic Curve Diffie-Hellman key exchange.
//!
//! This module implements the key exchange protocol that allows two parties
//! to derive a shared secret using their keypairs, without ever transmitting
//! the secret itself.
//!
//! # Protocol
//!
//! For encryption:
//! 1. Sender generates an ephemeral keypair
//! 2. Sender computes: shared_secret = ECDH(ephemeral_private, recipient_public)
//! 3. Sender derives encryption key via HKDF
//! 4. Ephemeral public key is sent with ciphertext
//!
//! For decryption:
//! 1. Recipient computes: shared_secret = ECDH(recipient_private, ephemeral_public)
//! 2. Recipient derives same encryption key via HKDF
//! 3. Same key allows decryption
//!
//! # Security
//!
//! - Forward secrecy: ephemeral keys mean past messages stay secure even if
//!   long-term keys are compromised
//! - HKDF adds domain separation to prevent key reuse across contexts
//! - All secrets are zeroized after use

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::pke::keys::{PrivateKey, PublicKey};

/// Shared secret from ECDH (32 bytes).
///
/// This is the raw output of the X25519 key exchange. It should be
/// passed through HKDF before use as an encryption key.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SharedSecret([u8; 32]);

impl SharedSecret {
    /// Get the raw bytes of the shared secret.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SharedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedSecret")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// Derived encryption key (32 bytes for AES-256).
///
/// This is the output of HKDF, ready for use with AES-256-GCM.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DerivedEncryptionKey([u8; 32]);

impl DerivedEncryptionKey {
    /// Get the raw bytes of the derived key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for DerivedEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DerivedEncryptionKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Domain separation context for HKDF.
const HKDF_INFO_KEK: &[u8] = b"matric-memory-pke-kek-v1";

/// Perform X25519 Diffie-Hellman key exchange.
///
/// Computes the shared secret from our private key and their public key.
/// The result is the same whether computed as:
/// - ECDH(our_private, their_public)
/// - ECDH(their_private, our_public)
///
/// # Arguments
///
/// * `our_private` - Our private key
/// * `their_public` - Their public key
///
/// # Returns
///
/// The shared secret (32 bytes)
pub fn ecdh(our_private: &PrivateKey, their_public: &PublicKey) -> SharedSecret {
    let secret = our_private.to_x25519();
    let public = their_public.to_x25519();
    let shared = secret.diffie_hellman(&public);
    SharedSecret(*shared.as_bytes())
}

/// Derive an encryption key from a shared secret using HKDF-SHA256.
///
/// # Arguments
///
/// * `shared_secret` - The raw ECDH output
/// * `salt` - Optional salt (use ephemeral public key)
/// * `info` - Context info for domain separation
///
/// # Returns
///
/// A 32-byte key suitable for AES-256
pub fn derive_encryption_key(
    shared_secret: &SharedSecret,
    salt: Option<&[u8]>,
    info: &[u8],
) -> DerivedEncryptionKey {
    let hkdf = Hkdf::<Sha256>::new(salt, shared_secret.as_bytes());
    let mut key = [0u8; 32];
    // HKDF expand cannot fail with a 32-byte output
    hkdf.expand(info, &mut key)
        .expect("HKDF expand failed - this should never happen with 32-byte output");
    DerivedEncryptionKey(key)
}

/// Derive a Key Encryption Key (KEK) for wrapping the DEK.
///
/// This key is used to encrypt the Data Encryption Key (DEK) for each recipient.
///
/// # Arguments
///
/// * `ephemeral_private` - Sender's ephemeral private key
/// * `recipient_public` - Recipient's public key
/// * `ephemeral_public` - Sender's ephemeral public key (used as salt)
pub fn derive_kek(
    ephemeral_private: &PrivateKey,
    recipient_public: &PublicKey,
    ephemeral_public: &PublicKey,
) -> DerivedEncryptionKey {
    let shared = ecdh(ephemeral_private, recipient_public);
    derive_encryption_key(&shared, Some(ephemeral_public.as_bytes()), HKDF_INFO_KEK)
}

/// Derive a KEK for decryption (from recipient's perspective).
///
/// # Arguments
///
/// * `recipient_private` - Recipient's private key
/// * `ephemeral_public` - Sender's ephemeral public key (from ciphertext header)
pub fn derive_kek_for_decrypt(
    recipient_private: &PrivateKey,
    ephemeral_public: &PublicKey,
) -> DerivedEncryptionKey {
    let shared = ecdh(recipient_private, ephemeral_public);
    derive_encryption_key(&shared, Some(ephemeral_public.as_bytes()), HKDF_INFO_KEK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pke::keys::Keypair;

    #[test]
    fn test_ecdh_shared_secret() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        // Alice computes shared secret with Bob's public key
        let shared_alice = ecdh(&alice.private, &bob.public);

        // Bob computes shared secret with Alice's public key
        let shared_bob = ecdh(&bob.private, &alice.public);

        // They should be the same
        assert_eq!(shared_alice.as_bytes(), shared_bob.as_bytes());
    }

    #[test]
    fn test_ecdh_different_keys_different_secrets() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();
        let carol = Keypair::generate();

        let shared_ab = ecdh(&alice.private, &bob.public);
        let shared_ac = ecdh(&alice.private, &carol.public);

        assert_ne!(shared_ab.as_bytes(), shared_ac.as_bytes());
    }

    #[test]
    fn test_derive_encryption_key() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let shared = ecdh(&alice.private, &bob.public);
        let key = derive_encryption_key(&shared, None, b"test-context");

        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_derive_encryption_key_deterministic() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let shared = ecdh(&alice.private, &bob.public);
        let key1 = derive_encryption_key(&shared, Some(b"salt"), b"context");
        let key2 = derive_encryption_key(&shared, Some(b"salt"), b"context");

        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_encryption_key_different_contexts() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let shared = ecdh(&alice.private, &bob.public);
        let key1 = derive_encryption_key(&shared, None, b"context-1");
        let key2 = derive_encryption_key(&shared, None, b"context-2");

        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_kek_symmetric() {
        let recipient = Keypair::generate();
        let ephemeral = Keypair::generate();

        // Sender derives KEK
        let kek_sender = derive_kek(&ephemeral.private, &recipient.public, &ephemeral.public);

        // Recipient derives KEK (only knows ephemeral public)
        let kek_recipient = derive_kek_for_decrypt(&recipient.private, &ephemeral.public);

        // They should match
        assert_eq!(kek_sender.as_bytes(), kek_recipient.as_bytes());
    }

    #[test]
    fn test_shared_secret_debug_redacted() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();
        let shared = ecdh(&alice.private, &bob.public);

        let debug = format!("{:?}", shared);
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn test_derived_key_debug_redacted() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();
        let shared = ecdh(&alice.private, &bob.public);
        let key = derive_encryption_key(&shared, None, b"test");

        let debug = format!("{:?}", key);
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn test_different_ephemeral_different_kek() {
        let recipient = Keypair::generate();
        let ephemeral1 = Keypair::generate();
        let ephemeral2 = Keypair::generate();

        let kek1 = derive_kek(&ephemeral1.private, &recipient.public, &ephemeral1.public);
        let kek2 = derive_kek(&ephemeral2.private, &recipient.public, &ephemeral2.public);

        // Different ephemeral keys should produce different KEKs
        assert_ne!(kek1.as_bytes(), kek2.as_bytes());
    }
}
