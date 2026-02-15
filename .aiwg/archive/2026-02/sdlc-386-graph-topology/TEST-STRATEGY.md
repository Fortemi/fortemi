# Test Strategy: Graph Topology Improvement (Issue #386)

**Feature**: Replace threshold-based auto-linking with mutual k-NN linking
**Issue**: #386
**Author**: Test Architect
**Date**: 2026-02-14
**Status**: DRAFT

---

## 1. Test Strategy Overview

### 1.1 Scope

This test strategy covers the replacement of threshold-based semantic linking with mutual k-NN (k-nearest neighbors) linking to improve graph topology quality.

**In Scope:**
- GraphConfig parsing from environment variables
- Mutual k-NN algorithm implementation
- Adaptive k computation based on corpus size
- Isolated node fallback mechanism
- Topology metrics endpoint (`GET /api/v1/graph/topology/stats`)
- Backward compatibility with threshold-based linking
- Performance validation (≤200ms per note)

**Out of Scope:**
- Wiki-style `[[link]]` parsing (unchanged)
- Embedding generation (existing functionality)
- Graph traversal API (existing functionality)
- Multi-memory archive linking (future enhancement)

### 1.2 Quality Objectives

| Objective | Target | Measurement |
|-----------|--------|-------------|
| **Code Coverage** | >90% line coverage for new code | cargo-llvm-cov |
| **Functional Correctness** | 100% FR requirements satisfied | Manual test case verification |
| **Performance** | ≤200ms linking latency per note | Benchmark tests with real corpus |
| **Backward Compatibility** | Zero regression in threshold mode | Regression test suite |
| **Graph Quality** | Higher clustering coefficient vs threshold | Topology metrics comparison |

### 1.3 Risk-Based Testing Priorities

| Risk | Impact | Likelihood | Priority | Mitigation |
|------|--------|------------|----------|------------|
| Mutual k-NN creates zero links for isolated notes | HIGH | MEDIUM | **P0** | Isolated node fallback + extensive edge case testing |
| Performance regression on large corpora (>10K notes) | HIGH | MEDIUM | **P0** | Benchmark suite with synthetic corpora |
| Breaking existing threshold-based linking | MEDIUM | LOW | **P1** | Regression tests + integration tests |
| Adaptive k formula incorrect for edge cases (N=0, N=1) | MEDIUM | MEDIUM | **P1** | Unit tests with boundary conditions |
| Topology stats sampling bias | LOW | MEDIUM | **P2** | Statistical validation of sampling |

### 1.4 Test Approach

**Test Pyramid Distribution:**
- **70% Unit Tests**: Config parsing, adaptive k computation, topology classification
- **25% Integration Tests**: LinkingHandler with both strategies, end-to-end API
- **5% Performance Tests**: Latency benchmarks, corpus size scalability

**Isolation Strategy:**
- Use UUIDs for test note identifiers (never timestamp millis)
- No `std::env::set_var` (constructor injection for config)
- `#[tokio::test]` with manual pool setup (not `#[sqlx::test]` due to non-transactional requirements)

---

## 2. Test Levels

### 2.1 Unit Tests

**Location**: Inline `#[cfg(test)]` modules in respective crates

#### 2.1.1 GraphConfig Parsing (`matric-core`)

**Purpose**: Validate environment variable parsing and default values

**Test Module**: `crates/matric-core/src/graph_config.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GraphConfig::default();
        assert_eq!(config.strategy, LinkingStrategy::Threshold);
        assert_eq!(config.min_k, 5);
        assert_eq!(config.max_k, 15);
        assert!(config.enable_fallback);
    }

    #[test]
    fn test_parse_strategy_threshold() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_LINKING_STRATEGY" => Some("threshold".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.strategy, LinkingStrategy::Threshold);
    }

    #[test]
    fn test_parse_strategy_mutual_knn() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_LINKING_STRATEGY" => Some("mutual_knn".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.strategy, LinkingStrategy::MutualKNN);
    }

    #[test]
    fn test_parse_strategy_invalid_defaults_to_threshold() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_LINKING_STRATEGY" => Some("invalid_value".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.strategy, LinkingStrategy::Threshold);
    }

    #[test]
    fn test_parse_min_k_valid() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_MIN_K" => Some("3".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.min_k, 3);
    }

    #[test]
    fn test_parse_min_k_invalid_uses_default() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_MIN_K" => Some("not_a_number".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.min_k, 5); // default
    }

    #[test]
    fn test_parse_max_k_valid() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_MAX_K" => Some("20".to_string()),
                _ => None,
            }
        });
        assert_eq!(config.max_k, 20);
    }

    #[test]
    fn test_parse_enable_fallback_true() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_ENABLE_FALLBACK" => Some("true".to_string()),
                _ => None,
            }
        });
        assert!(config.enable_fallback);
    }

    #[test]
    fn test_parse_enable_fallback_false() {
        let config = GraphConfig::from_env_with(|key| {
            match key {
                "GRAPH_ENABLE_FALLBACK" => Some("false".to_string()),
                _ => None,
            }
        });
        assert!(!config.enable_fallback);
    }
}
```

