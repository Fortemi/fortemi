//! Key derivation using Argon2id.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{CryptoError, CryptoResult};

/// Minimum passphrase length.
pub const MIN_PASSPHRASE_LENGTH: usize = 12;

/// Argon2id parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory in KiB (default: 65536 = 64 MiB).
    pub memory_kib: u32,
    /// Time iterations (default: 3).
    pub iterations: u32,
    /// Parallelism degree (default: 4).
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 65536, // 64 MiB
            iterations: 3,
            parallelism: 4,
        }
    }
}

impl KdfParams {
    /// Create low-memory parameters (for resource-constrained environments).
    pub fn low_memory() -> Self {
        Self {
            memory_kib: 32768, // 32 MiB
            iterations: 4,
            parallelism: 4,
        }
    }

    /// Create high-security parameters (for long-term archives).
    pub fn high_security() -> Self {
        Self {
            memory_kib: 131072, // 128 MiB
            iterations: 4,
            parallelism: 4,
        }
    }
}

/// Key wrapper with automatic zeroization on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey {
    key: [u8; 32],
}

impl DerivedKey {
    /// Create a new derived key from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { key: bytes }
    }

    /// Get the key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

impl std::fmt::Debug for DerivedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DerivedKey")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

/// Derive a 256-bit key from passphrase using Argon2id.
pub fn derive_key(
    passphrase: &[u8],
    salt: &[u8; 32],
    params: &KdfParams,
) -> CryptoResult<DerivedKey> {
    if passphrase.len() < MIN_PASSPHRASE_LENGTH {
        return Err(CryptoError::PassphraseTooShort(MIN_PASSPHRASE_LENGTH));
    }

    let argon2_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(32),
    )
    .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

    Ok(DerivedKey { key })
}

/// Load key from keyfile (raw 32 bytes or base64-encoded).
pub fn load_keyfile(path: &std::path::Path) -> CryptoResult<DerivedKey> {
    let contents = std::fs::read(path)?;

    let key = if contents.len() == 32 {
        // Raw 32-byte key
        let mut key = [0u8; 32];
        key.copy_from_slice(&contents);
        key
    } else {
        // Try base64 decode (strip whitespace first)
        let cleaned: String = String::from_utf8_lossy(&contents)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&cleaned)
            .map_err(|e| CryptoError::InvalidKeyfile(e.to_string()))?;

        if decoded.len() != 32 {
            return Err(CryptoError::InvalidKeyfile(format!(
                "Expected 32 bytes, got {}",
                decoded.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        key
    };

    Ok(DerivedKey { key })
}

/// Generate a random keyfile and save it.
pub fn generate_keyfile(path: &std::path::Path) -> CryptoResult<()> {
    use rand::RngCore;

    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);

    // Write as base64 for easier handling
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    std::fs::write(path, encoded)?;

    // Zeroize the key in memory
    key.zeroize();

    Ok(())
}

/// Validate passphrase strength.
pub fn validate_passphrase(passphrase: &str) -> CryptoResult<()> {
    if passphrase.len() < MIN_PASSPHRASE_LENGTH {
        return Err(CryptoError::PassphraseTooShort(MIN_PASSPHRASE_LENGTH));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_kdf_params_default() {
        let params = KdfParams::default();
        assert_eq!(params.memory_kib, 65536);
        assert_eq!(params.iterations, 3);
        assert_eq!(params.parallelism, 4);
    }

    #[test]
    fn test_kdf_params_low_memory() {
        let params = KdfParams::low_memory();
        assert_eq!(params.memory_kib, 32768);
    }

    #[test]
    fn test_kdf_params_high_security() {
        let params = KdfParams::high_security();
        assert_eq!(params.memory_kib, 131072);
    }

    #[test]
    fn test_derive_key_success() {
        let passphrase = b"my-secure-passphrase-123";
        let salt = [0u8; 32];
        let params = KdfParams::default();

        let key = derive_key(passphrase, &salt, &params);
        assert!(key.is_ok());
        assert_eq!(key.unwrap().as_bytes().len(), 32);
    }

    #[test]
    fn test_derive_key_deterministic() {
        let passphrase = b"my-secure-passphrase-123";
        let salt = [42u8; 32];
        let params = KdfParams::default();

        let key1 = derive_key(passphrase, &salt, &params).unwrap();
        let key2 = derive_key(passphrase, &salt, &params).unwrap();

        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_different_salts() {
        let passphrase = b"my-secure-passphrase-123";
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];
        let params = KdfParams::default();

        let key1 = derive_key(passphrase, &salt1, &params).unwrap();
        let key2 = derive_key(passphrase, &salt2, &params).unwrap();

        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_passphrase_too_short() {
        let passphrase = b"short";
        let salt = [0u8; 32];
        let params = KdfParams::default();

        let result = derive_key(passphrase, &salt, &params);
        assert!(matches!(result, Err(CryptoError::PassphraseTooShort(_))));
    }

    #[test]
    fn test_derived_key_debug_redacted() {
        let key = DerivedKey::from_bytes([0u8; 32]);
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("REDACTED"));
        assert!(!debug_str.contains("0"));
    }

    #[test]
    fn test_generate_and_load_keyfile() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.key");

        // Generate keyfile
        generate_keyfile(&path).unwrap();

        // Verify file exists
        assert!(path.exists());

        // Load keyfile
        let key = load_keyfile(&path).unwrap();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_load_keyfile_raw() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("raw.key");

        // Write raw 32-byte key
        let raw_key = [42u8; 32];
        std::fs::write(&path, raw_key).unwrap();

        let key = load_keyfile(&path).unwrap();
        assert_eq!(key.as_bytes(), &raw_key);
    }

    #[test]
    fn test_load_keyfile_invalid_size() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("invalid.key");

        // Write wrong size
        std::fs::write(&path, [0u8; 16]).unwrap();

        let result = load_keyfile(&path);
        assert!(matches!(result, Err(CryptoError::InvalidKeyfile(_))));
    }

    #[test]
    fn test_validate_passphrase_success() {
        assert!(validate_passphrase("my-long-passphrase").is_ok());
    }

    #[test]
    fn test_validate_passphrase_too_short() {
        let result = validate_passphrase("short");
        assert!(matches!(result, Err(CryptoError::PassphraseTooShort(_))));
    }

    #[test]
    fn test_kdf_params_serialization() {
        let params = KdfParams::default();
        let json = serde_json::to_string(&params).unwrap();
        let parsed: KdfParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, parsed);
    }
}
