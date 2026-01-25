# ADR-011: Hybrid Search with RRF Fusion

**Status:** Accepted
**Date:** 2026-01-02
**Deciders:** roctinam
**Research:** REF-027 (Cormack et al., 2009), REF-028 (Robertson & Zaragoza, 2009), REF-029 (Karpukhin et al., 2020)

## Context

matric-memory requires search functionality that captures both:
1. **Exact term matches** - Technical vocabulary, specific phrases, code references
2. **Semantic similarity** - Conceptually related content even without keyword overlap

Single-mode search has known limitations:
- BM25/FTS alone misses semantic connections ("database" vs "PostgreSQL")
- Semantic search alone can miss exact technical terms

Industry research shows hybrid approaches outperform either method alone.

## Decision

Implement hybrid search combining three components:
1. **BM25 Full-Text Search** (PostgreSQL `ts_rank`) - Lexical matching
2. **Semantic Vector Search** (pgvector cosine similarity) - Conceptual matching
3. **Reciprocal Rank Fusion (RRF)** - Score combination with k=60

RRF was chosen over alternatives (linear combination, Condorcet) based on research showing it "consistently outperforms both individual rankers and complex fusion methods" (REF-027).

## Consequences

### Positive
- (+) 4-5% improvement over best individual ranker (per REF-027 experiments)
- (+) No tuning required - k=60 works across diverse collections
- (+) Graceful degradation if one component has no results
- (+) Captures both exact matches and conceptual similarity
- (+) Normalized scores (0.0-1.0) for consistent thresholding

### Negative
- (-) Higher latency (two searches + fusion)
- (-) More complex query pipeline
- (-) Both FTS and vector indices required
- (-) Debug complexity when results seem unexpected

## Implementation

**Code Location:** `crates/matric-search/src/`
- `hybrid.rs` - Search orchestration
- `rrf.rs` - Fusion algorithm

**RRF Algorithm (from REF-027):**

```rust
// crates/matric-search/src/rrf.rs

/// RRF constant (empirically optimal per REF-027 p.758)
pub const RRF_K: f32 = 60.0;

pub fn rrf_fuse(ranked_lists: Vec<Vec<SearchHit>>, limit: usize) -> Vec<SearchHit> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();

    for list in ranked_lists {
        for (rank, hit) in list.into_iter().enumerate() {
            // RRF formula: 1 / (k + rank)
            let rrf_score = 1.0 / (RRF_K + (rank as f32) + 1.0);
            *scores.entry(hit.note_id).or_insert(0.0) += rrf_score;
        }
    }

    // Normalize to 0.0-1.0 range
    let max_possible = num_lists as f32 / (RRF_K + 1.0);
    // ... sort and return top-k
}
```

**Search Pipeline:**

```
Query
  ├── FTS Search (PostgreSQL ts_rank)
  │     └── BM25 parameters: k1=1.2, b=0.75 (per REF-028)
  │
  ├── Semantic Search (pgvector)
  │     └── Query embedding → cosine similarity
  │
  └── RRF Fusion (k=60)
        └── Normalized combined scores
```

**Configuration:**

```rust
pub struct HybridSearchConfig {
    pub fts_weight: f32,       // Default: 0.5
    pub semantic_weight: f32,  // Default: 0.5
    pub min_score: f32,        // Default: 0.0
    // ...
}
```

## Research Citations

> "RRF is a strong baseline that is hard to beat, and indeed raises the bar for the lower bound of what can be learned." (REF-027, Cormack et al., 2009, p. 759)

> "BM25 with k1=1.2, b=0.75 provides robust baseline ranking across diverse collections." (REF-028, Robertson & Zaragoza, 2009)

## References

- `.aiwg/research/paper-analysis/REF-027-mm-analysis.md`
- `.aiwg/research/paper-analysis/REF-028-mm-analysis.md`
- `.aiwg/research/citable-claims-index.md` (Hybrid Search section)
