# ADR-093: Key Provider Trait — KMS Required for Hosted Multi-Tenant at Launch

**Status:** Accepted (revised 2026-05-20 to align with HotM ADR-MOBILE-001 Decision 4)
**Date:** 2026-05-20
**Deciders:** roctinam
**Related:** ADR-006 (symmetric encryption), ADR-007 (envelope encryption), ADR-010 (in-memory encryption), ADR-088 (plugin strategy)
**Upstream:** HotM `.aiwg/architecture/adr-mobile-cloud-architecture.md` (ADR-MOBILE-001 Decision 4) — strategic source
**Related rules:** `.claude/rules/no-key-reuse-across-purposes.md`, `.claude/rules/no-adhoc-kdf.md`, `.claude/rules/sec-key-material-handling.md`, `.claude/rules/crypto-flag-verification.md`
**Related issues:** Fortemi/fortemi#707 sub-items 5 and 6 (BYO-LLM secret storage + proxy)

## Revision history

| Rev | Date | Change |
|---|---|---|
| 0 | 2026-05-20 | Initial draft framed KMS as an EE plugin upgrade; `EnvKeyProvider` as CE default |
| 1 | 2026-05-20 | **Revised** to align with HotM ADR-MOBILE-001 Decision 4: KMS required for hosted multi-tenant at launch. `EnvKeyProvider` retained only for the HotM desktop sidecar (single-tenant local install) and explicit dev-only opt-out. "KEK file on disk" launch posture is rejected for hosted. |

## July 2026 checkpoint rebaseline

Accepted status means the KMS-required hosted target is accepted; it does not mean the KeyProvider implementation is complete. The July 2026 checkpoint found the target documented here and in `docs/architecture/cryptographic-decisions.md`, but no `KeyProvider` trait/provider implementation in `crates/`. Hosted multi-tenant secret storage remains blocked on `Fortemi/fortemi#1019` and `Fortemi-Enterprise/kms#2`.

## Context

`matric-crypto` provides symmetric encryption (ADR-006), envelope encryption (ADR-007), and in-memory encryption (ADR-010). The current implementation reads its master key material from an environment variable or local file — acceptable for the HotM desktop sidecar (single-tenant, root-owned, local trust boundary).

HotM ADR-MOBILE-001 Decision 4 rejected the "KEK file on disk" launch posture for the hosted multi-tenant deployment:

> "A 'KEK file on disk' launch posture is rejected because it fails against host-root compromise, memory disclosure, and backup-tape exfil; the migration path to KMS later is too easy a path to defer."

The hosted deployment will store user BYO-LLM provider keys (encrypted under user DEKs, themselves wrapped under a per-tenant or per-application KEK). Loss of the KEK compromises every user's stored provider key simultaneously. KMS posture is therefore launch-blocking, not a future upgrade path.

The initial draft of this ADR framed KMS as an EE plugin upgrade with `EnvKeyProvider` as the CE default. This was incorrect for the hosted multi-tenant deployment, which is the relevant launch target.

## Decision

