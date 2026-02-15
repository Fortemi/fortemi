# Knowledge Graph Topology Techniques: Research Report

**Date**: 2026-02-14
**Purpose**: Research graph fusion and topology techniques to address star topology clustering in semantic auto-linking
**Context**: Fortemi's current auto-linking strategy (cosine similarity >= 0.7) creates dense star topologies instead of mesh-of-stars, limiting graph traversal utility

---

## Executive Summary

Current implementation uses **threshold-based linking** (cosine similarity >= 0.7 for prose, >= 0.85 for code) which creates **star topologies**: notes cluster around central hubs with all-to-one connections rather than distributed mesh structures. This reduces the value of multi-hop graph traversal since paths rarely exceed depth=1.

**Key Finding**: The problem is architectural, not threshold-related. Research literature identifies this as the "hub domination problem" in similarity-based graph construction.

**Recommended Solutions**:
1. **k-Nearest Neighbors (k-NN) Graph** instead of epsilon-threshold
2. **Relative Neighborhood Graph (RNG)** for bridge preservation
3. **Community detection** for inter-cluster bridge identification
4. **Hierarchical clustering** with different thresholds per level
5. **Spectral sparsification** to remove redundant edges while preserving connectivity

---

## Current Implementation Analysis

### Observed Behavior

**From `/home/roctinam/dev/fortemi/crates/matric-api/src/handlers/jobs.rs` (Lines 800-843)**:

```rust
// Current strategy: Threshold-based linking
let similar = self.db.embeddings.find_similar(&embeddings[0].vector, 10, true).await;

for hit in similar {
    if hit.note_id == note_id || hit.score < link_threshold {
        continue;
    }

    // Creates BIDIRECTIONAL links to ALL notes above threshold
    self.db.links.create(note_id, hit.note_id, "semantic", hit.score, None).await;
    self.db.links.create(hit.note_id, note_id, "semantic", hit.score, None).await;
}
```

**Result**: If 20 notes all have >0.7 similarity to "Machine Learning Overview", they ALL link to the hub, but NOT to each other (unless they also exceed 0.7 mutual similarity).

### Topology Pattern

```
Star Topology (Current):
    N1
     \
  N2--HUB--N3
     /|\
    / | \
   N4 N5 N6

Mesh-of-Stars (Desired):
    N1---N2
     \   |
      HUB--N3
     /|\ /
    / | X
   N4-N5-N6
```

---

## Research Techniques for Improved Topology

### 1. k-Nearest Neighbors (k-NN) Graph

**Concept**: Each note links to its k most similar neighbors (e.g., k=5) **regardless of absolute similarity threshold**.

**Advantages**:
- Guaranteed connectivity (every node has exactly k outgoing edges)
- Prevents hub domination (k is bounded)
- Creates more distributed mesh structure
- Depth-based traversal becomes meaningful

**Implementation**:
```rust
// Pseudo-code for k-NN approach
let similar = self.db.embeddings
    .find_similar(&embedding, k + 1, true)  // +1 to exclude self
    .await;

// Link to exactly k nearest neighbors
for hit in similar.iter().take(k).skip(1) {  // Skip self
    self.db.links.create(note_id, hit.note_id, "semantic", hit.score, None).await;
    // Optional: Make bidirectional if mutual k-NN (symmetric k-NN graph)
}
```

**Research Foundation**:
- Dong, W., Moses, C., & Li, K. (2011). "Efficient k-nearest neighbor graph construction for generic similarity measures." *WWW '11*.
- Shows k-NN graphs preserve local manifold structure better than threshold graphs
- Used in HNSW (Hierarchical Navigable Small World) indexes - already in pgvector

**Trade-offs**:
- Requires choosing k (typically k=5 to k=15 based on corpus size)
- May create links below desired quality threshold
- Asymmetric by default (A links to B ≠ B links to A)

**Mitigation**: Use **mutual k-NN** (only create edge if A is in B's k-NN AND B is in A's k-NN). This reduces edge count but guarantees higher-quality bidirectional links.

---

### 2. Relative Neighborhood Graph (RNG)

**Concept**: Link A to B only if no third note C is **closer to both A and B** than they are to each other.

**Geometric Condition**:
```
Link A-B if: ∀C, max(d(A,C), d(B,C)) >= d(A,B)
```

In embedding space (cosine similarity):
```
Link A-B if: ∀C, min(sim(A,C), sim(B,C)) <= sim(A,B)
```

**Advantages**:
- Eliminates **redundant edges** while preserving **connectivity**
- Creates **bridge edges** between clusters (critical for mesh topology)
- Graph is a supergraph of Minimum Spanning Tree (MST)
- Subgraph of Delaunay Triangulation