#### 2.1.2 Adaptive k Computation (`matric-core`)

**Purpose**: Validate k computation formula: `max(min_k, min(ceil(log2(N)), max_k))`

**Test Module**: `crates/matric-core/src/graph_config.rs`

```rust
#[cfg(test)]
mod adaptive_k_tests {
    use super::*;

    #[test]
    fn test_adaptive_k_zero_corpus() {
        let config = GraphConfig::default();
        let k = config.compute_adaptive_k(0);
        assert_eq!(k, 5); // Should clamp to min_k
    }

    #[test]
    fn test_adaptive_k_single_note() {
        let config = GraphConfig::default();
        let k = config.compute_adaptive_k(1);
        assert_eq!(k, 5); // log2(1) = 0, clamps to min_k
    }

    #[test]
    fn test_adaptive_k_small_corpus() {
        let config = GraphConfig::default();
        let k = config.compute_adaptive_k(10);
        // log2(10) ≈ 3.32, ceil = 4, but min_k = 5
        assert_eq!(k, 5);
    }

    #[test]
    fn test_adaptive_k_medium_corpus() {
        let config = GraphConfig::default();
        let k = config.compute_adaptive_k(256);
        // log2(256) = 8, within [5, 15]
        assert_eq!(k, 8);
    }

    #[test]
    fn test_adaptive_k_large_corpus() {
        let config = GraphConfig::default();
        let k = config.compute_adaptive_k(100_000);
        // log2(100000) ≈ 16.6, ceil = 17, clamps to max_k = 15
        assert_eq!(k, 15);
    }

    #[test]
    fn test_adaptive_k_custom_bounds() {
        let mut config = GraphConfig::default();
        config.min_k = 3;
        config.max_k = 10;

        assert_eq!(config.compute_adaptive_k(8), 3);   // log2(8) = 3, at min boundary
        assert_eq!(config.compute_adaptive_k(64), 6);  // log2(64) = 6, within range
        assert_eq!(config.compute_adaptive_k(2048), 10); // log2(2048) = 11, clamps to max
    }
}
```

#### 2.1.3 Topology Classification (`matric-db`)

**Purpose**: Validate degree distribution classification logic

**Test Module**: `crates/matric-db/src/links.rs`

```rust
#[cfg(test)]
mod topology_tests {
    use super::*;

    #[test]
    fn test_classify_topology_star() {
        let degrees = vec![10, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let classification = classify_topology(&degrees);
        assert_eq!(classification, TopologyType::Star);
    }

    #[test]
    fn test_classify_topology_mesh() {
        let degrees = vec![5, 5, 5, 5, 5, 5];
        let classification = classify_topology(&degrees);
        assert_eq!(classification, TopologyType::Mesh);
    }

    #[test]
    fn test_classify_topology_scale_free() {
        // Power-law distribution: few hubs, many low-degree nodes
        let degrees = vec![20, 15, 10, 3, 3, 2, 2, 1, 1, 1, 1, 1];
        let classification = classify_topology(&degrees);
        assert_eq!(classification, TopologyType::ScaleFree);
    }

    #[test]
    fn test_classify_topology_sparse() {
        let degrees = vec![1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
        let classification = classify_topology(&degrees);
        assert_eq!(classification, TopologyType::Sparse);
    }
}
```

### 2.2 Integration Tests

**Location**: `crates/matric-api/tests/graph_linking_tests.rs`

#### 2.2.1 Database Setup Helper

```rust
use sqlx::{Pool, Postgres};
use matric_db::Database;
use uuid::Uuid;

async fn setup_test_pool() -> Pool<Postgres> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());

    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create test pool")
}

async fn create_test_note(
    db: &Database,
    content: &str,
    embedding: Option<Vec<f32>>,
) -> Uuid {
    let note_id = Uuid::new_v4(); // Use UUIDs for isolation

    // Create note
    db.notes.create(
        note_id,
        Some(content.to_string()),
        content.to_string(),
        None, // tags
        None, // collection_id
    ).await.unwrap();

    // Add embedding if provided
    if let Some(vec) = embedding {
        db.embeddings.create(
            note_id,
            vec,
            "default".to_string(),
            None,
        ).await.unwrap();
    }

    note_id
}
```

#### 2.2.2 Mutual k-NN Core Algorithm Tests

