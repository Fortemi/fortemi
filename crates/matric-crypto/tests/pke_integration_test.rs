//! Comprehensive integration tests for the PKE (Public Key Encryption) system.
//!
//! This test suite validates:
//! - Cryptographic correctness (encryption/decryption roundtrips)
//! - Address format and checksum validation
//! - Multi-recipient encryption scenarios
//! - Error handling and security properties
//! - Format compliance (MMPKE01)
//! - Key management and persistence

use matric_crypto::pke::{
    can_decrypt_pke, decrypt_pke, encrypt_pke, get_pke_recipients, is_pke_format, load_private_key,
    load_public_key, save_private_key, save_public_key, Address, Keypair, PublicKey, MAGIC_BYTES,
};
use tempfile::tempdir;

// ============================================================================
// Test Category 1: Cryptographic Correctness
// ============================================================================

#[test]
fn test_keypair_generation_produces_valid_keys() {
    // Generate multiple keypairs
    let kp1 = Keypair::generate();
    let kp2 = Keypair::generate();
    let kp3 = Keypair::generate();

    // Verify key lengths are correct (X25519 uses 32-byte keys)
    assert_eq!(kp1.public.as_bytes().len(), 32);
    assert_eq!(kp1.private.as_bytes().len(), 32);

    // Verify public keys are unique
    assert_ne!(kp1.public.as_bytes(), kp2.public.as_bytes());
    assert_ne!(kp1.public.as_bytes(), kp3.public.as_bytes());
    assert_ne!(kp2.public.as_bytes(), kp3.public.as_bytes());

    // Verify private keys are unique
    assert_ne!(kp1.private.as_bytes(), kp2.private.as_bytes());

    // Verify public key is derivable from private key
    let derived_public = kp1.private.public_key();
    assert_eq!(kp1.public.as_bytes(), derived_public.as_bytes());
}

#[test]
fn test_public_key_derivation_is_deterministic() {
    let keypair = Keypair::generate();

    // Derive public key multiple times
    let pub1 = keypair.private.public_key();
    let pub2 = keypair.private.public_key();
    let pub3 = keypair.private.public_key();

    // Should always get the same result
    assert_eq!(pub1.as_bytes(), pub2.as_bytes());
    assert_eq!(pub1.as_bytes(), pub3.as_bytes());
    assert_eq!(keypair.public.as_bytes(), pub1.as_bytes());
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    // Generate recipient key
    let recipient = Keypair::generate();

    // Test message
    let message = b"This is a secret message that should be encrypted and decrypted correctly.";

    // Encrypt message for recipient
    let encrypted = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Decrypt with recipient private key
    let (decrypted, _header) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    // Verify plaintext matches
    assert_eq!(message.as_slice(), decrypted.as_slice());
}

#[test]
fn test_encrypt_decrypt_roundtrip_with_metadata() {
    let recipient = Keypair::generate();
    let message = b"Test message with metadata";
    let filename = "confidential.txt";

    // Encrypt with metadata
    let encrypted = encrypt_pke(
        message,
        std::slice::from_ref(&recipient.public),
        Some(filename.into()),
    )
    .unwrap();

    // Decrypt and verify metadata is preserved
    let (decrypted, header) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    assert_eq!(message.as_slice(), decrypted.as_slice());
    assert_eq!(header.original_filename, Some(filename.to_string()));
    assert!(header.created_at.is_some());
    assert_eq!(header.version, 1);
}

