# Hosted Telemetry Classification

This contract defines how hosted Fortemi classifies logs, telemetry, retained
event payloads, health output, and diagnostics. It complements the normalized
`fortemi.audit` stream in `docs/architecture/audit-event-mapping.md`; ordinary
operational telemetry must not become an unclassified audit or content store.

## Classes

| Class | Purpose | Default allowed fields | Default forbidden fields |
|---|---|---|---|
| `security_audit` | Security-relevant decisions and administrative actions routed through `fortemi.audit`. | `AuditEvent` fields, safe resource refs, stable reason codes, bounded counts/classes. | Secrets, raw content, raw provider payloads, raw request bodies, raw backend errors. |
| `operational_event` | Support-safe job, queue, connector, outbox, and runtime state. | Stable event names, coarse component, safe ids, counts, durations, sizes, status, reason codes. | Prompts, transcripts, note bodies, attachment bytes, raw connector payloads, raw job payloads/results/errors. |
| `metrics_health` | Low-cardinality dashboard and public health state. | Aggregate counts, coarse status, lag buckets, enabled flags, bounded rates. | Tenant names, connector names on public endpoints, raw source ids, payload fields, per-user/content labels. |
| `diagnostic_sensitive` | Admin/operator-only diagnostics for incident response. | Redacted endpoint classes, object refs after policy, error classes, bounded stderr/error lengths, fingerprints when approved. | Raw secrets, bearer/API keys, private keys, unbounded errors, raw SQL snippets, raw filesystem paths, raw provider response bodies. |
| `content_prohibited` | Content that broad logs/metrics/events must not emit by default. | Lengths, counts, parser names, schema names, stable parse/validation codes. | User prompts, generated responses, transcripts, attachment bytes, webhook bodies, tool args/results, retrieval snippets, raw inbound events. |

All untrusted strings that remain loggable must be bounded and sanitized for
CR/LF, delimiter injection, terminal control characters, and oversized values
before they reach stdout, files, event-bus telemetry, metrics, or health JSON.

## Retained Payload Sinks

`event_outbox` and `inbound_dlq` can retain external or content-derived
payloads. Treat those tables as retained payload sinks, not as support-safe
telemetry:

- `event_outbox` rows with `entity_type = 'inbound_event'` may contain raw
  connector payloads and are `diagnostic_sensitive` or `content_prohibited`
  depending on payload type.
- `inbound_dlq.payload` and `inbound_dlq.error` may contain upstream event
  content, backend errors, credentials, internal paths, or tenant identifiers.
- Broad logs and public health should expose only counts, age, lag, delivery
  status, source class/length, payload size, and stable reason codes.
- Raw payload inspection requires a protected diagnostic path with explicit
  authorization, retention, DSAR treatment, and audit events for privileged
  reads.
- External dead-letter sinks, such as Kafka DLQ topics, inherit the same
  classification and must not be treated as ordinary telemetry exports.

## Sink Rules

- **stdout/file tracing:** support-safe by default. Do not emit
  `content_prohibited` or raw `diagnostic_sensitive` values.
- **`fortemi::events` tracing mirror:** support-safe by default. Use stable
  event names, safe ids, counts, classes, and bounded lengths.
- **public health/metrics:** aggregate only unless a route is operator-gated.
  Do not expose connector names, source names, tenant names, per-user labels, or
  raw object URLs.
- **protected diagnostics:** require admin/operator authorization, retention
  policy, redaction, and a security-audit event once audit read producers are
  wired.
- **security audit:** use `AuditEvent`; do not overload operational logs as
  audit proof.

## Cross-Issue Contract

- #910 owns the `AuditEvent` security-audit envelope and `fortemi.audit` sink.
- #967 owns public API error shapes; error payloads must use stable problem
  codes and request ids, not backend messages or sensitive details.
- #968 owns secret classes, storage/lifecycle rules, and shared redaction
  semantics used by telemetry.
- #974 owns this operational telemetry classification and the call-site
  migration away from raw values.
- #939 owns Redis Stream payload minimization; Redis/event stream payloads still
  consume this telemetry policy for logs, metrics, and diagnostics.
- #900 consumes the retained telemetry classes for DSAR, retention,
  beyond-use, and legal-hold behavior.
