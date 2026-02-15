# Graph Topology: Quick Reference Card

**For**: Developers implementing graph topology improvements
**Quick Links**:
- Comprehensive Research: `knowledge-graph-topology-techniques.md`
- Implementation Guide: `graph-topology-implementation-guide.md`
- Executive Summary: `graph-topology-executive-summary.md`

---

## TL;DR

**Problem**: Current auto-linking creates star clusters (all notes → hub), not mesh networks (distributed connections)

**Solution**: Switch from threshold-based to mutual k-NN linking

**Effort**: ~50 lines of code in `/home/roctinam/dev/fortemi/crates/matric-api/src/handlers/jobs.rs`

**Impact**: Enables meaningful multi-hop graph traversal

---

## Linking Strategies Compared

| Strategy | Topology | Links/Node | Code Complexity | Status |
|----------|----------|------------|-----------------|--------|
| **Threshold (current)** | Star | 0 or 20+ | Simple | Production |
| **k-NN** | Mesh | ~k | Simple | Recommended |
| **Mutual k-NN** | Sparse mesh | ~k/2 | Simple | **Best choice** |
| **RNG** | Optimal sparse | Variable | Medium | Optional enhancement |
| **Community + Bridges** | Hierarchical | Variable | Complex | Enterprise scale |

---

## Minimal Implementation (30 min)

Replace threshold logic in `LinkingHandler::execute` (line 800):

### Before (Threshold)
```rust
let similar = self.db.embeddings.find_similar(&embeddings[0].vector, 10, true).await?;

for hit in similar {
    if hit.score < link_threshold {
        continue;
    }
    // Create link to EVERY note above threshold
}
```

### After (Mutual k-NN)
```rust
const K: usize = 7;

let candidates = self.db.embeddings
    .find_similar(&embeddings[0].vector, (K + 1) as i64, true)
    .await?
    .into_iter()
    .skip(1)  // Skip self
    .take(K)
    .collect::<Vec<_>>();

for candidate in candidates {
    // Get candidate's k-NN
    let cand_emb = self.db.embeddings.get_for_note(candidate.note_id).await?.into_iter().next();
    let reverse = self.db.embeddings.find_similar(&cand_emb.unwrap().vector, (K+1) as i64, true).await?;

    // Only link if mutual
    if reverse.iter().any(|n| n.note_id == note_id) {
        self.db.links.create_reciprocal(note_id, candidate.note_id, "semantic", candidate.score, None).await?;
    }
}
```

**That's it.** Graph topology improves immediately.

---

## Configuration

### Environment Variables

Add to `.env`:
```bash
GRAPH_LINKING_STRATEGY=mutual_knn  # or "threshold" for legacy
GRAPH_K_NEIGHBORS=7
```

### Adaptive k Formula
```rust
let k = (total_notes as f64).log2().round().clamp(5.0, 15.0) as usize;
```

| Corpus Size | Recommended k |
|-------------|---------------|
| <100 notes | 5 |
| 100-1,000 notes | 7 |
| 1,000-10,000 notes | 10 |
| >10,000 notes | 15 |

---

## Testing

### Unit Test
```rust
#[tokio::test]
async fn test_mutual_knn_limits_degree() {
    let notes = create_test_notes(10).await;
    for note in &notes {
        execute_mutual_knn(note, k=5).await;
    }

    for note in &notes {
        let links = db.links.get_outgoing(note).await;
        assert!(links.len() <= 5);  // Degree bounded by k
    }
}
```

### Metrics to Check
```rust
// Before and after comparison
let stats = compute_topology_stats(&db).await;
assert!(stats.clustering_coefficient > 0.3);  // Mesh, not star
assert!(stats.avg_degree < 15.0);  // Not too dense
```

---

## Troubleshooting

### Problem: Some notes have zero links

**Cause**: No mutual k-NN neighbors (outlier topic)

**Fix**: Fallback to single best link
```rust
if created == 0 && candidates.len() > 0 {
    self.db.links.create(note_id, candidates[0].note_id, "semantic", candidates[0].score, None).await?;
}
```

### Problem: Still seeing star topology

**Cause**: k is too high or corpus is small

