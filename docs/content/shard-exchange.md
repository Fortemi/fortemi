# Shard Exchange Primer

A practical guide to sharing, backing up, and recovering knowledge using encrypted shards.

## Overview

Knowledge shards are portable snapshots of your memory that can be securely shared, backed up, and restored. This guide covers common workflows using Alice, Bob, and Carol as our cast of characters.

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Shard Exchange Flow                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   Alice                          Bob                     Carol      │
│     │                             │                        │        │
│     │  1. Create shard            │                        │        │
│     ├──────────────────┐          │                        │        │
│     │                  │          │                        │        │
│     │  2. Encrypt for  │          │                        │        │
│     │     recipients   │          │                        │        │
│     │         │        │          │                        │        │
│     │         ▼        │          │                        │        │
│     │   ┌──────────┐   │          │                        │        │
│     │   │ .shard   │───┼──────────┼────────────────────────┤        │
│     │   │ .pke     │   │          │                        │        │
│     │   └──────────┘   │          │                        │        │
│     │                  │          │                        │        │
│     │                  │   3. Bob decrypts                 │        │
│     │                  │      with his key                 │        │
│     │                  │          │                        │        │
│     │                  │          ▼                 4. Carol│        │
│     │                  │   ┌──────────┐               decrypts      │
│     │                  │   │ Restored │                    │        │
│     │                  │   │ memory   │             ┌──────────┐    │
│     │                  │   └──────────┘             │ Restored │    │
│     │                  │                            │ memory   │    │
│     │                  │                            └──────────┘    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## The Cast

| Character | Role | Has |
|-----------|------|-----|
| **Alice** | Knowledge curator | Full memory instance, PKE keypair |
| **Bob** | Collaborator | His own instance, PKE keypair |
| **Carol** | New team member | Fresh instance, PKE keypair |
| **Eve** | Eavesdropper | Network access only |

## Getting Started: Key Setup

Before exchanging shards, each participant generates a keypair. The public address (like a wallet address) is shared openly; the private key stays secret.

### Alice generates her keypair

```bash
# Generate keypair
pke keygen --output ~/.matric/alice

# View her shareable address
cat ~/.matric/alice.pub
# Output: mm:1A3xK9mPqR7vNwZ2...

# Her private key is encrypted with a passphrase
# and stored at ~/.matric/alice.key.enc
```

### Bob and Carol do the same

```bash
# Bob
pke keygen --output ~/.matric/bob
cat ~/.matric/bob.pub
# mm:1B7yT2nQsL4wMxY5...

# Carol
pke keygen --output ~/.matric/carol
cat ~/.matric/carol.pub
# mm:1C9zU4oPtK6xNvW8...
```

### Exchange addresses

Alice, Bob, and Carol share their `mm:...` addresses through any channel (email, chat, in-person). These are public and safe to share.

```
Alice's address: mm:1A3xK9mPqR7vNwZ2...
Bob's address:   mm:1B7yT2nQsL4wMxY5...
Carol's address: mm:1C9zU4oPtK6xNvW8...
```

---

## Scenario 1: Alice Shares a Knowledge Shard

Alice wants to share her research notes with Bob and Carol.

### Step 1: Create the shard

```bash
# Via API
curl http://localhost:3000/api/v1/backup/knowledge-shard \
  -o research.shard

# Via MCP
knowledge_shard({ include: "notes,links,tags" })
```

### Step 2: Encrypt for recipients

Alice only needs Bob's and Carol's public addresses:

```bash
# Encrypt for Bob and Carol
pke encrypt research.shard \
  --recipient mm:1B7yT2nQsL4wMxY5... \
  --recipient mm:1C9zU4oPtK6xNvW8... \
  --output research.shard.pke
```

### Step 3: Share the encrypted shard

Alice sends `research.shard.pke` through any channel - email, file share, USB drive. Even if Eve intercepts it, she cannot decrypt without Bob's or Carol's private key.

### Step 4: Bob decrypts

```bash
# Bob decrypts with his private key
pke decrypt research.shard.pke \
  --key ~/.matric/bob.key.enc \
  --output research.shard

# Enter passphrase when prompted
# Now Bob has the decrypted shard
```

### Step 5: Bob imports the shard

```bash
# Preview what will be imported
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d "{\"shard_base64\": \"$(base64 -w0 research.shard)\", \"dry_run\": true}"

# Import for real
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d "{\"shard_base64\": \"$(base64 -w0 research.shard)\"}"
```

Carol follows the same process with her private key.

---

