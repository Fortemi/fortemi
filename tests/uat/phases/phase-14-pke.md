# UAT Phase 14: PKE Encryption

**Duration**: ~8 minutes
**Tools Tested**: 13 tools
**Dependencies**: Phase 0 (preflight)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Overview

PKE (Public Key Encryption) enables secure note sharing using X25519 key exchange. This phase tests keypair generation, encryption, decryption, and keyset management.

---

## Important Notes

- PKE uses X25519 elliptic curve cryptography
- Addresses use `mm:` prefix (e.g., `mm:abc123...`)
- Encrypted files use MMPKE01 format
- Passphrases protect private keys at rest
- Keysets are stored in `~/.matric/keys/`

---

## Test Setup

For these tests, you'll need:
- A test directory for key files
- Test content to encrypt

```javascript
const TEST_PASSPHRASE = "uat-test-passphrase-2026"
const TEST_CONTENT = "# Secret Note\n\nThis is confidential information for UAT testing."
```

---

## Test Cases

### Keypair Generation

#### PKE-001: Generate Keypair

**MCP Tool**: `pke_generate_keypair`

```javascript
pke_generate_keypair({
  passphrase: TEST_PASSPHRASE,
  label: "uat-primary"
})
```

**Expected**:
```json
{
  "address": "mm:...",
  "public_key_path": "~/.matric/keys/uat-primary/public.key",
  "private_key_path": "~/.matric/keys/uat-primary/private.key"
}
```

**Pass Criteria**:
- Address starts with `mm:`
- Key files created

**Store**: `primary_address`, `primary_public_path`, `primary_private_path`

---

#### PKE-002: Generate Second Keypair

**MCP Tool**: `pke_generate_keypair`

```javascript
pke_generate_keypair({
  passphrase: "secondary-passphrase",
  label: "uat-secondary"
})
```

**Store**: `secondary_address`

---

#### PKE-003: Get Address from Public Key

**MCP Tool**: `pke_get_address`

```javascript
pke_get_address({
  public_key_path: primary_public_path
})
```

**Expected**: `{ address: primary_address }`

**Pass Criteria**: Address matches PKE-001

---

#### PKE-004: Verify Address

**MCP Tool**: `pke_verify_address`

```javascript
pke_verify_address({
  address: primary_address
})
```

**Expected**:
```json
{
  "valid": true,
  "version": 1
}
```

---

#### PKE-005: Verify Invalid Address

**MCP Tool**: `pke_verify_address`

```javascript
pke_verify_address({
  address: "mm:invalid123"
})
```

**Expected**: `{ valid: false }`

---

### Encryption & Decryption

#### PKE-006: Encrypt for Single Recipient

**MCP Tool**: `pke_encrypt`

```javascript
// First, create a file to encrypt (or use existing)
// For MCP testing, we'll encrypt note export

const note = create_note({
  content: TEST_CONTENT,
  tags: ["uat/pke-test"],
  revision_mode: "none"
})

// Export to get content
const exported = export_note({ id: note.id })

pke_encrypt({
  input_path: "/tmp/uat-pke-test.md",
  output_path: "/tmp/uat-pke-test.md.enc",
  recipients: [primary_address]
})
```

**Expected**:
```json
{
  "success": true,
  "encrypted_file_path": "/tmp/uat-pke-test.md.enc"
}
```

**Store**: `encrypted_file_path`

---

#### PKE-007: List Recipients

**MCP Tool**: `pke_list_recipients`

```javascript
pke_list_recipients({
  input_path: encrypted_file_path
})
```

**Expected**:
```json
{
  "recipients": ["mm:..."]
}
```

**Pass Criteria**: Contains primary_address

---

#### PKE-008: Decrypt File

**MCP Tool**: `pke_decrypt`

```javascript
pke_decrypt({
  input_path: encrypted_file_path,
  output_path: "/tmp/uat-pke-decrypted.md",
  private_key_path: primary_private_path,
  passphrase: TEST_PASSPHRASE
})
```

**Expected**:
```json
{
  "success": true,
  "decrypted_file_path": "/tmp/uat-pke-decrypted.md"
}
```

**Verify**: Decrypted content matches original

---

#### PKE-009: Encrypt for Multiple Recipients

**MCP Tool**: `pke_encrypt`

```javascript
pke_encrypt({
  input_path: "/tmp/uat-pke-test.md",
  output_path: "/tmp/uat-pke-multi.md.enc",
  recipients: [primary_address, secondary_address]
})
```

**Expected**: File encrypted for both recipients

---

#### PKE-010: Verify Multiple Recipients

**MCP Tool**: `pke_list_recipients`

```javascript
pke_list_recipients({
  input_path: "/tmp/uat-pke-multi.md.enc"
})
```

**Expected**: Contains both addresses

---

#### PKE-011: Decrypt with Wrong Key

**MCP Tool**: `pke_decrypt`

```javascript
// Try to decrypt with secondary key when not a recipient
pke_decrypt({
  input_path: encrypted_file_path,  // Only encrypted for primary
  output_path: "/tmp/should-fail.md",
  private_key_path: "~/.matric/keys/uat-secondary/private.key",
  passphrase: "secondary-passphrase"
})
```

**Expected**: Error - not a recipient

**Pass Criteria**: Graceful error handling

---

### Keyset Management

#### PKE-012: List Keysets

**MCP Tool**: `pke_list_keysets`

