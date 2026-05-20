# ADR-093: Key Provider Trait (BYOK / HSM / KMS)

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, security review TBD
**Related:** ADR-006 (symmetric encryption), ADR-007 (envelope encryption), ADR-010 (in-memory encryption), ADR-088 (plugin strategy)
**Related rules:** `.claude/rules/no-key-reuse-across-purposes.md`, `.claude/rules/no-adhoc-kdf.md`, `.claude/rules/sec-key-material-handling.md`, `.claude/rules/crypto-flag-verification.md`

## Context

`matric-crypto` provides symmetric encryption (ADR-006), envelope encryption (ADR-007), and in-memory encryption (ADR-010). The current implementation reads its master key material from an environment variable (or local file). This is appropriate for single-tenant CE deployments.

For multi-tenant hosted EE, enterprise customers will require:
- **BYOK** (Bring Your Own Key) — customer controls the key encryption key (KEK)
- **HSM/KMS integration** — keys never leave the customer's trusted hardware boundary (FIPS 140-2 Level 2 or higher)
- **Per-tenant data encryption keys** — wrapped under the customer's KEK
- **Key rotation** — without re-encrypting all data
- **Audit log of key use** — every encrypt/decrypt/sign attributable

The fortemi codebase already has rigorous applied-cryptography guidance in the local rules (`no-key-reuse-across-purposes`, `no-adhoc-kdf`, `sec-key-material-handling`). This ADR aligns the `matric-crypto` plug-point with those rules.

## Decision

**Introduce a pluggable `KeyProvider` trait in `matric-crypto` (re-exported from `matric-core`) with an `EnvKeyProvider` default for CE and EE-provided implementations for AWS KMS, GCP KMS, HashiCorp Vault Transit, and YubiHSM2.**

### Trait

```rust
// crates/matric-crypto/src/provider.rs

use async_trait::async_trait;

#[async_trait]
pub trait KeyProvider: Send + Sync {
    /// Encrypt a data key (DEK) under the master key (KEK) for the given purpose.
    /// MUST use AEAD; MUST domain-separate purpose via HKDF info or KMS context.
    async fn wrap_dek(
        &self,
        plaintext_dek: &[u8],
        purpose: KeyPurpose,
        tenant: Option<&TenantId>,
    ) -> Result<WrappedKey, KeyError>;

    /// Decrypt a wrapped DEK.
    async fn unwrap_dek(
        &self,
        wrapped: &WrappedKey,
        purpose: KeyPurpose,
        tenant: Option<&TenantId>,
    ) -> Result<Vec<u8>, KeyError>;

    /// Generate a new random DEK and return it wrapped + plaintext (for immediate use).
    async fn generate_dek(
        &self,
        purpose: KeyPurpose,
        tenant: Option<&TenantId>,
        bytes: usize,
    ) -> Result<GeneratedDek, KeyError>;

    /// Sign arbitrary data with a managed signing key.
    /// Used for plugin JWT issuance and audit hash-chains.
    async fn sign(
        &self,
        purpose: KeyPurpose,
        tenant: Option<&TenantId>,
        data: &[u8],
    ) -> Result<Signature, KeyError>;

    async fn verify(
        &self,
        purpose: KeyPurpose,
        tenant: Option<&TenantId>,
        data: &[u8],
        signature: &Signature,
    ) -> Result<bool, KeyError>;

    /// Rotate the KEK for this provider. Implementation-specific:
    /// - EnvKeyProvider: not supported (returns error)
    /// - KMS-backed: triggers KMS API rotation; existing wrapped DEKs remain valid until re-wrap
    async fn rotate(&self, purpose: KeyPurpose, tenant: Option<&TenantId>)
        -> Result<RotationInfo, KeyError>;

    async fn health_check(&self) -> Result<HealthStatus, KeyError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyPurpose {
    /// AEAD encryption of user content blobs.
    ContentBlob,
    /// AEAD encryption of stored credentials (e.g., OAuth refresh tokens).
    StoredCredential,
    /// Signing of plugin JWTs (core ↔ Class B/C plugin).
    PluginJwt,
    /// Signing of audit hash-chain entries (when enabled).
    AuditChain,
    /// HMAC for API key validation.
    ApiKeyHmac,
    /// Custom purpose (informational; MUST use a distinct string).
    Custom(&'static str),
}
```

### Domain separation discipline

The `purpose` parameter is mandatory and MUST be propagated to the underlying KDF / KMS via:

- **Local impls (`EnvKeyProvider`)**: HKDF-Expand with `info = b"fortemi/<purpose_name>/v1"`. Tenant-scoping adds `/tenant/<id>` to the info string. This satisfies `no-key-reuse-across-purposes.md`.
- **KMS-backed impls**: pass `purpose` as an `EncryptionContext` parameter (AWS KMS), `additionalAuthenticatedData` (GCP KMS), or context (Vault Transit). The KMS will refuse decrypt if the context mismatches.

