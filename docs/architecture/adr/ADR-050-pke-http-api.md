# ADR-050: HTTP API Endpoints for PKE Operations

**Status:** Proposed
**Date:** 2026-02-06
**Deciders:** Architecture team
**Related:** Gitea issue #70, ADR-007 (Envelope Encryption)

## Context

The MCP server currently exposes PKE (Public Key Encryption) tools that shell out to the `matric-pke` CLI binary via `execSync()`. This creates several operational issues:

1. **Binary availability**: The `matric-pke` binary must be in PATH or explicitly installed where the MCP server runs
2. **Docker deployment**: In containerized environments, the CLI binary may not be built or accessible
3. **Security**: Shelling out creates process overhead and potential command injection risks
4. **Error handling**: JSON parsing of stdout/stderr from CLI is fragile
5. **Maintenance burden**: Two interfaces (CLI + MCP) for the same operations

### Current MCP Tools (lines 1039-1092 in `mcp-server/index.js`)

The following MCP tools rely on `matric-pke` CLI:

- `pke_generate_keypair` - Generate X25519 keypair
- `pke_get_address` - Get public key address
- `pke_encrypt` - Encrypt file for recipients
- `pke_decrypt` - Decrypt file with private key
- `pke_list_recipients` - List recipients of encrypted file
- `pke_verify_address` - Verify address checksum

### Existing PKE Library (crates/matric-crypto/src/pke/)

The PKE functionality is already implemented as a Rust library in `crates/matric-crypto/src/pke/`:

- **Keypair operations**: `Keypair::generate()`, key storage (`save_private_key`, `load_public_key`)
- **Encryption**: `encrypt_pke(plaintext, recipients, filename)` - Multi-recipient ECDH + AES-256-GCM
- **Decryption**: `decrypt_pke(ciphertext, private_key)` - Returns plaintext + metadata
- **Inspection**: `get_pke_recipients(ciphertext)` - List recipient addresses without decrypting
- **Address utilities**: `PublicKey::to_address()`, `Address::verify_checksum()`

The CLI in `crates/matric-crypto/src/bin/pke.rs` is a thin wrapper around these library functions.

## Decision

Add HTTP API endpoints for PKE operations that directly call the Rust library functions, eliminating the need for the MCP server to shell out to the CLI binary.

### Proposed API Endpoints

All endpoints under `/api/v1/pke/*`:

#### 1. Generate Keypair
```
POST /api/v1/pke/keygen
Content-Type: application/json

{
  "passphrase": "strong-passphrase-123",
  "label": "Work Key"  // optional
}

Response 200:
{
  "address": "mm:abc123...",
  "public_key": "base64-encoded-public-key",
  "encrypted_private_key": "base64-encoded-encrypted-private-key"
}
```

**Notes:**
- Returns keys as base64-encoded strings rather than writing to disk
- Caller can choose to save keys to filesystem or store in database
- Private key is encrypted with Argon2id using the provided passphrase

#### 2. Get Address from Public Key
```
POST /api/v1/pke/address
Content-Type: application/json

{
  "public_key": "base64-encoded-public-key"
}

Response 200:
{
  "address": "mm:abc123...",
  "version": 1
}
```

#### 3. Encrypt Data
```
POST /api/v1/pke/encrypt
Content-Type: application/json

{
  "plaintext": "base64-encoded-plaintext",
  "recipients": ["mm:abc123...", "mm:xyz789..."],
  "original_filename": "document.pdf"  // optional
}

Response 200:
{
  "ciphertext": "base64-encoded-ciphertext",
  "input_size": 1024,
  "output_size": 1234,
  "recipients": ["mm:abc123...", "mm:xyz789..."]
}
```

**Notes:**
- Recipients specified by their public addresses (not public key files)
- Requires a separate endpoint to map addresses to public keys (see "Address Registry" below)
- Uses ephemeral ECDH + AES-256-GCM with MMPKE01 format

#### 4. Decrypt Data
```
POST /api/v1/pke/decrypt
Content-Type: application/json

{
  "ciphertext": "base64-encoded-ciphertext",
  "encrypted_private_key": "base64-encoded-encrypted-private-key",
  "passphrase": "strong-passphrase-123"
}

Response 200:
{
  "plaintext": "base64-encoded-plaintext",
  "input_size": 1234,
  "output_size": 1024,
  "original_filename": "document.pdf",
  "created_at": "2026-02-06T12:00:00Z"
}

Error 403:
{
  "error": "Not a recipient or invalid passphrase"
}
```

#### 5. List Recipients
```
POST /api/v1/pke/recipients
Content-Type: application/json

{
  "ciphertext": "base64-encoded-ciphertext"
}

Response 200:
{
  "recipients": ["mm:abc123...", "mm:xyz789..."],
  "count": 2
}
```

