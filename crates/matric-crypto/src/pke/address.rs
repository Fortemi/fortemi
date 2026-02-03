//! Human-friendly address format for public keys.
//!
//! Addresses are derived from public keys using BLAKE3 hashing and
//! Base58Check encoding, similar to cryptocurrency wallet addresses.
//!
//! # Format
//!
//! ```text
//! mm:<version><hash><checksum>
//!
//! - Prefix: "mm:" (matric-memory)
//! - Version: 1 byte (0x01 for v1)
//! - Hash: 20 bytes of BLAKE3(public_key)
//! - Checksum: 4 bytes of BLAKE3(version || hash)
//! - Encoding: Base58 (Bitcoin alphabet)
//! ```
//!
//! # Example
//!
//! ```text
//! Public Key: [32 bytes]
//! Address: mm:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa
//! ```

use std::fmt;
use std::str::FromStr;

use crate::error::{CryptoError, CryptoResult};
use crate::pke::keys::PublicKey;

/// Address version byte.
const ADDRESS_VERSION: u8 = 0x01;

/// Address prefix.
const ADDRESS_PREFIX: &str = "mm:";

/// Length of the hash portion (truncated BLAKE3).
const HASH_LENGTH: usize = 20;

/// Length of the checksum.
const CHECKSUM_LENGTH: usize = 4;

/// A human-friendly address derived from a public key.
///
/// Addresses are:
/// - Easy to copy/paste (no special characters)
/// - Self-verifying (checksum catches typos)
/// - Version-aware (future-proof)
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Address(String);

impl Address {
    /// Create an address from a public key.
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        // Hash the public key with BLAKE3
        let full_hash = blake3::hash(public_key.as_bytes());
        let hash_bytes = &full_hash.as_bytes()[..HASH_LENGTH];

        // Build payload: version || hash
        let mut payload = Vec::with_capacity(1 + HASH_LENGTH);
        payload.push(ADDRESS_VERSION);
        payload.extend_from_slice(hash_bytes);

        // Compute checksum: first 4 bytes of BLAKE3(payload)
        let checksum_hash = blake3::hash(&payload);
        let checksum = &checksum_hash.as_bytes()[..CHECKSUM_LENGTH];

        // Append checksum to payload
        payload.extend_from_slice(checksum);

        // Base58 encode
        let encoded = bs58::encode(&payload).into_string();

        Self(format!("{}{}", ADDRESS_PREFIX, encoded))
    }

    /// Parse an address string.
    ///
    /// Validates the prefix, checksum, and version.
    pub fn parse(s: &str) -> CryptoResult<Self> {
        // Check prefix
        if !s.starts_with(ADDRESS_PREFIX) {
            return Err(CryptoError::InvalidAddress(format!(
                "Address must start with '{}'",
                ADDRESS_PREFIX
            )));
        }

        let encoded = &s[ADDRESS_PREFIX.len()..];

        // Base58 decode
        let payload = bs58::decode(encoded)
            .into_vec()
            .map_err(|e| CryptoError::InvalidAddress(format!("Invalid Base58: {}", e)))?;

        // Check minimum length
        let min_len = 1 + HASH_LENGTH + CHECKSUM_LENGTH;
        if payload.len() != min_len {
            return Err(CryptoError::InvalidAddress(format!(
                "Invalid address length: expected {}, got {}",
                min_len,
                payload.len()
            )));
        }

        // Extract components
        let version = payload[0];
        let hash = &payload[1..1 + HASH_LENGTH];
        let checksum = &payload[1 + HASH_LENGTH..];

        // Verify version
        if version != ADDRESS_VERSION {
            return Err(CryptoError::InvalidAddress(format!(
                "Unsupported address version: {}",
                version
            )));
        }

        // Verify checksum
        let mut check_payload = Vec::with_capacity(1 + HASH_LENGTH);
        check_payload.push(version);
        check_payload.extend_from_slice(hash);
        let computed_checksum = blake3::hash(&check_payload);
        let expected_checksum = &computed_checksum.as_bytes()[..CHECKSUM_LENGTH];

        if checksum != expected_checksum {
            return Err(CryptoError::InvalidAddress(
                "Invalid checksum - address may be corrupted".to_string(),
            ));
        }

        Ok(Self(s.to_string()))
    }

    /// Get the address as a string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Verify the checksum is valid.
    pub fn verify_checksum(&self) -> bool {
        Self::parse(&self.0).is_ok()
    }

    /// Get just the hash portion (without prefix, version, or checksum).
    pub fn hash_bytes(&self) -> CryptoResult<[u8; HASH_LENGTH]> {
        let encoded = &self.0[ADDRESS_PREFIX.len()..];
        let payload = bs58::decode(encoded)
            .into_vec()
            .map_err(|e| CryptoError::InvalidAddress(e.to_string()))?;

        let mut hash = [0u8; HASH_LENGTH];
        hash.copy_from_slice(&payload[1..1 + HASH_LENGTH]);
        Ok(hash)
    }

    /// Get the version byte from this address.
    pub fn version(&self) -> u8 {
        // Address has been validated, so we can safely extract the version
        let encoded = &self.0[ADDRESS_PREFIX.len()..];
        let payload = bs58::decode(encoded).into_vec().unwrap();
        payload[0]
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.0)
    }
}