```rust
#[tokio::test]
async fn test_mutual_knn_creates_mutual_links() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create 5 notes with known embedding similarities
    // Notes A, B, C form a tight cluster (cosine similarity > 0.9)
    // Notes D, E are distant outliers
    let note_a = create_test_note(&db, "Rust programming", Some(vec![1.0, 0.0, 0.0])).await;
    let note_b = create_test_note(&db, "Rust language", Some(vec![0.95, 0.1, 0.0])).await;
    let note_c = create_test_note(&db, "Rust tutorial", Some(vec![0.9, 0.15, 0.0])).await;
    let note_d = create_test_note(&db, "Python basics", Some(vec![0.0, 1.0, 0.0])).await;
    let note_e = create_test_note(&db, "JavaScript guide", Some(vec![0.0, 0.0, 1.0])).await;

    // Configure mutual k-NN with k=2
    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        min_k: 2,
        max_k: 5,
        enable_fallback: false,
    };

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.expect("Linking failed");

    // Verify mutual links created
    let outgoing = db.links.get_outgoing(note_a).await.unwrap();

    // Should have links to B and C (mutual k-NN verified)
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.iter().any(|l| l.to_note_id == note_b));
    assert!(outgoing.iter().any(|l| l.to_note_id == note_c));

    // Verify metadata includes mutual_knn marker
    let link_to_b = outgoing.iter().find(|l| l.to_note_id == note_b).unwrap();
    assert_eq!(link_to_b.kind, "semantic");
    assert!(link_to_b.metadata.as_ref()
        .and_then(|m| m.get("strategy"))
        .map(|s| s.as_str() == Some("mutual_knn"))
        .unwrap_or(false));
}

#[tokio::test]
async fn test_mutual_knn_rejects_non_mutual() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create asymmetric similarity:
    // A → B (high similarity)
    // B → C (high similarity)
    // A → C (low similarity)
    let note_a = create_test_note(&db, "Rust programming", Some(vec![1.0, 0.0, 0.0])).await;
    let note_b = create_test_note(&db, "Rust and Python", Some(vec![0.6, 0.6, 0.0])).await;
    let note_c = create_test_note(&db, "Python basics", Some(vec![0.0, 1.0, 0.0])).await;

    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        min_k: 1,
        max_k: 5,
        enable_fallback: false,
    };

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.expect("Linking failed");

    let outgoing = db.links.get_outgoing(note_a).await.unwrap();

    // Should only link to B (mutual top-1)
    // NOT to C (C's top-1 is B, not A)
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to_note_id, note_b);
}

#[tokio::test]
async fn test_mutual_knn_bounded_degree() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create 20 very similar notes
    let mut note_ids = Vec::new();
    for i in 0..20 {
        let embedding = vec![1.0, i as f32 * 0.01, 0.0]; // Slight variations
        let note_id = create_test_note(
            &db,
            &format!("Similar note {}", i),
            Some(embedding),
        ).await;
        note_ids.push(note_id);
    }

    // k=5 should limit degree to 5
    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        min_k: 5,
        max_k: 5,
        enable_fallback: false,
    };

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_ids[0]));

    handler.execute(ctx).await.expect("Linking failed");

    let outgoing = db.links.get_outgoing(note_ids[0]).await.unwrap();

    // Should have at most k=5 links
    assert!(outgoing.len() <= 5, "Degree exceeds k limit: {}", outgoing.len());
}
```

#### 2.2.3 Isolated Node Fallback Tests

```rust
#[tokio::test]
async fn test_fallback_activates_for_isolated_node() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create a single note with no mutual neighbors
    // (all other notes are in a different cluster)
    let isolated = create_test_note(&db, "Completely unique content", Some(vec![1.0, 0.0, 0.0])).await;

    // Cluster of unrelated notes
    for i in 0..10 {
        create_test_note(
            &db,
            &format!("Unrelated topic {}", i),
            Some(vec![0.0, 1.0, i as f32 * 0.01]),
        ).await;
    }

    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        min_k: 5,
        max_k: 10,
        enable_fallback: true, // Enable fallback
    };

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(isolated));

    let result = handler.execute(ctx).await.expect("Linking failed");

    // Verify fallback was used
    assert!(result.as_object()
        .and_then(|o| o.get("fallback_used"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false));

    let outgoing = db.links.get_outgoing(isolated).await.unwrap();

    // Should have exactly 1 link (best-match fallback)
    assert_eq!(outgoing.len(), 1);

    // Verify metadata indicates fallback
    let link = &outgoing[0];
    assert_eq!(link.kind, "semantic");
    assert!(link.metadata.as_ref()
        .and_then(|m| m.get("fallback"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false));
}

#[tokio::test]
async fn test_fallback_disabled_creates_zero_links() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let isolated = create_test_note(&db, "Completely unique", Some(vec![1.0, 0.0, 0.0])).await;

    for i in 0..10 {
        create_test_note(
            &db,
            &format!("Unrelated {}", i),
            Some(vec![0.0, 1.0, i as f32 * 0.01]),
        ).await;
    }

    let config = GraphConfig {
        strategy: LinkingStrategy::MutualKNN,
        min_k: 5,
        max_k: 10,
        enable_fallback: false, // Disable fallback
    };

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(isolated));

    handler.execute(ctx).await.expect("Linking failed");

    let outgoing = db.links.get_outgoing(isolated).await.unwrap();

    // Should have zero links
    assert_eq!(outgoing.len(), 0);
}
```

#### 2.2.4 Backward Compatibility Tests

