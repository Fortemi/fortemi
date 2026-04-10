# ADR-001: MPC Protocol Selection

| Field | Value |
|-------|-------|
| **Decision ID** | ADR-001 |
| **Status** | Proposed |
| **Date** | 2026-04-09 |
| **Deciders** | MPC Wallet Architecture Team |
| **Relates to** | ADR-002 (Trust Attestation Format), ADR-004 (Roko Temporal Bridge) |

---

## Reasoning

The MPC wallet serves as the commissioning authority for the Personal Trust Network. It must produce threshold signatures across two cryptographic curves: Ed25519 (Fortemi's existing PKE system, note signing, trust attestations) and secp256k1 (Roko Network's ECDSA-based temporal receipts). The protocol must operate within Roko's 100ms tick window for real-time temporal anchoring while running on consumer devices (phones, laptops) that hold the user's own shares in a 2-of-3 configuration. There are no institutional custodians; the user's devices are both shareholders and signers.

---

## Context

Fortemi currently uses X25519 for key agreement and AES-256-GCM for symmetric encryption in its PKE wallet (`crates/matric-core`). The Roko Network is a Substrate blockchain using ECDSA secp256k1 for its Proof of Authenticated Time (PoAT) consensus, issuing nanosecond-precision temporal receipts. The MPC wallet must unify these two cryptographic worlds:

- **Ed25519/Schnorr domain**: Note signing, trust attestations, device certificates, Fortemi-internal operations
- **secp256k1/ECDSA domain**: Roko temporal receipt requests, on-chain identity binding, cross-chain attestations

The wallet operates in a self-sovereign model: a user's own devices (phone, laptop, hardware token) each hold one share of a 2-of-3 threshold key. The MPC wallet identity is the root of trust (commissioning authority) that issues device certificates and signs trust attestations. Device keys handle frequent runtime signing; the MPC wallet signs only for high-value operations (new device enrollment, trust attestation issuance, key rotation).

Key constraints:
- Roko's 100ms block tick means signing must complete well under 100ms end-to-end
- Consumer devices communicate over potentially high-latency links (Wi-Fi, cellular)
- The protocol must support both Schnorr (Ed25519) and ECDSA (secp256k1) signatures
- 2-of-3 threshold with no trusted dealer (DKG must be decentralized)
- Rust-native implementation required for integration with existing crate workspace

---

## Evaluation Criteria

| # | Criterion | Weight | Description |
|---|-----------|--------|-------------|
| 1 | **Latency** | 30% | Round count and total signing time. Roko's 100ms tick requires sub-50ms signing to leave room for network propagation. Fewer rounds = less coordination overhead on consumer devices. |
| 2 | **Multi-curve support** | 25% | Native support for both Ed25519/Schnorr and secp256k1/ECDSA. Single codebase for both curves strongly preferred over maintaining parallel implementations. |
| 3 | **Security model** | 20% | Threshold assumptions (honest majority vs. dishonest majority), identifiable aborts, resistance to rogue-key attacks, formal security proofs. |
| 4 | **Implementation maturity** | 15% | Production-readiness of available Rust crates, audit status, active maintenance, community adoption, test coverage. |
| 5 | **Maintenance burden** | 10% | Complexity of integration, dependency count, API surface, upgrade path, documentation quality. |

---

## Options

### Option 1: FROST (Flexible Round-Optimized Schnorr Threshold)

**Description**: FROST is a threshold Schnorr signature scheme specified in IETF RFC 9591. It requires only 2 rounds for signing (a single preprocessing round can be amortized offline, reducing online signing to 1 round). The ZCash Foundation maintains production Rust crates (`frost-core`, `frost-ed25519`, `frost-secp256k1-tr`) with per-ciphersuite implementations. FROST operates natively on any group where Schnorr signatures are defined, which includes Ed25519 directly. For secp256k1, FROST produces Schnorr signatures (compatible with BIP-340/Taproot) rather than ECDSA.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Latency | 5 | 2 rounds (1 online with preprocessing). Sub-10ms signing on modern hardware. Optimal for Roko's 100ms tick. |
| Multi-curve support | 4 | Native Ed25519 and secp256k1 Schnorr via ZCash crates. Does NOT produce ECDSA signatures for secp256k1 — only Schnorr. Roko's current ECDSA verification would need a Schnorr verifier added. |
| Security model | 5 | Honest-majority (t < n) with identifiable aborts in FROST3 variant. Formal proofs in the FROST paper (Komlo & Goldberg 2020). RFC 9591 standardized. Resistant to rogue-key attacks via proof-of-knowledge in DKG. |
| Implementation maturity | 5 | ZCash Foundation crates are audited (NCC Group, 2023), actively maintained, used in Zcash Orchard. Comprehensive test vectors from IETF. `frost-core` v2.0+ with stable API. |
| Maintenance burden | 4 | Clean trait-based architecture (`frost-core` + per-ciphersuite crates). Well-documented. Dependency on `group` and `ff` traits aligns with Rust crypto ecosystem. Minor burden: need to handle Schnorr vs ECDSA distinction for Roko. |

