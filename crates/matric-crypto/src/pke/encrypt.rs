//! Public-key encryption and decryption implementation.
//!
//! This module provides the high-level encrypt and decrypt functions
//! for the MMPKE01 format.
//!
//! # Encryption Flow
//!
//! 1. Generate ephemeral X25519 keypair
//! 2. Generate random DEK (Data Encryption Key)
//! 3. For each recipient:
//!    a. Compute shared secret via ECDH
//!    b. Derive KEK (Key Encryption Key) via HKDF
//!    c. Encrypt DEK with KEK using AES-256-GCM
//! 4. Encrypt plaintext with DEK using AES-256-GCM
//! 5. Serialize MMPKE01 format
//!
//! # Decryption Flow
//!
//! 1. Parse MMPKE01 header
//! 2. Find recipient block matching our address
//! 3. Compute shared secret via ECDH
//! 4. Derive KEK via HKDF
//! 5. Decrypt DEK using KEK
//! 6. Decrypt ciphertext using DEK

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use zeroize::Zeroize;

use crate::error::{CryptoError, CryptoResult};
use crate::pke::address::Address;
use crate::pke::ecdh::{derive_kek, derive_kek_for_decrypt};
use crate::pke::format::{parse_header, serialize_header, PkeHeader, RecipientBlock};
use crate::pke::keys::{Keypair, PrivateKey, PublicKey};

