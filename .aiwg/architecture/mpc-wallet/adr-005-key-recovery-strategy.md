# ADR-005: Key Recovery Strategy

| Field | Value |
|-------|-------|
| **Decision ID** | ADR-005 |
| **Status** | Proposed |
| **Date** | 2026-04-09 |
| **Deciders** | MPC Wallet Architecture Team |
| **Relates to** | ADR-001 (MPC Protocol Selection), ADR-003 (Device Certificate Model), ADR-004 (Roko Temporal Bridge) |

---

## Reasoning

Key recovery is the most critical user experience problem in self-sovereign identity systems. If a user loses enough devices to fall below the 2-of-3 threshold (e.g., phone lost + laptop destroyed), their MPC wallet identity is permanently unrecoverable without a recovery mechanism. Unlike institutional custody where a company can reset access, the Personal Trust Network has no central authority. Recovery must be designed into the system from day one — retrofitting it later is architecturally difficult and creates a window of vulnerability for early adopters.

The recovery strategy must balance security (recovery should not be a backdoor) with usability (recovery should actually work when needed, which means users must set it up before disaster strikes).

---

## Context

### MPC Wallet Share Distribution

Per ADR-001 (FROST, 2-of-3 threshold):
- **Share A**: User's primary device (e.g., phone)
- **Share B**: User's secondary device (e.g., laptop)
- **Share C**: User's tertiary device (e.g., hardware token, tablet, or another machine)

Normal operation requires any 2 of 3 shares. Loss scenarios:

| Scenario | Shares Available | Impact |
|----------|-----------------|--------|
| 1 device lost | 2 of 3 | No impact — reshare to new device using remaining 2 |
| 2 devices lost | 1 of 3 | **CRITICAL — below threshold, wallet unusable** |
| All devices lost | 0 of 3 | **CATASTROPHIC — total identity loss** |
| 1 device compromised | Attacker has 1 | No immediate risk (needs 2). Reshare urgently to exclude compromised share. |
| 2 devices compromised | Attacker has 2 | **CRITICAL — attacker can sign. Wallet compromised.** |

The recovery strategy addresses the "2 devices lost" and "all devices lost" scenarios. The "1 device lost" scenario is handled by normal FROST resharing and does not require a recovery mechanism.

### Design Principles

1. **Recovery should not weaken the threshold**: A recovery mechanism that allows wallet access with less than the threshold (2-of-3) effectively lowers the security to 1-of-N.
2. **Recovery should be time-delayed**: Prevents an attacker from using the recovery mechanism to hijack a wallet before the legitimate owner can react.
3. **Recovery should require action before loss**: Users must set up recovery while they still have wallet access. Post-loss setup is impossible by definition.
4. **Recovery should not depend on a single party**: Aligns with the decentralized, self-sovereign model.

### Roko Network as a Time-Lock

Roko's temporal receipts (ADR-004) provide a valuable primitive for recovery: a time-locked commitment. A recovery request can be submitted to Roko with a mandatory waiting period (e.g., 72 hours). During this period, the legitimate owner (if they still have 1 share) can see the recovery attempt and cancel it. This turns Roko into a decentralized time-lock without requiring a smart contract — just a temporal receipt with a "do not execute before" timestamp.

---

## Evaluation Criteria

| # | Criterion | Weight | Description |
|---|-----------|--------|-------------|
| 1 | **Security** | 35% | Resistance to unauthorized recovery. An attacker should not be able to recover a wallet they don't own. Time delays, multi-party requirements, and proof-of-identity mechanisms all contribute. |
| 2 | **Usability** | 25% | Likelihood that users will set up recovery AND successfully execute it when needed. Complex setup reduces adoption; complex execution increases failure rate under stress. |
| 3 | **Recovery success rate** | 20% | Probability that a legitimate user can actually recover their wallet across realistic loss scenarios (house fire, theft, device failure, long-term storage degradation). |
| 4 | **Implementation complexity** | 15% | Engineering effort, new infrastructure requirements, testing surface, interaction with existing FROST/device cert systems. |
| 5 | **Cost** | 5% | Ongoing operational costs (Roko transaction fees, storage costs, third-party service dependencies). |

