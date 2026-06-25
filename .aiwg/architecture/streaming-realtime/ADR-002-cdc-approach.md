# ADR-002: Change Data Capture Approach

**Status**: Accepted
**Date**: 2026-04-09
**Track**: fortemi/streaming-realtime
**Decision**: Manual outbox pattern (application-level)

## Context

Fortemi needs to capture data changes (note CRUD, tag changes, embedding updates, job state) and publish them to Redis Streams for downstream consumers (SSE, WebSocket, webhooks, incremental processing).

Three CDC approaches were evaluated:
1. **Manual outbox** — Write event row to `event_outbox` table in the same PG transaction
2. **pgwire-replication** — Rust-native WAL consumer (immature crate)
3. **Sequin** — Purpose-built PG CDC in a Docker container

## Decision

**Manual outbox pattern.** Write events to an `event_outbox` table in the same transaction as the data change. A background task reads the outbox, publishes to Redis Stream, and marks events as consumed.

## Rationale

- Proven pattern — Fortemi already uses SKIP LOCKED for job queues, which is conceptually the same approach.
- Zero new dependencies — no new crates, no new containers.
- Selective capture — only instrument the events that matter (note CRUD, tag changes, embedding completion), not every database mutation.
- Transactional consistency — event is written in the same transaction as the data change. If the transaction rolls back, the event is never published.
- pgwire-replication is immature and adds replication slot complexity, especially with multi-schema (multi-memory) routing.
- Sequin adds another container to the Docker bundle, increasing operational surface.

## Consequences

- Every write path that should produce an event must be explicitly instrumented with an outbox insert.
- Changes not instrumented (e.g., direct SQL, migrations) won't produce events — accepted because those aren't user-facing data mutations.
- The outbox table needs cleanup (DELETE consumed rows or partition-based retention).
- CDC (pgwire-replication or Sequin) remains an option for Phase 3+ if automatic capture of all changes becomes necessary.

## Implementation

### Migration: `event_outbox` table

```sql
CREATE TABLE event_outbox (
    id         BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,           -- e.g., 'note.created', 'tag.renamed'
    entity_id  UUID,                    -- affected entity
    payload    JSONB NOT NULL DEFAULT '{}',
    memory     TEXT,                    -- archive/schema scope
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    consumed   BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_outbox_unconsumed ON event_outbox (id) WHERE NOT consumed;
```

### Write path (in same transaction)

```rust
// In note creation handler, after inserting the note:
sqlx::query("INSERT INTO event_outbox (event_type, entity_id, payload, memory) VALUES ($1, $2, $3, $4)")
    .bind("note.created")
    .bind(note_id)
    .bind(serde_json::to_value(&payload)?)
    .bind(memory_schema)
    .execute(&mut *tx)
    .await?;
```

### Background publisher

- Poll outbox (or PgListener wake-up) for unconsumed rows
- XADD to Redis Stream per event
- Mark consumed
- Periodic cleanup of consumed rows older than retention window
