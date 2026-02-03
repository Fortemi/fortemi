# Encryption Guide

This guide covers the encryption features in Fortémi for securing data using public-key encryption.

## Overview

Fortémi uses Public Key Encryption (PKE) for all encryption operations. This provides wallet-style encryption where users share public key addresses (similar to cryptocurrency wallets) instead of exchanging passphrases.

| Feature | Description |
|---------|-------------|
| **Format** | MMPKE01 |
| **Key Exchange** | X25519 (Curve25519 ECDH) |
| **Encryption** | AES-256-GCM |
| **Address Format** | `mm:` prefix + Base58Check |

For detailed PKE documentation, see the [PKE Encryption Guide](./pke-encryption.md).

## Security Properties

The encryption system provides:

- **Confidentiality** - AES-256-GCM encryption (256-bit key)
- **Integrity** - AEAD authentication tag prevents tampering
- **Key Protection** - Private keys encrypted with Argon2id + AES-256-GCM
- **Forward Secrecy** - Ephemeral keypair per encryption operation
- **Secure Memory** - Keys zeroized immediately after use
- **Multi-Recipient** - Encrypt once for multiple recipients

### Cryptographic Primitives

| Component | Algorithm | Parameters |
|-----------|-----------|------------|
| Key Exchange | X25519 | Curve25519 ECDH |
| KEK Derivation | HKDF-SHA256 | With domain separation |
| Symmetric Cipher | AES-256-GCM | 256-bit key, 96-bit nonce |
| Address Hash | BLAKE3 | 20-byte truncated + 4-byte checksum |
| Private Key Storage | Argon2id + AES-256-GCM | 64 MiB, 3 iterations |
| Random Generation | ChaCha20 | OS entropy source |

## Quick Start

### Generate a Keypair

```bash
# Using CLI
matric-pke keygen -p "your-secure-passphrase-123" -o ~/.matric-keys

# Output:
# {
#   "address": "mm:ULnxkCj4TCc8QnFsar8Be4DVV4TkWivXE",
#   "private_key_path": "/home/user/.matric-keys/private.key.enc",
#   "public_key_path": "/home/user/.matric-keys/public.key"
# }
```

### Share Your Address

Share your public address (`mm:...`) with anyone who needs to send you encrypted data. Your address is safe to share publicly - it cannot be used to decrypt anything.

### Encrypt for Recipients

```bash
# Encrypt for one or more recipients
matric-pke encrypt \
  -i document.pdf \
  -o document.pdf.mmpke \
  -r /path/to/alice.pub \
  -r /path/to/bob.pub
```

### Decrypt with Your Private Key

```bash
# Decrypt using your private key
matric-pke decrypt \
  -i document.pdf.mmpke \
  -o document.pdf \
  -k ~/.matric-keys/private.key.enc \
  -p "your-secure-passphrase-123"
```

## File Format (MMPKE01)

```
┌──────────────────────────────────────┐
│ Magic: "MMPKE01\n"                   │ 8 bytes
├──────────────────────────────────────┤
│ Header Length                        │ 4 bytes (little-endian u32)
├──────────────────────────────────────┤
│ Header (JSON)                        │ Variable
│ {                                    │
│   "version": 1,                      │
│   "ephemeral_pubkey": "<base64>",    │
│   "recipients": [                    │
│     {                                │
│       "address": "mm:...",           │
│       "encrypted_dek": "<base64>",   │
│       "dek_nonce": "<base64>"        │
│     },                               │
│     ...                              │
│   ],                                 │
│   "data_nonce": "<base64>",          │
│   "created_at": "2026-01-22T...",    │
│   "original_filename": "doc.pdf"     │
│ }                                    │
├──────────────────────────────────────┤
│ Encrypted Data                       │ Variable
│ (ciphertext + 16-byte auth tag)      │
└──────────────────────────────────────┘
```

## MCP Tools

### pke_generate_keypair

Generate a new X25519 keypair.

