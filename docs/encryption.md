# Encryption Guide

This guide covers the encryption features in Matric Memory for securing backup archives and knowledge shards.

## Overview

Matric Memory provides application-level encryption to protect your data at rest. The system supports two encryption modes:

| Mode | Use Case | Format | Recipients |
|------|----------|--------|------------|
| **Standard** | Personal backups | `.enc` (MMENC01) | Single passphrase or keyfile |
| **E2E** | Shared knowledge shards | `.e2e` (MME2E01) | Multiple recipients |

## Security Properties

The encryption system provides:

- **Confidentiality** - AES-256-GCM encryption (256-bit key)
- **Integrity** - AEAD authentication tag prevents tampering
- **Key Protection** - Argon2id memory-hard key derivation
- **Forward Secrecy** - Random salt per file prevents rainbow tables
- **Secure Memory** - Keys zeroized immediately after use

### Cryptographic Primitives

| Component | Algorithm | Parameters |
|-----------|-----------|------------|
| Symmetric Cipher | AES-256-GCM | 256-bit key, 96-bit nonce |
| Key Derivation | Argon2id | 64 MiB memory, 3 iterations, 4 parallelism |
| Random Generation | ChaCha20 | OS entropy source |

## Standard Encryption

Standard encryption protects files with a single passphrase or keyfile. Use this for personal backups.

### Encrypting with Passphrase

```bash
# Via API - encrypt a backup
curl -X POST http://localhost:3000/api/v1/backup/export \
  -H "Content-Type: application/json" \
  -d '{"encrypt": true, "passphrase": "your-secure-passphrase"}' \
  -o backup.enc

# Via MCP tool
export_all_notes({ encrypt: true, passphrase: "your-secure-passphrase" })
```

### Encrypting with Keyfile

Keyfiles provide stronger security than passphrases and can be stored separately.

```bash
# Generate a keyfile (32 random bytes, base64 encoded)
curl -X POST http://localhost:3000/api/v1/crypto/generate-keyfile \
  -o backup.key

# Encrypt using keyfile
curl -X POST http://localhost:3000/api/v1/backup/export \
  -H "Content-Type: application/json" \
  -d '{"encrypt": true, "keyfile_base64": "'$(base64 -w0 backup.key)'"}' \
  -o backup.enc
```

### Decrypting

```bash
# Decrypt with passphrase
curl -X POST http://localhost:3000/api/v1/backup/import \
  -H "Content-Type: application/json" \
  -d '{
    "backup_base64": "'$(base64 -w0 backup.enc)'",
    "passphrase": "your-secure-passphrase"
  }'

# Decrypt with keyfile
curl -X POST http://localhost:3000/api/v1/backup/import \
  -H "Content-Type: application/json" \
  -d '{
    "backup_base64": "'$(base64 -w0 backup.enc)'",
    "keyfile_base64": "'$(base64 -w0 backup.key)'"
  }'
```

### Passphrase Requirements

- Minimum 12 characters
- No maximum length
- All UTF-8 characters supported
- Stronger passphrases recommended (20+ characters, mixed case, numbers, symbols)

## E2E Multi-Recipient Encryption

E2E encryption allows sharing knowledge shards with multiple recipients. Each recipient has their own passphrase, and any recipient can decrypt the shard.

### How It Works

```
                    ┌─────────────────────────┐
                    │  Generate random DEK    │
                    │  (Data Encryption Key)  │
                    └───────────┬─────────────┘
                                │
                    ┌───────────▼─────────────┐
                    │  Encrypt data with DEK  │
                    │       (AES-256-GCM)     │
                    └───────────┬─────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
┌───────▼───────┐       ┌───────▼───────┐       ┌───────▼───────┐
│ Alice's KEK   │       │ Bob's KEK     │       │ Carol's KEK   │
│ (from pass)   │       │ (from pass)   │       │ (from pass)   │
└───────┬───────┘       └───────┬───────┘       └───────┬───────┘
        │                       │                       │
┌───────▼───────┐       ┌───────▼───────┐       ┌───────▼───────┐
│Encrypt DEK    │       │Encrypt DEK    │       │Encrypt DEK    │
│with Alice KEK │       │with Bob KEK   │       │with Carol KEK │
└───────┬───────┘       └───────┬───────┘       └───────┬───────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                │
                    ┌───────────▼─────────────┐
                    │   Store all encrypted   │
                    │   DEKs in header        │
                    └─────────────────────────┘
```

