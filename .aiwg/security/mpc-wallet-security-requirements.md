# Security Requirements: MPC Wallet & Personal Trust Network

| Field | Value |
|-------|-------|
| **Feature** | MPC Wallet & Personal Trust Network |
| **Project** | Fortemi (matric-memory) |
| **Created** | 2026-04-09 |
| **Last Updated** | 2026-04-09 |
| **Status** | Draft |
| **Classification** | Internal |
| **Related Risk Register** | `.aiwg/planning/mpc-wallet/risk-register.md` |

## Priority Levels

- **MUST**: Mandatory for launch. Failure to implement blocks release.
- **SHOULD**: Expected for launch. Deferral requires documented risk acceptance.
- **COULD**: Desired enhancement. May be deferred to post-launch iteration.

---

## 1. MPC Ceremony Security

Requirements governing the FROST Distributed Key Generation (DKG) and threshold signing ceremonies.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| MPC-001 | All DKG ceremony communication channels MUST be mutually authenticated using pre-established device identity keys before any key material is exchanged. Unauthenticated participants MUST be rejected before Round 1 begins. | MUST | Integration test: attempt DKG join with invalid device identity; verify rejection. Code review: confirm authentication check precedes first protocol message. |
| MPC-002 | Secret share transport between DKG participants MUST be encrypted with forward-secret channels (X25519 ephemeral ECDH + AES-256-GCM per session). Share ciphertext MUST use unique nonces per recipient per round. | MUST | Unit test: verify each share is encrypted with a distinct ephemeral keypair and nonce. Packet capture test: confirm no plaintext share bytes appear on the wire. |
| MPC-003 | The full group private key MUST never exist in memory on any single device at any point during DKG or signing. No code path shall combine threshold-or-more shares into a reconstructed private key. The system MUST enforce this by design: no function accepting t-or-more `SecretShare` values exists in the codebase. | MUST | Static analysis: `grep` / `clippy` lint for any function that accepts `Vec<SecretShare>` with length >= threshold. Property test: inject t shares into all public API surfaces; verify none produce a valid `SecretKey`. Code review gate. |
| MPC-004 | All intermediate cryptographic values (polynomial coefficients, partial signatures, ephemeral nonces, decrypted shares) MUST be zeroized immediately after use. Types holding secret material MUST implement `ZeroizeOnDrop` (extending the existing `matric-crypto` pattern from `keys.rs`). | MUST | Code review: every `struct` holding secret bytes derives `ZeroizeOnDrop`. Unit test: allocate secret, drop, read memory region (unsafe test helper) -- verify zeroed. CI: `cargo clippy` custom lint for missing `Zeroize` on types containing `[u8; 32]`. |
| MPC-005 | Every DKG and signing ceremony MUST produce an audit log entry containing: ceremony type, participant device IDs (not shares), timestamp, success/failure status, and a commitment hash (hash of all public commitments exchanged). Audit logs MUST NOT contain any secret material (shares, nonces, partial signatures). | MUST | Integration test: run DKG ceremony, verify audit log entry exists with required fields. Negative test: search audit log output for patterns matching base64-encoded 32-byte values; verify none match actual share material. |
| MPC-006 | Signing ceremony messages MUST include monotonically increasing round counters per participant. A participant MUST reject any message with a counter value less than or equal to the last seen counter for that sender. This prevents replay of old signing round messages. | MUST | Unit test: send valid Round 2 message, then replay it; verify rejection with `ReplayDetected` error. Integration test: simulate network duplication; verify exactly one signature produced. |
| MPC-007 | The signing coordinator MUST detect equivocation (a participant sending different commitments to different peers in the same round) by requiring all participants to broadcast their received commitments. If any commitment mismatch is detected, the ceremony MUST abort with an identifiable-abort report naming the equivocating participant. | MUST | Integration test: simulate equivocating participant (send different Round 1 commitments to two peers); verify ceremony aborts and abort report identifies the malicious participant by device ID. |
| MPC-008 | DKG ceremony SHOULD enforce a maximum duration timeout (default: 60 seconds). If any participant fails to respond within the timeout, the ceremony aborts cleanly with partial state zeroized. Timeout value MUST be configurable. | SHOULD | Integration test: start DKG with 3 participants, delay one participant beyond timeout; verify clean abort. Verify no share material persists after abort (memory zeroization). Configuration test: override timeout via config, verify new value applies. |
| MPC-009 | Pre-computed FROST nonce commitments (for latency optimization per R-007 mitigation) SHOULD be stored encrypted at rest and MUST have a maximum age of 24 hours. Nonces older than the maximum age MUST be discarded and regenerated. | SHOULD | Unit test: create nonce commitment, advance clock 25 hours, attempt signing; verify nonce rejection and regeneration. Storage test: verify nonce file is encrypted (not plaintext parseable). |
| MPC-010 | The system COULD support a "ceremony observer" role that receives public commitments and verification data without participating in signing. Observers MUST NOT receive any secret share material or partial signatures. | COULD | Integration test: attach observer to signing ceremony; verify observer log contains only public data (commitments, final signature). Negative test: verify observer cannot influence ceremony outcome. |

