# ADR-098: Per-Tenant Rate Limits and Quotas

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, reliability/product review TBD
**Related:** ADR-088, ADR-090 (tenancy), ADR-092 (UsageMeter+QuotaPolicy), ADR-097 (statelessness)

## July 2026 checkpoint rebaseline

This ADR remains design-only at the July 2026 checkpoint. The quota/rate-limit design depends on ADR-092, and no `UsageMeter` or `QuotaPolicy` implementation was found in `crates/`.

- **Decision status:** Proposed; design only and dependency-blocked.
- **Implementation phase:** Per-tenant quota middleware after ADR-092 construction.
- **Phase owner:** `Fortemi/fortemi#714`, with private billing integration in `Fortemi-Enterprise/billing#1`.
- **Checkpoint decision date:** 2026-07-14.

## Context

Fortemi today has a single process-wide rate limiter (`rate_limit_middleware`). This:
- Does not distinguish tenants
- Is process-local (multi-instance becomes per-instance, not aggregate)
- Has no concept of token-budget, storage-cap, or job-queue-cap
- Cannot enforce noisy-neighbor mitigation

For multi-tenant EE, the rate-limit and quota plane is critical. ADR-092 defines the trait surface (`UsageMeter`, `QuotaPolicy`). This ADR specifies **what to enforce, where, and how**.

## Decision

**Replace the process-wide rate limiter with a per-tenant, multi-dimensional quota + rate-limit middleware backed by the `UsageMeter` + `QuotaPolicy` traits. Use Redis (or alternative shared store) for multi-instance coordination.**

### Enforcement dimensions

| Dimension | Default CE | EE default | Notes |
|---|---|---|---|
| Requests / second per tenant | unlimited | 100 (free tier), 1000 (paid) | Burst allowance via token bucket |
| Tokens / hour per tenant (LLM input+output) | unlimited | tier-defined | Pre-call estimate + post-call reconcile |
| Storage bytes per tenant | unlimited | tier-defined | Block writes when exceeded |
| Concurrent jobs per tenant | 32 | tier-defined | Queue length + in-flight |
| Embeddings count per tenant | unlimited | tier-defined | Per month |
| Media-processing seconds per tenant | unlimited | tier-defined | Per month |
| Concurrent connections per principal | 32 | 32 | Per session |
| Requests / minute per IP | 600 | 600 | DoS protection, IP-level |

### Middleware stack (order matters)

```
Request
  → IP rate limit (process-local OK for L7 DoS)
  → Authentication (ADR-071)
  → Tenant resolution (ADR-090)
  → Per-tenant quota check (this ADR; QuotaPolicy::check)
  → Authorization (ADR-089)
  → Handler
  → On response: UsageMeter::record (this ADR; ADR-092 trait)
```

Failure modes:
- IP rate limit exceeded → 429 with `Retry-After`
- Tenant quota exceeded (Hard) → 429 with `X-Quota-Dimension` header
- Tenant quota soft-exceeded → 200 with `X-Quota-Warning` header (informational)

### Backing store

CE single-instance: in-process token bucket. Acceptable because there is only one process, and CE has no quotas anyway.

EE / multi-instance:
- **Redis** (primary recommendation): atomic INCR with PEXPIRE for sliding windows; well-supported in Rust ecosystem (`tower-governor`, `redis-rs`); ops familiar
- **PostgreSQL** (alternative): leverage existing PG cluster; `advisory locks` + table-based counters; higher latency but no new dep

The choice is configurable via a `QuotaPolicy` impl. The trait abstracts the backing store from the middleware.

### Rate-limit response headers

Per standard practice (RFC 6585, draft-ietf-httpapi-ratelimit-headers):

```
HTTP/1.1 200 OK
RateLimit-Limit: 1000
RateLimit-Remaining: 873
RateLimit-Reset: 60
RateLimit-Policy: "1000;w=60"

# On 429:
HTTP/1.1 429 Too Many Requests
Retry-After: 12
X-Quota-Dimension: api_request
X-Quota-Reset: 2026-05-20T15:00:00Z
```

### Burst allowance

Token bucket with `bucket_size = limit * 2`, `refill_rate = limit / window`. Allows 2× short bursts; sustained rate matches the limit.

### Quota dimensions persistence

`UsageMeter` records to:
- Hot tier: Redis counters with TTL matching window
- Warm tier: PG `usage_events` table for the current month
- Cold tier: archived to object store (S3) or warehoused (Snowflake/BigQuery) by EE billing plugins

### Tier configuration

Tenant plans (free/team/enterprise) and their limits live in a `tenant_plans` table. A `QuotaPolicy` plugin reads from this table; EE customers may swap to a control-plane API.

### MCP-specific quotas

MCP tools (43 of them per README) may have per-tool quotas distinct from general API limits. ADR-100 (MCP scope gate) extends this with tool-specific authorization and rate limits.

## Consequences

### Positive
- (+) Noisy-neighbor mitigation
- (+) Standards-compliant rate-limit headers
- (+) Per-tenant visibility for capacity planning
- (+) Integrates with billing pipeline (UsageMeter)
- (+) Multi-instance safe

### Negative
- (-) New backing-store dependency (Redis or PG-based) for EE multi-instance
- (-) ~1-3ms additional latency per request (Redis call, cached on hot path)
- (-) Tenant plan migration tooling required when limits change
- (-) Customer pushback on first 429 — must include diagnostic headers for self-service investigation

### Neutral
- (~) Per-IP rate limit unchanged in CE
- (~) "Unlimited" CE defaults preserved

## Implementation

**Code location:**
- Middleware: `crates/matric-api/src/middleware/quota.rs` (new)
- QuotaPolicy impls: in EE crates per ADR-088 (`fortemi-enterprise-quota-static`, `-dynamic`)

**Phases:**
1. Land middleware shell with `UnlimitedQuota` default
2. Add Redis-backed quota policy as a feature-gated CE option
3. Land tier plans table + read-through cache
4. Cut over from old `rate_limit_middleware` to new quota middleware
5. EE plans served via control-plane

## References

- ADR-088, ADR-090, ADR-092
- RFC 6585 — 429 status code
- IETF draft — `draft-ietf-httpapi-ratelimit-headers`
- Redis token-bucket / sliding-window patterns
