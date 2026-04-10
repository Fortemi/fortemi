# Vision Document: MPC Wallet & Personal Trust Network

**Document ID**: VIS-MPC-001
**Version**: 0.1
**Status**: Draft
**Date**: 2026-04-09
**Author**: Architecture Team

---

## 1. Reasoning

### 1.1 Need Justification

Fortemi's existing PKE wallet (X25519/AES-256-GCM, `mm:...` addresses) provides solid pairwise encryption, but it operates within a single-device, single-curve trust model. Users have no way to:

- Distribute key material across multiple devices to reduce blast radius of a single compromise.
- Express trust relationships between wallets in a way that is auditable and temporally grounded.
- Delegate signing authority to devices without exposing root key material.
- Anchor trust decisions to an objective timeline independent of any single server's clock.

Roko Network's PoAT (Proof of Accurate Time) consensus already produces nanosecond-precision temporal receipts using ECDSA secp256k1. Bridging Fortemi's identity layer to Roko creates a trust substrate where every attestation, revocation, and delegation carries cryptographic proof of when it happened, not merely when some server recorded it.

### 1.2 Stakeholder Impact

| Stakeholder | Impact | Benefit |
|-------------|--------|---------|
| Fortemi users | High | Self-sovereign identity; no single point of key compromise; peer trust without institutional intermediaries |
| Roko validators | Medium | Temporal anchoring demand increases network utility and receipt volume |
| Developers | High | Unified multi-curve API replaces ad-hoc key management; trust graph queries enable new application patterns |
| Operators | Medium | Device cert lifecycle replaces static API keys; revocation is instant |

### 1.3 Feasibility

- **Cryptographic**: FROST (Flexible Round-Optimized Schnorr Threshold) is production-tested for secp256k1 (ZCash Foundation implementation). X25519 and Ed25519 share the Curve25519 field, enabling key derivation from a common seed without separate DKG ceremonies.
- **Architectural**: The existing `matric-crypto` crate already isolates all cryptographic operations. The MPC wallet extends this crate with a new `mpc` module alongside the existing `pke` module.
- **Operational**: 2-of-3 threshold signing requires only two devices online. LAN latency for the two FROST rounds is sub-100ms. WAN latency is sub-second even across continents.
- **Backward Compatibility**: Existing `mm:...` addresses and MMPKE01 format remain fully functional. MPC wallets introduce a new address prefix (`mw:...`) and coexist with PKE keys.

### 1.4 Priority

**High**. This feature is foundational to Fortemi's evolution from a personal knowledge base to a self-sovereign knowledge network. Trust attestations gate future features: collaborative memories, cross-archive sharing, and verifiable provenance chains.

### 1.5 Verification

- All cryptographic operations must pass test vectors from the FROST RFC (draft-irtf-cfrg-frost).
- Threshold signing correctness verified by round-trip: sign with 2-of-3, verify with aggregate public key.
- Temporal anchoring verified by Roko receipt validation (signature check + timestamp bounds).
- Device cert delegation verified by chain validation: MPC root -> device cert -> operation signature.
- Zeroization verified by memory scanning tests (no key material in process memory after drop).

---

## 2. Product Overview

The MPC Wallet & Personal Trust Network is a **personal internet identity layer** built on threshold cryptography and decentralized temporal anchoring. Each Fortemi user becomes their own trust root -- a self-sovereign certificate authority whose signing key never exists in one place. Trust between users is expressed through wallet-signed attestations anchored to Roko consensus time, creating an auditable, decentralized web of trust that requires no institutional PKI, no certificate authorities, and no committee governance.

The system operates edge-first: users control their own trust stores, their own device delegations, and their own revocation decisions. No central server can revoke trust, issue certificates, or override user decisions. The only shared infrastructure is the Roko temporal anchor, which provides objective ordering without authority.

---

## 3. Problem Statement

### 3.1 Centralized Trust Authorities

Traditional PKI (X.509, TLS CAs) concentrates trust in a small number of institutional certificate authorities. A single CA compromise (DigiNotar 2011, Symantec 2017) can undermine trust for millions of users. Users cannot choose whom they trust; browser vendors make that decision. For a personal knowledge base handling private, sensitive data, delegating trust to institutions the user has no relationship with is architecturally wrong.

### 3.2 Key Compromise Blast Radius

Fortemi's current PKE model stores the full private key on a single device. If that device is compromised, the attacker gains the user's complete cryptographic identity: they can decrypt all past messages (no forward secrecy at the identity level), impersonate the user, and sign fraudulent data. There is no partial compromise -- it is all or nothing.

