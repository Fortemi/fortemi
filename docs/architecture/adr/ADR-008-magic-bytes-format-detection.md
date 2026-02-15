# ADR-008: Magic Bytes for Format Detection

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-015-encryption.md

## Context

Need to auto-detect encrypted vs. unencrypted files for seamless import experience. Users shouldn't need to specify whether a file is encrypted - the system should detect it automatically.

Additionally, need to distinguish between:
- Unencrypted backup (tar.gz)
- Standard encrypted (single passphrase)
- E2E encrypted (multi-recipient)

## Decision

Use 8-byte ASCII magic identifiers at the start of encrypted files:
- `MMENC01\x00` - Standard encrypted format (matric-memory encryption v01)
- `MME2E01\x00` - E2E multi-recipient format v01

These magic bytes are:
- Human-readable in hex dumps
- Distinguishable from tar.gz magic (`0x1f 0x8b`)
- Version-numbered for future format changes
- Null-terminated for easy C compatibility

## Consequences

### Positive
- (+) Instant format detection without parsing entire header
- (+) Clear version identification for future format changes
- (+) No collision with tar.gz magic
- (+) Human-readable in debugging
- (+) Enables automatic decryption prompts

### Negative
- (-) Slightly increased file size (~8 bytes overhead)
- (-) Magic bytes are not secret (format is identifiable)
- (-) Must reserve byte sequences to avoid future collisions

## Implementation

**Code Location:** `crates/matric-crypto/src/detect.rs`

**Magic Constants:**

```rust
pub const MAGIC_STANDARD: &[u8; 8] = b"MMENC01\x00";
pub const MAGIC_E2E: &[u8; 8] = b"MME2E01\x00";
pub const MAGIC_GZIP: &[u8; 2] = &[0x1f, 0x8b];
```

**Detection Function:**

```rust
pub enum FileFormat {
    Unencrypted,      // Plain tar.gz
    EncryptedStandard, // MMENC01
    EncryptedE2E,      // MME2E01
    Unknown,
}

pub fn detect_format(data: &[u8]) -> FileFormat {
    if data.len() < 8 {
        return FileFormat::Unknown;
    }

    if &data[..8] == MAGIC_STANDARD {
        FileFormat::EncryptedStandard
    } else if &data[..8] == MAGIC_E2E {
        FileFormat::EncryptedE2E
    } else if &data[..2] == MAGIC_GZIP {
        FileFormat::Unencrypted
    } else {
        FileFormat::Unknown
    }
}
```

## References

- ARCH-015-encryption.md (Section 15)
- File format magic number conventions
- gzip specification (RFC 1952)
