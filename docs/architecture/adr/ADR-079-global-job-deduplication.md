# ADR-079: Global Job Deduplication by Job Type

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam

## Context

The job queue deduplication system was originally designed for per-note jobs: when a note is re-indexed, only one pending/running `EmbedNote` job per `(note_id, job_type)` pair is allowed. This prevents redundant work when a note is updated rapidly.

Two job types — `GraphMaintenance` and `ReEmbedAll` — do not operate on a single note. They process the entire archive. These jobs have no meaningful `note_id`. The original deduplication logic only applied when `note_id` was present, meaning multiple `GraphMaintenance` jobs could accumulate in the queue simultaneously. Running two concurrent `GraphMaintenance` jobs causes lock contention on graph edges, produces inconsistent intermediate states, and wastes significant compute on large graphs.

## Decision

Extend `queue_deduplicated` to handle jobs with no `note_id` using `job_type`-level deduplication: at most one pending or running instance of a global job type is allowed at any time.

When `note_id IS NULL`, the dedup query uses `job_type` as the sole uniqueness key:

```sql
SELECT id FROM job
WHERE note_id IS NULL
  AND job_type = $1
  AND status IN ('pending', 'running')
LIMIT 1
```

If a matching job is found, the new request is dropped (not queued). If no matching job exists, a new job is inserted normally.

This replaces the prior behavior where `note_id IS NULL` bypassed deduplication entirely.

**Affected job types:** `GraphMaintenance`, `ReEmbedAll` (any future global job types that set `note_id = NULL` will also receive this deduplication automatically).

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Use a database unique constraint | Cannot dedup across `pending` and `running` states with a simple constraint |
| Lock the row and skip if locked | Doesn't prevent queue accumulation — jobs stack up when prior run is complete |
| Separate global job table | Unnecessary complexity; same queue works with NULL note_id as the global key |
| Always deduplicate by type regardless of note_id | Breaks per-note jobs where same type legitimately runs for different notes concurrently |

## Consequences

### Positive
- (+) Prevents redundant concurrent `GraphMaintenance` runs with O(E*N) compute each
- (+) Queue remains clean: no pile-up of global maintenance jobs during active embedding periods
- (+) Automatic coverage for any future global (note_id = NULL) job type
- (+) No schema change required; uses existing `note_id IS NULL` sentinel

### Negative
- (-) A queued-but-stale `GraphMaintenance` blocks a fresh request even if data changed since queuing
- (-) Dedup logic is split across two code paths (note-level vs. global) inside `queue_deduplicated`
- (-) No built-in way to force-queue a second instance (must manually cancel the pending job first)

## Implementation

**Code Location:**
- `crates/matric-db/src/jobs.rs` (`PgJobRepository::queue_deduplicated`)
- Branch on `note_id.is_none()` to select global vs. per-note dedup SQL

**Dedup SQL (global path):**

```sql
SELECT id FROM job
WHERE note_id IS NULL
  AND job_type = $2::job_type
  AND status IN ('pending', 'running')
LIMIT 1
```

**Job types using global deduplication:**
- `JobType::GraphMaintenance` — graph quality pipeline (SNN, PFNET, snapshot)
- `JobType::ReEmbedAll` — bulk re-embedding of all notes in an archive

## References

- ADR-073: Graph Quality Pipeline Architecture
- ADR-080: Auto GraphMaintenance After Embedding (depends on this dedup)
- `crates/matric-db/src/jobs.rs`
