# Graph Topology Implementation Guide

**Companion to**: `knowledge-graph-topology-techniques.md`
**Purpose**: Practical implementation patterns and code examples

---

## Quick Decision Matrix

| Corpus Size | Recommended Strategy | Rationale |
|-------------|---------------------|-----------|
| < 100 notes | Mutual k-NN (k=5) | Simple, fast, good quality |
| 100-1,000 notes | Mutual k-NN (k=7) + RNG pruning | Balanced topology |
| 1,000-10,000 notes | Community detection + bridges | Scalable, maintains bridges |
| > 10,000 notes | Hierarchical + spectral sparsification | Multi-scale navigation |

---

## Implementation Pattern 1: Mutual k-NN

### Minimal Changes to `LinkingHandler`

**File**: `/home/roctinam/dev/fortemi/crates/matric-api/src/handlers/jobs.rs`

**Current (Lines 800-843)** - Threshold approach:
```rust
let similar = self.db.embeddings
    .find_similar(&embeddings[0].vector, 10, true)
    .await?;

for hit in similar {
    if hit.note_id == note_id || hit.score < link_threshold {
        continue;
    }
    // Create links to ALL above threshold
}
```

**Proposed (Mutual k-NN)**:
```rust
// Configuration constant
const K_NEIGHBORS: usize = 7;  // Miller's Law (7±2)

// 1. Get this note's k nearest neighbors
let candidates = self.db.embeddings
    .find_similar(&embeddings[0].vector, (K_NEIGHBORS + 1) as i64, true)
    .await?
    .into_iter()
    .skip(1)  // Skip self
    .take(K_NEIGHBORS)
    .collect::<Vec<_>>();

// 2. For each candidate, check mutual k-NN relationship
for candidate in candidates {
    // Get candidate's k nearest neighbors
    let candidate_embedding = match self.db.embeddings
        .get_for_note(candidate.note_id)
        .await?
        .into_iter()
        .next()
    {
        Some(e) => e.vector,
        None => continue,
    };

    let reverse_neighbors = self.db.embeddings
        .find_similar(&candidate_embedding, (K_NEIGHBORS + 1) as i64, true)
        .await?;

    // Check if note_id appears in candidate's k-NN
    let is_mutual = reverse_neighbors
        .iter()
        .any(|n| n.note_id == note_id);

    if is_mutual {
        // Create bidirectional link (both recognize each other)
        let metadata = serde_json::json!({
            "strategy": "mutual_knn",
            "k": K_NEIGHBORS,
            "rank_forward": candidates.iter().position(|c| c.note_id == candidate.note_id),
            "rank_reverse": reverse_neighbors.iter().position(|n| n.note_id == note_id)
        });

        if let Err(e) = self.db.links.create_reciprocal(
            note_id,
            candidate.note_id,
            "semantic",
            candidate.score,
            Some(metadata)
        ).await {
            debug!(error = %e, "Failed to create mutual k-NN link (may already exist)");
        } else {
            created += 2;  // Bidirectional
        }
    }
}
```

### Database Schema (No Changes Required)

Mutual k-NN works with existing schema. Optional metadata can be stored in the `link.metadata` JSONB column.

---

## Implementation Pattern 2: Adaptive k Based on Corpus Size

### Dynamic k Selection

```rust
async fn compute_optimal_k(&self) -> Result<usize> {
    let total_notes = self.db.notes.count_active().await?;

    // Research-backed heuristic: k = log₂(N), clamped to [5, 15]
    let k = (total_notes as f64).log2().round() as usize;
    let k_clamped = k.clamp(5, 15);

    info!(
        total_notes = total_notes,
        raw_k = k,
        clamped_k = k_clamped,
        "Computed optimal k for corpus size"
    );

    Ok(k_clamped)
}
```

**Usage**:
```rust
let k = self.compute_optimal_k().await?;
let candidates = self.db.embeddings
    .find_similar(&embedding, (k + 1) as i64, true)
    .await?;
```

---

## Implementation Pattern 3: RNG Edge Pruning

### Post-Processing Function

Add to `LinkingHandler`:

