# ADR-023: Matryoshka Representation Learning (MRL) Support

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team, implemented via issues #384-#389

## Context

Modern embedding models like `nomic-embed-text` support Matryoshka Representation Learning (MRL), which encodes semantic information at multiple scales within a single vector. The first N dimensions contain a valid, lower-dimensional embedding that preserves most semantic meaning.

This enables significant optimizations:
- **Storage**: 768-dim → 256-dim = 3× reduction, 768-dim → 64-dim = 12× reduction
- **Compute**: Smaller vectors = faster similarity calculations
- **Memory**: Smaller HNSW indexes fit in RAM

However, MRL introduces trade-offs:
- Lower dimensions = reduced precision for fine-grained similarity
- Not all models support MRL (truncating non-MRL embeddings destroys meaning)
- Different use cases benefit from different dimension targets

## Decision

Add MRL support with the following design:

1. **Per-set truncation**: `truncate_dim` column on `embedding_set` specifies target dimensions
2. **Model metadata**: `embedding_config` tracks `supports_mrl` and `mrl_dimensions` array
3. **Validation**: Only allow truncation to dimensions in the model's MRL list
4. **Null = native**: When `truncate_dim` is NULL, use model's native dimensions

Valid MRL dimensions for nomic-embed-text: `[64, 128, 256, 512, 768]`

Truncation happens at embedding time:
```sql
-- Store truncated vector
INSERT INTO embedding (vector, ...)
VALUES ($1[1:truncate_dim], ...)
```

## Consequences

### Positive
- (+) 12× storage savings possible (768 → 64 dimensions)
- (+) Faster similarity search with smaller vectors
- (+) Smaller HNSW indexes = better memory efficiency
- (+) Per-set optimization - high-precision sets can use full dimensions
- (+) Graceful degradation - precision loss is predictable and documented

### Negative
- (-) Reduced precision at lower dimensions (documented trade-offs)
- (-) Model dependency - must track which models support MRL
- (-) No post-hoc truncation - must re-embed to change dimensions
- (-) Index rebuilding required when changing truncate_dim

## Implementation

**Code Location:**
- Schema: `migrations/20260201500000_full_embedding_sets.sql`
- Models: `crates/matric-core/src/models.rs` (EmbeddingConfigProfile.mrl_dimensions)
- Repository: `crates/matric-db/src/embedding_sets.rs`

**Key Changes:**
- Added `truncate_dim` column to `embedding_set` (nullable integer)
- Added `supports_mrl` boolean and `mrl_dimensions` integer array to `embedding_config`
- Seeded MRL-aware configs: nomic-embed-text with [64, 128, 256, 512, 768]
- Validation prevents invalid dimension values

**Dimension Trade-offs (nomic-embed-text):**

| Dimensions | Storage | Precision | Use Case |
|------------|---------|-----------|----------|
| 768 | 100% | Highest | Research, fine-grained similarity |
| 512 | 67% | High | General purpose |
| 256 | 33% | Good | Most applications |
| 128 | 17% | Moderate | High-volume, coarse matching |
| 64 | 8% | Basic | Massive scale, rough clustering |

## References

- Issue #385: MRL Truncation Support
- [Nomic Embed MRL Documentation](https://docs.nomic.ai/reference/endpoints/embed-text)
- [Matryoshka Representation Learning Paper](https://arxiv.org/abs/2205.13147)
- [Embedding Model Selection Guide](docs/content/embedding-model-selection.md)