---

## 2. Key Storage Security

Requirements for storing MPC secret shares and related key material at rest, extending the existing `matric-crypto` PKE key storage patterns.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| KEY-001 | MPC secret shares MUST be encrypted at rest using the existing Argon2id + AES-256-GCM pattern from `matric-crypto::pke::key_storage`. The encrypted format MUST be versioned (extend `MMPKEKEY` header or introduce `MMPCSHR01` format) to allow future algorithm changes. | MUST | Unit test: write share to disk, read raw bytes, verify magic header present and body is not plaintext-parseable. Round-trip test: encrypt share, decrypt share, verify equality. |
| KEY-002 | On platforms supporting hardware-backed key storage (Android Keystore, iOS Secure Enclave, TPM 2.0, Windows Hello), the share encryption key SHOULD be derived from or protected by the hardware security module. The system MUST fall back gracefully to software-only Argon2id when hardware backing is unavailable. | SHOULD | Platform detection test: verify correct backend selection on each supported platform (mock hardware availability). Fallback test: disable hardware keystore mock; verify software path activates without error. |
| KEY-003 | All types holding secret share bytes MUST implement `Zeroize` and `ZeroizeOnDrop` (consistent with existing `PrivateKey` in `keys.rs`). Secret share bytes MUST NOT appear in `Debug`, `Display`, or `Serialize` output. | MUST | Compile-time: `#[derive(ZeroizeOnDrop)]` on all share types. Unit test: format share with `Debug`; verify output shows redacted placeholder (e.g., `SecretShare(***)`). `Serialize` test: serialize share struct; verify share bytes field is absent or redacted. |
| KEY-004 | Secret share bytes, decrypted private keys, and intermediate ceremony values MUST NOT appear in application logs at any log level (trace, debug, info, warn, error). This extends the existing no-key-logging pattern in `matric-crypto`. | MUST | Integration test: run DKG and signing ceremonies at `RUST_LOG=trace`; capture all log output; search for base64/hex patterns matching known share values; verify zero matches. Code review: no `tracing::debug!` or `log::debug!` calls that format secret types. |
| KEY-005 | Backup share encryption (the "recovery share" per R-006) MUST use a user-provided passphrase with minimum estimated entropy of 128 bits (zxcvbn score >= 4). The system MUST reject passphrases below this threshold with a clear error message. | MUST | Unit test: submit weak passphrase ("password123"); verify rejection. Submit strong passphrase (generated 20-char random); verify acceptance. Boundary test: submit passphrase at exactly score 3 and score 4; verify correct accept/reject. |
| KEY-006 | The recovery share file format MUST NOT contain any metadata that identifies the user, wallet address, or device. The file MUST contain only: format magic bytes, encryption parameters (salt, nonce), and the encrypted share blob. | MUST | Unit test: generate recovery share file; parse all bytes; verify no user-identifying strings. Verify file is indistinguishable from random data without the passphrase (no structured plaintext header beyond magic bytes). |
| KEY-007 | Share deletion MUST perform secure erasure: overwrite the file content with random bytes before filesystem deletion. On platforms supporting it (Linux with `shred`, macOS with `srm`), use OS-level secure delete. SHOULD fall back to single-pass random overwrite + `fsync` + `unlink`. | SHOULD | Unit test (Linux): write share file, invoke secure delete, attempt to read file; verify file absent. Integration test: verify `shred` invocation on Linux via process spy. Fallback test: mock `shred` unavailability; verify random overwrite path executes. |
| KEY-008 | The system COULD support encrypted share export for migration between devices. Exported shares MUST be encrypted to the receiving device's public key (not a passphrase) and MUST expire after a configurable window (default: 15 minutes). | COULD | Unit test: export share, import on simulated device; verify round-trip. Expiry test: export share, advance clock 16 minutes, attempt import; verify rejection. Verify export is encrypted to destination device's specific public key. |

