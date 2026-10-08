# ADR-109: Point-in-Time Memory Export with a Transaction-Safe High-Water Mark

**Status:** Accepted  
**Date:** 2026-10-08  
**Authority:** `Fortemi/fortemi#1157`  
**Related:** ADR-102 (Knowledge Shard contract), ADR-090 (multi-tenancy), `Fortemi/fortemi#905`

## Context

Downstream analytics rebuild graph and knowledge projections from Fortemi. They
need an export boundary they can replay: a consistent snapshot, a mark that a
later export continues from, deletions, and a manifest with counts and hashes.
The Knowledge Shard and JSON backup exports are transfer and restore formats.
They do not promise a consistent snapshot, give no continuation mark, and carry
no tombstones.

A timestamp mark (`updated_at > T`) is unsafe in PostgreSQL. Commit order
differs from statement order: a transaction that began earlier can commit after
an export has read past its timestamp, and the next incremental skips its rows.
A sequence-number mark has the same defect, because `nextval()` is not
transactional. A writer can take a lower value and commit after a reader saw a
higher one.

## Decision

1. **Change stamps.** `note`, `note_original`, `note_revised_current`,
   `note_tag`, `link` and `collection` each gain
   `export_change_xid xid8 NOT NULL DEFAULT '0'`. A `BEFORE INSERT OR UPDATE`
   trigger sets it to `pg_current_xact_id()`. An update that changes only
   columns never exported (access counters, generated `tsv`,
   `shard_note_count`) keeps the old stamp, so reads do not churn exports.
   Existing rows keep `'0'`, which is older than any mark, so the migration does
   not rewrite tables.
2. **Tombstones.** An `AFTER` row trigger records a hard delete, or a primary
   key change, in the per-memory `export_tombstone` table, stamped the same way.
   Re-inserting a key deletes its tombstone in the same transaction, so a
   tombstone exists exactly when its key is absent. The trigger writes to
   `TG_TABLE_SCHEMA`, not to wherever `search_path` points. The table carries
   `tenant_id` and forced tenant RLS, is listed in the tenant catalog, and is
   cloned into new archives like any per-memory table. Its tenant foreign key
   cascades: tombstones mean nothing without their tenant and must not block
   tenant removal. Soft-deleted notes are
   state rather than deletions: they are exported with `deleted_at` set.
3. **One snapshot.** A single SQL statement reads the records, the tombstones,
   `pg_snapshot_xmin(pg_current_snapshot())` and the largest visible stamp. It
   therefore sees one MVCC snapshot under any isolation level. Community
   requests run it in `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY`. Hosted
   requests run it in the verified tenant request transaction after
   `SET TRANSACTION READ ONLY`. The statement also filters on
   `app.current_tenant` explicitly, so the scope holds even for roles that
   bypass RLS.
4. **High-water mark.** `M = min(max_visible_stamp + 1, snapshot_xmin)`, or
   `min(1, snapshot_xmin)` for a memory without stamps. It is published as a
   decimal string.
   - *Safety:* every transaction below `snapshot_xmin` has finished. Any change
     the snapshot cannot see therefore carries a stamp `>= snapshot_xmin >= M`,
     and `incremental(since = M)` returns it. That is the torn-read guarantee.
   - *Determinism:* when no transaction older than the newest visible stamp is
     still running, `M = max + 1`. This depends only on the data, so repeated
     exports of unchanged data are byte-identical.
   - *Monotonicity:* `snapshot_xmin` never decreases. The largest stamp never
     decreases either, because deletes and re-inserts are stamped later and
     tombstones are not pruned. So `M` never decreases.
5. **Incremental.** `since = N` returns live records with stamp `>= N`, plus
   tombstones with stamp `>= N`. A `since` greater than the current mark is
   rejected. When an older writer held `M` below `max + 1`, some records the
   previous export already contained can be sent again. Upsert semantics make
   that harmless.
6. **Canonical output.** Each line is canonical JSON with sorted keys.
   Timestamps are rendered with `TimeZone=UTC` and `extra_float_digits=1`.
   Lines are ordered by entity type, then by the canonical key text compared
   bytewise. The manifest carries per-type counts and `sha256` digests over the
   lines, a total digest, the selection, the mark window, the memory and tenant,
   `export_version`, `schema_version` (the newest embedded migration), and the
   producer version plus `MATRIC_GIT_SHA`. `generated_at` is the only field
   that is not deterministic. No build-time timestamp is embedded.
7. **Surfaces.** `POST /api/v1/memory/export` has policy class
   `AuthenticatedRead` (`read` scope) and `no-store` caching, and is admitted to
   the hosted tenant request transaction. The MCP tool `export_memory_snapshot`
   (full tool mode) wraps it. The contract lives in `contracts/memory-export/`.

## Alternatives considered

- **Exported snapshots (`pg_export_snapshot`).** These live only while the
  originating transaction is open, so they cannot serve a stateless API or a
  mark that is reused days later.
- **Logical decoding or an outbox.** These are exact but need replication
  privileges or a write on every change. Logical decoding does not work under
  a restricted hosted runtime role, and the existing outbox is
  application-emitted and incomplete.
- **Raw `pg_snapshot` cursor.** Visibility tests against the full prior
  snapshot are exact. But the snapshot text changes with unrelated cluster
  activity, which defeats byte-identical output, and it is not a scalar mark.
- **`track_commit_timestamp`.** This needs server configuration that managed
  databases may not allow, and commit timestamps can still tie or skew.

## Consequences

- Every write to the six tables runs two lightweight row triggers. One looks up
  the tombstone primary key on insert, and one writes an upsert on delete.
- Tombstones accumulate until a future retention policy adds a published
  tombstone floor. Exports report no floor in 1.0.0.
- `TRUNCATE` is not tracked, and the application does not use it on these
  tables. A database-level restore or `pg_dump` load carries stamps from
  another transaction-id space, so consumers must take a new full export after
  any restore.
- Exports are materialized in memory and returned as one JSON body. This is
  required in hosted mode, where streaming would outlive the tenant
  transaction. Very large memories should export by entity type.

## Relationship to #905

`#905` decides what Referenced archives copy on clone, backup and transfer. A
memory export is a read-only projection of Fortemi-owned rows in one memory. It
never includes operator source bytes or host paths, and it creates no archive.
When Referenced storage lands, its metadata rows are exported only if they
become one of the listed entity types, and #905's behaviour matrix should list
memory export as metadata-only.

@depends `contracts/memory-export/README.md`
@depends `migrations/20261008000000_memory_export_change_tracking.sql`
@issue `Fortemi/fortemi#1157`
