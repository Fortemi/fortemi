//! End-to-end multi-recipient encryption for secure shard sharing.

use chrono::Utc;
use zeroize::Zeroize;

use crate::cipher::{aes_gcm_decrypt, aes_gcm_encrypt, generate_nonce, generate_salt};
use crate::error::{CryptoError, CryptoResult};
use crate::format::{
    base64_decode, base64_encode, serialize_encrypted, E2EHeader, EncryptedFile, FileFormat,
    Recipient,
};
use crate::kdf::{derive_key, validate_passphrase, KdfParams};

/// A recipient with their passphrase for E2E encryption.
#[derive(Debug, Clone)]
pub struct RecipientInput {
    /// Recipient identifier (e.g., name, email, username).
    pub id: String,
    /// Recipient's passphrase.
    pub passphrase: String,
}

/// Maximum number of recipients allowed.
pub const MAX_RECIPIENTS: usize = 10;

/// Encrypt data for multiple recipients using envelope encryption.
///
/// # Algorithm
///
/// 1. Generate random Data Encryption Key (DEK)
/// 2. Encrypt data with DEK using AES-256-GCM
/// 3. For each recipient:
///    - Derive Key Encryption Key (KEK) from their passphrase
///    - Encrypt DEK with KEK
/// 4. Store all encrypted DEKs in header
///
/// Any recipient can decrypt using their passphrase.
pub fn encrypt_e2e(
    plaintext: &[u8],
    recipients: &[RecipientInput],
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    if recipients.is_empty() {
        return Err(CryptoError::NoMatchingRecipient);
    }

    if recipients.len() > MAX_RECIPIENTS {
        return Err(CryptoError::InvalidRecipientId(format!(
            "Maximum {} recipients allowed",
            MAX_RECIPIENTS
        )));
    }

    // Validate all passphrases
    for recipient in recipients {
        validate_passphrase(&recipient.passphrase)?;
        validate_recipient_id(&recipient.id)?;
    }

    // Step 1: Generate random DEK
    let mut dek: [u8; 32] = crate::cipher::generate_random();
    let data_nonce = generate_nonce();

    // Step 2: Encrypt data with DEK
    let ciphertext = aes_gcm_encrypt(&dek, &data_nonce, plaintext)?;

    // Step 3: For each recipient, encrypt the DEK with their KEK
    let mut recipient_entries = Vec::with_capacity(recipients.len());

    for recipient in recipients {
        let salt = generate_salt();
        let dek_nonce = generate_nonce();
        let kdf_params = KdfParams::default();

        // Derive KEK from recipient's passphrase
        let kek = derive_key(recipient.passphrase.as_bytes(), &salt, &kdf_params)?;

        // Encrypt DEK with KEK
        let encrypted_dek = aes_gcm_encrypt(kek.as_bytes(), &dek_nonce, &dek)?;

        recipient_entries.push(Recipient {
            id: recipient.id.clone(),
            kdf: "argon2id".to_string(),
            kdf_params,
            salt: base64_encode(&salt),
            encrypted_dek: base64_encode(&encrypted_dek),
            dek_nonce: base64_encode(&dek_nonce),
        });
    }

    // Step 4: Build header
    let header = E2EHeader {
        version: 1,
        algorithm: "AES-256-GCM".to_string(),
        dek_algorithm: "AES-256-GCM".to_string(),
        recipients: recipient_entries,
        data_nonce: base64_encode(&data_nonce),
        created_at: Utc::now(),
        original_filename,
    };

    let header_json = serde_json::to_vec(&header)?;

    // Zeroize DEK
    dek.zeroize();

    Ok(serialize_encrypted(
        FileFormat::E2E,
        &header_json,
        &ciphertext,
    ))
}

