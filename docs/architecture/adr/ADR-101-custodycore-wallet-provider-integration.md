# ADR-101: CustodyCore Wallet Provider Integration

**Status:** Proposed
**Date:** 2026-07-14
**Deciders:** Fortemi architecture and security owners

## Context

Fortemi needs custody-backed signing and verification without taking ownership of
CustodyCore key material or introducing provider-specific branching into Fortemi
business flows. CustodyCore now owns the provider-neutral custody boundary and the
ROKO/EVM chain-signing direction in ADR-017 through ADR-019. This repository has no
Fortemi-side decision describing how a Fortemi consumer adopts that boundary.

The integration must preserve Fortemi tenant and subject context, fail closed on weak
or unavailable evidence, retain existing PKE flows, and permit an operational rollback
that never substitutes unsigned proof.

## Decision

Fortemi will consume CustodyCore through a versioned provider-neutral adapter once the
CustodyCore compatibility gate is implementation-PASS. Fortemi supplies canonical
business intent and context; CustodyCore evaluates custody policy and returns a
structured signature or verification result.

Fortemi must send the contract's required domain, payload, tenant/subject,
idempotency, replay, capability, and audit-correlation bindings. It must evaluate
structured results rather than a boolean-only success value. `noop`, unsigned,
degraded, unknown-provider, unsupported-scheme/chain, expired, revoked, and
unavailable outcomes cannot satisfy proof-required operations.

CustodyCore provider selection, key custody, consent, replay prevention, audit events,
and chain transaction construction remain outside Fortemi. Fortemi must not send blind
signing digests or directly submit CustodyCore-managed chain transactions. ROKO uses
the EVM chain path with `chain_ref = eip155:442`.

The first production integration is feature-gated and disabled by default. The concrete
endpoint mapping remains deferred until CustodyCore #213 delivers the live service
boundary and the joint adapter design can use real contract fixtures.

### Transport and Service Authentication

The adapter reuses Fortemi's established Class B/C integration profile: mTLS is the
primary service-authentication mechanism, with a short-TTL (15 minute) core-issued JWT
fallback for deployments where mTLS is unavailable. Credentials use distinct key
purposes: mTLS private keys are never reused as JWT signing keys, and the JWT signing
purpose is distinct from user, provider, and audit keys. Neither secret values nor JWT
values may be logged or passed in environment variables.

The eventual HTTP or RPC endpoint contract must authenticate before interpreting a
tenant, subject, provider, or requested capability. The Fortemi adapter must bind the
authenticated service identity to the request's tenant and allowed custody
capabilities, reject mismatches, and send credentials only in transport-protected
headers. The pending live-boundary work determines endpoint and certificate-issuance
details, not this security model.

## Consequences

### Positive
- (+) Fortemi remains wallet-provider agnostic while receiving custody-backed evidence.
- (+) Key material and provider-specific controls remain isolated in CustodyCore.
- (+) The adapter can adopt native threshold ECDSA later without a Fortemi contract
  change.

### Negative
- (-) Fortemi adoption is blocked on CustodyCore's live runtime and compatibility gate.
- (-) A new adapter, contract fixtures, and feature-flag rollout tests are required.

## Implementation

**Code Location:** Fortemi integration adapter to be introduced after CustodyCore #207
passes; CustodyCore contract and rollout source:
`roko/CustodyCore/.aiwg/frameworks/sdlc-complete/projects/default/planning/fortemi-custodycore-integration-plan-2026-07-14.md`.

**Key Changes:**
- Add a provider-neutral Fortemi adapter and configuration schema.
- Map Fortemi authorization to explicit CustodyCore requested capabilities.
- Propagate audit correlation, idempotency, and replay fields end-to-end.
- Reuse the Fortemi mTLS-primary / short-TTL-JWT-fallback service-authentication
  profile with purpose-separated key material.
- Add contract conformance and fail-closed feature-flag tests before enabling the
  adapter.

## References

- CustodyCore ADR-016 through ADR-019.
- CustodyCore Fortemi-CustodyCore Wallet Provider Integration Plan, 2026-07-14.
- `docs/testing/custodycore-wallet-provider-conformance.md`.