```rust
#[tokio::test]
async fn test_threshold_strategy_unchanged() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create notes with known similarity > 0.7 (default threshold)
    let note_a = create_test_note(&db, "Machine learning", Some(vec![1.0, 0.0, 0.0])).await;
    let note_b = create_test_note(&db, "Deep learning", Some(vec![0.85, 0.1, 0.0])).await;
    let note_c = create_test_note(&db, "Neural networks", Some(vec![0.8, 0.15, 0.0])).await;

    // Use threshold strategy (default)
    let config = GraphConfig::default(); // threshold mode

    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.expect("Linking failed");

    let outgoing = db.links.get_outgoing(note_a).await.unwrap();

    // Should create bidirectional links to all notes above threshold
    assert_eq!(outgoing.len(), 2); // B and C

    // Verify no mutual_knn metadata
    for link in &outgoing {
        assert!(link.metadata.as_ref()
            .and_then(|m| m.get("strategy"))
            .is_none());
    }
}

#[tokio::test]
async fn test_threshold_strategy_respects_code_threshold() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create a code-category document type
    let doc_type_id = db.document_types.create(
        "rust_code",
        "Rust",
        DocumentCategory::Code,
        ".rs",
        None,
    ).await.unwrap();

    // Create code note with document type
    let note_a = create_test_note_with_type(
        &db,
        "fn main() { println!(\"hello\"); }",
        Some(vec![1.0, 0.0, 0.0]),
        Some(doc_type_id),
    ).await;

    let note_b = create_test_note_with_type(
        &db,
        "fn test() { assert!(true); }",
        Some(vec![0.8, 0.1, 0.0]), // 0.8 similarity
        Some(doc_type_id),
    ).await;

    let config = GraphConfig::default();
    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.expect("Linking failed");

    let outgoing = db.links.get_outgoing(note_a).await.unwrap();

    // Should NOT link (0.8 < 0.85 code threshold)
    assert_eq!(outgoing.len(), 0);
}
```

#### 2.2.5 Topology Metrics Endpoint Tests

```rust
#[tokio::test]
async fn test_topology_stats_endpoint() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create known graph topology: 3-node triangle
    let note_a = create_test_note(&db, "A", Some(vec![1.0, 0.0, 0.0])).await;
    let note_b = create_test_note(&db, "B", Some(vec![0.9, 0.1, 0.0])).await;
    let note_c = create_test_note(&db, "C", Some(vec![0.85, 0.15, 0.0])).await;

    // Create reciprocal links (full triangle)
    db.links.create_reciprocal(note_a, note_b, "semantic", 0.9, None).await.unwrap();
    db.links.create_reciprocal(note_b, note_c, "semantic", 0.85, None).await.unwrap();
    db.links.create_reciprocal(note_a, note_c, "semantic", 0.8, None).await.unwrap();

    // Call topology stats endpoint
    let stats = db.links.get_topology_stats(None).await.unwrap();

    // Verify metrics
    assert_eq!(stats.node_count, 3);
    assert_eq!(stats.edge_count, 6); // 3 bidirectional = 6 directed
    assert_eq!(stats.avg_degree, 2.0); // Each node has 2 neighbors

    // Clustering coefficient = 1.0 (perfect triangle)
    assert!((stats.clustering_coefficient - 1.0).abs() < 0.01);

    // Degree distribution
    assert_eq!(stats.degree_distribution.len(), 1);
    assert_eq!(stats.degree_distribution[0].degree, 2);
    assert_eq!(stats.degree_distribution[0].count, 3);

    // Topology type
    assert_eq!(stats.topology_type, TopologyType::Mesh);
}

#[tokio::test]
async fn test_topology_stats_sampling_large_graph() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create large graph (>1000 nodes triggers sampling)
    for i in 0..1500 {
        create_test_note(
            &db,
            &format!("Note {}", i),
            Some(vec![i as f32 / 1500.0, 0.0, 0.0]),
        ).await;
    }

    // Call with sampling enabled
    let stats = db.links.get_topology_stats(Some(500)).await.unwrap();

    // Should return results based on sample
    assert!(stats.is_sampled);
    assert_eq!(stats.sample_size, Some(500));
    assert!(stats.node_count <= 1500);
}

#[tokio::test]
async fn test_topology_stats_empty_graph() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let stats = db.links.get_topology_stats(None).await.unwrap();

    assert_eq!(stats.node_count, 0);
    assert_eq!(stats.edge_count, 0);
    assert_eq!(stats.avg_degree, 0.0);
    assert_eq!(stats.clustering_coefficient, 0.0);
    assert!(stats.degree_distribution.is_empty());
}
```

### 2.3 Performance Tests

