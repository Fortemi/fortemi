# ADR-014: pgvector with HNSW Indexing

**Status:** Accepted
**Date:** 2026-01-02
**Deciders:** roctinam
**Research:** REF-031 (Malkov & Yashunin, 2020)

## Context

matric-memory requires vector similarity search for semantic retrieval. Key requirements:
- Sub-millisecond query latency at 100K+ vectors
- 95%+ recall for finding relevant documents
- Incremental updates without full index rebuilds
- PostgreSQL integration (unified data layer)

Options considered:
1. **Dedicated vector DB** (Pinecone, Qdrant) - Separate infrastructure
2. **In-memory** (Faiss) - Process restarts lose index
3. **pgvector IVFFlat** - Requires periodic retraining
4. **pgvector HNSW** - Approximate nearest neighbor with incremental updates

## Decision

Use **pgvector 0.4.1+** with **HNSW indexes** for vector similarity search.

HNSW (Hierarchical Navigable Small World) was chosen based on research findings:
- O(log N) query complexity vs O(N) brute force
- 95%+ recall at sub-millisecond latencies
- Incremental insertion without rebuilds

## Consequences

### Positive
- (+) Unified data layer (vectors + metadata in same DB)
- (+) ACID transactions span vectors and notes
- (+) O(log N) query time vs O(N) brute force
- (+) No external service dependencies
- (+) pgvector is actively maintained, PostgreSQL native

### Negative
- (-) PostgreSQL memory constraints for large vector sets
- (-) Index build time O(N × M × log N)
- (-) Scaling beyond 10M vectors may require sharding
- (-) HNSW parameter tuning needed for optimal performance

## Implementation

**Code Location:**
- Index DDL: `migrations/20260102000000_initial_schema.sql`
- Embedding sets: `crates/matric-db/src/embedding_sets.rs`
- Core types: `crates/matric-core/src/models.rs`

**Index Creation:**

```sql
-- HNSW index for fast approximate vector search
CREATE INDEX idx_embedding_vector
ON embedding USING hnsw (vector vector_cosine_ops);

-- For embedding sets with configurable parameters
CREATE INDEX idx_embedding_set_vector
ON embedding_set_membership USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);
```

**HNSW Parameters:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| M | 16 | Connections per node (memory/recall tradeoff) |
| ef_construction | 64 | Build-time neighbors considered |
| ef_search | 100 | Query-time neighbors considered |

```rust
// crates/matric-core/src/models.rs
pub struct EmbeddingConfig {
    pub hnsw_m: Option<i32>,             // Default: 16
    pub hnsw_ef_construction: Option<i32>, // Default: 64
    pub ivfflat_lists: Option<i32>,      // Fallback option
    // ...
}
```

**Query Pattern:**

```sql
-- Set search accuracy (higher = better recall, slower)
SET LOCAL hnsw.ef_search = 100;

-- Find k nearest neighbors
SELECT note_id, 1 - (embedding <=> $1::vector) AS similarity
FROM embedding
WHERE 1 - (embedding <=> $1::vector) > 0.7
ORDER BY embedding <=> $1::vector
LIMIT 20;
```

**Performance Characteristics (from REF-031):**

| N (vectors) | Brute Force | HNSW | Speedup |
|-------------|-------------|------|---------|
| 10K | 10ms | 0.5ms | 20x |
| 100K | 100ms | 0.7ms | 143x |
| 1M | 1000ms | 1ms | 1000x |

## Research Citations

> "HNSW achieves O(log N) query time with high probability." (REF-031, Malkov & Yashunin, 2020, Section 3)

> "HNSW outperforms all other tested algorithms in terms of the trade-off between search quality and search speed." (REF-031, Section 5)

> "HNSW supports efficient incremental insertion without rebuilding the index." (REF-031, Section 4)

## References

- `.aiwg/research/paper-analysis/REF-031-mm-analysis.md`
- `.aiwg/research/citable-claims-index.md` (Vector Indexing section)
- pgvector documentation: https://github.com/pgvector/pgvector
