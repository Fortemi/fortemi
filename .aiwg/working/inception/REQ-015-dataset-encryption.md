# Requirements: Dataset Encryption (#15)

**Document ID:** REQ-015
**Status:** Inception Complete
**Created:** 2026-01-22
**Stakeholder Input:** Interactive session

---

## 1. Overview

Implement encryption for dataset packages (archives and knowledge shards) with support for:
- Passphrase-based encryption
- Keyfile-based encryption
- **End-to-end (E2E) encryption for shards** - allowing secure sharing between parties

## 2. Business Requirements

### BR-1: Data Protection
Users must be able to encrypt backup archives and knowledge shards to protect sensitive knowledge base content.

### BR-2: Portability
Encrypted packages must be portable - recipients with the correct key/passphrase can decrypt on any system.

### BR-3: Sharing (E2E for Shards)
Knowledge shards support multi-key E2E encryption, allowing two parties to share a shard that only they can decrypt. This enables secure knowledge sharing.

### BR-4: Multiple Decryption Flows
Support flexible decryption workflows:
1. Auto-detect encrypted files and prompt for credentials
2. Explicit decrypt command before import
3. Inline passphrase/keyfile parameter during import

## 3. Functional Requirements

### FR-1: Encryption Algorithm
- **Algorithm:** AES-256-GCM (AEAD)
- **Key Derivation:** Argon2id for passphrases
- **Parameters:**
  - Salt: 32 bytes random
  - Nonce: 12 bytes random
  - Memory: 64 MiB
  - Iterations: 3
  - Parallelism: 4

### FR-2: Key Sources
| Source | Use Case |
|--------|----------|
| Passphrase | Interactive use, human-memorable |
| Keyfile | Automation, CI/CD, scripts |
| Environment variable | Container deployments |

### FR-3: File Format

#### Standard Encrypted Archive (.tar.gz.enc, .shard.enc)
```
+------------------+
| Magic: "MMENC01" | 8 bytes - format identifier
+------------------+
| Header Length    | 4 bytes - header size (little-endian)
+------------------+
| Header (JSON)    | Variable - encryption metadata
+------------------+
| Encrypted Data   | Variable - AES-256-GCM ciphertext
+------------------+
| Auth Tag         | 16 bytes - GCM authentication tag
+------------------+
```

#### Header Schema
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
  "salt": "<base64>",
  "nonce": "<base64>",
  "key_type": "passphrase|keyfile|e2e",
  "created_at": "2026-01-22T12:00:00Z",
  "original_filename": "backup.shard"
}
```

### FR-4: E2E Encryption for Shards

Shards support multi-recipient E2E encryption using envelope encryption:

```
+------------------+
| Magic: "MME2E01" | 8 bytes - E2E format identifier
+------------------+
| Header Length    | 4 bytes
+------------------+
| Header (JSON)    | Variable - includes encrypted DEKs
+------------------+
| Encrypted Data   | Variable - encrypted with DEK
+------------------+
| Auth Tag         | 16 bytes
+------------------+
```

#### E2E Header Schema
```json
{
  "version": 1,
  "algorithm": "AES-256-GCM",
  "dek_algorithm": "AES-256-GCM",
  "recipients": [
    {
      "id": "alice",
      "kdf": "argon2id",
      "kdf_params": {...},
      "salt": "<base64>",
      "encrypted_dek": "<base64>",
      "dek_nonce": "<base64>"
    },
    {
      "id": "bob",
      "kdf": "argon2id",
      "kdf_params": {...},
      "salt": "<base64>",
      "encrypted_dek": "<base64>",
      "dek_nonce": "<base64>"
    }
  ],
  "data_nonce": "<base64>",
  "created_at": "2026-01-22T12:00:00Z",
  "original_filename": "shared.shard"
}
```

**E2E Flow:**
1. Generate random Data Encryption Key (DEK)
2. Encrypt shard data with DEK
3. For each recipient: derive KEK from their passphrase, encrypt DEK
4. Store all encrypted DEKs in header
5. Any recipient can decrypt their DEK, then decrypt data

### FR-5: API Endpoints

```
# Encrypt archive
GET /api/v1/backup/archive?encrypt=true&passphrase=...
GET /api/v1/backup/archive?encrypt=true&keyfile_path=...