**Location**: `crates/matric-api/benches/linking_bench.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use tokio::runtime::Runtime;

fn benchmark_mutual_knn_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let pool = rt.block_on(setup_test_pool());
    let db = Database::new(pool);

    // Benchmark different corpus sizes
    let corpus_sizes = vec![10, 50, 100, 500, 1000];

    let mut group = c.benchmark_group("mutual_knn_latency");

    for size in corpus_sizes {
        // Setup: create corpus with embeddings
        let notes = rt.block_on(async {
            let mut ids = Vec::new();
            for i in 0..size {
                let embedding = vec![i as f32 / size as f32, 0.0, 0.0];
                let id = create_test_note(&db, &format!("Note {}", i), Some(embedding)).await;
                ids.push(id);
            }
            ids
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    let config = GraphConfig {
                        strategy: LinkingStrategy::MutualKNN,
                        min_k: 5,
                        max_k: 15,
                        enable_fallback: true,
                    };

                    let handler = LinkingHandler::new(db.clone(), config);
                    let ctx = JobContext::new(JobType::Linking, Some(notes[0]));

                    // Measure end-to-end latency
                    handler.execute(black_box(ctx)).await.expect("Linking failed")
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_mutual_knn_latency);
criterion_main!(benches);
```

---

## 3. Test Cases

### 3.1 Configuration Parsing Test Cases

| Test ID | Description | Input | Expected Output | Priority |
|---------|-------------|-------|----------------|----------|
| TC-001 | Default configuration | No env vars | `strategy=Threshold, min_k=5, max_k=15, fallback=true` | P0 |
| TC-002 | Parse valid threshold strategy | `GRAPH_LINKING_STRATEGY=threshold` | `strategy=Threshold` | P0 |
| TC-003 | Parse valid mutual_knn strategy | `GRAPH_LINKING_STRATEGY=mutual_knn` | `strategy=MutualKNN` | P0 |
| TC-004 | Invalid strategy defaults to threshold | `GRAPH_LINKING_STRATEGY=invalid` | `strategy=Threshold` | P1 |
| TC-005 | Parse valid min_k | `GRAPH_MIN_K=3` | `min_k=3` | P1 |
| TC-006 | Invalid min_k uses default | `GRAPH_MIN_K=abc` | `min_k=5` | P2 |
| TC-007 | Parse valid max_k | `GRAPH_MAX_K=20` | `max_k=20` | P1 |
| TC-008 | Invalid max_k uses default | `GRAPH_MAX_K=-5` | `max_k=15` | P2 |
| TC-009 | Parse fallback enabled | `GRAPH_ENABLE_FALLBACK=true` | `enable_fallback=true` | P1 |
| TC-010 | Parse fallback disabled | `GRAPH_ENABLE_FALLBACK=false` | `enable_fallback=false` | P1 |

### 3.2 Adaptive k Computation Test Cases

| Test ID | Description | Corpus Size (N) | Expected k | Priority |
|---------|-------------|-----------------|------------|----------|
| TC-011 | Zero corpus | 0 | 5 (min_k) | P0 |
| TC-012 | Single note | 1 | 5 (min_k) | P0 |
| TC-013 | Small corpus | 10 | 5 (ceil(log2(10))=4, clamp to min) | P1 |
| TC-014 | Medium corpus | 256 | 8 (ceil(log2(256))=8) | P1 |
| TC-015 | Large corpus | 100,000 | 15 (ceil(log2(100k))=17, clamp to max) | P1 |
| TC-016 | Boundary at min | 32 | 5 (ceil(log2(32))=5) | P2 |
| TC-017 | Boundary at max | 32,768 | 15 (ceil(log2(32k))=15) | P2 |

### 3.3 Mutual k-NN Algorithm Test Cases

| Test ID | Description | Setup | Expected Behavior | Priority |
|---------|-------------|-------|------------------|----------|
| TC-018 | Creates mutual links | 3 notes in tight cluster | A↔B, A↔C, B↔C mutual links created | P0 |
| TC-019 | Rejects non-mutual | A→B, B→C, A⇢C asymmetric | Only A↔B created, not A→C | P0 |
| TC-020 | Bounded degree (k=5) | 20 similar notes | Each note has ≤5 links | P0 |
| TC-021 | Bidirectional links | A→B mutual | Creates both A→B and B→A | P1 |
| TC-022 | Metadata includes strategy | Mutual k-NN link | `metadata.strategy = "mutual_knn"` | P1 |
| TC-023 | Excludes self-links | Note with embedding | No A→A links | P1 |
| TC-024 | Archived notes excluded | 1 active, 1 archived | Only links to active notes | P2 |

### 3.4 Isolated Node Fallback Test Cases

| Test ID | Description | Setup | Expected Behavior | Priority |
|---------|-------------|-------|------------------|----------|
| TC-025 | Fallback creates single link | Isolated note, fallback=true | 1 link to best match | P0 |
| TC-026 | Fallback metadata set | Fallback link created | `metadata.fallback = true` | P1 |
| TC-027 | Fallback disabled returns empty | Isolated note, fallback=false | 0 links created | P1 |
| TC-028 | Fallback not triggered for normal | 5 mutual neighbors | Normal mutual k-NN, no fallback | P2 |

### 3.5 Backward Compatibility Test Cases

