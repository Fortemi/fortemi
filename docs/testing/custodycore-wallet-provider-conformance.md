# CustodyCore Wallet Provider Conformance Plan

**Status:** Planned
**Date:** 2026-07-14
**Related:** ADR-101, Fortemi #1053, CustodyCore #207

## Purpose

Define the Fortemi-side evidence required before enabling the CustodyCore wallet
provider adapter. Tests consume published CustodyCore contract fixtures and do not
depend on a provider-specific implementation detail.

## Preconditions

- CustodyCore #213 exposes the live service boundary.
- CustodyCore #207 publishes a passing compatibility matrix and fixture set.
- Fortemi and CustodyCore agree on a supported contract version.

## Required Tests

| Area | Evidence |
|---|---|
| Contract version | Adapter accepts a compatible version and rejects an incompatible one before use. |
| Service authentication | mTLS authenticates the service identity; the short-TTL JWT fallback rejects an invalid signature, expired token, wrong audience, or unauthorized tenant/capability binding. |
| Binding propagation | Tenant, subject, capability, domain tag, payload hash, idempotency, replay, and audit-correlation fields arrive unchanged. |
| Success result | A valid signature/verification envelope is consumed only when its provider, scheme, chain, domain, and payload bindings match. |
| Structured failures | Unsigned, degraded, unknown provider/key, unsupported scheme/chain, expired, revoked, and unavailable results fail closed for proof-required work. |
| Chain intent | `sign_anchor` and `sign_settlement` requests require a decoded `custodycore.chain.tx.v1` payload and CAIP-2 chain reference; blind digests are rejected. |
| Feature flag | Disabled adapter stops the request; it never falls back to noop or an alternate signer. |
| Audit and replay | Correlation ID is observable; duplicate/replayed requests preserve the CustodyCore result and do not initiate a second signing operation. |

## Exit Criteria

- All required tests pass against a published CustodyCore fixture set.
- The Fortemi adapter makes no provider-specific proof decision outside the versioned
  contract.
- Failure and rollback tests prove that disabling the integration cannot weaken a
  proof-required operation.
- Authentication tests prove that mTLS and JWT-fallback credentials cannot be replayed
  across tenant or capability boundaries and are absent from logs and diagnostics.
- Test results link to Fortemi #1053 and CustodyCore #207.
