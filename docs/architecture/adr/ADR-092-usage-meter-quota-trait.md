# ADR-092: Usage Meter and Quota Trait

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, product/billing review TBD
**Related:** ADR-088 (plugin strategy), ADR-090 (tenancy), ADR-098 (per-tenant rate limits)
**Related docs:** `.aiwg/security/multi-tenant-threat-model.md` §8

## July 2026 checkpoint rebaseline

This ADR remains design-only at the July 2026 checkpoint. `UsageMeter`, `QuotaPolicy`, `NoOpMeter`, and `UnlimitedQuota` were documented, but no implementation was found in `crates/`.

- **Decision status:** Proposed; design only.
- **Implementation phase:** Core metering/quota contract construction.
- **Phase owner:** `Fortemi/fortemi#713`, with private billing integration in `Fortemi-Enterprise/billing#1`.
- **Checkpoint decision date:** 2026-07-14.

## Context

Fortemi today has a single process-wide rate limiter (`rate_limit_middleware`). It has no concept of:
- Per-tenant request counts
- Per-tenant token consumption (input/output for LLM operations)
- Per-tenant storage in bytes
- Per-tenant job-queue depth
- Aggregation of usage across time windows
- Communication of usage to a billing pipeline

For multi-tenant hosted EE, this is a launch-blocking gap:
- Without metering, billing is impossible
- Without quotas, a noisy-neighbor tenant degrades the platform for all
- Without per-tenant aggregation, capacity planning is blind

This ADR defines the **measurement and decision** surface. The actual rate-limit middleware lives in ADR-098. The billing-pipeline integration (Stripe Metering, OpenMeter, etc.) is an EE plugin.

## Decision

**Introduce two pluggable traits in `matric-core`: `UsageMeter` (records consumption) and `QuotaPolicy` (decides whether a request is over budget). Ship `NoOpMeter` and `UnlimitedQuota` as CE defaults.**

### Trait: `UsageMeter`

```rust
// crates/matric-core/src/metering.rs

#[async_trait]
pub trait UsageMeter: Send + Sync {
    /// Record a usage event. MUST be non-blocking on the request path
    /// (typically queues to an internal buffer; aggregator flushes async).
    async fn record(&self, event: &UsageEvent) -> Result<(), MeteringError>;

    /// Query current-window consumption for a tenant + dimension.
    async fn current(&self, tenant: &TenantId, dim: &UsageDimension, window: TimeWindow)
        -> Result<u64, MeteringError>;

    /// Flush in-flight events. Called at shutdown.
    async fn flush(&self, grace: Duration) -> Result<(), MeteringError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub tenant_id: TenantId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub dimension: UsageDimension,
    pub quantity: u64,
    pub attrs: HashMap<String, serde_json::Value>,
}

pub enum UsageDimension {
    ApiRequest,                   // count
    TokensInput { model: String },
    TokensOutput { model: String },
    StorageBytes,
    EmbeddingsCount,
    JobsEnqueued { job_kind: String },
    MediaProcessedSeconds { kind: MediaKind },
    Custom(String),
}

pub enum TimeWindow {
    LastMinute,
    LastHour,
    LastDay,
    LastMonth,
    Lifetime,
    Custom(Duration),
}
```

### Trait: `QuotaPolicy`

```rust
#[async_trait]
pub trait QuotaPolicy: Send + Sync {
    /// Check whether a tenant has budget remaining for a given dimension+quantity.
    /// MUST be fast (<5ms in-process); MAY use stale data within a configured drift window.
    async fn check(&self, tenant: &TenantId, dim: &UsageDimension, quantity: u64)
        -> Result<QuotaDecision, QuotaError>;
}

pub enum QuotaDecision {
    /// Within budget. Soft consumption count attached for caller telemetry.
    Allow { remaining: Option<u64> },
    /// Over hard cap. Caller MUST reject the request.
    Hard,
    /// Over soft cap. Caller MAY proceed but with reduced QoS (queue priority, lower rate-limit ceiling).
    Soft { remaining: Option<u64> },
}
```

### Default impls (CE)

