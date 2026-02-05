# REF-027: Reciprocal Rank Fusion - matric-memory Analysis

**Paper:** Cormack, G. V., Clarke, C. L. A., & Buttcher, S. (2009). Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank Learning Methods.

**Analysis Date:** 2026-01-25
**Relevance:** Critical - Core hybrid search implementation

---

## Implementation Mapping

| RRF Concept | matric-memory Implementation | Location |
|-------------|------------------------------|----------|
| RRF formula | `rrf_score()` function | `crates/matric-search/src/hybrid.rs:186` |
| k=60 constant | Configuration parameter | `crates/matric-search/src/config.rs` |
| Rank aggregation | Result merging after BM25 + semantic | `crates/matric-search/src/hybrid.rs` |
| Multiple rankers | BM25 (FTS) + Dense retrieval | `crates/matric-db/src/search.rs`, `matric-inference` |
| Score normalization | Implicit in RRF formula | No explicit normalization needed |

---

## matric-memory as Hybrid Search System

### The Fusion Problem

matric-memory faces the classic IR fusion problem: combining results from fundamentally different retrieval methods.

```
Traditional Approach:
- Run BM25 search → lexical matches
- Run semantic search → conceptual matches
- How to combine? Different score distributions!

matric-memory Solution (via RRF):
- Run BM25 search → rank positions (not scores)
- Run semantic search → rank positions
- RRF fusion on ranks → unified results
```

### RRF in matric-memory Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Search Query                          │
└─────────────────────────────────────────────────────────┘
                           │
           ┌───────────────┴───────────────┐
           ▼                               ▼
┌─────────────────────┐       ┌─────────────────────┐
│   BM25 Search       │       │  Semantic Search    │
│   (PostgreSQL FTS)  │       │  (pgvector HNSW)    │
│                     │       │                     │
│   doc_a: rank 1     │       │   doc_c: rank 1     │
│   doc_b: rank 2     │       │   doc_a: rank 2     │
│   doc_c: rank 3     │       │   doc_b: rank 3     │
└─────────────────────┘       └─────────────────────┘
           │                               │
           └───────────────┬───────────────┘
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    RRF Fusion                            │
│                                                          │
│   score(d) = Σ 1/(k + rank_i(d))  where k=60            │
│                                                          │
│   doc_a: 1/(60+1) + 1/(60+2) = 0.0164 + 0.0161 = 0.0325 │
│   doc_c: 1/(60+3) + 1/(60+1) = 0.0159 + 0.0164 = 0.0323 │
│   doc_b: 1/(60+2) + 1/(60+3) = 0.0161 + 0.0159 = 0.0320 │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│              Final Ranking: doc_a, doc_c, doc_b          │
└─────────────────────────────────────────────────────────┘
```

### Implementation Details

```rust
// crates/matric-search/src/hybrid.rs

/// Reciprocal Rank Fusion score calculation
/// Based on Cormack et al. 2009 (REF-027)
pub fn rrf_score(ranks: &[usize], k: f32) -> f32 {
    ranks.iter()
        .map(|&rank| 1.0 / (k + rank as f32))
        .sum()
}

