//! Encrypted file format definitions and parsing.

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, CryptoResult};
use crate::kdf::KdfParams;

/// Magic bytes for standard encrypted format.
pub const MAGIC_STANDARD: &[u8; 8] = b"MMENC01\x00";

/// Magic bytes for E2E encrypted format.
pub const MAGIC_E2E: &[u8; 8] = b"MME2E01\x00";

/// File format type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Standard single-key encryption (MMENC01).
    Standard,
    /// E2E multi-recipient encryption (MME2E01).
    E2E,
    /// Unencrypted file.
    Unencrypted,
}

/// Key type used for encryption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    /// Key derived from passphrase.
    Passphrase,
    /// Key loaded from file.
    Keyfile,
}

/// Header for standard encrypted files (MMENC01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// Format version.
    pub version: u32,
    /// Encryption algorithm (always "AES-256-GCM").
    pub algorithm: String,
    /// Key derivation function (always "argon2id").
    pub kdf: String,
    /// KDF parameters.
    pub kdf_params: KdfParams,
    /// Salt used for key derivation (base64).
    pub salt: String,
    /// Nonce used for encryption (base64).
    pub nonce: String,
    /// Type of key used.
    pub key_type: KeyType,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Original filename (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

/// Single recipient entry in E2E header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    /// Recipient identifier.
    pub id: String,
    /// KDF used for this recipient.
    pub kdf: String,
    /// KDF parameters.
    pub kdf_params: KdfParams,
    /// Salt for this recipient (base64).
    pub salt: String,
    /// Encrypted DEK for this recipient (base64).
    pub encrypted_dek: String,
    /// Nonce used to encrypt DEK (base64).
    pub dek_nonce: String,
}