---

## 3. Trust Attestation Security

Requirements for the peer-to-peer trust attestation system that forms the Personal Trust Network.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| TRUST-001 | Before accepting a trust attestation, the system MUST verify the attestation signature against the attester's known public key (MPC group key or device cert public key). Attestations with invalid or unverifiable signatures MUST be rejected and logged. | MUST | Unit test: create attestation with valid key, verify acceptance. Create attestation with wrong key, verify rejection with `InvalidSignature` error. Integration test: tamper with attestation bytes post-signing; verify rejection. |
| TRUST-002 | If a trust attestation includes a Roko temporal receipt, the system MUST verify the receipt signature via `ecrecover` and confirm the recovered address belongs to a known Roko authority. Attestations with invalid temporal receipts MUST be flagged but SHOULD NOT be auto-rejected (the attestation itself may still be valid, just unanchored). | MUST | Unit test: verify valid temporal receipt passes. Verify receipt with corrupted signature fails `ecrecover`. Verify receipt signed by non-authority address is flagged as `unanchored`. Integration test: verify UI displays anchored vs. unanchored status correctly. |
| TRUST-003 | Temporal receipt freshness MUST be validated: receipts older than a configurable maximum age (default: 1 hour) MUST trigger a warning. Receipts older than 24 hours SHOULD be treated as stale and the attestation marked as `stale_anchor`. | SHOULD | Unit test: create receipt with timestamp 30 minutes ago; verify passes freshness check. Create receipt 2 hours old; verify warning. Create receipt 25 hours old; verify `stale_anchor` flag. Configuration test: override max age; verify new threshold applies. |
| TRUST-004 | The system MUST check revocation status before trusting a device certificate used in an attestation. Revoked certificates MUST cause the attestation to be rejected with `RevokedCertificate` error. Revocation checks MUST query the issuing wallet's revocation list. | MUST | Integration test: create attestation from device cert, revoke cert, attempt to verify attestation; verify rejection. Verify that attestations created before revocation but verified after are also rejected (revocation is retroactive for pending verifications). |
| TRUST-005 | Trust MUST NOT propagate transitively. If Alice trusts Bob and Bob trusts Carol, Alice MUST NOT automatically trust Carol. The system MUST require explicit attestation for each trust relationship. Introductions (Bob introduces Carol to Alice) are permitted but MUST result in Alice explicitly verifying and attesting Carol. | MUST | Integration test: create A->B trust and B->C trust; query A's trust for C; verify `NotTrusted`. Verify introduction flow creates a pending verification request for A, not an automatic trust. Verify trust graph query correctly distinguishes direct trust from transitive paths. |
| TRUST-006 | Trust attestations MUST have an expiry timestamp. Default expiry SHOULD be 365 days. Expired attestations MUST be treated as inactive (not deleted, for audit trail) and MUST NOT be used for trust decisions. Re-attestation creates a new attestation record. | MUST | Unit test: create attestation with 365-day expiry; verify active. Advance clock 366 days; verify inactive. Verify inactive attestation remains in database (not deleted). Verify re-attestation creates new record with new expiry. |
| TRUST-007 | Attestation creation MUST include replay protection. Each attestation MUST contain a unique nonce and a reference to the attester's current monotonic counter. The system MUST reject attestations with a nonce that has been previously seen from the same attester. | MUST | Unit test: create attestation, record nonce; attempt to create identical attestation; verify rejection with `ReplayDetected`. Verify that different attestations from the same attester have strictly increasing counters. |
| TRUST-008 | The system COULD support trust attestation with confidence levels (e.g., `verified_in_person`, `verified_video`, `verified_online_only`). Confidence levels MUST be attester-declared and MUST NOT be automatically elevated. Higher-confidence attestations COULD be weighted more heavily in trust graph queries. | COULD | Unit test: create attestations with each confidence level; verify stored correctly. Verify no code path upgrades confidence level automatically. If weighted queries are implemented: verify `verified_in_person` scores higher than `verified_online_only` in graph traversal. |

---

## 4. Device Certificate Security

