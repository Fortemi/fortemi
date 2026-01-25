# ADR-010: In-Memory Encryption vs Streaming

**Status:** Accepted
**Date:** 2026-01-22
**Deciders:** roctinam
**Source:** ARCH-015-encryption.md

## Context

Need to encrypt backup archives and shards which can range from a few KB to hundreds of MB. Two approaches:

1. **In-Memory:** Load entire file, encrypt, write out
2. **Streaming:** Process in chunks, never hold full file in memory

Streaming is more memory-efficient for large files but adds complexity (chunk authentication, state management).

## Decision

For v1.0, use in-memory encryption. Plan to add streaming support for files > 100 MB in a future version.

This decision is acceptable because:
- Typical shard sizes are < 50 MB
- Server environments usually have sufficient RAM
- Simplifies implementation significantly
- Can revisit when real-world usage patterns emerge

## Consequences

### Positive
- (+) Simpler implementation with fewer edge cases
- (+) AES-GCM handles entire file as single authenticated block
- (+) No chunk boundary issues
- (+) Easier testing and verification
- (+) Works well for typical use cases

### Negative
- (-) Memory usage = 2x file size during encryption (input + output buffers)
- (-) Cannot encrypt files larger than available RAM
- (-) Not suitable for streaming from network sources
- (-) Future migration to streaming will require format consideration

## Implementation

**Code Location:** `crates/matric-crypto/src/cipher.rs`

**Memory Layout:**

```
Encryption:
1. Read file into Vec<u8> (N bytes)
2. Allocate output buffer (N + 16 bytes for tag)
3. Peak memory: ~2N bytes
4. Write to file
5. Zeroize input buffer

Decryption:
1. Read file into Vec<u8>
2. Decrypt in place (reuse buffer)
3. Peak memory: ~N bytes
4. Return plaintext
```

**Encryption Function:**

```rust
pub fn encrypt_file(
    input_path: &Path,
    output_path: &Path,
    passphrase: &str,
) -> Result<()> {
    // Read entire file
    let plaintext = std::fs::read(input_path)?;

    // Derive key
    let (key, salt) = derive_key(passphrase)?;

    // Encrypt (returns ciphertext + tag)
    let ciphertext = encrypt_aes_gcm(&key, &plaintext)?;

    // Build header
    let header = EncryptionHeader::new(salt, nonce);

    // Write: magic + header + ciphertext
    write_encrypted_file(output_path, &header, &ciphertext)?;

    // Secure cleanup
    key.zeroize();

    Ok(())
}
```

**Future Streaming API (Planned):**

```rust
// For future implementation when needed
pub fn encrypt_file_streaming(
    input: impl Read,
    output: impl Write,
    passphrase: &str,
    chunk_size: usize,  // e.g., 64 KiB
) -> Result<()> {
    // Would use AES-GCM-SIV or ChaCha20-Poly1305 in chunked mode
    todo!("Streaming encryption for large files")
}
```

## References

- ARCH-015-encryption.md (Section 15)
- AES-GCM limitations for large files
- libsodium secretstream API (reference for future streaming)