```rust
/// Prune redundant edges using Relative Neighborhood Graph criterion.
///
/// An edge (A, B) is redundant if there exists a third note C such that:
/// - C is closer to A than B is
/// - C is closer to B than A is
///
/// This preserves bridges while removing redundant intra-cluster edges.
async fn prune_redundant_edges(&self, note_id: Uuid) -> Result<usize> {
    let mut pruned = 0;

    // Get all outgoing links for this note
    let neighbors = self.db.links.get_outgoing(note_id).await?;

    if neighbors.len() < 2 {
        return Ok(0);  // Need at least 2 neighbors to prune
    }

    // For each pair of neighbors (i, j)
    for i in 0..neighbors.len() {
        for j in (i + 1)..neighbors.len() {
            let neighbor_i = &neighbors[i];
            let neighbor_j = &neighbors[j];

            // Get embedding for both neighbors
            let embedding_i = match self.get_first_embedding(neighbor_i.to_note_id.unwrap()).await {
                Some(e) => e,
                None => continue,
            };

            let embedding_j = match self.get_first_embedding(neighbor_j.to_note_id.unwrap()).await {
                Some(e) => e,
                None => continue,
            };

            // Compute similarity between the two neighbors
            let neighbor_similarity = cosine_similarity(&embedding_i.vector.to_vec(), &embedding_j.vector.to_vec());

            // RNG criterion: If neighbors are more similar to each other
            // than either is to the source note, one edge is redundant
            let min_source_similarity = neighbor_i.score.min(neighbor_j.score);

            if neighbor_similarity > min_source_similarity {
                // Keep the higher-scoring link, remove the lower
                let to_remove = if neighbor_i.score > neighbor_j.score {
                    neighbor_j.to_note_id.unwrap()
                } else {
                    neighbor_i.to_note_id.unwrap()
                };

                // Delete the redundant edge
                if let Err(e) = self.db.links.delete_link(note_id, to_remove).await {
                    warn!(error = %e, "Failed to delete redundant link");
                } else {
                    pruned += 1;
                }
            }
        }
    }

    Ok(pruned)
}

/// Helper to get first embedding for a note
async fn get_first_embedding(&self, note_id: Uuid) -> Option<matric_db::Embedding> {
    self.db.embeddings
        .get_for_note(note_id)
        .await
        .ok()?
        .into_iter()
        .next()
}

/// Cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}
```

**Call after k-NN linking**:
```rust
// After creating mutual k-NN links
let pruned = self.prune_redundant_edges(note_id).await?;
info!(
    note_id = %note_id,
    links_created = created,
    edges_pruned = pruned,
    "Linking with RNG pruning completed"
);
```

---

## Implementation Pattern 4: Community Detection + Bridges

### External Dependency

Add to `Cargo.toml`:
```toml
[dependencies]
petgraph = "0.6"  # Graph algorithms library
```

### Community Detection Handler

New file: `/home/roctinam/dev/fortemi/crates/matric-api/src/handlers/graph_optimization.rs`