#### 6. Verify Address
```
GET /api/v1/pke/verify/{address}

Response 200:
{
  "address": "mm:abc123...",
  "valid": true,
  "version": 1
}

Response 400:
{
  "address": "mm:invalid",
  "valid": false,
  "error": "Invalid checksum"
}
```

### Address Registry (Future Enhancement)

To support encrypting by address (not just by public key file), we need an address-to-public-key mapping:

```
POST /api/v1/pke/keys
Content-Type: application/json

{
  "public_key": "base64-encoded-public-key",
  "label": "Alice's Work Key"  // optional
}

Response 201:
{
  "address": "mm:abc123...",
  "label": "Alice's Work Key"
}
```

```
GET /api/v1/pke/keys/{address}

Response 200:
{
  "address": "mm:abc123...",
  "public_key": "base64-encoded-public-key",
  "label": "Alice's Work Key",
  "created_at": "2026-02-06T12:00:00Z"
}
```

This would be stored in a new table:
```sql
CREATE TABLE pke_public_keys (
  address TEXT PRIMARY KEY,
  public_key BYTEA NOT NULL,
  label TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Implementation Plan

#### Phase 1: Core Endpoints (Minimal Viable API)

1. Add handler functions to `crates/matric-api/src/handlers/mod.rs` (or new `pke.rs` module)
2. Wire routes in `main.rs` router setup (around line 650)
3. Add request/response types to `crates/matric-core/src/models.rs`
4. Update OpenAPI spec in `crates/matric-api/src/openapi.yaml`
5. Add integration tests in `crates/matric-api/tests/pke_api_tests.rs`

**Estimated effort:** 1-2 days
**Dependencies:** None (uses existing PKE library)

#### Phase 2: MCP Server Migration

1. Update MCP tools in `mcp-server/index.js` to call HTTP endpoints instead of `execSync`
2. Replace file I/O with base64 encoding/decoding
3. Update tool descriptions to reflect HTTP-based implementation
4. Add error handling for API connectivity issues

**Estimated effort:** 1 day
**Dependencies:** Phase 1 complete

#### Phase 3: Address Registry (Optional)

1. Create migration for `pke_public_keys` table
2. Add repository methods in `crates/matric-db/src/pke_keys.rs`
3. Add endpoints for key registration and lookup
4. Update `pke_encrypt` endpoint to resolve addresses to public keys
5. Add MCP tools for key management

**Estimated effort:** 2 days
**Dependencies:** Phase 2 complete

## Consequences

### Positive

- (+) **No CLI dependency**: MCP server works without `matric-pke` binary in PATH
- (+) **Docker-friendly**: HTTP API runs in same container as API server
- (+) **Better error handling**: Structured JSON errors instead of parsing stderr
- (+) **Consistent interface**: Same authentication/authorization as other API endpoints
- (+) **Easier testing**: HTTP endpoints easier to test than CLI + file I/O
- (+) **Performance**: Avoids process spawning overhead
- (+) **Security**: No shell command injection risks

### Negative

- (-) **API surface growth**: Adds 6+ endpoints to already large API (700+ lines of routes)
- (-) **Data encoding overhead**: Base64 encoding increases payload size by ~33%
- (-) **Memory usage**: Full plaintext/ciphertext loaded into memory (not streaming)
- (-) **Breaking change**: MCP tools will need update (but backward compatible with CLI)

### Mitigations

- **Modular handlers**: Create separate `handlers/pke.rs` module to isolate PKE logic
- **Streaming support**: Future enhancement could add chunked upload/download for large files
- **CLI preservation**: Keep `matric-pke` CLI for standalone/scripting use cases
- **Graceful migration**: MCP server could try HTTP first, fall back to CLI if API unavailable

## Implementation

**Code Locations:**

1. **Handler module** (new): `crates/matric-api/src/handlers/pke.rs`
   ```rust
   pub async fn pke_keygen(
       Json(req): Json<PkeKeygenRequest>,
   ) -> Result<Json<PkeKeygenResponse>, StatusCode> { ... }

   pub async fn pke_encrypt(
       Json(req): Json<PkeEncryptRequest>,
   ) -> Result<Json<PkeEncryptResponse>, StatusCode> { ... }

   pub async fn pke_decrypt(
       Json(req): Json<PkeDecryptRequest>,
   ) -> Result<Json<PkeDecryptResponse>, StatusCode> { ... }
   ```

2. **Request/response types**: `crates/matric-core/src/models.rs`
   ```rust
   #[derive(Deserialize)]
   pub struct PkeKeygenRequest {
       pub passphrase: String,
       pub label: Option<String>,
   }

   #[derive(Serialize)]
   pub struct PkeKeygenResponse {
       pub address: String,
       pub public_key: String,  // base64
       pub encrypted_private_key: String,  // base64
   }
   ```

3. **Router registration**: `crates/matric-api/src/main.rs` (around line 700)
   ```rust
   // PKE endpoints
   .route("/api/v1/pke/keygen", post(pke::pke_keygen))
   .route("/api/v1/pke/address", post(pke::pke_address))
   .route("/api/v1/pke/encrypt", post(pke::pke_encrypt))
   .route("/api/v1/pke/decrypt", post(pke::pke_decrypt))
   .route("/api/v1/pke/recipients", post(pke::pke_recipients))
   .route("/api/v1/pke/verify/:address", get(pke::pke_verify))
   ```

4. **MCP tool updates**: `mcp-server/index.js` (lines 1039-1092)
   ```javascript
   case "pke_generate_keypair": {
     result = await apiRequest("POST", "/api/v1/pke/keygen", {
       passphrase: args.passphrase,
       label: args.label
     });
     // Save to disk if output_dir specified
     if (args.output_dir) {
       // Write public_key and encrypted_private_key to files
     }
     break;
   }
   ```

5. **OpenAPI spec**: `crates/matric-api/src/openapi.yaml`
   ```yaml
   /api/v1/pke/keygen:
     post:
       summary: Generate X25519 keypair
       tags: [pke]
       requestBody:
         content:
           application/json:
             schema:
               type: object
               required: [passphrase]
               properties:
                 passphrase:
                   type: string
                   minLength: 12
                 label:
                   type: string
       responses:
         '200':
           description: Keypair generated
           content:
             application/json:
               schema:
                 type: object
                 properties:
                   address:
                     type: string
                   public_key:
                     type: string
                     format: byte
                   encrypted_private_key:
                     type: string
                     format: byte
   ```

6. **Integration tests** (new): `crates/matric-api/tests/pke_api_tests.rs`
   ```rust
   #[tokio::test]
   async fn test_pke_keygen() {
       let client = reqwest::Client::new();
       let resp = client.post("http://localhost:3000/api/v1/pke/keygen")
           .json(&json!({
               "passphrase": "test-passphrase-123"
           }))
           .send()
           .await
           .unwrap();

       assert_eq!(resp.status(), 200);
       let body: PkeKeygenResponse = resp.json().await.unwrap();
       assert!(body.address.starts_with("mm:"));
   }
   ```

## Security Considerations

1. **Passphrase handling**: Passphrases transmitted over HTTPS only. Not logged or stored.
2. **Rate limiting**: Apply rate limiting to `/pke/keygen` to prevent key generation DoS
3. **Memory zeroing**: Sensitive data (private keys, plaintexts) should be zeroized after use
4. **Size limits**: Enforce max plaintext size (e.g., 10MB) to prevent memory exhaustion
5. **Authentication**: Consider requiring authentication for PKE endpoints (currently anonymous)

## Testing Strategy

### Unit Tests
- Request/response serialization
- Base64 encoding/decoding
- Error handling for invalid inputs

### Integration Tests
- Full encrypt/decrypt round-trip via HTTP
- Multi-recipient encryption
- Invalid address verification
- Passphrase mismatch error handling

### MCP Tests
- Update existing MCP PKE tests to use HTTP endpoints
- Verify backward compatibility with file-based workflows

## Rollout Plan

1. **Development**: Implement Phase 1 (core endpoints) in feature branch
2. **Testing**: Run integration tests + manual testing with Postman/curl
3. **Docker bundle**: Build and test in docker-compose.bundle.yml
4. **MCP migration**: Update MCP server (Phase 2) in same release
5. **Documentation**: Update CLAUDE.md with new HTTP API usage
6. **Release**: Deploy as part of v2026.2.0

## Alternative Approaches Considered

### Alternative 1: Embed CLI binary in MCP container
**Rejected**: Still requires building and distributing the binary, doesn't solve the fragility of shell parsing.

### Alternative 2: Rust WASM module in Node.js
**Rejected**: Complex build setup, overkill for this use case when HTTP API is already available.

### Alternative 3: Keep CLI-only, require users to install it
**Rejected**: Poor developer experience, defeats purpose of MCP integration.

## References

- Gitea issue #70: "PKE tools require matric-pke CLI binary"
- ADR-007: Envelope Encryption for E2E Multi-Recipient
- `crates/matric-crypto/src/pke/mod.rs` - PKE library documentation
- `crates/matric-crypto/src/bin/pke.rs` - CLI implementation
- `mcp-server/index.js` lines 1039-1092 - Current MCP PKE tools