### Creating E2E Encrypted Shards

```bash
# Via API
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard \
  -H "Content-Type: application/json" \
  -d '{
    "e2e": true,
    "recipients": [
      {"id": "alice@example.com", "passphrase": "alice-secure-phrase"},
      {"id": "bob@example.com", "passphrase": "bob-secure-phrase"}
    ]
  }' \
  -o shared.shard.e2e

# Via MCP tool
knowledge_shard({
  e2e: true,
  recipients: [
    { id: "alice@example.com", passphrase: "alice-secure-phrase" },
    { id: "bob@example.com", passphrase: "bob-secure-phrase" }
  ]
})
```

### Decrypting E2E Shards

Any recipient can decrypt with their credentials:

```bash
# Alice decrypts
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d '{
    "shard_base64": "'$(base64 -w0 shared.shard.e2e)'",
    "recipient_id": "alice@example.com",
    "passphrase": "alice-secure-phrase"
  }'

# Or auto-detect recipient (tries each until one works)
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d '{
    "shard_base64": "'$(base64 -w0 shared.shard.e2e)'",
    "passphrase": "alice-secure-phrase"
  }'
```

### Listing Recipients

View who can decrypt an E2E shard without needing the passphrase:

```bash
curl -X POST http://localhost:3000/api/v1/crypto/recipients \
  -H "Content-Type: application/json" \
  -d '{"file_base64": "'$(base64 -w0 shared.shard.e2e)'"}'

# Response:
{
  "format": "e2e",
  "recipients": ["alice@example.com", "bob@example.com"]
}
```

### Recipient Limits

- Maximum 10 recipients per shard
- Recipient IDs: 1-64 characters
- Allowed characters: alphanumeric, underscore, dash, dot, @

## Format Detection

The system automatically detects encrypted files:

```bash
# Check if a file is encrypted
curl -X POST http://localhost:3000/api/v1/crypto/detect \
  -H "Content-Type: application/json" \
  -d '{"file_base64": "'$(base64 -w0 backup.enc)'"}'

# Response:
{
  "encrypted": true,
  "format": "standard",  # or "e2e" or "unencrypted"
  "version": 1
}
```

## File Formats

### Standard Format (MMENC01)

```
┌──────────────────────────────────────┐
│ Magic: "MMENC01\0"                   │ 8 bytes
├──────────────────────────────────────┤
│ Header Length                        │ 4 bytes (little-endian u32)
├──────────────────────────────────────┤
│ Header (JSON)                        │ Variable
│ {                                    │
│   "version": 1,                      │
│   "algorithm": "AES-256-GCM",        │
│   "kdf": "argon2id",                 │
│   "kdf_params": {...},               │
│   "salt": "<base64>",                │
│   "nonce": "<base64>",               │
│   "key_type": "passphrase",          │
│   "created_at": "2026-01-22T...",    │
│   "original_filename": "backup.tar"  │
│ }                                    │
├──────────────────────────────────────┤
│ Encrypted Data                       │ Variable
│ (ciphertext + 16-byte auth tag)      │
└──────────────────────────────────────┘
```

### E2E Format (MME2E01)

```
┌──────────────────────────────────────┐
│ Magic: "MME2E01\0"                   │ 8 bytes
├──────────────────────────────────────┤
│ Header Length                        │ 4 bytes (little-endian u32)
├──────────────────────────────────────┤
│ Header (JSON)                        │ Variable
│ {                                    │
│   "version": 1,                      │
│   "algorithm": "AES-256-GCM",        │
│   "dek_algorithm": "AES-256-GCM",    │
│   "recipients": [                    │
│     {                                │
│       "id": "alice@example.com",     │
│       "kdf": "argon2id",             │
│       "kdf_params": {...},           │
│       "salt": "<base64>",            │
│       "encrypted_dek": "<base64>",   │
│       "dek_nonce": "<base64>"        │
│     },                               │
│     ...                              │
│   ],                                 │
│   "data_nonce": "<base64>",          │
│   "created_at": "2026-01-22T...",    │
│   "original_filename": "shared.shard"│
│ }                                    │
├──────────────────────────────────────┤
│ Encrypted Data                       │ Variable
│ (ciphertext + 16-byte auth tag)      │
└──────────────────────────────────────┘
```

## MCP Tools