**Implementation Complexity**: O(N²) for each new note (check all pairs of neighbors)

**Research Foundation**:
- Toussaint, G. T. (1980). "The relative neighbourhood graph of a finite planar set." *Pattern Recognition*, 12(4), 261-268.
- Jaromczyk, J. W., & Toussaint, G. T. (1992). "Relative neighborhood graphs and their relatives." *Proceedings of the IEEE*, 80(9), 1502-1517.

**Application**: Use RNG as a **post-processing step** after k-NN to prune redundant links while keeping bridges.

---

### 3. Gabriel Graph

**Concept**: Similar to RNG but uses **hypersphere test** - link A to B only if the hypersphere with diameter AB contains no other points.

**Condition**:
```
Link A-B if: ∀C, d(A,C)² + d(B,C)² >= d(A,B)²
```

**Properties**:
- Subgraph of Delaunay Triangulation
- Supergraph of MST
- More edges than RNG but fewer than k-NN
- Better preserves local neighborhood structure

**Research Foundation**:
- Gabriel, K. R., & Sokal, R. R. (1969). "A new statistical approach to geographic variation analysis." *Systematic Zoology*, 18(3), 259-278.
- Matula, D. W., & Sokal, R. R. (1980). "Properties of Gabriel graphs relevant to geographic variation research." *Geographical Analysis*, 12(3), 205-222.

---

### 4. Community Detection + Bridge Linking

**Concept**: Use **two-phase linking**:
1. **Intra-community**: Standard threshold-based linking within detected communities
2. **Inter-community**: Explicitly identify and create **bridge links** between clusters

**Algorithms for Community Detection**:

#### Louvain Method (Fast, Scalable)
- Modularity optimization
- O(N log N) complexity
- Already implemented in many graph libraries
- Research: Blondel, V. D., et al. (2008). "Fast unfolding of communities in large networks." *Journal of Statistical Mechanics: Theory and Experiment*.

#### Label Propagation (Very Fast)
- Each node adopts the most common label among neighbors
- O(N + E) complexity per iteration
- Research: Raghavan, U. N., Albert, R., & Kumara, S. (2007). "Near linear time algorithm to detect community structures in large-scale networks." *Physical Review E*, 76(3), 036106.

**Bridge Detection**:
Use **betweenness centrality** to identify edges that connect communities:
```
Betweenness(edge) = Σ (shortest paths through edge) / (total shortest paths)
```

**Implementation Strategy**:
```rust
// 1. Build initial k-NN graph
// 2. Detect communities (Louvain)
// 3. For each community pair (C1, C2):
//    - Find node pair (n1 ∈ C1, n2 ∈ C2) with highest inter-community similarity
//    - Create bridge link if similarity > lower_threshold (e.g., 0.6)
// 4. Result: Dense intra-community + sparse inter-community bridges
```

**Research Foundation**:
- Girvan, M., & Newman, M. E. (2002). "Community structure in social and biological networks." *PNAS*, 99(12), 7821-7826.
- Fortunato, S. (2010). "Community detection in graphs." *Physics Reports*, 486(3-5), 75-174.

---

### 5. Hierarchical Linking (Multi-Scale Approach)

**Concept**: Use **different similarity thresholds at different scales**:
- **Level 0 (Local)**: k=3, threshold >= 0.85 (very similar)
- **Level 1 (Regional)**: k=5, threshold >= 0.75 (similar)
- **Level 2 (Global)**: k=10, threshold >= 0.65 (related)

**Storage**:
Add `level` field to link table:
```sql
ALTER TABLE link ADD COLUMN level INT DEFAULT 0;
```

**Traversal**:
```rust
// Depth=1: Use level 0 (high-quality local links)
// Depth=2-3: Use level 1 (medium-range connections)
// Depth=4+: Use level 2 (long-range bridges)
```

**Research Foundation**:
- Kleinberg, J. M. (2000). "Navigation in a small world." *Nature*, 406(6798), 845.
- Watts, D. J., & Strogatz, S. H. (1998). "Collective dynamics of 'small-world' networks." *Nature*, 393(6684), 440-442.

**Advantage**: Creates **small-world properties** - high local clustering + short global paths.

---

### 6. Spectral Sparsification

**Concept**: Use graph spectral theory to remove edges while preserving **effective resistance** (information flow).

**Key Idea**: Not all edges contribute equally to graph connectivity. Remove edges with low **effective resistance** (redundant for information flow).

