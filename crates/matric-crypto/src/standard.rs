//! Standard single-key encryption for archives and shards.

use chrono::Utc;

use crate::cipher::{aes_gcm_decrypt, aes_gcm_encrypt, generate_nonce, generate_salt};
use crate::error::{CryptoError, CryptoResult};
use crate::format::{
    base64_decode, base64_encode, serialize_encrypted, EncryptedFile, FileFormat, Header, KeyType,
};
use crate::kdf::{derive_key, load_keyfile, validate_passphrase, DerivedKey, KdfParams};

/// Options for standard encryption.
#[derive(Debug, Clone, Default)]
pub struct EncryptOptions {
    /// Type of key used.
    pub key_type: Option<KeyType>,
    /// Original filename to store in header.
    pub original_filename: Option<String>,
    /// Custom KDF parameters.
    pub kdf_params: Option<KdfParams>,
}

/// Encrypt data using standard single-key encryption.
///
/// Returns the encrypted file bytes in MMENC01 format.
pub fn encrypt_standard(
    plaintext: &[u8],
    key: &DerivedKey,
    salt: &[u8; 32],
    options: EncryptOptions,
) -> CryptoResult<Vec<u8>> {
    let nonce = generate_nonce();
    let kdf_params = options.kdf_params.unwrap_or_default();

    // Encrypt data
    let ciphertext = aes_gcm_encrypt(key.as_bytes(), &nonce, plaintext)?;

    // Build header
    let header = Header {
        version: 1,
        algorithm: "AES-256-GCM".to_string(),
        kdf: "argon2id".to_string(),
        kdf_params,
        salt: base64_encode(salt),
        nonce: base64_encode(&nonce),
        key_type: options.key_type.unwrap_or(KeyType::Passphrase),
        created_at: Utc::now(),
        original_filename: options.original_filename,
    };

    let header_json = serde_json::to_vec(&header)?;

    Ok(serialize_encrypted(
        FileFormat::Standard,
        &header_json,
        &ciphertext,
    ))
}

/// Encrypt data with passphrase (derives key internally).
pub fn encrypt_with_passphrase(
    plaintext: &[u8],
    passphrase: &str,
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    validate_passphrase(passphrase)?;

    let salt = generate_salt();
    let kdf_params = KdfParams::default();
    let key = derive_key(passphrase.as_bytes(), &salt, &kdf_params)?;

    encrypt_standard(
        plaintext,
        &key,
        &salt,
        EncryptOptions {
            key_type: Some(KeyType::Passphrase),
            original_filename,
            kdf_params: Some(kdf_params),
        },
    )
}

/// Encrypt data with keyfile.
pub fn encrypt_with_keyfile(
    plaintext: &[u8],
    keyfile_path: &std::path::Path,
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    let key = load_keyfile(keyfile_path)?;

    // For keyfile, we still use a random salt but it's not used for derivation
    let salt = generate_salt();

    encrypt_standard(
        plaintext,
        &key,
        &salt,
        EncryptOptions {
            key_type: Some(KeyType::Keyfile),
            original_filename,
            kdf_params: None,
        },
    )
}