| Test ID | Description | Setup | Expected Behavior | Priority |
|---------|-------------|-------|------------------|----------|
| TC-029 | Threshold strategy unchanged | `strategy=Threshold` | Same behavior as v2026.1.x | P0 |
| TC-030 | Code threshold still applies | Code doc type, threshold mode | Uses 0.85 threshold | P0 |
| TC-031 | No mutual_knn metadata in threshold | Threshold link | `metadata.strategy` absent | P1 |
| TC-032 | Wiki links unaffected | `[[wiki-link]]` in content | Still creates explicit links | P1 |

### 3.6 Topology Metrics Endpoint Test Cases

| Test ID | Description | Setup | Expected Output | Priority |
|---------|-------------|-------|----------------|----------|
| TC-033 | Triangle graph metrics | 3-node triangle | `clustering_coeff=1.0, avg_degree=2.0` | P0 |
| TC-034 | Star graph metrics | 1 hub + 5 leaves | `topology_type=Star` | P1 |
| TC-035 | Empty graph | No notes | `node_count=0, edge_count=0` | P1 |
| TC-036 | Large graph sampling | 2000 nodes | `is_sampled=true, sample_size=500` | P1 |
| TC-037 | Degree distribution | Mixed degrees | Correct histogram counts | P2 |
| TC-038 | Error handling (invalid limit) | `sample_size=-1` | Returns 400 error | P2 |

### 3.7 Performance Test Cases

| Test ID | Description | Setup | Requirement | Priority |
|---------|-------------|-------|-------------|----------|
| TC-039 | Small corpus latency | 10 notes | ≤50ms | P0 |
| TC-040 | Medium corpus latency | 100 notes | ≤150ms | P0 |
| TC-041 | Large corpus latency | 1000 notes | ≤200ms | P0 |
| TC-042 | Very large corpus | 10,000 notes | ≤500ms | P1 |

---

## 4. Test Data Strategy

### 4.1 Synthetic Embedding Generation

**Strategy**: Generate embeddings with controlled cosine similarity to test linking behavior.

```rust
/// Create embedding with known similarity to reference vector
fn create_similar_embedding(reference: &[f32], similarity: f32) -> Vec<f32> {
    // Use angle-based generation for exact cosine similarity
    let theta = similarity.acos(); // angle for desired similarity

    let mut result = reference.to_vec();
    result[0] *= similarity;
    result[1] = (1.0 - similarity * similarity).sqrt();

    // Normalize to unit vector
    let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
    result.iter_mut().for_each(|x| *x /= norm);

    result
}

#[test]
fn test_embedding_similarity_generation() {
    let reference = vec![1.0, 0.0, 0.0];
    let similar = create_similar_embedding(&reference, 0.9);

    let cosine_sim = reference.iter()
        .zip(similar.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>();

    assert!((cosine_sim - 0.9).abs() < 0.01, "Expected ~0.9, got {}", cosine_sim);
}
```

### 4.2 Graph Topology Templates

**Predefined topologies for testing:**

```rust
enum GraphTemplate {
    Triangle,      // 3 nodes, fully connected
    Star,          // 1 hub + N leaves
    Chain,         // Linear sequence A→B→C→D
    ScaleFree,     // Power-law degree distribution
    Mesh,          // All nodes highly connected
    Sparse,        // Many isolated nodes
}

async fn create_graph_from_template(
    db: &Database,
    template: GraphTemplate,
    size: usize,
) -> Vec<Uuid> {
    match template {
        GraphTemplate::Triangle => {
            // 3 nodes with pairwise similarity > 0.9
            vec![
                create_test_note(db, "A", Some(vec![1.0, 0.0, 0.0])).await,
                create_test_note(db, "B", Some(vec![0.95, 0.1, 0.0])).await,
                create_test_note(db, "C", Some(vec![0.9, 0.15, 0.0])).await,
            ]
        },
        GraphTemplate::Star => {
            let mut nodes = Vec::new();

            // Hub at origin
            nodes.push(create_test_note(db, "Hub", Some(vec![1.0, 0.0, 0.0])).await);

            // Leaves around circle
            for i in 0..size {
                let angle = 2.0 * std::f32::consts::PI * (i as f32) / (size as f32);
                nodes.push(create_test_note(
                    db,
                    &format!("Leaf {}", i),
                    Some(vec![angle.cos(), angle.sin(), 0.0]),
                ).await);
            }

            nodes
        },
        // ... other templates
    }
}
```

### 4.3 Real-World Corpus Simulation

**Strategy**: Import small representative corpus from existing notes.

```rust
/// Load embeddings from real notes for integration testing
async fn load_real_corpus_sample(db: &Database, count: usize) -> Vec<Uuid> {
    let notes = db.notes.list(count as i64, 0).await.unwrap();

    notes.into_iter()
        .take(count)
        .map(|n| n.id)
        .collect()
}
```

---

## 5. Coverage Requirements

### 5.1 Code Coverage Targets

| Component | Line Coverage | Branch Coverage | Priority |
|-----------|---------------|-----------------|----------|
| `GraphConfig` | >95% | >90% | P0 |
| `LinkingHandler::execute_mutual_knn` | >90% | >85% | P0 |
| `LinkingHandler::execute_threshold` | >80% (regression) | >75% | P1 |
| `compute_adaptive_k` | 100% | 100% | P0 |
| Topology stats API | >85% | >80% | P1 |