---

## Options

### Option 1: Social Recovery

**Description**: The user designates 3-5 trusted peers (friends, family, colleagues) as recovery guardians. Each guardian receives a recovery attestation signed by the user's MPC wallet. To recover, the user contacts a quorum of guardians (e.g., 3-of-5) who co-sign a recovery request. The recovery request is anchored to Roko with a mandatory 72-hour time lock. After the time lock expires and no cancellation is received, the user can generate a new MPC wallet and the guardians' co-signature authorizes the identity migration.

**Recovery flow**:
1. **Setup** (while wallet is active): User selects 3-5 guardians from their trust network. MPC wallet signs a `RecoveryGuardianAttestation` for each guardian, stored in both the user's and guardian's Fortemi instances.
2. **Initiation** (after loss): User contacts guardians out-of-band (phone, email, in person). Each consenting guardian signs a `RecoveryVote` with their own device key.
3. **Submission**: Recovery votes are collected and submitted to Roko as a time-locked recovery request (72-hour delay).
4. **Challenge period**: If the original wallet holder still has any device, they see the recovery request (via Roko event subscription) and can cancel it by signing a cancellation with their remaining share.
5. **Execution**: After 72 hours with no cancellation, the new MPC wallet inherits the trust graph of the old wallet (guardians attest to the continuity).

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Security | 4 | Requires quorum of guardians (3-of-5) — no single guardian can trigger recovery. 72-hour Roko time lock gives the legitimate owner a window to cancel. Social engineering attack requires corrupting a quorum of independent guardians. Weakness: guardian collusion (if 3+ guardians conspire, they can hijack the wallet after the time lock). |
| Usability | 3 | Setup requires selecting guardians and having them accept the role — a social coordination task many users will procrastinate on. Guardian availability during recovery is not guaranteed (what if guardians are unreachable, have lost their devices, or have died?). Recovery requires out-of-band communication. The 72-hour delay adds anxiety during an already stressful situation. |
| Recovery success rate | 3 | Depends entirely on guardian availability. If the user selected guardians who are still active, reachable, and willing to help, success rate is high. But guardian availability degrades over time: people change devices, lose contact, move on. After 2+ years, the probability of reaching a quorum of original guardians drops significantly. No fallback if guardians are unavailable. |
| Implementation complexity | 3 | Requires: RecoveryGuardianAttestation format, RecoveryVote protocol, Roko time-lock submission, cancellation mechanism, identity migration protocol, guardian management UI. Significant new code across trust attestations (ADR-002), Roko bridge (ADR-004), and device enrollment (ADR-003). |
| Cost | 4 | Roko transaction fees for time-lock submission and cancellation (minimal — a few cents per recovery attempt). No ongoing storage costs beyond the guardian attestations. No third-party service dependencies. |

**Weighted Score**: (4 x 0.35) + (3 x 0.25) + (3 x 0.20) + (3 x 0.15) + (4 x 0.05) = 1.40 + 0.75 + 0.60 + 0.45 + 0.20 = **3.40**

### Option 2: Encrypted Backup Share

**Description**: Generate an additional FROST share (the 4th share in a 2-of-4 resharing, maintaining the 2-of-N threshold) and encrypt it with a user-chosen passphrase using Argon2id key derivation + AES-256-GCM. The encrypted share is stored durably: local encrypted file, cloud storage (iCloud/Google Drive), printed QR code, or USB drive in a safe. Recovery requires the passphrase + the encrypted share + 0 other devices (the share alone is enough to participate in a resharing ceremony with any 1 remaining device, or with a new empty wallet for catastrophic recovery).

