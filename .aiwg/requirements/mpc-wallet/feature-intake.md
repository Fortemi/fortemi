# Project Intake Form — MPC Wallet & Personal Trust Network

## Metadata

| Field | Value |
|-------|-------|
| **Project Name** | MPC Wallet & Personal Trust Network |
| **Requestor** | Joseph Magly |
| **Date** | 2026-04-09 |
| **Parent Project** | Fortemi (matric-memory) |
| **Affected Crates** | `matric-crypto` (primary), `matric-api`, `matric-db`, `matric-core` |
| **Stakeholders** | Fortemi core team, Roko Network validators, Fortemi self-hosted users |

## Problem and Outcomes

### Problem Statement

Fortemi users currently have a single-purpose cryptographic identity: an X25519 keypair used exclusively for note encryption via the PKE system. This identity cannot sign anything, cannot attest trust in other users, and has no relationship to the Roko Network's secp256k1 signing ecosystem. Users who want to participate in both Fortemi (encrypted knowledge) and Roko (temporal receipts, blockchain transactions) must manage separate key materials with no unified identity or trust model.

Meanwhile, the conventional approach to unified identity — centralized PKI with institutional certificate authorities — contradicts Fortemi's edge-first philosophy. Users should not depend on a third-party CA hierarchy to establish trust between each other or to certify their own devices.

The MPC Wallet & Personal Trust Network solves this by making each user their own certificate authority. A single identity key, never materialized whole on any device, is distributed as threshold shares across the user's own devices using FROST. This identity key becomes the root of trust for everything: encrypting notes, signing Roko temporal receipts (via device delegation), attesting trust in other users, and issuing short-lived device certificates.

### Target Personas

1. **Fortemi self-hosted users** — Already use PKE encryption for notes. Want a stronger identity model that secures their devices and lets them selectively trust other Fortemi users for collaboration.
2. **Roko Network validators** — Need secp256k1 signing for temporal receipts and ECDSA consensus. Want device-level key delegation so validator nodes can sign without exposing the identity key.
3. **Privacy-conscious individuals** — Want self-sovereign identity without relying on institutional CAs. Control their own trust store, decide who they trust directly, and anchor trust events to physics-grounded timestamps via Roko.

### Success Metrics

| Metric | Target |
|--------|--------|
| FROST threshold signing ceremony completes successfully | 2-of-3 share threshold, < 2 second ceremony latency on LAN |
| Device certificate issuance from MPC wallet | DeviceCerts issued with < 5 second latency including Roko temporal anchoring |
| Trust attestation round-trip | User A attests trust in User B, attestation is threshold-signed and Roko-timestamped, verifiable by any third party |
| Existing PKE encryption continues to work | Zero regression — all existing `mm:` address encryption/decryption paths unchanged |
| Key share backup and recovery | User can recover identity from any 2-of-3 shares stored on separate devices |
| Multi-curve key derivation | Single identity seed deterministically produces keys for all four curves (secp256k1, X25519, Ed25519, Sr25519) |

## Scope and Constraints

### In-Scope

1. **MPC Key Management (FROST)**
   - FROST (Flexible Round-Optimized Schnorr Threshold) implementation for 2-of-3 threshold signing over secp256k1
   - Distributed Key Generation (DKG) ceremony across user's devices
   - Key share storage with per-device encryption (Argon2id + AES-256-GCM, consistent with existing PKE key storage)
   - Share refresh protocol (re-share without changing the public key)
   - Recovery from lost share (regenerate one share from remaining 2)

2. **Multi-Curve Key Derivation**
   - Deterministic derivation of curve-specific keypairs from the identity root:
     - **secp256k1**: Roko temporal receipt signing, Ethereum-compatible addresses
     - **X25519**: Fortemi note encryption (bridge to existing PKE system)
     - **Ed25519**: CA operations (DeviceCert signing, trust attestation signing)
     - **Sr25519**: Substrate BABE block production (if user runs a validator)
   - BIP-32-style hierarchical derivation with curve-specific hardened paths
   - Domain separation to prevent cross-curve key reuse attacks

