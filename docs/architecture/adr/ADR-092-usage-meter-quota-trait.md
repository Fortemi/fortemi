# ADR-092: Usage Meter and Quota Trait

**Status:** Accepted (core contract implemented 2026-07-18)
**Date:** 2026-05-20
**Deciders:** roctinam, product/billing review TBD
**Related:** ADR-088 (plugin strategy), ADR-090 (tenancy), ADR-098 (per-tenant rate limits), #713, #714, #877
**Related docs:** `.aiwg/security/multi-tenant-threat-model.md` §8

## July 2026 checkpoint rebaseline

The core contract, CE defaults, and non-durable in-memory recorder are
implemented in `matric-core`. Durable ledger storage, runtime recorder wiring,
hosted enforcement, and external sink plugins remain later phases.

- **Decision status:** Accepted; core contract implemented.
- **Implementation phase:** Runtime recorder and hosted policy integration.
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

    /// Query current-window consumption for a resolved subject + dimension.
    async fn current(&self, subject: &UsageSubject, dim: &UsageDimension, window: TimeWindow)
        -> Result<UsageAggregate, MeteringError>;

    /// Flush in-flight events. Called at shutdown.
    async fn flush(&self, grace: Duration) -> Result<(), MeteringError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Globally unique immutable event identity.
    pub event_id: Uuid,
    /// Stable logical-operation key reused by producer retries.
    pub idempotency_key: String,
    /// When the measured operation occurred.
    pub event_time: chrono::DateTime<chrono::Utc>,
    /// When Fortemi accepted the event into its ledger.
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub measurement: UsageMeasurement,
    pub class: UsageClass,
    pub producer: UsageProducer,
    pub source: UsageSource,
    pub outcome: UsageOutcome,
    pub correlation: UsageCorrelation,
    pub attrs: UsageAttributes,
}

pub struct UsageSubject {
    pub tenant_id: Option<TenantId>,
    pub principal_id: Option<String>,
    pub client_id: Option<String>,
    pub archive_id: Option<String>,
    pub anonymous_key: Option<String>,
}

pub struct UsageQuantity {
    /// Exact decimal serialized as a canonical base-10 string.
    pub value: BigDecimal,
    pub unit: UsageUnit,
}

pub enum UsageMeasurement {
    Measured(UsageQuantity),
    Unavailable { unit: UsageUnit },
}

pub struct UsageAggregate {
    pub quantity: UsageQuantity,
    pub unavailable_events: u64,
}

pub enum UsageUnit {
    Count,
    Token,
    Byte,
    Millisecond,
    Second,
    Vector,
    CurrencyMinorUnit { currency: String },
    Custom(String),
}

pub enum UsageProducer {
    Api,
    Jobs,
    Bridge,
    Mcp,
    Realtime,
    Inference,
}

pub struct UsageCorrelation {
    pub request_id: Option<String>,
    pub job_id: Option<Uuid>,
    pub bridge_session_id: Option<String>,
    pub mcp_call_id: Option<String>,
    pub provider_call_id: Option<String>,
}

pub enum UsageClass {
    BillableActual,
    NonBillableEstimate,
    NonBillableAdmission,
    NonBillableSaturation,
    Reversal,
}

pub enum UsageSource {
    ProviderReported,
    LocalMeasured,
    Estimated,
    Cache,
    Admission,
    Unavailable,
}

pub enum UsageOutcome {
    Completed,
    ClientInterrupted,
    ProviderInterrupted,
    Denied,
    FailedBeforeUsage,
    FailedAfterPartialUsage,
    Corrected,
}

pub enum UsageDimension {
    ApiRequest,                   // count
    InferenceInputTokens,
    InferenceOutputTokens,
    CachedInputTokens,
    ReasoningOutputTokens,
    AudioInputTokens,
    AudioOutputTokens,
    StorageBytes,
    IngestRows,
    EmbeddingTokens,
    EmbeddingVectors,
    JobEnqueued,
    ActiveJob,
    ActiveStream,
    ConcurrentOperation,
    MediaProcessedSeconds,
    RealtimeAudioSeconds,
    BridgeSession,
    BridgeProviderCall,
    McpToolCall,
    SaturationSignal,
    Custom { name: String, unit: UsageUnit },
}