### Wrapped key format

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub version: u8,                      // Format version (currently 1)
    pub kek_id: KekIdentifier,            // Identifies which KEK wrapped this
    pub purpose: KeyPurpose,
    pub ciphertext: Vec<u8>,
    pub aad: Vec<u8>,                     // Authenticated additional data
    pub provider_metadata: serde_json::Value,
}
```

`kek_id` is opaque and provider-specific. For KMS it's the key ARN; for `EnvKeyProvider` it's the SHA-256 of the master key prefix.

### Default impl (CE): `EnvKeyProvider`

```rust
pub struct EnvKeyProvider {
    master_key: Zeroizing<[u8; 32]>,  // Loaded from env var or file
    kek_id: KekIdentifier,
}
```

- Master key loaded once at startup from `FORTEMI_MASTER_KEY` env or `FORTEMI_MASTER_KEY_FILE` path
- File mode MUST be `0600` (enforced at startup; fail if not)
- Key is zeroized on drop (`zeroize::Zeroizing`)
- HKDF-Expand derives per-purpose DEKs and signing keys
- `rotate()` returns `Err(KeyError::Unsupported)` — operators rotate by changing the master key + manual re-wrap

### EE impls (planned)

- `fortemi-enterprise-kms-aws` — AWS KMS via the `aws-sdk-kms` crate; supports key contexts and multi-region keys
- `fortemi-enterprise-kms-gcp` — GCP Cloud KMS
- `fortemi-enterprise-kms-vault` — HashiCorp Vault Transit engine
- `fortemi-enterprise-kms-yubihsm` — YubiHSM2 via PKCS#11 (for on-prem hardware)
- `fortemi-enterprise-kms-byok-wrapper` — Customer KEK in their KMS, Fortemi-managed DEKs

### What this trait does NOT do

- **Does not store DEKs.** DEKs are encrypted in-app and stored in the database next to the data they protect. The provider only deals with the KEK.
- **Does not handle TLS material.** TLS certs and private keys are managed by ops outside this surface.
- **Does not handle OAuth client secrets.** Those are stored in the database, encrypted with a DEK derived via `StoredCredential` purpose.

### Required audit events (per ADR-091)

| Operation | Event |
|---|---|
| `wrap_dek` / `generate_dek` | `key.use` (severity: Info) |
| `unwrap_dek` | `key.use` (severity: Info) |
| `sign` / `verify` | `key.use` (severity: Info) |
| `rotate` | `key.rotate` (severity: Notice) |
| Any operation that fails authentication to KMS | `key.access_denied` (severity: Critical) |
| Any attempt to export raw master key | `key.export_attempt` (severity: Critical) — note: trait does not expose export; this catches misuse via direct provider APIs |

## Consequences

### Positive
- (+) Enterprise BYOK and HSM integration available as plugins
- (+) Domain separation enforced at the trait surface (purpose mandatory)
- (+) CE continues to work with simple env-var key
- (+) Compliance-friendly (SOC 2, FIPS 140-2 via HSM impls)
- (+) Audit-logged key operations
- (+) Key rotation is a first-class operation (where supported)

### Negative
- (-) KMS-backed operations add latency (5–50 ms per `wrap`/`unwrap` typical, 100+ ms for cross-region)
- (-) Mitigated by DEK caching (5-minute LRU) with explicit invalidation on rotate
- (-) BYOK adds operational complexity for the customer (they own the KEK lifecycle)
- (-) Performance regression on first-use after restart (cold cache hits KMS)

### Neutral
- (~) Cross-region KMS replication is operator's responsibility
- (~) Per-tenant KEKs (the strongest BYOK form) require multi-tenant features to be configured for tenant scoping in `purpose` info

## Implementation

**Code location:**
- Trait + types: `crates/matric-crypto/src/provider.rs` (new)
- Default impl: `crates/matric-crypto/src/provider/env.rs`
- Re-export: `matric_core::crypto::{KeyProvider, KeyPurpose, ...}`
- EE plugins: separate `fortemi-enterprise-kms-*` crates

**Key changes:**
1. Define `KeyProvider`, `KeyPurpose`, `WrappedKey`, `GeneratedDek`, `Signature` types
2. Implement `EnvKeyProvider` honoring `.claude/rules/no-adhoc-kdf.md` (HKDF-Expand, no concat-and-hash)
3. Refactor existing `matric-crypto` encrypt/decrypt to use `KeyProvider::generate_dek` + `unwrap_dek`
4. DEK cache (LRU, default 5-min TTL, per-tenant)
5. Wire `KeyProvider` into `AppState`
6. First EE plugin: `fortemi-enterprise-kms-vault`

**Testing:**
- KAT (Known Answer Test) suite verifies HKDF derivation matches RFC 5869 vectors
- Purpose-separation test: same input + different purpose → different output
- Rotation test: existing wrapped DEKs decrypt after rotate
- Audit emission test: all wrap/unwrap/sign emit `key.use`

## References

- ADR-006, ADR-007, ADR-010 — Current crypto architecture
- ADR-091 — Audit sink (consumes key operation events)
- `.claude/rules/no-key-reuse-across-purposes.md`
- `.claude/rules/no-adhoc-kdf.md`
- `.claude/rules/sec-key-material-handling.md`
- RFC 5869 — HKDF
- NIST SP 800-57 Part 1 — Key Management
- FIPS 140-2 — Security Requirements for Cryptographic Modules
- AWS KMS Developer Guide — Encryption Contexts
- HashiCorp Vault Transit Secrets Engine docs
