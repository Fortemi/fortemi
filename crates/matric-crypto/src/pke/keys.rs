//! X25519 keypair generation and storage for public-key encryption.
//!
//! This module provides:
//! - Keypair generation using X25519 (Curve25519)
//! - Secure private key storage (encrypted with Argon2id + AES-256-GCM)
//! - Public key export/import
//!
//! # Security
//!
//! - Private keys are zeroized on drop
//! - Private key files are encrypted at rest
//! - Random number generation uses ChaCha20-based CSPRNG

use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{CryptoError, CryptoResult};
use crate::pke::key_storage::{decrypt_private_key, encrypt_private_key};

/// X25519 public key (32 bytes).
///
/// Public keys can be freely shared and are used by senders to encrypt
/// data that only the corresponding private key holder can decrypt.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// Create a public key from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the public key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to the x25519-dalek public key type.
    pub(crate) fn to_x25519(&self) -> X25519Public {
        X25519Public::from(self.0)
    }
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PublicKey({})", hex::encode(&self.0[..8]))
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.0);
        serializer.serialize_str(&encoded)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &s)
            .map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("invalid public key length"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

/// X25519 private key (32 bytes) with automatic zeroization.
///
/// Private keys must be kept secret. They are automatically zeroized
/// when dropped to prevent key material from remaining in memory.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey([u8; 32]);

impl PrivateKey {
    /// Create a private key from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the private key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to the x25519-dalek static secret type.
    pub(crate) fn to_x25519(&self) -> StaticSecret {
        StaticSecret::from(self.0)
    }

    /// Derive the corresponding public key.
    pub fn public_key(&self) -> PublicKey {
        let secret = self.to_x25519();
        let public = X25519Public::from(&secret);
        PublicKey(*public.as_bytes())
    }
}

impl Clone for PrivateKey {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// X25519 keypair for public-key encryption.
pub struct Keypair {
    /// The public key (can be shared).
    pub public: PublicKey,
    /// The private key (must be kept secret).
    pub private: PrivateKey,
}

impl Keypair {
    /// Generate a new random keypair.
    ///
    /// Uses a cryptographically secure random number generator.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);

        let secret = StaticSecret::from(secret_bytes);
        let public = X25519Public::from(&secret);

        // Zeroize the temporary bytes
        secret_bytes.zeroize();

        Self {
            public: PublicKey(*public.as_bytes()),
            private: PrivateKey(secret.to_bytes()),
        }
    }

    /// Create a keypair from an existing private key.
    pub fn from_private(private: PrivateKey) -> Self {
        let public = private.public_key();
        Self { public, private }
    }
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keypair")
            .field("public", &self.public)
            .field("private", &"[REDACTED]")
            .finish()
    }
}

