//! MMPKE01 file format for public-key encrypted data.
//!
//! # Format Specification
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Magic: "MMPKE01\n" (8 bytes)                                │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Header Length: u32 LE (4 bytes)                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Header (JSON)                                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Encrypted Data (AES-256-GCM ciphertext + 16-byte tag)       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Header Structure
//!
//! ```json
//! {
//!   "version": 1,
//!   "ephemeral_pubkey": "<base64>",
//!   "recipients": [
//!     {
//!       "address": "mm:...",
//!       "encrypted_dek": "<base64>",
//!       "dek_nonce": "<base64>"
//!     }
//!   ],
//!   "data_nonce": "<base64>",
//!   "original_filename": "backup.json"
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, CryptoResult};
use crate::pke::address::Address;
use crate::pke::keys::PublicKey;

/// Magic bytes for MMPKE01 format.
pub const MAGIC_BYTES: &[u8; 8] = b"MMPKE01\n";

/// Current format version.
pub const FORMAT_VERSION: u8 = 1;

/// MMPKE01 file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkeHeader {
    /// Format version (currently 1).
    pub version: u8,

    /// Sender's ephemeral public key for ECDH.
    pub ephemeral_pubkey: PublicKey,

    /// Per-recipient encrypted DEK blocks.
    pub recipients: Vec<RecipientBlock>,

    /// Nonce for data encryption.
    #[serde(with = "base64_bytes")]
    pub data_nonce: [u8; 12],

    /// Original filename (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,

    /// Creation timestamp (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Per-recipient block containing the encrypted DEK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientBlock {
    /// Recipient's address (for identification).
    pub address: Address,

    /// DEK encrypted with recipient's KEK.
    #[serde(with = "base64_bytes_vec")]
    pub encrypted_dek: Vec<u8>,

    /// Nonce used for DEK encryption.
    #[serde(with = "base64_bytes")]
    pub dek_nonce: [u8; 12],
}

