# ADR-022: Embedding Set Types (Filter vs Full)

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team, implemented via issues #384-#389

## Context

Matric Memory initially supported only one embedding strategy: all notes shared embeddings from a single default embedding set. This "filter set" approach works well for homogeneous content but has limitations:

1. **Model lock-in**: All content uses the same embedding model, even when different models would be more appropriate (e.g., code vs. prose)
2. **No dimension optimization**: Cannot use different vector dimensions for different use cases
3. **Cross-contamination**: Semantic search across unrelated domains can produce poor results
4. **No isolation**: Cannot maintain separate embedding configurations for different projects or tenants

Users need the ability to create fully independent embedding sets with their own models, dimensions, and lifecycle.

## Decision

Introduce a `set_type` enum with two values:

- **Filter** (default): Lightweight sets that reference embeddings from the default set. Notes can belong to multiple filter sets. Embeddings are shared and deduplicated.

- **Full**: Independent sets with dedicated embeddings. Each full set maintains its own embedding vectors, can use a different embedding model, and supports dimension truncation via MRL.

Key design choices:
1. Default to `filter` for backward compatibility
2. Full sets require explicit `embedding_config_id` specification
3. Full sets track `embeddings_current` separately from `embedding_count`
4. Modified unique constraint on `embedding` table to allow same note in multiple full sets

## Consequences

### Positive
- (+) Support for heterogeneous content with appropriate models per domain
- (+) Tenant/project isolation with independent embedding spaces
- (+) Backward compatible - existing sets remain filter type
- (+) Storage efficiency - filter sets share embeddings, full sets isolate when needed
- (+) Enables future features: fine-tuning per set, custom similarity thresholds

### Negative
- (-) Increased complexity in embedding pipeline (must check set type)
- (-) Storage overhead for full sets (duplicate vectors if same note in multiple full sets)
- (-) Migration complexity when converting filter→full or vice versa
- (-) Query routing must consider set type for optimal performance

## Implementation

**Code Location:**
- Schema: `migrations/20260201500000_full_embedding_sets.sql`
- Models: `crates/matric-core/src/models.rs` (EmbeddingSetType enum)
- Repository: `crates/matric-db/src/embedding_sets.rs`

**Key Changes:**
- Added `embedding_set_type` enum to PostgreSQL
- Added `set_type` column to `embedding_set` table (default: 'filter')
- Modified `embedding` unique constraint: `(note_id, chunk_index, embedding_set_id)`
- Updated all queries to include `set_type` field

## References

- Issue #384: Embedding Set Types
- [Embedding Sets Documentation](docs/content/embedding-sets.md)