/// Merge results from multiple retrieval methods
pub fn hybrid_search(
    fts_results: Vec<SearchResult>,
    semantic_results: Vec<SearchResult>,
    k: f32,  // RRF constant, default 60.0
) -> Vec<SearchResult> {
    let mut doc_ranks: HashMap<Uuid, Vec<usize>> = HashMap::new();

    // Collect ranks from FTS results
    for (rank, result) in fts_results.iter().enumerate() {
        doc_ranks.entry(result.note_id)
            .or_insert_with(Vec::new)
            .push(rank + 1);  // 1-indexed ranks
    }

    // Collect ranks from semantic results
    for (rank, result) in semantic_results.iter().enumerate() {
        doc_ranks.entry(result.note_id)
            .or_insert_with(Vec::new)
            .push(rank + 1);
    }

    // Calculate RRF scores and sort
    let mut fused: Vec<_> = doc_ranks.iter()
        .map(|(id, ranks)| (*id, rrf_score(ranks, k)))
        .collect();

    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Return top results with RRF scores
    fused.into_iter()
        .take(limit)
        .map(|(id, score)| SearchResult { note_id: id, score })
        .collect()
}
```

---

## Benefits Mirroring RRF Research Findings

### 1. No Score Calibration Required

**Paper Finding:**
> "RRF makes no assumption about the distribution of scores from individual rankers." (p. 758)

**matric-memory Benefit:**
- BM25 scores: logarithmic, document-length dependent
- Cosine similarity: bounded [0, 1]
- RRF ignores scores entirely, using only rank positions
- No normalization code needed, simpler implementation

### 2. Robust Without Tuning

**Paper Finding:**
> "k=60 was determined empirically to be a good value... neither too aggressive nor too conservative." (p. 758)

**matric-memory Benefit:**
- Single constant works across all query types
- No per-collection tuning required
- Consistent behavior as knowledge base grows

### 3. Outperforms Learned Methods

**Paper Finding:**

| Method | MAP Score | vs BM25 |
|--------|-----------|---------|
| Best Individual | 0.3586 | baseline |
| Borda Count | 0.3614 | +0.8% |
| Condorcet Fuse | 0.3652 | +1.8% |
| CombMNZ | 0.3575 | -0.3% |
| **RRF (k=60)** | **0.3686** | **+2.8%** |

**matric-memory Benefit:**
- Better quality than either method alone
- No training data required
- Immediate improvement from combining existing searches

### 4. Handles Missing Documents Gracefully

**Paper Finding:**
> "Documents appearing in only one list receive a score from that list alone." (p. 758)

**matric-memory Benefit:**
- Notes with exact keyword matches (BM25 only) still rank well
- Notes with semantic similarity (semantic only) still rank well
- No penalty for single-method matches

---

## Comparison: Traditional vs matric-memory Approach

| Aspect | Traditional Score Fusion | matric-memory RRF |
|--------|--------------------------|-------------------|
| Input | Raw scores from rankers | Rank positions only |
| Calibration | Required per-ranker | None needed |
| Score distribution | Must be compatible | Irrelevant |
| Tuning | Multiple weights | Single k parameter |
| Training data | Often required | Not needed |
| New ranker | Re-calibrate all | Just add ranks |
| Implementation | Complex normalization | Simple formula |

### Why RRF Over Alternatives

**CombSUM/CombMNZ:**
- Require comparable score scales
- BM25 and cosine similarity are not comparable
- Would need min-max or z-score normalization

**Learned Fusion:**
- Requires relevance judgments
- matric-memory has no labeled training data
- Would need user feedback collection

**Borda Count:**
- Only considers presence in top-k
- Loses granularity of exact positions
- RRF preserves position information

---

## RRF Configuration in matric-memory

### The k Parameter

The k value controls the balance between high-ranking and lower-ranking documents:

```
k small (e.g., k=10):
  rank 1: 1/11 = 0.091  (strong influence)
  rank 10: 1/20 = 0.050 (still significant)
  ratio: 1.8x

k large (e.g., k=100):
  rank 1: 1/101 = 0.0099
  rank 10: 1/110 = 0.0091
  ratio: 1.1x (nearly equal)
