# ADR-078: SNN Sparse and Planned-Retention Guards

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam
**Amended:** 2026-08-28
**Issue:** #474, #481, #1102

## Context

Shared Nearest Neighbor (SNN) scoring requires that nodes have at least k neighbors to produce meaningful intersection counts. SNN(A, B) = |kNN(A) ∩ kNN(B)| / k. When mean graph degree is below k, the intersection is almost always empty — every edge gets a score of 0.0, which would cause the SNN pruning step to delete all edges in the graph.

On small corpora or freshly initialized archives, the graph is sparse by construction. Running SNN pruning unconditionally on a sparse graph destroys all semantic links, leaving no connections in the graph at all. This defeats the purpose of graph maintenance and cannot be undone without a full re-link pass.

The pre-run density check is necessary but not sufficient. A production corpus with 1,583 notes and 11,449 edges passed the `mean_degree >= k` check, yet its fully computed SNN plan retained only 132 edges (1.15%). The old guard could detect the resulting sparse graph only on the next run, after the destructive transaction committed.

## Decision

Skip the SNN pruning step when the graph's mean degree falls below k (the SNN neighborhood size):

```
mean_degree = total_edge_count / node_count
if mean_degree < k:
    log warning and return SnnResult { skipped: true, ... }
```

The guard uses the same k value as the SNN computation (`GRAPH_K_NEIGHBORS`, with `0` selecting adaptive k). The condition is strict (`<` not `<=`) to allow SNN to run on graphs where mean degree exactly equals k.

After all SNN scores are computed, but before any update or delete, evaluate the proposed plan against both:

- a minimum edge-retention ratio (`GRAPH_SNN_MIN_RETENTION_RATIO`, default `0.05`); and
- a minimum retained mean degree across previously linked nodes (`GRAPH_SNN_MIN_RETAINED_MEAN_DEGREE`, default `1.0`).

Dry-run and commit calls use the same decision function. A violating plan returns `status: safety_aborted` with retained/pruned counts, ratio, node count, retained mean degree, `k`, threshold, score distribution, reason codes, and remediation. No link metadata or row is changed, the graph-maintenance job fails at the SNN step, and PFNET/snapshot steps do not run.

An intentionally destructive run requires the explicit request field `allow_aggressive_pruning: true` or the job-time environment setting `GRAPH_SNN_ALLOW_AGGRESSIVE_PRUNING=true`. The result records that the override was applied.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Run SNN regardless and accept mass pruning | Destroys all edges on sparse graphs; unrecoverable without full re-link |
| Lower SNN threshold on sparse graphs | Arbitrary; doesn't address the root cause (insufficient neighbors for meaningful intersection) |
| Skip SNN entirely below a fixed node count | Node count is not the right signal; edge density matters |
| Warn and continue with partial scores | Partial SNN scores have no meaningful interpretation when most intersections are empty |
| Check only the proposed retention ratio | A small absolute remnant can pass a ratio check on small/disconnected graphs while leaving unusable topology |
| Automatically lower the threshold | Hides a data-dependent policy change and makes dry-run evidence differ from commit behavior |

## Consequences

### Positive
- (+) Prevents complete graph destruction on small or recently initialized archives
- (+) Self-correcting: as the corpus grows past the k-threshold, SNN automatically activates
- (+) Diagnostic logging gives operators clear signal when and why SNN was skipped
- (+) No configuration required; guard uses the same k as the SNN algorithm
- (+) Prevents dense-before/sparse-after plans from committing under defaults
- (+) Preserves exact pre-run rows and metadata on safety abort
- (+) Provides the same actionable plan evidence for API dry-runs and jobs

### Negative
- (-) Sparse graphs receive no SNN quality improvement; graph density matters for quality
- (-) Operator must check logs to understand why graph topology is not being refined
- (-) Guard condition (mean_degree < k) means SNN is skipped on graphs with many isolated nodes
- (-) Legitimately aggressive pruning requires an explicit, auditable override
- (-) Conservative defaults may reject unusual but intentional disconnected topologies

## Implementation

**Code Location:**
- Guards: `crates/matric-db/src/links.rs` (`PgLinkRepository::recompute_snn_scores_tx`)
- Configuration: `crates/matric-core/src/defaults.rs` (`GraphConfig` SNN fields)
- API: `POST /api/v1/graph/snn/recompute` returns HTTP 409 plus `SnnResult` when safety-aborted
- Job pipeline: `GraphMaintenanceHandler` stops before PFNET/snapshot on safety abort

**Guard Logic:**

```rust
let mean_degree = 2.0 * edge_count as f64 / node_count;

if mean_degree < k as f64 {
    warn!(
        mean_degree = mean_degree,
        k = k,
        "SNN skipped: graph too sparse (mean degree {:.1} < k={})",
        mean_degree, k
    );
    return Ok(SnnResult { skipped: true, mean_degree, k, ..Default::default() });
}

let plan = compute_all_snn_scores();
let reasons = policy.rejection_reasons(
    plan.retention_ratio,
    plan.retained_mean_degree,
    plan.total_edges,
    plan.retained,
);
if !reasons.is_empty() && !policy.allow_aggressive_pruning {
    return Ok(SnnResult::safety_aborted(plan, reasons));
}

apply_updates_and_deletes(plan);
```

## References

- ADR-073: Graph Quality Pipeline Architecture
- ADR-074: Louvain Community Detection
- ADR-075: PFNET Sparsification Strategy
- Issue #474: SNN Scoring Implementation
- Issue #481: Graph Quality Overhaul Epic
- Issue #1102: Abort SNN maintenance before catastrophic edge retention