**Weighted Score**: (5 x 0.30) + (4 x 0.25) + (5 x 0.20) + (5 x 0.15) + (4 x 0.10) = 1.50 + 1.00 + 1.00 + 0.75 + 0.40 = **4.65**

### Option 2: CGGMP21 (Canetti-Gennaro-Goldfeder-Makriyannis-Peled)

**Description**: CGGMP21 is a threshold ECDSA protocol supporting dishonest majority (t < n with identifiable aborts even when the majority is corrupt). It requires 4-5 rounds for presigning and 1 round for online signing. The protocol is specifically designed for ECDSA on secp256k1 and similar curves. Rust implementations exist in `multi-party-ecdsa` (ZenGo) and `cggmp21` crates, though maturity varies.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Latency | 3 | 4-5 rounds for presigning, 1 round online. Presigning can be amortized, but the initial cost is high. Online round is fast but presign coordination adds latency for first-time or non-cached signing. |
| Multi-curve support | 2 | ECDSA-only by design. Cannot produce Ed25519/Schnorr signatures. Would require a completely separate protocol for Ed25519 operations, doubling the implementation surface. |
| Security model | 5 | Dishonest majority with identifiable aborts — strongest security model available. Formal proofs in CCS 2021 paper. Each party can identify and exclude malicious participants. |
| Implementation maturity | 3 | ZenGo's `multi-party-ecdsa` is widely referenced but has known issues and limited maintenance. `cggmp21` by DFNS is newer and cleaner but less battle-tested. No IETF standardization. Audits exist but are less comprehensive than FROST's. |
| Maintenance burden | 2 | Complex protocol with many moving parts (Paillier encryption, range proofs, commitment schemes). Heavy dependency tree. Significant expertise required to maintain and debug. |

**Weighted Score**: (3 x 0.30) + (2 x 0.25) + (5 x 0.20) + (3 x 0.15) + (2 x 0.10) = 0.90 + 0.50 + 1.00 + 0.45 + 0.20 = **3.05**

### Option 3: GG20 (Gennaro-Goldfeder 2020)

**Description**: GG20 is the predecessor to CGGMP21, with 8 rounds for key generation and 6 rounds for signing. It is battle-tested in production at Fireblocks, Zengo, and other institutional custody platforms. The protocol supports threshold ECDSA on secp256k1 with honest-majority assumptions. The `multi-party-ecdsa` crate implements this protocol.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Latency | 1 | 8 rounds for DKG, 6 rounds for signing. Even with fast networks, 6 round-trips between consumer devices over Wi-Fi/cellular will exceed 100ms. Fundamentally incompatible with Roko's tick window for interactive signing. |
| Multi-curve support | 2 | secp256k1 ECDSA only. Same limitation as CGGMP21 — no Ed25519 support. Would need a parallel Schnorr threshold scheme. |
| Security model | 4 | Honest majority with abort detection (not identifiable aborts like CGGMP21). Well-understood security properties. Formal proofs in the GG20 paper. Weaker than CGGMP21's dishonest majority model. |
| Implementation maturity | 4 | Most battle-tested option — runs at Fireblocks scale (billions of USD secured). ZenGo's implementation has years of production exposure. However, the crate is showing its age and maintenance is declining in favor of CGGMP21. |
| Maintenance burden | 2 | Similar complexity to CGGMP21 (Paillier, range proofs). Additionally, the ecosystem is migrating away from GG20 toward CGGMP21, meaning long-term maintenance becomes increasingly solo. |

**Weighted Score**: (1 x 0.30) + (2 x 0.25) + (4 x 0.20) + (4 x 0.15) + (2 x 0.10) = 0.30 + 0.50 + 0.80 + 0.60 + 0.20 = **2.40**

### Option 4: Multi-Protocol Approach (FROST + tECDSA Wrapper)