/// Decrypt E2E encrypted file using recipient's passphrase.
///
/// # Arguments
///
/// * `encrypted` - The encrypted file bytes
/// * `recipient_id` - The recipient identifier
/// * `passphrase` - The recipient's passphrase
pub fn decrypt_e2e(
    encrypted: &[u8],
    recipient_id: &str,
    passphrase: &str,
) -> CryptoResult<(Vec<u8>, E2EHeader)> {
    let file = EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::E2E {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_e2e_header()?;

    // Validate version
    if header.version != 1 {
        return Err(CryptoError::UnsupportedVersion(header.version));
    }

    // Find matching recipient
    let recipient = header
        .recipients
        .iter()
        .find(|r| r.id == recipient_id)
        .ok_or(CryptoError::NoMatchingRecipient)?;

    // Derive KEK from passphrase
    let salt_bytes = base64_decode(&recipient.salt)?;
    let salt: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid salt size".into()))?;

    let kek = derive_key(passphrase.as_bytes(), &salt, &recipient.kdf_params)?;

    // Decrypt DEK
    let encrypted_dek = base64_decode(&recipient.encrypted_dek)?;
    let dek_nonce_bytes = base64_decode(&recipient.dek_nonce)?;
    let dek_nonce: [u8; 12] = dek_nonce_bytes
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid DEK nonce size".into()))?;

    let dek_bytes = aes_gcm_decrypt(kek.as_bytes(), &dek_nonce, &encrypted_dek)?;

    if dek_bytes.len() != 32 {
        return Err(CryptoError::Decryption);
    }

    let mut dek = [0u8; 32];
    dek.copy_from_slice(&dek_bytes);

    // Decrypt data with DEK
    let data_nonce_bytes = base64_decode(&header.data_nonce)?;
    let data_nonce: [u8; 12] = data_nonce_bytes
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid data nonce size".into()))?;

    let plaintext = aes_gcm_decrypt(&dek, &data_nonce, &file.ciphertext)?;

    // Zeroize DEK
    dek.zeroize();

    Ok((plaintext, header))
}

/// Try to decrypt with any matching recipient (auto-detect).
///
/// Tries each recipient's slot until one succeeds or all fail.
/// Returns the decrypted data, header, and the matching recipient ID.
pub fn decrypt_e2e_auto(
    encrypted: &[u8],
    passphrase: &str,
) -> CryptoResult<(Vec<u8>, E2EHeader, String)> {
    let file = EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::E2E {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_e2e_header()?;

    // Try each recipient
    for recipient in &header.recipients {
        match decrypt_e2e(encrypted, &recipient.id, passphrase) {
            Ok((plaintext, header)) => {
                return Ok((plaintext, header, recipient.id.clone()));
            }
            Err(CryptoError::Decryption) | Err(CryptoError::Authentication) => {
                // Try next recipient
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(CryptoError::NoMatchingRecipient)
}

/// Get recipient IDs from an E2E encrypted file without decrypting.
pub fn get_recipient_ids(encrypted: &[u8]) -> CryptoResult<Vec<String>> {
    let file = EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::E2E {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_e2e_header()?;
    Ok(header.recipients.iter().map(|r| r.id.clone()).collect())
}

/// Validate recipient ID format.
fn validate_recipient_id(id: &str) -> CryptoResult<()> {
    if id.is_empty() {
        return Err(CryptoError::InvalidRecipientId("ID cannot be empty".into()));
    }
    if id.len() > 64 {
        return Err(CryptoError::InvalidRecipientId(
            "ID cannot exceed 64 characters".into(),
        ));
    }
    // Allow alphanumeric, underscore, dash, dot, @ (for emails)
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '@')
    {
        return Err(CryptoError::InvalidRecipientId(
            "ID contains invalid characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_recipients(names: &[&str]) -> Vec<RecipientInput> {
        names
            .iter()
            .map(|name| RecipientInput {
                id: name.to_string(),
                passphrase: format!("{}-secure-passphrase", name),
            })
            .collect()
    }

    #[test]
    fn test_e2e_single_recipient() {
        let plaintext = b"Secret shared data";
        let recipients = make_recipients(&["alice"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let (decrypted, header) =
            decrypt_e2e(&encrypted, "alice", "alice-secure-passphrase").unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(header.recipients.len(), 1);
        assert_eq!(header.recipients[0].id, "alice");
    }

    #[test]
    fn test_e2e_multi_recipient() {
        let plaintext = b"Shared secret for team";
        let recipients = make_recipients(&["alice", "bob", "carol"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();

        // Alice can decrypt
        let (dec_alice, _) = decrypt_e2e(&encrypted, "alice", "alice-secure-passphrase").unwrap();
        assert_eq!(plaintext.as_slice(), dec_alice.as_slice());

        // Bob can decrypt
        let (dec_bob, _) = decrypt_e2e(&encrypted, "bob", "bob-secure-passphrase").unwrap();
        assert_eq!(plaintext.as_slice(), dec_bob.as_slice());

        // Carol can decrypt
        let (dec_carol, _) = decrypt_e2e(&encrypted, "carol", "carol-secure-passphrase").unwrap();
        assert_eq!(plaintext.as_slice(), dec_carol.as_slice());
    }

    #[test]
    fn test_e2e_wrong_passphrase() {
        let plaintext = b"Secret data";
        let recipients = make_recipients(&["alice"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let result = decrypt_e2e(&encrypted, "alice", "wrong-passphrase!!");

        assert!(matches!(result, Err(CryptoError::Decryption)));
    }

    #[test]
    fn test_e2e_wrong_recipient_id() {
        let plaintext = b"Secret data";
        let recipients = make_recipients(&["alice"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let result = decrypt_e2e(&encrypted, "bob", "alice-secure-passphrase");

        assert!(matches!(result, Err(CryptoError::NoMatchingRecipient)));
    }

    #[test]
    fn test_e2e_auto_decrypt() {
        let plaintext = b"Auto-detect recipient";
        let recipients = make_recipients(&["alice", "bob"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();

        // Bob's passphrase
        let (decrypted, _, found_id) =
            decrypt_e2e_auto(&encrypted, "bob-secure-passphrase").unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
        assert_eq!(found_id, "bob");
    }

    #[test]
    fn test_e2e_auto_decrypt_no_match() {
        let plaintext = b"Secret data";
        let recipients = make_recipients(&["alice", "bob"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let result = decrypt_e2e_auto(&encrypted, "wrong-passphrase!!");

        assert!(matches!(result, Err(CryptoError::NoMatchingRecipient)));
    }

    #[test]
    fn test_e2e_no_recipients() {
        let plaintext = b"test";
        let result = encrypt_e2e(plaintext, &[], None);

        assert!(matches!(result, Err(CryptoError::NoMatchingRecipient)));
    }

    #[test]
    fn test_e2e_too_many_recipients() {
        let plaintext = b"test";
        let recipients: Vec<RecipientInput> = (0..11)
            .map(|i| RecipientInput {
                id: format!("user{}", i),
                passphrase: format!("passphrase-for-user-{}", i),
            })
            .collect();

        let result = encrypt_e2e(plaintext, &recipients, None);
        assert!(matches!(result, Err(CryptoError::InvalidRecipientId(_))));
    }

    #[test]
    fn test_e2e_with_filename() {
        let plaintext = b"Shared shard";
        let recipients = make_recipients(&["alice"]);

        let encrypted =
            encrypt_e2e(plaintext, &recipients, Some("shared.shard".to_string())).unwrap();
        let (_, header) = decrypt_e2e(&encrypted, "alice", "alice-secure-passphrase").unwrap();

        assert_eq!(header.original_filename, Some("shared.shard".to_string()));
    }

    #[test]
    fn test_get_recipient_ids() {
        let plaintext = b"test";
        let recipients = make_recipients(&["alice", "bob", "carol"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let ids = get_recipient_ids(&encrypted).unwrap();

        assert_eq!(ids, vec!["alice", "bob", "carol"]);
    }

    #[test]
    fn test_validate_recipient_id_empty() {
        let result = validate_recipient_id("");
        assert!(matches!(result, Err(CryptoError::InvalidRecipientId(_))));
    }

    #[test]
    fn test_validate_recipient_id_too_long() {
        let long_id = "a".repeat(65);
        let result = validate_recipient_id(&long_id);
        assert!(matches!(result, Err(CryptoError::InvalidRecipientId(_))));
    }

    #[test]
    fn test_validate_recipient_id_valid() {
        assert!(validate_recipient_id("alice").is_ok());
        assert!(validate_recipient_id("alice_bob").is_ok());
        assert!(validate_recipient_id("alice-bob").is_ok());
        assert!(validate_recipient_id("alice.bob").is_ok());
        assert!(validate_recipient_id("alice@example.com").is_ok());
        assert!(validate_recipient_id("Alice123").is_ok());
    }

    #[test]
    fn test_validate_recipient_id_invalid_chars() {
        let result = validate_recipient_id("alice bob");
        assert!(matches!(result, Err(CryptoError::InvalidRecipientId(_))));

        let result = validate_recipient_id("alice<script>");
        assert!(matches!(result, Err(CryptoError::InvalidRecipientId(_))));
    }

    #[test]
    fn test_e2e_empty_data() {
        let plaintext = b"";
        let recipients = make_recipients(&["alice"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let (decrypted, _) = decrypt_e2e(&encrypted, "alice", "alice-secure-passphrase").unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_e2e_large_data() {
        let plaintext = vec![42u8; 1024 * 1024]; // 1 MiB
        let recipients = make_recipients(&["alice"]);

        let encrypted = encrypt_e2e(&plaintext, &recipients, None).unwrap();
        let (decrypted, _) = decrypt_e2e(&encrypted, "alice", "alice-secure-passphrase").unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_e2e_each_recipient_has_unique_encrypted_dek() {
        let plaintext = b"test";
        let recipients = make_recipients(&["alice", "bob"]);

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();
        let file = EncryptedFile::parse(&encrypted).unwrap();
        let header = file.parse_e2e_header().unwrap();

        // Each recipient should have different encrypted DEK
        assert_ne!(
            header.recipients[0].encrypted_dek,
            header.recipients[1].encrypted_dek
        );

        // And different salts
        assert_ne!(header.recipients[0].salt, header.recipients[1].salt);
    }
}
