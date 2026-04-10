# User Stories: MPC Wallet & Personal Trust Network

**Document ID**: US-MPC-001
**Version**: 0.1
**Status**: Draft
**Date**: 2026-04-09
**Related**: VIS-MPC-001 (Vision Document)

---

## Epic 1: MPC Key Management

### US-1.1: Distributed Identity Generation

**Title**: Generate distributed identity via MPC DKG

**Story**: As a user, I want to generate a distributed identity across my devices so that no single device holds my full private key.

**Acceptance Criteria**:
- DKG ceremony completes across 3 enrolled devices producing 3 threshold shares and 1 aggregate public key.
- The aggregate public key is deterministic: re-running DKG with the same randomness produces the same result.
- No device ever holds or computes the full private key at any point during the ceremony.
- The resulting identity is expressed as an `mw:...` address derived from the aggregate public key.
- DKG ceremony fails cleanly if fewer than 3 devices are available, with actionable error message.

**Priority**: Must (P0)
**Complexity**: XL
**Traces To**: F-01

---

### US-1.2: Threshold Signing

**Title**: Sign operations with threshold quorum

**Story**: As a user, I want to threshold-sign operations from any 2 of my 3 devices so that I can use my identity without needing all devices online.

**Acceptance Criteria**:
- Any combination of 2 out of 3 enrolled devices can produce a valid signature.
- The produced signature is a standard ECDSA (secp256k1) or EdDSA (Ed25519) signature verifiable with the aggregate public key.
- Signing completes in under 500ms on LAN and under 2 seconds on WAN.
- If a device goes offline mid-ceremony, the signing round aborts cleanly without leaking nonce state.
- The non-participating device cannot derive any information about the signature from observing the result.

**Priority**: Must (P0)
**Complexity**: XL
**Traces To**: F-02

---

### US-1.3: Key Resharing

**Title**: Reshare key to new device set

**Story**: As a user, I want to reshare my key to new devices without changing my public identity so that I can add, remove, or replace devices over time.

**Acceptance Criteria**:
- After resharing, the aggregate public key and `mw:...` address remain unchanged.
- Old shares are invalidated and cannot participate in future signing ceremonies.
- Resharing requires a threshold quorum (2-of-3) of current shareholders to authorize.
- New shares are zeroized in transit (encrypted end-to-end between devices).
- The resharing ceremony completes in under 10 seconds on LAN.

**Priority**: Should (P1)
**Complexity**: L
**Traces To**: F-09

---

### US-1.4: Identity Recovery

**Title**: Recover identity after device loss

**Story**: As a user, I want to recover my identity if I lose devices below threshold so that a catastrophic device failure does not permanently destroy my cryptographic identity.

**Acceptance Criteria**:
- User can configure k-of-n social recovery custodians (e.g., 3-of-5 trusted peers).
- Each custodian holds an encrypted recovery share that is useless in isolation.
- Recovery ceremony requires k custodians to participate simultaneously (no sequential assembly).
- Recovery ceremony includes a configurable time-lock (minimum 24 hours) to prevent instant custodian collusion.
- After recovery, user receives new threshold shares on new devices; old shares from lost devices cannot sign.

**Priority**: Should (P1)
**Complexity**: XL
**Traces To**: F-08

---

## Epic 2: Trust Attestations

### US-2.1: Trust a Peer

**Title**: Create trust attestation for another user

**Story**: As a user, I want to trust another user's wallet so that I can securely communicate with them and include them in encrypted content sharing.

**Acceptance Criteria**:
- Trust attestation contains: truster address, trustee address, trust level, scope, and creation timestamp.
- Attestation is threshold-signed by the truster's MPC wallet (2-of-3 quorum required).
- Trust level is one of: `contact`, `trusted`, `verified` (user-selectable, defaults to `contact`).
- Attestation is persisted locally and queued for Roko temporal anchoring.
- Creating a trust attestation for an already-trusted peer updates the existing attestation (idempotent).

**Priority**: Must (P0)
**Complexity**: M
**Traces To**: F-03

---

### US-2.2: Revoke Trust

**Title**: Revoke trust in a compromised peer

**Story**: As a user, I want to revoke trust in a peer whose wallet was compromised so that they can no longer access content encrypted to my trust set.

