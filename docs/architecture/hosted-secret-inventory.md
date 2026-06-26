# Hosted Secret Inventory

This contract defines the hosted Fortemi secret inventory, response
representations, storage classes, lifecycle requirements, and observability
boundaries for #968. It complements
`docs/architecture/hosted-telemetry-classification.md`: this document says what
is secret and how it may be stored or represented; the telemetry document says
what can reach logs, metrics, health, diagnostics, and retained payload sinks.

## Secret Classes

| Class | Examples | Storage rule | Response representation | Ordinary telemetry |
|---|---|---|---|---|
| `verifier_only` | API keys, OAuth client secrets, registration access tokens, refresh tokens, ingest tokens, future webhook verifier tokens. | Store only a keyed hash or password-hash-style verifier plus safe metadata. Raw value may appear only in the creation/import response. | One-time display where unavoidable; later reads show `secret_set`, prefix/fingerprint where approved, creation/rotation/expiry/revocation timestamps, owner/tenant, last-used metadata, and audit event ids. | No raw value, hash, prefix, or last4 unless the field is explicitly approved for support-safe display. Use stable credential class/count metadata. |
| `retrievable_secret` | Provider API keys, outbound webhook signing secrets, KMS-wrapped user secrets, encrypted private-key blobs that must be used after storage. | Encrypt at rest through #897 `KeyProvider`/KMS or a hosted secret store. Hosted mode fails closed if the configured secret store is unavailable. | Never return raw value after creation/import. Reads show `secret_set`, version, rotation timestamps, owner/tenant, last-used metadata, and audit event ids. | Presence flags, length classes, storage class, key purpose, version, and stable reason codes only. |
| `transient_request_secret` | Inline BYOK `api_key`, OAuth authorization codes, PKCE verifier, PKE passphrase/plaintext, webhook receiver signature material, bearer tokens. | Do not persist except as an approved verifier or encrypted retrievable secret. Keep lifetime scoped to the handler/provider call. | Not echoed. Validation failures use stable problem details and no submitted secret fragments. | Presence, length class, route/action, and stable validation or provider reason only. |
| `deployment_secret` | Environment variables, secret files, DB/Redis DSNs, subprocess credentials, KMS credentials, CI secrets. | Operator-managed or platform secret-store managed. Hosted images must require operator-supplied secrets and reject production use of local defaults. | Not exposed through health/status/config APIs. Operator docs use scanner-safe placeholders. | Source class, configured/not-configured, destination class, and bounded failure reason only. |
| `secret_adjacent_topology` | Provider base URLs, recording URLs, presigned URLs, callback URLs, internal hosts, bucket names, filesystem paths. | Store only where product behavior requires it and protect reads by object/admin policy. Credential-bearing URLs are secrets. | Redacted class/fingerprint when approved; never return embedded credentials or tokens. | URL class, host class, path length, query-token presence, and stable destination-policy reason. |
| `content_derived_sensitive` | Search/cache keys, prompts, generated responses, transcripts, extraction metadata, inbound payloads, tool args/results. | Treat as product content or retained diagnostic content, not as operational log metadata. | Govern by product object policy and DSAR/retention rules. | Lengths/counts/parser names/stable codes only unless an approved protected diagnostic sink is active. |

## Surface Inventory

Hosted secret handling must account for these surfaces:

- API request/response DTOs, including manual `Debug`/`Display`
  implementations and test assertion output.
- Database columns and JSON blobs, including config history, job payloads,
  outbox payloads, DLQ payload/error fields, localized audit stores, and future
  `user_secrets` rows.
- Environment variables, mounted secret files, subprocess environments,
  command arguments, shell history, crash dumps, temporary directories, and
  cleanup traps.
- Logs, spans, `fortemi.audit` attrs, `fortemi::events`, metrics, public health,
  protected diagnostics, and support exports.
- OpenAPI/AsyncAPI examples, docs, scripts, fixtures, generated curl commands,
  MCP tool results, and model-visible helper output.

## Response And Metadata Rules

- Raw secret values are only allowed in a creation/import response when there is
  no protocol alternative. The response must document one-time display
  semantics and avoid logging/tracing the serialized value.
- List/get/update/delete responses for secret-bearing records must use metadata
  DTOs rather than relying on `serde(skip_serializing)` on domain structs.
- Approved metadata is limited to `secret_set`, storage class, creation time,
  rotation time, expiry time, revocation/disabled time, owner/tenant, last-used
  time, version, safe reference ids, and audit event ids.
- Prefix, last4, or fingerprint display must be deliberately approved per
  credential type. Operator-facing UI masks are not automatically safe for
  logs, metrics, or audit attrs.
- Verifier columns should be named `*_hash` or documented as hash-only legacy
  columns. Names that imply raw recovery are forbidden for new hosted schema.

## Lifecycle Rules

- Create/import: generate or accept the secret, classify it, store it as
  `verifier_only` or `retrievable_secret`, emit metadata-only audit, and return
  only approved one-time output.
- Rotate: create a new version, record previous-version disposition, emit
  metadata-only audit, and never echo old or new raw values outside an approved
  one-time response.
- Expire/disable/revoke: record timestamp, reason class, actor, and audit event
  id. Verification paths must reject disabled, revoked, or expired credentials.
- Export/import: replay-safe import must preserve class and version metadata;
  exports must not include raw retrievable secrets unless a separately
  authorized encrypted backup profile includes them.
- Delete: deletion must remove verifier or encrypted secret material where the
  product policy allows deletion; retained audit metadata must not contain raw
  secret values.

## Hosted Storage Rules

- Hosted multi-tenant mode requires #897 `KeyProvider`/KMS or a hosted secret
  store for any `retrievable_secret`.
- Hash-only credentials must use a one-way verifier and never store raw values,
  reversible encryption, or raw hashes in telemetry.
- Encryption context/AAD values are audit-visible metadata. They must not carry
  user-entered secret labels, provider URLs, prompts, or raw topology values.
- Secret-store unavailability is fail-closed for hosted create/read/use/rotate
  paths that require a retrievable secret. Bootstrap behavior may degrade only
  where the owning audit/KMS contract explicitly permits it.
- Local/dev defaults may exist only under explicit local-development profiles
  and must be blocked or rejected in hosted/production mode.

## Cross-Issue Contract

- #897 owns `KeyProvider`/KMS availability, key purpose, wrapping, and rotation
  mechanics.
- #910 owns metadata-only `fortemi.audit` events for secret lifecycle and
  privileged diagnostic reads.
- #967 owns public RFC 9457 problem responses for secret-related failures.
- #974 owns telemetry classes and redaction at log/event/metric/health sinks.
- #900 and #969 consume secret metadata, retained payload classes, and processor
  disclosures for DSAR, retention, and privacy notice behavior.
