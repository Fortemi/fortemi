# Architecture Design: Dataset Encryption (REQ-015)

**Document ID:** ARCH-015
**Status:** Draft
**Created:** 2026-01-22
**Author:** Architecture Designer
**Requirements:** REQ-015-dataset-encryption.md

---

## 1. Executive Summary

This document defines the architecture for implementing encryption in matric-memory's dataset packages. The design supports:

1. **Passphrase-based encryption** - For interactive human use
2. **Keyfile-based encryption** - For automation and CI/CD
3. **End-to-end (E2E) multi-recipient encryption** - For secure shard sharing

The architecture integrates with the existing backup infrastructure while maintaining backward compatibility with unencrypted archives and shards.

---

## 2. Architecture Overview

### 2.1 High-Level Architecture

```
                                    +------------------+
                                    |   matric-api     |
                                    |   (HTTP API)     |
                                    +--------+---------+
                                             |
                  +--------------------------+---------------------------+
                  |                          |                           |
        +---------v---------+      +---------v---------+      +----------v---------+
        |  Backup Handler   |      |  Shard Handler    |      |  Import Handler    |
        |  (Archive Export) |      |  (E2E Export)     |      |  (Decrypt+Import)  |
        +---------+---------+      +---------+---------+      +----------+---------+
                  |                          |                           |
                  +--------------------------+---------------------------+
                                             |
                                    +--------v---------+
                                    |  matric-crypto   |  <-- NEW CRATE
                                    |  (Encryption)    |
                                    +--------+---------+
                                             |
                  +--------------------------+---------------------------+
                  |                          |                           |
        +---------v---------+      +---------v---------+      +----------v---------+
        |   Standard Enc    |      |   E2E Envelope    |      |   Key Derivation   |
        |   (Single Key)    |      |   (Multi-Key)     |      |   (Argon2id)       |
        +-------------------+      +-------------------+      +--------------------+
```

### 2.2 Component Diagram

```
+-----------------------------------------------------------------------------+
|                              matric-memory                                   |
|                                                                             |
|  +------------------------+     +----------------------------------------+  |
|  |    matric-api          |     |           matric-crypto (NEW)          |  |
|  |                        |     |                                        |  |
|  |  +------------------+  |     |  +------------------+  +--------------+ |  |
|  |  | backup handlers  |<--------->| EncryptionEngine |  | KeyDerivation| |  |
|  |  +------------------+  |     |  +------------------+  +--------------+ |  |
|  |                        |     |                                        |  |
|  |  +------------------+  |     |  +------------------+  +--------------+ |  |
|  |  | shard handlers   |<--------->| E2EEnvelopeEnc   |  | FileFormat   | |  |
|  |  +------------------+  |     |  +------------------+  +--------------+ |  |
|  |                        |     |                                        |  |
|  +------------------------+     +----------------------------------------+  |
|                                                                             |
|  +------------------------+     +----------------------------------------+  |
|  |    mcp-server          |     |         Dependencies                   |  |
|  |                        |     |                                        |  |
|  |  - backup_export       |     |  - aes-gcm (AES-256-GCM)              |  |
|  |  - knowledge_shard_e2e |     |  - argon2 (Key derivation)            |  |
|  |  - backup_decrypt      |     |  - zeroize (Secure memory)            |  |
|  |  - backup_import       |     |  - rand (Crypto RNG)                  |  |
|  |                        |     |  - base64 (Header encoding)           |  |
|  +------------------------+     +----------------------------------------+  |
|                                                                             |
+-----------------------------------------------------------------------------+
```

---

## 3. Module Structure

### 3.1 New Crate: `matric-crypto`

```
crates/matric-crypto/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API exports
│   ├── error.rs            # Error types
│   ├── kdf.rs              # Key derivation (Argon2id)
│   ├── cipher.rs           # AES-256-GCM operations
│   ├── format.rs           # File format parsing/writing
│   ├── standard.rs         # Standard single-key encryption
│   ├── e2e.rs              # E2E envelope encryption
│   ├── detect.rs           # Magic byte detection
│   └── stream.rs           # Streaming encryption (future)
└── tests/
    ├── kdf_tests.rs
    ├── cipher_tests.rs
    ├── format_tests.rs
    ├── standard_tests.rs
    └── e2e_tests.rs
```

### 3.2 Cargo.toml for matric-crypto

```toml
[package]
name = "matric-crypto"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# Cryptographic primitives
aes-gcm = "0.10"
argon2 = "0.5"
rand = { version = "0.8", features = ["getrandom"] }

# Secure memory handling
zeroize = { version = "1", features = ["derive"] }

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }
base64 = { workspace = true }

# Utilities
chrono = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
tempfile = "3"
```

### 3.3 Core Type Definitions

```rust
// src/lib.rs
pub mod cipher;
pub mod detect;
pub mod e2e;
pub mod error;
pub mod format;
pub mod kdf;
pub mod standard;

pub use error::{CryptoError, CryptoResult};
pub use format::{EncryptedFile, FileFormat, Header, E2EHeader, Recipient};
pub use kdf::{derive_key, KdfParams};
pub use standard::{encrypt_standard, decrypt_standard};
pub use e2e::{encrypt_e2e, decrypt_e2e};
pub use detect::{detect_format, is_encrypted};

// src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid magic bytes - not an encrypted file")]
    InvalidMagic,

    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u32),

    #[error("Header parsing failed: {0}")]
    HeaderParse(String),

    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),

    #[error("Encryption failed: {0}")]
    Encryption(String),

    #[error("Decryption failed - wrong key or corrupted data")]
    Decryption,

    #[error("Authentication failed - data may be tampered")]
    Authentication,

    #[error("No matching recipient found")]
    NoMatchingRecipient,

    #[error("Invalid keyfile: {0}")]
    InvalidKeyfile(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type CryptoResult<T> = Result<T, CryptoError>;
```

