# ADR-074: Louvain Community Detection (Pure Rust)

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Issue:** #473

## Context

The graph payload (v1 contract, #467) includes `community_id`, `community_label`, and `community_confidence` fields on each `GraphNode`. These fields must be populated server-side so that clients can render community-colored visualizations and community-scoped navigation without running their own clustering algorithms.

Requirements:
- Deterministic: same graph produces same communities across runs
- Configurable resolution for controlling community granularity
- Labels derived from SKOS concept taxonomy (not arbitrary numeric IDs)
- No external crate dependencies for the core algorithm (minimize supply chain risk)
- Handles disconnected components gracefully

## Decision

Implement Louvain modularity optimization as a pure Rust function (`assign_communities`) within `PgLinkRepository`. Community labels are derived post-hoc from SKOS concepts via `label_communities_skos`, which selects the most-used concept label among a community's member notes.

**Algorithm:** Standard two-phase Louvain:
1. **Local moves:** Each node greedily moves to the neighboring community that maximizes modularity gain (delta_Q). Nodes iterated in UUID sort order for determinism.
2. **Aggregation:** Communities become super-nodes; repeat until no improvement.

Resolution parameter (default 1.0) scales the null-model term, controlling community size:
- Higher resolution (>1.0) produces more, smaller communities
- Lower resolution (<1.0) merges into fewer, larger communities

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| petgraph + community crate | Additional dependency for a <200-line algorithm |
| Leiden algorithm | More complex implementation; Louvain sufficient for <10K node graphs |
| Spectral clustering | Requires eigenvector computation; overkill for this graph scale |
| Client-side detection | Duplicates logic, inconsistent results, no SKOS integration |

## Consequences

### Positive
- (+) O(E) per pass with typically 2-5 passes to convergence
- (+) Deterministic: UUID-sorted iteration order eliminates non-determinism
- (+) Configurable resolution via `GRAPH_COMMUNITY_RESOLUTION` env var (0.1-10.0)
- (+) SKOS-derived labels provide meaningful names ("Machine Learning" vs. "Community 3")
- (+) Zero external dependencies for the algorithm itself

### Negative
- (-) Louvain can produce poorly-connected communities on certain graph topologies
- (-) Pure Rust implementation must be maintained alongside algorithm improvements
- (-) SKOS labeling requires DB access (runs in transaction context)
- (-) Resolution parameter requires tuning per corpus

## Implementation

**Code Location:**
- Algorithm: `crates/matric-db/src/links.rs` (`PgLinkRepository::assign_communities`)
- SKOS labeling: `crates/matric-db/src/links.rs` (`PgLinkRepository::label_communities_skos`)
- Graph traversal integration: `crates/matric-db/src/links.rs` (`explore_graph_v1_tx`)
- Coarse community endpoint: `crates/matric-db/src/links.rs` (`coarse_community_detection_tx`)

**Key Changes:**
- `GraphNode` struct includes `community_id`, `community_label`, `community_confidence` fields
- Communities assigned during graph exploration (populated per-request)
- Coarse community detection runs on MRL-truncated vectors (see ADR-076)
- 6 unit tests: single node, disconnected, two-cluster, clique, determinism, resolution sensitivity

**GraphNode Fields:**

```rust
pub struct GraphNode {
    pub id: Uuid,
    pub title: Option<String>,
    pub depth: i32,
    // ...
    pub community_id: Option<i32>,
    pub community_label: Option<String>,
    pub community_confidence: Option<f32>,
}
```

## References

- Blondel, V. D., et al. "Fast unfolding of communities in large networks." (2008)
- ADR-073: Graph Quality Pipeline Architecture
- ADR-076: MRL Coarse Community Detection
- Issue #473: Louvain Community Detection