### 5.2 Coverage Measurement

```bash
# Generate coverage report
cargo llvm-cov --workspace --html

# Enforce minimum thresholds
cargo llvm-cov --workspace --fail-under-lines 90
```

### 5.3 Uncovered Code Policy

**Acceptable exclusions:**
- Error logging statements (tracing macros)
- Defensive assertions that should never execute
- Platform-specific dead code

**Not acceptable:**
- Core algorithm paths
- Configuration parsing
- User-facing API endpoints

---

## 6. Test Environment

### 6.1 Database Setup

**Test PostgreSQL Instance:**
- Image: `matric-testdb:local` (`build/Dockerfile.testdb`)
- User: `matric` (superuser)
- Database: `matric_test`
- Extensions: pgvector, PostGIS, pg_trgm

**Setup Script:**

```bash
#!/bin/bash
# scripts/setup-test-db.sh

docker build -f build/Dockerfile.testdb -t matric-testdb:local .

docker run -d \
  --name matric-test-db \
  -e POSTGRES_USER=matric \
  -e POSTGRES_PASSWORD=matric \
  -e POSTGRES_DB=matric_test \
  -p 5432:5432 \
  matric-testdb:local

# Wait for startup
sleep 5

# Run migrations
export DATABASE_URL=postgres://matric:matric@localhost/matric_test
cargo sqlx migrate run
```

### 6.2 Test Isolation

**Per-test isolation strategies:**

1. **UUID-based namespacing**: Use unique UUIDs for note IDs
2. **Separate test DB**: Run tests against `matric_test` database
3. **Parallel execution**: Tests are safe to run with `cargo test --workspace`
4. **No global state**: No `std::env::set_var` usage

### 6.3 CI/CD Integration

**Gitea Actions Workflow (`test.yml`):**

```yaml
- name: Run unit tests
  run: cargo test --workspace --lib

- name: Run integration tests
  run: cargo test --workspace --test '*'
  env:
    DATABASE_URL: postgres://matric:matric@localhost:5432/matric_test

- name: Generate coverage report
  run: cargo llvm-cov --workspace --lcov --output-path lcov.info

- name: Enforce coverage threshold
  run: cargo llvm-cov --workspace --fail-under-lines 90
```

---

## 7. Regression Tests

### 7.1 Regression Test Suite

**Purpose**: Ensure existing linking behavior unchanged when `strategy=Threshold`

**Test Cases:**

```rust
#[tokio::test]
async fn regression_threshold_bidirectional_links() {
    // Verify threshold mode still creates bidirectional links
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let note_a = create_test_note(&db, "AI ethics", Some(vec![1.0, 0.0, 0.0])).await;
    let note_b = create_test_note(&db, "Machine ethics", Some(vec![0.85, 0.1, 0.0])).await;

    let config = GraphConfig::default(); // threshold mode
    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.unwrap();

    // Verify bidirectional
    let a_to_b = db.links.get_outgoing(note_a).await.unwrap();
    let b_to_a = db.links.get_incoming(note_a).await.unwrap();

    assert_eq!(a_to_b.len(), 1);
    assert_eq!(b_to_a.len(), 1);
}

#[tokio::test]
async fn regression_wiki_links_still_work() {
    // Verify [[wiki-links]] unaffected
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let target = create_test_note(&db, "Target Page", None).await;
    let source = create_test_note(&db, "See [[Target Page]]", None).await;

    let config = GraphConfig::default();
    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(source));

    handler.execute(ctx).await.unwrap();

    let links = db.links.get_outgoing(source).await.unwrap();

    // Should have 1 wiki link
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, "wiki");
    assert_eq!(links[0].to_note_id, target);
}

#[tokio::test]
async fn regression_no_self_links() {
    // Verify self-links still prevented
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let note_a = create_test_note(&db, "Content", Some(vec![1.0, 0.0, 0.0])).await;

    let config = GraphConfig::default();
    let handler = LinkingHandler::new(db.clone(), config);
    let ctx = JobContext::new(JobType::Linking, Some(note_a));

    handler.execute(ctx).await.unwrap();

    let links = db.links.get_outgoing(note_a).await.unwrap();

    // Should have no links to self
    assert!(links.iter().all(|l| l.to_note_id != note_a));
}
```

### 7.2 Regression Test Baseline

**Establish baseline metrics before changes:**

```bash
# Run existing linking tests to capture baseline
cargo test --workspace linking -- --nocapture > baseline_linking.log

# Measure current graph metrics
curl http://localhost:3000/api/v1/graph/topology/stats > baseline_topology.json
```

**Comparison after implementation:**

```bash
# Re-run tests
cargo test --workspace linking -- --nocapture > new_linking.log

# Compare results
diff baseline_linking.log new_linking.log
```

---

## 8. Test Execution Plan

### 8.1 Development Phase Testing