### 3.3 No Temporal Grounding for Trust Decisions

Trust relationships change over time: a user might trust a peer today and revoke tomorrow. Without an objective, tamper-evident timeline, there is no way to distinguish between "Alice trusted Bob at time T" and "Alice trusts Bob now." Existing systems use server timestamps, which are trivially forgeable. Roko's PoAT consensus provides nanosecond-precision temporal receipts signed by validator sets, creating an objective ordering that no single party can manipulate.

### 3.4 Static Device Authorization

Current API keys and long-lived tokens provide no cryptographic delegation chain. If a device token is stolen, there is no way to prove it was issued by the user's identity, when it was issued, or whether it has been revoked. Short-lived device certificates signed by the user's MPC wallet solve all three problems.

---

## 4. Stakeholder Summary

### 4.1 Fortemi Users (Primary)

Individual users managing personal knowledge who need:
- A distributed identity that survives device loss.
- The ability to selectively trust peers for encrypted communication and shared memories.
- Device management without manual key rotation ceremonies.
- Full sovereignty over trust decisions with no external dependency.

### 4.2 Roko Validators

Network participants who maintain PoAT consensus and need:
- Demand for temporal receipts to justify validator economics.
- Clean integration between wallet-signed attestations and receipt issuance.
- Device key delegation so validator nodes can sign temporal receipts without holding the operator's root MPC shares.

### 4.3 Platform Developers

Engineers building on Fortemi and Roko who need:
- A unified API for multi-curve cryptographic operations (secp256k1, X25519, Ed25519).
- Trust graph query primitives for building social features, access control, and provenance verification.
- Clear documentation of the MPC ceremony protocol for client implementations.

---

## 5. Product Positioning

### 5.1 vs. PGP Web of Trust

PGP's web of trust (1991) was the first attempt at decentralized identity attestation. It failed for three reasons: key management was impossibly complex for normal users, there was no temporal grounding (signatures carried no verifiable timestamp), and the trust model was binary (trusted/not trusted) with no revocation propagation. The MPC wallet addresses all three: threshold signing eliminates single-key management burden, Roko anchoring provides temporal grounding, and the trust graph supports graduated trust levels with instant revocation.

### 5.2 vs. DID/VC (W3C Decentralized Identifiers)