pub enum TimeWindow {
    LastMinute,
    LastHour,
    LastDay,
    LastMonth,
    Lifetime,
    Range { start: DateTime<Utc>, end: DateTime<Utc> },
}
```

`UsageAttributes` is not an arbitrary JSON bag. Keys are a fixed
`UsageAttributeKey` enum and each dimension has an explicit key allowlist.
String labels have a restricted character set and size bound. The core
contract rejects unknown, mismatched, URL-shaped, or oversized values before
persistence. Provider-specific usage can be attached only through the
scrubbed structure described below.

`UsageQuantity.value` uses `BigDecimal` and serializes as a canonical
base-10 string paired with an explicit unit. Producers never send binary
floating-point quantities, and consumers must reject a dimension/unit mismatch
rather than convert it implicitly. `UsageMeasurement::Unavailable` represents
missing provider usage without coercing it to numeric zero; aggregates report
the count of unavailable actual events separately.
`UsageProducer` values are stable and low-cardinality. Tenant, model, route,
principal, provider request, and operation identifiers never become producer
names.

### Trait: `QuotaPolicy`

```rust
#[async_trait]
pub trait QuotaPolicy: Send + Sync {
    /// Check whether a resolved subject has budget for a dimension+quantity.
    /// MUST be fast (<5ms in-process); MAY use stale data within a configured drift window.
    async fn check(
        &self,
        subject: &UsageSubject,
        dim: &UsageDimension,
        quantity: &UsageQuantity,
    )
        -> Result<QuotaDecision, QuotaError>;

    /// Atomically reserve estimated capacity before a costly operation.
    async fn reserve(&self, request: &QuotaReservationRequest)
        -> Result<QuotaReservation, QuotaError>;

    /// Reconcile a reservation with actual measured usage.
    async fn finalize(&self, reservation: &QuotaReservation, actual: &UsageQuantity)
        -> Result<QuotaDecision, QuotaError>;

    /// Release unused capacity after denial, cancellation, or failure.
    async fn release(&self, reservation: &QuotaReservation)
        -> Result<(), QuotaError>;
}

pub struct QuotaReservationRequest {
    pub reservation_id: Uuid,
    pub idempotency_key: String,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub estimated: UsageQuantity,
    pub expires_at: DateTime<Utc>,
}

pub struct QuotaReservation {
    pub reservation_id: Uuid,
    pub idempotency_key: String,
    pub subject: UsageSubject,
    pub dimension: UsageDimension,
    pub policy_id: String,
    pub reserved: UsageQuantity,
    pub expires_at: DateTime<Utc>,
}

pub enum QuotaDecision {
    Allow {
        remaining: Option<UsageQuantity>,
        policy_id: String,
        reset_at: Option<DateTime<Utc>>,
    },
    HardLimit {
        policy_id: String,
        retry_after: Option<Duration>,
        reset_at: Option<DateTime<Utc>>,
    },
    SoftLimit {
        remaining: Option<UsageQuantity>,
        policy_id: String,
        reset_at: Option<DateTime<Utc>>,
    },
}
```

### Event identity, ordering, and duplicate handling

Fortemi's usage ledger is the source of truth. External billing and metering
systems are downstream projections, never the only durable copy.

- `event_id` identifies one immutable ledger event.
- `idempotency_key` identifies the logical operation and event phase. Producer
  retries and sink replays reuse it; a different actual, partial, or reversal
  event receives a different phase-qualified key.
- The ledger enforces uniqueness for both identities. An identical duplicate
  is acknowledged without changing aggregates. A conflicting duplicate is a
  data-integrity error and is quarantined rather than overwritten.
- `event_time` records when usage occurred. `recorded_at` records when Fortemi
  durably accepted it. Billing windows use `event_time`; replay, lateness, and
  incident analysis use both.
- Delivery to sinks is at-least-once. Per-sink delivery state records attempts,
  acknowledgements, `exported_at`, and replay position so an outage cannot
  silently lose or duplicate billable usage.
- Source correlation fields are opaque identifiers only. They must not contain
  bearer tokens, prompts, filenames, URLs with credentials, or customer text.

### Failure, partial-operation, and billability semantics

| Outcome | Event behavior |
|---|---|
| Admission/preflight estimate | `NonBillableEstimate`; never billed as actual usage |
| Accepted request with no measured consumption yet | Optional `NonBillableAdmission` event |
| Completed provider/local operation | One or more `BillableActual` events using measured or provider-reported units |
| Cached response | `Cache` source with explicit cache policy; never silently charged as a provider call |
| Denied before execution | Non-billable denial/admission telemetry; no billable usage |
| Provider failure before consumption | Non-billable failure unless the provider reports chargeable units |
| Partial or interrupted stream | Billable actual units observed so far, marked partial; finalization reuses the same operation correlation and cannot double count |
| Successful retry/resume | New attempt correlation, but idempotency rules prevent replaying already-recorded units |
| Compensated operation | Append a `Reversal`; never mutate or delete the original ledger event |
| Unknown measurement | Preserve an explicit unavailable/unknown source state; never coerce it to zero |

Streaming dimensions define whether each event is a delta or cumulative
snapshot. A final cumulative event subtracts already-recorded partial units, or
replaces a non-billable estimate; it never re-adds prior deltas.

Quota admission checks and billable recording are separate. A pre-call estimate
may reserve capacity, but only post-call actuals or an explicit provider charge
become billable. Hosted quota checks fail closed once #714 enables enforcement.
CE `NoOpMeter` and `UnlimitedQuota` remain fail-open defaults. If durable
recording fails after a successful proxied response, Fortemi does not rewrite
that response as a failure; it emits restricted audit/telemetry and retains a
replayable outbox entry.

### Default impls (CE)

```rust
pub struct NoOpMeter;  // Records nothing
pub struct UnlimitedQuota;  // Always Allow

