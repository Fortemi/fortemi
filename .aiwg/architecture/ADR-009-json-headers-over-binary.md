# ADR-009: JSON Headers Over Binary

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-015-encryption.md

## Context

Need to store encryption metadata in file headers:
- Algorithm identifiers
- KDF parameters (memory, iterations, parallelism)
- Nonces and salts
- Recipient information (for E2E)
- Timestamps

Could use binary encoding (compact) or text-based format (readable).

## Decision

Use JSON for header format instead of binary encoding. Headers are prefixed with a 4-byte length field for efficient parsing.

Format: `[magic:8][header_len:4][json_header][ciphertext]`

## Consequences

### Positive
- (+) Human-readable for debugging (`xxd` or `strings`)
- (+) Easily extensible for future parameters
- (+) No endianness concerns for numeric fields
- (+) Standard tooling for parsing (jq, serde_json)
- (+) Schema evolution is straightforward

### Negative
- (-) ~30% larger than binary encoding for typical headers
- (-) JSON parsing overhead (negligible for small headers)
- (-) Must handle JSON escaping for binary data (base64)
- (-) Header size variable (binary could be fixed)

## Implementation

**Code Location:** `crates/matric-crypto/src/format.rs`

**Header Serialization:**

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct EncryptionHeader {
    pub version: u8,
    pub algorithm: String,
    pub kdf: String,
    pub kdf_params: KdfParams,
    #[serde(with = "base64_serde")]
    pub salt: Vec<u8>,
    #[serde(with = "base64_serde")]
    pub nonce: Vec<u8>,
    pub created_at: String,  // ISO 8601
}

#[derive(Serialize, Deserialize)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}
```

**Wire Format:**

```
Offset  | Size | Content
--------|------|--------
0       | 8    | Magic bytes ("MMENC01\0")
8       | 4    | Header length (big-endian u32)
12      | N    | JSON header (UTF-8)
12+N    | M    | Ciphertext
```

**Example Header:**

```json
{
  "version": 1,
  "algorithm": "AES-256-GCM",
  "kdf": "argon2id",
  "kdf_params": {
    "memory_kib": 65536,
    "iterations": 3,
    "parallelism": 4
  },
  "salt": "rJ3f2Qk9xYpL...",
  "nonce": "a8Kj2nM...",
  "created_at": "2026-01-22T12:00:00Z"
}
```

## References

- ARCH-015-encryption.md (Section 15)
- JSON specification (RFC 8259)
- Comparison with CBOR, MessagePack considered but rejected for debuggability
