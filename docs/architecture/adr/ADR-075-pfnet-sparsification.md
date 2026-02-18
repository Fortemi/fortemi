# ADR-075: PFNET Sparsification Strategy

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Issue:** #476

## Context

After SNN pruning (ADR-073, step 2), the semantic graph still contains geometrically redundant edges. In a triangle A-B-C, if the path A->B->C is shorter than the direct edge A->C, then A->C adds no topological information -- it can be removed without disconnecting any reachable paths.

The "seashell pattern" (high-similarity same-domain embeddings) means even after SNN filtering, many edges survive because they connect genuinely similar notes. However, these edges form dense cliques that obscure the graph's backbone structure.

We need a sparsification method that:
- Preserves path connectivity (no node becomes unreachable)
- Retains topology-defining edges (the "skeleton" of the graph)
- Is parameter-light (avoids arbitrary threshold selection)
- Works on weighted graphs where weight = similarity (converted to distance for path computation)

## Decision

Use **PFNET(infinity, 2)**, which is equivalent to the Relative Neighborhood Graph (RNG, Toussaint 1980). For each edge (A, B) with distance d(A,B):
- Find all witness nodes W that are neighbors of both A and B
- Compute the indirect path cost: max(d(A,W), d(W,B)) (the L-infinity/minimax criterion with q=2)
- If any witness W provides an indirect path cost <= d(A,B), the edge is redundant and pruned

Distance is derived from similarity: d = 1.0 - similarity.

The q parameter (default 2, configurable via `GRAPH_PFNET_Q`) controls sparsity:
- q=2: RNG-equivalent, moderate sparsification
- Higher q: sparser graph approaching minimum spanning tree
- q>2 gated to N<=1000 nodes due to computational cost

**Graph PFNET optimization:** Only considers witnesses from neighbors(A) union neighbors(B) in the input edge set, rather than all nodes. This reduces complexity from O(E*N^2) to O(E*N) in practice.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Simple threshold pruning | Arbitrary threshold; no topology preservation guarantee |
| Betweenness centrality | O(V*E) computation; removes high-betweenness edges (bridges), which is opposite of what we want |
| Minimum spanning tree | Too aggressive; loses all redundancy, single point of failure per path |
| k-nearest neighbor graph | Already applied upstream (HNSW/SNN); doesn't address geometric redundancy |

## Consequences

### Positive
- (+) Retains 5-15% of edges on typical corpora, revealing backbone structure
- (+) Preserves path connectivity: no node becomes unreachable
- (+) Parameter-light: q=2 works well as default, no threshold tuning needed
- (+) Topology-preserving: important structural edges (bridges, bottlenecks) survive
- (+) Results stored in edge metadata (`pfnet_retained: true`) for diagnostics

### Negative
- (-) O(E*N) complexity per run; may be slow on graphs with >10K edges
- (-) Aggressive sparsification can obscure genuine multi-path relationships
- (-) Must run after SNN (step ordering dependency in pipeline)
- (-) Soft-delete (metadata flag) means pruned edges still exist in DB

## Implementation

**Code Location:**
- Algorithm: `crates/matric-db/src/links.rs` (`PgLinkRepository::pfnet_sparsify_tx`)
- Configuration: `crates/matric-core/src/defaults.rs` (`GraphConfig::pfnet_q`)
- Pipeline step: `crates/matric-api/src/handlers/jobs.rs` (`GraphMaintenanceHandler`, step 3)
- Diagnostics: `crates/matric-db/src/links.rs` (`pfnet_retention_ratio` in `GraphDiagnostics`)

**API Endpoint:**

```
POST /api/v1/graph/pfnet/sparsify
{
  "q": 2,
  "dry_run": false
}
```

**Result Structure:**

```rust
pub struct PfnetResult {
    pub retained: i64,
    pub pruned: i64,
    pub retention_ratio: f64,
    pub q_used: usize,
    pub dry_run: bool,
}
```

**Unit Tests:** 6 tests covering empty graph, single edge, linear chain, triangle pruning, equilateral triangle, and complex topologies.

## References

- Schvaneveldt, R. W. "Pathfinder Associative Networks." (1990)
- Toussaint, G. T. "The Relative Neighbourhood Graph of a Finite Planar Set." (1980)
- ADR-073: Graph Quality Pipeline Architecture
- Issue #476: PFNET Sparsification
