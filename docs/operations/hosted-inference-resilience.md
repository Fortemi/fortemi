# Hosted Inference Circuit Breaker

This runbook covers the #1099 shared-state circuit breaker for hosted stored-secret generation,
SSE, and embedding. It does not change Community Edition routing or its local/transient BYOK
contract. Hosted requests still resolve credentials through forced RLS and KMS, enforce the shared
#920 outbound destination policy, and validate the approved model before acquiring breaker state.

## Scope And Configuration

Breaker state is shared in-process by the exact trusted tuple of tenant, user, secret row, provider,
and approved model. The tuple is hashed with unambiguous length framing and never appears in logs,
errors, or debug output. Different models remain isolated. The registry is bounded with LRU eviction
to prevent unbounded cardinality.

- `FORTEMI_INFERENCE_BREAKER_FAILURE_THRESHOLD`: default `3`, range `1..=100`
- `FORTEMI_INFERENCE_BREAKER_COOLDOWN_SECS`: default `30`, range `1..=3600`
- `FORTEMI_INFERENCE_BREAKER_CAPACITY`: default `4096`, range `1..=65536`

Invalid hosted configuration fails startup. Community Edition does not construct the registry. A
hosted process missing the registry fails stored-secret inference closed with the same generic,
non-enumerating unavailable response.

## State Behavior

Provider request-start failures, generation failures, SSE chunk failures, and embedding failures
increment consecutive provider failures. At threshold, the scope opens and subsequent calls fail
before an outbound request. After cooldown, exactly one half-open probe is admitted across all three
surfaces. Probe success closes and resets the scope; probe failure reopens it. An SSE client
disconnect does not blame the provider: an inconclusive half-open probe is abandoned and the
cooldown restarts without incrementing failures.

Credential lookup, revoked/missing rows, KMS unwrap, quota, audit, model validation, destination
policy, and backend construction failures occur before breaker acquisition and do not poison
provider health. Client-facing circuit failures are generic and cannot identify whether another
tenant, user, secret, provider account, or model exists.

State is process-local and intentionally not durable. A process restart clears it; normal recovery
is the single successful half-open probe. Capacity eviction also resets one inactive scope, so the
approved-model gate and bounded capacity are part of cardinality control. There is no public or
tenant-wide reset route.

## Release Evidence

CI receipts must cover shared generation/SSE/embed state, exact single-probe behavior, client
disconnect neutrality, scope isolation, bounded eviction, redacted debug, and CE compilation. A
production receipt must additionally induce provider failure through an approved non-secret test
account/destination, observe threshold fast-fail with no outbound call, restore the provider,
observe one half-open probe and recovery across all three surfaces, and verify logs/audit contain
only stable classes and bounded metadata. Deterministic local tests are not live-provider evidence.