## Scenario 2: Personal Backup

Alice wants to back up her entire memory for disaster recovery.

### Create encrypted backup

```bash
# Create comprehensive shard with embeddings
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,links,tags,embeddings" \
  -o full-backup.shard

# Encrypt to herself (she's the only recipient)
pke encrypt full-backup.shard \
  --recipient mm:1A3xK9mPqR7vNwZ2... \
  --output full-backup.shard.pke

# Store in multiple locations
cp full-backup.shard.pke /mnt/backup-drive/
aws s3 cp full-backup.shard.pke s3://alice-backups/matric/
```

### Recovery

If Alice loses her data:

```bash
# Retrieve backup
aws s3 cp s3://alice-backups/matric/full-backup.shard.pke .

# Decrypt with her private key
pke decrypt full-backup.shard.pke \
  --key ~/.matric/alice.key.enc \
  --output full-backup.shard

# Restore to fresh instance
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d "{\"shard_base64\": \"$(base64 -w0 full-backup.shard)\"}"
```

---

## Scenario 3: Team Knowledge Base

A team maintains a shared knowledge base. Any member can create and share updates.

### Initial setup

Each team member generates a keypair and shares their address in a team directory:

```yaml
# team-keys.yaml (shared in team wiki/docs)
team:
  alice: mm:1A3xK9mPqR7vNwZ2...
  bob: mm:1B7yT2nQsL4wMxY5...
  carol: mm:1C9zU4oPtK6xNvW8...
```

### Weekly knowledge sync

Bob curates this week's learnings and shares with the team:

```bash
# Create shard with recent notes
curl "http://localhost:3000/api/v1/backup/knowledge-shard" \
  -o weekly-update.shard

# Encrypt for all team members
pke encrypt weekly-update.shard \
  --recipient mm:1A3xK9mPqR7vNwZ2... \
  --recipient mm:1B7yT2nQsL4wMxY5... \
  --recipient mm:1C9zU4oPtK6xNvW8... \
  --output weekly-update.shard.pke

# Share via team file server
cp weekly-update.shard.pke /shared/team-updates/week-03.shard.pke
```

### Carol imports the update

```bash
# Decrypt and import
pke decrypt /shared/team-updates/week-03.shard.pke \
  --key ~/.matric/carol.key.enc \
  --output weekly-update.shard

# Merge with existing knowledge (skip duplicates)
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d "{
    \"shard_base64\": \"$(base64 -w0 weekly-update.shard)\",
    \"on_conflict\": \"skip\"
  }"
```

---

## Scenario 4: Key Recovery

What if Alice loses her private key?

### Preventive: Key escrow

Alice encrypts her private key to a trusted backup:

```bash
# Export private key (still encrypted with passphrase)
cp ~/.matric/alice.key.enc ~/secure-backup/

# Or encrypt to a hardware security key / recovery keypair
pke encrypt ~/.matric/alice.key.enc \
  --recipient mm:1RecoveryKeyHere... \
  --output ~/.matric/alice-recovery.pke
```

### Recovery procedure

1. Retrieve the escrowed key
2. Decrypt with recovery key
3. Restore to new machine

```bash
# Decrypt recovery bundle
pke decrypt alice-recovery.pke \
  --key /secure/recovery.key.enc \
  --output ~/.matric/alice.key.enc

# Now Alice can decrypt her backups again
pke decrypt full-backup.shard.pke \
  --key ~/.matric/alice.key.enc \
  --output full-backup.shard
```

### If no escrow exists

Alice's encrypted shards are **permanently inaccessible** without her private key. This is a feature, not a bug - it ensures true end-to-end encryption.

**Mitigation**: Always keep at least one unencrypted backup in a physically secure location, or use key escrow.

---

## Scenario 5: Revoking Access

Carol leaves the team. Future shards should not be decryptable by her.

### Going forward

Simply stop including Carol's address when encrypting:

```bash
# New shards only for active members
pke encrypt new-shard.shard \
  --recipient mm:1A3xK9mPqR7vNwZ2... \
  --recipient mm:1B7yT2nQsL4wMxY5... \
  --output new-shard.shard.pke
```

### Past shards

Carol can still decrypt any shards that were encrypted to her address. If this is a concern:

1. Create new shards without sensitive content
2. Re-encrypt existing shards to only current members
3. Delete old encrypted shards from shared storage

---

## Shard Contents Reference

