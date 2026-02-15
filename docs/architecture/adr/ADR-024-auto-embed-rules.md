# ADR-024: Auto-Embed Rules for Lifecycle Management

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team, implemented via issues #384-#389

## Context

Managing embeddings for full embedding sets requires lifecycle automation. Without automation, users must:

1. Manually trigger re-embedding when notes change
2. Track which notes need embedding updates
3. Decide when to rebuild vs. incrementally update
4. Handle stale embeddings after content modifications

This manual process is error-prone and doesn't scale. Users need declarative rules that automatically maintain embedding freshness.

## Decision

Introduce `auto_embed_rules` as a JSONB column on `embedding_set` with the following structure:

```json
{
  "on_create": true,      // Embed when note added to set
  "on_update": true,      // Re-embed when note content changes
  "on_delete": "cascade", // "cascade" | "orphan" | "keep"
  "batch_size": 100,      // Max notes per background job
  "priority": "normal",   // "low" | "normal" | "high"
  "schedule": null        // Optional cron for full rebuilds
}
```

**Rule Semantics:**

- `on_create`: When a note is added to the set, queue embedding job
- `on_update`: When note content changes, queue re-embedding job
- `on_delete`:
  - `cascade`: Delete embeddings when note removed from set
  - `orphan`: Keep embeddings but mark as orphaned
  - `keep`: Retain embeddings (for historical/audit purposes)
- `batch_size`: Control job granularity for backpressure
- `priority`: Influence job queue ordering
- `schedule`: Cron expression for periodic full rebuilds (e.g., weekly refresh)

**Defaults** (for backward compatibility with filter sets):
```json
{
  "on_create": false,
  "on_update": false,
  "on_delete": "cascade",
  "batch_size": 100,
  "priority": "normal",
  "schedule": null
}
```

## Consequences

### Positive
- (+) Declarative lifecycle management - set rules once, forget
- (+) Automatic freshness - embeddings stay in sync with content
- (+) Flexible deletion policies for different use cases
- (+) Backpressure control via batch_size
- (+) Priority support for critical vs. background sets

### Negative
- (-) Job queue load increases with many auto-embed sets
- (-) Potential for embedding storms during bulk imports
- (-) Complexity in job orchestration (dependencies, ordering)
- (-) Storage for rule configuration per set

## Implementation

**Code Location:**
- Schema: `migrations/20260201500000_full_embedding_sets.sql`
- Models: `crates/matric-core/src/models.rs` (AutoEmbedRules struct)
- Repository: `crates/matric-db/src/embedding_sets.rs`
- Jobs: `crates/matric-jobs/src/handlers/` (future: embed handlers)

**Key Changes:**
- Added `auto_embed_rules` JSONB column to `embedding_set`
- Created `AutoEmbedRules` struct with serde serialization
- Default rules applied when column is NULL
- Job types added: `EmbedNotes`, `ReEmbedAll`, `PruneOrphanedEmbeddings`

**Future Integration Points:**
- Note repository triggers check auto_embed_rules on insert/update
- Membership changes (add_members/remove_members) trigger embedding jobs
- Scheduler reads `schedule` field for periodic rebuilds

## References

- Issue #386: Auto-Embed Rules
- [Embedding Sets Documentation](docs/content/embedding-sets.md)
- Related: ADR-022 (Set Types), ADR-023 (MRL Support)
