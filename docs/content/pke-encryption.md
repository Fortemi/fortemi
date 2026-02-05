# Public Key Encryption (PKE) Guide

This guide covers the wallet-style public key encryption system for secure sharing without exchanging passphrases.

## Overview

Unlike traditional passphrase-based encryption where secrets must be shared between parties, PKE uses asymmetric cryptography:

- **Public key address** (`mm:...`) - Shareable identifier, like a crypto wallet address
- **Private key** - Secret, stored encrypted on disk, never shared
- Senders encrypt using only the recipient's public address
- Only the private key holder can decrypt

## Quick Start

### Generate Your Identity

```rust
use matric_crypto::pke::{Keypair, save_private_key, save_public_key};

// Generate a new keypair
let keypair = Keypair::generate();

// Get your public address to share with others
let my_address = keypair.public.to_address();
println!("My address: {}", my_address);
// Example: mm:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ

// Save your keys
save_private_key(&keypair.private, "/path/to/private.key", "your-passphrase")?;
save_public_key(&keypair.public, "/path/to/public.key", Some("My Key"))?;
```

### Share Your Address

Your address (`mm:...`) can be shared publicly. It's derived from your public key using BLAKE3 hashing and Base58Check encoding with a checksum to catch typos.

```text
Address format: mm:<base58check-encoded-data>

Example: mm:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ
```

### Encrypt for Recipients

To encrypt data, you only need the recipients' public keys (which can be derived from their addresses):

```rust
use matric_crypto::pke::{encrypt_pke, Keypair};

// Recipient public keys
let alice = Keypair::generate();
let bob = Keypair::generate();

// Encrypt for multiple recipients
let secret_data = b"Confidential information";
let encrypted = encrypt_pke(
    secret_data,
    &[alice.public.clone(), bob.public.clone()],
    Some("data.json".into())
)?;

// Save encrypted data to file
std::fs::write("encrypted.mmpke", &encrypted)?;
```

### Decrypt with Your Private Key

```rust
use matric_crypto::pke::{decrypt_pke, load_private_key};

// Load your private key
let private_key = load_private_key("/path/to/private.key", "your-passphrase")?;

// Decrypt
let encrypted = std::fs::read("encrypted.mmpke")?;
let (plaintext, header) = decrypt_pke(&encrypted, &private_key)?;

println!("Decrypted {} bytes", plaintext.len());
println!("Original filename: {:?}", header.original_filename);
```

## Address Format

Addresses use a Bitcoin-style format with built-in error detection:

```text
┌─────────────────────────────────────────────────────────┐
│ mm:<version><hash><checksum>                            │
│                                                          │
│ - Prefix: "mm:" (Fortémi)                          │
│ - Version: 1 byte (0x01 for v1)                          │
│ - Hash: 20 bytes of BLAKE3(public_key)                   │
│ - Checksum: 4 bytes of BLAKE3(version || hash)           │
│ - Encoding: Base58 (Bitcoin alphabet)                    │
└─────────────────────────────────────────────────────────┘
```

**Properties:**
- ~45 characters total length
- No confusing characters (0, O, I, l excluded)
- Checksum catches typos and copy-paste errors
- Version byte allows future format upgrades

### Address Validation

```rust
use matric_crypto::pke::Address;

// Parse and validate an address
let addr: Address = "mm:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ".parse()?;

// Verify checksum
assert!(addr.verify_checksum());
```

## File Format (MMPKE01)

Encrypted files use the MMPKE01 format:

```text
┌─────────────────────────────────────────────────────────────┐
│ Magic: "MMPKE01\n" (8 bytes)                                │
├─────────────────────────────────────────────────────────────┤
│ Header Length: u32 LE (4 bytes)                             │
├─────────────────────────────────────────────────────────────┤
│ Header (JSON):                                              │
│ {                                                           │
│   "version": 1,                                             │
│   "ephemeral_pubkey": "<base64>",                           │
│   "recipients": [                                           │
│     {                                                       │
│       "address": "mm:ABC123...",                            │
│       "encrypted_dek": "<base64>",                          │
│       "dek_nonce": "<base64>"                               │
│     }                                                       │
│   ],                                                        │
│   "data_nonce": "<base64>",                                 │
│   "original_filename": "backup.json"                        │
│ }                                                           │
├─────────────────────────────────────────────────────────────┤
│ Encrypted Data (AES-256-GCM ciphertext + 16-byte tag)       │
└─────────────────────────────────────────────────────────────┘
```

### Inspecting Encrypted Files