**Recovery flow**:
1. **Setup** (while wallet is active): User triggers backup share generation. FROST resharing produces a 4th share. User enters a strong passphrase. Share is encrypted and exported.
2. **Storage**: User stores the encrypted backup in one or more durable locations. Fortemi provides export as file, QR code, or cloud upload.
3. **Recovery (1 device lost)**: Normal FROST resharing with remaining 2 devices. Backup share is not needed.
4. **Recovery (2 devices lost, 1 remaining)**: Decrypt backup share with passphrase. Use backup share + remaining device share (2 shares = threshold met) to reshare to new devices.
5. **Recovery (all devices lost)**: Decrypt backup share with passphrase. The single share is below threshold — cannot sign alone. Requires combining with social recovery (Option 1) or accepting identity reset.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Security | 3 | The encrypted backup is a static secret protected only by the passphrase. Argon2id provides strong KDF, but passphrase entropy is user-dependent. If the passphrase is weak or reused, the backup share is vulnerable to offline brute-force. If the backup file is stolen AND the passphrase is compromised, the attacker has 1 share (still below threshold, but combined with device theft = wallet compromise). No time-lock protection — recovery is immediate. |
| Usability | 5 | Setup is simple: click "Create Backup," enter a passphrase, save the file. No social coordination required. Recovery is equally simple: retrieve the backup file, enter the passphrase. Users are familiar with passphrase-protected backups from password managers and crypto wallets. Most likely to actually be set up. |
| Recovery success rate | 4 | Depends on: (a) user remembering the passphrase, (b) backup file being accessible. Printed QR codes survive device loss. Cloud-stored backups survive house fires. Multiple backup locations increase reliability. Weakness: passphrase memory degrades over time — if the user hasn't needed the backup for years, they may have forgotten the passphrase. Passphrase hints or derivation from memorable information help. |
| Implementation complexity | 4 | FROST resharing from 2-of-3 to 2-of-4 is a supported operation (FROST's `repairable` module). Argon2id + AES-256-GCM encryption is straightforward (`argon2` + `aes-gcm` crates, both already in Fortemi's dependency tree for PKE). Export formats (file, QR) are simple. Least new infrastructure of all options. |
| Cost | 5 | Zero ongoing cost. No Roko transactions, no third-party services, no guardian management. The encrypted file is self-contained and free to store. |

**Weighted Score**: (3 x 0.35) + (5 x 0.25) + (4 x 0.20) + (4 x 0.15) + (5 x 0.05) = 1.05 + 1.25 + 0.80 + 0.60 + 0.25 = **3.95**

### Option 3: Threshold Increase (Proactive Resharing)

**Description**: Proactively reshare the MPC wallet from 2-of-3 to 3-of-5 (or higher) during calm periods, distributing additional shares to more devices or secure storage locations. Degradation is graceful: losing 1-2 devices still leaves enough shares to operate and reshare. The idea is to maintain a surplus of shares so that reaching below-threshold requires an unlikely number of simultaneous losses.

**Recovery flow**:
1. **Setup**: User adds devices 4 and 5 (or secure storage locations) and triggers resharing from 2-of-3 to 3-of-5.
2. **Normal operation**: Any 3 of 5 shares can sign. Losing 1-2 devices leaves 3 shares = still operational.
3. **Recovery after 2 losses**: Still have 3 of 5 shares. Reshare to replace lost devices. No special recovery mechanism needed.
4. **Recovery after 3 losses**: 2 of 5 shares remain = below threshold. Same catastrophic failure as original 2-of-3 with all devices lost.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Security | 4 | Higher threshold (3-of-5) means an attacker needs to compromise 3 devices instead of 2. More shares = larger attack surface, but the threshold increase compensates. No new cryptographic assumptions — just FROST resharing. However, more shares means more storage locations to secure and more devices that could be targeted. |
| Usability | 2 | Requires the user to have 5 devices or secure storage locations. Most users have 2-3 devices. Finding 2 additional share holders requires either buying hardware (expensive) or storing shares in cloud services (adds trust dependencies). Managing 5 shares is more complex than 3. Users must track which devices hold shares and keep them updated. |
| Recovery success rate | 4 | For the common case (1-2 device loss), this is the best option — recovery is just normal operation (resharing with remaining shares). However, it does not solve catastrophic loss (3+ devices). It reduces the probability of needing recovery but does not eliminate it. The fundamental problem (below-threshold = unrecoverable) is deferred, not solved. |
| Implementation complexity | 3 | FROST resharing from t-of-n to t'-of-n' is supported but involves a full DKG-like ceremony with all n' participants online simultaneously. Coordinating 5 devices for resharing is logistically harder than 3. Share tracking, device inventory management, and resharing scheduling add UX complexity. |
| Cost | 4 | Minimal cost — just the additional devices or storage media. No ongoing fees. However, if "device 4" is a cloud HSM or similar service, subscription costs apply. |

**Weighted Score**: (4 x 0.35) + (2 x 0.25) + (4 x 0.20) + (3 x 0.15) + (4 x 0.05) = 1.40 + 0.50 + 0.80 + 0.45 + 0.20 = **3.35**

### Option 4: Hybrid (Encrypted Backup Share + Social Recovery)

**Description**: Combine Options 1 and 2. The user creates an encrypted backup share (Option 2) for the common recovery case (lost devices, forgotten in a drawer, hardware failure). For catastrophic scenarios where even the backup is lost (house fire, natural disaster, long-term memory loss of passphrase), social recovery (Option 1) serves as a last resort. The two mechanisms are independent — either one can restore wallet access.

**Recovery tiers**:
1. **Tier 1 — Normal**: 1 device lost. FROST resharing with remaining 2 devices. No recovery mechanism needed.
2. **Tier 2 — Backup**: 2 devices lost. Decrypt backup share + use remaining 1 device share = threshold met. Reshare to new devices.
3. **Tier 3 — Social**: All devices lost AND backup inaccessible. Guardian quorum + Roko time lock. Identity migration to new wallet.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Security | 5 | Two independent recovery paths, each with different attack vectors. Compromising the encrypted backup requires the passphrase (knowledge factor). Compromising social recovery requires corrupting a guardian quorum (social factor). An attacker would need to compromise BOTH paths to guarantee unauthorized recovery — layered defense. The Roko time lock on social recovery adds a temporal defense layer. The backup share is still below-threshold alone (needs 1 more device), so theft of the backup alone doesn't compromise the wallet. |
| Usability | 4 | Backup share setup is simple (most users will do this). Social recovery setup is more effort but positioned as optional-but-recommended (advanced users, high-value wallets). Recovery execution: Tier 2 (backup) is as simple as Option 2. Tier 3 (social) is available as a fallback but most users will never need it. The tiered approach means the easy path covers 90%+ of recovery scenarios. |
| Recovery success rate | 5 | Highest of all options. Backup share covers device loss with high probability (user has the file + remembers passphrase). Social recovery covers backup loss with moderate probability (guardians are reachable). The probability that BOTH the backup AND the social recovery path fail is very low (requires: all devices lost + backup inaccessible + guardian quorum unreachable). Two independent recovery vectors multiply the success probability. |
| Implementation complexity | 2 | Requires implementing both Option 1 (social recovery) and Option 2 (encrypted backup). Combined complexity exceeds either alone: guardian management, backup generation, Roko time lock, identity migration, two recovery UX flows, interaction between the two systems (e.g., can social recovery be used to reset a forgotten backup passphrase? — decision: no, to avoid weakening the backup). |
| Cost | 4 | Slightly higher than either option alone: Roko transaction fees for social recovery time lock + zero cost for encrypted backup. No ongoing subscriptions. Guardian storage costs are negligible. |

**Weighted Score**: (5 x 0.35) + (4 x 0.25) + (5 x 0.20) + (2 x 0.15) + (4 x 0.05) = 1.75 + 1.00 + 1.00 + 0.30 + 0.20 = **4.25**

---

## Comparison Matrix

| Criterion | Weight | Social Recovery | Encrypted Backup | Threshold Increase | Hybrid |
|-----------|--------|----------------|-----------------|-------------------|--------|
| Security | 35% | 4 (quorum+timelock) | 3 (passphrase) | 4 (higher threshold) | **5** (layered defense) |
| Usability | 25% | 3 (social coordination) | **5** (passphrase) | 2 (5 devices) | 4 (tiered) |
| Recovery success rate | 20% | 3 (guardian availability) | 4 (file+passphrase) | 4 (surplus shares) | **5** (two vectors) |
| Implementation complexity | 15% | 3 (moderate) | **4** (simple) | 3 (resharing UX) | 2 (both systems) |
| Cost | 5% | 4 (minimal Roko fees) | **5** (zero) | 4 (device costs) | 4 (minimal) |
| **Weighted Total** | | 3.40 | 3.95 | 3.35 | **4.25** |

---

## Decision

**Adopt the Hybrid approach (Option 4)** with phased implementation: encrypted backup share first (Phase 1), social recovery second (Phase 2).

### Rationale

The Hybrid approach scores highest (4.25) and is the only option that achieves a 5 on both security and recovery success rate — the two most critical criteria for a recovery system. The implementation complexity trade-off (score 2) is mitigated by phased delivery.

**Phase 1 — Encrypted Backup Share** (ship with MPC wallet MVP):
- Covers the most common recovery scenario (device loss)
- Simple to implement (FROST resharing + Argon2id + AES-256-GCM)
- Simple to use (passphrase + file)
- No external dependencies (no Roko, no guardians)
- Users get recovery protection from day one

**Phase 2 — Social Recovery** (ship 1-2 iterations after MVP):
- Covers the catastrophic scenario (all devices + backup lost)
- Requires Roko temporal bridge (ADR-004) for time lock
- Requires trust network to be established (users need guardians)
- More complex to build and test
- Positioned as recommended-but-optional enhancement

This phasing means the MPC wallet ships with recovery protection immediately (encrypted backup), and gains defense-in-depth as the trust network matures (social recovery).

The encrypted backup alone (Option 2, score 3.95) was the close alternative and would be acceptable as a permanent solution for V1. However, the marginal effort of adding social recovery in Phase 2 provides disproportionate security value: it eliminates the single point of failure (passphrase memory) and adds the Roko time-lock defense against unauthorized recovery.

---

## Consequences

### Positive

- **Defense in depth**: Two independent recovery mechanisms with different failure modes. An attacker must compromise both passphrase knowledge AND social trust to hijack a wallet via recovery. A legitimate user only needs one path to succeed.
- **Graduated UX complexity**: Phase 1 (encrypted backup) is as simple as creating a password manager backup. Phase 2 (social recovery) is opt-in for users who want maximum resilience. Users choose their security/convenience trade-off.
- **Recovery success probability > 99%**: For users who set up both mechanisms, the probability of permanent wallet loss requires: all devices destroyed + backup file inaccessible + passphrase forgotten + insufficient guardian quorum reachable. This is a conjunction of 4 independent low-probability events.
- **Roko time lock adds temporal defense**: Social recovery cannot be executed instantly. The 72-hour window gives the legitimate owner time to detect and cancel unauthorized recovery attempts. This is unique to Option 4 — encrypted backup alone has no such defense.
- **Existing crypto infrastructure**: Argon2id and AES-256-GCM are already in Fortemi's dependency tree for the PKE wallet. FROST resharing is part of ADR-001's crate selection. Minimal new cryptographic dependencies.

### Negative

- **Implementation is the most complex option**: Building both recovery mechanisms doubles the engineering effort compared to either alone. Phase 1 is straightforward; Phase 2 (social recovery + Roko time lock + identity migration) is architecturally significant.
- **User education burden**: Users must understand two recovery mechanisms and their purposes. Clear UX guidance is essential: "Backup protects against device loss. Guardians protect against total disaster." Confusing these mechanisms could lead to false security (e.g., user thinks guardians alone are sufficient and skips backup).
- **Social recovery depends on trust network maturity**: Phase 2 only works when users have established trust relationships with reliable guardians. Early adopters (before the network has critical mass) may not have enough eligible guardians. Mitigation: allow guardians from outside Fortemi (they just need an Ed25519 keypair and a communication channel).
- **Passphrase as a knowledge factor**: The encrypted backup's security ultimately rests on the user's passphrase. Users who choose weak passphrases or store them insecurely undermine the backup's security. Mitigation: enforce minimum passphrase entropy (zxcvbn score >= 3), display strength feedback, suggest generated diceware phrases.

### Neutral

- **Recovery does not preserve device certificates**: When recovering to a new wallet (social recovery / identity migration), all previous device certificates are invalidated. The new wallet must issue new device certificates. Trust attestations issued by the old wallet remain valid (the public key hasn't changed in backup recovery; in social recovery, guardians attest to the identity continuity).
- **Backup share rotation**: The encrypted backup share should be rotated periodically (recommended: annually) to limit the window of exposure if the passphrase is compromised. Rotation = generate new backup share, encrypt with new passphrase, destroy old backup. Fortemi can remind users via notification.
- **Guardian selection guidance**: Fortemi should provide guidance on guardian selection: geographic diversity (not all in the same city), relationship diversity (not all family members), technical reliability (guardians who actively use their devices). This is a UX concern, not an architectural one.

---

## Implementation Notes

### Phase 1: Encrypted Backup Share

#### Crate Structure

```
crates/matric-core/src/mpc/
  recovery/
    mod.rs              -- Recovery public API
    backup_share.rs     -- Encrypted backup share generation + recovery
    passphrase.rs       -- Passphrase validation (zxcvbn integration)
```

#### Backup Generation Flow

```rust
pub struct EncryptedBackupShare {
    /// Format version
    pub version: u8,
    /// Argon2id parameters used for KDF
    pub kdf_params: Argon2Params,
    /// Random salt for Argon2id
    pub salt: [u8; 32],
    /// AES-256-GCM nonce
    pub nonce: [u8; 12],
    /// Encrypted FROST share (ciphertext + 16-byte GCM tag)
    pub ciphertext: Vec<u8>,
    /// MPC wallet public key (for identification)
    pub wallet_pubkey: [u8; 32],
}

impl EncryptedBackupShare {
    pub fn create(
        wallet: &FrostWallet,
        passphrase: &str,
    ) -> Result<Self, RecoveryError> {
        // 1. Validate passphrase strength (zxcvbn score >= 3)
        // 2. FROST reshare: 2-of-3 -> 2-of-4, producing share #4
        // 3. Serialize share #4 to CBOR
        // 4. Argon2id(passphrase, salt) -> 256-bit key
        // 5. AES-256-GCM encrypt the serialized share
        // 6. Return EncryptedBackupShare
    }

    pub fn recover(
        &self,
        passphrase: &str,
    ) -> Result<FrostShare, RecoveryError> {
        // 1. Argon2id(passphrase, self.salt) -> 256-bit key
        // 2. AES-256-GCM decrypt
        // 3. Deserialize FROST share from CBOR
        // 4. Return share (caller combines with remaining device share for resharing)
    }
}
```

#### Export Formats

1. **File**: CBOR-encoded `EncryptedBackupShare`, extension `.fortemi-backup`
2. **QR Code**: Same CBOR bytes, base45-encoded for QR efficiency, split into multi-QR if > 2KB
3. **Printable**: Base32-encoded with checksum, formatted for manual entry (last resort)

#### Argon2id Parameters

```rust
pub struct Argon2Params {
    pub memory_kib: u32,    // 256 MiB (256 * 1024)
    pub iterations: u32,    // 4
    pub parallelism: u32,   // 4
}
```

These parameters target ~1 second derivation on consumer hardware, making offline brute-force expensive (~$10K+ for a 12-character passphrase with mixed case + digits).

### Phase 2: Social Recovery

#### Additional Crate Structure

```
crates/matric-core/src/mpc/
  recovery/
    social.rs           -- Guardian management, recovery vote protocol
    timelock.rs         -- Roko time-lock submission and monitoring
    migration.rs        -- Identity migration (old wallet -> new wallet)
```

#### Guardian Attestation (CBOR/COSE, consistent with ADR-002)

```
RecoveryGuardianAttestation {
    wallet_pubkey: [u8; 32],       // User's MPC wallet public key
    guardian_pubkey: [u8; 32],     // Guardian's device/wallet public key
    guardian_index: u8,            // 1-5, for quorum tracking
    quorum_required: u8,           // e.g., 3 (of 5 guardians)
    created_at: i64,
    expires_at: i64,               // Guardian designation expiry
    // Signed by user's MPC wallet (FROST threshold signature)
}
```

#### Recovery Vote Protocol

```
RecoveryVote {
    original_wallet_pubkey: [u8; 32],  // Wallet being recovered
    new_wallet_pubkey: [u8; 32],       // New wallet claiming identity
    guardian_pubkey: [u8; 32],          // Voting guardian
    voted_at: i64,
    // Signed by guardian's device key
}
```

#### Roko Time Lock

The collected recovery votes (meeting quorum) are hashed and submitted to Roko as a temporal anchor with a metadata field indicating `type: recovery_request, delay: 72h`. The recovery executor polls Roko for the receipt timestamp and does not proceed until `receipt_timestamp + 72h < now`.

Cancellation: The original wallet holder (with any remaining share) signs a `RecoveryCancellation` and submits it to Roko, referencing the recovery request's receipt hash. Guardians and the recovery executor monitor for cancellations.

#### Database Schema

```sql
-- Phase 1
CREATE TABLE recovery_backup_shares (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_pubkey BYTEA NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Encrypted share is NOT stored in DB -- exported to user only
    backup_exists BOOLEAN NOT NULL DEFAULT false,
    last_rotated_at TIMESTAMPTZ
);

-- Phase 2
CREATE TABLE recovery_guardians (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_pubkey BYTEA NOT NULL,
    guardian_pubkey BYTEA NOT NULL,
    guardian_index SMALLINT NOT NULL,
    quorum_required SMALLINT NOT NULL,
    attestation_cbor BYTEA NOT NULL,       -- COSE_Sign1 envelope
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    UNIQUE(wallet_pubkey, guardian_index)
);

CREATE TABLE recovery_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    original_wallet_pubkey BYTEA NOT NULL,
    new_wallet_pubkey BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'collecting_votes',  -- collecting_votes, timelocked, completed, cancelled
    votes_collected SMALLINT NOT NULL DEFAULT 0,
    quorum_required SMALLINT NOT NULL,
    roko_receipt_hash BYTEA,
    roko_receipt_timestamp BIGINT,
    timelock_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ
);
```

### API Endpoints

```
# Phase 1
POST   /api/v1/mpc/recovery/backup          -- Generate encrypted backup share
GET    /api/v1/mpc/recovery/backup/status    -- Check if backup exists, last rotated

# Phase 2
POST   /api/v1/mpc/recovery/guardians        -- Designate guardians
GET    /api/v1/mpc/recovery/guardians        -- List current guardians
DELETE /api/v1/mpc/recovery/guardians/:id    -- Revoke a guardian
POST   /api/v1/mpc/recovery/request          -- Initiate social recovery
POST   /api/v1/mpc/recovery/vote             -- Submit guardian recovery vote
POST   /api/v1/mpc/recovery/cancel           -- Cancel a recovery request
GET    /api/v1/mpc/recovery/request/:id      -- Check recovery request status
```

### Testing Strategy

Recovery is inherently difficult to test because it simulates disaster scenarios. Key test approaches:

1. **Unit tests**: Backup share encrypt/decrypt round-trip with known passphrase. Guardian attestation sign/verify. Recovery vote quorum logic.
2. **Integration tests**: Full recovery flow with mock FROST wallet (3 shares, lose 2, recover via backup). Social recovery with mock guardians and mock Roko time lock.
3. **Chaos testing**: Simulate partial failures during recovery (guardian goes offline mid-vote, Roko connection drops during time lock submission). Verify idempotent retry behavior.
4. **Passphrase strength**: Test that weak passphrases are rejected (zxcvbn score < 3). Test Argon2id parameter calibration on CI hardware.

---

## References

- Buterin, V. (2021). "Why we need wide adoption of social recovery wallets." https://vitalik.eth.limo/general/2021/01/11/recovery.html
- FROST Repairable Threshold Scheme: ZCash Foundation `frost-core` resharing module
- Argon2 (RFC 9106): Password hashing and key derivation
- AES-256-GCM (NIST SP 800-38D): Authenticated encryption
- zxcvbn: Realistic password strength estimation. Rust port: `zxcvbn` crate
- Roko PoAT time-lock primitive: Internal specification (ADR-004 dependency)