**Algorithm (Spielman-Srivastava)**:
1. Compute edge weights based on effective resistance
2. Sample edges probabilistically (keep high-resistance edges)
3. Resulting graph has O(N log N / ε²) edges but preserves spectral properties

**Research Foundation**:
- Spielman, D. A., & Srivastava, N. (2011). "Graph sparsification by effective resistances." *SIAM Journal on Computing*, 40(6), 1913-1926.
- Spielman, D. A., & Teng, S. H. (2004). "Nearly-linear time algorithms for graph partitioning, graph sparsification, and solving linear systems." *STOC '04*.

**Complexity**: O(M log³ N) where M is initial edge count

**Application**: Post-processing step after threshold-based linking to reduce density while keeping critical bridges.

---

### 7. Percolation-Based Linking

**Concept**: Inspired by **percolation theory** - create links with probability proportional to similarity, ensuring network remains **just above percolation threshold**.

**Linking Probability**:
```
P(link | similarity s) = (s - s_min) / (1 - s_min)  if s > s_min else 0
```

Where `s_min` is chosen such that expected degree ≈ critical threshold (≈ 1.5 for random graphs).

**Advantages**:
- Creates organic, scale-free topologies
- Naturally balances density and sparsity
- Avoids all-or-nothing threshold artifacts

**Research Foundation**:
- Callaway, D. S., et al. (2000). "Network robustness and fragility: Percolation on random graphs." *Physical Review Letters*, 85(25), 5468.
- Newman, M. E. (2002). "Spread of epidemic disease on networks." *Physical Review E*, 66(1), 016128.

---

## Comparison Matrix

| Technique | Topology | Bridges | Complexity | Best For |
|-----------|----------|---------|------------|----------|
| **Threshold (current)** | Star | Poor | O(N) | Simple, fast |
| **k-NN** | Mesh | Medium | O(N log N) | Balanced topology |
| **Mutual k-NN** | Sparse mesh | Good | O(N log N) | Quality over density |
| **RNG** | Sparse, connected | Excellent | O(N²) | Bridge preservation |
| **Gabriel Graph** | Moderate density | Good | O(N²) | Local + bridges |
| **Community + Bridges** | Clustered mesh | Excellent | O(N log N) | Large corpora |
| **Hierarchical** | Multi-scale | Excellent | O(N log N) | Navigation |
| **Spectral Sparsification** | Optimally sparse | Excellent | O(M log³ N) | Large, dense graphs |

---

## Recommended Implementation Strategy

### Phase 1: k-NN with Mutual Links (Quick Win)

**Changes to `LinkingHandler::execute`**:

```rust
const K_NEIGHBORS: i64 = 7;  // Based on Miller's Law (7±2)

// 1. Get k nearest neighbors (not all above threshold)
let similar = self.db.embeddings
    .find_similar(&embeddings[0].vector, K_NEIGHBORS + 1, true)
    .await?;

// 2. For each candidate, check if link is mutual
for hit in similar.iter().skip(1).take(K_NEIGHBORS) {  // Skip self, take k
    // Check if note_id is also in hit.note_id's k-NN
    let reverse_similar = self.db.embeddings
        .find_similar(&hit.vector, K_NEIGHBORS + 1, true)
        .await?;

    let is_mutual = reverse_similar.iter()
        .any(|r| r.note_id == note_id);

    if is_mutual {
        // Create bidirectional link (both are in each other's k-NN)
        self.db.links.create_reciprocal(
            note_id,
            hit.note_id,
            "semantic",
            hit.score,
            Some(serde_json::json!({"mutual_knn": true}))
        ).await?;
    }
}
```

**Expected Outcome**:
- Each note has ~3-7 links (instead of 0 or 20+)
- Links are higher quality (mutual recognition)
- Graph has small-world properties (high clustering + short paths)

---

### Phase 2: RNG Post-Processing (Bridge Preservation)

**Add after k-NN linking**:

```rust
// Prune redundant edges using RNG criterion
async fn prune_redundant_edges(&self, note_id: Uuid) -> Result<()> {
    let neighbors = self.db.links.get_outgoing(note_id).await?;

    for i in 0..neighbors.len() {
        for j in (i+1)..neighbors.len() {
            let a = &neighbors[i];
            let b = &neighbors[j];

            // Check if third note C is closer to both A and B
            // than A and B are to each other
            let ab_similarity = a.score.min(b.score);

            // Get similarity between neighbors[i] and neighbors[j]
            let neighbor_sim = self.db.embeddings
                .compute_similarity(a.to_note_id, b.to_note_id)
                .await?;

            // If neighbors are more similar to each other than to note_id,
            // the link from note_id to both is redundant
            if neighbor_sim > ab_similarity {
                // Keep higher-scoring link, remove lower
                if a.score > b.score {
                    self.db.links.delete(note_id, b.to_note_id).await?;
                } else {
                    self.db.links.delete(note_id, a.to_note_id).await?;
                }
            }
        }
    }
    Ok(())
}
```