/// Key source for decryption.
pub enum KeySource<'a> {
    /// Derive key from passphrase.
    Passphrase(&'a str),
    /// Load key from file.
    Keyfile(&'a std::path::Path),
    /// Use pre-derived key.
    DerivedKey(DerivedKey),
}

/// Decrypt standard encrypted file.
///
/// Returns the decrypted plaintext and the file header.
pub fn decrypt_standard(
    encrypted: &[u8],
    key_source: KeySource<'_>,
) -> CryptoResult<(Vec<u8>, Header)> {
    let file = EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::Standard {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_standard_header()?;

    // Validate version
    if header.version != 1 {
        return Err(CryptoError::UnsupportedVersion(header.version));
    }

    // Get the key
    let key = match key_source {
        KeySource::Passphrase(passphrase) => {
            let salt_bytes = base64_decode(&header.salt)?;
            let salt: [u8; 32] = salt_bytes
                .try_into()
                .map_err(|_| CryptoError::HeaderParse("Invalid salt size".into()))?;
            derive_key(passphrase.as_bytes(), &salt, &header.kdf_params)?
        }
        KeySource::Keyfile(path) => load_keyfile(path)?,
        KeySource::DerivedKey(key) => key,
    };

    // Decode nonce
    let nonce_bytes = base64_decode(&header.nonce)?;
    let nonce: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid nonce size".into()))?;

    // Decrypt
    let plaintext = aes_gcm_decrypt(key.as_bytes(), &nonce, &file.ciphertext)?;

    Ok((plaintext, header))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encrypt_decrypt_passphrase_roundtrip() {
        let plaintext = b"Hello, World! This is a test message.";
        let passphrase = "my-secure-passphrase-123";

        let encrypted = encrypt_with_passphrase(plaintext, passphrase, None).unwrap();
        let (decrypted, header) =
            decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(header.version, 1);
        assert_eq!(header.algorithm, "AES-256-GCM");
        assert_eq!(header.key_type, KeyType::Passphrase);
    }

    #[test]
    fn test_encrypt_with_filename() {
        let plaintext = b"test data";
        let passphrase = "my-secure-passphrase-123";
        let filename = "backup.tar.gz";

        let encrypted =
            encrypt_with_passphrase(plaintext, passphrase, Some(filename.to_string())).unwrap();
        let (_, header) = decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(header.original_filename, Some(filename.to_string()));
    }

    #[test]
    fn test_encrypt_decrypt_keyfile_roundtrip() {
        let dir = tempdir().unwrap();
        let keyfile = dir.path().join("test.key");

        // Generate keyfile
        crate::kdf::generate_keyfile(&keyfile).unwrap();

        let plaintext = b"Secret data encrypted with keyfile";
        let encrypted = encrypt_with_keyfile(plaintext, &keyfile, None).unwrap();
        let (decrypted, header) =
            decrypt_standard(&encrypted, KeySource::Keyfile(&keyfile)).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(header.key_type, KeyType::Keyfile);
    }

    #[test]
    fn test_wrong_passphrase() {
        let plaintext = b"Secret data";
        let encrypted = encrypt_with_passphrase(plaintext, "correct-passphrase", None).unwrap();

        let result = decrypt_standard(&encrypted, KeySource::Passphrase("wrong-passphrase"));
        assert!(matches!(result, Err(CryptoError::Decryption)));
    }

    #[test]
    fn test_passphrase_too_short() {
        let plaintext = b"test";
        let result = encrypt_with_passphrase(plaintext, "short", None);
        assert!(matches!(result, Err(CryptoError::PassphraseTooShort(_))));
    }

    #[test]
    fn test_encrypt_empty_data() {
        let plaintext = b"";
        let passphrase = "my-secure-passphrase-123";

        let encrypted = encrypt_with_passphrase(plaintext, passphrase, None).unwrap();
        let (decrypted, _) =
            decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_large_data() {
        let plaintext = vec![0u8; 1024 * 1024]; // 1 MiB
        let passphrase = "my-secure-passphrase-123";

        let encrypted = encrypt_with_passphrase(&plaintext, passphrase, None).unwrap();
        let (decrypted, _) =
            decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_passphrases_different_ciphertext() {
        let plaintext = b"Same message";

        let enc1 = encrypt_with_passphrase(plaintext, "passphrase-one-123", None).unwrap();
        let enc2 = encrypt_with_passphrase(plaintext, "passphrase-two-456", None).unwrap();

        // Different because of random salt and nonce
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn test_same_passphrase_different_ciphertext() {
        let plaintext = b"Same message";
        let passphrase = "same-passphrase-123";

        let enc1 = encrypt_with_passphrase(plaintext, passphrase, None).unwrap();
        let enc2 = encrypt_with_passphrase(plaintext, passphrase, None).unwrap();

        // Different because of random salt and nonce
        assert_ne!(enc1, enc2);

        // But both decrypt to same plaintext
        let (dec1, _) = decrypt_standard(&enc1, KeySource::Passphrase(passphrase)).unwrap();
        let (dec2, _) = decrypt_standard(&enc2, KeySource::Passphrase(passphrase)).unwrap();
        assert_eq!(dec1, dec2);
    }

    #[test]
    fn test_custom_kdf_params() {
        let plaintext = b"Test with custom KDF";
        let passphrase = "my-secure-passphrase-123";
        let salt = generate_salt();
        let kdf_params = KdfParams::low_memory();
        let key = derive_key(passphrase.as_bytes(), &salt, &kdf_params).unwrap();

        let encrypted = encrypt_standard(
            plaintext,
            &key,
            &salt,
            EncryptOptions {
                key_type: Some(KeyType::Passphrase),
                original_filename: None,
                kdf_params: Some(kdf_params.clone()),
            },
        )
        .unwrap();

        let (decrypted, header) =
            decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(header.kdf_params.memory_kib, kdf_params.memory_kib);
    }
}
