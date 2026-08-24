# PostgreSQL Audit Sink Operations

The PostgreSQL audit sink is an optional durable implementation of the
`matric-core` `AuditSink` contract. CE behavior is unchanged: `TracingSink`
remains the default until runtime wiring explicitly selects
`PostgresAuditSink`.

## Guarantees and boundaries

- Every stored row has a tenant ID and is protected by forced PostgreSQL RLS.
- Event IDs and tenant-scoped idempotency keys suppress retry duplicates.
- A database trigger rejects updates and deletes. Inserts remain possible, so
  this is append-only application storage, not WORM or tamper evidence.
- Events are sanitized immediately before persistence. Sink errors expose only
  coarse error classes.
- A successful write is durable according to the PostgreSQL cluster's commit,
  WAL, replication, and backup configuration.

The sink does not configure retention, legal holds, audit export authorization,
replication, backups, or external immutable storage. Operators must not claim
compliance-ready or tamper-evident audit retention from this table alone.

## Health and failure policy

`check_health()` performs a database round trip. Failed writes and checks mark
the sink `Unavailable`, increment `consecutive_failures`, and retain a coarse
error class. A later successful write or check returns it to `Ready`.

Callers must apply each event's `AuditFailurePolicy`: `FailClosed` maps to
operation rejection after runtime readiness, `DegradeWithAlert` maps to an
explicit degraded state, and `BestEffort` may continue. Installing the sink in
application state and exposing health are runtime/API wiring receipts and are
not part of the database crate foundation.

## Deployment verification

1. Apply all SQLx migrations and confirm `public.audit_event` exists.
2. Run the service with the non-owner, non-`BYPASSRLS` runtime database role.
3. Emit a synthetic tenant audit event and verify one row appears under that
   tenant scope and no row appears under another tenant scope.
4. Retry with the same idempotency key and verify the row count is unchanged.
5. Attempt an update and delete with an administrative test connection; both
   must fail with `audit_event is append-only`.
6. Stop PostgreSQL and verify audit health becomes unavailable and a
   fail-closed test operation is rejected; restore PostgreSQL and verify health
   returns to ready.

Monitor PostgreSQL commit latency, connection-pool exhaustion, WAL/archive
health, disk capacity, replication lag, failed audit writes, and the sink's
consecutive-failure count. Back up and restore this table under the same tenant
and audit-access controls as the primary database.

## Rollback

Runtime selection can be rolled back to CE `TracingSink` without deleting
stored rows. Do not drop or rewrite `audit_event` during an incident. Preserve
the table and WAL for investigation, remove runtime selection separately, and
record the durability gap for the affected interval.