```rust
use petgraph::graph::UnGraph;
use petgraph::algo::louvain;
use std::collections::HashMap;
use uuid::Uuid;

pub struct GraphOptimizationHandler {
    db: Database,
}

impl GraphOptimizationHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Detect communities using Louvain method
    pub async fn detect_communities(&self) -> Result<Vec<Community>> {
        // 1. Build graph from all links
        let mut graph = UnGraph::new_undirected();
        let mut node_map: HashMap<Uuid, petgraph::graph::NodeIndex> = HashMap::new();

        // Get all notes
        let notes = self.db.notes.list_all_ids().await?;

        for note_id in &notes {
            let idx = graph.add_node(*note_id);
            node_map.insert(*note_id, idx);
        }

        // Get all links and add edges
        let links = self.db.links.list_all(100_000, 0).await?;

        for link in links {
            if let (Some(from), Some(to)) = (link.from_note_id, link.to_note_id) {
                if let (Some(&from_idx), Some(&to_idx)) = (node_map.get(&from), node_map.get(&to)) {
                    graph.add_edge(from_idx, to_idx, link.score);
                }
            }
        }

        // 2. Run Louvain community detection
        let communities = louvain::louvain(&graph);

        // 3. Convert to Community objects
        let mut community_groups: HashMap<usize, Vec<Uuid>> = HashMap::new();

        for (node_idx, community_id) in communities.iter().enumerate() {
            if let Some(note_id) = graph.node_weight(node_idx.into()) {
                community_groups.entry(*community_id)
                    .or_insert_with(Vec::new)
                    .push(*note_id);
            }
        }

        Ok(community_groups
            .into_iter()
            .enumerate()
            .map(|(id, members)| Community {
                id,
                members,
            })
            .collect())
    }

    /// Find best bridge link between two communities
    pub async fn find_best_bridge(&self, c1: &Community, c2: &Community) -> Result<Option<BridgeLink>> {
        let mut best_bridge: Option<BridgeLink> = None;
        let mut best_score = 0.0f32;

        // Compare all pairs of nodes between communities
        for &note1 in &c1.members {
            let embedding1 = match self.get_first_embedding(note1).await {
                Some(e) => e,
                None => continue,
            };

            for &note2 in &c2.members {
                let embedding2 = match self.get_first_embedding(note2).await {
                    Some(e) => e,
                    None => continue,
                };

                let similarity = cosine_similarity(&embedding1.vector.to_vec(), &embedding2.vector.to_vec());

                if similarity > best_score {
                    best_score = similarity;
                    best_bridge = Some(BridgeLink {
                        from: note1,
                        to: note2,
                        score: similarity,
                        community_from: c1.id,
                        community_to: c2.id,
                    });
                }
            }
        }

        Ok(best_bridge)
    }

    /// Create bridge links between all community pairs
    pub async fn create_inter_community_bridges(&self) -> Result<usize> {
        let communities = self.detect_communities().await?;
        let mut bridges_created = 0;

        // Lower threshold for bridges (they're important even if not super similar)
        const BRIDGE_THRESHOLD: f32 = 0.6;

        // For each pair of communities
        for i in 0..communities.len() {
            for j in (i + 1)..communities.len() {
                let c1 = &communities[i];
                let c2 = &communities[j];

                if let Some(bridge) = self.find_best_bridge(c1, c2).await? {
                    if bridge.score >= BRIDGE_THRESHOLD {
                        let metadata = serde_json::json!({
                            "is_bridge": true,
                            "community_from": bridge.community_from,
                            "community_to": bridge.community_to,
                            "bridge_strength": bridge.score
                        });

                        if let Err(e) = self.db.links.create_reciprocal(
                            bridge.from,
                            bridge.to,
                            "bridge",
                            bridge.score,
                            Some(metadata)
                        ).await {
                            warn!(error = %e, "Failed to create bridge link");
                        } else {
                            bridges_created += 2;  // Bidirectional
                        }
                    }
                }
            }
        }

        Ok(bridges_created)
    }
}

#[derive(Debug, Clone)]
pub struct Community {
    pub id: usize,
    pub members: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct BridgeLink {
    pub from: Uuid,
    pub to: Uuid,
    pub score: f32,
    pub community_from: usize,
    pub community_to: usize,
}
```

### Periodic Batch Job

Add to job types in `matric-core/src/traits.rs`:
```rust
pub enum JobType {
    // ... existing types
    GraphOptimization,
}
```

Schedule as periodic job (e.g., daily):
```rust
// In job worker startup
let handler = GraphOptimizationHandler::new(db.clone());
let bridges_created = handler.create_inter_community_bridges().await?;
info!(bridges_created = bridges_created, "Graph optimization completed");
```

---

## Configuration Management

### Environment Variables

Add to `.env`:
```bash
# Graph topology configuration
GRAPH_LINKING_STRATEGY=mutual_knn  # Options: threshold, knn, mutual_knn, rng
GRAPH_K_NEIGHBORS=7  # Number of nearest neighbors (5-15)
GRAPH_ADAPTIVE_K=true  # Auto-adjust k based on corpus size
GRAPH_PRUNE_REDUNDANT=true  # Apply RNG pruning
GRAPH_DETECT_BRIDGES=true  # Detect and create inter-community bridges
GRAPH_BRIDGE_THRESHOLD=0.6  # Minimum similarity for bridge links
```

### Configuration Struct

Add to `matric-core/src/config.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub strategy: LinkingStrategy,
    pub k_neighbors: usize,
    pub adaptive_k: bool,
    pub prune_redundant: bool,
    pub detect_bridges: bool,
    pub bridge_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkingStrategy {
    Threshold,  // Current implementation
    KNN,  // k-nearest neighbors
    MutualKNN,  // Mutual k-NN (recommended)
    RNG,  // Relative Neighborhood Graph
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            strategy: LinkingStrategy::MutualKNN,
            k_neighbors: 7,
            adaptive_k: true,
            prune_redundant: true,
            detect_bridges: false,  // Expensive, opt-in
            bridge_threshold: 0.6,
        }
    }
}

impl GraphConfig {
    pub fn from_env() -> Self {
        Self {
            strategy: std::env::var("GRAPH_LINKING_STRATEGY")
                .ok()
                .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok())
                .unwrap_or(LinkingStrategy::MutualKNN),
            k_neighbors: std::env::var("GRAPH_K_NEIGHBORS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(7),
            adaptive_k: std::env::var("GRAPH_ADAPTIVE_K")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            prune_redundant: std::env::var("GRAPH_PRUNE_REDUNDANT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            detect_bridges: std::env::var("GRAPH_DETECT_BRIDGES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
            bridge_threshold: std::env::var("GRAPH_BRIDGE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.6),
        }
    }
}
```