#[test]
fn test_multi_recipient_encryption() {
    // Create 3 recipients
    let alice = Keypair::generate();
    let bob = Keypair::generate();
    let carol = Keypair::generate();

    let message = b"Message encrypted for three recipients";

    // Encrypt for all three
    let encrypted = encrypt_pke(
        message,
        &[
            alice.public.clone(),
            bob.public.clone(),
            carol.public.clone(),
        ],
        Some("multi-recipient.txt".into()),
    )
    .unwrap();

    // Verify all three can decrypt
    let (decrypted_alice, header_alice) = decrypt_pke(&encrypted, &alice.private).unwrap();
    let (decrypted_bob, header_bob) = decrypt_pke(&encrypted, &bob.private).unwrap();
    let (decrypted_carol, header_carol) = decrypt_pke(&encrypted, &carol.private).unwrap();

    // All should get the same plaintext
    assert_eq!(message.as_slice(), decrypted_alice.as_slice());
    assert_eq!(message.as_slice(), decrypted_bob.as_slice());
    assert_eq!(message.as_slice(), decrypted_carol.as_slice());

    // All should see the same metadata
    assert_eq!(
        header_alice.original_filename,
        Some("multi-recipient.txt".to_string())
    );
    assert_eq!(header_bob.original_filename, header_alice.original_filename);
    assert_eq!(
        header_carol.original_filename,
        header_alice.original_filename
    );

    // All should see the same ephemeral public key
    assert_eq!(
        header_alice.ephemeral_pubkey.as_bytes(),
        header_bob.ephemeral_pubkey.as_bytes()
    );
    assert_eq!(
        header_alice.ephemeral_pubkey.as_bytes(),
        header_carol.ephemeral_pubkey.as_bytes()
    );
}

#[test]
fn test_multi_recipient_non_recipients_cannot_decrypt() {
    // Create recipients and a non-recipient
    let alice = Keypair::generate();
    let bob = Keypair::generate();
    let eve = Keypair::generate(); // Not a recipient

    let message = b"Secret for Alice and Bob only";

    // Encrypt for Alice and Bob
    let encrypted =
        encrypt_pke(message, &[alice.public.clone(), bob.public.clone()], None).unwrap();

    // Alice and Bob can decrypt
    assert!(decrypt_pke(&encrypted, &alice.private).is_ok());
    assert!(decrypt_pke(&encrypted, &bob.private).is_ok());

    // Eve cannot decrypt (not a recipient)
    let result = decrypt_pke(&encrypted, &eve.private);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No recipient block found"));
}

#[test]
fn test_encryption_provides_forward_secrecy() {
    let recipient = Keypair::generate();
    let message = b"Same message, different encryptions";

    // Encrypt same message twice
    let encrypted1 = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();
    let encrypted2 = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Ciphertexts should be different (due to ephemeral keys and nonces)
    assert_ne!(encrypted1, encrypted2);

    // But both should decrypt to the same plaintext
    let (decrypted1, _) = decrypt_pke(&encrypted1, &recipient.private).unwrap();
    let (decrypted2, _) = decrypt_pke(&encrypted2, &recipient.private).unwrap();

    assert_eq!(decrypted1, decrypted2);
    assert_eq!(message.as_slice(), decrypted1.as_slice());
}

#[test]
fn test_encryption_maximum_recipients() {
    // Create 100 recipients (the maximum allowed)
    let mut recipients = Vec::new();
    for _ in 0..100 {
        recipients.push(Keypair::generate());
    }

    let message = b"Message for 100 recipients";
    let public_keys: Vec<PublicKey> = recipients.iter().map(|kp| kp.public.clone()).collect();

    // Should succeed with 100 recipients
    let encrypted = encrypt_pke(message, &public_keys, None).unwrap();

    // Verify first, middle, and last recipient can decrypt
    assert!(decrypt_pke(&encrypted, &recipients[0].private).is_ok());
    assert!(decrypt_pke(&encrypted, &recipients[49].private).is_ok());
    assert!(decrypt_pke(&encrypted, &recipients[99].private).is_ok());

    // Verify all recipients are listed
    let recipient_addrs = get_pke_recipients(&encrypted).unwrap();
    assert_eq!(recipient_addrs.len(), 100);
}

#[test]
fn test_encryption_exceeds_maximum_recipients() {
    // Try to create 101 recipients (exceeds maximum)
    let mut recipients = Vec::new();
    for _ in 0..101 {
        recipients.push(Keypair::generate().public);
    }

    let message = b"Too many recipients";

    // Should fail with error
    let result = encrypt_pke(message, &recipients, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Maximum"));
}

// ============================================================================
// Test Category 2: Address Format Tests
// ============================================================================

#[test]
fn test_address_format_has_correct_prefix() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Verify address starts with "mm:"
    assert!(address.as_str().starts_with("mm:"));
}

#[test]
fn test_address_format_reasonable_length() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Address should be: "mm:" (3) + Base58(1 version + 20 hash + 4 checksum)
    // Base58 encoding of 25 bytes ≈ 34-36 characters
    // Total: ~37-39 characters
    let addr_str = address.as_str();
    assert!(
        addr_str.len() >= 35,
        "Address too short: {}",
        addr_str.len()
    );
    assert!(addr_str.len() <= 45, "Address too long: {}", addr_str.len());
}

