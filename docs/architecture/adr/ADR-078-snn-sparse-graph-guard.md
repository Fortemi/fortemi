# ADR-078: SNN Sparse Graph Guard

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Issue:** #474, #481

## Context

Shared Nearest Neighbor (SNN) scoring requires that nodes have at least k neighbors to produce meaningful intersection counts. SNN(A, B) = |kNN(A) ∩ kNN(B)| / k. When mean graph degree is below k, the intersection is almost always empty — every edge gets a score of 0.0, which would cause the SNN pruning step to delete all edges in the graph.

On small corpora or freshly initialized archives, the graph is sparse by construction. Running SNN pruning unconditionally on a sparse graph destroys all semantic links, leaving no connections in the graph at all. This defeats the purpose of graph maintenance and cannot be undone without a full re-link pass.

## Decision

Skip the SNN pruning step when the graph's mean degree falls below k (the SNN neighborhood size):

```
mean_degree = total_edge_count / node_count
if mean_degree < k:
    log warning and return SnnResult { skipped: true, ... }
```

The guard uses the same k value as the SNN computation (default k=10, configurable via `GRAPH_SNN_K`). The condition is strict (`<` not `<=`) to allow SNN to run on graphs where mean degree exactly equals k.

The result is logged at `warn` level with the actual mean_degree and k values so operators can diagnose why SNN was skipped.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Run SNN regardless and accept mass pruning | Destroys all edges on sparse graphs; unrecoverable without full re-link |
| Lower SNN threshold on sparse graphs | Arbitrary; doesn't address the root cause (insufficient neighbors for meaningful intersection) |
| Skip SNN entirely below a fixed node count | Node count is not the right signal; edge density matters |
| Warn and continue with partial scores | Partial SNN scores have no meaningful interpretation when most intersections are empty |

## Consequences

### Positive
- (+) Prevents complete graph destruction on small or recently initialized archives
- (+) Self-correcting: as the corpus grows past the k-threshold, SNN automatically activates
- (+) Diagnostic logging gives operators clear signal when and why SNN was skipped
- (+) No configuration required; guard uses the same k as the SNN algorithm

### Negative
- (-) Sparse graphs receive no SNN quality improvement; graph density matters for quality
- (-) Operator must check logs to understand why graph topology is not being refined
- (-) Guard condition (mean_degree < k) means SNN is skipped on graphs with many isolated nodes

## Implementation

**Code Location:**
- Guard: `crates/matric-db/src/links.rs` (`PgLinkRepository::recompute_snn`) — early return when `mean_degree < k`
- Configuration: `crates/matric-core/src/defaults.rs` (`GraphConfig::snn_k`)
- Diagnostics: `SnnResult::skipped` field with `mean_degree` and `k` populated on early return

**Guard Logic:**

```rust
let mean_degree = if node_count > 0.0 {
    edge_count as f64 / node_count
} else {
    0.0
};

if mean_degree < k as f64 {
    warn!(
        mean_degree = mean_degree,
        k = k,
        "SNN skipped: graph too sparse (mean degree {:.1} < k={})",
        mean_degree, k
    );
    return Ok(SnnResult { skipped: true, mean_degree, k, ..Default::default() });
}
```

## References

- ADR-073: Graph Quality Pipeline Architecture
- ADR-074: Louvain Community Detection
- ADR-075: PFNET Sparsification Strategy
- Issue #474: SNN Scoring Implementation
- Issue #481: Graph Quality Overhaul Epic
