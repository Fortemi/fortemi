//! AES-256-GCM cipher operations.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

use crate::error::{CryptoError, CryptoResult};

/// Generate cryptographically secure random bytes.
pub fn generate_random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// Generate a random salt (32 bytes).
pub fn generate_salt() -> [u8; 32] {
    generate_random()
}

/// Generate a random nonce (12 bytes).
pub fn generate_nonce() -> [u8; 12] {
    generate_random()
}

/// Encrypt plaintext with AES-256-GCM.
///
/// Returns ciphertext with appended authentication tag (16 bytes).
pub fn aes_gcm_encrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> CryptoResult<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let nonce = Nonce::from_slice(nonce);

    cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Encryption("AES-GCM encryption failed".into()))
}

/// Decrypt ciphertext with AES-256-GCM.
///
/// The ciphertext must include the authentication tag (16 bytes) at the end.
pub fn aes_gcm_decrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> CryptoResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CryptoError::Decryption("Invalid key".to_string()))?;

    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decryption("AES-GCM decryption failed".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_salt() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();

        assert_eq!(salt1.len(), 32);
        assert_eq!(salt2.len(), 32);
        assert_ne!(salt1, salt2); // Should be random
    }

    #[test]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();

        assert_eq!(nonce1.len(), 12);
        assert_eq!(nonce2.len(), 12);
        assert_ne!(nonce1, nonce2); // Should be random
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let nonce = [1u8; 12];
        let plaintext = b"Hello, World!";

        let ciphertext = aes_gcm_encrypt(&key, &nonce, plaintext).unwrap();
        let decrypted = aes_gcm_decrypt(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_ciphertext_longer_than_plaintext() {
        let key = [42u8; 32];
        let nonce = [1u8; 12];
        let plaintext = b"Hello, World!";

        let ciphertext = aes_gcm_encrypt(&key, &nonce, plaintext).unwrap();

        // Ciphertext should be plaintext + 16 byte auth tag
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let key1 = [42u8; 32];
        let key2 = [99u8; 32];
        let nonce = [1u8; 12];
        let plaintext = b"Secret data";

        let ciphertext = aes_gcm_encrypt(&key1, &nonce, plaintext).unwrap();
        let result = aes_gcm_decrypt(&key2, &nonce, &ciphertext);

        assert!(matches!(result, Err(CryptoError::Decryption(_))));
    }

    #[test]
    fn test_decrypt_wrong_nonce() {
        let key = [42u8; 32];
        let nonce1 = [1u8; 12];
        let nonce2 = [2u8; 12];
        let plaintext = b"Secret data";

        let ciphertext = aes_gcm_encrypt(&key, &nonce1, plaintext).unwrap();
        let result = aes_gcm_decrypt(&key, &nonce2, &ciphertext);

        assert!(matches!(result, Err(CryptoError::Decryption(_))));
    }

    #[test]
    fn test_decrypt_tampered_ciphertext() {
        let key = [42u8; 32];
        let nonce = [1u8; 12];
        let plaintext = b"Secret data";

        let mut ciphertext = aes_gcm_encrypt(&key, &nonce, plaintext).unwrap();

        // Tamper with the ciphertext
        ciphertext[0] ^= 0xFF;

        let result = aes_gcm_decrypt(&key, &nonce, &ciphertext);
        assert!(matches!(result, Err(CryptoError::Decryption(_))));
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let key = [42u8; 32];
        let nonce = [1u8; 12];
        let plaintext = b"";

        let ciphertext = aes_gcm_encrypt(&key, &nonce, plaintext).unwrap();
        let decrypted = aes_gcm_decrypt(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_large_plaintext() {
        let key = [42u8; 32];
        let nonce = [1u8; 12];
        let plaintext = vec![0u8; 1024 * 1024]; // 1 MiB

        let ciphertext = aes_gcm_encrypt(&key, &nonce, &plaintext).unwrap();
        let decrypted = aes_gcm_decrypt(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_nonces_different_ciphertext() {
        let key = [42u8; 32];
        let nonce1 = [1u8; 12];
        let nonce2 = [2u8; 12];
        let plaintext = b"Same message";

        let ciphertext1 = aes_gcm_encrypt(&key, &nonce1, plaintext).unwrap();
        let ciphertext2 = aes_gcm_encrypt(&key, &nonce2, plaintext).unwrap();

        assert_ne!(ciphertext1, ciphertext2);
    }
}