**Acceptance Criteria**:
- Revocation attestation is threshold-signed and references the original trust attestation by ID.
- Revocation takes effect immediately in the local trust store (no propagation delay for local enforcement).
- Revocation is submitted to Roko for temporal anchoring (proving when revocation occurred).
- After revocation, new encryption operations exclude the revoked peer; existing encrypted content remains accessible to them (no retroactive revocation).
- Revoking trust for a peer who is not currently trusted returns a clear "not found" error.

**Priority**: Must (P0)
**Complexity**: M
**Traces To**: F-03

---

### US-2.3: Temporally Anchored Trust

**Title**: Timestamp trust decisions via Roko consensus

**Story**: As a user, I want my trust decisions timestamped by Roko consensus so that there is tamper-evident proof of when I trusted or revoked a peer.

**Acceptance Criteria**:
- Each trust attestation and revocation receives a Roko temporal receipt with nanosecond precision.
- The receipt includes: PoAT timestamp, validator set signatures, and block hash reference.
- If Roko is unreachable, the attestation is stored locally with "pending anchor" status and retried.
- Anchored attestations display both local creation time and Roko consensus time.
- Receipt verification is available via API: given an attestation and receipt, return validity and timestamp.

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-05

---

### US-2.4: Trust Graph Exploration

**Title**: View trust relationships

**Story**: As a user, I want to see who trusts me and who I trust so that I can understand and manage my trust network.

**Acceptance Criteria**:
- API returns direct trusters (who trusts me) and direct trustees (who I trust) with trust levels and timestamps.
- Trust graph supports transitive path queries up to configurable depth (default: 3 hops).
- Mutual trust detection: flag peers where trust is bidirectional.
- Query response time under 50ms for direct trust; under 200ms for 3-hop transitive queries.
- Trust graph data is scoped to the active memory archive (respects multi-memory isolation).

**Priority**: Should (P1)
**Complexity**: M
**Traces To**: F-11

---

## Epic 3: Device Certificates

### US-3.1: Commission New Device

**Title**: Issue device certificate via MPC wallet

**Story**: As a user, I want to commission a new device with a short-lived certificate signed by my MPC wallet so that the device can perform operations on my behalf without holding MPC shares.

**Acceptance Criteria**:
- Device cert binds: device public key, user MPC identity (aggregate public key), validity period, and permitted scopes.
- Cert is signed by threshold quorum (2-of-3 MPC devices) and includes the MPC aggregate public key for chain verification.
- Default validity period is 30 days (configurable between 1 hour and 365 days).
- Cert includes scope constraints: which operations the device may perform (e.g., `sign`, `encrypt`, `attest`).
- Cert issuance is logged as an attestation with Roko temporal anchoring.

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-04

---

### US-3.2: Auto-Renew Device Certificate

**Title**: Automatic device certificate renewal

**Story**: As a user, I want device certificates to auto-renew without a full MPC ceremony so that my devices remain operational without manual intervention.

**Acceptance Criteria**:
- Renewal triggers automatically when a cert reaches 80% of its validity period.
- Renewal uses the existing cert to request a new one, which is then signed by MPC threshold quorum.
- If MPC quorum is unavailable at renewal time, retry with exponential backoff up to cert expiry.
- Renewal logs are visible in the device management UI/API.
- A cert that has been explicitly revoked cannot be renewed (revocation is permanent for that cert).

**Priority**: Should (P1)
**Complexity**: M
**Traces To**: F-10

---

### US-3.3: Revoke Device Certificate

**Title**: Immediate device certificate revocation

**Story**: As a user, I want to revoke a device certificate immediately if the device is compromised so that the attacker cannot use the stolen device to act as me.

**Acceptance Criteria**:
- Revocation takes effect in the local trust store within 1 second.
- Revocation is threshold-signed and submitted to Roko for temporal anchoring.
- Any operation signed by a revoked device cert after the revocation timestamp is rejected.
- Revocation CRL (Certificate Revocation List) is queryable via API for remote verification.
- Revocation of a device cert does not affect other device certs or the MPC identity itself.

**Priority**: Must (P0)
**Complexity**: M
**Traces To**: F-04

---

## Epic 4: Roko Temporal Bridge

### US-4.1: Verify Temporal Receipts

**Title**: Verify Roko temporal receipts in Fortemi

**Story**: As a developer, I want to verify Roko temporal receipts in Fortemi so that I can validate the timing of trust attestations and other anchored events.