/// Public key file format (plaintext JSON).
#[derive(Serialize, Deserialize)]
struct PublicKeyFile {
    version: u8,
    public_key: PublicKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Save a private key to a file, encrypted with a passphrase.
///
/// The file format uses the MMPKEKEY format with Argon2id + AES-256-GCM.
///
/// # Arguments
///
/// * `key` - The private key to save
/// * `path` - File path to write to
/// * `passphrase` - Passphrase to encrypt the private key (min 12 characters)
///
/// # Errors
///
/// Returns an error if:
/// - The passphrase is too short (< 12 characters)
/// - File I/O fails
pub fn save_private_key(key: &PrivateKey, path: &Path, passphrase: &str) -> CryptoResult<()> {
    let encrypted = encrypt_private_key(key.as_bytes(), passphrase)?;
    std::fs::write(path, encrypted)?;
    Ok(())
}

/// Load a private key from an encrypted file.
///
/// Uses the MMPKEKEY format with Argon2id key derivation and
/// AES-256-GCM encryption.
///
/// # Arguments
///
/// * `path` - File path to read from
/// * `passphrase` - Passphrase to decrypt the private key
///
/// # Errors
///
/// Returns an error if:
/// - The passphrase is incorrect
/// - The file is corrupted or invalid
/// - File I/O fails
pub fn load_private_key(path: &Path, passphrase: &str) -> CryptoResult<PrivateKey> {
    let encrypted = std::fs::read(path)?;
    let bytes = decrypt_private_key(&encrypted, passphrase)?;
    Ok(PrivateKey(bytes))
}

/// Save a public key to a file (plaintext).
///
/// # Arguments
///
/// * `key` - The public key to save
/// * `path` - File path to write to
/// * `label` - Optional label/description for the key
pub fn save_public_key(key: &PublicKey, path: &Path, label: Option<&str>) -> CryptoResult<()> {
    let file = PublicKeyFile {
        version: 1,
        public_key: key.clone(),
        label: label.map(String::from),
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| CryptoError::InvalidFormat(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a public key from a file.
///
/// Supports both the JSON format (created by `save_public_key`) and
/// raw base64-encoded public keys.
pub fn load_public_key(path: &Path) -> CryptoResult<PublicKey> {
    let contents = std::fs::read_to_string(path)?;

    // Try JSON format first
    if let Ok(file) = serde_json::from_str::<PublicKeyFile>(&contents) {
        return Ok(file.public_key);
    }

    // Try raw base64
    let cleaned: String = contents.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &cleaned)
        .map_err(|e| CryptoError::InvalidKeyfile(e.to_string()))?;

    if bytes.len() != 32 {
        return Err(CryptoError::InvalidKeyfile(format!(
            "Expected 32 bytes, got {}",
            bytes.len()
        )));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(PublicKey(arr))
}

// Hex encoding for debug output (internal helper)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_keypair_generation() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();

        // Different keypairs should have different keys
        assert_ne!(kp1.public.as_bytes(), kp2.public.as_bytes());
        assert_ne!(kp1.private.as_bytes(), kp2.private.as_bytes());
    }

    #[test]
    fn test_private_key_derives_public() {
        let kp = Keypair::generate();
        let derived_public = kp.private.public_key();
        assert_eq!(kp.public.as_bytes(), derived_public.as_bytes());
    }

    #[test]
    fn test_keypair_from_private() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::from_private(kp1.private.clone());
        assert_eq!(kp1.public.as_bytes(), kp2.public.as_bytes());
    }

    #[test]
    fn test_save_load_private_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("private.key.enc");

        let kp = Keypair::generate();
        let passphrase = "secure-passphrase-123";

        // Save
        save_private_key(&kp.private, &path, passphrase).unwrap();

        // Load
        let loaded = load_private_key(&path, passphrase).unwrap();
        assert_eq!(kp.private.as_bytes(), loaded.as_bytes());
    }

    #[test]
    fn test_save_load_private_key_wrong_passphrase() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("private.key.enc");

        let kp = Keypair::generate();
        save_private_key(&kp.private, &path, "correct-passphrase").unwrap();

        let result = load_private_key(&path, "wrong-passphrase!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_save_load_public_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("public.key");

        let kp = Keypair::generate();

        // Save with label
        save_public_key(&kp.public, &path, Some("My Key")).unwrap();

        // Load
        let loaded = load_public_key(&path).unwrap();
        assert_eq!(kp.public.as_bytes(), loaded.as_bytes());
    }

    #[test]
    fn test_load_public_key_raw_base64() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("raw.pub");

        let kp = Keypair::generate();
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            kp.public.as_bytes(),
        );
        std::fs::write(&path, encoded).unwrap();

        let loaded = load_public_key(&path).unwrap();
        assert_eq!(kp.public.as_bytes(), loaded.as_bytes());
    }

    #[test]
    fn test_public_key_serialization() {
        let kp = Keypair::generate();
        let json = serde_json::to_string(&kp.public).unwrap();
        let parsed: PublicKey = serde_json::from_str(&json).unwrap();
        assert_eq!(kp.public.as_bytes(), parsed.as_bytes());
    }

    #[test]
    fn test_private_key_debug_redacted() {
        let kp = Keypair::generate();
        let debug = format!("{:?}", kp.private);
        assert!(debug.contains("REDACTED"));
        // Should not contain any key bytes
        assert!(!debug.contains("0x"));
    }

    #[test]
    fn test_keypair_debug() {
        let kp = Keypair::generate();
        let debug = format!("{:?}", kp);
        assert!(debug.contains("Keypair"));
        assert!(debug.contains("PublicKey"));
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn test_public_key_clone() {
        let kp = Keypair::generate();
        let cloned = kp.public.clone();
        assert_eq!(kp.public.as_bytes(), cloned.as_bytes());
    }

    #[test]
    fn test_private_key_clone() {
        let kp = Keypair::generate();
        let cloned = kp.private.clone();
        assert_eq!(kp.private.as_bytes(), cloned.as_bytes());
    }
}