impl FromStr for Address {
    type Err = CryptoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Extension trait to get an address from a public key.
impl PublicKey {
    /// Convert this public key to a human-friendly address.
    pub fn to_address(&self) -> Address {
        Address::from_public_key(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pke::keys::Keypair;

    #[test]
    fn test_address_from_public_key() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        // Should start with prefix
        assert!(addr.as_str().starts_with("mm:"));

        // Should be reasonable length (~36-40 chars total)
        assert!(addr.as_str().len() > 30);
        assert!(addr.as_str().len() < 50);
    }

    #[test]
    fn test_address_deterministic() {
        let kp = Keypair::generate();
        let addr1 = kp.public.to_address();
        let addr2 = kp.public.to_address();

        assert_eq!(addr1.as_str(), addr2.as_str());
    }

    #[test]
    fn test_address_unique_per_key() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();

        let addr1 = kp1.public.to_address();
        let addr2 = kp2.public.to_address();

        assert_ne!(addr1.as_str(), addr2.as_str());
    }

    #[test]
    fn test_address_parse_valid() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        let parsed = Address::parse(addr.as_str()).unwrap();
        assert_eq!(addr.as_str(), parsed.as_str());
    }

    #[test]
    fn test_address_parse_invalid_prefix() {
        let result = Address::parse("xx:invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with"));
    }

    #[test]
    fn test_address_parse_invalid_base58() {
        let result = Address::parse("mm:0OIl"); // Invalid Base58 chars
        assert!(result.is_err());
    }

    #[test]
    fn test_address_parse_invalid_checksum() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        // Corrupt one character
        let mut corrupted = addr.as_str().to_string();
        let last_char = corrupted.pop().unwrap();
        let new_char = if last_char == 'A' { 'B' } else { 'A' };
        corrupted.push(new_char);

        let result = Address::parse(&corrupted);
        assert!(result.is_err());
    }

    #[test]
    fn test_address_verify_checksum() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        assert!(addr.verify_checksum());
    }

    #[test]
    fn test_address_display() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        let displayed = format!("{}", addr);
        assert_eq!(displayed, addr.as_str());
    }

    #[test]
    fn test_address_from_str() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        let parsed: Address = addr.as_str().parse().unwrap();
        assert_eq!(addr, parsed);
    }

    #[test]
    fn test_address_serialization() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        let json = serde_json::to_string(&addr).unwrap();
        let parsed: Address = serde_json::from_str(&json).unwrap();

        assert_eq!(addr, parsed);
    }

    #[test]
    fn test_address_hash_bytes() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();

        let hash = addr.hash_bytes().unwrap();
        assert_eq!(hash.len(), HASH_LENGTH);

        // Same key should give same hash
        let hash2 = kp.public.to_address().hash_bytes().unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_address_clone_eq_hash() {
        let kp = Keypair::generate();
        let addr = kp.public.to_address();
        let cloned = addr.clone();

        assert_eq!(addr, cloned);

        // Test Hash trait
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(addr.clone());
        assert!(set.contains(&cloned));
    }
}