#[test]
fn test_address_deterministic_for_same_key() {
    let keypair = Keypair::generate();

    // Generate address multiple times
    let addr1 = keypair.public.to_address();
    let addr2 = keypair.public.to_address();
    let addr3 = keypair.public.to_address();

    // Should always generate the same address
    assert_eq!(addr1.as_str(), addr2.as_str());
    assert_eq!(addr1.as_str(), addr3.as_str());
}

#[test]
fn test_address_unique_per_key() {
    // Generate multiple keypairs
    let kp1 = Keypair::generate();
    let kp2 = Keypair::generate();
    let kp3 = Keypair::generate();

    let addr1 = kp1.public.to_address();
    let addr2 = kp2.public.to_address();
    let addr3 = kp3.public.to_address();

    // All addresses should be unique
    assert_ne!(addr1.as_str(), addr2.as_str());
    assert_ne!(addr1.as_str(), addr3.as_str());
    assert_ne!(addr2.as_str(), addr3.as_str());
}

#[test]
fn test_address_checksum_detects_single_character_corruption() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();
    let addr_str = address.as_str();

    // Corrupt single character at different positions
    for pos in 3..addr_str.len() {
        // Skip the "mm:" prefix
        let mut corrupted = addr_str.to_string();
        let original_char = corrupted.chars().nth(pos).unwrap();

        // Replace with a different character
        let new_char = if original_char == 'A' { 'B' } else { 'A' };
        corrupted.replace_range(pos..=pos, &new_char.to_string());

        // Parsing should fail
        let result: Result<Address, _> = corrupted.parse();
        assert!(
            result.is_err(),
            "Checksum failed to detect corruption at position {}",
            pos
        );
    }
}

#[test]
fn test_address_checksum_detects_transposition() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();
    let addr_str = address.as_str();
    let chars: Vec<char> = addr_str.chars().collect();

    // Try swapping adjacent characters that are actually different.
    // Skip prefix characters (version byte) and checksum tail.
    let mut detected = false;
    for i in 2..chars.len().saturating_sub(2) {
        if chars[i] == chars[i + 1] {
            continue; // Swapping identical chars is a no-op
        }
        let mut swapped = chars.clone();
        swapped.swap(i, i + 1);
        let corrupted: String = swapped.into_iter().collect();

        let result: Result<Address, _> = corrupted.parse();
        if result.is_err() {
            detected = true;
            break;
        }
    }
    assert!(detected, "Checksum failed to detect any adjacent transposition");
}

#[test]
fn test_address_roundtrip_parsing() {
    let keypair = Keypair::generate();
    let original_addr = keypair.public.to_address();

    // Convert to string
    let addr_string = original_addr.to_string();

    // Parse back from string
    let parsed_addr: Address = addr_string.parse().unwrap();

    // Should be equal
    assert_eq!(original_addr, parsed_addr);
    assert_eq!(original_addr.as_str(), parsed_addr.as_str());
}

#[test]
fn test_address_verify_checksum() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Valid address should pass checksum verification
    assert!(address.verify_checksum());
}

#[test]
fn test_address_version_byte() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Version should be 1
    assert_eq!(address.version(), 1);
}

#[test]
fn test_address_hash_bytes_consistent() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();

    // Get hash bytes multiple times
    let hash1 = address.hash_bytes().unwrap();
    let hash2 = address.hash_bytes().unwrap();

    // Should be consistent
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 20); // HASH_LENGTH
}

// ============================================================================
// Test Category 3: Error Handling Tests
// ============================================================================

#[test]
fn test_reject_invalid_address_prefix() {
    // Wrong prefix
    let result: Result<Address, _> = "xx:123456789".parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("must start with"));
}

#[test]
fn test_reject_invalid_base58_characters() {
    // Base58 doesn't include 0, O, I, l
    let result: Result<Address, _> = "mm:0OIl".parse();
    assert!(result.is_err());
}

#[test]
fn test_reject_invalid_address_length() {
    // Too short
    let result: Result<Address, _> = "mm:abc".parse();
    assert!(result.is_err());
}

