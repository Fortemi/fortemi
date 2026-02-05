# ADR-007: Envelope Encryption for E2E Multi-Recipient

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-015-encryption.md

## Context

Need to encrypt shards for sharing with multiple recipients. Each recipient should be able to decrypt with their own passphrase. Naive approach would re-encrypt the entire shard for each recipient, which is:
- Computationally expensive for large shards
- Storage inefficient (N copies of ciphertext)

## Decision

Use envelope encryption (also called "key wrapping"):
1. Generate a random Data Encryption Key (DEK)
2. Encrypt the data once with the DEK
3. For each recipient, encrypt the DEK with their Key Encryption Key (KEK)
4. Store: encrypted data + list of encrypted DEKs

This is the pattern used by age, GPG, and cloud KMS systems.

## Consequences

### Positive
- (+) Efficient: Data encrypted once regardless of recipient count
- (+) Scalable: Adding recipients is O(1) per recipient
- (+) Each recipient uses their own passphrase
- (+) Removing recipient doesn't require re-encryption (just remove wrapped DEK)
- (+) Standard pattern with well-understood security properties

### Negative
- (-) DEK must be securely generated and zeroized after use
- (-) More complex header format
- (-) All recipients share same underlying DEK (compromise one, compromise all)
- (-) Recipient list visible in plaintext header

## Implementation

**Code Location:** `crates/matric-crypto/src/pke/`

**Envelope Structure:**

```rust
pub struct E2EHeader {
    pub version: u8,
    pub algorithm: String,        // For data encryption
    pub dek_algorithm: String,    // For DEK wrapping
    pub recipients: Vec<Recipient>,
    pub data_nonce: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

pub struct Recipient {
    pub id: String,               // Recipient identifier
    pub kdf: String,              // "argon2id"
    pub kdf_params: KdfParams,
    pub salt: Vec<u8>,            // Unique per recipient
    pub encrypted_dek: Vec<u8>,   // DEK encrypted with this recipient's KEK
    pub dek_nonce: Vec<u8>,       // Nonce for DEK encryption
}
```

**Encryption Flow:**

```
1. Generate random DEK (32 bytes)
2. Encrypt data with DEK → ciphertext

For each recipient:
3. Derive KEK from recipient's passphrase + unique salt
4. Encrypt DEK with KEK → encrypted_dek
5. Store recipient entry in header

6. Zeroize DEK from memory
7. Write: magic + header + ciphertext
```

## References

- ARCH-015-encryption.md (Section 15)
- age encryption specification
- AWS Envelope Encryption documentation
