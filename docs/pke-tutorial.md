# Public Key Encryption: An Illustrated Primer

A visual guide to understanding how public key encryption works, featuring Alice and Bob.

## Table of Contents

- [Chapter 1: The Key Ceremony](#chapter-1-the-key-ceremony)
- [Chapter 2: Bob Wants to Send a Secret](#chapter-2-bob-wants-to-send-a-secret)
- [Chapter 3: The Magic Inside](#chapter-3-the-magic-inside)
- [Chapter 4: Sending the File](#chapter-4-sending-the-file)
- [Chapter 5: Alice Decrypts](#chapter-5-alice-decrypts)
- [Chapter 6: Multi-Recipient Encryption](#chapter-6-multi-recipient-encryption)
- [Summary](#summary)

---

## Introduction

Public Key Encryption (PKE) solves a fundamental problem: **How can two people communicate securely without first meeting to exchange a secret password?**

The answer is elegant: each person has TWO keys:
- A **public key** they share with everyone (like a mailing address)
- A **private key** they keep secret (like the key to their mailbox)

Anyone can drop a letter in the mailbox, but only the owner can open it.

---

## Chapter 1: The Key Ceremony

Alice wants to receive encrypted messages. First, she generates her keypair:

```
┌─────────────────────────────────────────────────────────────┐
│                    ALICE'S COMPUTER                         │
│                                                             │
│   Generating keypair...                                     │
│                                                             │
│   ┌──────────────────────────────────────────────┐         │
│   │                                              │         │
│   │  [████████████████████████████] 100%         │         │
│   │                                              │         │
│   │  ✓ Private key generated                     │         │
│   │    (stored encrypted with passphrase)        │         │
│   │                                              │         │
│   │  ✓ Public key derived                        │         │
│   │                                              │         │
│   │  Your public address:                        │         │
│   │  ┌────────────────────────────────────────┐ │         │
│   │  │ pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ │ │         │
│   │  └────────────────────────────────────────┘ │         │
│   └──────────────────────────────────────────────┘         │
│                                                             │
│   Alice's Keys:                                             │
│   ├── private.key  (secret! encrypted with passphrase)     │
│   └── public.key   (shareable with anyone)                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

Alice shares her public address on her website, like a cryptocurrency wallet:

```
┌─────────────────────────────────────────────────────────────┐
│  alice.example.com                                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Alice's Secure Inbox                                      │
│                                                             │
│   Want to send me something private?                        │
│   Encrypt it using my public key:                           │
│                                                             │
│   ╔════════════════════════════════════════════════════╗   │
│   ║  pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ            ║   │
│   ╚════════════════════════════════════════════════════╝   │
│                                                             │
│   [Copy to Clipboard]                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Key Points:**
- The public address (`pk:...`) is safe to share with the entire world
- The private key stays on Alice's computer, protected by a passphrase
- Anyone can encrypt data FOR Alice using just her public key
- Only Alice can decrypt it (with her private key)

---

## Chapter 2: Bob Wants to Send a Secret

Bob has a confidential document for Alice. He visits her website and copies her public key:

```
┌─────────────────────────────────────────────────────────────┐
│                      BOB'S COMPUTER                         │
│                                                             │
│   secret-proposal.pdf                                       │
│   ┌─────────────────────────────────────────────┐          │
│   │                                             │          │
│   │   CONFIDENTIAL: Project Moonshot           │          │
│   │                                             │          │
│   │   Budget: $10,000,000                       │          │
│   │   Timeline: Q3 2026                         │          │
│   │   ...                                       │          │
│   │                                             │          │
│   └─────────────────────────────────────────────┘          │
│                                                             │
│   Bob doesn't have Alice's passphrase.                      │
│   Bob doesn't NEED it! He only needs her PUBLIC KEY.        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

Bob encrypts the file using Alice's public key:

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   Encrypting for recipient:                                 │
│     pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ                   │
│                                                             │
│   ┌──────────────────────────────────────────────┐         │
│   │                                              │         │
│   │   ✓ Generating ephemeral keypair...          │         │
│   │   ✓ Performing key exchange (ECDH)...        │         │
│   │   ✓ Generating random data key...            │         │
│   │   ✓ Encrypting data key for recipient...     │         │
│   │   ✓ Encrypting document...                   │         │
│   │                                              │         │
│   │   Done! Created: secret-proposal.pdf.enc     │         │
│   │     Size: 1.2 MB                             │         │
│   │     Recipients: 1                            │         │
│   │                                              │         │
│   └──────────────────────────────────────────────┘         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Chapter 3: The Magic Inside

What actually happens during encryption? This is where it gets interesting.

### The Problem with Simple Encryption

You might ask: "Why not just encrypt directly with Alice's public key?"

The answer: **Public key encryption is slow.** Encrypting large files directly would take forever.

### The Solution: Hybrid Encryption

Modern PKE uses a clever two-layer approach:

1. Generate a random **Data Encryption Key (DEK)** - just 32 random bytes
2. Encrypt the actual data with the DEK (fast symmetric encryption)
3. Encrypt the DEK with the recipient's public key (slow but tiny)

```
                     THE ENCRYPTION PROCESS

┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   1. Generate ephemeral keypair (one-time use)              │
│                                                             │
│      ┌──────────────┐                                       │
│      │ ephemeral_sk │──┐   (Secret: discarded after use)    │
│      │ ephemeral_pk │  │   (Public: stored in output file)  │
│      └──────────────┘  │                                    │
│                        │                                    │
│   2. ECDH key exchange │                                    │
│                        │                                    │
│      ┌──────────────┐  │    ┌───────────────┐              │
│      │ ephemeral_sk │──┼───>│ shared_secret │              │
│      └──────────────┘  │    └───────┬───────┘              │
│                        │            │                       │
│      ┌──────────────┐  │            │                       │
│      │ alice_pubkey │──┘            │                       │
│      └──────────────┘               │                       │
│                                     v                       │
│   3. Derive KEK (Key Encryption Key)                        │
│                                                             │
│      ┌───────────────┐    ┌──────────┐                     │
│      │ shared_secret │───>│ HKDF-256 │───> KEK             │
│      └───────────────┘    └──────────┘                     │
│                                                             │
│   4. Generate random DEK (Data Encryption Key)              │
│                                                             │
│      ┌────────────────────────────┐                        │
│      │  DEK = 32 random bytes     │                        │
│      └────────────────────────────┘                        │
│                                                             │
│   5. Wrap (encrypt) DEK with KEK                            │
│                                                             │
│      ┌─────┐   ┌──────────────┐   ┌──────────────┐         │
│      │ DEK │ + │ AES-256-GCM  │ = │ wrapped_dek  │         │
│      └─────┘   └──────────────┘   └──────────────┘         │
│                       ^                                     │
│                       │                                     │
│                      KEK                                    │
│                                                             │
│   6. Encrypt the document with DEK                          │
│                                                             │
│      ┌───────────┐   ┌──────────────┐   ┌────────────┐     │
│      │ plaintext │ + │ AES-256-GCM  │ = │ ciphertext │     │
│      └───────────┘   └──────────────┘   └────────────┘     │
│                            ^                                │
│                            │                                │
│                           DEK                               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Why Ephemeral Keys?

Each encryption generates a fresh "ephemeral" keypair. Why?

**Forward Secrecy**: If Alice's private key is compromised later, past messages remain secure. Each message used a different ephemeral key that was discarded after encryption.

### The Encrypted File Structure

The output file contains everything needed for decryption:

```
secret-proposal.pdf.enc
┌─────────────────────────────────────────────────────────────┐
│                         HEADER                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Ephemeral Public Key:  kL3mN9pQ2rS5tU8vW...               │
│                         (Bob threw away the private half)   │
│                                                             │
│  Recipients:                                                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Address: pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ    │    │
│  │ Wrapped DEK: xY4zA7bC2dE5fG8hI...                  │    │
│  │ Nonce: jK9lM0nO1pQ2rS3t                            │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
│  Data Nonce: uV4wX5yZ6aB7cD8e                              │
│  Original Filename: secret-proposal.pdf                     │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    ENCRYPTED DATA                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   a7f3b2c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9...    │
│   (AES-256-GCM ciphertext with authentication tag)          │
│                                                             │
│   The actual document, encrypted with the DEK               │
│   (1.2 MB of unreadable ciphertext)                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Chapter 4: Sending the File

Bob sends the encrypted file to Alice. Since it's encrypted, he can use ANY channel:

```
┌───────────────────────────────────────────────────────────┐
│  Email                                                    │
│                                                           │
│  From: bob@example.com                                    │
│  To: alice@example.com                                    │
│  Subject: Project Moonshot Proposal                       │
│                                                           │
│  ─────────────────────────────────────────────────────── │
│                                                           │
│  Hi Alice,                                                │
│                                                           │
│  Please find the proposal attached. I encrypted it        │
│  to your public key:                                      │
│  pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ                    │
│                                                           │
│  Only you can decrypt it.                                 │
│                                                           │
│  - Bob                                                    │
│                                                           │
│  [Attachment] secret-proposal.pdf.enc (1.2 MB)            │
│                                                           │
└───────────────────────────────────────────────────────────┘

                          │
                          │  The Internet
                          │
                          │  (Email servers, ISPs, hackers,
                          │   government agencies can all
                          │   SEE the file... but they can't
                          │   READ its contents!)
                          │
                          v

┌───────────────────────────────────────────────────────────┐
│  Alice's Inbox                                            │
│                                                           │
│  📨 New message from Bob                                  │
│     [Attachment] secret-proposal.pdf.enc                  │
│                                                           │
└───────────────────────────────────────────────────────────┘
```

**Security Note:** The encrypted file can safely travel over insecure channels. Even if intercepted, attackers see only random-looking bytes. The contents are protected by military-grade AES-256-GCM encryption.

---

## Chapter 5: Alice Decrypts

Alice receives the file and decrypts it with her private key:

```
┌─────────────────────────────────────────────────────────────┐
│                    ALICE'S COMPUTER                         │
│                                                             │
│   Decrypting: secret-proposal.pdf.enc                       │
│   Enter passphrase for private key: ••••••••••••            │
│                                                             │
│   ┌──────────────────────────────────────────────┐         │
│   │                                              │         │
│   │   ✓ Unlocking private key...                 │         │
│   │   ✓ Found my recipient block                 │         │
│   │   ✓ Performing key exchange (ECDH)...        │         │
│   │   ✓ Unwrapping data key...                   │         │
│   │   ✓ Decrypting document...                   │         │
│   │   ✓ Verifying integrity...                   │         │
│   │                                              │         │
│   │   Success! Output: secret-proposal.pdf       │         │
│   │                                              │         │
│   └──────────────────────────────────────────────┘         │
│                                                             │
│   secret-proposal.pdf                                       │
│   ┌─────────────────────────────────────────────┐          │
│   │                                             │          │
│   │   CONFIDENTIAL: Project Moonshot           │          │
│   │                                             │          │
│   │   Budget: $10,000,000                       │          │
│   │   Timeline: Q3 2026                         │          │
│   │   ...                                       │          │
│   │                                             │          │
│   └─────────────────────────────────────────────┘          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### The Decryption Flow

```
                     ALICE'S DECRYPTION PROCESS

┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   1. Parse header from encrypted file                       │
│                                                             │
│      ┌──────────────────┐                                   │
│      │ ephemeral_pubkey │  (from Bob's encryption)          │
│      │ recipients[]     │                                   │
│      │ data_nonce       │                                   │
│      └──────────────────┘                                   │
│                                                             │
│   2. Find my recipient block (matching my address)          │
│                                                             │
│      ┌──────────────────────────────────────────┐          │
│      │ address: pk:7Xq9KmPvR3nYhW2s...          │ ◄─ Me!   │
│      │ wrapped_dek: xY4zA7bC2dE5fG...           │          │
│      │ nonce: jK9lM0nO1pQ2rS3t                  │          │
│      └──────────────────────────────────────────┘          │
│                                                             │
│   3. ECDH key exchange (reverse of Bob's process)           │
│                                                             │
│      ┌──────────────┐       ┌───────────────┐              │
│      │ my_private   │──┐    │               │              │
│      └──────────────┘  ├───>│ shared_secret │              │
│      ┌──────────────┐  │    │               │              │
│      │ephemeral_pub │──┘    └───────┬───────┘              │
│      └──────────────┘               │                       │
│                                     │                       │
│   The magic: this produces the SAME shared secret           │
│   that Bob computed, even though Alice never saw            │
│   Bob's ephemeral private key!                              │
│                                     │                       │
│                                     v                       │
│   4. Derive KEK (same as Bob did)                           │
│                                                             │
│      ┌───────────────┐    ┌──────────┐                     │
│      │ shared_secret │───>│ HKDF-256 │───> KEK             │
│      └───────────────┘    └──────────┘                     │
│                                                             │
│   5. Unwrap DEK                                             │
│                                                             │
│      ┌─────────────┐   ┌──────────────┐   ┌─────┐          │
│      │ wrapped_dek │ + │ AES-256-GCM  │ = │ DEK │          │
│      └─────────────┘   └──────────────┘   └─────┘          │
│                              ^                              │
│                              │                              │
│                             KEK                             │
│                                                             │
│   6. Decrypt the document                                   │
│                                                             │
│      ┌────────────┐   ┌──────────────┐   ┌───────────┐     │
│      │ ciphertext │ + │ AES-256-GCM  │ = │ plaintext │     │
│      └────────────┘   └──────────────┘   └───────────┘     │
│                             ^                               │
│                             │                               │
│                            DEK                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### The Mathematical Magic

The core of PKE relies on **Elliptic Curve Diffie-Hellman (ECDH)**:

```
Bob computes:    ephemeral_private × Alice_public  = shared_secret
Alice computes:  Alice_private × ephemeral_public  = shared_secret
                                                     ↑
                                              Same result!
```

This works because of elliptic curve math. Neither party reveals their private key, yet they arrive at the same shared secret.

---

## Chapter 6: Multi-Recipient Encryption

What if Bob needs to send the same document to both Alice AND Carol?

```
Carol also has a keypair:

┌────────────────────────────────────────────┐
│  Carol's public key:                       │
│  pk:8Yr0LnQwS4oZiX3uK7fMcN2dV9hJ6pE5wT    │
└────────────────────────────────────────────┘
```

Bob encrypts for BOTH recipients in a single operation:

```
Encrypting for recipients:
  • pk:7Xq9KmPvR3nYhW2sT8uJcL4bN6aF5gD1eZ  (Alice)
  • pk:8Yr0LnQwS4oZiX3uK7fMcN2dV9hJ6pE5wT  (Carol)
```

### How Multi-Recipient Works

The clever insight: **encrypt the data once, wrap the key multiple times.**

```
                    MULTI-RECIPIENT ENCRYPTION

       ┌─────────────────────────────────────────┐
       │          Single DEK                     │
       │        (32 random bytes)                │
       └───────────┬─────────────┬──────────────┘
                   │             │
          ┌────────v───────┐  ┌──v─────────────┐
          │ Wrapped for    │  │ Wrapped for    │
          │ Alice's key    │  │ Carol's key    │
          │                │  │                │
          │ (Different     │  │ (Different     │
          │  ciphertext!)  │  │  ciphertext!)  │
          └────────────────┘  └────────────────┘


The document is encrypted ONCE with the DEK.
The DEK is wrapped separately for EACH recipient.
```

### The Multi-Recipient File

```
secret-proposal.pdf.enc (multi-recipient)
┌─────────────────────────────────────────────────────────────┐
│                         HEADER                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Ephemeral Public Key: kL3mN9pQ2rS5tU8vW...                │
│                                                             │
│  Recipients:                                                │
│  ┌────────────────────────────────────────────────────┐    │
│  │ [1] Address: pk:7Xq9KmPvR3nYhW2s...    ← Alice     │    │
│  │     Wrapped DEK: xY4zA7bC2dE5fG...                 │    │
│  │     Nonce: jK9lM0nO1pQ2rS3t                        │    │
│  ├────────────────────────────────────────────────────┤    │
│  │ [2] Address: pk:8Yr0LnQwS4oZiX3u...    ← Carol     │    │
│  │     Wrapped DEK: aB1cD2eF3gH4iJ...     (different!)│    │
│  │     Nonce: kL5mN6oP7qR8sT9u                        │    │
│  └────────────────────────────────────────────────────┘    │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    ENCRYPTED DATA                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   (Same encrypted document for all recipients)              │
│   The data is encrypted ONCE with the DEK                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Who Can Decrypt?

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   ALICE     │    │   CAROL     │    │    EVE      │
│             │    │             │    │             │
│   ✓ Can     │    │   ✓ Can     │    │   ✗ Cannot  │
│   decrypt   │    │   decrypt   │    │   decrypt   │
│             │    │             │    │             │
│ Has private │    │ Has private │    │ Not in      │
│ key for     │    │ key for     │    │ recipient   │
│ pk:7Xq9...  │    │ pk:8Yr0...  │    │ list        │
└─────────────┘    └─────────────┘    └─────────────┘
```

### Efficiency

Adding more recipients barely increases file size:

| Recipients | Header Size | Data Size | Total Overhead |
|------------|-------------|-----------|----------------|
| 1          | ~200 bytes  | 1.2 MB    | 0.02%          |
| 10         | ~1 KB       | 1.2 MB    | 0.08%          |
| 100        | ~10 KB      | 1.2 MB    | 0.8%           |

The document is encrypted once. Only the DEK wrapping is repeated.

---

## Summary

```
╔═══════════════════════════════════════════════════════════╗
║                                                           ║
║   PUBLIC KEY ENCRYPTION: KEY CONCEPTS                     ║
║                                                           ║
║   ┌─────────────────────────────────────────────────────┐ ║
║   │                                                     │ ║
║   │  • Share your PUBLIC KEY freely (like an address)  │ ║
║   │                                                     │ ║
║   │  • Keep your PRIVATE KEY secret (like a password)  │ ║
║   │                                                     │ ║
║   │  • Anyone can encrypt FOR you using your public    │ ║
║   │    key - they don't need your password             │ ║
║   │                                                     │ ║
║   │  • Only YOU can decrypt (with your private key)    │ ║
║   │                                                     │ ║
║   │  • No shared passwords to exchange!                │ ║
║   │                                                     │ ║
║   │  • Multiple recipients in a single file            │ ║
║   │                                                     │ ║
║   │  • Forward secrecy (ephemeral keys protect past    │ ║
║   │    messages even if your key is compromised)       │ ║
║   │                                                     │ ║
║   └─────────────────────────────────────────────────────┘ ║
║                                                           ║
╚═══════════════════════════════════════════════════════════╝
```

### Glossary

| Term | Description |
|------|-------------|
| **Public Key** | Your "address" - share it freely |
| **Private Key** | Your secret - never share, stored encrypted |
| **Keypair** | A matched public + private key |
| **Ephemeral Key** | One-time keypair, discarded after each encryption |
| **DEK** | Data Encryption Key - random key for encrypting data |
| **KEK** | Key Encryption Key - derived from ECDH, wraps the DEK |
| **ECDH** | Elliptic Curve Diffie-Hellman - the key exchange magic |
| **Hybrid Encryption** | Combining asymmetric (PKE) and symmetric (AES) encryption |
| **Forward Secrecy** | Past messages stay secure even if keys are later compromised |

### Cryptographic Building Blocks

| Purpose | Common Algorithms |
|---------|-------------------|
| Key Exchange | X25519 (Curve25519), P-256, P-384 |
| Key Derivation | HKDF-SHA256, HKDF-SHA384 |
| Symmetric Encryption | AES-256-GCM, ChaCha20-Poly1305 |
| Password-Based Key Storage | Argon2id, scrypt, PBKDF2 |

### The Security Properties

1. **Confidentiality**: Only recipients can read the data
2. **Integrity**: Any tampering is detected (GCM authentication)
3. **Authenticity**: The sender's ephemeral key is bound to the message
4. **Forward Secrecy**: Compromised long-term keys don't expose past messages

---

## Appendix: Why Not Just Use Passwords?

You might wonder: "Why not just share a password?"

| Password Sharing | Public Key Encryption |
|------------------|----------------------|
| Need to meet or use secure channel first | Just publish your public key |
| Same password for all senders | Each sender uses your public key |
| Compromise affects everyone | Compromise of one sender doesn't affect others |
| Can't prove who sent it | Ephemeral keys provide some sender binding |
| No forward secrecy | Fresh keys = forward secrecy |

PKE elegantly solves the **key distribution problem** that plagued cryptography for centuries.

---

*This primer explains the concepts behind public key encryption. For implementation-specific details, consult your encryption software's documentation.*