/// Encrypt data for multiple recipients using public-key encryption.
///
/// # Arguments
///
/// * `plaintext` - The data to encrypt
/// * `recipients` - Public keys of the recipients
/// * `original_filename` - Optional filename for metadata
///
/// # Returns
///
/// The encrypted data in MMPKE01 format.
///
/// # Example
///
/// ```rust
/// use matric_crypto::pke::{encrypt_pke, Keypair};
///
/// let alice = Keypair::generate();
/// let bob = Keypair::generate();
///
/// let plaintext = b"Secret message";
/// let encrypted = encrypt_pke(plaintext, &[alice.public.clone(), bob.public.clone()], None).unwrap();
/// ```
pub fn encrypt_pke(
    plaintext: &[u8],
    recipients: &[PublicKey],
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    if recipients.is_empty() {
        return Err(CryptoError::InvalidInput(
            "At least one recipient required".to_string(),
        ));
    }

    if recipients.len() > 100 {
        return Err(CryptoError::InvalidInput(
            "Maximum 100 recipients allowed".to_string(),
        ));
    }

    let mut rng = rand::thread_rng();

    // Generate ephemeral keypair
    let ephemeral = Keypair::generate();

    // Generate random DEK
    let mut dek = [0u8; 32];
    rng.fill_bytes(&mut dek);

    // Generate data nonce
    let mut data_nonce = [0u8; 12];
    rng.fill_bytes(&mut data_nonce);

    // Encrypt DEK for each recipient
    let mut recipient_blocks = Vec::with_capacity(recipients.len());
    for recipient_pubkey in recipients {
        let address = recipient_pubkey.to_address();

        // Derive KEK for this recipient
        let kek = derive_kek(&ephemeral.private, recipient_pubkey, &ephemeral.public);

        // Generate nonce for DEK encryption
        let mut dek_nonce = [0u8; 12];
        rng.fill_bytes(&mut dek_nonce);

        // Encrypt DEK with KEK
        let cipher = Aes256Gcm::new_from_slice(kek.as_bytes())
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;
        let nonce = Nonce::from_slice(&dek_nonce);
        let encrypted_dek = cipher
            .encrypt(nonce, dek.as_slice())
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        recipient_blocks.push(RecipientBlock {
            address,
            encrypted_dek,
            dek_nonce,
        });
    }

    // Encrypt plaintext with DEK
    let cipher =
        Aes256Gcm::new_from_slice(&dek).map_err(|e| CryptoError::Encryption(e.to_string()))?;
    let nonce = Nonce::from_slice(&data_nonce);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    // Zeroize DEK
    dek.zeroize();

    // Build header
    let header = PkeHeader::new(
        ephemeral.public,
        recipient_blocks,
        data_nonce,
        original_filename,
    );

    // Serialize
    let mut output = serialize_header(&header)?;
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decrypt data using a private key.
///
/// # Arguments
///
/// * `ciphertext` - The encrypted data in MMPKE01 format
/// * `private_key` - The recipient's private key
///
/// # Returns
///
/// A tuple of (plaintext, header) on success.
///
/// # Errors
///
/// Returns an error if:
/// - The ciphertext is not valid MMPKE01 format
/// - The private key doesn't match any recipient
/// - The ciphertext has been tampered with
///
/// # Example
///
/// ```rust
/// use matric_crypto::pke::{encrypt_pke, decrypt_pke, Keypair};
///
/// let alice = Keypair::generate();
/// let plaintext = b"Secret message";
///
/// let encrypted = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();
/// let (decrypted, header) = decrypt_pke(&encrypted, &alice.private).unwrap();
///
/// assert_eq!(plaintext.as_slice(), decrypted.as_slice());
/// ```
pub fn decrypt_pke(
    ciphertext: &[u8],
    private_key: &PrivateKey,
) -> CryptoResult<(Vec<u8>, PkeHeader)> {
    // Parse header
    let (header, encrypted_data) = parse_header(ciphertext)?;

    // Derive our address from the private key
    let our_pubkey = private_key.public_key();
    let our_address = our_pubkey.to_address();

    // Find our recipient block
    let recipient_block = header.find_recipient(&our_address).ok_or_else(|| {
        CryptoError::Decryption(format!(
            "No recipient block found for address {}",
            our_address
        ))
    })?;

    // Derive KEK
    let kek = derive_kek_for_decrypt(private_key, &header.ephemeral_pubkey);

    // Decrypt DEK
    let cipher = Aes256Gcm::new_from_slice(kek.as_bytes())
        .map_err(|e| CryptoError::Decryption(e.to_string()))?;
    let nonce = Nonce::from_slice(&recipient_block.dek_nonce);
    let dek_bytes = cipher
        .decrypt(nonce, recipient_block.encrypted_dek.as_slice())
        .map_err(|_| CryptoError::Decryption("Failed to decrypt DEK - wrong key?".to_string()))?;

    if dek_bytes.len() != 32 {
        return Err(CryptoError::Decryption(format!(
            "Invalid DEK length: expected 32, got {}",
            dek_bytes.len()
        )));
    }

    let mut dek = [0u8; 32];
    dek.copy_from_slice(&dek_bytes);

    // Decrypt data
    let cipher =
        Aes256Gcm::new_from_slice(&dek).map_err(|e| CryptoError::Decryption(e.to_string()))?;
    let nonce = Nonce::from_slice(&header.data_nonce);
    let plaintext = cipher
        .decrypt(nonce, encrypted_data)
        .map_err(|_| CryptoError::Decryption("Failed to decrypt data - corrupted?".to_string()))?;

    // Zeroize DEK
    dek.zeroize();

    Ok((plaintext, header))
}

/// Get the list of recipient addresses from an encrypted file without decrypting.
///
/// # Arguments
///
/// * `ciphertext` - The encrypted data in MMPKE01 format
///
/// # Returns
///
/// A list of recipient addresses.
pub fn get_pke_recipients(ciphertext: &[u8]) -> CryptoResult<Vec<Address>> {
    let (header, _) = parse_header(ciphertext)?;
    Ok(header
        .recipients
        .iter()
        .map(|r| r.address.clone())
        .collect())
}

/// Check if a private key can decrypt the given ciphertext.
///
/// This is useful for finding which key to use when you have multiple.
pub fn can_decrypt_pke(ciphertext: &[u8], private_key: &PrivateKey) -> bool {
    let (header, _) = match parse_header(ciphertext) {
        Ok(h) => h,
        Err(_) => return false,
    };

    let our_pubkey = private_key.public_key();
    let our_address = our_pubkey.to_address();

    header.find_recipient(&our_address).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_single_recipient() {
        let alice = Keypair::generate();
        let plaintext = b"Hello, Alice!";

        let encrypted = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();
        let (decrypted, _header) = decrypt_pke(&encrypted, &alice.private).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_multiple_recipients() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();
        let carol = Keypair::generate();

        let plaintext = b"Hello, everyone!";
        let recipients = vec![
            alice.public.clone(),
            bob.public.clone(),
            carol.public.clone(),
        ];

        let encrypted = encrypt_pke(plaintext, &recipients, Some("message.txt".into())).unwrap();

        // All recipients can decrypt
        let (decrypted_alice, _) = decrypt_pke(&encrypted, &alice.private).unwrap();
        let (decrypted_bob, _) = decrypt_pke(&encrypted, &bob.private).unwrap();
        let (decrypted_carol, header) = decrypt_pke(&encrypted, &carol.private).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted_alice.as_slice());
        assert_eq!(plaintext.as_slice(), decrypted_bob.as_slice());
        assert_eq!(plaintext.as_slice(), decrypted_carol.as_slice());
        assert_eq!(header.original_filename, Some("message.txt".to_string()));
    }

    #[test]
    fn test_encrypt_decrypt_wrong_key() {
        let alice = Keypair::generate();
        let eve = Keypair::generate();

        let plaintext = b"Secret for Alice only";
        let encrypted = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();

        // Eve cannot decrypt
        let result = decrypt_pke(&encrypted, &eve.private);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_no_recipients() {
        let result = encrypt_pke(b"data", &[], None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("recipient"));
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let alice = Keypair::generate();
        let encrypted = encrypt_pke(b"", std::slice::from_ref(&alice.public), None).unwrap();
        let (decrypted, _) = decrypt_pke(&encrypted, &alice.private).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_encrypt_large_plaintext() {
        let alice = Keypair::generate();
        let plaintext = vec![42u8; 1024 * 1024]; // 1 MB

        let encrypted = encrypt_pke(&plaintext, std::slice::from_ref(&alice.public), None).unwrap();
        let (decrypted, _) = decrypt_pke(&encrypted, &alice.private).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_get_pke_recipients() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let encrypted =
            encrypt_pke(b"data", &[alice.public.clone(), bob.public.clone()], None).unwrap();

        let recipients = get_pke_recipients(&encrypted).unwrap();
        assert_eq!(recipients.len(), 2);

        let alice_addr = alice.public.to_address();
        let bob_addr = bob.public.to_address();

        assert!(recipients.contains(&alice_addr));
        assert!(recipients.contains(&bob_addr));
    }

    #[test]
    fn test_can_decrypt_pke() {
        let alice = Keypair::generate();
        let bob = Keypair::generate();

        let encrypted = encrypt_pke(b"data", std::slice::from_ref(&alice.public), None).unwrap();

        assert!(can_decrypt_pke(&encrypted, &alice.private));
        assert!(!can_decrypt_pke(&encrypted, &bob.private));
    }

    #[test]
    fn test_tamper_detection() {
        let alice = Keypair::generate();
        let plaintext = b"Important data";

        let mut encrypted =
            encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();

        // Tamper with the ciphertext (last byte)
        let len = encrypted.len();
        encrypted[len - 1] ^= 0xFF;

        // Decryption should fail
        let result = decrypt_pke(&encrypted, &alice.private);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_preserves_metadata() {
        let alice = Keypair::generate();
        let encrypted = encrypt_pke(
            b"data",
            std::slice::from_ref(&alice.public),
            Some("backup.json".into()),
        )
        .unwrap();

        let (_, header) = decrypt_pke(&encrypted, &alice.private).unwrap();

        assert_eq!(header.original_filename, Some("backup.json".to_string()));
        assert!(header.created_at.is_some());
        assert_eq!(header.version, 1);
    }

    #[test]
    fn test_encrypt_decrypt_binary_data() {
        let alice = Keypair::generate();

        // Binary data with all byte values
        let plaintext: Vec<u8> = (0..=255).collect();

        let encrypted = encrypt_pke(&plaintext, std::slice::from_ref(&alice.public), None).unwrap();
        let (decrypted, _) = decrypt_pke(&encrypted, &alice.private).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_encryptions_different_ciphertext() {
        let alice = Keypair::generate();
        let plaintext = b"Same message";

        let encrypted1 = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();
        let encrypted2 = encrypt_pke(plaintext, std::slice::from_ref(&alice.public), None).unwrap();

        // Different ephemeral keys and nonces = different ciphertext
        assert_ne!(encrypted1, encrypted2);

        // But both decrypt to same plaintext
        let (decrypted1, _) = decrypt_pke(&encrypted1, &alice.private).unwrap();
        let (decrypted2, _) = decrypt_pke(&encrypted2, &alice.private).unwrap();

        assert_eq!(decrypted1, decrypted2);
        assert_eq!(plaintext.as_slice(), decrypted1.as_slice());
    }
}