3. **Self-Sovereign CA (Personal Trust Root)**
   - User's MPC identity acts as a self-signed CA root
   - CA certificate format (X.509-inspired but purpose-built, not full X.509 complexity)
   - Certificate fields: subject (user identity), issuer (self), validity period, public key, extensions (permitted curves, device constraints)
   - Certificate serialization format (CBOR for compactness, with canonical encoding for deterministic hashing)

4. **Device Delegation (DeviceCerts)**
   - MPC wallet issues short-lived DeviceCerts for user's devices
   - DeviceCert binds a device's local keypair to the user's identity
   - Device keys are generated locally on each device (not derived from identity key)
   - DeviceCert fields: device public key, permitted operations (e.g., "roko-sign", "fortemi-encrypt", "api-auth"), validity window, issuer signature (Ed25519 from MPC wallet)
   - Revocation via short-lived certs (default 24h) + optional revocation list

5. **P2P Trust Attestations**
   - Trust attestation structure: "User A trusts User B for {scope} as of {timestamp}"
   - Attestation is threshold-signed by User A's MPC wallet (FROST ceremony)
   - Attestation gets a Roko temporal receipt for physics-grounded timestamp
   - Trust is direct and non-transitive — User A trusting User B does not imply User A trusts User C whom User B trusts
   - Trust scopes: `knowledge-sharing`, `device-endorsement`, `validator-peer`
   - Trust store is local per-user (no global trust registry)

6. **Roko Temporal Anchoring**
   - RPC client for Roko Network temporal receipt submission
   - Submit hash of trust attestation / DeviceCert issuance to Roko for PoAT timestamping
   - Verify temporal receipts (check validator signatures, consensus confirmation)
   - Temporal receipt storage alongside the anchored artifact
   - Graceful degradation when Roko is unavailable (local timestamp with deferred anchoring queue)

7. **PKE Bridge**
   - Derive X25519 keypair from MPC identity for backward compatibility with existing `mm:` address system
   - Migration path: existing standalone PKE keypairs can be "claimed" by an MPC identity (sign the old public key with the new identity)
   - New MPC-backed users can encrypt/decrypt notes using the same `encrypt_pke`/`decrypt_pke` API

8. **Database Schema**
   - `mpc_identities` table: identity public keys, creation metadata, Roko anchor receipts
   - `device_certs` table: issued DeviceCerts, validity windows, revocation status
   - `trust_attestations` table: signed attestations with Roko temporal receipts
   - `key_shares` table: encrypted share metadata (NOT the shares themselves — shares live on devices)
   - Per-memory schema isolation (consistent with existing multi-memory architecture)

9. **API Endpoints**
   - `POST /api/v1/mpc/identity` — Initialize MPC identity (returns DKG session)
   - `POST /api/v1/mpc/dkg/{session_id}/contribute` — Submit DKG share contribution
   - `POST /api/v1/mpc/sign` — Initiate threshold signing ceremony
   - `POST /api/v1/mpc/device-certs` — Issue DeviceCert for a device
   - `GET /api/v1/mpc/device-certs` — List active DeviceCerts
   - `DELETE /api/v1/mpc/device-certs/{cert_id}` — Revoke a DeviceCert
   - `POST /api/v1/mpc/trust` — Create trust attestation
   - `GET /api/v1/mpc/trust` — List trust attestations (inbound and outbound)
   - `DELETE /api/v1/mpc/trust/{attestation_id}` — Revoke trust attestation
   - `GET /api/v1/mpc/identity` — Get identity info (public keys for all curves, active devices)

10. **MCP Server Tools**
    - `manage_mpc_wallet` — Discriminated union tool for identity, device, and trust operations
    - Integration with existing `manage_encryption` tool for PKE bridge

### Out-of-Scope