```rust
pub struct NoOpMeter;  // Records nothing
pub struct UnlimitedQuota;  // Always Allow

#[async_trait]
impl UsageMeter for NoOpMeter {
    async fn record(&self, _e: &UsageEvent) -> Result<(), MeteringError> { Ok(()) }
    async fn current(&self, _: &TenantId, _: &UsageDimension, _: TimeWindow)
        -> Result<u64, MeteringError> { Ok(0) }
    async fn flush(&self, _: Duration) -> Result<(), MeteringError> { Ok(()) }
}

#[async_trait]
impl QuotaPolicy for UnlimitedQuota {
    async fn check(&self, _: &TenantId, _: &UsageDimension, _: u64)
        -> Result<QuotaDecision, QuotaError>
    {
        Ok(QuotaDecision::Allow { remaining: None })
    }
}
```

### Wire-in points

| Surface | Records | Checks |
|---|---|---|
| Router middleware (every request) | `ApiRequest` | `ApiRequest` quota |
| Inference call site | `TokensInput`, `TokensOutput` | `TokensInput` (pre-call estimate) |
| Embedding call site | `EmbeddingsCount`, `TokensInput` | `EmbeddingsCount` |
| Storage write | `StorageBytes` | `StorageBytes` (cumulative) |
| Job dispatcher | `JobsEnqueued` | `JobsEnqueued` (queue depth) |
| Media extraction | `MediaProcessedSeconds` | `MediaProcessedSeconds` |

### EE plugins

- `fortemi-enterprise-billing-stripe` — `UsageMeter` flushes events to Stripe Metering API
- `fortemi-enterprise-billing-openmeter` — Self-hosted OpenMeter integration
- `fortemi-enterprise-billing-warehouse` — Direct insert into customer's data warehouse (BigQuery, Snowflake)
- `fortemi-enterprise-quota-static` — Static per-plan quotas from configuration
- `fortemi-enterprise-quota-dynamic` — Quotas served from a control-plane API

A typical EE deployment composes one `UsageMeter` + one `QuotaPolicy`. CE deployments may install either independently (e.g., metering for observability without billing; quotas without metering for free-tier protection).

### Quota check vs record relationship

For idempotent operations, the pattern is:
1. `check(tenant, dim, estimated_quantity)` → if `Hard`, reject
2. Proceed with operation
3. `record(actual_event)` after operation

For non-idempotent or pre-paid operations:
1. Reserve quota (atomic check+record) — implementations MAY expose `try_reserve` for this
2. On success, proceed; on failure, release if necessary

The split is to keep the hot-path fast (check is read-mostly) while allowing accurate billing (record reflects actual).

## Consequences

### Positive
- (+) Per-tenant metering and quota became first-class concepts
- (+) Billing is an EE plugin, not a fork of core
- (+) Noisy-neighbor mitigation via hard/soft quotas
- (+) Compatible with current rate-limit middleware (ADR-098 replaces it)
- (+) CE retains current "unlimited" behavior explicitly

### Negative
- (-) Per-request quota check adds latency (target <2ms with in-memory cache; degrades to ~10ms for cold cache hit)
- (-) Two traits = two plugins to maintain — but they often pair naturally
- (-) Pre-call token estimation is imprecise (model tokenization varies); reconcile via `record` post-call
- (-) Records flushed asynchronously can lose data on crash; mitigated by EE plugins writing to durable queue first

### Neutral
- (~) Aggregation windows (per-hour vs per-month) are sink-defined; core just records events with timestamps

## Implementation

**Code location:**
- Trait + types: `crates/matric-core/src/metering.rs` (new)
- Default impls: `crates/matric-core/src/metering/no_op.rs`
- Middleware: `crates/matric-api/src/middleware/quota.rs` (new, used in ADR-098)
- EE plugins: separate `fortemi-enterprise-billing-*`, `fortemi-enterprise-quota-*` crates

**Phases:**
1. Land traits + NoOp impls
2. Wire `record` into router and inference paths
3. Wire `check` into router (gated behind `multi-tenant` feature)
4. First EE billing plugin (`fortemi-enterprise-billing-openmeter`)
5. First EE quota plugin (`fortemi-enterprise-quota-static`)

**Testing:**
- Property test: total recorded usage = sum of operations in a fuzz scenario
- Load test: 10k req/s with metering enabled adds <5% latency

## References

- ADR-088, ADR-090 — Plugin model, tenancy
- ADR-098 — Per-tenant rate limits and quotas (uses this trait)
- `.aiwg/security/multi-tenant-threat-model.md` §8
- OpenMeter docs (cloud-events aligned metering schema reference)
- Stripe Metering API (one of the target EE backends)