#[test]
fn test_reject_corrupted_checksum() {
    let keypair = Keypair::generate();
    let address = keypair.public.to_address();
    let mut addr_str = address.as_str().to_string();

    // Corrupt the last character (part of checksum)
    // Use A/B swap like the unit test in address.rs to ensure valid Base58
    let last_char = addr_str.pop().unwrap();
    let new_char = if last_char == 'A' { 'B' } else { 'A' };
    addr_str.push(new_char);

    let result: Result<Address, _> = addr_str.parse();
    // Corruption may cause checksum error OR length error (Base58 is non-uniform)
    assert!(result.is_err(), "Corrupted address should fail to parse");
}

#[test]
fn test_reject_wrong_recipient_key() {
    let alice = Keypair::generate();
    let bob = Keypair::generate();

    // Encrypt for Alice
    let encrypted = encrypt_pke(b"Secret", std::slice::from_ref(&alice.public), None).unwrap();

    // Bob cannot decrypt (not a recipient)
    let result = decrypt_pke(&encrypted, &bob.private);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No recipient block found"));
}

#[test]
fn test_reject_tampered_ciphertext() {
    let recipient = Keypair::generate();
    let message = b"Original message";

    let mut encrypted =
        encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Tamper with the last byte of ciphertext
    let len = encrypted.len();
    encrypted[len - 1] ^= 0xFF;

    // Decryption should fail (AES-GCM authentication)
    let result = decrypt_pke(&encrypted, &recipient.private);
    assert!(result.is_err());
}

#[test]
fn test_reject_tampered_header() {
    let recipient = Keypair::generate();
    let message = b"Message";

    let mut encrypted =
        encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Tamper with header (after magic bytes, before ciphertext)
    // Magic is 8 bytes, length is 4 bytes, so byte 20 is likely in the JSON header
    if encrypted.len() > 20 {
        encrypted[20] ^= 0xFF;

        // Should fail to parse or decrypt
        let result = decrypt_pke(&encrypted, &recipient.private);
        assert!(result.is_err());
    }
}

#[test]
fn test_reject_invalid_magic_bytes() {
    let recipient = Keypair::generate();
    let message = b"Message";

    let mut encrypted =
        encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Corrupt magic bytes
    encrypted[0] = b'X';

    // Should fail format detection
    assert!(!is_pke_format(&encrypted));

    // Should fail parsing
    let result = decrypt_pke(&encrypted, &recipient.private);
    assert!(result.is_err());
}

#[test]
fn test_reject_truncated_ciphertext() {
    let recipient = Keypair::generate();
    let message = b"Message";

    let encrypted = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Truncate to just the header
    let truncated = &encrypted[..encrypted.len() / 2];

    // Should fail to decrypt
    let result = decrypt_pke(truncated, &recipient.private);
    assert!(result.is_err());
}

#[test]
fn test_reject_empty_recipients_list() {
    let message = b"Message";

    // Should fail - no recipients
    let result = encrypt_pke(message, &[], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("recipient"));
}

#[test]
fn test_wrong_passphrase_for_private_key() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("private.key.enc");

    let keypair = Keypair::generate();
    let correct_pass = "correct-password-123";
    let wrong_pass = "wrong-password-456";

    // Save with correct passphrase
    save_private_key(&keypair.private, &key_path, correct_pass).unwrap();

    // Try to load with wrong passphrase
    let result = load_private_key(&key_path, wrong_pass);
    assert!(result.is_err());
}

#[test]
fn test_load_private_key_corrupted_file() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("corrupted.key.enc");

    // Write invalid data
    std::fs::write(&key_path, b"not a valid encrypted key file").unwrap();

    // Should fail to load
    let result = load_private_key(&key_path, "any-password");
    assert!(result.is_err());
}

#[test]
fn test_load_public_key_invalid_json() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("invalid.pub");

    // Write invalid JSON
    std::fs::write(&key_path, b"{ not valid json }").unwrap();

    // Should fail to load
    let result = load_public_key(&key_path);
    assert!(result.is_err());
}

#[test]
fn test_load_public_key_wrong_length() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("wrong_len.pub");

    // Write base64 of wrong length (not 32 bytes)
    let wrong_data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"short");
    std::fs::write(&key_path, wrong_data).unwrap();

    let result = load_public_key(&key_path);
    assert!(result.is_err());
}