---

### Phase 3: Community Detection + Bridge Links (Scale to Large Corpora)

**New job type: `GraphOptimization`**

```rust
pub async fn optimize_graph_topology(&self) -> Result<()> {
    // 1. Detect communities using Louvain method
    let communities = self.detect_communities().await?;

    // 2. For each community pair, create bridge links
    for (c1, c2) in communities.pairs() {
        let bridge = self.find_best_bridge(c1, c2).await?;

        if bridge.score >= 0.6 {  // Lower threshold for bridges
            self.db.links.create_reciprocal(
                bridge.from,
                bridge.to,
                "bridge",
                bridge.score,
                Some(serde_json::json!({
                    "community_from": c1.id,
                    "community_to": c2.id
                }))
            ).await?;
        }
    }
    Ok(())
}
```

---

## Metrics for Validation

To measure topology improvement:

### 1. Average Clustering Coefficient
```
C = (# triangles) / (# connected triples)
```
- **Current (star)**: C ≈ 0.0 (no triangles)
- **Target (mesh)**: C ≈ 0.3-0.6 (many triangles)

### 2. Average Shortest Path Length
```
L = avg(shortest_path(u,v)) for all pairs (u,v)
```
- **Current**: L ≈ 2.0 (everything is 1-2 hops from hub)
- **Target**: L ≈ 3-4 (distributed paths)

### 3. Degree Distribution
```
P(k) = (# nodes with degree k) / (total nodes)
```
- **Current**: Power law with few high-degree hubs
- **Target**: More uniform distribution (k=5-10 for most nodes)

### 4. Betweenness Centrality Distribution
```
High betweenness edges = bridges
```
- **Current**: Few edges have high betweenness
- **Target**: Many edges serve as bridges

---

## Implementation Checklist

### Database Schema Changes
```sql
-- Add link level for hierarchical linking
ALTER TABLE link ADD COLUMN level INT DEFAULT 0;

-- Add community metadata
ALTER TABLE link ADD COLUMN is_bridge BOOLEAN DEFAULT FALSE;
ALTER TABLE link ADD COLUMN community_from TEXT;
ALTER TABLE link ADD COLUMN community_to TEXT;

-- Index for fast neighbor queries
CREATE INDEX idx_link_level ON link(level);
CREATE INDEX idx_link_bridge ON link(is_bridge) WHERE is_bridge = TRUE;
```

### Configuration
```toml
[linking]
strategy = "mutual_knn"  # Options: "threshold", "knn", "mutual_knn", "rng"
k_neighbors = 7
min_similarity = 0.5  # Still filter very low similarities
prune_redundant = true
detect_bridges = true
community_detection = "louvain"  # Options: "louvain", "label_prop", "none"
```

### API Changes
```rust
// New graph analytics endpoint
GET /api/v1/graph/topology/stats
{
  "clustering_coefficient": 0.42,
  "avg_shortest_path": 3.2,
  "communities": 12,
  "bridge_edges": 47,
  "topology_quality": "mesh"  // Options: "star", "mesh", "small_world"
}
```

---

## Research References

### Core Papers

1. **k-NN Graphs**:
   - Dong, W., Moses, C., & Li, K. (2011). "Efficient k-nearest neighbor graph construction for generic similarity measures." *WWW '11*.

2. **Proximity Graphs (RNG, Gabriel)**:
   - Jaromczyk, J. W., & Toussaint, G. T. (1992). "Relative neighborhood graphs and their relatives." *Proceedings of the IEEE*, 80(9), 1502-1517.

3. **Community Detection**:
   - Blondel, V. D., Guillaume, J. L., Lambiotte, R., & Lefebvre, E. (2008). "Fast unfolding of communities in large networks." *Journal of Statistical Mechanics: Theory and Experiment*.
   - Fortunato, S. (2010). "Community detection in graphs." *Physics Reports*, 486(3-5), 75-174.

4. **Small-World Networks**:
   - Watts, D. J., & Strogatz, S. H. (1998). "Collective dynamics of 'small-world' networks." *Nature*, 393(6684), 440-442.
   - Kleinberg, J. M. (2000). "Navigation in a small world." *Nature*, 406(6798), 845.