**Description**: Use FROST as the primary protocol for Ed25519/Schnorr operations and wrap a threshold ECDSA protocol (CGGMP21 or a FROST-to-ECDSA adapter like FROST-secp256k1 with an ECDSA compatibility layer) for secp256k1 ECDSA when Roko strictly requires it. The adapter approach uses FROST's DKG and share management for both curves but applies an ECDSA signing conversion for secp256k1 outputs. Research papers (Crites et al., 2023) describe adaptor signatures that bridge Schnorr and ECDSA.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Latency | 4 | FROST path remains 2 rounds. ECDSA adapter adds 1-2 extra rounds for the conversion step. Still within 50ms budget for most scenarios, but adds variance. |
| Multi-curve support | 5 | Full native support for both Ed25519 and secp256k1 ECDSA. Best of both worlds — Schnorr where possible, ECDSA where required. |
| Security model | 4 | FROST's security for Schnorr operations. ECDSA adapter introduces additional cryptographic assumptions (adaptor signature security). Less thoroughly analyzed than pure FROST or pure CGGMP21. Composition of two protocols requires careful security analysis. |
| Implementation maturity | 2 | FROST crates are mature, but ECDSA adaptor layers are research-grade. No production-audited Rust implementation of FROST-to-ECDSA adaptor signatures exists. Would require custom implementation and independent audit. |
| Maintenance burden | 1 | Two protocols to maintain, understand, and debug. Adapter layer is novel code with no community support. Security composition must be re-verified on every update. Highest maintenance cost of all options. |

**Weighted Score**: (4 x 0.30) + (5 x 0.25) + (4 x 0.20) + (2 x 0.15) + (1 x 0.10) = 1.20 + 1.25 + 0.80 + 0.30 + 0.10 = **3.65**

---

## Comparison Matrix

| Criterion | Weight | FROST | CGGMP21 | GG20 | Multi-Protocol |
|-----------|--------|-------|---------|------|----------------|
| Latency | 30% | **5** (2 rounds) | 3 (4-5 rounds) | 1 (6+ rounds) | 4 (2-4 rounds) |
| Multi-curve support | 25% | 4 (Schnorr only) | 2 (ECDSA only) | 2 (ECDSA only) | **5** (both native) |
| Security model | 20% | **5** (RFC 9591) | **5** (dishonest maj.) | 4 (honest maj.) | 4 (composed) |
| Implementation maturity | 15% | **5** (audited ZCash) | 3 (mixed) | 4 (Fireblocks) | 2 (research) |
| Maintenance burden | 10% | **4** (clean traits) | 2 (complex) | 2 (declining) | 1 (two protocols) |
| **Weighted Total** | | **4.65** | 3.05 | 2.40 | 3.65 |

---

## Decision

**Adopt FROST (Option 1)** as the MPC protocol for the Fortemi wallet, using the ZCash Foundation's `frost-core`, `frost-ed25519`, and `frost-secp256k1-tr` crates.

### Rationale

FROST wins decisively on the two highest-weighted criteria: latency (30%) and implementation maturity (15%), while scoring top marks on security model (20%). Its 2-round signing protocol is the only option that reliably fits within Roko's 100ms tick window when operating across consumer devices with variable network latency.

The primary trade-off is multi-curve support: FROST produces Schnorr signatures on secp256k1, not ECDSA. This is addressed by evolving Roko Network to accept Schnorr (BIP-340 compatible) verification alongside ECDSA. This is a tractable change because:

1. Substrate's `sp-core` already supports sr25519 (Schnorr-based) — adding secp256k1 Schnorr verification is architecturally consistent
2. The Schnorr signature is strictly superior to ECDSA (provable security, batch verification, linearity)
3. The broader ecosystem (Bitcoin Taproot, Ethereum account abstraction) is converging on Schnorr acceptance

If Roko cannot be modified to accept Schnorr signatures in the near term, a thin ECDSA adapter can be added later as a tactical bridge without restructuring the core MPC protocol.

---

## Consequences

### Positive

- **Sub-50ms signing latency**: 2-round FROST with offline preprocessing enables real-time Roko temporal anchoring. In practice, with pre-computed nonces, online signing is a single round (~5-15ms on consumer hardware).
- **Unified share management**: Single DKG produces shares usable for both Ed25519 and secp256k1 Schnorr operations. One key ceremony, one backup strategy, one recovery flow.
- **IETF standardization**: RFC 9591 provides a stable specification that won't drift. Reduces risk of protocol-level breaking changes.
- **Audited implementation**: ZCash Foundation's crates have undergone professional security audit. Reduces time-to-production and audit costs for Fortemi.
- **Clean Rust integration**: `frost-core`'s trait-based architecture (`Ciphersuite` trait) integrates naturally with Fortemi's existing crate workspace. Can implement custom ciphersuites if needed.
- **Batch verification**: Schnorr signatures support efficient batch verification, which benefits trust attestation validation (verifying many attestations at once).