**Fix**: Reduce k
```bash
GRAPH_K_NEIGHBORS=3  # More restrictive
```

### Problem: Graph feels disconnected

**Cause**: k is too low or mutual filter too strict

**Fix**: Increase k or use asymmetric k-NN
```bash
GRAPH_K_NEIGHBORS=10  # More permissive
```

---

## Advanced: RNG Pruning

**When**: Graphs with >15 links/node need optimization

**Function**:
```rust
async fn prune_redundant_edges(&self, note_id: Uuid) -> Result<usize> {
    let neighbors = self.db.links.get_outgoing(note_id).await?;
    let mut pruned = 0;

    for i in 0..neighbors.len() {
        for j in (i+1)..neighbors.len() {
            let sim_ij = compute_similarity(neighbors[i], neighbors[j]).await?;
            let min_source_sim = neighbors[i].score.min(neighbors[j].score);

            if sim_ij > min_source_sim {
                // Neighbors are more similar to each other than to source
                // Remove the weaker link
                delete_link(note_id, weaker_of(neighbors[i], neighbors[j])).await?;
                pruned += 1;
            }
        }
    }
    Ok(pruned)
}
```

**Call after k-NN**:
```rust
if neighbors.len() > 15 {
    let pruned = self.prune_redundant_edges(note_id).await?;
    info!(pruned = pruned, "Pruned redundant edges");
}
```

---

## Metrics API

### Add Endpoint

```rust
#[utoipa::path(get, path = "/api/v1/graph/topology/stats")]
async fn get_topology_stats(Extension(db): Extension<Database>) -> Json<Value> {
    let stats = compute_stats(&db).await.unwrap();
    Json(json!({
        "clustering_coefficient": stats.clustering,
        "avg_degree": stats.avg_degree,
        "topology_type": if stats.clustering > 0.4 { "mesh" } else { "star" }
    }))
}
```

### Test
```bash
curl http://localhost:3000/api/v1/graph/topology/stats
# {"clustering_coefficient":0.42,"avg_degree":7.2,"topology_type":"mesh"}
```

---

## Migration Strategy

### Option A: Dual-Mode (Safe)
```rust
match env::var("GRAPH_LINKING_STRATEGY").as_deref() {
    Ok("mutual_knn") => execute_mutual_knn(...),
    _ => execute_threshold(...),  // Legacy default
}
```

### Option B: Feature Flag
```rust
if cfg!(feature = "experimental_knn") {
    execute_mutual_knn(...);
} else {
    execute_threshold(...);
}
```

### Option C: Gradual Rollout
```rust
// Hash user_id to cohort
let cohort = hash(user_id) % 100;
if cohort < rollout_percent {
    execute_mutual_knn(...);
} else {
    execute_threshold(...);
}
```

---

## Performance Expectations

| Operation | Current (Threshold) | k-NN | k-NN + RNG |
|-----------|---------------------|------|------------|
| Link creation | 50-200ms | 50-100ms | 150-400ms |
| Edges created | 0-50 per note | 3-7 per note | 2-5 per note |
| Graph traversal | <50ms | <50ms | <50ms |

**Why k-NN is faster**: HNSW already computes k-NN for search; threshold filtering adds extra work.

---

## Cheat Sheet: When to Use What

### Use Mutual k-NN When:
- ✅ You want mesh topology (not star clusters)
- ✅ Corpus size is 100+ notes
- ✅ Graph traversal is a core feature
- ✅ You want predictable link counts

### Use Threshold When:
- ✅ Legacy compatibility required
- ✅ Corpus size <100 notes
- ✅ Dense clusters are desired behavior
- ✅ Graph traversal is not critical

### Add RNG Pruning When:
- ✅ k-NN creates too many links (>15/node)
- ✅ Performance budget allows O(N²) per node
- ✅ Bridge preservation is critical

### Add Community Detection When:
- ✅ Corpus size >10,000 notes
- ✅ Explicit topic hierarchies needed
- ✅ Inter-cluster navigation is key use case

---

## Code Locations