Requirements for the short-lived device certificates delegated by the MPC wallet for routine operations.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| CERT-001 | Device certificates MUST have a maximum lifetime of 24 hours (default). The maximum configurable lifetime MUST NOT exceed 72 hours. Certificate creation MUST reject requested lifetimes exceeding the maximum. | MUST | Unit test: request cert with 24h lifetime; verify issued. Request cert with 73h lifetime; verify rejection with `ExceedsMaxLifetime` error. Configuration test: set max to 48h; verify 49h request rejected. |
| CERT-002 | Each device certificate MUST include an explicit scope field enumerating the operations it authorizes (e.g., `sign_note`, `create_attestation`, `read_encrypted`). Operations not listed in the scope MUST be rejected when presented with the certificate. Default scope SHOULD be `sign_note, read_encrypted` (routine operations only). | MUST | Unit test: issue cert with `sign_note` scope; attempt `create_attestation` operation; verify rejection with `ScopeViolation` error. Verify default scope includes only routine operations. Verify commissioning operations (`rotate_key`, `delegate_device`) require explicit scope grant. |
| CERT-003 | The MPC wallet MUST provide immediate revocation capability for any issued device certificate. Revocation MUST take effect within 60 seconds across all system components that validate certificates. Revocation MUST be persisted (survives restart). | MUST | Integration test: issue cert, use it successfully, revoke it, attempt use within 60 seconds; verify rejection. Restart test: revoke cert, restart system, attempt use; verify still revoked. Timing test: measure revocation propagation delay; assert < 60 seconds. |
| CERT-004 | Certificate validation MUST be offline-capable: a valid signature, time window, and non-revoked status in a fresh local revocation cache are sufficient for authorization. Wallet liveness checks are optional hardening and MUST NOT be required on the hot path. If wallet liveness is unavailable and revocation cache freshness exceeds configured TTL, operations SHOULD be allowed with a `revocation_stale` warning and auditable policy flag. | SHOULD | Integration test: validate cert with wallet offline and fresh revocation cache; verify full access per scope. Simulate stale revocation cache; verify warning flag is surfaced and policy behavior matches configuration. Verify online mode still enforces revocation checks. |
| CERT-005 | Device certificates MUST NOT use wildcard scopes or "admin" scopes that bypass per-operation authorization. Every scope value MUST map to a specific, enumerated operation. The scope enum MUST be maintained in a single source of truth (`matric-crypto::mpc::CertScope`). | MUST | Compile-time: `CertScope` is an enum, not a string. No `CertScope::All` or `CertScope::Admin` variant exists. Code review: verify all scope checks use exhaustive match. Unit test: verify every API operation maps to exactly one `CertScope` variant. |
| CERT-006 | Device certificate issuance MUST require a threshold signing ceremony (2-of-3 MPC). A single device MUST NOT be able to issue certificates for other devices unilaterally. | MUST | Integration test: attempt cert issuance with single share; verify failure. Complete 2-of-3 ceremony; verify cert issued successfully. Verify issued cert's signature validates against the group public key. |
| CERT-007 | The system SHOULD implement certificate transparency: all issued certificates MUST be recorded in an append-only local log. This log SHOULD be periodically anchored to Roko temporal receipts. The log enables detection of unauthorized certificate issuance. | SHOULD | Integration test: issue 3 certificates; verify all 3 appear in transparency log. Verify log is append-only (no deletion API). Verify log entries contain: cert fingerprint, scope, lifetime, issuance timestamp, device ID. If Roko anchoring implemented: verify periodic anchor receipts. |
| CERT-008 | Expired device certificates COULD be retained in a tombstone table for audit purposes (default: 90 days retention). Tombstoned certificates MUST NOT be accepted for any operation. Tombstone records MUST include: cert fingerprint, original scope, issuance time, expiry time, and revocation status at expiry. | COULD | Unit test: verify expired cert is tombstoned. Verify tombstoned cert rejected for all operations. Verify tombstone record contains required fields. Retention test: create tombstone 91 days old; verify cleanup removes it. |

---

## 5. Roko Bridge Security