```javascript
pke_list_keysets()
```

**Expected**:
```json
{
  "keysets": [
    {
      "name": "uat-primary",
      "address": "mm:...",
      "public_key_path": "...",
      "private_key_path": "...",
      "created": "<timestamp>"
    },
    {
      "name": "uat-secondary",
      "address": "mm:...",
      ...
    }
  ]
}
```

**Pass Criteria**: Both test keysets listed

---

#### PKE-013: Create Named Keyset

**MCP Tool**: `pke_create_keyset`

```javascript
pke_create_keyset({
  name: "uat-named-keyset",
  passphrase: "named-keyset-passphrase"
})
```

**Expected**: Keyset created with name

**Store**: `named_keyset_name`

---

#### PKE-014: Get Active Keyset (None)

**MCP Tool**: `pke_get_active_keyset`

```javascript
pke_get_active_keyset()
```

**Expected**: `null` or current active keyset

---

#### PKE-015: Set Active Keyset

**MCP Tool**: `pke_set_active_keyset`

```javascript
pke_set_active_keyset({
  name: "uat-primary"
})
```

**Expected**: Success

---

#### PKE-016: Verify Active Keyset

**MCP Tool**: `pke_get_active_keyset`

```javascript
pke_get_active_keyset()
```

**Expected**: Returns uat-primary keyset

---

#### PKE-017: Export Keyset

**MCP Tool**: `pke_export_keyset`

```javascript
pke_export_keyset({
  name: "uat-primary",
  output_dir: "/tmp/uat-keyset-export"
})
```

**Expected**:
```json
{
  "success": true,
  "keyset_name": "uat-primary",
  "export_path": "/tmp/uat-keyset-export",
  "files": {
    "public_key": "public.key",
    "private_key": "private.key",
    "metadata": "keyset.json"
  }
}
```

---

#### PKE-018: Import Keyset

**MCP Tool**: `pke_import_keyset`

```javascript
pke_import_keyset({
  name: "uat-imported",
  import_path: "/tmp/uat-keyset-export"
})
```

**Expected**: Keyset imported with new name

**Verify**: `pke_list_keysets` includes uat-imported

---

#### PKE-019: Delete Keyset

**MCP Tool**: `pke_delete_keyset`

```javascript
pke_delete_keyset({
  name: "uat-named-keyset"
})
```

**Expected**: Keyset deleted

**Verify**: `pke_list_keysets` no longer includes it

---

#### PKE-020: Delete Active Keyset

**MCP Tool**: `pke_delete_keyset`

```javascript
pke_delete_keyset({
  name: "uat-primary"  // Currently active
})
```

**Expected**:
- Keyset deleted
- Active keyset cleared

**Verify**: `pke_get_active_keyset` returns null

---

## Cleanup

```javascript
// Delete remaining test keysets
pke_delete_keyset({ name: "uat-secondary" })
pke_delete_keyset({ name: "uat-imported" })

// Clean up temp files
// rm /tmp/uat-pke-*.md*
// rm -rf /tmp/uat-keyset-export

// Delete test note
delete_note({ id: pke_test_note_id })
```

---

## Success Criteria

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| PKE-001 | Generate keypair | `pke_generate_keypair` | |
| PKE-002 | Generate second keypair | `pke_generate_keypair` | |
| PKE-003 | Get address | `pke_get_address` | |
| PKE-004 | Verify valid address | `pke_verify_address` | |
| PKE-005 | Verify invalid address | `pke_verify_address` | |
| PKE-006 | Encrypt single recipient | `pke_encrypt` | |
| PKE-007 | List recipients | `pke_list_recipients` | |
| PKE-008 | Decrypt file | `pke_decrypt` | |
| PKE-009 | Encrypt multi-recipient | `pke_encrypt` | |
| PKE-010 | Verify multi-recipients | `pke_list_recipients` | |
| PKE-011 | Wrong key error | `pke_decrypt` | |
| PKE-012 | List keysets | `pke_list_keysets` | |
| PKE-013 | Create named keyset | `pke_create_keyset` | |
| PKE-014 | Get active (none) | `pke_get_active_keyset` | |
| PKE-015 | Set active keyset | `pke_set_active_keyset` | |
| PKE-016 | Verify active | `pke_get_active_keyset` | |
| PKE-017 | Export keyset | `pke_export_keyset` | |
| PKE-018 | Import keyset | `pke_import_keyset` | |
| PKE-019 | Delete keyset | `pke_delete_keyset` | |
| PKE-020 | Delete active keyset | `pke_delete_keyset` | |

**Pass Rate Required**: 100% (20/20)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `pke_generate_keypair` | PKE-001, PKE-002 |
| `pke_get_address` | PKE-003 |
| `pke_verify_address` | PKE-004, PKE-005 |
| `pke_encrypt` | PKE-006, PKE-009 |
| `pke_decrypt` | PKE-008, PKE-011 |
| `pke_list_recipients` | PKE-007, PKE-010 |
| `pke_list_keysets` | PKE-012 |
| `pke_create_keyset` | PKE-013 |
| `pke_get_active_keyset` | PKE-014, PKE-016 |
| `pke_set_active_keyset` | PKE-015 |
| `pke_export_keyset` | PKE-017 |
| `pke_import_keyset` | PKE-018 |
| `pke_delete_keyset` | PKE-019, PKE-020 |

**Coverage**: 13/13 PKE tools (100%)