#[async_trait]
impl UsageMeter for NoOpMeter {
    async fn record(&self, event: &UsageEvent) -> Result<(), MeteringError> {
        event.validate()
    }
    async fn current(&self, _: &UsageSubject, dim: &UsageDimension, _: TimeWindow)
        -> Result<UsageAggregate, MeteringError>
    {
        Ok(UsageAggregate {
            quantity: UsageQuantity::zero(dim.unit())?,
            unavailable_events: 0,
        })
    }
    async fn flush(&self, _: Duration) -> Result<(), MeteringError> { Ok(()) }
}

#[async_trait]
impl QuotaPolicy for UnlimitedQuota {
    async fn check(&self, _: &UsageSubject, _: &UsageDimension, _: &UsageQuantity)
        -> Result<QuotaDecision, QuotaError>
    {
        Ok(QuotaDecision::Allow {
            remaining: None,
            policy_id: "unlimited".to_string(),
            reset_at: None,
        })
    }

    async fn reserve(&self, request: &QuotaReservationRequest)
        -> Result<QuotaReservation, QuotaError>
    {
        Ok(QuotaReservation {
            reservation_id: request.reservation_id,
            policy_id: "unlimited".to_string(),
            reserved: request.estimated.clone(),
            expires_at: request.expires_at,
        })
    }

    async fn finalize(&self, _: &QuotaReservation, _: &UsageQuantity)
        -> Result<QuotaDecision, QuotaError>
    {
        Ok(QuotaDecision::Allow {
            remaining: None,
            policy_id: "unlimited".to_string(),
            reset_at: None,
        })
    }