---

## 4. Cryptographic Primitives

### 4.1 Algorithm Selection

| Component | Algorithm | Justification |
|-----------|-----------|---------------|
| Symmetric Cipher | AES-256-GCM | Industry standard AEAD, hardware acceleration |
| Key Derivation | Argon2id | Memory-hard, resistant to GPU/ASIC attacks |
| Random Generation | ChaCha20-based CSPRNG | Cryptographically secure via `getrandom` |

### 4.2 Key Derivation Implementation

```rust
// src/kdf.rs
use argon2::{Argon2, Algorithm, Version, Params};
use zeroize::Zeroize;

/// Argon2id parameters matching requirements
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KdfParams {
    /// Memory in KiB (default: 65536 = 64 MiB)
    pub memory_kib: u32,
    /// Time iterations (default: 3)
    pub iterations: u32,
    /// Parallelism degree (default: 4)
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 65536,  // 64 MiB
            iterations: 3,
            parallelism: 4,
        }
    }
}

/// Key wrapper with automatic zeroization
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct DerivedKey {
    key: [u8; 32],
}

impl DerivedKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

/// Derive a 256-bit key from passphrase using Argon2id
pub fn derive_key(
    passphrase: &[u8],
    salt: &[u8; 32],
    params: &KdfParams,
) -> CryptoResult<DerivedKey> {
    let argon2_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(32), // Output length
    ).map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    let mut key = [0u8; 32];
    argon2.hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;

    Ok(DerivedKey { key })
}

/// Load key from keyfile (raw 32 bytes or base64-encoded)
pub fn load_keyfile(path: &std::path::Path) -> CryptoResult<DerivedKey> {
    let contents = std::fs::read(path)?;

    let key = if contents.len() == 32 {
        // Raw 32-byte key
        let mut key = [0u8; 32];
        key.copy_from_slice(&contents);
        key
    } else {
        // Try base64 decode
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &contents
        ).map_err(|e| CryptoError::InvalidKeyfile(e.to_string()))?;

        if decoded.len() != 32 {
            return Err(CryptoError::InvalidKeyfile(
                format!("Expected 32 bytes, got {}", decoded.len())
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        key
    };

    Ok(DerivedKey { key })
}
```

### 4.3 Cipher Implementation

```rust
// src/cipher.rs
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

/// Generate cryptographically secure random bytes
pub fn generate_random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// Encrypt plaintext with AES-256-GCM
/// Returns (ciphertext, auth_tag) where auth_tag is appended to ciphertext
pub fn aes_gcm_encrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> CryptoResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    let nonce = Nonce::from_slice(nonce);

    cipher.encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Encryption("AES-GCM encryption failed".into()))
}

/// Decrypt ciphertext with AES-256-GCM
pub fn aes_gcm_decrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> CryptoResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| CryptoError::Decryption)?;

    let nonce = Nonce::from_slice(nonce);

    cipher.decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decryption)
}
```

---

## 5. File Format Specifications

### 5.1 Standard Encrypted Format (MMENC01)

For single-key encryption of archives and shards.

```
+------------------+
| Magic: "MMENC01" | 8 bytes - ASCII format identifier
+------------------+
| Header Length    | 4 bytes - little-endian u32
+------------------+
| Header (JSON)    | Variable - encryption metadata
+------------------+
| Encrypted Data   | Variable - AES-256-GCM ciphertext + 16-byte tag
+------------------+
```