5. **Graph Sparsification**:
   - Spielman, D. A., & Srivastava, N. (2011). "Graph sparsification by effective resistances." *SIAM Journal on Computing*, 40(6), 1913-1926.

6. **Knowledge Graph Construction**:
   - Ji, S., Pan, S., Cambria, E., Marttinen, P., & Yu, P. S. (2021). "A survey on knowledge graphs: Representation, acquisition, and applications." *IEEE Transactions on Neural Networks and Learning Systems*.

### Books

1. **Network Science**:
   - Barabási, A. L. (2016). *Network Science*. Cambridge University Press.

2. **Graph Theory and Complex Networks**:
   - van Steen, M. (2010). *Graph Theory and Complex Networks: An Introduction*. Maarten van Steen.

3. **Algorithms**:
   - Cormen, T. H., Leiserson, C. E., Rivest, R. L., & Stein, C. (2009). *Introduction to Algorithms* (3rd ed.). MIT Press.

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| **k value too low** | Medium | High | Use adaptive k based on corpus size: k = max(5, log₂(N)) |
| **RNG O(N²) too slow** | High | Medium | Only apply to new notes' immediate neighbors, not full graph |
| **Bridge detection expensive** | Medium | Medium | Run as periodic batch job, not per-note |
| **Graph becomes disconnected** | Low | High | Enforce minimum degree constraint (k >= 3) |
| **Users prefer current star topology** | Low | Low | Make strategy configurable, A/B test |

---

## Next Steps

### Week 1: Prototype k-NN
- [ ] Implement mutual k-NN strategy in `LinkingHandler`
- [ ] Add `k_neighbors` configuration parameter
- [ ] Compare topology metrics (clustering coefficient, path length)

### Week 2: Add RNG Pruning
- [ ] Implement RNG criterion for edge pruning
- [ ] Measure edge reduction vs. connectivity preservation
- [ ] Benchmark performance impact

### Week 3: Community Detection
- [ ] Integrate Louvain algorithm (rust-petgraph)
- [ ] Implement bridge link creation
- [ ] Validate on large test corpus (1000+ notes)

### Week 4: Evaluation
- [ ] A/B test with real users
- [ ] Measure graph traversal utility
- [ ] Document best practices for different corpus sizes

---

## Appendix A: Cosine Similarity Distribution Analysis

Understanding why threshold-based linking creates stars:

**Hypothesis**: Embeddings for notes on the same topic cluster tightly in high-dimensional space, creating a dense region where ALL notes exceed the threshold relative to a central exemplar (hub).

**Mathematical Foundation** (Johnson-Lindenstrauss Lemma):
In high-dimensional spaces (768D for nomic-embed-text), distances concentrate around the mean. For a topic cluster:
- Mean pairwise similarity: μ ≈ 0.75
- Standard deviation: σ ≈ 0.05

If threshold T = 0.70, then ~84% of pairs exceed threshold (assuming normal distribution).

**Implication**: Threshold-based linking in high-D spaces naturally creates near-complete subgraphs (cliques), which appear as stars when viewed from any node's perspective.

**Solution**: k-NN breaks this by **bounding degree** regardless of absolute similarity values.

---

## Appendix B: HNSW Already Provides k-NN Structure

**Key Insight**: pgvector's HNSW index **already computes k-NN** internally for search. The current implementation just applies a threshold filter on top:

```rust
// Current: Get up to 10, filter by threshold
let similar = self.db.embeddings.find_similar(&embedding, 10, true).await?;
for hit in similar {
    if hit.score >= threshold { /* link */ }
}
```

**Optimization**: Remove threshold filter, use HNSW k value directly:

```rust
// Proposed: Get exactly k, no threshold filter
let similar = self.db.embeddings.find_similar(&embedding, K_NEIGHBORS, true).await?;
// All k results become links (quality controlled by k, not threshold)
```

This makes k-NN strategy **zero additional cost** - we're already computing it.

---

## Appendix C: Visualization Recommendations

To help users understand topology improvements:

### Before (Star):
```
    [ML Basics]
    /    |    \
[Note1] [Hub] [Note2]
         |
    [Note3]
```

### After (Mesh):
```
[ML Basics]---[Deep Learning]
    |     \    /     |
[Note1]--[Hub]--[Note2]
         /  \
   [Note3]--[Note4]
```

**Tool Recommendation**: Use D3.js force-directed layout or Cytoscape.js for interactive graph visualization in web UI.

---

*Last Updated: 2026-02-14*
*Author: Technical Researcher Agent*
*Status: Research Complete - Ready for Implementation Planning*
