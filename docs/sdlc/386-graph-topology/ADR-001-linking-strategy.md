# ADR-001: Replace Threshold-Based Linking with HNSW Algorithm 4

## Status

Accepted (superseded: originally proposed mutual k-NN; implemented HNSW Algorithm 4 directly)

## Date

2026-02-14

## Issue

[#386 - Graph Topology Improvement](https://github.com/fortemi/fortemi/issues/386)

## Context

Fortemi's auto-linking pipeline (`LinkingHandler::execute` in `crates/matric-api/src/handlers/jobs.rs:710-863`) creates bidirectional semantic links between notes based on embedding cosine similarity. The current implementation retrieves up to 10 similar notes via `find_similar()` and links all candidates whose score exceeds a content-type-aware threshold (>=0.7 for prose, >=0.85 for code, defined in `crates/matric-core/src/defaults.rs:284-320`).

This threshold-based approach produces **star topologies**: notes cluster around central hub documents with all-to-one connections rather than distributed mesh structures. The root cause is architectural, not a threshold tuning problem. In 768-dimensional embedding space (nomic-embed-text), notes on the same topic form tight clusters where all members exceed the threshold relative to a central exemplar. The Johnson-Lindenstrauss lemma explains this: distances concentrate around the mean in high-dimensional spaces, so if mean pairwise similarity within a topic cluster is ~0.75 with standard deviation ~0.05, roughly 84% of pairs exceed a 0.70 threshold, creating near-complete subgraphs.

**Observed consequences:**
- Graph traversal beyond depth=1 is rarely useful (most paths are 1-2 hops through a hub)
- Hub notes accumulate unbounded link counts (0 or 20+ links, bimodal distribution)
- No meaningful bridge connections between topic clusters
- Clustering coefficient approaches 0.0 (no triangles in the graph)

**Desired state:** Mesh-of-stars topology with bounded node degree (5-10 links), high clustering coefficient (0.3-0.6), and meaningful multi-hop traversal paths.

```
Star Topology (Current):           Mesh-of-Stars (Desired):
    N1                                 N1---N2
     \                                  \   |
  N2--HUB--N3                            HUB--N3
     /|\                               /|\ /
    / | \                              / | X
   N4 N5 N6                          N4-N5-N6
```

A research evaluation of 7 alternative graph construction techniques was conducted (see `docs/research/knowledge-graph-topology-techniques.md`). This ADR originally selected mutual k-NN, but during construction the decision was revised to implement **HNSW Algorithm 4** (Malkov & Yashunin 2018) directly, skipping the intermediate mutual k-NN step.

## Decision

Replace threshold-based linking with **HNSW Algorithm 4 (SELECT-NEIGHBORS-HEURISTIC)** as the default graph construction strategy. Retain threshold-based linking as a configurable fallback via `GRAPH_LINKING_STRATEGY=threshold`.

**Revision note**: The original ADR proposed mutual k-NN (Option C, score 4.10) as a stepping stone toward HNSW Algorithm 4 (Option D). After review, the team decided to implement Algorithm 4 directly since the implementation complexity difference was marginal and Algorithm 4 produces strictly better topology.

### Tree-of-Thoughts Evaluation

Three primary candidates were evaluated against weighted criteria derived from Fortemi's non-functional requirements.

**Criteria and weights:**

| Criterion | Weight | Rationale |
|-----------|--------|-----------|
| Topology quality | 30% | Primary goal: mesh-of-stars with bounded degree |
| Implementation simplicity | 25% | Small team, single-crate change preferred |
| Performance overhead | 20% | Linking runs as background job; latency budget is 50-500ms per note |
| Backward compatibility | 15% | Existing deployments must not break |
| Extensibility | 10% | Path to Phase 2 (HNSW Algorithm 4, community detection) |

**Scoring matrix (1-5):**

| Criterion (Weight) | Option A: Keep Threshold | Option B: k-NN (asymmetric) | Option C: Mutual k-NN |
|---------------------|--------------------------|-----------------------------|-----------------------|
| Topology quality (30%) | 1 - Star, unbounded degree | 4 - Mesh, bounded, may include low-quality links | 5 - Mesh, bounded, quality-filtered |
| Implementation simplicity (25%) | 5 - No change | 4 - Remove threshold filter, use k directly | 4 - Add reverse lookup per candidate |
| Performance overhead (20%) | 5 - 1 query per note | 4 - 1 query per note (same cost) | 3 - k+1 queries per note (reverse lookups) |
| Backward compatibility (15%) | 5 - No change | 3 - Different link set, no fallback | 4 - Configurable, env var toggle |
| Extensibility (10%) | 2 - Dead end | 3 - Foundation for RNG/community | 4 - Natural path to HNSW Algo 4 |
| **Weighted score** | **3.25** | **3.75** | **4.10** |

**Option C (Mutual k-NN) selected** with weighted composite 4.10, outperforming threshold (3.25) and asymmetric k-NN (3.75).

### Deferred Alternatives

The full research evaluated 7 techniques. Options not selected for Phase 1 are documented here for future reference.

| Option | Pros | Cons | Verdict |
|--------|------|------|---------|
| 1. Keep threshold | Simple, zero effort | Star topology, unbounded degree, poor bridges | Rejected (retain as fallback only) |
| 2. Asymmetric k-NN | Bounded degree, mesh | May include low-quality links, A links B does not imply B links A | Considered, superseded by mutual k-NN |
| 3. **Mutual k-NN** | Quality + mesh, bounded degree, uses existing HNSW index | k+1 queries per note instead of 1 | **Selected** |
| 4. HNSW Algorithm 4 (diverse neighbor selection) | Best quality, diverse connections, approximates RNG | More complex to implement, requires custom pgvector query | Deferred to Phase 2 |
| 5. RNG pruning | Optimal sparsity, excellent bridge preservation | O(N^2) per note, expensive for large neighborhoods | Deferred to Phase 2 (post-processing step) |
| 6. Community detection (Louvain/Label Propagation) | Hierarchical clustering, explicit inter-cluster bridges | External dependency (petgraph), batch job complexity, overkill for <10k notes | Deferred (enterprise scale) |
| 7. Hierarchical linking (multi-scale thresholds) | Multi-scale navigation, small-world properties | Schema changes (link level column), complex traversal logic | Deferred (future enhancement) |

### Backtracking Triggers

Revisit this decision if any of the following occur:

- **Isolated node rate exceeds 15%** after mutual k-NN deployment (too many notes with zero mutual neighbors)
- **Clustering coefficient remains below 0.2** after re-linking a test corpus of 200+ notes
- **Linking latency exceeds 2 seconds** per note on corpora with >5,000 notes
- **User feedback** indicates the new link set is less useful than the previous star topology

If backtracking is triggered, evaluate HNSW Algorithm 4 (Option 4) as the next candidate per the deferred alternatives above.

## Rationale

Mutual k-NN provides the strongest balance of quality, performance, and implementation simplicity for Fortemi's current scale.

**Why mutual k-NN over threshold:**
- Threshold-based linking is fundamentally unable to produce mesh topology in high-dimensional clustered embedding spaces (the hub domination problem is mathematical, not parametric)
- k-NN bounds degree regardless of absolute similarity, preventing hub accumulation
- Mutual filtering ensures both parties "agree" the link is meaningful, producing higher-quality edges than either threshold or asymmetric k-NN

**Why mutual k-NN over HNSW Algorithm 4:**
- HNSW Algorithm 4 (diverse neighbor selection heuristic from Malkov & Yashunin 2018, REF-031) produces superior topology by approximating the Relative Neighborhood Graph, but requires implementing a custom neighbor selection loop that pgvector does not expose directly
- Mutual k-NN achieves 80% of the benefit with 20% of the implementation effort
- Mutual k-NN is a natural stepping stone: the data structures and configuration needed for Phase 2 (HNSW Algorithm 4) are a superset of Phase 1

**Why not community detection:**
- Community detection (Louvain, Label Propagation) adds a `petgraph` dependency and requires a periodic batch job
- Research consensus: overkill for corpora under 10,000 notes
- Mutual k-NN alone should produce sufficient inter-cluster bridges for typical Fortemi deployments

**pgvector HNSW index reuse:**
The current `find_similar()` implementation in `crates/matric-db/src/embeddings.rs:68-134` already uses pgvector's HNSW index to compute k nearest neighbors, then filters by threshold. Switching to k-NN means removing the threshold filter and using HNSW results directly -- zero additional index cost.

## Implementation Plan

### Phase 1: Mutual k-NN (This Issue)

**Scope:** Changes confined to 3 files, no schema migrations, no new dependencies.

#### 1. New `GraphConfig` in `crates/matric-core/src/defaults.rs`

```rust
/// Graph linking strategy configuration.
pub struct GraphConfig {
    /// Linking strategy: "mutual_knn" (default) or "threshold" (legacy)
    pub strategy: String,
    /// Number of nearest neighbors to consider (mutual k-NN)
    pub k_neighbors: usize,
    /// Minimum similarity floor (reject candidates below this regardless of k)
    pub min_similarity: f32,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            strategy: "mutual_knn".into(),
            k_neighbors: 7,
            min_similarity: 0.5,
        }
    }
}
```

Environment variable overrides:
- `GRAPH_LINKING_STRATEGY` -- `mutual_knn` (default) or `threshold`
- `GRAPH_K_NEIGHBORS` -- default `7`, adaptive formula `max(5, log2(N))` when set to `0`
- `GRAPH_MIN_SIMILARITY` -- default `0.5`, absolute floor below which no links are created

#### 2. Modified `LinkingHandler::execute` in `crates/matric-api/src/handlers/jobs.rs`

Replace the current threshold loop (lines 800-843) with:

```rust
// Mutual k-NN strategy
let k = graph_config.k_neighbors;
let candidates = self.db.embeddings
    .find_similar(&embeddings[0].vector, (k + 1) as i64, true)
    .await?;

let mut created_count = 0;
for hit in candidates.iter().filter(|h| h.note_id != note_id) {
    if hit.score < graph_config.min_similarity {
        continue;
    }

    // Reverse lookup: is note_id in hit's k-NN?
    let hit_embedding = self.db.embeddings.get_for_note(hit.note_id).await?;
    if hit_embedding.is_empty() {
        continue;
    }

    let reverse = self.db.embeddings
        .find_similar(&hit_embedding[0].vector, (k + 1) as i64, true)
        .await?;

    let is_mutual = reverse.iter().any(|r| r.note_id == note_id);

    if is_mutual {
        self.db.links.create_reciprocal(
            note_id, hit.note_id, "semantic", hit.score,
            Some(serde_json::json!({"strategy": "mutual_knn", "k": k})),
        ).await?;
        created_count += 1;
    }
}

// Fallback: if no mutual neighbors, link to single best match
if created_count == 0 {
    if let Some(best) = candidates.iter()
        .find(|h| h.note_id != note_id && h.score >= graph_config.min_similarity)
    {
        self.db.links.create_reciprocal(
            note_id, best.note_id, "semantic", best.score,
            Some(serde_json::json!({"strategy": "fallback_best", "k": k})),
        ).await?;
    }
}
```

The existing `create_reciprocal` method in `crates/matric-db/src/links.rs:58-112` already handles idempotent bidirectional link creation with `WHERE NOT EXISTS` guards.

#### 3. New topology metrics endpoint

`GET /api/v1/graph/topology/stats`

```json
{
  "total_notes": 450,
  "total_links": 1823,
  "avg_degree": 8.1,
  "degree_std_dev": 2.3,
  "max_degree": 14,
  "isolated_nodes": 3,
  "clustering_coefficient": 0.42,
  "strategy": "mutual_knn",
  "k_neighbors": 7
}
```

This endpoint queries existing `link` and `note` tables with aggregate SQL (no schema changes needed). Clustering coefficient can be computed via a CTE that counts triangles.

### Phase 2: HNSW Algorithm 4 + RNG Pruning (Future)

- Implement diverse neighbor selection heuristic from Malkov & Yashunin (2018) Algorithm 4
- Add RNG pruning as optional post-processing for notes with >15 links
- Requires new `find_similar_diverse()` method in embeddings repository
- Estimated effort: 16-24 hours

### Phase 3: Community Detection (Enterprise, Deferred)

- Louvain or Label Propagation via `petgraph` crate
- Periodic batch job (`GraphOptimization` job type) to detect communities and create bridge links
- Schema additions: `link.is_bridge`, `link.community_from`, `link.community_to`
- Only justified at >10,000 notes per memory archive

## Consequences

### Positive

- **Mesh topology**: Bounded degree (5-10 links per note) with distributed connections instead of star clusters
- **Meaningful graph traversal**: Multi-hop paths become useful; clustering coefficient target 0.3-0.6 (up from ~0.0)
- **Zero index cost**: Reuses existing pgvector HNSW index; `find_similar()` already computes k-NN
- **Backward compatible**: Configurable via `GRAPH_LINKING_STRATEGY=threshold` for deployments that prefer current behavior
- **No schema changes**: Existing `link` table structure is sufficient; metadata field stores strategy provenance
- **Isolated node safety**: Fallback to single best match prevents orphan notes

### Negative

- **Additional queries per note**: Mutual k-NN requires k+1 embedding lookups (one reverse query per candidate) instead of 1. For k=7 and a typical linking job, this adds ~6 HNSW queries (~5-10ms each), increasing per-note linking time from ~50ms to ~100ms. This runs in a background job worker, so user-facing latency is unaffected.
- **Mixed topology during migration**: Existing graphs will retain star topology for previously linked notes until they are re-linked. A bulk re-linking job (delete semantic links, re-enqueue all notes) is needed for full migration.
- **k selection sensitivity**: The default k=7 is based on Miller's Law (7 +/- 2) and research recommendations, but optimal k varies with corpus size. The adaptive formula `max(5, log2(N))` mitigates this, but requires `GRAPH_K_NEIGHBORS=0` to activate.
- **Fewer total links**: Mutual k-NN is more selective than threshold linking. Some notes that currently have 15+ links may have only 3-5 after migration. This is by design (quality over quantity) but may surprise users accustomed to dense link sets.

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `GRAPH_LINKING_STRATEGY` | `mutual_knn` | Linking strategy: `mutual_knn` or `threshold` |
| `GRAPH_K_NEIGHBORS` | `7` | Number of nearest neighbors. Set `0` for adaptive `max(5, log2(N))` |
| `GRAPH_MIN_SIMILARITY` | `0.5` | Absolute similarity floor (no links below this regardless of k) |

Legacy threshold constants (`SEMANTIC_LINK_THRESHOLD`, `SEMANTIC_LINK_THRESHOLD_CODE`) remain in `defaults.rs` and are used when `GRAPH_LINKING_STRATEGY=threshold`.

## Metrics for Validation

| Metric | Current (Star) | Target (Mesh) | Measurement |
|--------|---------------|---------------|-------------|
| Clustering coefficient | ~0.0 | 0.3-0.6 | `(triangles) / (connected triples)` |
| Average degree | Bimodal (0 or 20+) | 5-10 | `(total links) / (total notes)` |
| Degree std deviation | High | Low (uniform) | `std_dev(node_degrees)` |
| Average path length | ~2.0 | 3-4 | `avg(shortest_path(u,v))` |
| Depth>1 link traversal rate | <10% | >30% | User click-through analytics |

## References

- **REF-031**: Malkov, Y. A., & Yashunin, D. A. (2018). "Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs." *IEEE Transactions on Pattern Analysis and Machine Intelligence*, 42(4), 824-836. -- Algorithm 4 (diverse neighbor selection) demonstrates prevention of star topology on clustered data by approximating the Relative Neighborhood Graph.
- **REF-032**: Hogan, A., et al. (2021). "Knowledge Graphs." *ACM Computing Surveys*, 54(4), 1-37.
- Dong, W., Moses, C., & Li, K. (2011). "Efficient k-nearest neighbor graph construction for generic similarity measures." *WWW '11*. -- k-NN graphs preserve local manifold structure better than threshold graphs.
- Kleinberg, J. M. (2000). "Navigation in a small world." *Nature*, 406(6798), 845. -- Small-world navigation requires bounded degree.
- Watts, D. J., & Strogatz, S. H. (1998). "Collective dynamics of 'small-world' networks." *Nature*, 393(6684), 440-442.
- Research: `docs/research/knowledge-graph-topology-techniques.md`
- Executive summary: `docs/research/graph-topology-executive-summary.md`