---

## Metrics and Monitoring

### Graph Topology Stats Endpoint

Add to `main.rs`:
```rust
#[utoipa::path(get, path = "/api/v1/graph/topology/stats", tag = "Graph",
    responses(
        (status = 200, description = "Graph topology statistics")
    )
)]
async fn get_topology_stats(
    Extension(db): Extension<Database>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let stats = compute_topology_stats(&db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!(stats)))
}

async fn compute_topology_stats(db: &Database) -> Result<TopologyStats> {
    let all_notes = db.notes.list_all_ids().await?;
    let total_nodes = all_notes.len();

    if total_nodes == 0 {
        return Ok(TopologyStats::default());
    }

    let mut total_degree = 0;
    let mut degree_distribution: HashMap<usize, usize> = HashMap::new();
    let mut triangle_count = 0;
    let mut connected_triple_count = 0;

    for note_id in &all_notes {
        let outgoing = db.links.get_outgoing(*note_id).await?;
        let degree = outgoing.len();
        total_degree += degree;

        *degree_distribution.entry(degree).or_insert(0) += 1;

        // Count triangles (simplified - full count requires checking all triples)
        for i in 0..outgoing.len() {
            for j in (i + 1)..outgoing.len() {
                connected_triple_count += 1;

                // Check if outgoing[i] and outgoing[j] are connected
                let i_neighbors = db.links.get_outgoing(outgoing[i].to_note_id.unwrap()).await?;
                if i_neighbors.iter().any(|n| n.to_note_id == outgoing[j].to_note_id) {
                    triangle_count += 1;
                }
            }
        }
    }

    let avg_degree = if total_nodes > 0 {
        total_degree as f64 / total_nodes as f64
    } else {
        0.0
    };

    let clustering_coefficient = if connected_triple_count > 0 {
        triangle_count as f64 / connected_triple_count as f64
    } else {
        0.0
    };

    // Determine topology type based on metrics
    let topology_type = match clustering_coefficient {
        c if c < 0.1 => "star",
        c if c < 0.4 => "transitional",
        c if c < 0.7 => "mesh",
        _ => "small_world",
    };

    Ok(TopologyStats {
        total_nodes,
        total_edges: total_degree / 2,  // Bidirectional
        avg_degree,
        clustering_coefficient,
        degree_distribution,
        topology_type: topology_type.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopologyStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub avg_degree: f64,
    pub clustering_coefficient: f64,
    pub degree_distribution: HashMap<usize, usize>,
    pub topology_type: String,
}
```

---

## Testing Strategy

### Unit Tests

Add to `crates/matric-db/tests/linking.rs`:

```rust
#[tokio::test]
async fn test_mutual_knn_creates_bidirectional_links() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create 5 notes with similar embeddings
    let notes = create_test_notes_with_embeddings(&db, 5, 0.85).await;

    // Run mutual k-NN linking with k=3
    let handler = LinkingHandler::new(db.clone());
    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        k_neighbors: 3,
        ..Default::default()
    };

    for note_id in &notes {
        handler.execute_with_config(*note_id, &config).await.unwrap();
    }

    // Verify each note has at most 3 outgoing links
    for note_id in &notes {
        let outgoing = db.links.get_outgoing(*note_id).await.unwrap();
        assert!(outgoing.len() <= 3, "Note {} has {} links (expected <= 3)", note_id, outgoing.len());
    }

    // Verify all links are bidirectional
    for note_id in &notes {
        let outgoing = db.links.get_outgoing(*note_id).await.unwrap();
        for link in outgoing {
            let reverse_links = db.links.get_incoming(link.to_note_id.unwrap()).await.unwrap();
            assert!(
                reverse_links.iter().any(|r| r.from_note_id == *note_id),
                "Bidirectional link missing for {} -> {}",
                note_id,
                link.to_note_id.unwrap()
            );
        }
    }
}

#[tokio::test]
async fn test_topology_improves_with_knn() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create 20 notes in same topic cluster
    let notes = create_test_notes_with_embeddings(&db, 20, 0.75).await;

    // Baseline: Threshold linking
    let threshold_config = GraphConfig {
        strategy: LinkingStrategy::Threshold,
        ..Default::default()
    };

    let handler = LinkingHandler::new(db.clone());
    for note_id in &notes {
        handler.execute_with_config(*note_id, &threshold_config).await.unwrap();
    }

    let threshold_stats = compute_topology_stats(&db).await.unwrap();

    // Clear links
    for note_id in &notes {
        db.links.delete_for_note(*note_id).await.unwrap();
    }

    // k-NN linking
    let knn_config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        k_neighbors: 5,
        ..Default::default()
    };

    for note_id in &notes {
        handler.execute_with_config(*note_id, &knn_config).await.unwrap();
    }

    let knn_stats = compute_topology_stats(&db).await.unwrap();

    // k-NN should have higher clustering coefficient
    assert!(
        knn_stats.clustering_coefficient > threshold_stats.clustering_coefficient,
        "k-NN clustering ({}) should be higher than threshold ({})",
        knn_stats.clustering_coefficient,
        threshold_stats.clustering_coefficient
    );

    // k-NN should have more uniform degree distribution
    let threshold_std_dev = compute_std_dev(&threshold_stats.degree_distribution);
    let knn_std_dev = compute_std_dev(&knn_stats.degree_distribution);

    assert!(
        knn_std_dev < threshold_std_dev,
        "k-NN degree distribution ({}) should be more uniform than threshold ({})",
        knn_std_dev,
        threshold_std_dev
    );
}
```

