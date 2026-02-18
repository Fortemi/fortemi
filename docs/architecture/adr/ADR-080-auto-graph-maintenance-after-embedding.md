# ADR-080: Auto GraphMaintenance Trigger After Embedding

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam

## Context

The graph quality pipeline (ADR-073) runs as a `GraphMaintenance` background job. Previously, operators had to manually trigger this job via `POST /api/v1/graph/maintenance` after running bulk embedding operations. When a large batch of notes is re-embedded (via `ReEmbedAll` or per-note `EmbedNote` jobs), new semantic links are computed and inserted, but the graph topology is not refined until `GraphMaintenance` runs.

This creates a gap: the graph accumulates new edges from fresh embeddings but does not apply SNN pruning or PFNET sparsification. The unrefined graph contains noisy, redundant edges and does not reflect the improved embedding quality. Users see the dense "seashell pattern" hairball even after re-embedding.

## Decision

Queue a deduplicated `GraphMaintenance` job automatically at the end of each embedding job execution, specifically after semantic links are recomputed for a note.

The trigger is placed inside `EmbeddingHandler::execute`, after the `recompute_links` call that generates new semantic links from the fresh embedding:

```rust
// Queue a deduplicated GraphMaintenance job so SNN/PFNET run after new
// links are created. Dedup (ADR-079) ensures at most one instance is
// queued even if many notes embed in rapid succession.
let _ = db.jobs
    .queue_deduplicated(
        None, // no note_id — global job
        schema,
        JobType::GraphMaintenance,
        JobType::GraphMaintenance.default_priority(),
        serde_json::Value::Null,
    )
    .await;
```

The trigger uses `queue_deduplicated` (ADR-079), which ensures at most one `GraphMaintenance` job is pending regardless of how many notes embed concurrently. The `let _ =` pattern intentionally ignores errors — a failed queue attempt is not fatal to the embedding job.

`GraphMaintenance` runs at lower priority than embedding jobs, so the refining pass naturally follows after the embedding batch completes.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Manual operator trigger only | Requires operator awareness; easy to forget after bulk operations |
| Scheduled cron-based trigger | Adds infrastructure complexity; delay is unpredictable |
| Trigger after every ReEmbedAll completion | ReEmbedAll queues many EmbedNote jobs; triggering after the batch completes is harder to detect |
| Real-time link refinement per embedding | O(E*N) SNN computation per note is prohibitively expensive |

## Consequences

### Positive
- (+) Graph quality automatically improves after every embedding batch without operator intervention
- (+) Deduplication prevents queue flooding when many notes embed concurrently (only one `GraphMaintenance` queued)
- (+) Priority ordering ensures `GraphMaintenance` runs after the embedding batch that triggered it
- (+) Zero new configuration; uses existing job infrastructure

### Negative
- (-) `GraphMaintenance` is triggered even for single-note embeddings, which may be unnecessary overhead on small corpora
- (-) Ignores queue failure silently (`let _ = ...`); metrics are the only way to detect repeated queue failures
- (-) Adds latency between embedding completion and graph refinement (job queue processing time)

## Implementation

**Code Location:**
- Trigger: `crates/matric-api/src/handlers/jobs.rs` (`EmbeddingHandler::execute`, after `recompute_links`)
- Dedup: `crates/matric-db/src/jobs.rs` (`queue_deduplicated` — ADR-079)
- Consumer: `crates/matric-api/src/handlers/jobs.rs` (`GraphMaintenanceHandler`)

## References

- ADR-073: Graph Quality Pipeline Architecture
- ADR-079: Global Job Deduplication by Job Type
- Issue #481: Graph Quality Overhaul Epic