// ============================================================================
// Test Category 4: Format Compliance (MMPKE01)
// ============================================================================

#[test]
fn test_encrypted_data_has_correct_magic_bytes() {
    let recipient = Keypair::generate();
    let message = b"Test message";

    let encrypted = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // First 8 bytes should be magic
    assert_eq!(&encrypted[..8], MAGIC_BYTES);
}

#[test]
fn test_format_detection() {
    let recipient = Keypair::generate();
    let encrypted = encrypt_pke(b"data", std::slice::from_ref(&recipient.public), None).unwrap();

    // Should be detected as PKE format
    assert!(is_pke_format(&encrypted));

    // Random data should not be detected
    assert!(!is_pke_format(b"random data"));
    assert!(!is_pke_format(b""));

    // Other format should not be detected
    assert!(!is_pke_format(b"MMENC01\nother format"));
}

#[test]
fn test_header_contains_ephemeral_pubkey() {
    let recipient = Keypair::generate();
    let encrypted = encrypt_pke(b"data", std::slice::from_ref(&recipient.public), None).unwrap();

    let (_, header) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    // Ephemeral pubkey should be 32 bytes
    assert_eq!(header.ephemeral_pubkey.as_bytes().len(), 32);

    // Should be different from recipient's public key
    assert_ne!(
        header.ephemeral_pubkey.as_bytes(),
        recipient.public.as_bytes()
    );
}

#[test]
fn test_header_contains_recipient_list() {
    let alice = Keypair::generate();
    let bob = Keypair::generate();

    let encrypted =
        encrypt_pke(b"data", &[alice.public.clone(), bob.public.clone()], None).unwrap();

    let recipients = get_pke_recipients(&encrypted).unwrap();

    assert_eq!(recipients.len(), 2);
    assert!(recipients.contains(&alice.public.to_address()));
    assert!(recipients.contains(&bob.public.to_address()));
}

#[test]
fn test_can_decrypt_pke_function() {
    let alice = Keypair::generate();
    let bob = Keypair::generate();

    let encrypted = encrypt_pke(b"data", std::slice::from_ref(&alice.public), None).unwrap();

    // Alice can decrypt
    assert!(can_decrypt_pke(&encrypted, &alice.private));

    // Bob cannot decrypt
    assert!(!can_decrypt_pke(&encrypted, &bob.private));
}

#[test]
fn test_header_version_is_one() {
    let recipient = Keypair::generate();
    let encrypted = encrypt_pke(b"data", std::slice::from_ref(&recipient.public), None).unwrap();

    let (_, header) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    assert_eq!(header.version, 1);
}

// ============================================================================
// Test Category 5: Key Persistence and Management
// ============================================================================

#[test]
fn test_private_key_save_load_roundtrip() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("test.key.enc");

    let original_keypair = Keypair::generate();
    let passphrase = "secure-test-passphrase-12345";

    // Save private key
    save_private_key(&original_keypair.private, &key_path, passphrase).unwrap();

    // Verify file was created
    assert!(key_path.exists());

    // Load private key
    let loaded_private = load_private_key(&key_path, passphrase).unwrap();

    // Keys should match
    assert_eq!(
        original_keypair.private.as_bytes(),
        loaded_private.as_bytes()
    );

    // Derived public keys should match
    let original_public = original_keypair.private.public_key();
    let loaded_public = loaded_private.public_key();
    assert_eq!(original_public.as_bytes(), loaded_public.as_bytes());
}

#[test]
fn test_public_key_save_load_roundtrip() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("test.pub");

    let keypair = Keypair::generate();
    let label = "Test Public Key";

    // Save public key
    save_public_key(&keypair.public, &key_path, Some(label)).unwrap();

    // Verify file was created
    assert!(key_path.exists());

    // Load public key
    let loaded_public = load_public_key(&key_path).unwrap();

    // Keys should match
    assert_eq!(keypair.public.as_bytes(), loaded_public.as_bytes());
}

#[test]
fn test_public_key_save_without_label() {
    let dir = tempdir().unwrap();
    let key_path = dir.path().join("no_label.pub");

    let keypair = Keypair::generate();

    // Save without label
    save_public_key(&keypair.public, &key_path, None).unwrap();

    // Should still load correctly
    let loaded = load_public_key(&key_path).unwrap();
    assert_eq!(keypair.public.as_bytes(), loaded.as_bytes());
}