- **Browser CA trust store integration** — DeviceCerts are application-level, not OS/browser-level X.509 certificates. No `chrome://settings/certificates` integration.
- **Enterprise PKI interop** — No PKCS#11, no HSM integration, no LDAP certificate publishing. This is personal trust, not organizational trust.
- **Mobile app** — Device share management will initially be CLI and API only. Mobile key share management is a future project.
- **Transitive trust / web of trust** — Trust attestations are strictly direct. No trust propagation algorithms, no trust scores, no PageRank-style trust metrics.
- **Full X.509 compliance** — DeviceCerts are purpose-built, not ASN.1/DER-encoded X.509. No CRL distribution points, no OCSP responders.
- **Cross-archive MPC operations** — MPC identity is per-user, not per-archive. Cross-archive linking is already out of scope per the multi-memory architecture.
- **Substrate keystore format import/export** — Initial version uses its own key storage. Substrate keystore interop is a future enhancement.
- **Automated key rotation** — Share refresh is manual (user-initiated). Automated rotation policies are future work.

### Platform Constraints

| Constraint | Detail |
|------------|--------|
| **Language** | Rust for all cryptographic operations (matric-crypto crate). No C FFI for crypto primitives — pure Rust implementations only. |
| **MCP Server** | TypeScript (mcp-server/index.js). New MPC tools follow existing discriminated-union pattern. |
| **Database** | PostgreSQL 18 with existing migration framework (sqlx). New tables in per-memory schemas. |
| **Existing Dependencies** | Must coexist with current `x25519-dalek`, `aes-gcm`, `argon2`, `blake3`, `bs58` stack. |
| **FROST Implementation** | Use `frost-secp256k1` from the FROST reference implementation (ZCash Foundation). |
| **Additional Crypto Deps** | `k256` (secp256k1), `ed25519-dalek` (Ed25519), `schnorrkel` (Sr25519). |

## Non-Functional Preferences

### Security Posture: Critical

This is a cryptographic identity system. Security is not a preference — it is the primary design constraint.

| Requirement | Target |
|-------------|--------|
| **Key material exposure** | Identity key NEVER materialized whole on any single device. Shares are encrypted at rest with Argon2id (memory cost >= 64 MiB). |
| **Side-channel resistance** | All secret operations use constant-time implementations. `zeroize` on drop for all key material. |
| **Cryptographic agility** | Protocol version field in all serialized structures to allow future cipher suite upgrades without breaking existing data. |
| **Audit trail** | All MPC ceremonies, DeviceCert issuances, and trust attestations are logged with Roko temporal receipts when available. |
| **Secure defaults** | DeviceCert validity defaults to 24 hours. Trust attestation requires explicit scope. No wildcard permissions. |
| **Dependency vetting** | All new cryptographic dependencies must be from audited or widely-reviewed crates (dalek-cryptography, RustCrypto, ZCash Foundation). |

### Reliability

| Requirement | Target |
|-------------|--------|
| **DKG ceremony tolerance** | DKG must handle network interruptions gracefully — session state is persisted, ceremony can resume. |
| **Roko unavailability** | Temporal anchoring degrades to local timestamps with a deferred anchoring queue. Queue drains automatically when Roko reconnects. |
| **Share loss** | Loss of 1-of-3 shares is recoverable. Loss of 2-of-3 shares is unrecoverable (by design — this is the security threshold). |
| **Backward compatibility** | All existing PKE operations (`encrypt_pke`, `decrypt_pke`, `mm:` addresses) continue to work unchanged. MPC identity is additive, not replacing. |

### Scale