    async fn release(&self, _: &QuotaReservation) -> Result<(), QuotaError> {
        Ok(())
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
| Bridge/provider call | Normalized provider usage buckets and call outcome | Capability/provider budget |
| MCP dispatch | `McpToolCall` by tool class | MCP tool-class quota |

### Provider usage preservation

Provider adapters normalize usage without flattening away source detail. The
normalized record supports:

- input/prompt, output/completion, and total tokens;
- cached input tokens and cache state;
- reasoning output tokens, including non-visible reasoning units;
- audio input/output tokens or seconds;
- embedding tokens and vector counts;
- other provider-specific numeric buckets in a versioned allowlist.

Every record also carries provider, endpoint/protocol, model slug, provider
request id, usage source, completion/partial state, and the adapter schema
version. `raw_usage` may preserve a size-bounded scrubbed provider usage object
for future reconciliation. It must exclude prompts, completions, credentials,
headers, signed URLs, customer identifiers, and other unapproved response
fields.

When the provider omits usage, estimation metadata records the tokenizer/model
family, `estimator_version`, estimation phase, and method (preflight tokenizer,
request payload, streaming chunks, or posthoc approximation). Provider-reported
and estimated quantities remain distinguishable in reports and sinks.

Pricing is a separate versioned projection keyed by provider, model, endpoint,
currency, and effective interval. `unknown_price` is explicit and queryable.
Only a configured local/free policy may convert usage to a `$0` cost; missing
catalog data never silently becomes free usage.

### EE plugins and downstream sinks

- `fortemi-enterprise-billing-stripe` — projects ledger events to Stripe
  Billing Meters and Meter Events (v1 or API v2/Meter Event Streams as
  throughput requires)
- `fortemi-enterprise-billing-openmeter` — maps stable Fortemi event identity,
  producer/source, subject, and data to CloudEvents-compatible OpenMeter usage
  events
- `fortemi-enterprise-billing-warehouse` — Direct insert into customer's data warehouse (BigQuery, Snowflake)
- `fortemi-enterprise-quota-static` — Static per-plan quotas from configuration
- `fortemi-enterprise-quota-dynamic` — Quotas served from a control-plane API

Stripe legacy usage records are migration context only. New integrations use
Billing Meters/Meter Events and retain Fortemi event identity, recorded time,
delivery state, `exported_at`, and replay history locally. Sink aggregation
(sum/count/max, billing period, grace period) does not redefine the immutable
raw event.

A typical EE deployment composes one `UsageMeter` + one `QuotaPolicy`. CE deployments may install either independently (e.g., metering for observability without billing; quotas without metering for free-tier protection).

### Quota check vs record relationship

`check` is a read-only advisory operation. Any path that can spend a limited
budget uses the atomic lifecycle below, including idempotent paths that may run
concurrently:

1. `reserve` an estimate with a stable reservation identity and expiry.
2. Proceed only after the reservation succeeds.
3. `finalize` exactly once with actual measured usage. This reconciles unused
   capacity or atomically attempts to acquire any excess.
4. `release` on denial, cancellation, or failure before finalization. Expired
   reservations are also released by policy.
5. `record` the separate immutable actual ledger event; reservation state is
   never treated as billable usage.

Hosted hard-cap policies fail closed when an atomic reservation cannot be
established. `UnlimitedQuota` implements the same lifecycle as a no-op. This
prevents concurrent requests from all passing stale read-only checks and
overspending one budget while keeping recorded billing based on actual usage.

### Privacy and retention

The implemented ledger-specific store inventory, runtime modes, deletion
limits, backup beyond-use posture, and verification contract are defined in
[`usage-ledger-retention.md`](../usage-ledger-retention.md). The broader DSAR
catalog and legal/operator decisions remain owned by #900.

- Subject identifiers use internal opaque IDs or keyed pseudonyms. Email
  addresses, display names, IP addresses, and raw external account IDs are not
  general-purpose usage attributes.
- Attribute keys and value types are allowlisted by event schema. Values are
  length-bounded and sanitized before ledger or sink delivery.
- Prompts, completions, note bodies, filenames, authorization headers, API
  keys, cookies, provider request headers, and credential-bearing URLs are
  forbidden in both `attrs` and `raw_usage`.
- Provider call IDs are restricted correlation data. They are pseudonymized or
  omitted in external sinks and are not sink grouping dimensions by default.
- Provider/model slugs and error details use the same telemetry classification
  and redaction rules as security-sensitive audit data. Stable reason codes
  replace raw provider error messages.
- Ledger, raw provider sidecar, delivery-attempt, and aggregate retention are
  separate policies. A billing retention need does not authorize indefinite
  retention of raw provider payloads.
- Usage events and security audit events may share correlation IDs but remain
  separate stores and purposes. Audit logs are not the billing ledger.

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
- (-) Durable hosted recording requires a Fortemi-owned ledger/outbox before
  asynchronous sink delivery; direct best-effort sink calls are insufficient

### Neutral
- (~) Aggregation windows (per-hour vs per-month) are sink-defined; core just records events with timestamps

## Implementation

**Code location:**
- Traits, types, CE defaults, and in-memory recorder:
  `crates/matric-core/src/metering.rs`
- Middleware: `crates/matric-api/src/middleware/quota.rs` (new, used in ADR-098)
- EE plugins: separate `fortemi-enterprise-billing-*`, `fortemi-enterprise-quota-*` crates

**Phases:**
1. Land traits + CE defaults + in-memory contract recorder (complete)
2. Wire `record` into router and inference paths
3. Wire `check` into router (gated behind `multi-tenant` feature)
4. First EE billing plugin (`fortemi-enterprise-billing-openmeter`)
5. First EE quota plugin (`fortemi-enterprise-quota-static`)

**Testing:**
- Property test: total recorded usage = sum of operations in a fuzz scenario
- Duplicate/replay test: repeated event/idempotency identities do not change totals
- Conflict test: a reused identity with different content is quarantined
- Partial/reversal test: interrupted streams and compensation cannot double count
- Quantity test: canonical decimal precision and dimension/unit compatibility
  are enforced without binary floating-point conversion
- Reservation test: concurrent reserve/finalize/release, expiry, retry, and
  actual-over-estimate paths cannot overspend or leak capacity
- Privacy test: forbidden attributes and unsanitized raw provider usage are rejected
- Pricing test: unknown price remains distinct from configured local `$0`
- Load test: 10k req/s with metering enabled adds <5% latency

## References

- ADR-088, ADR-090 — Plugin model, tenancy
- ADR-098 — Per-tenant rate limits and quotas (uses this trait)
- `Fortemi/fortemi#713` — contract and CE defaults
- `Fortemi/fortemi#714` — hosted enforcement
- `Fortemi/fortemi#877` — bridge/provider usage specialization
- `.aiwg/security/multi-tenant-threat-model.md` §8
- [OpenMeter usage events](https://openmeter.io/docs/metering/events/usage-events)
- [Stripe Billing Meters](https://docs.stripe.com/api/billing/meter)
- [Stripe Meter Events v1](https://docs.stripe.com/api/billing/meter-event)
- [Stripe Meter Events API v2](https://docs.stripe.com/api/v2/meter-events)
- [Stripe Meter Event Streams](https://docs.stripe.com/api/v2/meter-event-streams)
- [Stripe legacy usage-record migration](https://docs.stripe.com/billing/subscriptions/usage-based-legacy/migration-guide)
