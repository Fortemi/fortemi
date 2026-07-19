# Hosted Telemetry Classification

This contract defines how hosted Fortemi classifies logs, telemetry, retained
event payloads, health output, and diagnostics. It complements the normalized
`fortemi.audit` stream in `docs/architecture/audit-event-mapping.md`; ordinary
operational telemetry must not become an unclassified audit or content store.
Secret classes, storage rules, response metadata, and credential lifecycle
requirements are defined in `docs/architecture/hosted-secret-inventory.md`.

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

## Localized Telemetry Stores

Several existing tables look audit-like but are not interchangeable with the
normalized `fortemi.audit` stream. Hosted mode must classify reads, retention,
and redaction per store before exposing them to support tools, dashboards, or
exports:

| Store | Owner | Default class | Access boundary | Retention / DSAR handling | Safe broad telemetry | `fortemi.audit` mapping |
|---|---|---|---|---|---|---|
| `inference_config_audit` | Runtime model-provider configuration. | `diagnostic_sensitive` | Admin/operator only; route reads require the control-plane authorization path. | DSAR-relevant configuration history when tied to a user/tenant/provider record; retention follows the model-config history policy, not ordinary log retention. | Action, provider count, changed-field names/counts, source class, result, and bounded reason codes. | Config set/reset/test actions mirror as `category=model_config` with sanitized before/after summaries and no raw provider JSON. |
| `file_upload_audit` | Upload validation and file-safety outcomes. | `security_audit` plus `diagnostic_sensitive` details | Admin/operator or owning-resource policy after hosted object authorization. | DSAR/security-retention relevant when rows identify a user, note, attachment, or quarantined object. | Outcome, file-size bucket, content-type class, extension length, reason code, and quarantine presence. | Accepted/denied/quarantined decisions mirror as `category=file_upload` without filenames, paths, payloads, or raw headers. |
| `skos_audit_log` | Taxonomy governance history. | `security_audit` | Taxonomy governance authorization; not a broad support log. | Retain as governance history; include in tenant export/retention review when taxonomy labels or definitions are user-authored. | Entity type, relation/action class, counts, status class, archive presence/length, and reason code. | Hosted taxonomy mutations mirror as `category=taxonomy` with sanitized change summaries and safe resource refs. |
| `note_access_log` | Access-frequency analytics for note reads/search/traversal. | `metrics_health` aggregated; raw rows are `diagnostic_sensitive`. | Public health may expose aggregates only; raw row reads require protected diagnostics or a future note-read audit path. | DSAR/retention relevant because rows can reveal content access patterns even without note bodies. | Aggregate counts, time buckets, and coarse access mode only. | Not security-audit proof. If hosted note-read audit is required, add a separate producer with principal, tenant, purpose, outcome, and retention fields. |
| `event_outbox` non-inbound rows | Product event fan-out and replay. | `operational_event` or `diagnostic_sensitive` by entity type. | Support-safe metadata only unless a protected diagnostic read path authorizes payload inspection. | Retention follows product outbox/replay policy; payload-bearing rows must be mapped into DSAR/retention matrices. | Entity type, event type, publish status, age, retry count, size, and reason code. | Security-relevant product decisions should also emit `fortemi.audit`; outbox delivery alone is not audit proof. |
| `inbound_dlq` | Failed inbound connector events. | `diagnostic_sensitive` or `content_prohibited` for raw payload/error. | Protected diagnostics only for raw rows; public/support views expose aggregate status. | Retained external/user payload sink; include in DSAR, retention, beyond-use, and legal-hold handling. | Source class/length, payload class, serialized length, secret-candidate flag, attempt count, age, and stable failure reason. | Privileged raw inspection should emit a future audit-read event once read producers are available. |
| `usage_event_ledger` and delivery/attempt rows | Authoritative usage facts plus replay state. | `diagnostic_sensitive`; aggregate health is `metrics_health`. | Subject-level rows require tenant-scoped billing/privacy authorization. Broad telemetry exposes aggregate lag/status only. | Follow [`usage-ledger-retention.md`](usage-ledger-retention.md); immutable accounting evidence, delivery metadata, external sinks, and backups have separate outcomes. | Counts by safe dimension/status, age/lag buckets, unavailable count, required-sink health, and bounded reason classes. | Billing events are not security-audit proof. Privileged policy/config changes should emit separate audit events. |

This inventory complements `docs/architecture/audit-event-mapping.md`: that
document defines which security-relevant actions should be mirrored into
`fortemi.audit`; this document defines what can safely appear in operational
logs, health, metrics, support views, and diagnostic exports for the same
localized stores.

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
  semantics used by telemetry; see
  `docs/architecture/hosted-secret-inventory.md`.
- #974 owns this operational telemetry classification and the call-site
  migration away from raw values.
- #939 owns Redis Stream payload minimization; Redis/event stream payloads still
  consume this telemetry policy for logs, metrics, and diagnostics.
- #900 consumes the retained telemetry classes for DSAR, retention,
  beyond-use, and legal-hold behavior.