DID and Verifiable Credentials are committee-driven standards with broad scope but slow iteration. DID methods typically depend on specific blockchains or registries (did:ethr, did:ion, did:web), creating implicit centralization. The DID specification is 104 pages; the VC specification is 82 pages. The MPC wallet takes a narrower, more opinionated approach: one trust model, one temporal anchor, one threshold scheme. Interoperability with DID methods is possible (the MPC wallet's aggregate public key can be expressed as a DID document) but is not a design goal.

### 5.3 vs. Hyperledger Besu / Consortium Chains

Consortium-governed identity systems (Hyperledger Indy, Sovrin) require institutional membership and governance committees to operate. They are designed for enterprise use cases where organizations need to agree on identity schemas. Fortemi's trust model is fundamentally individual: each user is their own root CA, and trust is peer-to-peer. There is no consortium to join, no governance committee to petition, and no schema to comply with.

### 5.4 vs. Apple/Google Passkeys (FIDO2/WebAuthn)

Passkeys solve authentication but not identity attestation. They are device-bound credentials managed by platform vendors (iCloud Keychain, Google Password Manager), creating implicit trust in Apple and Google. They cannot express "I trust this peer's wallet" or anchor trust decisions to an independent timeline. The MPC wallet is complementary -- passkeys could serve as a device-level authentication factor that gates access to an MPC share.

---

## 6. User Summary

### 6.1 Individual User

**Profile**: A person who stores personal notes, documents, and media in Fortemi and wants to selectively share encrypted content with trusted peers.

**Goals**: Generate a distributed identity across their devices. Trust and be trusted by peers without institutional mediation. Encrypt notes to specific recipients. Survive device loss without identity loss.

**Technical Comfort**: Moderate. Expects wallet-like UX (addresses, trust/untrust actions). Does not want to understand threshold cryptography.

### 6.2 Validator Operator

**Profile**: A technically proficient user running a Roko Network validator node who needs to delegate signing authority from their identity to their validator hardware.

**Goals**: Issue device certificates to validator nodes. Ensure temporal receipts are attributable to their identity. Rotate validator keys without changing their public identity.

**Technical Comfort**: High. Comfortable with key management, CLI tools, and network operations.

### 6.3 Platform Developer

**Profile**: A developer building applications on Fortemi's API who needs programmatic access to trust and signing operations.

**Goals**: Integrate trust verification into application logic. Query the trust graph for access control decisions. Use a single API surface for all cryptographic operations regardless of curve.

**Technical Comfort**: High. Expects well-documented APIs, clear error types, and test utilities.

---

## 7. Product Features

### 7.1 Must Have (P0)

| ID | Feature | Description |
|----|---------|-------------|
| F-01 | MPC Distributed Key Generation | FROST-based 2-of-3 DKG ceremony producing threshold shares distributed across user devices. No full private key materialization at any point. Generates aggregate public key as user identity. |
| F-02 | Threshold Signing (FROST) | 2-of-3 FROST threshold signing for secp256k1 (Roko-compatible) and Ed25519 (Fortemi internal). Two online devices produce a valid signature indistinguishable from a single-signer signature. |
| F-03 | Trust Attestations | Wallet-signed statements of trust between users. Structure: `{truster, trustee, level, scope, timestamp}`. Signed by truster's MPC wallet. Includes revocation attestations. |
| F-04 | Device Certificate Issuance | MPC wallet acts as personal CA. Issues short-lived X.509-style certificates to device keys. Certificate binds device public key to user identity with expiry and scope constraints. |
| F-05 | Roko Temporal Anchoring | Every trust attestation and revocation is submitted to Roko Network for temporal receipt. Receipt includes PoAT nanosecond timestamp, validator signatures, and block reference. |
| F-06 | Multi-Curve Key Derivation | From MPC seed: derive secp256k1 (Roko signing), X25519 (Fortemi PKE encryption), and Ed25519 (general-purpose signing). Single DKG ceremony, multiple curve-specific keys. |
| F-07 | PKE Backward Compatibility | Existing `mm:...` addresses and MMPKE01 encrypted blobs continue to work. MPC wallet can import existing PKE keypairs as a non-threshold fallback. |

### 7.2 Should Have (P1)

| ID | Feature | Description |
|----|---------|-------------|
| F-08 | Social Recovery | k-of-n recovery using trusted peers as share custodians. If user loses devices below threshold, trusted peers can participate in a recovery ceremony to reconstitute shares on new devices. |
| F-09 | Key Resharing | Proactive resharing protocol: redistribute shares to a new set of devices without changing the aggregate public key. Handles device addition, removal, and periodic rotation. |
| F-10 | Device Cert Auto-Renewal | Device certificates that are approaching expiry are automatically renewed via a lightweight MPC signing round, without requiring user interaction beyond having two devices online. |
| F-11 | Trust Graph Queries | API endpoints for traversing the trust graph: direct trusters/trustees, transitive trust paths (up to configurable depth), mutual trust detection, trust clustering. |
| F-12 | Attestation Verification API | Given an attestation and a Roko receipt, verify the full chain: MPC aggregate key -> attestation signature -> Roko temporal receipt -> validator set signature. |

### 7.3 Could Have (P2)

| ID | Feature | Description |
|----|---------|-------------|
| F-13 | Sr25519 Support | Schnorr signature scheme on Ristretto255 (used by Substrate's BABE consensus). Enables direct participation in Substrate runtime without key type conversion. |
| F-14 | Cross-Memory Trust Isolation | Per-memory trust stores that allow different trust configurations per Fortemi archive. A user might trust different peers in their work archive vs. personal archive. |
| F-15 | Delegated Intermediate CA | Device certificates can issue further short-lived certificates for automated processes (cron jobs, CI/CD, MCP servers) without requiring an MPC ceremony. Chain: MPC root -> device cert -> process cert. |
| F-16 | Trust Level Semantics | Graduated trust levels (e.g., `contact`, `trusted`, `verified`, `delegated`) with configurable permissions per level. Level semantics are user-defined, not protocol-enforced. |
| F-17 | Offline Signing | Pre-computed nonce commitments that allow one device to prepare a partial signature offline, to be completed when connectivity is restored. |

---

## 8. Constraints

### 8.1 Cryptographic Constraints

- **No full key materialization**: At no point during DKG, signing, resharing, or recovery may the full private key exist in a single memory space. This is a hard security invariant, not an optimization.
- **FROST 2-round latency**: FROST threshold signing requires two communication rounds (commitment, then signature share). Roko block production must tolerate this latency. Target: sub-500ms for LAN, sub-2s for WAN.
- **Zeroization**: All key material (shares, nonces, partial signatures) must be zeroized on drop. The existing `ZeroizeOnDrop` pattern in `matric-crypto` is the baseline.

### 8.2 Compatibility Constraints

- **PKE backward compatibility**: The `mm:...` address scheme, MMPKE01 file format, and all existing encrypted content must remain fully functional. MPC wallet addresses use a distinct `mw:...` prefix.
- **Roko secp256k1**: Roko Network uses ECDSA secp256k1 for all on-chain operations. The MPC wallet must produce standard ECDSA signatures that Roko validators accept without protocol changes.
- **Existing keyset management**: The `pke_keysets` database table and API endpoints continue to manage single-key PKE. MPC wallets get a parallel management path.

### 8.3 Operational Constraints

- **Edge-first**: All trust decisions, cert issuance, and revocations are user-initiated. No server-side trust policy or automated revocation.
- **No phone-home**: MPC ceremonies operate peer-to-peer between user devices. No Fortemi server participates in DKG, signing, or resharing.
- **Device minimums**: 2-of-3 threshold means users need at least 3 devices for full redundancy. Minimum viable operation requires 2 devices (with reduced fault tolerance).

---

## 9. Dependencies

### 9.1 External Dependencies

| Dependency | Purpose | Status |
|------------|---------|--------|
| `frost-secp256k1-tr` (ZCash Foundation) | FROST DKG and threshold signing for secp256k1 | Stable, audited |
| `frost-ed25519` (ZCash Foundation) | FROST for Ed25519 signing | Stable |
| Roko Network RPC | Temporal receipt submission and verification | In development |
| `x25519-dalek` | X25519 key agreement (existing dependency) | Stable, in use |
| `k256` | secp256k1 scalar and point arithmetic | Stable |
| `ed25519-dalek` | Ed25519 signatures | Stable |

### 9.2 Internal Dependencies

| Dependency | Purpose | Status |
|------------|---------|--------|
| `matric-crypto` crate | Host crate for new `mpc` module | Existing |
| `matric-db` crate | Persistence for MPC shares, trust attestations, device certs | Requires new tables |
| `matric-api` crate | HTTP endpoints for MPC ceremonies, trust operations, cert management | Requires new route group |
| PKE wallet (`pke` module) | Backward-compatible encryption; X25519 key derivation target | Existing |
| Multi-memory architecture | Per-archive trust isolation (P2 feature) | Existing |

---

## 10. Quality Attributes

### 10.1 Crypto Correctness

- All FROST operations must pass the reference test vectors from the IETF FROST draft (draft-irtf-cfrg-frost-15).
- Threshold signatures must be indistinguishable from single-signer signatures under standard ECDSA/EdDSA verification.
- Key derivation from MPC seed to curve-specific keys must be deterministic and reproducible across implementations.

### 10.2 Forward Secrecy

- Per-session ephemeral keys for encryption operations (inherited from existing PKE design).
- Device cert revocation immediately invalidates all future operations; past operations signed by the cert remain verifiable (non-repudiation preserved).

### 10.3 Zeroization

- All MPC shares, nonces, partial signatures, and derived key material implement `ZeroizeOnDrop`.
- Memory scanning tests verify no key material remains in process memory after struct deallocation.
- Serialized shares at rest are encrypted with the same Argon2id + AES-256-GCM scheme used for PKE private keys.

### 10.4 Performance

- **DKG ceremony**: Target under 5 seconds for 2-of-3 on LAN (one-time operation).
- **Threshold signing**: Target under 500ms for 2-of-3 on LAN (two FROST rounds).
- **Trust attestation creation**: Target under 1 second including Roko receipt submission.
- **Device cert issuance**: Target under 2 seconds including MPC signing round.
- **Trust graph query** (direct trust, depth-1): Target under 50ms from database.

### 10.5 Auditability

- Every trust state change (attestation, revocation, cert issuance, cert revocation) produces an immutable log entry with Roko temporal receipt.
- Trust graph state at any historical point is reconstructable from the attestation log.

---

## 11. Risks

### 11.1 MPC Implementation Bugs

**Severity**: Critical
**Probability**: Medium
**Description**: Threshold cryptography implementations are notoriously difficult to get right. Bugs in nonce generation, share validation, or signature aggregation can leak key material or produce forgeable signatures.
**Mitigation**: Use the ZCash Foundation FROST libraries exclusively (audited, test-vectored). No custom threshold crypto. Fuzz testing on all ceremony paths. External audit before production use.

### 11.2 Multi-Curve Maintenance Burden

**Severity**: High
**Probability**: High
**Description**: Supporting secp256k1, X25519, Ed25519, and potentially Sr25519 quadruples the surface area for cryptographic bugs, dependency updates, and security advisories.
**Mitigation**: Abstract curve-specific operations behind a trait interface. Pin all crypto dependencies to audited versions. Subscribe to RustSec advisories for all curve crates. Limit Sr25519 to P2 (Could Have) until demand is demonstrated.

### 11.3 Trust Graph Fragmentation

**Severity**: Medium
**Probability**: High
**Description**: If trust attestations are purely peer-to-peer with no bootstrap mechanism, new users face a cold-start problem: no one trusts them, and they trust no one. The trust graph fragments into disconnected islands.
**Mitigation**: Provide optional trust bootstrap mechanisms: QR code trust exchange for in-person meetings, mutual trust via shared memory archives, and Roko validator endorsement for known-good identities. Do not mandate any bootstrap mechanism -- keep it user-choice.

### 11.4 First-Contact Problem

**Severity**: Medium
**Probability**: High
**Description**: When two users want to establish trust for the first time, they need a secure channel to exchange wallet addresses. This is the same bootstrap problem that plagues PGP key exchange.
**Mitigation**: Multiple first-contact channels: in-app QR scanning, verified Fortemi profile pages, Roko on-chain identity registration, and out-of-band verification (voice call, physical meeting). The system does not solve first-contact -- it provides tools for users to solve it in whatever way they find natural.

### 11.5 Recovery Scenarios

**Severity**: High
**Probability**: Medium
**Description**: If a user loses 2 of 3 devices (below threshold), they lose their identity permanently unless social recovery is implemented. Social recovery introduces its own risks: collusion among recovery custodians, custodian device loss, and recovery ceremony complexity.
**Mitigation**: Social recovery (F-08) is P1 priority. Design the recovery protocol to require k-of-n custodians (configurable by user) with a time-locked recovery ceremony (prevents instant custodian collusion). Provide clear UX guidance on custodian selection. Document the "identity death" scenario honestly for users who choose not to configure recovery.

### 11.6 Roko Network Availability

**Severity**: Medium
**Probability**: Low
**Description**: If Roko Network is unreachable, trust attestations cannot be temporally anchored. Users can still sign and encrypt, but attestations lack temporal proof.
**Mitigation**: Queue attestations locally when Roko is unreachable. Submit when connectivity returns. Local attestations are valid but carry a "pending temporal anchor" status. Applications that require temporal proof can reject unanchored attestations.

### 11.7 Device Synchronization Complexity

**Severity**: Medium
**Probability**: Medium
**Description**: MPC ceremonies require real-time communication between devices. Users with devices across networks (phone on cellular, laptop on VPN, desktop on LAN) may face NAT traversal and connectivity issues.
**Mitigation**: Implement a lightweight relay protocol for MPC message passing (encrypted, no key material visible to relay). Relay can be Fortemi server or any TURN-like infrastructure. All MPC messages are end-to-end encrypted between devices; the relay sees only ciphertext.

---

## Appendix A: Glossary

| Term | Definition |
|------|-----------|
| DKG | Distributed Key Generation -- protocol where multiple parties collaboratively generate a shared key without any single party learning the full secret |
| FROST | Flexible Round-Optimized Schnorr Threshold signatures -- a two-round threshold signing protocol |
| MPC | Multi-Party Computation -- cryptographic protocols where multiple parties jointly compute a function without revealing their individual inputs |
| PoAT | Proof of Accurate Time -- Roko Network's consensus mechanism producing nanosecond-precision temporal receipts |
| Temporal Receipt | A Roko Network artifact containing a PoAT timestamp, validator signatures, and block reference proving an event occurred at a specific time |
| Trust Attestation | A wallet-signed statement expressing one user's trust in another, anchored to a temporal receipt |
| Device Certificate | A short-lived certificate binding a device's public key to a user's MPC identity, signed by the user's threshold wallet |
| Resharing | Protocol to redistribute MPC shares to a new set of parties without changing the aggregate public key |
| Social Recovery | Recovery mechanism where trusted peers hold encrypted share fragments that can reconstitute a user's identity |

## Appendix B: Related Documents

- Fortemi PKE module: `crates/matric-crypto/src/pke/`
- Fortemi keyset management: `crates/matric-db/src/pke_keysets.rs`
- FROST RFC: draft-irtf-cfrg-frost-15
- Roko Network specification: (external)
- User stories: `.aiwg/requirements/mpc-wallet/user-stories.md`