```rust
use matric_crypto::pke::get_pke_recipients;

let encrypted = std::fs::read("encrypted.mmpke")?;
let recipients = get_pke_recipients(&encrypted)?;

println!("This file can be decrypted by:");
for addr in recipients {
    println!("  {}", addr);
}
```

## Cryptographic Details

### Algorithms Used

| Component | Algorithm | Purpose |
|-----------|-----------|---------|
| Key Exchange | X25519 (Curve25519) | ECDH shared secret derivation |
| Key Derivation | HKDF-SHA256 | Derive encryption keys from shared secrets |
| Symmetric Encryption | AES-256-GCM | Authenticated encryption |
| Address Hashing | BLAKE3 | Fast, secure address derivation |

### Encryption Flow

```text
ENCRYPTION (sender → recipients)

1. Generate ephemeral X25519 keypair
2. Generate random DEK (Data Encryption Key, 32 bytes)
3. For each recipient:
   a. ECDH: ephemeral_private + recipient_public → shared_secret
   b. HKDF: shared_secret → KEK (Key Encryption Key)
   c. AES-GCM: encrypt DEK with KEK
4. AES-GCM: encrypt plaintext with DEK
5. Serialize MMPKE01 format with ephemeral public key
```

### Decryption Flow

```text
DECRYPTION (recipient)

1. Parse MMPKE01 header
2. Find recipient block matching our address
3. ECDH: my_private + ephemeral_public → shared_secret
4. HKDF: shared_secret → KEK
5. AES-GCM: decrypt encrypted_dek with KEK → DEK
6. AES-GCM: decrypt ciphertext with DEK → plaintext
```

## Security Properties

### Forward Secrecy

Each encryption operation generates a fresh ephemeral keypair. Even if a recipient's long-term private key is compromised later, past encrypted messages remain secure because the ephemeral keys are not stored.

### Multi-Recipient Efficiency

The data is encrypted only once (with the DEK). Adding more recipients only adds small KEK-wrapped DEK blocks to the header, not re-encryption of the full payload.

### Key Protection

Private keys are encrypted at rest using:
- Argon2id key derivation (memory-hard, GPU-resistant)
- AES-256-GCM encryption
- MMPKEKEY format with secure salt and nonce generation

### Tamper Detection

AES-256-GCM is an AEAD (Authenticated Encryption with Associated Data) cipher. Any modification to the ciphertext or header will be detected during decryption.

## Key Management

### Generating Keys

```rust
use matric_crypto::pke::Keypair;

// Generate new keypair
let keypair = Keypair::generate();

// Or from an existing private key
let keypair = Keypair::from_private(existing_private_key);
```

### Saving Keys

```rust
use matric_crypto::pke::{save_private_key, save_public_key};

// Private key (encrypted with passphrase)
save_private_key(&keypair.private, "~/.matric/private.key", "strong-passphrase")?;

// Public key (plaintext, shareable)
save_public_key(&keypair.public, "~/.matric/public.key", Some("Work Key"))?;
```

### Loading Keys

```rust
use matric_crypto::pke::{load_private_key, load_public_key};

let private = load_private_key("~/.matric/private.key", "passphrase")?;
let public = load_public_key("~/.matric/public.key")?;
```

### Key Backup

**Critical**: Back up your private key securely. If lost, you cannot decrypt any data encrypted for your address.

Recommended backup strategies:
1. **Encrypted USB drive** - Store the encrypted private key file
2. **Paper backup** - Print the passphrase and store securely
3. **Password manager** - Store the passphrase with key file location

## Error Handling

### Common Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `InvalidAddress` | Typo or corrupted address | Verify checksum, re-copy address |
| `Decryption failed` | Wrong key or corrupted data | Use correct private key |
| `No recipient block` | Not an intended recipient | Check you have the right file |
| `PassphraseTooShort` | Passphrase < 12 characters | Use a longer passphrase |

### Checking Decryptability

```rust
use matric_crypto::pke::can_decrypt_pke;

if can_decrypt_pke(&encrypted, &my_private_key) {
    println!("I can decrypt this file");
} else {
    println!("I am not a recipient");
}
```

## Best Practices

1. **Use strong passphrases** for private key files (12+ characters)
2. **Verify addresses** before encrypting sensitive data
3. **Back up private keys** securely
4. **Rotate keys** periodically for long-term security
5. **Use unique addresses** for different contexts (work, personal)

## Related Documentation

- [Encryption Overview](./encryption.md) - PKE encryption guide
- [Architecture](./architecture.md) - System design
