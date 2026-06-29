# Cryptographic Decisions — KeyProvider / KMS Launch Contract

> **Status:** Accepted (2026-06-29) — ADR-093 follow-up; locks Fortemi/fortemi#897.
> **Consumes:** ADR-093 (`KeyProvider` trait). **Implemented by:** #734. **Consumed by:** #730 (secret storage), #731 (BYO-LLM proxy). **Audit taxonomy:** #711/#910.
> **Scope:** the implementation contract #734/#730/#731 build against — provider model, configurable key strategy, versioned AAD/context schema, provider-neutral `EncryptedBlob`/`WrappedKey`, DEK/secret lifetime, startup reachability, fail-closed matrix, and rotation/rewrap.

## 0. Decision summary (operator, 2026-06-29)

| Decision | Choice |
|---|---|
| Provider model | **Provider-neutral + configurable.** Multiple backends behind one trait; deployment selects via config. The original "#897 AWS-KMS-first" framing is **superseded** — see §1. |
| First-class launch backends | **Vault-Transit / OpenBao** (the Integro Labs on-prem network KMS) **and AWS KMS** (cloud SaaS). GCP KMS and additional backends are follow-on. |
| Key strategy | **Configurable** (`per-purpose` / `per-tenant` / `shared-with-context`). Not hard-coded — §3. |
| DEK caching (hosted v1) | **Disabled.** New data key per operation. Revisit later with explicit thresholds. §6. |
| Plaintext-DEK secure memory | **Zeroize-on-drop only** for v1; `mlock`/secure-enclave deferred to a hardening pass (documented gap). §6. |
| Startup reachability probe | **Generate-data-key + decrypt-canary round-trip** with the real encryption context; failures fail-closed for hosted. §7. |

## 1. Why provider-neutral, not AWS-first

#897 originally proposed locking AWS KMS as the sole first launch backend. The Integro Labs network secrets/KMS infrastructure (reviewed in `roctinam/itops`) is **OpenBao** (HashiCorp Vault 1.17 fork) at `vault.integrolabs.net`, auto-unsealed via **tpm2-pkcs11 (vTPM)** with 5/3 recovery shares, plus an **OpenBao PKI / YubiHSM 2** chain (`itops/docs/security/pki-key-management-sop.md`, `itops/docs/applications/vault.md`, `itops ADR-006`). Identity is **Keycloak** OIDC.

For this environment the envelope-encryption backend is **Vault Transit (OpenBao)**, not AWS KMS. Hosted SaaS deployments will use **AWS KMS**. We must support both — and others — without a major break. Therefore the contract is **provider-neutral at the core** (one trait, one blob format, one canonical AAD schema) with **per-backend mappings**, and the **key strategy is configuration, not a baked-in assumption**.

## 2. Provider model & configuration

`KeyProviderKind` (extensible enum, per ADR-093):

| Kind | Backend | Posture |
|---|---|---|
| `env` | `EnvKeyProvider` (local master) | Single-tenant desktop/HotM sidecar only; forbidden in hosted multi-tenant. |
| `vault-transit` | OpenBao / HashiCorp Vault Transit | **Launch** — on-prem hosted (Integro Labs default). Seal/HSM is the backend's concern (tpm2-pkcs11 / YubiHSM). |
| `aws-kms` | AWS KMS (`aws-sdk-kms`) | **Launch** — cloud SaaS hosted. |
| `gcp-kms` | GCP Cloud KMS | Follow-on. |

Canonical config (namespaced; never ad-hoc env reads):

```
FORTEMI_KEY_PROVIDER = env | vault-transit | aws-kms | gcp-kms
FORTEMI_KEY_STRATEGY = per-purpose | per-tenant | shared-with-context   # §3
FORTEMI_KEY_CONTEXT_VERSION = 1                                          # §4

# vault-transit (OpenBao)
FORTEMI_VAULT_ADDR, FORTEMI_VAULT_NAMESPACE, FORTEMI_VAULT_TRANSIT_MOUNT,
FORTEMI_VAULT_TRANSIT_KEY, FORTEMI_VAULT_AUTH_METHOD (+ method-specific)

# aws-kms
FORTEMI_AWS_KMS_KEY_ID (ARN/alias), FORTEMI_AWS_REGION, FORTEMI_AWS_KMS_KEY_MAP (purpose→key, optional)

# gcp-kms (follow-on)
FORTEMI_GCP_KMS_KEY_RING, FORTEMI_GCP_KMS_KEY, FORTEMI_GCP_LOCATION, FORTEMI_GCP_PROJECT
```