**Header Schema:**

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
  "salt": "<base64 encoded 32 bytes>",
  "nonce": "<base64 encoded 12 bytes>",
  "key_type": "passphrase",
  "created_at": "2026-01-22T12:00:00Z",
  "original_filename": "backup.tar.gz"
}
```

### 5.2 E2E Envelope Format (MME2E01)

For multi-recipient encryption of shards.

```
+------------------+
| Magic: "MME2E01" | 8 bytes - ASCII format identifier
+------------------+
| Header Length    | 4 bytes - little-endian u32
+------------------+
| Header (JSON)    | Variable - includes all encrypted DEKs
+------------------+
| Encrypted Data   | Variable - AES-256-GCM ciphertext + 16-byte tag
+------------------+
```

**E2E Header Schema:**

```json
{
  "version": 1,
  "algorithm": "AES-256-GCM",
  "dek_algorithm": "AES-256-GCM",
  "recipients": [
    {
      "id": "alice",
      "kdf": "argon2id",
      "kdf_params": {
        "memory_kib": 65536,
        "iterations": 3,
        "parallelism": 4
      },
      "salt": "<base64 encoded 32 bytes>",
      "encrypted_dek": "<base64 encoded 48 bytes (32-byte key + 16-byte tag)>",
      "dek_nonce": "<base64 encoded 12 bytes>"
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
  "data_nonce": "<base64 encoded 12 bytes>",
  "created_at": "2026-01-22T12:00:00Z",
  "original_filename": "shared.shard"
}
```

### 5.3 Format Implementation

```rust
// src/format.rs
use base64::Engine;
use serde::{Deserialize, Serialize};

pub const MAGIC_STANDARD: &[u8; 8] = b"MMENC01\x00";
pub const MAGIC_E2E: &[u8; 8] = b"MME2E01\x00";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileFormat {
    Standard,
    E2E,
    Unencrypted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub algorithm: String,
    pub kdf: String,
    pub kdf_params: KdfParams,
    pub salt: String,      // base64
    pub nonce: String,     // base64
    pub key_type: KeyType,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub original_filename: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    Passphrase,
    Keyfile,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct E2EHeader {
    pub version: u32,
    pub algorithm: String,
    pub dek_algorithm: String,
    pub recipients: Vec<Recipient>,
    pub data_nonce: String,  // base64
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub original_filename: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Recipient {
    pub id: String,
    pub kdf: String,
    pub kdf_params: KdfParams,
    pub salt: String,          // base64
    pub encrypted_dek: String, // base64
    pub dek_nonce: String,     // base64
}

/// Parsed encrypted file structure
pub struct EncryptedFile {
    pub format: FileFormat,
    pub header_bytes: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl EncryptedFile {
    /// Parse an encrypted file from bytes
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

        let header_len = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;

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

    /// Parse the header as standard format
    pub fn parse_standard_header(&self) -> CryptoResult<Header> {
        serde_json::from_slice(&self.header_bytes)
            .map_err(|e| CryptoError::HeaderParse(e.to_string()))
    }

    /// Parse the header as E2E format
    pub fn parse_e2e_header(&self) -> CryptoResult<E2EHeader> {
        serde_json::from_slice(&self.header_bytes)
            .map_err(|e| CryptoError::HeaderParse(e.to_string()))
    }
}

/// Serialize an encrypted file
pub fn serialize_encrypted(
    format: FileFormat,
    header: &[u8],
    ciphertext: &[u8],
) -> Vec<u8> {
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
```

---

## 6. Standard Encryption Implementation

### 6.1 Encrypt Function

```rust
// src/standard.rs
use crate::{
    cipher::{aes_gcm_encrypt, generate_random},
    format::{serialize_encrypted, FileFormat, Header, KeyType, KdfParams},
    kdf::{derive_key, DerivedKey},
    CryptoResult,
};

/// Options for standard encryption
pub struct EncryptOptions {
    pub key_type: KeyType,
    pub original_filename: Option<String>,
    pub kdf_params: Option<KdfParams>,
}

impl Default for EncryptOptions {
    fn default() -> Self {
        Self {
            key_type: KeyType::Passphrase,
            original_filename: None,
            kdf_params: None,
        }
    }
}

/// Encrypt data using standard single-key encryption
pub fn encrypt_standard(
    plaintext: &[u8],
    key: &DerivedKey,
    salt: &[u8; 32],
    options: EncryptOptions,
) -> CryptoResult<Vec<u8>> {
    let nonce: [u8; 12] = generate_random();
    let kdf_params = options.kdf_params.unwrap_or_default();

    // Encrypt data
    let ciphertext = aes_gcm_encrypt(key.as_bytes(), &nonce, plaintext)?;

    // Build header
    let header = Header {
        version: 1,
        algorithm: "AES-256-GCM".to_string(),
        kdf: "argon2id".to_string(),
        kdf_params,
        salt: base64_encode(salt),
        nonce: base64_encode(&nonce),
        key_type: options.key_type,
        created_at: chrono::Utc::now(),
        original_filename: options.original_filename,
    };

    let header_json = serde_json::to_vec(&header)?;

    Ok(serialize_encrypted(FileFormat::Standard, &header_json, &ciphertext))
}

/// Encrypt data with passphrase (derives key internally)
pub fn encrypt_with_passphrase(
    plaintext: &[u8],
    passphrase: &str,
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    let salt: [u8; 32] = generate_random();
    let kdf_params = KdfParams::default();
    let key = derive_key(passphrase.as_bytes(), &salt, &kdf_params)?;

    encrypt_standard(plaintext, &key, &salt, EncryptOptions {
        key_type: KeyType::Passphrase,
        original_filename,
        kdf_params: Some(kdf_params),
    })
}

/// Encrypt data with keyfile
pub fn encrypt_with_keyfile(
    plaintext: &[u8],
    keyfile_path: &std::path::Path,
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    let key = crate::kdf::load_keyfile(keyfile_path)?;

    // For keyfile, we still use a random salt but it's not used for derivation
    let salt: [u8; 32] = generate_random();

    encrypt_standard(plaintext, &key, &salt, EncryptOptions {
        key_type: KeyType::Keyfile,
        original_filename,
        kdf_params: None,
    })
}

fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}
```

### 6.2 Decrypt Function

```rust
/// Decrypt standard encrypted file
pub fn decrypt_standard(
    encrypted: &[u8],
    key_source: KeySource,
) -> CryptoResult<(Vec<u8>, Header)> {
    let file = crate::format::EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::Standard {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_standard_header()?;

    // Get the key
    let key = match key_source {
        KeySource::Passphrase(passphrase) => {
            let salt = base64_decode(&header.salt)?;
            derive_key(passphrase.as_bytes(), &salt.try_into()?, &header.kdf_params)?
        }
        KeySource::Keyfile(path) => {
            crate::kdf::load_keyfile(path)?
        }
        KeySource::DerivedKey(key) => key,
    };

    let nonce = base64_decode(&header.nonce)?;

    // Decrypt
    let plaintext = crate::cipher::aes_gcm_decrypt(
        key.as_bytes(),
        &nonce.try_into().map_err(|_| CryptoError::HeaderParse("Invalid nonce".into()))?,
        &file.ciphertext,
    )?;

    Ok((plaintext, header))
}

pub enum KeySource<'a> {
    Passphrase(&'a str),
    Keyfile(&'a std::path::Path),
    DerivedKey(DerivedKey),
}
```

---

## 7. E2E Envelope Encryption Implementation

### 7.1 Encrypt Function

```rust
// src/e2e.rs
use crate::{
    cipher::{aes_gcm_encrypt, aes_gcm_decrypt, generate_random},
    format::{serialize_encrypted, FileFormat, E2EHeader, Recipient, KdfParams},
    kdf::derive_key,
    CryptoResult,
};

/// A recipient with their passphrase for E2E encryption
pub struct RecipientInput {
    pub id: String,
    pub passphrase: String,
}

/// Encrypt data for multiple recipients using envelope encryption
pub fn encrypt_e2e(
    plaintext: &[u8],
    recipients: &[RecipientInput],
    original_filename: Option<String>,
) -> CryptoResult<Vec<u8>> {
    if recipients.is_empty() {
        return Err(CryptoError::NoMatchingRecipient);
    }

    // Step 1: Generate random Data Encryption Key (DEK)
    let dek: [u8; 32] = generate_random();
    let data_nonce: [u8; 12] = generate_random();

    // Step 2: Encrypt the data with DEK
    let ciphertext = aes_gcm_encrypt(&dek, &data_nonce, plaintext)?;

    // Step 3: For each recipient, encrypt the DEK with their KEK
    let mut recipient_entries = Vec::with_capacity(recipients.len());

    for recipient in recipients {
        let salt: [u8; 32] = generate_random();
        let dek_nonce: [u8; 12] = generate_random();
        let kdf_params = KdfParams::default();

        // Derive Key Encryption Key (KEK) from recipient's passphrase
        let kek = derive_key(recipient.passphrase.as_bytes(), &salt, &kdf_params)?;

        // Encrypt DEK with KEK
        let encrypted_dek = aes_gcm_encrypt(kek.as_bytes(), &dek_nonce, &dek)?;

        recipient_entries.push(Recipient {
            id: recipient.id.clone(),
            kdf: "argon2id".to_string(),
            kdf_params,
            salt: base64_encode(&salt),
            encrypted_dek: base64_encode(&encrypted_dek),
            dek_nonce: base64_encode(&dek_nonce),
        });
    }

    // Step 4: Build header
    let header = E2EHeader {
        version: 1,
        algorithm: "AES-256-GCM".to_string(),
        dek_algorithm: "AES-256-GCM".to_string(),
        recipients: recipient_entries,
        data_nonce: base64_encode(&data_nonce),
        created_at: chrono::Utc::now(),
        original_filename,
    };

    let header_json = serde_json::to_vec(&header)?;

    // Step 5: Zeroize DEK
    // (handled automatically by Zeroize on drop)

    Ok(serialize_encrypted(FileFormat::E2E, &header_json, &ciphertext))
}
```

### 7.2 Decrypt Function

```rust
/// Decrypt E2E encrypted file using recipient's passphrase
pub fn decrypt_e2e(
    encrypted: &[u8],
    recipient_id: &str,
    passphrase: &str,
) -> CryptoResult<(Vec<u8>, E2EHeader)> {
    let file = crate::format::EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::E2E {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_e2e_header()?;

    // Find matching recipient
    let recipient = header.recipients.iter()
        .find(|r| r.id == recipient_id)
        .ok_or(CryptoError::NoMatchingRecipient)?;

    // Derive KEK from passphrase
    let salt: [u8; 32] = base64_decode(&recipient.salt)?
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid salt".into()))?;

    let kek = derive_key(passphrase.as_bytes(), &salt, &recipient.kdf_params)?;

    // Decrypt DEK
    let encrypted_dek = base64_decode(&recipient.encrypted_dek)?;
    let dek_nonce: [u8; 12] = base64_decode(&recipient.dek_nonce)?
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid DEK nonce".into()))?;

    let dek_bytes = aes_gcm_decrypt(kek.as_bytes(), &dek_nonce, &encrypted_dek)?;

    if dek_bytes.len() != 32 {
        return Err(CryptoError::Decryption);
    }

    let mut dek = [0u8; 32];
    dek.copy_from_slice(&dek_bytes);

    // Decrypt data with DEK
    let data_nonce: [u8; 12] = base64_decode(&header.data_nonce)?
        .try_into()
        .map_err(|_| CryptoError::HeaderParse("Invalid data nonce".into()))?;

    let plaintext = aes_gcm_decrypt(&dek, &data_nonce, &file.ciphertext)?;

    Ok((plaintext, header))
}

/// Try to decrypt with any matching recipient (auto-detect)
pub fn decrypt_e2e_auto(
    encrypted: &[u8],
    passphrase: &str,
) -> CryptoResult<(Vec<u8>, E2EHeader, String)> {
    let file = crate::format::EncryptedFile::parse(encrypted)?;

    if file.format != FileFormat::E2E {
        return Err(CryptoError::InvalidMagic);
    }

    let header = file.parse_e2e_header()?;

    // Try each recipient
    for recipient in &header.recipients {
        match decrypt_e2e(encrypted, &recipient.id, passphrase) {
            Ok((plaintext, header)) => {
                return Ok((plaintext, header, recipient.id.clone()));
            }
            Err(CryptoError::Decryption) | Err(CryptoError::Authentication) => {
                // Try next recipient
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(CryptoError::NoMatchingRecipient)
}
```

---

## 8. API Endpoint Designs

### 8.1 Modified Backup Export Endpoint

```
GET /api/v1/backup/archive
```

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| encrypt | boolean | No | Enable encryption (default: false) |
| passphrase | string | No* | Passphrase for encryption |
| keyfile_path | string | No* | Server-side path to keyfile |

*Either passphrase or keyfile_path required when encrypt=true

**Response:** Binary archive data (encrypted if requested)

**Implementation:**

```rust
#[derive(Debug, Deserialize)]
struct ArchiveExportQuery {
    encrypt: Option<bool>,
    passphrase: Option<String>,
    keyfile_path: Option<String>,
}

async fn backup_archive(
    State(state): State<AppState>,
    Query(query): Query<ArchiveExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Generate archive as before
    let archive_data = generate_archive(&state.db).await?;

    let final_data = if query.encrypt.unwrap_or(false) {
        let filename = format!("matric-backup-{}.tar.gz", timestamp());

        if let Some(passphrase) = &query.passphrase {
            // SECURITY: Don't log passphrase
            matric_crypto::encrypt_with_passphrase(&archive_data, passphrase, Some(filename))?
        } else if let Some(keyfile_path) = &query.keyfile_path {
            matric_crypto::encrypt_with_keyfile(&archive_data, Path::new(keyfile_path), Some(filename))?
        } else {
            return Err(ApiError::BadRequest("encrypt=true requires passphrase or keyfile_path".into()));
        }
    } else {
        archive_data
    };

    // Set appropriate headers
    let extension = if query.encrypt.unwrap_or(false) { ".tar.gz.enc" } else { ".tar.gz" };
    // ... return response
}
```

### 8.2 New E2E Shard Endpoint

```
POST /api/v1/backup/knowledge-shard/e2e
```

**Request Body:**

```json
{
  "recipients": [
    {"id": "alice", "passphrase": "alice-secret-phrase"},
    {"id": "bob", "passphrase": "bob-secret-phrase"}
  ],
  "include": ["notes", "collections", "tags", "links"],
  "exclude_archived": false
}
```

**Response:**

```json
{
  "success": true,
  "filename": "matric-shard-20260122-120000.shard.enc",
  "size_bytes": 1234567,
  "recipients": ["alice", "bob"],
  "base64_data": "<base64-encoded encrypted shard>"
}
```

**Implementation:**

```rust
#[derive(Debug, Deserialize)]
struct E2EShardRequest {
    recipients: Vec<RecipientInput>,
    include: Option<Vec<String>>,
    exclude_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RecipientInput {
    id: String,
    passphrase: String,
}

async fn knowledge_shard_e2e(
    State(state): State<AppState>,
    Json(body): Json<E2EShardRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate recipients
    if body.recipients.is_empty() {
        return Err(ApiError::BadRequest("At least one recipient required".into()));
    }
    if body.recipients.len() > 10 {
        return Err(ApiError::BadRequest("Maximum 10 recipients allowed".into()));
    }

    // Generate shard (unencrypted)
    let shard_data = generate_knowledge_shard(&state.db, body.include, body.exclude_archived).await?;

    // E2E encrypt
    let recipients: Vec<matric_crypto::e2e::RecipientInput> = body.recipients.into_iter()
        .map(|r| matric_crypto::e2e::RecipientInput {
            id: r.id,
            passphrase: r.passphrase,
        })
        .collect();

    let filename = format!("matric-shard-{}.shard", timestamp());
    let encrypted = matric_crypto::encrypt_e2e(&shard_data, &recipients, Some(filename.clone()))?;

    // Return as base64
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&encrypted);

    Ok(Json(serde_json::json!({
        "success": true,
        "filename": format!("{}.enc", filename),
        "size_bytes": encrypted.len(),
        "recipients": recipients.iter().map(|r| &r.id).collect::<Vec<_>>(),
        "base64_data": base64_data,
    })))
}
```

### 8.3 Decrypt Endpoint

```
POST /api/v1/backup/decrypt
```

**Request Body:**

```json
{
  "encrypted_base64": "<base64 encoded encrypted file>",
  "passphrase": "secret-passphrase",
  "recipient_id": "alice"
}
```

**Response:**

```json
{
  "success": true,
  "format": "standard|e2e",
  "original_filename": "backup.tar.gz",
  "decrypted_base64": "<base64 encoded decrypted data>"
}
```

### 8.4 Modified Import Endpoint (Auto-Detect)

```
POST /api/v1/backup/import
```

**Request Body (multipart/form-data or JSON):**

```json
{
  "file_base64": "<base64 encoded file>",
  "passphrase": "optional-if-encrypted",
  "recipient_id": "optional-for-e2e",
  "dry_run": false,
  "on_conflict": "skip|overwrite|merge"
}
```

**Implementation:**

```rust
async fn backup_import(
    State(state): State<AppState>,
    Json(body): Json<ImportRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let file_data = base64::decode(&body.file_base64)?;

    // Auto-detect encryption
    let (plaintext, was_encrypted) = match matric_crypto::detect_format(&file_data) {
        FileFormat::Standard => {
            let passphrase = body.passphrase
                .ok_or(ApiError::BadRequest("Encrypted file requires passphrase".into()))?;
            let (data, _header) = matric_crypto::decrypt_standard(&file_data,
                KeySource::Passphrase(&passphrase))?;
            (data, true)
        }
        FileFormat::E2E => {
            let passphrase = body.passphrase
                .ok_or(ApiError::BadRequest("Encrypted file requires passphrase".into()))?;

            let (data, _header, _recipient) = if let Some(recipient_id) = &body.recipient_id {
                let (d, h) = matric_crypto::decrypt_e2e(&file_data, recipient_id, &passphrase)?;
                (d, h, recipient_id.clone())
            } else {
                matric_crypto::decrypt_e2e_auto(&file_data, &passphrase)?
            };
            (data, true)
        }
        FileFormat::Unencrypted => {
            (file_data, false)
        }
    };

    // Proceed with normal import
    import_shard(&state.db, &plaintext, body.dry_run, body.on_conflict).await
}
```

---

## 9. MCP Tool Specifications

### 9.1 backup_export (Enhanced)

```javascript
{
  name: "backup_export",
  description: `Export notes as encrypted backup archive.

Options:
- encrypt: Enable encryption (default: false)
- passphrase: Passphrase for encryption (required if encrypt=true)
- include: Components to include (default: all)

Returns base64-encoded archive data that can be saved to file.`,
  inputSchema: {
    type: "object",
    properties: {
      encrypt: {
        type: "boolean",
        description: "Enable encryption",
        default: false
      },
      passphrase: {
        type: "string",
        description: "Passphrase for encryption (required if encrypt=true)"
      },
      include: {
        type: "array",
        items: { type: "string" },
        description: "Components to include: notes, collections, tags, templates, links"
      },
    },
  },
}
```

### 9.2 knowledge_shard_e2e (New)

```javascript
{
  name: "knowledge_shard_e2e",
  description: `Create E2E encrypted knowledge shard for secure sharing.

Creates a shard that can only be decrypted by the specified recipients.
Each recipient provides their own passphrase - the shard can be decrypted
by ANY recipient using their passphrase.

Use cases:
- Share knowledge between team members securely
- Create encrypted backups with multiple access keys
- Collaborative knowledge sharing with privacy

Returns base64-encoded encrypted shard data.`,
  inputSchema: {
    type: "object",
    properties: {
      recipients: {
        type: "array",
        description: "List of recipients (2-10 allowed)",
        items: {
          type: "object",
          properties: {
            id: { type: "string", description: "Recipient identifier (e.g., name or email)" },
            passphrase: { type: "string", description: "Recipient's passphrase" }
          },
          required: ["id", "passphrase"]
        }
      },
      include: {
        type: "array",
        items: { type: "string" },
        description: "Components: notes, collections, tags, links, templates, embeddings"
      }
    },
    required: ["recipients"],
  },
}
```

### 9.3 backup_decrypt (New)

```javascript
{
  name: "backup_decrypt",
  description: `Decrypt an encrypted backup or shard.

Supports both standard encryption (single passphrase) and E2E encryption
(multi-recipient). For E2E files, optionally specify recipient_id to use
a specific recipient's slot, or omit to try all slots automatically.

Returns decrypted data as base64 that can then be imported.`,
  inputSchema: {
    type: "object",
    properties: {
      encrypted_base64: {
        type: "string",
        description: "Base64-encoded encrypted file data"
      },
      passphrase: {
        type: "string",
        description: "Decryption passphrase"
      },
      recipient_id: {
        type: "string",
        description: "For E2E files: specific recipient ID (optional, auto-detect if omitted)"
      }
    },
    required: ["encrypted_base64", "passphrase"],
  },
}
```

### 9.4 backup_import (Enhanced)

```javascript
{
  name: "backup_import",
  description: `Import a backup or shard (auto-detects encryption).

Automatically detects if the file is encrypted and prompts for credentials
if needed. Supports:
- Unencrypted archives/shards
- Standard encrypted files (passphrase)
- E2E encrypted files (passphrase + optional recipient_id)

Conflict strategies:
- skip: Keep existing notes, skip duplicates
- overwrite: Replace existing notes with imported versions
- merge: Merge imported data with existing (preserves newest)`,
  inputSchema: {
    type: "object",
    properties: {
      file_base64: {
        type: "string",
        description: "Base64-encoded file data (encrypted or not)"
      },
      passphrase: {
        type: "string",
        description: "Passphrase for decryption (required for encrypted files)"
      },
      recipient_id: {
        type: "string",
        description: "For E2E files: specific recipient ID"
      },
      dry_run: {
        type: "boolean",
        description: "Validate without importing",
        default: false
      },
      on_conflict: {
        type: "string",
        enum: ["skip", "overwrite", "merge"],
        description: "How to handle duplicate notes",
        default: "skip"
      }
    },
    required: ["file_base64"],
  },
}
```

---

## 10. Security Considerations

### 10.1 Key Material Handling

```rust
// All key material uses Zeroize for automatic secure clearing
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DerivedKey {
    key: [u8; 32],
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DataEncryptionKey {
    key: [u8; 32],
}
```

### 10.2 Logging Safety

```rust
// NEVER log sensitive data
impl std::fmt::Debug for EncryptRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptRequest")
            .field("encrypt", &self.encrypt)
            .field("passphrase", &"[REDACTED]")  // Never log passphrase
            .field("keyfile_path", &self.keyfile_path)
            .finish()
    }
}
```

### 10.3 Constant-Time Operations

```rust
// Use constant-time comparison for authentication
use subtle::ConstantTimeEq;

fn verify_tag(computed: &[u8], expected: &[u8]) -> bool {
    computed.ct_eq(expected).into()
}
```

### 10.4 Input Validation

| Input | Validation |
|-------|------------|
| Passphrase | Minimum 12 characters (recommended 16+) |
| Recipient ID | 1-64 alphanumeric + underscore/dash |
| Salt | Exactly 32 bytes |
| Nonce | Exactly 12 bytes |
| Keyfile | Exactly 32 bytes (raw or base64) |

### 10.5 Threat Model

| Threat | Mitigation |
|--------|------------|
| Brute-force passphrase | Argon2id with high memory (64 MiB), 3 iterations |
| Side-channel timing | Constant-time tag verification |
| Memory dump | Zeroize on drop for all key material |
| Weak passphrase | Minimum length requirement, warn on weak passwords |
| Replay attack | Fresh random nonce per encryption |
| Key reuse | Unique salt per encryption, unique DEK nonce per recipient |

---

## 11. Error Handling

### 11.1 Error Types

```rust
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid magic bytes - not an encrypted file")]
    InvalidMagic,

    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u32),

    #[error("Header parsing failed: {0}")]
    HeaderParse(String),

    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),

    #[error("Encryption failed: {0}")]
    Encryption(String),

    #[error("Decryption failed - wrong key or corrupted data")]
    Decryption,

    #[error("Authentication failed - data may be tampered")]
    Authentication,

    #[error("No matching recipient found")]
    NoMatchingRecipient,

    #[error("Invalid keyfile: {0}")]
    InvalidKeyfile(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### 11.2 API Error Mapping

| CryptoError | HTTP Status | User Message |
|-------------|-------------|--------------|
| InvalidMagic | 400 Bad Request | "Not an encrypted file" |
| UnsupportedVersion | 400 Bad Request | "Unsupported encryption version" |
| HeaderParse | 400 Bad Request | "Corrupted file header" |
| Decryption | 401 Unauthorized | "Wrong passphrase or corrupted data" |
| NoMatchingRecipient | 401 Unauthorized | "Passphrase does not match any recipient" |
| Io | 500 Internal | "File read/write error" |

### 11.3 User-Friendly Messages

```rust
impl ApiError {
    fn from_crypto_error(e: CryptoError) -> Self {
        match e {
            CryptoError::Decryption => Self::Unauthorized(
                "Decryption failed. Please check your passphrase.".into()
            ),
            CryptoError::NoMatchingRecipient => Self::Unauthorized(
                "Your passphrase does not match any recipient in this E2E encrypted file.".into()
            ),
            CryptoError::InvalidMagic => Self::BadRequest(
                "This file is not encrypted or uses an unsupported format.".into()
            ),
            _ => Self::InternalError(e.to_string()),
        }
    }
}
```

---

## 12. Testing Strategy

### 12.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_encrypt_decrypt_roundtrip() {
        let plaintext = b"Hello, World!";
        let passphrase = "test-passphrase-123";

        let encrypted = encrypt_with_passphrase(plaintext, passphrase, None).unwrap();
        let (decrypted, _) = decrypt_standard(&encrypted, KeySource::Passphrase(passphrase)).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let plaintext = b"Secret data";
        let encrypted = encrypt_with_passphrase(plaintext, "correct-passphrase", None).unwrap();

        let result = decrypt_standard(&encrypted, KeySource::Passphrase("wrong-passphrase"));
        assert!(matches!(result, Err(CryptoError::Decryption)));
    }

    #[test]
    fn test_e2e_multi_recipient() {
        let plaintext = b"Shared secret";
        let recipients = vec![
            RecipientInput { id: "alice".into(), passphrase: "alice-pass".into() },
            RecipientInput { id: "bob".into(), passphrase: "bob-pass".into() },
        ];

        let encrypted = encrypt_e2e(plaintext, &recipients, None).unwrap();

        // Alice can decrypt
        let (dec1, _) = decrypt_e2e(&encrypted, "alice", "alice-pass").unwrap();
        assert_eq!(plaintext.as_slice(), dec1.as_slice());

        // Bob can decrypt
        let (dec2, _) = decrypt_e2e(&encrypted, "bob", "bob-pass").unwrap();
        assert_eq!(plaintext.as_slice(), dec2.as_slice());

        // Wrong passphrase fails
        assert!(decrypt_e2e(&encrypted, "alice", "wrong-pass").is_err());
    }

    #[test]
    fn test_detect_encrypted_format() {
        let plain = b"Just plain data";
        let standard = encrypt_with_passphrase(plain, "pass", None).unwrap();
        let e2e = encrypt_e2e(plain, &[RecipientInput { id: "test".into(), passphrase: "pass".into() }], None).unwrap();

        assert_eq!(detect_format(plain), FileFormat::Unencrypted);
        assert_eq!(detect_format(&standard), FileFormat::Standard);
        assert_eq!(detect_format(&e2e), FileFormat::E2E);
    }
}
```

### 12.2 Integration Tests

```rust
#[tokio::test]
async fn test_api_encrypt_export() {
    let app = create_test_app().await;

    // Create some test notes
    // ...

    // Export with encryption
    let response = app
        .get("/api/v1/backup/archive")
        .query("encrypt", "true")
        .query("passphrase", "test-passphrase")
        .send()
        .await;

    assert_eq!(response.status(), 200);
    let data = response.bytes().await;

    // Verify it's encrypted
    assert_eq!(&data[0..8], MAGIC_STANDARD);

    // Can decrypt
    let (decrypted, _) = decrypt_standard(&data, KeySource::Passphrase("test-passphrase")).unwrap();
    assert!(is_valid_tar_gz(&decrypted));
}

#[tokio::test]
async fn test_api_e2e_shard() {
    let app = create_test_app().await;

    let response = app
        .post("/api/v1/backup/knowledge-shard/e2e")
        .json(&json!({
            "recipients": [
                {"id": "alice", "passphrase": "alice-secret"},
                {"id": "bob", "passphrase": "bob-secret"}
            ]
        }))
        .send()
        .await;

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await;

    let encrypted = base64::decode(body["base64_data"].as_str().unwrap()).unwrap();

    // Both recipients can decrypt
    assert!(decrypt_e2e(&encrypted, "alice", "alice-secret").is_ok());
    assert!(decrypt_e2e(&encrypted, "bob", "bob-secret").is_ok());
}
```

### 12.3 Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn roundtrip_any_data(data: Vec<u8>, pass: String) {
        prop_assume!(pass.len() >= 12);
        prop_assume!(data.len() < 10_000_000);  // Reasonable size

        let encrypted = encrypt_with_passphrase(&data, &pass, None).unwrap();
        let (decrypted, _) = decrypt_standard(&encrypted, KeySource::Passphrase(&pass)).unwrap();

        prop_assert_eq!(data, decrypted);
    }
}
```

---

## 13. Performance Considerations

### 13.1 Key Derivation Timing

| Configuration | Time | Memory |
|---------------|------|--------|
| Default (64 MiB, 3 iter) | ~1 second | 64 MiB |
| Low-memory (32 MiB, 4 iter) | ~0.8 seconds | 32 MiB |
| High-security (128 MiB, 4 iter) | ~1.5 seconds | 128 MiB |

### 13.2 Encryption Overhead

- AES-256-GCM: ~5 GB/s on modern CPUs with AES-NI
- Overhead per file: 8 (magic) + 4 (header len) + ~500 (header) + 16 (auth tag) = ~530 bytes
- For 100 MB shard: overhead < 0.001%

### 13.3 Streaming Encryption (Future)

For files > 100 MB, consider chunked encryption:

```rust
// Future enhancement for large files
pub struct StreamingEncryptor {
    cipher: Aes256Gcm,
    chunk_size: usize,
    chunk_counter: u64,
}
```

---

## 14. Implementation Roadmap

### Phase 1: Core Crypto Module (Week 1)

1. Create `matric-crypto` crate structure
2. Implement KDF (Argon2id)
3. Implement cipher operations (AES-256-GCM)
4. Implement file format parsing/writing
5. Unit tests for all primitives

### Phase 2: Standard Encryption (Week 2)

1. Implement `encrypt_standard` / `decrypt_standard`
2. Implement format detection
3. Add API endpoint modifications for backup/export
4. Integration tests

### Phase 3: E2E Encryption (Week 2-3)

1. Implement `encrypt_e2e` / `decrypt_e2e`
2. Add new `/knowledge-shard/e2e` endpoint
3. Add decrypt endpoint
4. E2E integration tests

### Phase 4: MCP Tools (Week 3)

1. Update `backup_export` tool
2. Add `knowledge_shard_e2e` tool
3. Add `backup_decrypt` tool
4. Update `backup_import` tool
5. MCP integration tests

### Phase 5: Documentation & Polish (Week 4)

1. Update OpenAPI spec
2. User documentation
3. Security review
4. Performance testing
5. Edge case handling

---

## 15. Architectural Decision Records (ADRs)

### ADR-001: Symmetric-Only Encryption for v1.0

**Status:** Accepted

**Context:** Requirements specify passphrase and keyfile encryption. Public key crypto (RSA/ECC) is explicitly out of scope for v1.0.

**Decision:** Use symmetric encryption only with passphrase-derived keys via Argon2id.

**Consequences:**
- Simpler implementation
- No key management infrastructure needed
- Sharing requires communicating passphrase out-of-band
- Future versions can add asymmetric crypto

### ADR-002: Envelope Encryption for E2E Multi-Recipient

**Status:** Accepted

**Context:** Need to encrypt shards for multiple recipients without re-encrypting the entire shard for each recipient.

**Decision:** Use envelope encryption - generate random DEK, encrypt data once with DEK, encrypt DEK separately for each recipient with their KEK.

**Consequences:**
- Efficient: Data encrypted once regardless of recipient count
- Scalable: Adding recipients is O(1) per recipient
- Each recipient uses their own passphrase
- DEK must be securely zeroized after use

### ADR-003: Magic Bytes for Format Detection

**Status:** Accepted

**Context:** Need to auto-detect encrypted vs. unencrypted files for seamless import experience.

**Decision:** Use 8-byte ASCII magic identifiers at file start ("MMENC01\x00" and "MME2E01\x00").

**Consequences:**
- Instant format detection without parsing entire header
- Clear version identification for future format changes
- No collision with tar.gz magic (0x1f 0x8b)
- Slightly increased file size (~8 bytes)

### ADR-004: JSON Headers Over Binary

**Status:** Accepted

**Context:** Need to store encryption metadata (algorithm, KDF params, nonces, etc.).

**Decision:** Use JSON for header format instead of binary encoding.

**Consequences:**
- Human-readable for debugging
- Easily extensible for future parameters
- Slightly larger than binary encoding
- No endianness concerns
- Standard tooling for parsing

### ADR-005: In-Memory Encryption vs Streaming

**Status:** Accepted

**Context:** Need to encrypt backup archives and shards which can be large.

**Decision:** For v1.0, use in-memory encryption. Add streaming for files > 100 MB in future version.

**Consequences:**
- Simpler implementation
- Works well for typical shard sizes (< 50 MB)
- Memory usage = 2x file size during encryption
- Future optimization path clear

---

## 16. Appendix

### A. Wire Format Examples

**Standard Encrypted File (hex dump of first 100 bytes):**

```
4D4D454E 43303100  00000142 7B227665  MMENC01.....{"ve
7273696F 6E223A31 2C22616C 676F7269  rsion":1,"algori
74686D22 3A224145 532D3235 362D4743  thm":"AES-256-GC
4D222C22 6B646622 3A226172 676F6E32  M","kdf":"argon2
69642...                             id...
```

**E2E Header Example (JSON):**

```json
{
  "version": 1,
  "algorithm": "AES-256-GCM",
  "dek_algorithm": "AES-256-GCM",
  "recipients": [
    {
      "id": "alice",
      "kdf": "argon2id",
      "kdf_params": {"memory_kib": 65536, "iterations": 3, "parallelism": 4},
      "salt": "rJ3f2Qk9...",
      "encrypted_dek": "Xm7pL9a2...",
      "dek_nonce": "a8Kj2nM..."
    }
  ],
  "data_nonce": "p9Lm3kN...",
  "created_at": "2026-01-22T12:00:00Z",
  "original_filename": "shared.shard"
}
```

### B. Dependency Versions

```toml
# Verified compatible versions
aes-gcm = "0.10.3"
argon2 = "0.5.3"
rand = "0.8.5"
zeroize = "1.7.0"
base64 = "0.22.1"
```

### C. Passphrase Strength Guidelines

| Strength | Min Length | Entropy | Use Case |
|----------|------------|---------|----------|
| Minimum | 12 chars | 60 bits | Short-term protection |
| Recommended | 16 chars | 80 bits | General use |
| High Security | 24 chars | 120 bits | Long-term archives |

---

*Document version: 1.0*
*Last updated: 2026-01-22*