```

**matric-memory uses k=60:**
- Balances top positions with deeper results
- Matches paper's empirical optimum
- Conservative enough to avoid over-weighting rank 1

### Weight Adjustment (Optional Enhancement)

For collections with known BM25/semantic preferences:

```rust
// Weighted RRF variant
pub fn weighted_rrf_score(
    bm25_rank: Option<usize>,
    semantic_rank: Option<usize>,
    k: f32,
    bm25_weight: f32,  // e.g., 1.0
    semantic_weight: f32,  // e.g., 1.2 for semantic-heavy
) -> f32 {
    let mut score = 0.0;
    if let Some(r) = bm25_rank {
        score += bm25_weight / (k + r as f32);
    }
    if let Some(r) = semantic_rank {
        score += semantic_weight / (k + r as f32);
    }
    score
}
```

---

## Cross-References

### Related Papers

| Paper | Relationship to RRF |
|-------|---------------------|
| REF-028 (BM25) | Provides lexical ranks for fusion |
| REF-029 (DPR) | Provides semantic ranks for fusion |
| REF-056 (ColBERT) | Could add third ranker to fusion |

### Related Code Locations

| File | RRF Usage |
|------|-----------|
| `crates/matric-search/src/hybrid.rs` | Core RRF implementation |
| `crates/matric-search/src/config.rs` | k parameter configuration |
| `crates/matric-api/src/handlers/search.rs` | Search endpoint orchestration |
| `mcp-server/src/tools/search.ts` | MCP search tool |

---

## Improvement Opportunities

### 1. Add More Rankers to Fusion

**Current:** 2 rankers (BM25 + semantic)

**Potential additions:**
- Title-only BM25 (boost exact title matches)
- Tag-based retrieval (notes with matching tags)
- Recency boost (recently modified notes)
- Link proximity (notes linked to query-relevant notes)

RRF naturally handles N rankers without modification.

### 2. Query-Dependent k Selection

**Research direction:** Different queries may benefit from different k values.

```rust
// Hypothetical adaptive k
fn adaptive_k(query: &str) -> f32 {
    if query.len() < 20 {
        40.0  // Short queries: favor top positions
    } else if query.contains('"') {
        30.0  // Phrase queries: exact matches important
    } else {
        60.0  // Default
    }
}
```

### 3. RRF with Score Tie-Breaking

When RRF scores are equal, use underlying scores:

```rust
// Tie-breaking enhancement
fused.sort_by(|a, b| {
    match b.rrf_score.partial_cmp(&a.rrf_score) {
        Some(Ordering::Equal) => {
            // Tie-break by semantic similarity
            b.semantic_score.partial_cmp(&a.semantic_score).unwrap()
        }
        other => other.unwrap()
    }
});
```

### 4. Expose RRF Diagnostics

For debugging and tuning:

```rust
pub struct RRFDiagnostics {
    pub note_id: Uuid,
    pub bm25_rank: Option<usize>,
    pub semantic_rank: Option<usize>,
    pub rrf_score: f32,
    pub contribution_bm25: f32,
    pub contribution_semantic: f32,
}
```

---

## Critical Insights for matric-memory Development

### 1. Simplicity is a Feature

RRF's main advantage is simplicity. Resist adding complexity:
- Don't add learned weights without evidence of improvement
- Don't calibrate scores when ranks suffice
- Don't over-tune k without A/B testing

### 2. Rank Quality Matters More Than Fusion

> "The quality of the fused result depends primarily on the quality of the individual rankers." (p. 759)

**Implication:** Improving BM25 or semantic search individually provides more benefit than fusion tweaks.

### 3. RRF Enables Experimentation

Adding a new retrieval method is trivial with RRF:

```rust
// Adding title search is one line
hybrid_search(vec![
    bm25_results,
    semantic_results,
    title_results,  // New ranker added
], k)
```

### 4. Missing Ranks Are Not Errors

Documents appearing in only one list are expected and handled naturally. Don't treat single-source results as lower quality.

---

## Key Quotes Relevant to matric-memory

> "Reciprocal Rank Fusion is simple to implement and requires no training or tuning." (p. 758)
>
> **Relevance:** matric-memory values simplicity and has no relevance labels for training.

> "The constant k=60 was determined empirically... and shows remarkable consistency across datasets." (p. 758)
>
> **Relevance:** Single value works for diverse note collections without per-user tuning.

> "RRF is a strong baseline that is hard to beat, and indeed raises the bar for the lower bound of what can be learned." (p. 759)
>
> **Relevance:** We can be confident in RRF quality while exploring more sophisticated methods.

> "Fusion of more than two systems continues to improve results, with diminishing returns." (p. 759)
>
> **Relevance:** Adding title search or tag-based search would provide incremental improvement.

---

## Summary

REF-027 provides the theoretical and empirical foundation for matric-memory's hybrid search. RRF's key contribution is enabling combination of incompatible score distributions through rank-based fusion, with a single robust parameter (k=60) that requires no tuning.

**Implementation Status:** Complete
**Configuration Status:** Using paper-recommended k=60
**Test Coverage:** Search integration tests verify hybrid results
**Future Work:** Consider additional rankers for fusion

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |
