//! Standalone key storage for PKE private keys.
//!
//! This module provides passphrase-protected storage for X25519 private keys
//! using AES-256-GCM encryption with Argon2id key derivation.
//!
//! # Format: MMPKEKEY
//!
//! ```text
//! +------------------+
//! | Magic: MMPKEKEY  | 8 bytes
//! +------------------+
//! | Header Length    | 4 bytes (little-endian)
//! +------------------+
//! | Header (JSON)    | Variable
//! +------------------+
//! | Encrypted Key    | 48 bytes (32-byte key + 16-byte auth tag)
//! +------------------+
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cipher::{aes_gcm_decrypt, aes_gcm_encrypt, generate_nonce, generate_salt};
use crate::error::{CryptoError, CryptoResult};
use crate::kdf::{derive_key, KdfParams};

/// Magic bytes for PKE key file format.
pub const MAGIC_PKEKEY: &[u8; 8] = b"MMPKEKEY";

/// Header for encrypted private key files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkeKeyHeader {
    /// Format version.
    pub version: u8,
    /// KDF algorithm (always "argon2id").
    pub kdf: String,
    /// KDF parameters.
    pub kdf_params: KdfParams,
    /// Salt for key derivation (base64).
    pub salt: String,
    /// Nonce for encryption (base64).
    pub nonce: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Encrypt a private key (32 bytes) with a passphrase.
///
/// Returns encrypted data in MMPKEKEY format.
pub fn encrypt_private_key(key_bytes: &[u8; 32], passphrase: &str) -> CryptoResult<Vec<u8>> {
    // Generate random salt and nonce
    let salt = generate_salt();
    let nonce = generate_nonce();

    // Derive encryption key from passphrase
    let kdf_params = KdfParams::default();
    let derived = derive_key(passphrase.as_bytes(), &salt, &kdf_params)?;

    // Encrypt the private key
    let ciphertext = aes_gcm_encrypt(derived.as_bytes(), &nonce, key_bytes)?;

    // Build header
    let header = PkeKeyHeader {
        version: 1,
        kdf: "argon2id".to_string(),
        kdf_params,
        salt: base64_encode(&salt),
        nonce: base64_encode(&nonce),
        created_at: Utc::now(),
    };

    let header_json = serde_json::to_vec(&header)
        .map_err(|e| CryptoError::Encryption(format!("Header serialization failed: {}", e)))?;

    // Serialize: magic + header_len + header + ciphertext
    let header_len = (header_json.len() as u32).to_le_bytes();

    let mut output = Vec::with_capacity(8 + 4 + header_json.len() + ciphertext.len());
    output.extend_from_slice(MAGIC_PKEKEY);
    output.extend_from_slice(&header_len);
    output.extend_from_slice(&header_json);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decrypt a private key from MMPKEKEY format.
///
/// Returns the decrypted 32-byte private key.
pub fn decrypt_private_key(encrypted: &[u8], passphrase: &str) -> CryptoResult<[u8; 32]> {
    // Validate minimum length: magic(8) + header_len(4) + min_header + ciphertext(48)
    if encrypted.len() < 60 {
        return Err(CryptoError::Decryption("File too short".to_string()));
    }

    // Check magic
    if &encrypted[0..8] != MAGIC_PKEKEY {
        return Err(CryptoError::InvalidMagic);
    }

    // Parse header length
    let header_len = u32::from_le_bytes(
        encrypted[8..12]
            .try_into()
            .map_err(|_| CryptoError::Decryption("Invalid header length".to_string()))?,
    ) as usize;

    // Validate we have enough data
    if encrypted.len() < 12 + header_len + 48 {
        return Err(CryptoError::Decryption("File truncated".to_string()));
    }

    // Parse header
    let header: PkeKeyHeader = serde_json::from_slice(&encrypted[12..12 + header_len])
        .map_err(|e| CryptoError::Decryption(format!("Invalid header: {}", e)))?;

    // Decode salt and nonce
    let salt = base64_decode(&header.salt)?;
    let nonce = base64_decode(&header.nonce)?;

    if salt.len() != 32 {
        return Err(CryptoError::Decryption("Invalid salt length".to_string()));
    }
    if nonce.len() != 12 {
        return Err(CryptoError::Decryption("Invalid nonce length".to_string()));
    }

    let salt_arr: [u8; 32] = salt.try_into().unwrap();
    let nonce_arr: [u8; 12] = nonce.try_into().unwrap();

    // Derive key from passphrase
    let derived = derive_key(passphrase.as_bytes(), &salt_arr, &header.kdf_params)?;

    // Decrypt the private key
    let ciphertext = &encrypted[12 + header_len..];
    let decrypted = aes_gcm_decrypt(derived.as_bytes(), &nonce_arr, ciphertext)?;

    if decrypted.len() != 32 {
        return Err(CryptoError::Decryption(format!(
            "Invalid key length: expected 32, got {}",
            decrypted.len()
        )));
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&decrypted);
    Ok(key_bytes)
}

/// Check if data is a PKE key file (starts with MMPKEKEY magic).
pub fn is_pke_key_file(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..8] == MAGIC_PKEKEY
}

// Base64 helpers
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(data: &str) -> CryptoResult<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| CryptoError::Decryption(format!("Invalid base64: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let passphrase = "secure-passphrase-123";

        let encrypted = encrypt_private_key(&key, passphrase).unwrap();
        let decrypted = decrypt_private_key(&encrypted, passphrase).unwrap();

        assert_eq!(key, decrypted);
    }

    #[test]
    fn test_wrong_passphrase() {
        let key = [42u8; 32];
        let encrypted = encrypt_private_key(&key, "correct-passphrase").unwrap();

        let result = decrypt_private_key(&encrypted, "wrong-passphrase!");
        assert!(result.is_err());
    }

    #[test]
    fn test_magic_bytes() {
        let key = [42u8; 32];
        let encrypted = encrypt_private_key(&key, "secure-passphrase-123").unwrap();

        assert!(is_pke_key_file(&encrypted));
        assert_eq!(&encrypted[0..8], MAGIC_PKEKEY);
    }

    #[test]
    fn test_not_pke_key_file() {
        let data = b"random data that is not a key file";
        assert!(!is_pke_key_file(data));
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(b"INVALID!");

        let result = decrypt_private_key(&data, "passphrase");
        assert!(matches!(result, Err(CryptoError::InvalidMagic)));
    }

    #[test]
    fn test_file_too_short() {
        let data = b"short";
        let result = decrypt_private_key(data, "passphrase");
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext() {
        let key = [42u8; 32];
        let mut encrypted = encrypt_private_key(&key, "secure-passphrase-123").unwrap();

        // Tamper with the last byte
        let len = encrypted.len();
        encrypted[len - 1] ^= 0xFF;

        let result = decrypt_private_key(&encrypted, "secure-passphrase-123");
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_output() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let passphrase = "secure-passphrase-123";

        let encrypted1 = encrypt_private_key(&key1, passphrase).unwrap();
        let encrypted2 = encrypt_private_key(&key2, passphrase).unwrap();

        // Same passphrase but different keys should produce different ciphertext
        assert_ne!(encrypted1, encrypted2);
    }
}
