# Fortemi-CustodyCore Settlement and ROKO Rollout Plan

**Status:** Proposed for tracker filing
**Date:** 2026-07-15
**CustodyCore baseline:** `v2026.7.0` (CalVer)
**Tracker:** `Fortemi/fortemi`

## Objective

Complete the Fortemi-owned integration work needed to consume CustodyCore anchoring,
temporal-receipt, and direct-settlement capabilities without moving custody, chain
submission, or receipt verification into Fortemi.

Fortemi owns business intent, tenant and subject authorization, durable orchestration,
idempotency, local receipt/settlement projections, and operator-visible reconciliation.
CustodyCore owns wallet policy, consent, signing, ROKO transaction construction and
submission, temporal-receipt verification, and direct settlement execution.

## Contract Baseline

- Compatibility targets CustodyCore `v2026.7.0` and follows CalVer. Fortemi must not
  infer SemVer-style compatibility from the version string.
- ROKO uses CAIP-2 chain reference `eip155:442`.
- Anchoring and settlement use CustodyCore typed operations and structured results.
- `unsigned`, `degraded`, `unsupported`, `expired`, `revoked`, `replayed`, and
  `unavailable` results fail closed for proof-required operations.
- Mainnet enablement is not part of this plan. It requires a separate operations
  approval after testnet evidence is accepted.

## Existing-Issue Audit

| Issue | Decision | Required maintenance |
|---|---|---|
| #1053 CustodyCore provider boundary | Keep and update | Remove stale blanket blockers, target `v2026.7.0`, incorporate mTLS/JWT provisioning, create the missing ADR/conformance artifacts, and link downstream work. |
| #1007 ROKO anchoring | Keep and rewrite | Make it a direct, purpose-specific Postgres outbox consumer with independent claim/lease state, receipt persistence, retry/DLQ, and reconciliation. |
| #958 provenance authorization/privacy | Keep, parallel | It remains valid. It gates receipt exposure through provenance APIs, not the internal anchoring worker or receipt projection. |
| #915 outbox backpressure | Keep, separate | It remains the generic Redis fan-out admission/retention issue and does not block a purpose-specific anchoring consumer. |
| #593 outbox-to-Redis publisher | Keep, separate | It remains valid for Redis fan-out. Anchoring must not depend on Redis or reuse `published_at` as its delivery state. |
| #896 streaming ADR refresh | Keep, separate | It governs the Redis streaming path and is not a CustodyCore integration dependency. |
| #608, #616, #617 | Leave closed | These were correctly migrated to CustodyCore and are historical context only. |

## Planned Tracker Work

### Existing #1053 - Provider Boundary

Update the issue to cover the executable Fortemi adapter against CustodyCore
`v2026.7.0`, including endpoint mapping, CalVer compatibility policy, mTLS primary
authentication, short-lived JWT fallback, host registration/scopes, exact intent
bindings, fail-closed structured outcomes, feature flags, and conformance fixtures.

### Existing #1007 - Anchoring Orchestration

Update the issue to select a direct Postgres consumer. The worker must maintain
purpose-specific claim/lease and completion state, preserve at-least-once delivery,
deduplicate through stable idempotency keys, persist temporal receipt links, expose
reconciliation state, and avoid coupling to the generic Redis publisher.

### New - Verified Direct Settlement

Fortemi will translate an authorized, immutable completed-work record and its verified
temporal receipt into CustodyCore `settlement.execute`. Fortemi persists a local
settlement projection and reconciles `requested`, `submitted`, `finalized`, and failed
states. A receipt may fund at most one settlement. The flow is direct payer-to-payee;
escrow, custody, raw transaction construction, and mainnet enablement are out of scope.

### New - Joint ROKO Testnet and Rollout Gate

Qualify one complete anchor and settlement path using real ROKO testnet dependencies,
a funded signer, independent authority/genesis inputs, real temporal receipts, and
Fortemi-to-CustodyCore service authentication. Exercise failure and rollback paths,
produce redacted evidence, and leave mainnet disabled.

## Dependency Graph

1. #1053 publishes and implements the Fortemi consumer boundary.
2. #1007 implements anchoring and receipt persistence on that boundary.
3. Verified direct settlement depends on #1053 and #1007.
4. The joint testnet gate depends on #1053, #1007, and verified direct settlement.

## Settlement Acceptance Boundary

- Settlement inputs come from authorized Fortemi records, not arbitrary client values.
- The work content hash, receipt artifact binding, payer, payee, amount, and
  `eip155:442` chain are validated before submission.
- Only a verified, unconsumed temporal receipt can authorize settlement.
- Stable idempotency returns the original result for exact retries and rejects
  conflicting reuse.
- Fortemi records CustodyCore operation ID, receipt reference, transaction hash,
  state, failure class, and audit correlation ID without recording credentials or key
  material.
- Disabling settlement stops new operations and never falls back to noop signing.

## Testnet Gate Evidence

- Contract/conformance results for the supported `v2026.7.0` CalVer baseline.
- Authenticated anchor from Fortemi outbox event through stored verified receipt.
- Authenticated settlement from verified receipt through finalized direct transfer.
- Failure injection for service/RPC outage, pending inclusion, wrong chain/authority,
  malformed or replayed receipt, conflicting idempotency, and expired/revoked service
  credentials.
- Correlation across Fortemi event, CustodyCore operation/audit event, ROKO transaction,
  temporal receipt, and settlement projection.
- Backlog, retry, dead-letter, reconciliation, and credential-expiry observability.
- Kill-switch and rollback drill proving no silent provider or signer downgrade.
- Redacted evidence packet and operations runbook; mainnet remains disabled.