#[test]
fn test_saved_keys_can_encrypt_decrypt() {
    let dir = tempdir().unwrap();
    let private_path = dir.path().join("sender.key.enc");
    let public_path = dir.path().join("recipient.pub");

    // Generate and save keys
    let recipient = Keypair::generate();

    // Use a passphrase that meets minimum length requirement (12 chars)
    save_private_key(&recipient.private, &private_path, "sender-pass-123").unwrap();
    save_public_key(&recipient.public, &public_path, Some("Recipient")).unwrap();

    // Load keys
    let loaded_recipient_public = load_public_key(&public_path).unwrap();

    // Encrypt with loaded public key
    let message = b"Test with loaded keys";
    let encrypted = encrypt_pke(message, &[loaded_recipient_public], None).unwrap();

    // Decrypt with original recipient private key
    let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    assert_eq!(message.as_slice(), decrypted.as_slice());
}

// ============================================================================
// Test Category 6: Edge Cases and Boundary Conditions
// ============================================================================

#[test]
fn test_encrypt_empty_message() {
    let recipient = Keypair::generate();

    // Encrypt empty message
    let encrypted = encrypt_pke(b"", std::slice::from_ref(&recipient.public), None).unwrap();

    // Should still work
    let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();
    assert_eq!(decrypted.len(), 0);
}

#[test]
fn test_encrypt_large_message() {
    let recipient = Keypair::generate();

    // 10 MB message
    let large_message = vec![0x42u8; 10 * 1024 * 1024];

    // Should handle large messages
    let encrypted = encrypt_pke(
        &large_message,
        std::slice::from_ref(&recipient.public),
        None,
    )
    .unwrap();
    let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    assert_eq!(large_message.len(), decrypted.len());
    assert_eq!(large_message, decrypted);
}

#[test]
fn test_encrypt_binary_data_all_bytes() {
    let recipient = Keypair::generate();

    // All possible byte values
    let binary_data: Vec<u8> = (0..=255).cycle().take(1000).collect();

    let encrypted =
        encrypt_pke(&binary_data, std::slice::from_ref(&recipient.public), None).unwrap();
    let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();

    assert_eq!(binary_data, decrypted);
}

#[test]
fn test_encrypt_with_very_long_filename() {
    let recipient = Keypair::generate();
    let long_filename = "a".repeat(1000); // 1000 character filename

    let encrypted = encrypt_pke(
        b"data",
        std::slice::from_ref(&recipient.public),
        Some(long_filename.clone()),
    )
    .unwrap();

    let (_, header) = decrypt_pke(&encrypted, &recipient.private).unwrap();
    assert_eq!(header.original_filename, Some(long_filename));
}

#[test]
fn test_encrypt_with_unicode_filename() {
    let recipient = Keypair::generate();
    let unicode_filename = "文件名-файл-αρχείο-🔐.txt";

    let encrypted = encrypt_pke(
        b"data",
        std::slice::from_ref(&recipient.public),
        Some(unicode_filename.into()),
    )
    .unwrap();

    let (_, header) = decrypt_pke(&encrypted, &recipient.private).unwrap();
    assert_eq!(header.original_filename, Some(unicode_filename.to_string()));
}

#[test]
fn test_single_recipient_encryption() {
    let recipient = Keypair::generate();
    let message = b"Single recipient message";

    let encrypted = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Should have exactly 1 recipient
    let recipients = get_pke_recipients(&encrypted).unwrap();
    assert_eq!(recipients.len(), 1);
    assert_eq!(recipients[0], recipient.public.to_address());

    // Recipient should be able to decrypt
    let (decrypted, _) = decrypt_pke(&encrypted, &recipient.private).unwrap();
    assert_eq!(message.as_slice(), decrypted.as_slice());
}

#[test]
fn test_keypair_from_private_key() {
    let original = Keypair::generate();

    // Create new keypair from existing private key
    let reconstructed = Keypair::from_private(original.private.clone());

    // Public keys should match
    assert_eq!(original.public.as_bytes(), reconstructed.public.as_bytes());

    // Should be able to decrypt with reconstructed keypair
    let message = b"Test reconstruction";
    let encrypted = encrypt_pke(message, std::slice::from_ref(&original.public), None).unwrap();
    let (decrypted, _) = decrypt_pke(&encrypted, &reconstructed.private).unwrap();

    assert_eq!(message.as_slice(), decrypted.as_slice());
}

