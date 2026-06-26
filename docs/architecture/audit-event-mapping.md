# Audit Event Mapping

This note records the #910 baseline mapping from existing localized audit-like
surfaces to the shared `AuditEvent` contract in `matric-core`.

The first implementation keeps domain tables in place. They remain useful for
local query workflows, but security-relevant actions should also emit sanitized
`fortemi.audit` events when a shared sink is configured.

## Shared Contract

`AuditEvent` is the normalized security-audit envelope:

- Identity and ordering: `id`, optional `idempotency_key`, `event_ts`,
  `observed_ts`, and `schema_version`.
- Actor and scope: `tenant_id`, `principal_id`, `visibility`, `retention`, and
  `source`.
- Object and correlation: `resource_kind`, `resource_id`, and `correlation_id`.
- Decision fields: `category`, `action`, `outcome`, `reason`, `severity`, and
  `failure_policy`.
- Attributes: sanitized metadata only. Consumers must not use free-form `attrs`
  for authorization, retention, or tenant-scoping decisions.

Sanitization happens before buffering or sink dispatch. Attribute values that
look like bearer tokens, API keys, private keys, DSNs, passwords, or client
secrets are redacted. CR/LF, tab, and delimiter characters are normalized, and
oversized strings are truncated.

## Local Surface Mapping

| Surface | Current purpose | Shared audit mapping |
|---|---|---|
| `inference_config_audit` | Domain history for model-provider config changes. | Keep as queryable config history. Config set/reset/test actions should emit `category=model_config`, `resource_kind=model_config`, actor/correlation metadata when available, and sanitized before/after summaries rather than raw provider JSON. |
| `file_upload_audit` | Upload validation/security outcomes. | Keep as upload security/domain history. Mirror blocked/quarantined/accepted outcomes as `category=file_upload` with sanitized filename/content type/user agent/source-IP classification. Do not emit payloads, full paths, or untrusted headers raw. |
| `skos_audit_log` | Taxonomy governance history. | Keep as domain governance history. Mirror hosted taxonomy mutations as `category=taxonomy`, with entity type/id as structured resource fields where safe and sanitized change summaries in attrs. |
| `note_access_log` | Access-frequency analytics for note reads/search/traversal. | Do not treat as security audit proof. If hosted note-read audit is required, add a separate producer with principal, tenant, purpose, outcome, and retention fields. |
| OAuth token cleanup / revoked-token retention comments | Token lifecycle retention guidance. | Future OAuth producers should emit `category=oauth` events for token issue/revoke/introspect cleanup decisions, using token/client ids or hashes only. Never emit bearer token values or raw client secrets. |
| Process startup | Low-risk runtime lifecycle signal. | Current first producer emits `category=process`, `action=startup`, safe logging destination mode, and build metadata through `TracingSink`. |

## Failure Policy

CE `TracingSink` is best-effort. The contract still represents future hosted
classes that must degrade or fail closed:

- `BestEffort`: low-risk operational events such as startup.
- `DegradeWithAlert`: important events where the initiating operation can
  continue but audit health must be visible.
- `FailClosed`: security-sensitive actions that hosted mode should reject if
  mandatory audit cannot be accepted.

`AuditBuffer` drops oldest best-effort events on overflow and tracks the drop.
Fail-closed events return an overflow error instead of silently dropping.

`AuditFailurePolicy::disposition_when_unavailable()` is the code-level bridge
for hosted mandatory audit behavior:

- During `Bootstrap`, fail-closed events degrade with an alertable audit-health
  condition instead of deadlocking startup on a sink that may depend on services
  still initializing.
- After hosted audit is `Ready`, fail-closed events map to
  `RejectOperation`. Hosted producers should use that disposition to reject the
  initiating security-sensitive operation when mandatory audit cannot be
  accepted.

This keeps KMS/key-provider startup able to emit through a bootstrap path while
still giving post-ready hosted operations a deterministic fail-closed contract.
