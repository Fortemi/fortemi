# ADR-006: Symmetric-Only Encryption for v1.0

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-015-encryption.md

## Context

Requirements specify passphrase and keyfile encryption for backup/export files. The question is whether to also support public key cryptography (RSA/ECC) in the initial release.

Public key crypto would enable:
- Encrypting for recipients without sharing secrets
- Digital signatures
- Key escrow scenarios

However, it also requires:
- Key management infrastructure
- Certificate handling
- More complex API surface

## Decision

Use symmetric encryption only for v1.0 with passphrase-derived keys via Argon2id. Public key cryptography is explicitly out of scope for the initial release.

The encryption format (MMENC01) is designed to allow future versions to add asymmetric key wrapping without breaking backward compatibility.

## Consequences

### Positive
- (+) Simpler implementation with fewer attack vectors
- (+) No key management infrastructure needed
- (+) Argon2id provides strong protection against brute force
- (+) Users understand passphrase-based encryption
- (+) Format allows future PKC extension

### Negative
- (-) Sharing requires communicating passphrase out-of-band
- (-) No digital signatures for integrity verification
- (-) Cannot encrypt for multiple recipients efficiently
- (-) Future versions will need migration path

## Implementation

**Code Location:** `crates/matric-crypto/src/`

**Encryption Flow:**

```
Passphrase → Argon2id → 256-bit Key → AES-256-GCM → Ciphertext
```

**KDF Parameters (Argon2id):**

```rust
pub const ARGON2_MEMORY_KIB: u32 = 65536;  // 64 MiB
pub const ARGON2_ITERATIONS: u32 = 3;
pub const ARGON2_PARALLELISM: u32 = 4;
```

**Format Header:**

```rust
pub struct EncryptionHeader {
    pub version: u8,           // 1 for MMENC01
    pub algorithm: String,     // "AES-256-GCM"
    pub kdf: String,           // "argon2id"
    pub kdf_params: KdfParams,
    pub salt: Vec<u8>,         // 16 bytes
    pub nonce: Vec<u8>,        // 12 bytes for GCM
}
```

## References

- ARCH-015-encryption.md (Section 15)
- REQ-015-dataset-encryption.md
- Argon2 RFC 9106
