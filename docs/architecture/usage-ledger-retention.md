# Usage Ledger Retention and DSAR Contract

This document classifies the durable usage ledger introduced by #1068. It is
an engineering contract, not legal advice or a compliance claim. The complete
Fortemi data-scope catalog and jurisdiction/operator decisions remain owned by
#900.

## Store Inventory

| Store | Purpose and data | Subject lookup | Default DSAR posture |
|---|---|---|---|
| `usage_event_ledger` | Immutable validated `UsageEvent`, event and recorded times, logical identity, and SHA-256 content fingerprint. It may contain opaque tenant, principal, client, archive, anonymous, request, job, bridge, MCP, or provider-call identifiers plus allowlisted usage attributes. | Join the structured `event.subject` fields within the authorized tenant/archive boundary. Correlation identifiers are supplemental lookup keys, not proof of subject identity. | Access/export is deferred to the #900 handler. Erasure is `deleted` only when policy permits removal of the accounting fact; otherwise report `retained_with_basis` or `deferred_manual`. |
| `usage_event_conflict` | Integrity quarantine containing incoming/existing identities and fingerprints. It never stores the incoming raw event. | Resolve `existing_event_id` to the ledger, then apply the same subject and tenant scope. | Retain with the ledger integrity evidence or cascade-delete with the ledger event. Do not expose fingerprints as customer content. |
| `usage_event_delivery` | Per-sink state, stable external idempotency key, attempt count, acknowledgement/export time, retry time, terminal reason class, and current lease. It contains no sink credential or provider response payload. | Join `event_id` to the ledger. | Include safe status/timestamps in an access receipt when relevant. Cascade-delete with an erasable ledger event. |
| `usage_delivery_attempt` | One row per claimed sink attempt with lease interval and bounded outcome/reason class. Completed attempts cannot be rewritten. | Join `(event_id, sink_name)` through delivery to the ledger. | Treat as billing/operations evidence. Cascade-delete with an erasable ledger event; otherwise return the same retained basis as the ledger event. |
| `usage_sink` | Low-cardinality operator configuration: name, enabled state, and whether the sink is required. No endpoint, credential, session token, or provider payload is stored. | Not subject keyed. | Normally `not_applicable`; tenant-specific future sink configuration must be added to #900 before use. |

## Retention Rules

- Fortemi does not hard-code a universal retention duration for accounting
  records. Operators must configure a reviewed financial, contractual,
  security, and privacy policy before hosted billing is enabled.
- The ledger, conflict evidence, delivery state, attempt history, aggregates,
  provider sidecars, external billing objects, and backups are separate
  retention classes. Retaining one does not authorize retaining the others.
- Deleting a ledger event cascades its conflict and delivery/attempt rows. It
  also removes Fortemi's durable duplicate identity for that event. Therefore
  the deletion horizon must not be shorter than the maximum producer retry,
  sink replay, reconciliation, dispute, and correction horizon.
- Reversals and corrections are new immutable events. They do not mutate or
  erase the original event. A policy deletion must evaluate the linked
  accounting set so it does not leave a misleading orphaned correction.
- The current schema does not implement in-place subject anonymization because
  ledger events are immutable. Until #900 selects and tests an approved
  pseudonymization or cryptographic-erasure design, requests requiring retained
  evidence are `retained_with_basis` or `deferred_manual`, not silently marked
  anonymized.
- Legal hold, statutory accounting retention, abuse prevention, and security
  investigation are operator/legal decisions. Receipts must name the configured
  basis class without exposing internal investigation detail.

## Backups and Restores

Database backups, knowledge archives, exported dumps, replicas, and WORM media
are separate retained copies. A live-system deletion does not prove those
copies were erased.

- Expired copies should be deleted by their own verified retention job.
- A copy that cannot be selectively modified is
  `beyond_use_until_retention_expiry`; access and ordinary restore are
  restricted by backup policy.
- Restore procedures must carry a durable re-erasure manifest or equivalent
  tombstone set and reapply subject deletions before the restored system serves
  traffic.
- Current #1068 code does not implement that manifest. Hosted operators must
  treat restore-time re-erasure as a launch blocker owned by #900/#980 rather
  than assuming database cascades cover backups.

## External Sinks

External billing, warehouse, and metering systems consume Fortemi delivery
state; they do not replace the Fortemi ledger.

- `external_idempotency_key` is stable across retries for one event/sink row.
  Sink plugins use it instead of generating a new key for every attempt.
- Sink credentials, authorization/session tokens, raw errors, and provider
  payloads must remain in the sink's protected configuration, never delivery
  or attempt rows.
- Processor deletion/notification, external object retention, and proof of
  acknowledgement are sink-specific #900 catalog entries. A local cascade does
  not claim external deletion.
- Terminal rejection is not automatically replayed. Bounded backfill creates
  only missing delivery rows for an enabled sink; it does not reset
  acknowledged or terminal state.

## Runtime Policy

`MATRIC_USAGE_METER_MODE` has two accepted values:

| Value | Behavior |
|---|---|
| `noop` | CE default. Validates envelopes but intentionally persists no usage. |
| `durable-required` | Uses the PostgreSQL ledger and refuses startup if the ledger cannot be queried. |

`MATRIC_USAGE_REQUIRE_SINK=true` is valid only with `durable-required` and also
requires at least one enabled sink marked `required`. Unknown values and
contradictory settings refuse startup; they never fall back to `noop`.

Runtime producer recording remains best effort after successful CE work, as
defined by ADR-092: a post-response ledger outage does not rewrite a successful
customer response. Hosted hard-cap admission and fail-closed quota reservation
remain owned by #714.

## Verification Contract

Before promoting this design beyond its current implementation:

- exact replay and both identity-conflict paths must pass;
- simultaneous identical writers must yield one accepted event and one set of
  sink deliveries;
- expired leases must be reclaimable, while stale workers cannot acknowledge a
  newer attempt;
- completed attempt history must reject mutation;
- partial, final, reversal, and unavailable events must round-trip unchanged
  and aggregate according to ADR-092;
- late-sink backfill must be bounded and repeat-safe;
- debug output and reason validation must exclude raw subject values,
  idempotency values, credentials, URLs, payloads, and raw backend errors;
- the #900 matrix must supply reviewed collection start, retention duration,
  access range, erasure basis, backup state, and external-processor outcome
  before a DSAR handler claims automated completion.