**Introduce a pluggable `KeyProvider` trait in `matric-crypto`. Ship two implementations: `EnvKeyProvider` (single-tenant only, asserts at startup) and `KmsKeyProvider` (required for hosted multi-tenant; backed by AWS KMS, GCP KMS, or HashiCorp Vault Transit at launch). YubiHSM2 and additional KMS backends ship as EE plugins.**

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

    /// Rotate the KEK for this purpose. KMS-backed implementations trigger KMS API
    /// rotation; existing wrapped DEKs remain valid until re-wrap. EnvKeyProvider
    /// returns Unsupported.
    async fn rotate(&self, purpose: KeyPurpose, tenant: Option<&TenantId>)
        -> Result<RotationInfo, KeyError>;

    async fn health_check(&self) -> Result<HealthStatus, KeyError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyPurpose {
    /// AEAD encryption of user content blobs.
    ContentBlob,
    /// AEAD encryption of stored credentials including BYO-LLM provider keys
    /// (HotM ADR-MOBILE-001 Decision 4, Fortemi/fortemi#707 sub-items 5/6).
    UserSecret,
    /// AEAD encryption of stored OAuth refresh tokens.
    OAuthRefreshToken,
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

- **Local impls (`EnvKeyProvider`)**: HKDF-Expand with `info = b"fortemi/<purpose_name>/v1"`. Tenant-scoping adds `/tenant/<id>` to the info string. Satisfies `no-key-reuse-across-purposes.md`.
- **KMS-backed impls**: pass `purpose` as an `EncryptionContext` parameter (AWS KMS), `additionalAuthenticatedData` (GCP KMS), or context (Vault Transit). The KMS refuses decrypt if context mismatches.

### Implementation matrix

| Provider | Crate | Tier | When |
|---|---|---|---|
| `EnvKeyProvider` | `matric-crypto` (core) | **Single-tenant only** | HotM desktop sidecar; explicit `FORTEMI_KEY_PROVIDER=env` opt-out for dev. Refuses to construct when `FORTEMI_MULTI_TENANT=true`. |
| `KmsKeyProvider` (AWS KMS) | `matric-crypto` (core, optional `kms-aws` feature) | **Launch — hosted** | Hosted multi-tenant. AWS KMS via `aws-sdk-kms`. Supports key contexts, multi-region keys. |
| `KmsKeyProvider` (GCP KMS) | `matric-crypto` (core, optional `kms-gcp` feature) | **Launch — hosted** | Hosted multi-tenant. GCP Cloud KMS. |
| `KmsKeyProvider` (Vault Transit) | `matric-crypto` (core, optional `kms-vault` feature) | **Launch — hosted** | Hosted multi-tenant. HashiCorp Vault Transit. Required for on-prem hosted deployments. |
| `YubiHsmKeyProvider` | `fortemi-enterprise-kms-yubihsm` | EE plugin (post-launch) | On-prem hardware HSM via PKCS#11. |
| `ByokWrapper` | `fortemi-enterprise-kms-byok` | EE plugin (post-launch) | Customer KEK in their KMS, Fortemi-managed DEKs. |

### Fail-closed default for hosted multi-tenant

`KmsKeyProvider` is the **only** acceptable provider in hosted multi-tenant mode. Startup asserts:

```rust
// crates/matric-api/src/startup_asserts.rs
if config.multi_tenant && !matches!(
    config.key_provider,
    KeyProviderKind::AwsKms | KeyProviderKind::GcpKms | KeyProviderKind::VaultTransit
) {
    return Err(Error::Config(
        "Hosted multi-tenant mode requires a KMS-backed KeyProvider \
         (aws-kms | gcp-kms | vault-transit). EnvKeyProvider is forbidden \
         per ADR-093 and HotM ADR-MOBILE-001 Decision 4.".into()
    ));
}
```

Single-tenant mode (HotM desktop sidecar) accepts `EnvKeyProvider`.

### EnvKeyProvider — single-tenant only

```rust
pub struct EnvKeyProvider {
    master_key: Zeroizing<[u8; 32]>,
    kek_id: KekIdentifier,
}
```

- Master key loaded once at startup from `FORTEMI_MASTER_KEY` env or `FORTEMI_MASTER_KEY_FILE` path (mode 0600, enforced)
- Key zeroized on drop (`zeroize::Zeroizing`)
- HKDF-Expand derives per-purpose DEKs and signing keys per `no-adhoc-kdf.md`
- `rotate()` returns `Err(KeyError::Unsupported)` — operators rotate by changing the master key + manual re-wrap (acceptable for desktop sidecar; not acceptable for multi-tenant)
- Construction fails if `FORTEMI_MULTI_TENANT=true` is set in the environment

### KmsKeyProvider — hosted multi-tenant launch posture

Three backends ship in the core crate behind Cargo features. The customer chooses one at deploy time:

```rust
pub struct AwsKmsKeyProvider {
    client: aws_sdk_kms::Client,
    key_id_map: HashMap<KeyPurpose, String>,  // KMS key ARN per purpose
    dek_cache: Mutex<LruCache<DekCacheKey, Zeroizing<Vec<u8>>>>,
}
```

- Per-purpose KMS key (separate ARN for `ContentBlob`, `UserSecret`, `OAuthRefreshToken`, `PluginJwt`, `AuditChain`, `ApiKeyHmac`)
- Per-tenant scoping via `EncryptionContext` (AWS KMS) / `additionalAuthenticatedData` (GCP KMS) / context (Vault)
- DEK cache (LRU, 5-min TTL) for hot-path performance — invalidated on `rotate()`
- `wrap_dek` / `unwrap_dek` / `sign` calls are KMS API operations; cost is 5-50ms typical, 100+ms cross-region
- `health_check()` calls KMS `DescribeKey`

### BYO-LLM provider key storage (the user-facing surface)

This is the primary motivator for KMS at launch. Per HotM ADR-MOBILE-001 Decision 4 and Fortemi/fortemi#707 sub-items 5 and 6:

```
User stores their Anthropic API key:
  → matric-api receives POST /v1/user/secrets
  → Generate fresh DEK via KeyProvider::generate_dek(KeyPurpose::UserSecret, Some(tenant_id), 32)
  → Encrypt the provider key with the DEK using AEAD (XChaCha20-Poly1305)
  → Store: ciphertext + WrappedKey { kek_id, ... } in user_secrets table

User makes inference request:
  → matric-api proxies POST /v1/inference/chat
  → Fetch the WrappedKey from user_secrets
  → KeyProvider::unwrap_dek(wrapped, KeyPurpose::UserSecret, Some(tenant_id))
  → Decrypt the provider key with the unwrapped DEK
  → Use the provider key to call Anthropic/OpenAI/etc.
  → Zeroize the unwrapped DEK and provider key after the request
```

DEK cache hit rate is the operational dial. With per-user DEKs and 5-min TTL, an active user pays one KMS round-trip per 5 minutes — negligible at scale.

### Wrapped key format

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKey {
    pub version: u8,                      // Format version (currently 1)
    pub kek_id: KekIdentifier,            // KMS key ARN, or SHA-256 of EnvKeyProvider master prefix
    pub purpose: KeyPurpose,
    pub ciphertext: Vec<u8>,
    pub aad: Vec<u8>,                     // Authenticated additional data
    pub provider_metadata: serde_json::Value,
}
```

`kek_id` is opaque and provider-specific.

### Required audit events (per ADR-091)

| Operation | Event | Severity |
|---|---|---|
| `wrap_dek` / `generate_dek` | `key.use` | Info |
| `unwrap_dek` | `key.use` | Info |
| `sign` / `verify` | `key.use` | Info |
| `rotate` | `key.rotate` | Notice |
| KMS authentication failure | `key.access_denied` | Critical |
| Any attempt to export raw master key | `key.export_attempt` | Critical |
| `EnvKeyProvider` construction in multi-tenant mode (refused) | `key.config_violation` | Critical |

### Key rotation

KMS-backed providers support rotation natively:

- **AWS KMS**: Automatic annual rotation OR manual rotation via `aws kms create-key-version`; existing wrapped DEKs continue to decrypt against the previous version until re-wrapped
- **GCP KMS**: `gcloud kms keys versions create` — same semantics
- **Vault Transit**: `vault write -f transit/keys/<name>/rotate` — same semantics

Re-wrap procedure: read each WrappedKey, unwrap_dek (uses old KEK version), generate_dek with new version, atomic UPDATE on the user_secrets row. This is a background job; can run online.

`EnvKeyProvider` rotation is a manual operator procedure documented in the runbook — read each WrappedKey, unwrap with old master, re-wrap with new master, update rows. Service downtime required.

## Consequences

### Positive

- (+) Aligns with HotM ADR-MOBILE-001 Decision 4 ("KMS at launch") and Fortemi/fortemi#707
- (+) BYO-LLM provider keys protected by enterprise-grade KMS from day one
- (+) Per-purpose KMS keys + per-tenant context = strong cryptographic separation
- (+) Compliance-friendly (SOC2 CC6.7, FIPS 140-2 via KMS posture)
- (+) Audit-logged key operations
- (+) Online rotation supported by all three launch backends
- (+) HotM desktop sidecar continues to work with the simpler EnvKeyProvider
- (+) Domain separation enforced at the trait surface (`purpose` mandatory)

### Negative

- (-) KMS-backed operations add latency (5–50 ms per `wrap`/`unwrap` typical; 100+ ms cross-region). Mitigated by per-user DEK cache (5-min TTL).
- (-) Three KMS backends to maintain in core (`kms-aws`, `kms-gcp`, `kms-vault`). Vault chosen specifically so on-prem hosted deployments are not blocked on cloud KMS access.
- (-) Operational dependency on KMS uptime — outages block decrypt operations. Mitigation: DEK cache keeps recently-active users servable for 5 minutes during a KMS blip.
- (-) Cost: AWS KMS at $1/key/month + $0.03 per 10k requests, GCP KMS comparable, Vault Transit free if self-hosted (but adds Vault ops burden)
- (-) Cross-region KMS replication is operator's responsibility; document in runbook
- (-) `EnvKeyProvider` is now a development/desktop-only tool, not a CE production option for hosted

### Neutral

- (~) The original "EE upgrade" framing is retained for YubiHSM2 and BYOK-wrapper backends; just not for the AWS/GCP/Vault baseline
- (~) Schema-per-tenant escalation (per ADR-090 Rev 1) does not change the KMS posture — per-tenant KEKs already work via the `tenant` parameter

## Implementation

**Code location:**
- Trait + types: `crates/matric-crypto/src/provider.rs` (new)
- EnvKeyProvider: `crates/matric-crypto/src/provider/env.rs`
- KmsKeyProvider (AWS): `crates/matric-crypto/src/provider/aws_kms.rs` (feature `kms-aws`)
- KmsKeyProvider (GCP): `crates/matric-crypto/src/provider/gcp_kms.rs` (feature `kms-gcp`)
- KmsKeyProvider (Vault): `crates/matric-crypto/src/provider/vault_transit.rs` (feature `kms-vault`)
- Startup assertion: `crates/matric-api/src/startup_asserts.rs`
- EE backends: separate `fortemi-enterprise-kms-yubihsm`, `fortemi-enterprise-kms-byok` crates

**Phases:**

1. Land trait + EnvKeyProvider (with multi-tenant refusal)
2. Refactor existing `matric-crypto` encrypt/decrypt to use `KeyProvider::generate_dek` + `unwrap_dek`
3. DEK cache (LRU, 5-min TTL, per-tenant per-purpose)
4. Wire `KeyProvider` into `AppState`
5. **`KmsKeyProvider` for AWS KMS** (first launch backend — Fortemi/fortemi#707 sub-item 5)
6. KAT (Known Answer Test) suite verifies HKDF derivation matches RFC 5869 vectors
7. Purpose-separation test: same input + different purpose → different output
8. Rotation test: existing wrapped DEKs decrypt after rotate
9. Audit emission test: all wrap/unwrap/sign emit `key.use`
10. Vault Transit backend
11. GCP KMS backend
12. Document operational runbook (KEK rotation, KMS region pairing, cost monitoring)
13. EE backends (YubiHSM2, BYOK) — post-launch

**Cross-doc updates needed (separate ticket):**

- `cryptographic-decisions.md` (referenced in HotM ADR-MOBILE-001 — to be authored before Phase 1)

## References

- HotM `.aiwg/architecture/adr-mobile-cloud-architecture.md` (ADR-MOBILE-001) Decision 4 — strategic source
- HotM `.aiwg/research/findings/mobile-multitenant-byo-llm.md` §3, §4 — BYO-LLM proxy + envelope encryption research
- Fortemi/fortemi#707 sub-items 5 (secret storage endpoints) and 6 (proxy implementation)
- ADR-006, ADR-007, ADR-010 — current crypto architecture
- ADR-091 — audit sink (consumes key operation events)
- `.claude/rules/no-key-reuse-across-purposes.md`
- `.claude/rules/no-adhoc-kdf.md`
- `.claude/rules/sec-key-material-handling.md`
- RFC 5869 — HKDF
- NIST SP 800-57 Part 1 — Key Management
- FIPS 140-2 — Security Requirements for Cryptographic Modules
- AWS KMS Developer Guide — Encryption Contexts
- HashiCorp Vault Transit Secrets Engine docs
