# MPC Wallet Prioritized Implementation Task List

**Date:** 2026-04-10  
**Source:** Security release-gate findings + architecture alignment decisions  
**Status:** Active backlog

## P0 - Blockers (Do First)

### T1. Publish canonical cryptographic profile
- Priority: P0
- Owner: Crypto Lead
- Scope:
  - Add one authoritative table: operation -> curve -> algorithm -> encoding -> verifier.
  - Enforce: FROST threshold outputs are Schnorr-family only (Ed25519, secp256k1-Schnorr).
  - Keep ECDSA secp256k1 explicitly for Roko receipt verification only.
- Deliverables:
  - New profile doc under `.aiwg/architecture/mpc-wallet/`.
  - Cross-reference from implementation plan, vision, test strategy, security requirements.
- Done when:
  - No conflicting ECDSA vs Schnorr statements remain in core planning docs.

### T2. Lock trust anchoring state model
- Priority: P0
- Owner: Platform Lead
- Scope:
  - Define and document `pending_anchor`, `anchored`, `anchor_failed`.
  - Define allowed operations/authorization behavior per state.
  - Add retry and reconciliation behavior (Roko outage and recovery).
- Deliverables:
  - Updated architecture + API contract + DB schema notes.
  - Integration test cases for outage/recovery transitions.
- Done when:
  - All trust-mutating operations have deterministic behavior without Roko availability.

### T3. Finalize offline-first certificate validation policy
- Priority: P0
- Owner: Security Lead
- Scope:
  - Make signature+time+fresh revocation cache sufficient for authorization.
  - Define stale revocation cache policy (`revocation_stale` behavior).
- Deliverables:
  - Updated security requirements + middleware behavior spec.
  - Tests for online/offline/partition modes.
- Done when:
  - No hard dependency on wallet reachability in cert hot path.

### T4. Remove admin/wildcard authorization path
- Priority: P0
- Owner: Security Lead
- Scope:
  - Remove `ADMIN`/wildcard semantics from cert capability model.
  - Map every API operation to explicit scope enum variant.
- Deliverables:
  - Scope enum source-of-truth doc and code contract.
  - Exhaustive match tests and privilege-escalation negative tests.
- Done when:
  - Scope model has no bypass scope and all operations are explicitly mapped.

### T5. Standardize identity namespace (`mw:` vs `mm:`)
- Priority: P0
- Owner: Platform Lead
- Scope:
  - Define `mw:` for MPC identity/trust operations and `mm:` for legacy PKE recipient addressing.
  - Define mixed-mode interoperability behavior and validation rules.
- Deliverables:
  - Namespace rules in architecture + API + schema docs.
  - E2E tests for mixed legacy/MPC recipients.
- Done when:
  - Address type usage is unambiguous across trust, cert, and encryption flows.

## P1 - Pre-GA Hard Requirements

### T6. Freeze crate boundaries and module ownership
- Priority: P1
- Owner: Architecture Team
- Scope:
  - Resolve `matric-crypto only` vs `new crates` split.
  - Publish final crate ownership matrix and dependency boundaries.

### T7. Finalize certificate lifetime policy
- Priority: P1
- Owner: Platform + Product
- Scope:
  - Align all docs on default/max cert TTL and renewal timing.
  - Align warning/notification expectations with selected TTL.

### T8. Formalize nonce precompute lifecycle
- Priority: P1
- Owner: Crypto Lead
- Scope:
  - Define durable reservation/consumption semantics.
  - Define crash recovery without nonce reuse.

### T9. Strengthen coordinator threat model and transcript integrity
- Priority: P1
- Owner: Crypto + Security
- Scope:
  - Define signed transcript hash, equivocation evidence retention, and replay protections.

### T10. Release waiver implementation for unused PKCS12 path
- Priority: P1
- Owner: Release Manager
- Scope:
  - Add release note documenting waiver.
  - Add pre-release evidence check proving zero production PKCS12 usage.

## Verification Work Items

### V1. Update test strategy for canonical crypto profile
- Priority: P0
- Owner: QA + Crypto

### V2. Add state-machine integration tests for anchor lifecycle
- Priority: P0
- Owner: QA + Platform

### V3. Add partition-mode certificate auth tests
- Priority: P0
- Owner: QA + Security

### V4. Add scope mapping completeness tests
- Priority: P0
- Owner: QA + API

### V5. Add mixed-namespace E2E tests (`mw:` + `mm:`)
- Priority: P0
- Owner: QA + API

## Recommended Execution Order

1. T1, T5, T6 (contract + ownership freeze)
2. T2, T3, T4 (runtime security behavior)
3. T7, T8, T9 (policy and protocol hardening)
4. V1-V5 (verification suite lock-in)
5. T10 (release evidence and waiver closure)