### Negative

- **Roko must accept Schnorr on secp256k1**: The Roko Network currently validates ECDSA only. Adding Schnorr verification is a protocol-level change requiring a Roko runtime upgrade. This introduces a cross-team dependency and timeline risk.
- **Honest majority assumption**: FROST requires t < n/2 honest participants. In a 2-of-3 scheme, this means at most 1 compromised device. If an attacker controls 2 of 3 devices, they can forge signatures. This is acceptable for the self-sovereign model (user controls all devices) but would not suit institutional custody with mutually distrusting parties.
- **No ECDSA output without adapter**: Any third-party integration requiring raw ECDSA signatures (e.g., Ethereum, legacy systems) would need an additional protocol layer. This is not a current requirement but limits future interoperability.

### Neutral

- **Pre-computation model shifts UX**: Offline nonce preprocessing means the wallet must anticipate signing needs. A background process on each device should periodically generate and cache nonce commitments. This is a design pattern change but not a burden — it improves perceived latency.
- **DKG ceremony is a one-time event**: FROST's DKG is heavier than signing (multiple rounds, Feldman VSS). However, key generation happens only during wallet creation and resharing, not during normal operation. The cost is amortized over the wallet's lifetime.
- **Share refresh is supported**: FROST supports proactive share refresh (re-randomizing shares without changing the public key), which enables periodic rotation of device shares without user-visible ceremony.

---

## Implementation Notes

### Crate Integration

```toml
# Cargo.toml additions
frost-core = "2.0"
frost-ed25519 = "2.0"
frost-secp256k1-tr = "2.0"   # Taproot-compatible Schnorr on secp256k1
```

### Architecture Sketch

```
crates/matric-core/src/mpc/
  mod.rs              -- MPC wallet public API
  frost_wallet.rs     -- FROST-based wallet implementation
  dkg.rs              -- Distributed key generation ceremony
  signing.rs          -- Threshold signing (Ed25519 + secp256k1 Schnorr)
  share_store.rs      -- Encrypted share persistence
  nonce_cache.rs      -- Pre-computed nonce management
```

### Key Design Decisions

1. **Share storage**: Each device's share is encrypted at rest using the device's local key (derived from device unlock credential). Shares never leave their device in plaintext.
2. **Nonce pre-computation**: Background task generates nonce pairs and exchanges commitments with other online devices. Target: maintain a buffer of 100+ pre-computed nonces per peer pair.
3. **DKG protocol**: Use FROST's built-in DKG (Pedersen/Feldman VSS) rather than a trusted dealer. All 3 devices must be online simultaneously for initial key generation.
4. **Ciphersuite abstraction**: Implement signing logic against `frost-core::Ciphersuite` trait so Ed25519 and secp256k1 paths share maximum code.

### Migration Path from Existing PKE

The existing X25519/AES-256-GCM PKE wallet continues to operate for note encryption. The MPC wallet is a new, parallel capability:
- PKE wallet: note-level encryption (symmetric, fast, existing)
- MPC wallet: identity signing, trust attestation, device delegation (threshold, new)

Phase 1 deploys FROST for Ed25519 operations only (trust attestations, device certs). Phase 2 adds secp256k1 Schnorr for Roko integration once Roko's Schnorr verifier is deployed.

### Roko Schnorr Verification Dependency

File a Roko Network RFC for adding BIP-340-compatible Schnorr signature verification to the PoAT pallet. This is a prerequisite for Phase 2. The change is additive (does not break existing ECDSA verification) and can be deployed via a standard Substrate runtime upgrade.

---

## References

- Komlo, C. & Goldberg, I. (2020). "FROST: Flexible Round-Optimized Schnorr Threshold Signatures." SAC 2020.
- IETF RFC 9591: "Two-Round Threshold Schnorr Signatures with FROST"
- ZCash Foundation FROST crates: https://github.com/ZcashFoundation/frost
- NCC Group Audit Report (2023): ZCash FROST Implementation Security Assessment
- BIP-340: Schnorr Signatures for secp256k1