Requirements for the integration with Roko Network for temporal anchoring and blockchain-verified identity.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| ROKO-001 | Temporal receipt signatures MUST be verified using `ecrecover` to extract the signer address, then validated against the current Roko authority set. Receipts signed by addresses not in the authority set MUST be rejected with `UnknownAuthority` error. | MUST | Unit test: verify valid receipt from known authority passes. Verify receipt from unknown address rejected. Integration test: rotate authority set on test chain; verify old authority receipts rejected after rotation. |
| ROKO-002 | Temporal receipt freshness MUST be enforced: receipts with a block timestamp older than a configurable window (default: 1 hour for attestation anchoring, 24 hours for historical verification) MUST trigger appropriate warnings or rejections based on the operation context. | MUST | Unit test: verify fresh receipt (5 minutes old) accepted. Verify 2-hour receipt triggers warning for attestation anchoring. Verify 25-hour receipt rejected for attestation anchoring but accepted for historical verification. Configuration test: override window; verify new threshold. |
| ROKO-003 | All Roko receipt validation MUST verify the genesis hash embedded in the receipt against the expected Roko chain genesis hash. This prevents cross-chain replay attacks where a receipt from a different Substrate chain (testnet, fork) is presented as a valid Roko receipt. | MUST | Unit test: create receipt with correct genesis hash; verify accepted. Create receipt with different genesis hash; verify rejection with `GenesisHashMismatch` error. Verify genesis hash is loaded from configuration (not hardcoded) to support testnet deployments. |
| ROKO-004 | The system SHOULD verify the Roko authority set against on-chain state before trusting receipts. Authority set verification MUST query the `session` or `aura` pallet for the current validator set. Stale authority set cache (older than 1 epoch) MUST trigger re-fetch. | SHOULD | Integration test: verify authority set is fetched from chain on first receipt verification. Verify stale cache triggers re-fetch. Mock test: simulate authority set rotation mid-epoch; verify receipts from new authority accepted after cache refresh. |
| ROKO-005 | When Roko RPC is unavailable, the system MUST degrade gracefully: all operations that require temporal anchoring MUST proceed without blocking, with the temporal receipt marked as `pending_anchor`. Pending anchors MUST be resolved via background retry with exponential backoff (initial: 5s, max: 5 minutes, max attempts: 20). | MUST | Integration test: disable Roko RPC mock; create trust attestation; verify attestation created with `pending_anchor` status. Re-enable RPC; verify background job resolves anchor within retry window. Verify exponential backoff timing. Verify operations are never blocked by RPC unavailability. |
| ROKO-006 | The Roko RPC client MUST enforce TLS for all connections to Roko endpoints in production. WebSocket connections MUST use `wss://`. HTTP connections MUST use `https://`. Non-TLS endpoints COULD be permitted only when a `ROKO_ALLOW_INSECURE=true` flag is set (development only). | MUST | Unit test: attempt connection to `ws://` endpoint without insecure flag; verify rejection. Verify `wss://` connection accepted. Configuration test: set `ROKO_ALLOW_INSECURE=true`; verify `ws://` accepted. Verify insecure flag is logged with warning at startup. |

---

## 6. API Security

Requirements for the HTTP API endpoints exposed for MPC wallet and trust network operations.