# Encrypt shard
GET /api/v1/backup/knowledge-shard?encrypt=true&passphrase=...

# E2E encrypt shard (multiple recipients)
POST /api/v1/backup/knowledge-shard/e2e
{
  "recipients": [
    {"id": "alice", "passphrase": "..."},
    {"id": "bob", "passphrase": "..."}
  ],
  "include": ["notes", "links"]
}

# Decrypt and import (auto-detect)
POST /api/v1/backup/import
Content-Type: multipart/form-data
file: <encrypted file>
passphrase: <optional, prompts if needed>

# Explicit decrypt
POST /api/v1/backup/decrypt
{
  "input_path": "/path/to/backup.shard.enc",
  "output_path": "/path/to/backup.shard",
  "passphrase": "..."
}
```

### FR-6: MCP Tools

```javascript
// Encrypt backup
backup_export({ encrypt: true, passphrase: "..." })

// E2E encrypt shard
knowledge_shard_e2e({
  recipients: [
    { id: "alice", passphrase: "..." },
    { id: "bob", passphrase: "..." }
  ]
})

// Decrypt
backup_decrypt({
  input_path: "...",
  passphrase: "..."
})

// Import encrypted (auto-detect)
backup_import({
  file: "backup.shard.enc",
  passphrase: "..."
})
```

### FR-7: CLI Commands

```bash
# Encrypt existing backup
matric-cli backup encrypt backup.shard --passphrase
matric-cli backup encrypt backup.shard --keyfile key.bin

# E2E encrypt for sharing
matric-cli backup e2e-encrypt backup.shard \
  --recipient alice --passphrase \
  --recipient bob --passphrase

# Decrypt
matric-cli backup decrypt backup.shard.enc --passphrase
matric-cli backup decrypt backup.shard.enc --keyfile key.bin

# Import encrypted (prompts for passphrase)
matric-cli backup import backup.shard.enc
```

## 4. Non-Functional Requirements

### NFR-1: Security
- No plaintext secrets in logs
- Secure memory handling (zeroize after use)
- Constant-time comparison for authentication
- Random salt/nonce per encryption

### NFR-2: Performance
- Encryption overhead: < 10% of plaintext size
- Streaming encryption for large files (don't load entire file in memory)
- Key derivation: ~1 second on modern hardware

### NFR-3: Compatibility
- Decryption must work on any platform with matric-memory
- No external dependencies (GPG, OpenSSL CLI)

## 5. Acceptance Criteria

- [ ] AC-1: Can encrypt archive with passphrase
- [ ] AC-2: Can encrypt archive with keyfile
- [ ] AC-3: Can encrypt shard with passphrase
- [ ] AC-4: Can E2E encrypt shard for multiple recipients
- [ ] AC-5: Auto-detect encrypted files on import
- [ ] AC-6: Explicit decrypt command works
- [ ] AC-7: Inline passphrase on import works
- [ ] AC-8: Wrong passphrase produces clear error
- [ ] AC-9: MCP tools support encryption/decryption
- [ ] AC-10: All existing tests pass
- [ ] AC-11: New unit tests for encryption module

## 6. Out of Scope (v1.0)

- Public key cryptography (RSA/ECC)
- Hardware security module (HSM) integration
- Key rotation for existing encrypted files
- Partial decryption of E2E shards

## 7. Dependencies

- `aes-gcm` - AES-256-GCM implementation
- `argon2` - Key derivation
- `zeroize` - Secure memory clearing
- `rand` - Cryptographic random number generation
- `base64` - Encoding for headers

## 8. Risks

| Risk | Mitigation |
|------|------------|
| Memory exhaustion on large files | Stream encryption |
| Weak passphrases | Minimum length requirement, strength meter |
| Lost passphrase | Document recovery is impossible by design |
| Side-channel attacks | Use constant-time operations |

---

*Document approved for Elaboration phase*