### encrypt_file

Encrypt arbitrary data with standard encryption.

```javascript
encrypt_file({
  data_base64: "<base64 data>",
  passphrase: "your-secure-passphrase",
  // OR
  keyfile_base64: "<base64 keyfile>",
  original_filename: "document.pdf"  // optional
})
```

### decrypt_file

Decrypt a standard encrypted file.

```javascript
decrypt_file({
  file_base64: "<base64 encrypted>",
  passphrase: "your-secure-passphrase"
  // OR
  keyfile_base64: "<base64 keyfile>"
})
```

### encrypt_e2e

Encrypt data for multiple recipients.

```javascript
encrypt_e2e({
  data_base64: "<base64 data>",
  recipients: [
    { id: "alice", passphrase: "alice-secret" },
    { id: "bob", passphrase: "bob-secret" }
  ],
  original_filename: "shared-notes.shard"
})
```

### decrypt_e2e

Decrypt E2E encrypted data.

```javascript
decrypt_e2e({
  file_base64: "<base64 encrypted>",
  recipient_id: "alice",  // optional - auto-detect if omitted
  passphrase: "alice-secret"
})
```

### detect_format

Check if a file is encrypted and what format it uses.

```javascript
detect_format({ file_base64: "<base64 data>" })
// Returns: { encrypted: true, format: "standard" | "e2e" | "unencrypted" }
```

### get_recipients

List recipients of an E2E encrypted file.

```javascript
get_recipients({ file_base64: "<base64 e2e file>" })
// Returns: { recipients: ["alice", "bob"] }
```

### generate_keyfile

Generate a new keyfile for encryption.

```javascript
generate_keyfile()
// Returns: { keyfile_base64: "<32 random bytes, base64>" }
```

## Best Practices

### Passphrase Selection

1. **Use a strong passphrase** - At least 20 characters mixing letters, numbers, and symbols
2. **Don't reuse passphrases** - Use unique passphrases for each encrypted backup
3. **Consider a password manager** - Store passphrases securely

### Keyfile Security

1. **Store keyfiles separately** - Don't keep keyfiles with encrypted data
2. **Back up keyfiles** - Loss of keyfile = loss of data
3. **Use secure storage** - Hardware security keys, encrypted volumes

### E2E Encryption

1. **Distribute passphrases securely** - Use secure channels (Signal, in-person)
2. **Verify recipient IDs** - Ensure you're sharing with intended recipients
3. **Rotate passphrases** - Create new shards with new passphrases periodically

### General

1. **Test decryption** - Always verify you can decrypt before deleting originals
2. **Keep format version** - Store version info to ensure future compatibility
3. **Multiple backups** - Encrypted backups should also follow 3-2-1 rule

## Troubleshooting

### "Passphrase too short"

Passphrases must be at least 12 characters. Use a longer, more secure passphrase.

### "Authentication failed" / "Decryption failed"

- Wrong passphrase or keyfile
- File was corrupted or tampered with
- Wrong encryption format (standard vs E2E)

### "No matching recipient"

For E2E encryption:
- The recipient ID doesn't match any in the file
- The passphrase is for a different recipient
- Use `get_recipients` to see valid recipient IDs

### "Invalid magic bytes"

The file is not encrypted with Matric Memory encryption, or is corrupted.

### Memory issues during decryption

Argon2id uses 64 MiB of memory by default. On constrained systems, files encrypted with `low_memory` KDF params use only 16 MiB.

## Security Considerations

### What's Protected

- File contents (confidentiality)
- File integrity (tampering detection)
- Key material in memory (zeroized after use)

### What's NOT Protected

- File existence (encrypted files are visible)
- File size (length is observable)
- Encryption metadata (algorithm, timestamp visible in header)
- Recipient IDs in E2E format (visible without decryption)

### Threat Model

The encryption is designed to protect against:
- Unauthorized access to backup files
- Data theft from storage devices
- Cloud storage provider access

It does NOT protect against:
- Compromised endpoints (malware on your system)
- Weak passphrases (brute force attacks)
- Coerced disclosure (legal/physical threats)

## Related Documentation

- [Shard Exchange Primer](./shard-exchange.md) - Practical workflows for sharing and recovering knowledge
- [Backup Guide](./backup.md) - Backup and restore procedures
- [Architecture](./architecture.md) - System design overview
- [Operations](./operations.md) - Deployment and maintenance