| Requirement | Target |
|-------------|--------|
| **Identities per instance** | Hundreds (one per user on a self-hosted instance). Not millions. |
| **Trust attestations per user** | Tens to low hundreds (direct trust is intentionally limited). |
| **DeviceCerts per identity** | 3-10 active at any time (user's personal devices). |
| **DKG ceremonies** | Rare — once per identity creation, once per share refresh. Latency-tolerant. |
| **Threshold signing ceremonies** | Occasional — DeviceCert issuance, trust attestation signing. Seconds-level latency acceptable. |

### Observability

| Requirement | Detail |
|-------------|--------|
| **MPC ceremony logging** | Structured logs for DKG initiation, share contributions, ceremony completion/failure. No secret material in logs. |
| **DeviceCert lifecycle** | Log issuance, expiry, revocation events with device identifiers. |
| **Trust attestation events** | Log creation and revocation of trust attestations (parties and scope, not attestation content). |
| **Roko anchoring status** | Log temporal receipt submission, confirmation, and deferred queue depth. |
| **Health endpoint** | Extend `/health` with MPC subsystem status: DKG availability, Roko connectivity, active cert count. |

## Testing Strategy

### Coverage Requirements

| Level | Target | Rationale |
|-------|--------|-----------|
| **Unit tests** | 90%+ for crypto modules | Cryptographic correctness is non-negotiable. Every code path through key derivation, signing, and verification must be tested. |
| **Integration tests** | All API endpoints | Full round-trip: create identity, issue DeviceCert, create trust attestation, verify attestation. |
| **Property-based tests** | Key derivation, serialization | Use `proptest` to verify: any valid seed produces valid keys for all curves; serialization round-trips are lossless; address encoding is bijective. |

### Test Levels

1. **Crypto Correctness Tests**
   - FROST DKG produces valid key shares that reconstruct to the expected public key
   - Threshold signing with any 2-of-3 shares produces a valid signature
   - Threshold signing with only 1-of-3 shares fails
   - Multi-curve derivation produces keys that pass each curve's validation
   - X25519 derived key is compatible with existing PKE encryption/decryption
   - Ed25519 signatures verify correctly
   - secp256k1 threshold signatures verify as BIP-340 Schnorr (FROST output); Roko receipt verification remains recoverable ECDSA (`ecrecover`)
   - Cross-curve domain separation: derived keys for different curves are independent

2. **MPC Ceremony Tests**
   - DKG session lifecycle: initiate, contribute, complete
   - DKG with simulated network delays and retries
   - DKG with one non-responsive participant (should timeout gracefully)
   - Share refresh: new shares work, old shares are invalidated
   - Threshold signing ceremony: initiate, collect partial signatures, aggregate

3. **DeviceCert Tests**
   - Issue cert, verify signature, check validity window
   - Expired cert is rejected
   - Revoked cert is rejected
   - Cert with wrong permitted operations is rejected for unauthorized operations
   - Cert chain: DeviceCert traces back to MPC identity root

4. **Trust Attestation Tests**
   - Create attestation, verify threshold signature
   - Verify Roko temporal receipt (mocked Roko RPC)
   - Attestation with invalid signature is rejected
   - Revoked attestation is no longer valid
   - Trust scope enforcement: attestation for `knowledge-sharing` does not grant `validator-peer`

5. **PKE Bridge Tests**
   - MPC-derived X25519 key encrypts/decrypts with existing PKE format
   - Existing standalone PKE keypair claim process
   - Mixed recipients: MPC-backed user + standalone PKE user in same encryption

6. **Roko Integration Tests**
   - Temporal receipt submission (mocked Roko RPC)
   - Receipt verification (mocked validator responses)
   - Deferred anchoring queue: submit when offline, drain when reconnected
   - Graceful degradation: all operations work without Roko (just no temporal anchoring)

### Automation

- All crypto tests run in CI with `cargo test --workspace` (no `#[ignore]`)
- MPC ceremony tests use in-process simulation (no actual network — shares are passed as function arguments)
- Roko RPC tests use a mock HTTP server (wiremock-rs)
- Property-based tests run with `proptest` configured for 1000 cases in CI, 10000 in nightly
- No `std::env::set_var` in tests (per project rules) — use constructor injection for all configuration

## Data

### Classification

| Data Type | Classification | Storage |
|-----------|---------------|---------|
| **Key shares** | **RESTRICTED** | Encrypted on user's devices only. Never stored in Fortemi database. `key_shares` table stores metadata (device ID, creation date, share index) not the share itself. |
| **Identity public keys** | **Public** | Stored in `mpc_identities` table. Shareable. |
| **DeviceCerts** | **Internal** | Stored in `device_certs` table. Contains device public key and permissions, signed by identity. |
| **Trust attestations** | **Internal** | Stored in `trust_attestations` table. Signed, timestamped, verifiable by any party with the signer's public key. |
| **Roko temporal receipts** | **Public** | Stored alongside anchored artifacts. Independently verifiable against Roko chain. |
| **DKG session state** | **RESTRICTED** | Ephemeral, in-memory only during ceremony. Zeroized on completion or timeout. Never persisted to disk or database. |

### Retention

| Data Type | Retention |
|-----------|-----------|
| **Identity records** | Permanent (until user deletes account) |
| **DeviceCerts** | Retained for audit trail after expiry. Purged after 1 year past expiry. |
| **Trust attestations** | Retained until explicitly revoked + 90 day grace period for propagation |
| **Roko temporal receipts** | Permanent (they are blockchain-anchored, deletion is meaningless) |
| **DKG session state** | Maximum 10 minute TTL, then forcibly zeroized |

### Backup Considerations

- Key shares are NOT part of Fortemi database backups (they live on devices)
- Identity public keys, DeviceCerts, and trust attestations ARE backed up with normal database backups
- Users are responsible for backing up their own key shares across their devices (the 2-of-3 threshold IS the backup strategy)

## Integrations

### Roko Network

| Aspect | Detail |
|--------|--------|
| **Protocol** | JSON-RPC 2.0 over WebSocket (Substrate standard) |
| **Endpoint** | Configurable via `ROKO_RPC_URL` env var |
| **Operations** | Submit temporal receipt request, query receipt status, verify receipt |
| **Authentication** | None for read operations. Temporal receipt submission requires a valid secp256k1 signature (device key signs, DeviceCert proves authority). |
| **Failure mode** | Roko unavailability does not block any Fortemi operation. Deferred anchoring queue with exponential backoff retry. |

### Existing Fortemi PKE System

| Aspect | Detail |
|--------|--------|
| **Bridge mechanism** | MPC identity derives an X25519 keypair that produces a standard `mm:` address |
| **Encryption compatibility** | MPC-derived X25519 key works with existing `encrypt_pke`/`decrypt_pke` functions unchanged |
| **Address format** | Same `mm:` prefix and Base58Check encoding. Indistinguishable from standalone PKE addresses. |
| **Migration** | Existing users can optionally "claim" their standalone PKE keypair by signing the public key with their new MPC identity. This is a one-way upgrade — the old keypair continues to work. |
| **Code impact** | Zero changes to `crates/matric-crypto/src/pke/`. The bridge is a new module that wraps derivation and delegates to existing functions. |

### Fortemi API Authentication

| Aspect | Detail |
|--------|--------|
| **Current auth** | OAuth2 + API keys (when `REQUIRE_AUTH=true`) |
| **MPC addition** | DeviceCerts can serve as an additional authentication method. Device presents DeviceCert + signs a challenge with its device key. API verifies DeviceCert signature chain back to MPC identity. |
| **Priority** | Lower priority than core MPC functionality. Can be added in a follow-up iteration. |

### Substrate Keystore (Future)

| Aspect | Detail |
|--------|--------|
| **Status** | Out of scope for initial implementation |
| **Future plan** | Export Ed25519 and Sr25519 keys in Substrate keystore format for validator node integration |
| **Dependency** | Requires `sp-keystore` crate from Substrate, which has a large dependency tree. Evaluate as optional feature gate. |

## Architecture Preferences

### Crate Structure

Extend the existing `matric-crypto` crate with new modules rather than creating a separate crate. The MPC wallet is fundamentally a cryptographic primitive, and keeping it in `matric-crypto` maintains the existing pattern.

```
crates/matric-crypto/src/
  pke/              # Existing — unchanged
  mpc/
    mod.rs          # MPC module root
    frost.rs        # FROST threshold signing (wraps frost-secp256k1)
    dkg.rs          # Distributed Key Generation ceremony
    identity.rs     # MPC identity type (multi-curve public keys)
    derivation.rs   # BIP-32-style multi-curve key derivation
    device_cert.rs  # DeviceCert issuance and verification
    trust.rs        # Trust attestation creation and verification
    roko.rs         # Roko temporal receipt client
    share.rs        # Key share types and encrypted storage format
    bridge.rs       # PKE bridge (derive X25519 from identity)
  lib.rs            # Updated to export mpc module
```

### Key Architectural Decisions

1. **MPC wallet is the commissioning authority, NOT the runtime signer.** The MPC ceremony (gathering 2-of-3 shares) is used only for rare, latency-tolerant operations: issuing DeviceCerts, signing trust attestations, share refresh. Device keys handle all frequent operations (API auth, Roko temporal receipt signing, note encryption).

2. **FROST over generic MPC.** FROST is specifically designed for Schnorr threshold signatures and has a clean, audited Rust implementation from the ZCash Foundation. We do not need general-purpose MPC (which would be far more complex for no additional benefit in this use case).

3. **Multi-curve derivation from single seed, NOT multi-curve MPC.** Running FROST across four different curves would be prohibitively complex. Instead, the MPC wallet manages a single secp256k1 identity key via FROST. Other curve keys are deterministically derived from the identity using HKDF with curve-specific domain separation. Only secp256k1 operations benefit from threshold signing; other curves use the derived keys directly (which is acceptable because those keys are only used on devices via DeviceCerts).

4. **CBOR for certificate serialization, NOT ASN.1/DER.** CBOR is compact, well-specified, has canonical encoding (RFC 8949 deterministic encoding), and has excellent Rust support (`ciborium` crate). ASN.1/DER would add massive complexity for no benefit since we are not integrating with the X.509 ecosystem.

5. **Non-transitive trust by design.** Transitive trust models (web of trust, trust propagation) introduce complexity and attack surface that is inappropriate for a personal trust system. If User A wants to trust User C, User A signs a direct attestation. User B's opinion is irrelevant to User A's trust decision.

6. **Roko anchoring is best-effort.** The system must work fully without Roko connectivity. Temporal anchoring adds tamper-evidence and non-repudiation but is not required for trust or device delegation to function.

### API Design

Follow existing Fortemi API patterns:
- Axum handlers in `crates/matric-api/`
- Database operations in `crates/matric-db/`
- Core types in `crates/matric-core/`
- MCP tools in `mcp-server/index.js` using discriminated-union pattern
- OpenAPI spec updated in `crates/matric-api/src/openapi.yaml`
- Schema-aware (multi-memory) via `SchemaContext` and `SET LOCAL search_path`

## Risk and Trade-offs

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **FROST implementation bugs** | Medium | Critical | Use ZCash Foundation's audited `frost-secp256k1` crate. Do not roll our own threshold signing. Extensive property-based testing. |
| **Multi-curve derivation weakness** | Low | Critical | Use established BIP-32-style derivation with HKDF. Each curve gets a unique domain separation tag. Review against known cross-curve attacks. |
| **DKG ceremony UX friction** | High | Medium | DKG requires coordination across devices. Initial version is CLI-only which limits adoption. Mitigate with clear documentation and a "guided setup" flow. |
| **Roko Network instability** | Medium | Low | Deferred anchoring queue ensures no Fortemi functionality depends on Roko availability. All trust and device operations work offline. |
| **Dependency sprawl** | Medium | Medium | Adding `frost-secp256k1`, `k256`, `ed25519-dalek`, `schnorrkel`, `ciborium` is significant. Gate behind Cargo features to keep compile times reasonable for users who do not need MPC. |
| **Key share loss** | Medium | Critical | 2-of-3 threshold means loss of 2 devices is unrecoverable. Document clearly. Encourage geographically distributed storage. No "forgot password" recovery — this is by design. |
| **Protocol versioning** | Low | High | All serialized structures include a version field from day one. Migration path must be planned before v2 of any structure. |

### Trade-off Decisions

| Trade-off | Decision | Rationale |
|-----------|----------|-----------|
| **Security vs. Convenience** | Security wins | 2-of-3 threshold is less convenient than a single key but dramatically reduces single-point-of-failure risk. MPC ceremony latency is acceptable because it is rare. |
| **Feature completeness vs. Incremental delivery** | Incremental | Ship core MPC + DeviceCerts first, trust attestations second, Roko anchoring third. Each layer is independently useful. |
| **Custom cert format vs. X.509** | Custom (CBOR) | X.509 compliance would add months of work and massive dependency weight for zero benefit since we are not integrating with browser/OS trust stores. |
| **Multi-curve MPC vs. Single-curve MPC + derivation** | Single-curve + derivation | Running FROST across four curves is research-grade complexity. Single-curve FROST + deterministic derivation is well-understood and auditable. |
| **Transitive trust vs. Direct trust** | Direct only | Transitive trust is a solved problem (PGP web of trust) but introduces complexity, ambiguity, and attack surface. Direct trust is simple, auditable, and sufficient for Fortemi's use case. |

### Priority Weights

| Factor | Weight | Rationale |
|--------|--------|-----------|
| **Correctness** | 10/10 | Crypto bugs are not "we'll fix it next sprint." They compromise user identity and data. |
| **Security** | 10/10 | The entire feature is a security primitive. |
| **Auditability** | 9/10 | Every crypto decision must be explainable and reviewable. Clean code over clever code. |
| **Backward compatibility** | 9/10 | Existing PKE users must not be affected. Zero breaking changes to the encryption API. |
| **Performance** | 5/10 | MPC ceremonies are rare. Seconds-level latency is acceptable. Optimize device key operations (those are frequent). |
| **Feature breadth** | 4/10 | Ship a small, correct system. Breadth comes in later iterations. |

## Team and Operations

### Team Requirements

| Role | Skills | Allocation |
|------|--------|------------|
| **Crypto engineer** | Rust, threshold cryptography, elliptic curve mathematics, FROST protocol | Primary implementer. Must understand the math, not just the API. |
| **Backend engineer** | Axum, sqlx, PostgreSQL migrations, existing Fortemi codebase | API endpoints, database schema, integration with existing auth system. |
| **Security reviewer** | Cryptographic protocol review, side-channel analysis | Review all crypto code before merge. Must not be the same person as the implementer. |

### Operational Considerations

| Concern | Plan |
|---------|------|
| **Key share backup documentation** | Ship user-facing documentation explaining the 2-of-3 model, what happens if a device is lost, and how to perform share refresh. This documentation is a release blocker. |
| **Roko RPC monitoring** | Add Roko connectivity to the health endpoint. Alert on sustained disconnection (> 1 hour) so deferred anchoring queue does not grow unbounded. |
| **DeviceCert expiry management** | Background job in `matric-jobs` to clean up expired DeviceCerts and optionally notify users of upcoming expiry. |
| **Migration path** | Existing users are not required to adopt MPC. The feature is opt-in. Standalone PKE continues to work indefinitely. |

### Delivery Phases

**Phase 1 — Core MPC + PKE Bridge**
- FROST DKG and threshold signing
- Multi-curve key derivation
- X25519 bridge to existing PKE system
- Key share encrypted storage format
- Database schema for identities and shares metadata

**Phase 2 — Device Delegation**
- DeviceCert issuance and verification
- DeviceCert-based API authentication
- Certificate lifecycle management (expiry, revocation)
- Database schema for device certs

**Phase 3 — Trust Attestations**
- Trust attestation creation and verification
- Trust scope enforcement
- Local trust store management
- Database schema for trust attestations

**Phase 4 — Roko Temporal Anchoring**
- Roko RPC client
- Temporal receipt submission and verification
- Deferred anchoring queue
- Integration with DeviceCerts and trust attestations from phases 2-3

**Phase 5 — MCP and UX**
- MCP server tools for MPC wallet operations
- CLI improvements for DKG ceremony guidance
- OpenAPI spec updates
- User documentation