| Component | Included By Default | Size Impact | Use Case |
|-----------|---------------------|-------------|----------|
| `notes` | Yes | Medium | Core content |
| `collections` | Yes | Small | Folder structure |
| `tags` | Yes | Small | Organization |
| `templates` | Yes | Small | Note templates |
| `links` | Yes | Medium | Semantic relationships |
| `embedding_sets` | Yes | Small | Set definitions |
| `embeddings` | **No** | **Large** | Vector search (regeneratable) |

### Minimal shard (fast, small)

```bash
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,tags" \
  -o minimal.shard
```

### Full shard (complete, large)

```bash
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,collections,tags,templates,links,embedding_sets,embeddings" \
  -o full.shard
```

---

## Security Properties

### What PKE protects

| Threat | Protected? | Notes |
|--------|------------|-------|
| Eve intercepts shard in transit | **Yes** | Cannot decrypt without private key |
| Cloud storage provider reads backup | **Yes** | Encrypted at rest |
| Stolen laptop (encrypted disk) | **Yes** | Double protection |
| Compromised recipient | **No** | They can decrypt their copy |
| Weak passphrase on private key | **Partial** | Argon2id slows brute force |

### What's visible without decryption

- File exists and approximate size
- Number of recipients (addresses visible in header)
- Recipient addresses (`mm:...`)
- Encryption timestamp

### Forward secrecy

Each encryption uses a fresh ephemeral key. Compromising Alice's long-term private key does **not** allow decrypting shards she *sent* to others - only shards sent *to* her.

---

## Command Reference

### pke CLI

```bash
# Generate keypair
pke keygen --output ~/.matric/mykey

# Encrypt for recipients
pke encrypt input.shard \
  --recipient mm:1abc... \
  --recipient mm:1xyz... \
  --output encrypted.shard.pke

# Decrypt with private key
pke decrypt encrypted.shard.pke \
  --key ~/.matric/mykey.key.enc \
  --output decrypted.shard

# Show recipients (no key needed)
pke info encrypted.shard.pke
# Recipients:
#   mm:1abc...
#   mm:1xyz...

# Verify you can decrypt (without actually decrypting)
pke verify encrypted.shard.pke --key ~/.matric/mykey.key.enc
```

### MCP Tools

```javascript
// Create shard
knowledge_shard({ include: "notes,links" })

// Import shard
knowledge_shard_import({
  shard_base64: "...",
  dry_run: true,
  on_conflict: "skip"
})

// Encrypt (via API)
encrypt_pke({
  data_base64: "...",
  recipients: ["mm:1abc...", "mm:1xyz..."]
})

// Decrypt
decrypt_pke({
  file_base64: "...",
  private_key_path: "~/.matric/mykey.key.enc",
  passphrase: "..."
})
```

---

## Best Practices

### Key management

1. **Generate keys on trusted hardware** - Not shared/public machines
2. **Use strong passphrases** - 20+ characters for private key encryption
3. **Back up private keys** - Encrypted, in multiple secure locations
4. **Rotate keys periodically** - Generate new keypairs yearly
5. **Revoke compromised keys** - Remove from team directories immediately

### Shard hygiene

1. **Verify before sharing** - Preview shard contents with `dry_run`
2. **Minimize recipients** - Only include those who need access
3. **Label clearly** - Use descriptive filenames with dates
4. **Clean up old shards** - Delete encrypted shards after confirmed receipt
5. **Test restores** - Periodically verify you can decrypt and import

### Transit security

1. **Any channel works** - PKE makes the channel security irrelevant
2. **Verify addresses** - Confirm recipient addresses out-of-band
3. **Checksum large files** - Use SHA256 to verify integrity after transfer

---

## Troubleshooting

### "No matching recipient"

Your private key's address doesn't match any recipient in the shard.

```bash
# Check your address
cat ~/.matric/mykey.pub

# Check shard recipients
pke info encrypted.shard.pke
```

### "Decryption failed"

- Wrong passphrase for private key
- Corrupted shard file
- Wrong private key file

### "Import conflicts"

Notes with the same ID already exist.

```bash
# Use conflict resolution
knowledge_shard_import({
  shard_base64: "...",
  on_conflict: "replace"  # or "skip" or "merge"
})
```

### Large shard upload fails

Base64 encoding increases size ~33%. For very large shards:

```bash
# Use file upload endpoint instead
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -F "shard=@large-backup.shard"
```

---

## Related Documentation

- [Encryption Guide](./encryption.md) - Cryptographic details and formats
- [Backup Guide](./backup.md) - Database backups and restore procedures
- [Operations](./operations.md) - Deployment and maintenance