| Req ID | Requirement | Priority | Verification Method |
|--------|-------------|----------|---------------------|
| API-001 | All MPC ceremony endpoints (`/api/v1/mpc/dkg/*`, `/api/v1/mpc/sign/*`) MUST require device authentication via valid device certificate or MPC wallet-level authentication. Unauthenticated requests MUST receive `401 Unauthorized`. These endpoints MUST NOT be accessible with only the existing OAuth2/API key auth (device identity is required). | MUST | Integration test: call DKG endpoint without device cert; verify 401. Call with valid device cert; verify 200. Call with valid OAuth token but no device cert; verify 401. Verify error response does not leak information about which authentication method was missing. |
| API-002 | DKG initiation endpoints MUST be rate-limited to prevent ceremony flooding. Default limit: 5 DKG initiations per wallet per hour. Signing ceremony endpoints MUST be rate-limited to 100 signing requests per device per minute. Rate limit violations MUST return `429 Too Many Requests` with `Retry-After` header. | MUST | Integration test: send 6 DKG initiation requests in 1 minute; verify 6th receives 429. Verify `Retry-After` header is present and contains a reasonable value. Signing test: send 101 signing requests in 1 minute; verify 101st receives 429. Configuration test: override rate limits via env var; verify new limits apply. |
| API-003 | No API response MUST contain secret key material (secret shares, partial signatures, private keys, decrypted ceremony state). API responses for ceremony status MUST contain only: ceremony ID, participant list (device IDs), current round, status (pending/active/complete/failed), and the final group public key (after successful DKG). | MUST | Integration test: run full DKG ceremony via API; capture all response bodies; search for base64/hex patterns matching any secret value generated during the ceremony; verify zero matches. Code review: verify all response serialization types exclude secret fields (using `#[serde(skip)]` or dedicated response DTOs). |
| API-004 | All trust attestation operations (create, revoke, query), device certificate operations (issue, revoke, list), and MPC ceremony operations MUST produce audit log entries. Audit entries MUST include: operation type, actor device ID, target entity, timestamp, and result (success/failure with error code). | MUST | Integration test: perform each operation type; query audit log; verify corresponding entry exists with all required fields. Verify audit log entries are created even for failed operations (with failure reason). Verify audit log is append-only (no mutation or deletion API). |
| API-005 | Trust graph query endpoints (`/api/v1/trust/graph/*`) SHOULD enforce query depth limits (default: max 3 hops) to prevent graph traversal DoS. Queries exceeding the depth limit MUST return a truncated result with a `truncated: true` flag, not an error. | SHOULD | Integration test: create trust graph 5 hops deep; query with default depth limit; verify 3 hops returned with `truncated: true`. Query with explicit `depth=5`; verify 5 hops returned (if within configurable max). Verify `depth=100` is clamped to configurable maximum. |
| API-006 | MPC wallet and trust network API endpoints MUST be gated behind the `mpc_wallet` feature flag. When the feature flag is disabled (default), these endpoints MUST return `404 Not Found` (not `403 Forbidden`, to avoid revealing the feature's existence). | MUST | Integration test (flag disabled): call `/api/v1/mpc/dkg/init`; verify 404. Integration test (flag enabled): call same endpoint with valid auth; verify 200 or appropriate success status. Verify feature flag is checked in router registration, not per-request middleware (zero overhead when disabled). |

---

## Appendix A: Cryptographic Algorithm Inventory

| Purpose | Algorithm | Curve/Params | Crate | Notes |
|---------|-----------|--------------|-------|-------|
| MPC Threshold Signing | FROST (Flexible Round-Optimized Schnorr Threshold) | secp256k1 | `frost-secp256k1` | 2-of-3 default; Roko-compatible |
| Encryption (existing) | ECDH + AES-256-GCM | X25519 | `x25519-dalek`, `aes-gcm` | Unchanged from current PKE system |
| Key Derivation | Argon2id | memory=256MB, t=4, p=4 | `argon2` | Share encryption at rest |
| Passphrase Validation | zxcvbn | score >= 4 | `zxcvbn` | Recovery share passphrase |
| Temporal Receipt Verification | ECDSA | secp256k1 | `k256` | `ecrecover` for Roko receipts |
| Random Number Generation | ChaCha20-based CSPRNG | - | `rand` (OsRng) | All ceremony randomness |

## Appendix B: Threat Model Summary

| Threat | Addressed By |
|--------|--------------|
| Single device compromise reveals full key | MPC-003 (no full key materialization), 2-of-3 threshold |
| Replay of signing messages | MPC-006 (monotonic counters), MPC-007 (equivocation detection) |
| MITM on first key exchange | TRUST-001 (signature verification), out-of-band SAS (R-009) |
| Side-channel on signing device | Constant-time library (R-003), optional HSM (KEY-002) |
| Roko chain fork/replay | ROKO-003 (genesis hash validation) |
| Unauthorized device delegation | CERT-006 (threshold ceremony required for issuance) |
| Ceremony flooding DoS | API-002 (rate limiting on DKG/signing endpoints) |
| Trust graph manipulation | TRUST-005 (no transitive trust), TRUST-007 (replay protection) |
| Key material in logs/responses | KEY-004 (no logging), API-003 (no secret in responses) |
| Recovery share brute force | KEY-005 (128-bit minimum entropy), KEY-001 (Argon2id) |

## Appendix C: Compliance Mapping

| Requirement | OWASP Crypto | NIST SP 800-57 | CIS Controls |
|-------------|--------------|----------------|--------------|
| MPC-003 (no full key) | Crypto-003 | 5.1 (key protection) | 3.10 |
| KEY-001 (encryption at rest) | Crypto-001 | 6.2 (storage security) | 3.11 |
| KEY-004 (no logging) | Crypto-007 | 6.2.6 (key compromise) | 8.3 |
| CERT-001 (short-lived certs) | Auth-005 | 5.2 (key period) | 6.4 |
| API-002 (rate limiting) | - | - | 13.1 |
| ROKO-006 (TLS enforcement) | Crypto-004 | 5.2.3 (transport) | 3.10 |