Hosted multi-tenant startup fails closed if `FORTEMI_KEY_PROVIDER=env` (ADR-093 §"Fail-closed default"). Secrets (Vault token, AWS creds) come from the runtime's secret source, never logged, never in process args.

## 3. Configurable key strategy

The tenant/purpose-to-key mapping is selected by `FORTEMI_KEY_STRATEGY`, so each deployment chooses its isolation/cost trade-off:

| Strategy | Mapping | Isolation | Cost | Fits |
|---|---|---|---|---|
| `per-purpose` | One KEK per purpose (`user_secret`, `oauth_refresh`, …); tenant bound via AAD `tenant_id` + IAM/policy | Strong (purpose) + AAD (tenant) | Low | **Default.** Vault Transit (one key per purpose); AWS (few CMKs). |
| `per-tenant` | One KEK per tenant | Strongest blast-radius; per-tenant disable/rotate | High at scale (AWS $/key; Vault key count) | Regulated/high-isolation tenants. |
| `shared-with-context` | One KEK; all isolation via AAD context + policy | Weakest cryptographic separation | Lowest | Small/single-tenant hosted. |

The `KeyProvider` impl resolves `(purpose, tenant)` → backend key reference per the configured strategy. Backends declare which strategies they support; an unsupported (strategy, backend) pair fails closed at startup. **Recommended default:** `per-purpose` with `tenant_id` in the AAD (works on both Vault Transit and AWS KMS).

## 4. Canonical AAD / encryption-context schema

One versioned, **non-secret** context, reconstructed on decrypt from trusted DB/`AuthContext` state — never accepted from the caller. Each backend maps it to its mechanism: **AWS** `EncryptionContext`, **Vault Transit** `context` (base64), **GCP** `additionalAuthenticatedData`.

```
fortemi_context_version = 1
tenant_id        # where applicable
user_id          # where applicable
purpose          # user_secret | oauth_refresh_token | ...
resource_id      # secret_id / row id, where applicable
provider_kind    # vault-transit | aws-kms | ...
kek_ref          # configured key alias/name/ARN (non-secret)
schema           # table family, e.g. user_secrets
```

Values MUST be non-secret and free of user-controllable free-text (AWS/Vault may surface context in logs/policy). The full context string is stored/reconstructed; a context-schema version is recorded in the blob so older blobs decrypt after a schema bump.

## 5. Provider-neutral `EncryptedBlob` / `WrappedKey`

The on-disk format is backend-agnostic; backend specifics live in an opaque, versioned metadata bag — so adding Vault/GCP/HSM never changes the envelope shape.

```rust
struct EncryptedBlob {
    format_version: u16,          // envelope format version
    aead_alg: AeadAlg,            // e.g. XChaCha20-Poly1305 / AES-256-GCM
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,          // AEAD(ciphertext+tag) of the plaintext under the DEK
    wrapped_key: WrappedKey,
}

struct WrappedKey {
    provider_kind: KeyProviderKind,
    kek_ref: String,              // alias/name/ARN — opaque, non-secret
    context_version: u16,         // AAD schema version (§4)
    wrapped_dek: Vec<u8>,         // DEK wrapped by the backend (KMS ciphertext / Vault ciphertext)
    provider_metadata: BTreeMap<String,String>, // opaque, backend-specific, non-secret
    created_at: Timestamp,
    rewrapped_at: Option<Timestamp>,
}
```

`provider_metadata` carries backend specifics as needed without changing the struct: AWS `KeyId`/`KeyMaterialId`; Vault key name + key version; GCP key version. No plaintext keys, DEKs, passphrases, or raw provider error bodies ever enter the blob.

## 6. DEK & secret lifetime (v1)

