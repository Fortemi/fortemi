# MPC Wallet Production Release Gate

**Document ID:** SEC-GATE-MPC-2026-04-10  
**Date:** 2026-04-10  
**Status:** Draft - Action Required  
**Scope:** MPC wallet planning corpus review for production deployment readiness

## Executive Summary

This gate review finds **5 release-blocking issues**, **5 high-priority pre-GA issues**, and **4 medium-priority hardening issues**.

**Gate decision today:** **FAIL (do not ship to production)** until all blockers are resolved and verified.

## Release Blockers (Must Fix Before Production)

### B1. Cryptographic Contract Mismatch (Schnorr vs ECDSA, curve intent)
- Severity: Critical
- Problem: Core docs conflict on what signature algorithms are produced/accepted, creating high risk of insecure or incompatible implementation.
- Evidence:
  - `.aiwg/architecture/mpc-wallet/adr-001-mpc-protocol-selection.md` lines 53, 60, 132, 138
  - `.aiwg/planning/mpc-wallet/implementation-plan.md` lines 50, 121
  - `.aiwg/architecture/mpc-wallet/software-architecture-doc.md` line 438
- Required remediation:
  - Publish one canonical crypto profile table: operation -> curve -> algorithm -> output format -> verifier.
  - Update all ADR/plan docs to exactly match the canonical profile.
  - Add CI doc-lint check asserting no conflicting algorithm claims across docs.
- Exit criteria:
  - One approved profile document merged.
  - All referenced documents updated with no contradictions.
  - Test strategy updated to match exact profile.

### B2. Trust Commit Semantics Conflict (Anchored-before-commit vs async best-effort)
- Severity: Critical
- Problem: Some docs require temporal anchoring before commit while others allow pending/unanchored trust records.
- Evidence:
  - `.aiwg/architecture/mpc-wallet/software-architecture-doc.md` lines 177-182
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 101
  - `.aiwg/requirements/mpc-wallet/feature-intake.md` line 347
- Required remediation:
  - Choose one consistency model per operation type (strong or async).
  - Define explicit state machine: `pending_anchor`, `anchored`, `anchor_failed`.
  - Define authorization behavior when anchor state is not final.
- Exit criteria:
  - State machine published and referenced by architecture, API, and tests.
  - Integration tests cover Roko outage and reconciliation.

### B3. Device Certificate Verification Model Conflict (offline verification vs wallet liveness dependency)
- Severity: Critical
- Problem: Offline verification is a design requirement, but security requirements also require wallet reachability for write operations.
- Evidence:
  - `.aiwg/architecture/mpc-wallet/adr-003-device-certificate-model.md` line 38
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 83
- Required remediation:
  - Define offline-capable verification baseline and optional liveness enhancement separately.
  - Specify exact failure policy under partitions (read/write behavior by operation class).
- Exit criteria:
  - Policy matrix approved (online/offline/partition).
  - Tests prove deterministic behavior across partition scenarios.

### B4. Authorization Scope Contradiction (`ADMIN` capability vs no-admin requirement)
- Severity: Critical
- Problem: Security requirements ban wildcard/admin scopes but ADR defines `ADMIN`.
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 84
  - `.aiwg/architecture/mpc-wallet/adr-003-device-certificate-model.md` line 193
- Required remediation:
  - Remove `ADMIN` from model or formally revise requirement with risk acceptance.
  - Replace with explicit operation-scoped grants only.
- Exit criteria:
  - Single source-of-truth scope enum with exhaustive mapping to API operations.
  - Negative tests prove no wildcard privilege escalation.

### B5. Identity Namespace Ambiguity (`mw:` vs `mm:`)
- Severity: Critical
- Problem: Mixed address namespace usage can cause trust/encryption misbinding and operational confusion.
- Evidence:
  - `.aiwg/requirements/mpc-wallet/vision.md` line 38
  - `.aiwg/requirements/mpc-wallet/user-stories.md` lines 294-299
  - `.aiwg/architecture/mpc-wallet/software-architecture-doc.md` line 321
- Required remediation:
  - Define canonical identity taxonomy and mapping rules.
  - Define migration and coexistence behavior for all endpoints and storage tables.
- Exit criteria:
  - Namespace spec published and approved.
  - API schema and DB schema updated accordingly.
  - E2E tests cover mixed legacy+MPC recipients and trust records.

