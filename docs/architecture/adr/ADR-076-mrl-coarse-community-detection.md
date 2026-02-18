# ADR-076: MRL Coarse Community Detection

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Issue:** #481

## Context

Full 768-dimensional cosine similarities from nomic-embed-text concentrate in a narrow range (~0.62-0.94) on same-domain corpora. This compressed range provides poor input for Louvain community detection because:

- Modularity optimization needs edge weight variance to distinguish intra-community vs. inter-community edges
- When all similarities are between 0.62 and 0.94, the algorithm cannot find meaningful community boundaries
- Normalization (gamma exponent) helps but cannot create variance that does not exist in the raw similarities

Matryoshka Representation Learning (MRL, ADR-023) encodes semantic information at multiple scales. Lower-dimensional truncations lose fine-grained precision but gain wider similarity spread -- exactly what community detection needs.

Empirical measurement on a ~140-note same-domain corpus:
- 768 dims: similarity range 0.62-0.94 (spread 0.32)
- 64 dims: similarity range 0.30-0.90 (spread 0.60)

The ~2x wider spread at 64 dimensions produces clearer cluster boundaries for Louvain.

## Decision

Use the first 64 dimensions of existing MRL-capable embeddings for community detection, computed via pgvector array truncation at query time:

```sql
(e.vector::float4[])[1:64]::vector
```

This reuses existing 768-dim embeddings stored in the `embedding` table -- no separate embedding set or re-embedding required. The truncation is applied in the SQL query within `coarse_community_detection_tx`.

A dedicated endpoint (`POST /api/v1/graph/community/coarse`) allows on-demand coarse community detection with configurable parameters:
- `coarse_dim` (default 64): number of MRL dimensions to use
- `similarity_threshold` (default 0.3): minimum similarity for edge inclusion
- `resolution` (default 1.0): Louvain resolution parameter

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| PCA dimensionality reduction | Requires computing covariance matrix across all embeddings; expensive and non-incremental |
| Separate low-dim model | Doubles embedding compute and storage; MRL achieves the same result for free |
| Random projection | Less semantically meaningful than MRL truncation; unpredictable quality |
| Full-dim clustering with adjusted threshold | Narrow similarity range still defeats modularity optimization |

## Consequences

### Positive
- (+) ~2x wider similarity spread (0.30-0.90 vs. 0.62-0.94) for better clustering
- (+) Zero additional storage: reuses existing 768-dim vectors via SQL truncation
- (+) Faster pairwise computation: 64-dim cosine is ~12x cheaper than 768-dim
- (+) Configurable dimension: can experiment with 64, 128, 256 via API parameter
- (+) Leverages MRL property already supported by nomic-embed-text (ADR-023)

### Negative
- (-) Only works with MRL-capable models (nomic-embed-text supports it; others may not)
- (-) 64-dim truncation loses fine-grained semantic distinctions within communities
- (-) SQL array truncation `(vector::float4[])[1:N]::vector` bypasses HNSW index (full table scan)
- (-) Community assignments from coarse detection may disagree with full-dim graph structure

## Implementation

**Code Location:**
- Algorithm: `crates/matric-db/src/links.rs` (`PgLinkRepository::coarse_community_detection_tx`)
- API handler: `crates/matric-api/src/main.rs` (`coarse_community_detection`)
- MCP tool: `mcp-server/tools.js` (`coarse_community_detection`)

**Key Query:**

```sql
SELECT e1.note_id AS note_a, e2.note_id AS note_b,
       1.0 - ((e1.vector::float4[])[1:$1]::vector
           <=> (e2.vector::float4[])[1:$1]::vector) AS similarity
FROM embedding e1
JOIN embedding e2
  ON e1.note_id < e2.note_id
 AND e1.chunk_index = 0 AND e2.chunk_index = 0
WHERE e1.chunk_index = 0
  AND 1.0 - ((e1.vector::float4[])[1:$1]::vector
          <=> (e2.vector::float4[])[1:$1]::vector) >= $2::real
```

**Result Structure:**

```rust
pub struct CoarseCommunityResult {
    pub note_count: usize,
    pub edge_count: usize,
    pub coarse_dim: i32,
    pub similarity_threshold: f32,
    pub community_count: usize,
    pub modularity_q: f64,
    pub largest_community_ratio: f64,
    pub communities: Vec<CommunityInfo>,
}
```

## References

- ADR-023: Matryoshka Representation Learning
- ADR-074: Louvain Community Detection
- Kusupati, A., et al. "Matryoshka Representation Learning." (2022)
- Issue #481: Graph Quality Overhaul Epic