/// Header for E2E encrypted files (MME2E01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2EHeader {
    /// Format version.
    pub version: u32,
    /// Data encryption algorithm.
    pub algorithm: String,
    /// DEK encryption algorithm.
    pub dek_algorithm: String,
    /// List of recipients with their encrypted DEKs.
    pub recipients: Vec<Recipient>,
    /// Nonce used to encrypt data (base64).
    pub data_nonce: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Original filename (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

/// Parsed encrypted file structure.
pub struct EncryptedFile {
    /// Detected file format.
    pub format: FileFormat,
    /// Raw header bytes (JSON).
    pub header_bytes: Vec<u8>,
    /// Encrypted data (ciphertext + auth tag).
    pub ciphertext: Vec<u8>,
}

impl EncryptedFile {
    /// Parse an encrypted file from bytes.
    pub fn parse(data: &[u8]) -> CryptoResult<Self> {
        if data.len() < 12 {
            return Err(CryptoError::InvalidMagic);
        }

        let magic = &data[0..8];
        let format = if magic == MAGIC_STANDARD {
            FileFormat::Standard
        } else if magic == MAGIC_E2E {
            FileFormat::E2E
        } else {
            return Err(CryptoError::InvalidMagic);
        };

        let header_len = u32::from_le_bytes(
            data[8..12]
                .try_into()
                .map_err(|_| CryptoError::HeaderParse("Invalid header length".into()))?,
        ) as usize;

        if data.len() < 12 + header_len {
            return Err(CryptoError::HeaderParse("File truncated".into()));
        }

        let header_bytes = data[12..12 + header_len].to_vec();
        let ciphertext = data[12 + header_len..].to_vec();

        Ok(Self {
            format,
            header_bytes,
            ciphertext,
        })
    }

    /// Parse the header as standard format.
    pub fn parse_standard_header(&self) -> CryptoResult<Header> {
        serde_json::from_slice(&self.header_bytes)
            .map_err(|e| CryptoError::HeaderParse(e.to_string()))
    }

    /// Parse the header as E2E format.
    pub fn parse_e2e_header(&self) -> CryptoResult<E2EHeader> {
        serde_json::from_slice(&self.header_bytes)
            .map_err(|e| CryptoError::HeaderParse(e.to_string()))
    }
}

/// Serialize an encrypted file to bytes.
pub fn serialize_encrypted(format: FileFormat, header: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let magic = match format {
        FileFormat::Standard => MAGIC_STANDARD,
        FileFormat::E2E => MAGIC_E2E,
        FileFormat::Unencrypted => panic!("Cannot serialize unencrypted format"),
    };

    let header_len = (header.len() as u32).to_le_bytes();

    let mut output = Vec::with_capacity(8 + 4 + header.len() + ciphertext.len());
    output.extend_from_slice(magic);
    output.extend_from_slice(&header_len);
    output.extend_from_slice(header);
    output.extend_from_slice(ciphertext);

    output
}

/// Encode bytes as base64.
pub fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Decode base64 string to bytes.
pub fn base64_decode(data: &str) -> CryptoResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| CryptoError::HeaderParse(format!("Invalid base64: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_constants() {
        assert_eq!(MAGIC_STANDARD.len(), 8);
        assert_eq!(MAGIC_E2E.len(), 8);
        assert!(MAGIC_STANDARD.starts_with(b"MMENC"));
        assert!(MAGIC_E2E.starts_with(b"MME2E"));
    }

    #[test]
    fn test_header_serialization() {
        let header = Header {
            version: 1,
            algorithm: "AES-256-GCM".to_string(),
            kdf: "argon2id".to_string(),
            kdf_params: KdfParams::default(),
            salt: base64_encode(&[0u8; 32]),
            nonce: base64_encode(&[0u8; 12]),
            key_type: KeyType::Passphrase,
            created_at: Utc::now(),
            original_filename: Some("test.tar.gz".to_string()),
        };

        let json = serde_json::to_string(&header).unwrap();
        let parsed: Header = serde_json::from_str(&json).unwrap();

        assert_eq!(header.version, parsed.version);
        assert_eq!(header.algorithm, parsed.algorithm);
        assert_eq!(header.original_filename, parsed.original_filename);
    }

    #[test]
    fn test_header_without_filename() {
        let header = Header {
            version: 1,
            algorithm: "AES-256-GCM".to_string(),
            kdf: "argon2id".to_string(),
            kdf_params: KdfParams::default(),
            salt: base64_encode(&[0u8; 32]),
            nonce: base64_encode(&[0u8; 12]),
            key_type: KeyType::Keyfile,
            created_at: Utc::now(),
            original_filename: None,
        };

        let json = serde_json::to_string(&header).unwrap();
        assert!(!json.contains("original_filename"));
    }

    #[test]
    fn test_e2e_header_serialization() {
        let header = E2EHeader {
            version: 1,
            algorithm: "AES-256-GCM".to_string(),
            dek_algorithm: "AES-256-GCM".to_string(),
            recipients: vec![
                Recipient {
                    id: "alice".to_string(),
                    kdf: "argon2id".to_string(),
                    kdf_params: KdfParams::default(),
                    salt: base64_encode(&[1u8; 32]),
                    encrypted_dek: base64_encode(&[2u8; 48]),
                    dek_nonce: base64_encode(&[3u8; 12]),
                },
                Recipient {
                    id: "bob".to_string(),
                    kdf: "argon2id".to_string(),
                    kdf_params: KdfParams::default(),
                    salt: base64_encode(&[4u8; 32]),
                    encrypted_dek: base64_encode(&[5u8; 48]),
                    dek_nonce: base64_encode(&[6u8; 12]),
                },
            ],
            data_nonce: base64_encode(&[7u8; 12]),
            created_at: Utc::now(),
            original_filename: Some("shared.shard".to_string()),
        };

        let json = serde_json::to_string(&header).unwrap();
        let parsed: E2EHeader = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.recipients.len(), 2);
        assert_eq!(parsed.recipients[0].id, "alice");
        assert_eq!(parsed.recipients[1].id, "bob");
    }

    #[test]
    fn test_serialize_encrypted_standard() {
        let header = b"test header";
        let ciphertext = b"encrypted data";

        let data = serialize_encrypted(FileFormat::Standard, header, ciphertext);

        assert_eq!(&data[0..8], MAGIC_STANDARD);
        assert_eq!(
            u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize,
            header.len()
        );
        assert_eq!(&data[12..12 + header.len()], header);
        assert_eq!(&data[12 + header.len()..], ciphertext);
    }

    #[test]
    fn test_serialize_encrypted_e2e() {
        let header = b"e2e header";
        let ciphertext = b"e2e data";

        let data = serialize_encrypted(FileFormat::E2E, header, ciphertext);

        assert_eq!(&data[0..8], MAGIC_E2E);
    }

    #[test]
    fn test_parse_standard_file() {
        let header = b"test header";
        let ciphertext = b"ciphertext";
        let data = serialize_encrypted(FileFormat::Standard, header, ciphertext);

        let file = EncryptedFile::parse(&data).unwrap();

        assert_eq!(file.format, FileFormat::Standard);
        assert_eq!(file.header_bytes, header);
        assert_eq!(file.ciphertext, ciphertext);
    }

    #[test]
    fn test_parse_e2e_file() {
        let header = b"e2e header";
        let ciphertext = b"ciphertext";
        let data = serialize_encrypted(FileFormat::E2E, header, ciphertext);

        let file = EncryptedFile::parse(&data).unwrap();

        assert_eq!(file.format, FileFormat::E2E);
    }

    #[test]
    fn test_parse_invalid_magic() {
        let data = b"INVALID!headerdata";
        let result = EncryptedFile::parse(data);
        assert!(matches!(result, Err(CryptoError::InvalidMagic)));
    }

    #[test]
    fn test_parse_truncated_file() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC_STANDARD);
        data.extend_from_slice(&100u32.to_le_bytes()); // Claims 100 byte header
        data.extend_from_slice(b"short"); // But only has 5 bytes

        let result = EncryptedFile::parse(&data);
        assert!(matches!(result, Err(CryptoError::HeaderParse(_))));
    }

    #[test]
    fn test_parse_too_short() {
        let data = b"short";
        let result = EncryptedFile::parse(data);
        assert!(matches!(result, Err(CryptoError::InvalidMagic)));
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = [42u8; 32];
        let encoded = base64_encode(&original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.as_slice(), decoded.as_slice());
    }

    #[test]
    fn test_base64_decode_invalid() {
        let result = base64_decode("not valid base64!!!");
        assert!(matches!(result, Err(CryptoError::HeaderParse(_))));
    }

    #[test]
    fn test_key_type_serialization() {
        assert_eq!(
            serde_json::to_string(&KeyType::Passphrase).unwrap(),
            "\"passphrase\""
        );
        assert_eq!(
            serde_json::to_string(&KeyType::Keyfile).unwrap(),
            "\"keyfile\""
        );
    }
}