### B6. PKCS12/Generic Wallet Migration Requirement (Waived by Product Decision)
- Severity: N/A (waived)
- Decision: Product owner confirmed no production users of PKCS12/generic wallet path; migration runbook is not required for this rollout.
- Evidence:
  - Direction from request thread on 2026-04-10: "we dont need to migrate, no one has used PKCS12 wallet"
- Required remediation:
  - Add release note explicitly stating migration is intentionally omitted due to zero-user path.
  - Add pre-release validation checklist item confirming no PKCS12/generic-wallet artifacts in production datasets.
- Exit criteria:
  - Release note merged.
  - Validation check executed and attached to release evidence.

## High Priority (Pre-GA Required)

### H1. Architecture ownership drift (single-crate extension vs new crates)
- Evidence:
  - `.aiwg/requirements/mpc-wallet/feature-intake.md` line 316
  - `.aiwg/planning/mpc-wallet/implementation-plan.md` line 233
  - `.aiwg/architecture/mpc-wallet/software-architecture-doc.md` lines 252, 262
- Required action:
  - Freeze crate boundaries before implementation starts.

### H2. Certificate lifetime policy conflict (24h vs 90d vs 7-day warning)
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 80
  - `.aiwg/architecture/mpc-wallet/adr-003-device-certificate-model.md` line 218
  - `.aiwg/requirements/mpc-wallet/user-stories.md` line 276
- Required action:
  - Publish one enforceable lifetime policy and renewal UX policy.

### H3. Recovery passphrase strength requirement is not rigorously enforceable as written
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 50
- Required action:
  - Replace entropy claim wording with measurable policy plus enforced generation/validation flow.

### H4. Nonce precompute lifecycle lacks crash-safe one-time-use contract
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 35
  - `.aiwg/architecture/mpc-wallet/adr-001-mpc-protocol-selection.md` line 167
- Required action:
  - Define durable reservation/consumption semantics and replay-safe recovery.

### H5. MPC ceremony coordinator hardening insufficiently specified
- Evidence:
  - `.aiwg/planning/mpc-wallet/implementation-plan.md` line 145
  - `.aiwg/architecture/mpc-wallet/software-architecture-doc.md` line 421
- Required action:
  - Define signed transcript binding, anti-equivocation evidence handling, and relay threat model.

## Medium Priority Hardening (Can Follow Immediately After GA Freeze)

### M1. Secure delete policy should prefer crypto-erasure over `shred` assumptions
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 52

### M2. Recovery blob metadata-minimization requirement conflicts with current key file header pattern
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 51
  - `crates/matric-crypto/src/pke/key_storage.rs` lines 12-17, 44

### M3. API auth rollout priority conflict (mandatory in security requirements, optional in intake priority)
- Evidence:
  - `.aiwg/security/mpc-wallet-security-requirements.md` line 112
  - `.aiwg/requirements/mpc-wallet/feature-intake.md` line 302

### M4. Risk register quality issue (summary count mismatch)
- Evidence:
  - `.aiwg/planning/mpc-wallet/risk-register.md` line 56

## Production Gate Checklist (Pass/Fail)

1. Cryptographic profile finalized and consistent across all architecture/planning/security docs.
2. Trust anchoring commit semantics finalized with explicit state machine and API behavior.
3. Device cert verification policy finalized for online/offline/partition modes.
4. Scope model finalized with no wildcard/admin escalation path.
5. Address namespace and legacy interoperability model finalized (`mw:`/`mm:` handling).
6. PKCS12/generic-wallet path waiver documented with zero-user validation evidence.
7. MPC endpoint auth policy finalized and implemented as security requirements mandate.
8. Nonce/precompute persistence and replay safety formally specified and testable.
9. Revocation propagation SLO and tests finalized.
10. Test strategy updated to match final cryptographic and API contracts.

**Gate rule:** All 10 must be `PASS` before production release.

## Recommended Execution Sequence

1. Finalize cryptographic profile (B1), namespace model (B5), and crate boundaries (H1).
2. Finalize trust consistency model (B2) and cert verification policy (B3).
3. Finalize authorization scope model (B4) and endpoint auth model (M3/H-level dependency).
4. Record and verify B6 waiver evidence (release note + zero-user validation).
5. Lock test strategy to final contracts and run pre-GA verification suite.

## Sign-Off Roles

- Crypto Lead: B1, H3, H4, H5
- Platform Lead: B2, B3, B5, M3
- Security Lead: B4, M1, M2
- Release Manager: B6 waiver evidence, gate checklist verification

---

## Decision

Current gate outcome: **FAIL**  
Production release is **blocked** pending closure of all Release Blockers and checklist completion.
