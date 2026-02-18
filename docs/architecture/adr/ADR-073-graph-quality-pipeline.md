# ADR-073: Graph Quality Pipeline Architecture

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Epic:** #481 (Graph Quality Overhaul)

## Context

Fortemi's knowledge graph uses 768-dimensional embeddings from nomic-embed-text to compute cosine similarity between notes. On same-domain corpora (e.g., all Rust programming notes, all ML research), these embeddings produce cosine similarities concentrated in a narrow band (~0.70-0.94) -- the "seashell pattern." This makes all nodes appear roughly equidistant, which degrades:

- **Linking quality:** The 0.7 semantic threshold (ADR-012) admits too many edges, producing a near-complete subgraph
- **Graph visualization:** Dense, uninformative hairball with no visible structure
- **Community detection:** Louvain modularity optimization fails when edge weights lack variance
- **Navigation:** Every note links to every other note, defeating the purpose of semantic linking

Single-pass approaches (e.g., raising the threshold to 0.85) help marginally but discard genuine cross-topic connections while keeping intra-topic noise.

## Decision

Implement a multi-stage graph quality pipeline that runs as a `GraphMaintenance` background job:

1. **Normalize** -- Apply gamma-exponent normalization to edge weights, amplifying top-end differences in the compressed similarity range
2. **SNN (Shared Nearest Neighbors)** -- Compute SNN scores (|kNN(A) intersection kNN(B)| / k) and prune edges below threshold, keeping only structurally significant connections
3. **PFNET (Pathfinder Network)** -- Remove geometrically redundant edges where a witness node provides a shorter indirect path (see ADR-075)
4. **Snapshot** -- Save diagnostics snapshot for before/after comparison

Community detection (Louvain, ADR-074) runs separately via the graph traversal path and coarse community endpoint.

Each step is independently configurable via environment variables and can be selectively included or excluded per invocation via the `steps` array.

## Consequences

### Positive
- (+) Modular pipeline: each stage addresses a distinct graph quality problem
- (+) Configurable via `GraphConfig` environment variables (no restart required)
- (+) Selective step execution allows targeted maintenance
- (+) Diagnostics snapshots enable before/after quality comparison
- (+) 20 unit tests cover SNN scoring, PFNET classification, Louvain clustering, and edge cases

### Negative
- (-) Multi-stage pipeline adds complexity vs. single-threshold approach
- (-) Pipeline execution time scales with edge count (SNN is O(E*k), PFNET is O(E*N))
- (-) Step ordering matters: SNN before PFNET produces different results than PFNET before SNN

## Implementation

**Code Location:**
- Pipeline orchestration: `crates/matric-api/src/handlers/jobs.rs` (`GraphMaintenanceHandler`)
- Graph algorithms: `crates/matric-db/src/links.rs` (`PgLinkRepository`)
- Configuration: `crates/matric-core/src/defaults.rs` (`GraphConfig`)
- Job type: `crates/matric-core/src/models.rs` (`JobType::GraphMaintenance`)
- Migration: `migrations/20260218400000_add_graph_maintenance_job_type.sql`

**Key Configuration (GraphConfig):**

| Variable | Default | Description |
|----------|---------|-------------|
| `GRAPH_NORMALIZATION_GAMMA` | 1.0 | Gamma exponent for edge weight normalization |
| `GRAPH_SNN_THRESHOLD` | 0.10 | SNN score below which edges are pruned |
| `GRAPH_PFNET_Q` | 2 | PFNET q parameter (2 = RNG-equivalent) |
| `GRAPH_COMMUNITY_RESOLUTION` | 1.0 | Louvain resolution parameter |

**API Endpoint:**

```
POST /api/v1/graph/maintenance
{
  "steps": ["normalize", "snn", "pfnet", "snapshot"]
}
```

## References

- ADR-012: Semantic Linking Threshold
- ADR-074: Louvain Community Detection
- ADR-075: PFNET Sparsification Strategy
- ADR-076: MRL Coarse Community Detection
- ADR-077: Embedding Content Separation
- Issue #481: Graph Quality Overhaul Epic