| File | Lines | Description |
|------|-------|-------------|
| `crates/matric-api/src/handlers/jobs.rs` | 665-863 | `LinkingHandler::execute` - Main linking logic |
| `crates/matric-db/src/links.rs` | 24-112 | Link repository methods |
| `crates/matric-core/src/defaults.rs` | 284-320 | Similarity threshold constants |
| `crates/matric-db/tests/linking.rs` | 1-747 | Linking tests (placeholders ready) |
| `docs/content/knowledge-graph-guide.md` | 1-195 | User-facing graph documentation |

---

## Research Paper Quick Picks

**Must Read**:
1. Dong et al. (2011) - "Efficient k-nearest neighbor graph construction" - **Why k-NN works**
2. Watts & Strogatz (1998) - "Small-world networks" - **Topology theory**

**If Time Permits**:
3. Jaromczyk & Toussaint (1992) - "Relative neighborhood graphs" - **RNG pruning**
4. Blondel et al. (2008) - "Fast unfolding of communities" - **Louvain algorithm**

**Foundation**:
5. Barabási (2016) - *Network Science* textbook - **Everything graphs**

---

## Decision Tree

```
Do you want mesh topology instead of star clusters?
├─ YES → Continue
│  └─ Is corpus >100 notes?
│     ├─ YES → Use mutual k-NN with k=7
│     │  └─ Are graphs still too dense (>15 links/node)?
│     │     ├─ YES → Add RNG pruning
│     │     └─ NO → Done!
│     └─ NO → Use mutual k-NN with k=5 or wait for growth
└─ NO → Keep threshold-based linking
```

---

## Validation Checklist

After implementing k-NN:

- [ ] Clustering coefficient >0.3 (run `/api/v1/graph/topology/stats`)
- [ ] Average degree 5-10 (not 0 or 20+)
- [ ] Graph traversal depth distribution shifted (more depth>1 visits)
- [ ] No isolated nodes (all notes have ≥1 link)
- [ ] Performance <100ms per note linking
- [ ] Unit tests pass (mutual k-NN creates bidirectional links)
- [ ] Integration tests pass (topology metrics in range)

---

## Git Commit Message Template

```
feat(linking): implement mutual k-NN graph topology

Replace threshold-based linking with mutual k-NN to create mesh
topology instead of star clusters. Enables meaningful multi-hop
graph traversal.

Changes:
- Add mutual k-NN strategy to LinkingHandler
- Make k configurable via GRAPH_K_NEIGHBORS env var
- Preserve legacy threshold mode via GRAPH_LINKING_STRATEGY
- Add topology metrics endpoint at /api/v1/graph/topology/stats

Impact:
- Clustering coefficient improves from ~0.0 to ~0.4
- Average degree becomes bounded (5-10 links/node)
- Graph traversal depth distribution shifts toward depth>1

Research: docs/research/knowledge-graph-topology-techniques.md

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```

---

## Quick Test Command

```bash
# 1. Update env
echo "GRAPH_LINKING_STRATEGY=mutual_knn" >> .env
echo "GRAPH_K_NEIGHBORS=7" >> .env

# 2. Restart API
docker compose -f docker-compose.bundle.yml restart

# 3. Create test notes
for i in {1..20}; do
  curl -X POST http://localhost:3000/api/v1/notes \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Test note $i about machine learning\"}"
done

# 4. Wait for linking jobs to complete
sleep 10

# 5. Check topology
curl http://localhost:3000/api/v1/graph/topology/stats

# Expected: clustering_coefficient > 0.3, avg_degree 5-10
```

---

## FAQ

**Q: Will this break existing knowledge bases?**
A: No. Links are additive. Worst case: old star topology + new mesh topology coexist.

**Q: Can users still use manual links?**
A: Yes. This only affects automatic semantic linking.

**Q: What if k=7 is wrong for my corpus?**
A: Use adaptive k formula: `k = log₂(N)` clamped to [5, 15].

**Q: How do I revert if it doesn't work?**
A: Set `GRAPH_LINKING_STRATEGY=threshold` and restart.

**Q: Does this affect search performance?**
A: No. Search uses HNSW index directly, not link table.

**Q: Can I visualize the topology change?**
A: Yes. Use D3.js force-directed layout on `/api/v1/graph/{id}/explore` data.

---

*Quick reference version: 2026-02-14*
*For full details, see comprehensive research document*