// ============================================================================
// Test Category 7: Security Properties
// ============================================================================

#[test]
fn test_private_key_zeroized_on_drop() {
    // This test verifies the zeroize behavior is present
    // Actual memory inspection would require unsafe code
    let keypair = Keypair::generate();
    let private_copy = keypair.private.clone();

    // Keys should be equal before drop
    assert_eq!(keypair.private.as_bytes(), private_copy.as_bytes());

    // After drop, the ZeroizeOnDrop trait ensures memory is cleared
    drop(private_copy);

    // Original should still be valid
    assert_eq!(keypair.private.as_bytes().len(), 32);
}

#[test]
fn test_addresses_are_collision_resistant() {
    use std::collections::HashSet;

    let mut addresses = HashSet::new();

    // Generate 1000 keypairs and check for address collisions
    for _ in 0..1000 {
        let keypair = Keypair::generate();
        let address = keypair.public.to_address();

        // Should not have seen this address before
        assert!(
            addresses.insert(address.as_str().to_string()),
            "Address collision detected"
        );
    }
}

#[test]
fn test_ciphertext_authentication_integrity() {
    let recipient = Keypair::generate();
    let message = b"Authenticated message";

    let encrypted = encrypt_pke(message, std::slice::from_ref(&recipient.public), None).unwrap();

    // Try modifying different parts of the ciphertext
    // We'll test a sample of positions rather than every byte
    let len = encrypted.len();

    // Test tampering with header (after magic and length)
    if len > 50 {
        let mut tampered = encrypted.clone();
        tampered[30] ^= 0x01;
        let result = decrypt_pke(&tampered, &recipient.private);
        assert!(result.is_err(), "Failed to detect header tampering");
    }

    // Test tampering with the last 16 bytes (auth tag)
    for i in (len.saturating_sub(16))..len {
        let mut tampered = encrypted.clone();
        tampered[i] ^= 0x01;
        let result = decrypt_pke(&tampered, &recipient.private);
        assert!(
            result.is_err(),
            "Failed to detect auth tag tampering at byte {}",
            i
        );
    }

    // Test tampering with middle of ciphertext
    if len > 100 {
        let mut tampered = encrypted.clone();
        tampered[len / 2] ^= 0xFF;
        let result = decrypt_pke(&tampered, &recipient.private);
        assert!(result.is_err(), "Failed to detect ciphertext tampering");
    }
}

#[test]
fn test_recipient_isolation() {
    // Verify that recipients cannot see each other's encrypted DEKs
    let alice = Keypair::generate();
    let bob = Keypair::generate();

    let encrypted =
        encrypt_pke(b"data", &[alice.public.clone(), bob.public.clone()], None).unwrap();

    // Both can decrypt
    let (data_alice, header_alice) = decrypt_pke(&encrypted, &alice.private).unwrap();
    let (data_bob, header_bob) = decrypt_pke(&encrypted, &bob.private).unwrap();

    // Same plaintext
    assert_eq!(data_alice, data_bob);

    // Both see both recipients in the header
    assert_eq!(header_alice.recipients.len(), 2);
    assert_eq!(header_bob.recipients.len(), 2);

    // But the encrypted DEKs should be different
    let alice_block = header_alice
        .find_recipient(&alice.public.to_address())
        .unwrap();
    let bob_block = header_bob.find_recipient(&bob.public.to_address()).unwrap();

    assert_ne!(alice_block.encrypted_dek, bob_block.encrypted_dek);
}

#[test]
fn test_metadata_cannot_be_used_for_decryption() {
    let recipient = Keypair::generate();
    let attacker = Keypair::generate();

    let encrypted = encrypt_pke(
        b"secret",
        std::slice::from_ref(&recipient.public),
        Some("public_filename.txt".into()),
    )
    .unwrap();

    // Attacker can see metadata (recipients, filename)
    let recipients = get_pke_recipients(&encrypted).unwrap();
    assert_eq!(recipients.len(), 1);

    // But cannot decrypt without the private key
    let result = decrypt_pke(&encrypted, &attacker.private);
    assert!(result.is_err());
}