impl PkeHeader {
    /// Create a new header.
    pub fn new(
        ephemeral_pubkey: PublicKey,
        recipients: Vec<RecipientBlock>,
        data_nonce: [u8; 12],
        original_filename: Option<String>,
    ) -> Self {
        Self {
            version: FORMAT_VERSION,
            ephemeral_pubkey,
            recipients,
            data_nonce,
            original_filename,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Find a recipient block by address.
    pub fn find_recipient(&self, address: &Address) -> Option<&RecipientBlock> {
        self.recipients.iter().find(|r| &r.address == address)
    }

    /// Find a recipient block by matching any of the given addresses.
    pub fn find_recipient_by_any(&self, addresses: &[Address]) -> Option<&RecipientBlock> {
        for addr in addresses {
            if let Some(block) = self.find_recipient(addr) {
                return Some(block);
            }
        }
        None
    }

    /// Get all recipient addresses.
    pub fn recipient_addresses(&self) -> Vec<&Address> {
        self.recipients.iter().map(|r| &r.address).collect()
    }
}

/// Serialize header to bytes (magic + length + JSON).
pub fn serialize_header(header: &PkeHeader) -> CryptoResult<Vec<u8>> {
    let json = serde_json::to_vec(header)
        .map_err(|e| CryptoError::InvalidFormat(format!("Failed to serialize header: {}", e)))?;

    let header_len = json.len() as u32;

    let mut output = Vec::with_capacity(8 + 4 + json.len());
    output.extend_from_slice(MAGIC_BYTES);
    output.extend_from_slice(&header_len.to_le_bytes());
    output.extend_from_slice(&json);

    Ok(output)
}

/// Parse header from bytes.
///
/// Returns the header and a slice to the remaining data (ciphertext).
pub fn parse_header(data: &[u8]) -> CryptoResult<(PkeHeader, &[u8])> {
    // Check minimum length
    if data.len() < 12 {
        return Err(CryptoError::InvalidFormat(
            "Data too short for MMPKE01 header".to_string(),
        ));
    }

    // Check magic bytes
    if &data[0..8] != MAGIC_BYTES {
        return Err(CryptoError::InvalidFormat(
            "Invalid magic bytes - not MMPKE01 format".to_string(),
        ));
    }

    // Read header length
    let header_len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;

    // Validate header length
    if data.len() < 12 + header_len {
        return Err(CryptoError::InvalidFormat(format!(
            "Data too short: expected {} bytes for header, got {}",
            header_len,
            data.len() - 12
        )));
    }

    // Parse JSON header
    let header_json = &data[12..12 + header_len];
    let header: PkeHeader = serde_json::from_slice(header_json)
        .map_err(|e| CryptoError::InvalidFormat(format!("Invalid header JSON: {}", e)))?;

    // Validate version
    if header.version != FORMAT_VERSION {
        return Err(CryptoError::InvalidFormat(format!(
            "Unsupported format version: {}",
            header.version
        )));
    }

    // Return header and remaining data
    let ciphertext = &data[12 + header_len..];
    Ok((header, ciphertext))
}

/// Check if data is in MMPKE01 format.
pub fn is_pke_format(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..8] == MAGIC_BYTES
}

/// Serde helper for base64-encoded fixed-size byte arrays.
mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 12], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 12], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)?;
        if bytes.len() != 12 {
            return Err(serde::de::Error::custom(format!(
                "Expected 12 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 12];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

/// Serde helper for base64-encoded Vec<u8>.
mod base64_bytes_vec {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pke::keys::Keypair;

    fn create_test_header() -> PkeHeader {
        let ephemeral = Keypair::generate();
        let recipient1 = Keypair::generate();
        let recipient2 = Keypair::generate();

        PkeHeader::new(
            ephemeral.public,
            vec![
                RecipientBlock {
                    address: recipient1.public.to_address(),
                    encrypted_dek: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                    dek_nonce: [0u8; 12],
                },
                RecipientBlock {
                    address: recipient2.public.to_address(),
                    encrypted_dek: vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
                    dek_nonce: [1u8; 12],
                },
            ],
            [42u8; 12],
            Some("test.json".to_string()),
        )
    }

    #[test]
    fn test_serialize_parse_header() {
        let header = create_test_header();
        let serialized = serialize_header(&header).unwrap();

        let (parsed, remaining) = parse_header(&serialized).unwrap();

        assert_eq!(parsed.version, header.version);
        assert_eq!(
            parsed.ephemeral_pubkey.as_bytes(),
            header.ephemeral_pubkey.as_bytes()
        );
        assert_eq!(parsed.recipients.len(), header.recipients.len());
        assert_eq!(parsed.data_nonce, header.data_nonce);
        assert_eq!(parsed.original_filename, header.original_filename);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_parse_header_with_ciphertext() {
        let header = create_test_header();
        let mut data = serialize_header(&header).unwrap();
        let ciphertext = b"encrypted data here";
        data.extend_from_slice(ciphertext);

        let (parsed, remaining) = parse_header(&data).unwrap();

        assert_eq!(parsed.version, header.version);
        assert_eq!(remaining, ciphertext);
    }

    #[test]
    fn test_is_pke_format() {
        let header = create_test_header();
        let data = serialize_header(&header).unwrap();

        assert!(is_pke_format(&data));
        assert!(!is_pke_format(b"not pke data"));
        assert!(!is_pke_format(b"MMENC01\n")); // Different format
    }

    #[test]
    fn test_parse_header_invalid_magic() {
        let result = parse_header(b"INVALID!\x00\x00\x00\x00{}");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_header_too_short() {
        let result = parse_header(b"short");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_recipient() {
        let header = create_test_header();
        let addr = &header.recipients[0].address;

        let found = header.find_recipient(addr);
        assert!(found.is_some());
        assert_eq!(&found.unwrap().address, addr);
    }

    #[test]
    fn test_find_recipient_not_found() {
        let header = create_test_header();
        let other = Keypair::generate();
        let other_addr = other.public.to_address();

        let found = header.find_recipient(&other_addr);
        assert!(found.is_none());
    }

    #[test]
    fn test_recipient_addresses() {
        let header = create_test_header();
        let addrs = header.recipient_addresses();

        assert_eq!(addrs.len(), 2);
    }

    #[test]
    fn test_header_json_serialization() {
        let header = create_test_header();
        let json = serde_json::to_string_pretty(&header).unwrap();

        // Verify it contains expected fields
        assert!(json.contains("\"version\":"));
        assert!(json.contains("\"ephemeral_pubkey\":"));
        assert!(json.contains("\"recipients\":"));
        assert!(json.contains("\"address\":"));
        assert!(json.contains("\"encrypted_dek\":"));
        assert!(json.contains("\"data_nonce\":"));
    }

    #[test]
    fn test_header_roundtrip_json() {
        let header = create_test_header();
        let json = serde_json::to_string(&header).unwrap();
        let parsed: PkeHeader = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.recipients.len(), header.recipients.len());
    }
}