- **No DEK cache** in hosted v1 — `generate_dek` per operation. (AWS Encryption SDK default; smallest plaintext-DEK window.) A bounded cache may be reconsidered later with explicit TTL/message/byte/capacity thresholds, zeroize-on-evict, and audit.
- **Zeroize-on-drop** for all plaintext DEK and provider-secret buffers (existing `matric-crypto` zeroize). **`mlock`/secure memory deferred** to a later hardening pass — documented known gap (swap/coredump exposure window).
- Never log/emit plaintext keys, DEKs, wrapped DEKs, ciphertext, provider credentials, Authorization headers, or raw KMS/Vault error bodies (per #711/#974).

## 7. Startup reachability probe

Hosted boot performs a **round-trip** that proves data-key generation **and** decrypt authorization under the real encryption context — `DescribeKey`/key-read alone is insufficient (it doesn't prove decrypt authz):

- **AWS KMS:** `GenerateDataKey` (with `DryRun` where available) + a `Decrypt` of a startup canary using the actual `EncryptionContext`.
- **Vault Transit (OpenBao):** `encrypt` + `decrypt` round-trip on the transit key with the actual `context`.
- **Result:** auth-denied / unreachable / context-mismatch / disabled-or-sealed-key → **fail closed** (block hosted startup). Transient-but-recoverable conditions may mark health degraded per the matrix below; hosted does not start in a state that cannot perform key ops.

## 8. Fail-closed degraded-mode matrix

| Condition | Hosted behavior |
|---|---|
| Provider unreachable at startup | Block startup. |
| Provider unreachable at runtime (no cache) | Fail the operation closed; mark health degraded; emit `key.*` audit. |
| Access denied / not authorized | Fail closed; audit `key.decrypt_denied` / `key.startup_check`. |
| Ciphertext/context mismatch (AAD) | Fail closed; audit `key.decrypt_denied` (context mismatch). |
| KEK disabled / Vault key deleted / sealed | Fail closed. |
| Rotation/rewrap in progress | Old blobs still decrypt (old key version); new encryptions use current metadata; no hard failure. |
| Audit sink unavailable for a key event | Per #711 event-class policy: key events are fail-closed-sensitive once hosted enforcement is active. |
| `env` provider in hosted multi-tenant | Refuse to start (ADR-093). |

## 9. Rotation / rewrap

- Model rotation as **provider metadata + key version**, not "new `kek_id`". AWS KMS automatic/on-demand rotation changes key material transparently — decrypt does not select a version; do not treat rotation as a new key id. Vault Transit rotation creates a new key **version**; old versions still decrypt.
- **Rewrap** is an online background job: read each `WrappedKey` → `unwrap_dek` (old version) → `generate_dek`/`rewrap` under current version → atomic row update; record `rewrapped_at` + updated `provider_metadata`. Tests prove old blobs decrypt before and after rotation and that new encryptions carry current metadata.
- `EnvKeyProvider` rotation is a manual, downtime runbook (re-wrap with new master). Not for multi-tenant.

## 10. Test fixtures & live-dev boundary

- **Unit:** an in-memory/mock `KeyProvider` for envelope/AAD/rotation logic with no network.
- **Vault Transit:** a dev OpenBao (or SoftHSM-backed dev seal) integration profile mirroring the itops topology.
- **AWS KMS:** LocalStack / `DryRun` for CI; a gated live-KMS profile for release verification.
- Rotation tests assert old wrapped DEKs decrypt after rotate; context-mismatch / decrypt-denied / disabled-key / startup-check failures emit **metadata-only** audit events (no raw key/ciphertext).

## 11. Follow-ups

- Update #734 to implement this provider-neutral, configurable-strategy contract (Vault-Transit **and** AWS KMS at launch), superseding the AWS-only framing.
- Link this doc from #730/#731 before their implementation starts.
- File/keep follow-on issues for GCP KMS and any HSM-direct backend; reserve the `key.*` audit taxonomy now (#711) so wiring is mechanical when #734 lands.

## References

- ADR-093 `docs/architecture/adr/ADR-093-key-provider-trait.md`; #897 (this contract), #734/#730/#731, #711/#910 (audit)
- itops: `roctinam/itops` — `docs/applications/vault.md`, `docs/security/pki-key-management-sop.md`, `docs/architecture/adr/ADR-006-certificate-secrets-management.md`, `config/secrets/README.md`
- AWS KMS GenerateDataKey / EncryptionContext / key rotation; HashiCorp/OpenBao Vault Transit secrets engine; RFC 5869 (HKDF); NIST SP 800-57
