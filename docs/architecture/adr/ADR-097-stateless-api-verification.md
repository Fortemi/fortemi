# ADR-097: Stateless API Process Verification

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, reliability review TBD
**Related:** ADR-088 (plugin strategy), ADR-090 (tenancy)
**Related rules:** `.claude/rules/stateless-processes.md`, `.claude/rules/disposable-processes.md`, `.claude/rules/logs-as-event-streams.md`

## Context

Horizontal scaling of `fortemi-api` (matric-api) requires that the process hold no request-affinity state in memory. Any state that must survive a single request or job must live in a backing service (PostgreSQL, object store, queue, event bus).

The CE/EE audit (finding Sc-4, severity MODERATE) flagged this as "verification needed" — there's no evidence today that the API holds session or business state in-process, but it has not been explicitly verified. The audit hedged because the assessment was visual, not exhaustive.

For multi-tenant EE deployments, statelessness is a launch-blocking property — load balancers must be free to route any request to any pod. The local AIWG rule `stateless-processes.md` already articulates the principle; this ADR commits Fortemi to it.

## Decision

**Commit to stateless-API as an explicit architectural invariant. Audit the codebase for violations. Add CI enforcement that prevents regressions.**

### Statelessness contract

`matric-api` MUST NOT hold the following in module-level globals, instance variables, or local files:
- User session state (any per-request data spanning multiple requests)
- Authentication cache that survives restart (OAuth tokens, JWT signatures)
- Business data (notes, embeddings, links — already in PG)
- File uploads in progress (use tus-resumable upload PG state per ADR-087)
- Queue state for jobs (in PG already, via matric-jobs)
- Rate-limit counters (must move to a shared backing store if multi-instance)
- Tenant configuration (read from DB, cached with explicit TTL)

`matric-api` MAY hold:
- Connection pools (re-created on restart)
- DEK cache (`KeyProvider` per ADR-093) with explicit TTL
- Prometheus metrics counters (process-lifetime; aggregated externally)
- OpenTelemetry span context (per-request, drops at request end)
- Compiled regex / parsed config / templates (read-only after startup)
- Background job worker state (drained on shutdown per ADR-084)

### Per-process Identity

Each API process gets a unique `process_id` (UUID) at startup. Used in:
- Telemetry (`process_id` span attribute)
- Lock acquisition for singleton background tasks (with PG advisory lock)
- Distributed leader election (when needed; today only for scheduled jobs)

### Audit work

A subtask of this ADR performs an exhaustive code search for:
- `static`, `lazy_static`, `OnceCell`, `Lazy` declarations
- `Arc<Mutex<...>>` and `Arc<RwLock<...>>` declarations
- File writes outside `/tmp` or declared volume paths
- `std::sync::atomic` usage in non-counter contexts

For each finding, classify as:
- ✓ Permitted (per the list above) — document why
- ✗ Violation — file an issue to remediate

### Rate limiter migration

The current `rate_limit_middleware` uses an in-process token bucket. For multi-instance deployment, this becomes per-instance, undermining the actual rate limit. Migration options:
- Redis-backed sliding window (e.g., `tower-governor` with `redis` backend)
- PostgreSQL-backed (using LISTEN/NOTIFY for coordination — heavier but no new dep)
- Move to the EE `UsageMeter` + `QuotaPolicy` plane (ADR-092) for tenant-aware limits

Decision: CE single-instance keeps in-process rate limiter (acceptable). Multi-instance (`--features multi-instance` or via env detection) MUST use a backing store. The `QuotaPolicy` plugin per ADR-098 handles this.

### CI enforcement

Add a CI lint that:
- Scans for new `static`/`Lazy`/`OnceCell` declarations introduced in a PR
- Requires either an `#[allow(stateless_audit::lazy_init)]` opt-in attribute with comment justification or fails the build

The lint is a `clippy_utils`-based custom lint or, more pragmatically, a `cargo-deny` rule + grep-based PR check.

### Startup contract

API process MUST:
- Open all required connections within the configured startup-timeout (default 30s)
- Pass `/livez` and `/readyz` health probes only after all initialization completes
- Refuse traffic until `/readyz` is true

### Shutdown contract

API process MUST (per `.claude/rules/disposable-processes.md`):
- Handle SIGTERM
- Stop accepting new requests
- Drain in-flight requests within configured grace (default 30s, < orchestrator SIGKILL of 60s)
- Flush `AuditSink` and `UsageMeter` buffers
- Close DB connections cleanly
- Exit with code 0

## Consequences

### Positive
- (+) Multi-instance deployment becomes safe
- (+) Rolling deploys do not lose state
- (+) Crashes are recoverable (no in-memory state to reconstruct)
- (+) Codifies the invariant in CI

### Negative
- (-) Existing in-process rate limiter must move to backing store for multi-instance — operational complexity
- (-) New developer ergonomic friction: any "small cache" requires explicit justification
- (-) Custom lint maintenance

### Neutral
- (~) DEK cache and config cache are explicit exceptions with TTL — needs clear documentation

## Implementation

**Code location:** Multiple — this is a cross-cutting audit + new CI rules.

**Phases:**
1. Code audit producing a violation report (output: `.aiwg/architecture/statelessness-audit-report.md`)
2. Remediation issues per violation
3. CI lint or `cargo-deny` rule
4. Move rate-limit to backing store (gated by multi-instance feature)
5. Update operational-readiness-checklist

## References

- `.claude/rules/stateless-processes.md`
- `.claude/rules/disposable-processes.md`
- `.claude/rules/logs-as-event-streams.md`
- 12-factor app — Factor VI (Processes), Factor IX (Disposability)
- ADR-090 — Tenancy (depends on this for multi-instance)
- ADR-098 — Per-tenant rate limits (consumes multi-instance support)