```javascript
pke_generate_keypair({
  passphrase: "secure-passphrase-123",
  output_dir: "/path/to/keys",
  label: "My Key"  // optional
})
```

### pke_encrypt

Encrypt data for recipients.

```javascript
pke_encrypt({
  input: "/path/to/file.pdf",
  output: "/path/to/file.pdf.mmpke",
  recipients: ["/path/to/alice.pub", "/path/to/bob.pub"]
})
```

### pke_decrypt

Decrypt data with your private key.

```javascript
pke_decrypt({
  input: "/path/to/file.pdf.mmpke",
  output: "/path/to/file.pdf",
  private_key: "/path/to/private.key.enc",
  passphrase: "your-passphrase"
})
```

### pke_list_recipients

List recipient addresses without decrypting.

```javascript
pke_list_recipients({
  input: "/path/to/file.mmpke"
})
// Returns: { recipients: ["mm:abc...", "mm:xyz..."], count: 2 }
```

### pke_get_address

Get the address for a public key.

```javascript
pke_get_address({
  public_key: "/path/to/public.key"
})
// Returns: { address: "mm:..." }
```

### pke_verify_address

Verify an address checksum.

```javascript
pke_verify_address({
  address: "mm:ULnxkCj4TCc8QnFsar8Be4DVV4TkWivXE"
})
// Returns: { valid: true, version: 1 }
```

## Best Practices

### Key Management

1. **Protect your private key** - Use a strong passphrase (20+ characters)
2. **Back up your keys** - Store copies in secure, separate locations
3. **Share only addresses** - Never share private keys or passphrases

### Passphrase Selection

1. **Use strong passphrases** - At least 12 characters (20+ recommended)
2. **Use a password manager** - Store passphrases securely
3. **Don't reuse passphrases** - Use unique passphrases for each key

### Recipient Management

1. **Verify addresses** - Confirm addresses through a trusted channel
2. **Check recipients before encrypting** - Use `pke_list_recipients` to verify
3. **Limit recipients** - Maximum 100 recipients per file

### General

1. **Test decryption** - Verify you can decrypt before deleting originals
2. **Multiple backups** - Follow the 3-2-1 backup rule
3. **Rotate keys periodically** - Generate new keypairs for long-term security

## Troubleshooting

### "Passphrase too short"

Passphrases must be at least 12 characters. Use a longer, more secure passphrase.

### "No recipient block found for address"

- Your address is not in the recipient list
- Use `pke_list_recipients` to see valid recipient addresses
- Ask the sender to re-encrypt with your public key

### "Failed to decrypt DEK - wrong key?"

- Wrong private key
- File was corrupted during transfer
- Check that your key matches one of the recipients

### "Invalid magic bytes"

The file is not in MMPKE01 format. It may be:
- A different encryption format
- Not encrypted at all
- Corrupted

### Memory issues during key derivation

Argon2id uses 64 MiB of memory by default for private key encryption. This is intentional to resist GPU/ASIC attacks.

## Security Considerations

### What's Protected

- File contents (confidentiality)
- File integrity (tampering detection)
- Key material in memory (zeroized after use)
- Forward secrecy (ephemeral keys per encryption)

### What's NOT Protected

- File existence (encrypted files are visible)
- File size (length is observable)
- Recipient addresses (visible in header without decryption)
- Encryption metadata (algorithm, timestamp visible in header)

### Threat Model

The encryption protects against:
- Unauthorized access to files
- Data theft from storage devices
- Cloud storage provider access
- Passive network attackers

It does NOT protect against:
- Compromised endpoints (malware on your system)
- Private key compromise
- Coerced disclosure (legal/physical threats)

## Related Documentation

- [PKE Encryption Guide](./pke-encryption.md) - Detailed PKE documentation
- [Shard Exchange Primer](./shard-exchange.md) - Practical workflows for sharing and recovering knowledge
- [Backup Guide](./backup.md) - Backup and restore procedures
- [Architecture](./architecture.md) - System design overview
