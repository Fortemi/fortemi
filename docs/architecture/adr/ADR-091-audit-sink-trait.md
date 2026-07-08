# ADR-091: Audit Sink Trait

**Status:** Proposed
**Date:** 2026-05-20
**Deciders:** roctinam, compliance/security review TBD
**Related:** ADR-088 (plugin strategy), ADR-089 (authorization), ADR-090 (tenancy), ADR-093 (key provider)
**Related docs:** `.aiwg/architecture/plugin-contract-spec.md` §10, `.aiwg/security/multi-tenant-threat-model.md` §7

## July 2026 checkpoint rebaseline

The core audit seam is partially implemented: `AuditEvent`, `AuditSink`, `TracingSink`, bounded buffering, and many metadata-only API audit producers exist. This ADR is not yet a compliance-ready hosted audit claim because mandatory hosted audit health, KMS lifecycle audit, tamper-evident retention, and private EE sinks remain gated by `Fortemi/fortemi#1019` and `Fortemi-Enterprise/audit-sinks#2`.

## Context

Fortemi today logs operationally via `tracing`. There is no concept of an **audit event** as a first-class, tamper-evident, queryable record of security-relevant operations.

Compliance frameworks Fortemi will need to support for enterprise customers (SOC 2 Type II, ISO 27001, HIPAA, GDPR Art. 30 records of processing) all require:
- A record of authentication events (success and failure)
- A record of authorization decisions (allow/deny with reason)
- A record of access to sensitive data (PII, PHI)
- A record of administrative actions (cross-tenant access, configuration changes, key rotations)
- A record of data exports and deletions (DSAR)
- Integrity protection (append-only, tamper-evident)
- Retention policy enforcement

Operationally, customer SIEMs (Splunk, Elastic, Datadog, Sumo Logic) expect to receive these events on a stable schema.

The CE/EE audit (finding S-7) classifies this as a HIGH-priority gap.

## Decision

**Introduce a pluggable `AuditSink` trait in `matric-core` with a `TracingSink` default for CE and EE-provided sinks for SIEM/S3-WORM/Datadog.**

### Audit event schema

```rust
// crates/matric-core/src/audit.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Server-assigned monotonic ULID. Used for ordering and idempotency.
    pub id: AuditEventId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tenant_id: Option<TenantId>,
    pub principal: Option<AuthPrincipal>,
    pub category: AuditCategory,
    pub action: String,                           // e.g., "auth.login_success"
    pub resource: Option<AuditResource>,
    pub outcome: AuditOutcome,                    // Success | Failure | Indeterminate
    pub reason: Option<String>,
    pub source_ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,                 // OpenTelemetry trace id
    pub attrs: HashMap<String, serde_json::Value>,
    pub severity: AuditSeverity,                  // Info | Notice | Warn | Critical
}

pub enum AuditCategory {
    Auth,           // login, logout, token issuance/revocation
    Authorization,  // allow/deny decisions
    Data,           // read/write/delete of user content
    Admin,          // configuration changes, user management
    System,         // process start/stop, plugin load/unload
    Key,            // key use, rotation, export attempt
    Privacy,        // DSAR events
}
```

### Trait

```rust
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Emit one or more events. Implementations MUST be idempotent under
    /// the provided `idempotency_key` — repeat invocations with the same
    /// key MUST NOT duplicate events.
    async fn emit(&self, events: &[AuditEvent], idempotency_key: &str)
        -> Result<(), AuditError>;

    /// Drain in-flight events. Called at shutdown with a grace window.
    async fn flush(&self, grace: Duration) -> Result<(), AuditError>;
}
```

### Default impl (CE)

```rust
pub struct TracingSink;

#[async_trait]
impl AuditSink for TracingSink {
    async fn emit(&self, events: &[AuditEvent], _: &str) -> Result<(), AuditError> {
        for event in events {
            tracing::info!(
                target: "fortemi.audit",
                event = ?event,
                "audit"
            );
        }
        Ok(())
    }

    async fn flush(&self, _: Duration) -> Result<(), AuditError> { Ok(()) }
}
```

CE behavior: events go to `tracing` (typically stdout JSON) under a dedicated target. Operators are responsible for shipping `fortemi.audit`-tagged log entries to a tamper-evident store of their choice.

### Mandatory audit events

Per `.aiwg/architecture/plugin-contract-spec.md` §10, plug-points declare a list of events they MUST emit. Initial set:

| Surface | Required event | Severity |
|---|---|---|
| Auth middleware | `auth.login_success`, `auth.login_failure` | Info / Notice |
| Auth middleware | `auth.token_issued`, `auth.token_revoked` | Info |
| AuthorizationPolicy | `auth.decision` (per request when ALLOW), `auth.decision_deny` | Info / Notice |
| AuthorizationPolicy | `auth.indeterminate`, `auth.policy_error` | Warn / Critical |
| TenantScopedDb | `system.cross_tenant_access` | Critical |
| KeyProvider | `key.use`, `key.rotate`, `key.export_attempt` | Info / Notice / Critical |
| DataSubjectRequestHandler | `privacy.dsar_received`, `privacy.dsar_completed` | Notice |
| MCP server | `mcp.tool_invoked`, `mcp.tool_denied` | Info / Notice |
| Admin endpoints | `admin.config_changed`, `admin.user_role_changed` | Notice |
| Plugin lifecycle | `plugin.loaded`, `plugin.unloaded`, `plugin.health_check_failed` | Info / Critical |

### Buffering and reliability

The core wraps the configured `AuditSink` in an `AuditBuffer` that:
- Batches events (default: flush every 1s or 100 events)
- Buffers up to N events on transient sink failure (default 10_000)
- Drops oldest with a `audit.buffer_overflow` warning on prolonged failure
- Persists buffer to disk on shutdown if sink unreachable (CE option)
- For EE: a "guaranteed delivery" mode writes events to a local durable queue (sqlite or PG `audit_outbox` table) before acknowledging the request

### Tamper-evidence (optional, EE)

`AuditSink` impls MAY implement hash-chaining: each event embeds `prev_hash = sha256(prev_event_canonical_json)`. This is opt-in via configuration and is not the core's responsibility — implementations like `S3WormSink` use S3 Object Lock; `SplunkSink` uses Splunk indexing integrity.

The hash-chain pattern is documented in `.aiwg/security/multi-tenant-threat-model.md` §7 for vendors who want to implement it.

### Retention

Retention is **not** the core's responsibility. The configured sink determines retention. CE deployments with `TracingSink` should configure their log pipeline (Loki, CloudWatch Logs, Elastic) with the appropriate retention.

## Consequences

### Positive
- (+) Single, well-defined audit event schema across all surfaces
- (+) CE has a usable default (tracing) without forcing operators to install EE
- (+) EE plugins compose: SIEM forwarding + S3-WORM long-term + custom internal warehouse
- (+) Idempotency-keyed `emit` enables safe retries
- (+) Compliance-ready surface (SOC 2, ISO, HIPAA) when paired with an appropriate sink
- (+) Plugin authors get a free audit channel via lifecycle events

### Negative
- (-) Per-request audit emission adds latency; mitigated by buffered batching (default 1ms additional tail)
- (-) Audit volume can be large; ops must size sink throughput accordingly
- (-) "Default tracing" CE behavior is convenient but operators MUST configure log shipping or audit is effectively absent
- (-) Plugins must remember to call `audit::emit` for their mandatory events; mitigated by clippy-style lints

### Neutral
- (~) Some events (e.g., `auth.decision` for read-only routes at high volume) may be sampled or rolled-up in EE; configurable in sink

## Implementation

**Code location:**
- Trait + types: `crates/matric-core/src/audit.rs` (new)
- Default sink: `crates/matric-core/src/audit/tracing_sink.rs` (new)
- Buffer: `crates/matric-core/src/audit/buffer.rs` (new)
- EE sinks: separate `fortemi-enterprise-audit-*` crates (jsonl, splunk, elastic, s3-worm, datadog)

**Key changes:**
1. Land trait + types + `TracingSink` in matric-core
2. Wire `AuditSink` into `AppState`
3. Emit `auth.login_*` events from auth middleware (ADR-071)
4. Emit `auth.decision*` events from authorization middleware (ADR-089)
5. Emit `system.cross_tenant_access` from `TenantScopedDb::for_system` (ADR-090)
6. Land first EE sink (`fortemi-enterprise-audit-jsonl`) per `.aiwg/architecture/plugin-contract-spec.md` §12

## References

- ADR-088 — Plugin strategy
- ADR-089 — Authorization policy
- ADR-090 — Tenancy
- ADR-093 — Key provider
- `.aiwg/architecture/plugin-contract-spec.md` §10
- `.aiwg/security/multi-tenant-threat-model.md` §7
- SOC 2 Type II Trust Services Criteria CC7.2 (system monitoring), CC7.3 (event log analysis)
- NIST SP 800-92 (log management)