**Acceptance Criteria**:
- API accepts a Roko receipt and returns: validity (boolean), PoAT timestamp, validator set that signed it, and block reference.
- Verification checks: receipt structure, validator signatures against known validator set, timestamp bounds (not in the future, not before genesis).
- Invalid receipts return a specific error code indicating the failure reason (bad signature, unknown validator, malformed receipt).
- Verification operates locally against a cached validator set (no Roko RPC call required for each verification).
- Validator set cache refreshes on configurable interval (default: every 100 blocks).

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-05, F-12

---

### US-4.2: Anchor Trust to Consensus Time

**Title**: Trust attestations carry Roko consensus timestamps

**Story**: As a user, I want my trust attestations anchored to Roko consensus time so that the temporal ordering of my trust decisions is independently verifiable.

**Acceptance Criteria**:
- Each trust attestation submitted to Roko receives a temporal receipt within one block confirmation (target: under 6 seconds).
- The attestation stores both the local creation timestamp and the Roko consensus timestamp.
- The consensus timestamp is the authoritative time for trust ordering disputes.
- Attestations are batched where possible (multiple attestations in one Roko submission) to reduce on-chain cost.
- Failed submissions are retried with the attestation marked as "unanchored" until successful.

**Priority**: Must (P0)
**Complexity**: M
**Traces To**: F-05

---

### US-4.3: Validator Device Delegation

**Title**: Delegate temporal receipt signing to device key

**Story**: As a validator, I want my device key to sign temporal receipts delegated from my MPC wallet so that my validator node operates autonomously without holding my root identity shares.

**Acceptance Criteria**:
- Validator device cert includes the `temporal_receipt_signing` scope.
- Roko Network accepts temporal receipts signed by a device key when accompanied by a valid device cert chain (device cert -> MPC aggregate key).
- Device key rotation does not require validator re-registration with Roko Network (the MPC identity is stable).
- Device cert expiry during active validation triggers a warning 7 days before expiry.
- Validator can operate with a device cert independently of MPC share availability (cert is self-contained).

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-04, F-05

---

## Epic 5: Crypto Operations

### US-5.1: Encrypt to Trusted Peers

**Title**: Encrypt notes to trusted peers using wallet addresses

**Story**: As a user, I want to encrypt notes to trusted peers using their wallet addresses so that only peers I have explicitly trusted can read my shared content.

**Acceptance Criteria**:
- Encryption accepts a list of `mw:...` (MPC wallet) or `mm:...` (legacy PKE) addresses as recipients.
- For MPC addresses, encryption uses the X25519 key derived from the recipient's aggregate public key.
- Encryption fails with a clear error if any recipient is not in the sender's trust store (trust-gated encryption).
- Encrypted content is compatible with the existing MMPKE01 format (extended header for MPC metadata).
- Decryption by a recipient with an MPC wallet uses the X25519 key derived from their threshold shares (no full key materialization).

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-06, F-07

---

### US-5.2: Threshold-Sign Data

**Title**: Sign arbitrary data with MPC identity

**Story**: As a user, I want to sign data with my identity using threshold signing so that others can verify I authored or approved content without any single device being a point of compromise.

**Acceptance Criteria**:
- Signing accepts arbitrary byte payloads and produces a standard signature (ECDSA for secp256k1, EdDSA for Ed25519).
- The signature is verifiable using the user's aggregate public key (standard verification, no threshold-aware verifier needed).
- User selects which curve to sign with based on use case (secp256k1 for Roko operations, Ed25519 for general purpose).
- Signing requires 2-of-3 device quorum and fails cleanly if quorum is unavailable.
- Signature metadata includes: signer `mw:...` address, curve identifier, and timestamp.

**Priority**: Must (P0)
**Complexity**: M
**Traces To**: F-02, F-06

---

### US-5.3: Unified Multi-Curve API

**Title**: Single API surface for multi-curve operations

**Story**: As a developer, I want a unified API for multi-curve operations so that I can integrate signing, encryption, and verification without managing curve-specific logic in application code.