**Iteration 1: Core Algorithm (Week 1)**
- [ ] TC-001 to TC-010: Config parsing
- [ ] TC-011 to TC-017: Adaptive k computation
- [ ] TC-018 to TC-024: Mutual k-NN core algorithm

**Iteration 2: Edge Cases (Week 2)**
- [ ] TC-025 to TC-028: Isolated node fallback
- [ ] TC-029 to TC-032: Backward compatibility
- [ ] TC-039 to TC-042: Performance benchmarks

**Iteration 3: API Integration (Week 3)**
- [ ] TC-033 to TC-038: Topology metrics endpoint
- [ ] Regression test suite
- [ ] Coverage validation (>90%)

### 8.2 Pre-Merge Checklist

- [ ] All P0 test cases passing
- [ ] All P1 test cases passing
- [ ] Code coverage >90% on new code
- [ ] No regression in existing tests
- [ ] Performance benchmarks meet latency requirements
- [ ] CI/CD pipeline green

### 8.3 Post-Merge Validation

- [ ] Deploy to staging environment
- [ ] Run topology stats on production-like corpus
- [ ] Verify clustering coefficient improvement
- [ ] Monitor job queue latency
- [ ] A/B test: threshold vs mutual k-NN on sample users

---

## 9. Test Metrics and Reporting

### 9.1 Success Metrics

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Test pass rate | 100% | CI/CD pipeline |
| Code coverage | >90% | cargo-llvm-cov |
| Performance (p50) | ≤150ms | Benchmark suite |
| Performance (p95) | ≤200ms | Benchmark suite |
| Regression count | 0 | Manual verification |

### 9.2 Defect Tracking

**Severity Levels:**
- **Critical**: Core algorithm failure, data loss
- **High**: Wrong links created, performance regression
- **Medium**: Edge case handling, error messages
- **Low**: Documentation, logging

**Triage Process:**
1. Log defect in issue tracker with test case reproduction
2. Assign severity based on impact
3. Fix critical/high before merge
4. Track medium/low for future iterations

### 9.3 Test Report Template

```markdown
# Test Execution Report: Issue #386

**Date**: YYYY-MM-DD
**Tester**: [Name]
**Build**: [Git commit SHA]

## Summary
- Total test cases: X
- Passed: Y
- Failed: Z
- Blocked: W

## Coverage
- Line coverage: XX%
- Branch coverage: YY%

## Performance
- p50 latency: XXms
- p95 latency: YYms

## Defects Found
| ID | Severity | Description | Status |
|----|----------|-------------|--------|
| ... | ... | ... | ... |

## Recommendations
[Pass/Fail/Conditional Pass]
```

---

## 10. Appendix

### 10.1 Test Data Fixtures

**Location**: `crates/matric-api/tests/fixtures/`

- `triangle_graph.json` - 3-node fully connected graph
- `star_graph.json` - Hub-and-spoke topology
- `scale_free_graph.json` - Power-law degree distribution
- `embeddings_similar.json` - High-similarity embedding vectors

### 10.2 Helper Functions Reference

```rust
// Database setup
async fn setup_test_pool() -> Pool<Postgres>
async fn create_test_note(db: &Database, content: &str, embedding: Option<Vec<f32>>) -> Uuid
async fn create_test_note_with_type(db: &Database, content: &str, embedding: Option<Vec<f32>>, doc_type_id: Option<Uuid>) -> Uuid

// Embedding generation
fn create_similar_embedding(reference: &[f32], similarity: f32) -> Vec<f32>
fn random_unit_vector(dim: usize) -> Vec<f32>

// Graph templates
async fn create_graph_from_template(db: &Database, template: GraphTemplate, size: usize) -> Vec<Uuid>

// Assertions
fn assert_mutual_link_exists(db: &Database, note_a: Uuid, note_b: Uuid)
fn assert_topology_metrics(stats: &TopologyStats, expected_type: TopologyType, expected_avg_degree: f32)
```

### 10.3 Configuration Examples

**Threshold mode (default):**
```bash
# No configuration needed - defaults to threshold
```

**Mutual k-NN mode:**
```bash
export GRAPH_LINKING_STRATEGY=mutual_knn
export GRAPH_MIN_K=5
export GRAPH_MAX_K=15
export GRAPH_ENABLE_FALLBACK=true
```

**Testing mode (aggressive linking):**
```bash
export GRAPH_LINKING_STRATEGY=mutual_knn
export GRAPH_MIN_K=2
export GRAPH_MAX_K=10
export GRAPH_ENABLE_FALLBACK=true
```

### 10.4 Related Documents

- `docs/sdlc/386-graph-topology/REQUIREMENTS.md` - Functional requirements
- `docs/sdlc/386-graph-topology/DESIGN.md` - Technical design
- `docs/testing-guide.md` - General testing guidelines
- `CLAUDE.md` - Project setup and conventions

---

**Document Status**: DRAFT
**Next Review**: Pre-implementation (before Issue #386 work begins)
**Approval**: [ ] Test Architect [ ] Software Implementer [ ] Deployment Manager