---

## Performance Benchmarks

### Benchmark Targets

| Operation | Current (Threshold) | Target (k-NN) |
|-----------|---------------------|---------------|
| Link creation (per note) | 50-200ms | 50-100ms |
| RNG pruning (per note) | N/A | 100-300ms |
| Community detection (1000 notes) | N/A | 1-2 seconds |
| Graph traversal (depth=3) | 20-50ms | 20-50ms |

### Benchmark Code

```rust
#[tokio::test]
async fn bench_linking_strategies() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let notes = create_test_notes_with_embeddings(&db, 100, 0.75).await;

    // Benchmark threshold linking
    let start = Instant::now();
    for note_id in &notes {
        execute_threshold_linking(&db, *note_id).await.unwrap();
    }
    let threshold_duration = start.elapsed();

    // Clear links
    for note_id in &notes {
        db.links.delete_for_note(*note_id).await.unwrap();
    }

    // Benchmark k-NN linking
    let start = Instant::now();
    for note_id in &notes {
        execute_knn_linking(&db, *note_id, 7).await.unwrap();
    }
    let knn_duration = start.elapsed();

    println!("Threshold: {:?}", threshold_duration);
    println!("k-NN: {:?}", knn_duration);
    println!("Ratio: {:.2}x", knn_duration.as_secs_f64() / threshold_duration.as_secs_f64());

    // k-NN should be within 2x of threshold performance
    assert!(knn_duration < threshold_duration * 2);
}
```

---

## Migration Path

### Phase 1: Dual-Mode Operation

Support both strategies in parallel:

```rust
match config.strategy {
    LinkingStrategy::Threshold => {
        // Existing code path
        execute_threshold_linking(/* ... */).await?;
    }
    LinkingStrategy::MutualKNN => {
        // New code path
        execute_mutual_knn_linking(/* ... */).await?;
    }
    _ => unimplemented!("Strategy not yet implemented"),
}
```

### Phase 2: A/B Testing

Split users into cohorts:
- 80% threshold (control)
- 20% mutual k-NN (experimental)

Track metrics:
- Graph traversal depth distribution
- User engagement with related notes
- Query: "How often do users click links beyond depth=1?"

### Phase 3: Gradual Rollout

If metrics improve:
1. Week 1: 50/50 split
2. Week 2: 80% k-NN
3. Week 3: 100% k-NN, deprecate threshold

---

## Troubleshooting

### Problem: Graph becomes disconnected

**Symptom**: Some notes have zero links after k-NN

**Cause**: k is too low, or notes are true outliers

**Fix**:
```rust
// Enforce minimum degree
if links_created == 0 && candidates.len() > 0 {
    // Fallback: Link to single best match regardless of mutuality
    let best = candidates[0];
    self.db.links.create_reciprocal(note_id, best.note_id, "semantic", best.score, None).await?;
}
```

### Problem: RNG pruning too slow

**Symptom**: Linking job takes > 1 second per note

**Cause**: O(N²) comparison of all neighbor pairs

**Fix**: Only prune if degree > threshold
```rust
if neighbors.len() > 15 {  // Only prune if excessively connected
    self.prune_redundant_edges(note_id).await?;
}
```

### Problem: Community detection fails

**Symptom**: Error in Louvain algorithm

**Cause**: Graph structure incompatible with petgraph

**Fix**: Validate graph before community detection
```rust
if graph.node_count() < 10 {
    return Ok(vec![]);  // Too small for meaningful communities
}
```

---

*Last Updated: 2026-02-14*
*Companion to: knowledge-graph-topology-techniques.md*