**Acceptance Criteria**:
- Single `CryptoOps` trait with methods: `sign(payload, curve) -> Signature`, `verify(payload, signature, pubkey) -> bool`, `encrypt(payload, recipients) -> Ciphertext`, `decrypt(ciphertext, identity) -> Payload`.
- Curve selection via enum: `Curve::Secp256k1`, `Curve::Ed25519`, `Curve::X25519` (encryption only).
- Error types are curve-agnostic: `SigningError`, `VerificationError`, `EncryptionError` with curve-specific details in error context.
- API documentation includes code examples for each curve and operation combination.
- The API handles MPC wallet identities and legacy PKE keys transparently (caller does not need to know the key type).

**Priority**: Must (P0)
**Complexity**: L
**Traces To**: F-06

---

### US-5.4: Import Legacy PKE Keys

**Title**: Import existing PKE keypair into MPC wallet context

**Story**: As a user, I want to import my existing PKE keypair into the MPC wallet context so that I can continue decrypting content encrypted to my old `mm:...` address while transitioning to my new `mw:...` identity.

**Acceptance Criteria**:
- Import accepts an existing PKE private key (from file or keyset) and registers it as a non-threshold fallback within the MPC wallet.
- The imported key retains its `mm:...` address and remains functional for decrypting existing MMPKE01 content.
- New trust attestations reference the `mw:...` identity, not the legacy `mm:...` address.
- The imported PKE key is stored with the same encryption-at-rest protections as MPC shares (Argon2id + AES-256-GCM).
- Import is a one-time operation per PKE key; re-importing the same key is idempotent.

**Priority**: Must (P0)
**Complexity**: S
**Traces To**: F-07

---

### US-5.5: Delegated Intermediate Certificates

**Title**: Device certs issue sub-certificates for automation

**Story**: As a developer, I want device certificates to issue further short-lived sub-certificates for automated processes so that CI/CD pipelines and MCP servers can sign operations without human-interactive MPC ceremonies.

**Acceptance Criteria**:
- A device cert with `delegate` scope can issue sub-certificates with validity no longer than the parent cert's remaining validity.
- Sub-certificates carry a chain: MPC root -> device cert -> process cert, verifiable by any party holding the MPC aggregate public key.
- Sub-certificate scope is a subset of the parent device cert's scope (no privilege escalation).
- Sub-certificates are revocable independently of the parent device cert.
- Maximum delegation depth is 2 (MPC -> device -> process); deeper chains are rejected.

**Priority**: Could (P2)
**Complexity**: L
**Traces To**: F-15

---

## Story Summary

| Epic | Story Count | Must | Should | Could |
|------|-------------|------|--------|-------|
| 1: MPC Key Management | 4 | 2 | 2 | 0 |
| 2: Trust Attestations | 4 | 3 | 1 | 0 |
| 3: Device Certificates | 3 | 2 | 1 | 0 |
| 4: Roko Temporal Bridge | 3 | 3 | 0 | 0 |
| 5: Crypto Operations | 5 | 4 | 0 | 1 |
| **Total** | **19** | **14** | **4** | **1** |

## Complexity Distribution

| Complexity | Count | Stories |
|------------|-------|---------|
| S | 1 | US-5.4 |
| M | 7 | US-2.1, US-2.2, US-3.2, US-3.3, US-4.2, US-5.2, US-5.3 |
| L | 7 | US-1.3, US-2.3, US-3.1, US-4.1, US-4.3, US-5.1, US-5.5 |
| XL | 4 | US-1.1, US-1.2, US-1.4, (none remaining) |

## Traceability Matrix

| Story | Feature IDs | Vision Section |
|-------|-------------|----------------|
| US-1.1 | F-01 | 7.1 |
| US-1.2 | F-02 | 7.1 |
| US-1.3 | F-09 | 7.2 |
| US-1.4 | F-08 | 7.2 |
| US-2.1 | F-03 | 7.1 |
| US-2.2 | F-03 | 7.1 |
| US-2.3 | F-05 | 7.1 |
| US-2.4 | F-11 | 7.2 |
| US-3.1 | F-04 | 7.1 |
| US-3.2 | F-10 | 7.2 |
| US-3.3 | F-04 | 7.1 |
| US-4.1 | F-05, F-12 | 7.1, 7.2 |
| US-4.2 | F-05 | 7.1 |
| US-4.3 | F-04, F-05 | 7.1 |
| US-5.1 | F-06, F-07 | 7.1 |
| US-5.2 | F-02, F-06 | 7.1 |
| US-5.3 | F-06 | 7.1 |
| US-5.4 | F-07 | 7.1 |
| US-5.5 | F-15 | 7.3 |
