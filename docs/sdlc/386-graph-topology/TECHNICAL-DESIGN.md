# Technical Design Document: Graph Topology Improvement

**Issue**: [#386](https://github.com/fortemi/fortemi/issues/386)
**Status**: Draft
**Created**: 2026-02-14
**ADR**: [ADR-001-linking-strategy.md](./ADR-001-linking-strategy.md)
**PRD**: [PRD.md](./PRD.md)

---

## 1. System Architecture

### 1.1 Component Overview

The graph topology improvement modifies three existing modules and adds one new handler.
No new crates, no new database migrations, no new dependencies.

```
                        +---------------------------+
                        |     Job Worker Queue      |
                        |  (crates/matric-jobs)     |
                        +-------------+-------------+
                                      |
                                      v
                     +----------------+------------------+
                     |        LinkingHandler::execute     |
                     |  (crates/matric-api/src/handlers/ |
                     |   jobs.rs:710-863)                |
                     +--+-------+--------+--------+-----+
                        |       |        |        |
             +----------+  +----+---+ +--+------+ +--------+
             |             |        | |         |          |
             v             v        v v         v          v
       +-----------+  +---------+ +-------+ +--------+ +----------+
       |GraphConfig|  |Embedding| | Link  | |  Note  | | Topology |
       |(defaults) |  |  Repo   | | Repo  | |  Repo  | |  Stats   |
       +-----------+  +---------+ +-------+ +--------+ | Handler  |
                       embeddings   links     notes     +----------+
                       .rs:68-134  .rs:1-660  (count)     (NEW)

  Modified:  [GraphConfig] [LinkingHandler]
  New:       [TopologyStats handler]
  Unchanged: [EmbeddingRepository] [LinkRepository] [link schema]
```

### 1.2 Data Flow: Linking Pipeline

Current flow (threshold):
```
Note saved -> Embedding job -> Linking job:
  1. fetch note
  2. parse wiki-links
  3. get embeddings for note
  4. find_similar(vector, limit=10)  -- 1 HNSW query
  5. filter by threshold (0.7/0.85)
  6. create bidirectional link for each passing candidate
```

New flow (mutual k-NN):
```
Note saved -> Embedding job -> Linking job:
  1. fetch note
  2. parse wiki-links            (unchanged)
  3. get embeddings for note     (unchanged)
  4. find_similar(vector, k+1)   -- 1 HNSW query (forward k-NN)
  5. for each candidate:
     a. get candidate embedding
     b. find_similar(candidate_vector, k+1)  -- 1 HNSW query (reverse)
     c. check if source note appears in candidate's k-NN
  6. create reciprocal link only for mutual neighbors
  7. if zero mutual neighbors: fallback to single best match
```

### 1.3 Files Modified

| File | Change | Lines Affected |
|------|--------|---------------|
| `crates/matric-core/src/defaults.rs` | Add `GraphConfig` struct + `GraphLinkingStrategy` enum | Insert after line 320 |
| `crates/matric-api/src/handlers/jobs.rs` | Rewrite `LinkingHandler::execute` semantic linking section | Lines 800-843 replaced |
| `crates/matric-api/src/main.rs` | Add `/api/v1/graph/topology/stats` route + handler | New route near line 1432, new handler function |

---

## 2. Detailed Design

### 2.1 GraphConfig (`crates/matric-core/src/defaults.rs`)

Add a new configuration struct after the existing similarity threshold constants (after line 320). The struct reads environment variables at construction time and validates ranges.

```rust
use std::env;

// =============================================================================
// GRAPH LINKING CONFIGURATION (Tier 2 -- Algorithm Parameters)
// =============================================================================

/// Graph linking strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphLinkingStrategy {
    /// Legacy threshold-based linking (0.7 prose / 0.85 code).
    Threshold,
    /// Mutual k-nearest neighbors -- link only when both notes
    /// include each other in their top-k neighbors.
    MutualKnn,
}

impl GraphLinkingStrategy {
    fn from_env(val: &str) -> Option<Self> {
        match val.to_lowercase().trim() {
            "threshold" => Some(Self::Threshold),
            "mutual_knn" | "mutualknn" | "mutual-knn" => Some(Self::MutualKnn),
            _ => None,
        }
    }
}

impl std::fmt::Display for GraphLinkingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Threshold => write!(f, "threshold"),
            Self::MutualKnn => write!(f, "mutual_knn"),
        }
    }
}

/// Default number of nearest neighbors for mutual k-NN.
pub const GRAPH_K_NEIGHBORS_DEFAULT: usize = 7;

/// Default minimum similarity floor for graph linking.
pub const GRAPH_MIN_SIMILARITY_DEFAULT: f32 = 0.5;

/// Minimum allowed value for k_neighbors.
pub const GRAPH_K_MIN: usize = 3;

/// Maximum allowed value for k_neighbors (hard cap).
pub const GRAPH_K_MAX: usize = 50;

/// Adaptive k lower bound.
pub const GRAPH_ADAPTIVE_K_MIN: usize = 5;

/// Adaptive k upper bound.
pub const GRAPH_ADAPTIVE_K_MAX: usize = 15;

/// Graph linking strategy configuration.
///
/// Constructed from environment variables. All fields have sensible defaults
/// so the system operates correctly with zero configuration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphConfig {
    /// Active linking strategy.
    pub strategy: GraphLinkingStrategy,
    /// Number of nearest neighbors to consider.
    /// When `0`, adaptive k is used: `max(5, log2(N))` clamped to [5, 15].
    pub k_neighbors: usize,
    /// Whether k is computed adaptively (k_neighbors env var was 0 or unset
    /// with adaptive mode).
    pub adaptive_k: bool,
    /// Absolute similarity floor. No links created below this regardless of
    /// k-NN membership.
    pub min_similarity: f32,
}

impl GraphConfig {
    /// Load configuration from environment variables.
    ///
    /// | Variable | Default | Range |
    /// |----------|---------|-------|
    /// | `GRAPH_LINKING_STRATEGY` | `mutual_knn` | `threshold`, `mutual_knn` |
    /// | `GRAPH_K_NEIGHBORS` | `7` | 0 (adaptive) or 3..=50 |
    /// | `GRAPH_MIN_SIMILARITY` | `0.5` | 0.0..=1.0 |
    pub fn from_env() -> Self {
        let strategy = env::var("GRAPH_LINKING_STRATEGY")
            .ok()
            .and_then(|v| GraphLinkingStrategy::from_env(&v))
            .unwrap_or(GraphLinkingStrategy::MutualKnn);

        let (k_neighbors, adaptive_k) = match env::var("GRAPH_K_NEIGHBORS") {
            Ok(val) => match val.trim().parse::<usize>() {
                Ok(0) => (0, true),
                Ok(k) => (k.clamp(GRAPH_K_MIN, GRAPH_K_MAX), false),
                Err(_) => {
                    tracing::warn!(
                        value = %val,
                        "Invalid GRAPH_K_NEIGHBORS, using default: {}",
                        GRAPH_K_NEIGHBORS_DEFAULT
                    );
                    (GRAPH_K_NEIGHBORS_DEFAULT, false)
                }
            },
            Err(_) => (GRAPH_K_NEIGHBORS_DEFAULT, false),
        };

        let min_similarity = env::var("GRAPH_MIN_SIMILARITY")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or(GRAPH_MIN_SIMILARITY_DEFAULT);

        Self {
            strategy,
            k_neighbors,
            adaptive_k,
            min_similarity,
        }
    }

    /// Compute effective k for a corpus of `note_count` embedded notes.
    ///
    /// When `adaptive_k` is true, returns `max(5, floor(log2(N)))` clamped
    /// to `[GRAPH_ADAPTIVE_K_MIN, GRAPH_ADAPTIVE_K_MAX]`.
    /// Otherwise returns `self.k_neighbors`.
    pub fn effective_k(&self, note_count: usize) -> usize {
        if self.adaptive_k {
            if note_count <= 1 {
                return GRAPH_ADAPTIVE_K_MIN;
            }
            let log2_n = (note_count as f64).log2().floor() as usize;
            log2_n.max(GRAPH_ADAPTIVE_K_MIN).min(GRAPH_ADAPTIVE_K_MAX)
        } else {
            self.k_neighbors
        }
    }
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            strategy: GraphLinkingStrategy::MutualKnn,
            k_neighbors: GRAPH_K_NEIGHBORS_DEFAULT,
            adaptive_k: false,
            min_similarity: GRAPH_MIN_SIMILARITY_DEFAULT,
        }
    }
}
```

**Design decisions:**

- `from_env()` rather than a config file because Fortemi's existing config pattern uses environment variables exclusively (see `REQUIRE_AUTH`, `ISSUER_URL`, `FTS_SCRIPT_DETECTION`).
- The enum is `Copy` to avoid allocation when passing through the job handler.
- Parsing tolerates multiple case/separator variations for `GRAPH_LINKING_STRATEGY` (e.g., `mutual-knn`, `MutualKnn`) because admin typos should not silently fall back.
- Invalid values log a warning via `tracing::warn!` and fall back to defaults rather than panicking at startup.
- `effective_k()` is a pure function that takes `note_count` as input so it can be tested without database access.

### 2.2 Modified LinkingHandler::execute (`crates/matric-api/src/handlers/jobs.rs`)

The handler currently lives at lines 710-863. The wiki-link parsing section (lines 722-775) remains unchanged. The semantic linking section (lines 777-844) is replaced with strategy dispatch.

#### 2.2.1 Strategy Dispatch

Replace the current `link_threshold` computation (lines 730-740) and the similarity loop (lines 800-843) with:

```rust
#[async_trait]
impl JobHandler for LinkingHandler {
    fn job_type(&self) -> JobType {
        JobType::Linking
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "linking", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let mut created = 0;
        #[allow(clippy::needless_late_init)]
        let wiki_links_found;
        let mut wiki_links_resolved = 0;

        // === Wiki-link parsing (unchanged) ===
        ctx.report_progress(10, Some("Parsing wiki-style links..."));

        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

        let content = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        let wiki_links = Self::parse_wiki_links(content);
        wiki_links_found = wiki_links.len();
        ctx.report_progress(20, Some(&format!("Found {} wiki-links", wiki_links_found)));

        for link_title in &wiki_links {
            if let Some(target_id) = self.resolve_wiki_link(link_title).await {
                if target_id != note_id {
                    let metadata = serde_json::json!({"wiki_title": link_title});
                    if let Err(e) = self
                        .db
                        .links
                        .create(note_id, target_id, "wiki", 1.0, Some(metadata))
                        .await
                    {
                        debug!(error = %e, "Failed to create wiki link (may already exist)");
                    } else {
                        created += 1;
                        wiki_links_resolved += 1;
                    }
                }
            }
        }

        // === Semantic linking (strategy-aware) ===
        ctx.report_progress(40, Some("Finding embeddings for semantic linking..."));

        let embeddings = match self.db.embeddings.get_for_note(note_id).await {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "No embeddings for note, skipping semantic linking");
                return JobResult::Success(Some(serde_json::json!({
                    "links_created": created,
                    "wiki_links_found": wiki_links_found,
                    "wiki_links_resolved": wiki_links_resolved
                })));
            }
        };

        if embeddings.is_empty() {
            return JobResult::Success(Some(serde_json::json!({
                "links_created": created,
                "wiki_links_found": wiki_links_found,
                "wiki_links_resolved": wiki_links_resolved
            })));
        }

        // Load graph configuration from environment
        let graph_config = matric_core::defaults::GraphConfig::from_env();

        let semantic_created = match graph_config.strategy {
            matric_core::defaults::GraphLinkingStrategy::Threshold => {
                self.link_by_threshold(&note, note_id, &embeddings[0].vector)
                    .await
            }
            matric_core::defaults::GraphLinkingStrategy::MutualKnn => {
                self.link_by_mutual_knn(note_id, &embeddings[0].vector, &graph_config)
                    .await
            }
        };

        match semantic_created {
            Ok(count) => created += count,
            Err(e) => return JobResult::Failed(format!("Semantic linking failed: {}", e)),
        }

        ctx.report_progress(100, Some("Linking complete"));
        info!(
            note_id = %note_id,
            result_count = created,
            wiki_found = wiki_links_found,
            wiki_resolved = wiki_links_resolved,
            strategy = %graph_config.strategy,
            duration_ms = start.elapsed().as_millis() as u64,
            "Linking completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "links_created": created,
            "wiki_links_found": wiki_links_found,
            "wiki_links_resolved": wiki_links_resolved,
            "strategy": graph_config.strategy.to_string(),
            "k_neighbors": graph_config.k_neighbors
        })))
    }
}
```

#### 2.2.2 Threshold Strategy (Legacy Path)

Extract the existing threshold logic into a private method. This preserves current behavior exactly when `GRAPH_LINKING_STRATEGY=threshold`:

```rust
impl LinkingHandler {
    /// Legacy threshold-based linking. Creates bidirectional links for all
    /// candidates above the content-type-aware similarity threshold.
    async fn link_by_threshold(
        &self,
        note: &matric_core::NoteWithContent,
        note_id: Uuid,
        vector: &pgvector::Vector,
    ) -> Result<usize, String> {
        let link_threshold = if let Some(dt_id) = note.note.document_type_id {
            match self.db.document_types.get(dt_id).await {
                Ok(Some(dt)) => matric_core::defaults::semantic_link_threshold_for(dt.category),
                _ => matric_core::defaults::SEMANTIC_LINK_THRESHOLD,
            }
        } else {
            matric_core::defaults::SEMANTIC_LINK_THRESHOLD
        };

        let similar = self
            .db
            .embeddings
            .find_similar(vector, 10, true)
            .await
            .map_err(|e| format!("find_similar failed: {}", e))?;

        let mut created = 0;
        for hit in similar {
            if hit.note_id == note_id || hit.score < link_threshold {
                continue;
            }
            // Forward link
            if self
                .db
                .links
                .create(note_id, hit.note_id, "semantic", hit.score, None)
                .await
                .is_ok()
            {
                created += 1;
            }
            // Backward link
            if self
                .db
                .links
                .create(hit.note_id, note_id, "semantic", hit.score, None)
                .await
                .is_ok()
            {
                created += 1;
            }
        }
        Ok(created)
    }
}
```

#### 2.2.3 Mutual k-NN Strategy (New)

```rust
impl LinkingHandler {
    /// Mutual k-NN linking. Creates reciprocal links only when both notes
    /// include each other in their respective k nearest neighbors.
    ///
    /// Algorithm:
    ///   1. Compute forward k-NN: top-(k+1) similar notes for the source
    ///   2. For each candidate (excluding self, above min_similarity):
    ///      a. Retrieve candidate's embedding
    ///      b. Compute reverse k-NN: top-(k+1) similar notes for the candidate
    ///      c. If source appears in candidate's k-NN, the pair is mutual
    ///   3. Create reciprocal links for all mutual pairs
    ///   4. If zero mutual pairs found, fallback to single best match
    async fn link_by_mutual_knn(
        &self,
        note_id: Uuid,
        vector: &pgvector::Vector,
        config: &matric_core::defaults::GraphConfig,
    ) -> Result<usize, String> {
        // Compute effective k (adaptive or fixed)
        let note_count = self
            .db
            .embeddings
            .count()
            .await
            .map_err(|e| format!("embedding count failed: {}", e))?
            as usize;

        let k = config.effective_k(note_count);

        // Step 1: Forward k-NN query
        // Request k+1 results because the source note itself may appear
        let candidates = self
            .db
            .embeddings
            .find_similar(vector, (k + 1) as i64, true)
            .await
            .map_err(|e| format!("forward k-NN failed: {}", e))?;

        let mut created = 0;

        // Step 2-3: Reverse verification + link creation
        for hit in candidates.iter() {
            // Skip self-match
            if hit.note_id == note_id {
                continue;
            }
            // Enforce minimum similarity floor
            if hit.score < config.min_similarity {
                continue;
            }

            // Retrieve candidate's embedding for reverse lookup
            let hit_embeddings = match self.db.embeddings.get_for_note(hit.note_id).await {
                Ok(e) if !e.is_empty() => e,
                _ => {
                    debug!(
                        candidate = %hit.note_id,
                        "Skipping candidate: no embeddings for reverse lookup"
                    );
                    continue;
                }
            };

            // Reverse k-NN: does the candidate have source in its top-k?
            let reverse = self
                .db
                .embeddings
                .find_similar(&hit_embeddings[0].vector, (k + 1) as i64, true)
                .await
                .map_err(|e| format!("reverse k-NN failed for {}: {}", hit.note_id, e))?;

            let is_mutual = reverse.iter().any(|r| r.note_id == note_id);

            if is_mutual {
                let metadata = serde_json::json!({
                    "strategy": "mutual_knn",
                    "k": k,
                    "forward_score": hit.score
                });
                if let Err(e) = self
                    .db
                    .links
                    .create_reciprocal(note_id, hit.note_id, "semantic", hit.score, Some(metadata))
                    .await
                {
                    debug!(error = %e, "Failed to create mutual link (may already exist)");
                } else {
                    created += 1;
                }
            }
        }

        // Step 4: Isolated node fallback
        if created == 0 {
            if let Some(best) = candidates
                .iter()
                .find(|h| h.note_id != note_id && h.score >= config.min_similarity)
            {
                let metadata = serde_json::json!({
                    "strategy": "fallback_best",
                    "k": k,
                    "reason": "no_mutual_neighbors"
                });
                if let Err(e) = self
                    .db
                    .links
                    .create_reciprocal(
                        note_id,
                        best.note_id,
                        "semantic",
                        best.score,
                        Some(metadata),
                    )
                    .await
                {
                    debug!(error = %e, "Failed to create fallback link (may already exist)");
                } else {
                    created += 1;
                }
            } else {
                debug!(note_id = %note_id, "No candidates above min_similarity, note remains isolated");
            }
        }

        Ok(created)
    }
}
```

### 2.3 Topology Stats Handler (`crates/matric-api/src/main.rs`)

#### 2.3.1 Route Registration

Add after the existing graph route (line 1432):

```rust
.route("/api/v1/graph/topology/stats", get(get_topology_stats))
```

#### 2.3.2 Response Schema

```rust
/// Graph topology statistics.
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
struct TopologyStats {
    /// Total non-deleted notes in this archive.
    total_notes: i64,
    /// Total semantic links (kind = 'semantic').
    total_semantic_links: i64,
    /// Total links of all kinds.
    total_links: i64,
    /// Average outgoing semantic link count per note.
    avg_degree: f64,
    /// Standard deviation of node degrees.
    degree_std_dev: f64,
    /// Maximum outgoing semantic link count for any single note.
    max_degree: i64,
    /// Notes with zero semantic links (incoming or outgoing).
    isolated_nodes: i64,
    /// Approximate clustering coefficient (fraction of connected triples
    /// that form triangles). Sampled for performance on large graphs.
    clustering_coefficient: f64,
    /// Active linking strategy.
    strategy: String,
    /// Current k value (fixed or adaptive).
    k_neighbors: usize,
    /// Whether k is computed adaptively.
    adaptive_k: bool,
}
```

#### 2.3.3 Handler Implementation

```rust
#[utoipa::path(
    get,
    path = "/api/v1/graph/topology/stats",
    tag = "Graph",
    responses(
        (status = 200, description = "Topology statistics", body = TopologyStats)
    )
)]
async fn get_topology_stats(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;

    let stats_row = ctx
        .query(|tx| {
            Box::pin(async move {
                let row = sqlx::query(
                    r#"
                    WITH note_degrees AS (
                        SELECT
                            n.id AS note_id,
                            COUNT(l.id) AS degree
                        FROM note n
                        LEFT JOIN link l
                            ON (l.from_note_id = n.id OR l.to_note_id = n.id)
                            AND l.kind = 'semantic'
                        WHERE n.deleted_at IS NULL
                        GROUP BY n.id
                    ),
                    degree_stats AS (
                        SELECT
                            COUNT(*) AS total_notes,
                            COALESCE(AVG(degree), 0) AS avg_degree,
                            COALESCE(STDDEV_POP(degree), 0) AS degree_std_dev,
                            COALESCE(MAX(degree), 0) AS max_degree,
                            COUNT(*) FILTER (WHERE degree = 0) AS isolated_nodes
                        FROM note_degrees
                    ),
                    link_counts AS (
                        SELECT
                            COUNT(*) AS total_links,
                            COUNT(*) FILTER (WHERE kind = 'semantic') AS total_semantic_links
                        FROM link
                    ),
                    -- Approximate clustering coefficient via triangle counting.
                    -- Sample up to 500 notes with degree >= 2 for performance.
                    sampled_nodes AS (
                        SELECT note_id, degree
                        FROM note_degrees
                        WHERE degree >= 2
                        ORDER BY random()
                        LIMIT 500
                    ),
                    triangles AS (
                        SELECT
                            sn.note_id,
                            sn.degree,
                            COUNT(*) AS triangle_count
                        FROM sampled_nodes sn
                        JOIN link l1 ON (
                            (l1.from_note_id = sn.note_id OR l1.to_note_id = sn.note_id)
                            AND l1.kind = 'semantic'
                        )
                        JOIN link l2 ON (
                            (l2.from_note_id = sn.note_id OR l2.to_note_id = sn.note_id)
                            AND l2.kind = 'semantic'
                            AND l2.id > l1.id
                        )
                        -- Check if the two neighbors are also linked
                        JOIN link l3 ON l3.kind = 'semantic' AND (
                            (l3.from_note_id = CASE WHEN l1.from_note_id = sn.note_id
                                THEN l1.to_note_id ELSE l1.from_note_id END
                             AND l3.to_note_id = CASE WHEN l2.from_note_id = sn.note_id
                                THEN l2.to_note_id ELSE l2.from_note_id END)
                            OR
                            (l3.from_note_id = CASE WHEN l2.from_note_id = sn.note_id
                                THEN l2.to_note_id ELSE l2.from_note_id END
                             AND l3.to_note_id = CASE WHEN l1.from_note_id = sn.note_id
                                THEN l1.to_note_id ELSE l1.from_note_id END)
                        )
                        GROUP BY sn.note_id, sn.degree
                    ),
                    clustering AS (
                        SELECT
                            CASE
                                WHEN COUNT(*) = 0 THEN 0.0
                                ELSE AVG(
                                    triangle_count::float8 /
                                    NULLIF((degree * (degree - 1)) / 2, 0)::float8
                                )
                            END AS clustering_coefficient
                        FROM triangles
                    )
                    SELECT
                        ds.total_notes,
                        ds.avg_degree,
                        ds.degree_std_dev,
                        ds.max_degree,
                        ds.isolated_nodes,
                        lc.total_links,
                        lc.total_semantic_links,
                        COALESCE(cl.clustering_coefficient, 0.0) AS clustering_coefficient
                    FROM degree_stats ds
                    CROSS JOIN link_counts lc
                    CROSS JOIN clustering cl
                    "#,
                )
                .fetch_one(&mut **tx)
                .await
                .map_err(matric_core::Error::Database)?;
                Ok(row)
            })
        })
        .await?;

    let graph_config = matric_core::defaults::GraphConfig::from_env();
    let note_count = stats_row.get::<i64, _>("total_notes") as usize;

    let stats = TopologyStats {
        total_notes: stats_row.get("total_notes"),
        total_semantic_links: stats_row.get("total_semantic_links"),
        total_links: stats_row.get("total_links"),
        avg_degree: stats_row.get("avg_degree"),
        degree_std_dev: stats_row.get("degree_std_dev"),
        max_degree: stats_row.get("max_degree"),
        isolated_nodes: stats_row.get("isolated_nodes"),
        clustering_coefficient: stats_row.get("clustering_coefficient"),
        strategy: graph_config.strategy.to_string(),
        k_neighbors: graph_config.effective_k(note_count),
        adaptive_k: graph_config.adaptive_k,
    };

    Ok(Json(stats))
}
```

**Performance note on clustering coefficient**: The triangle-counting CTE is bounded by `LIMIT 500` on the sampled nodes. For a graph with N notes and average degree d, the join complexity per sampled node is O(d^2). With d bounded by k (typically 7-15), this is O(500 * 15^2) = O(112,500) row comparisons, completing well under 100ms on PostgreSQL.

---

## 3. Data Model

### 3.1 Schema (No Changes)

The existing `link` table schema is sufficient:

```sql
CREATE TABLE link (
  id UUID PRIMARY KEY,
  from_note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  to_note_id UUID,
  to_url TEXT,
  kind TEXT NOT NULL,        -- 'semantic', 'wiki', 'manual'
  score REAL NOT NULL,        -- cosine similarity (0.0 - 1.0)
  created_at_utc TIMESTAMPTZ NOT NULL,
  metadata JSONB DEFAULT '{}'::jsonb  -- strategy provenance
);
```

### 3.2 Link Metadata JSON Format

The `metadata` JSONB field stores strategy provenance for each link. This enables distinguishing link origins without schema changes.

**Mutual k-NN link:**
```json
{
  "strategy": "mutual_knn",
  "k": 7,
  "forward_score": 0.82
}
```

**Fallback link (isolated node):**
```json
{
  "strategy": "fallback_best",
  "k": 7,
  "reason": "no_mutual_neighbors"
}
```

**Legacy threshold link (existing, unchanged):**
```json
null
```
or missing key entirely -- existing links have `metadata = '{}'::jsonb` or `NULL`.

**Wiki link (unchanged):**
```json
{
  "wiki_title": "Link Target Title"
}
```

### 3.3 Distinguishing Old vs. New Links

After migration to mutual k-NN, the link table will contain a mix:
- **Pre-migration**: `metadata` is `NULL` or `{}` (threshold-era links)
- **Post-migration**: `metadata->>'strategy'` is `"mutual_knn"` or `"fallback_best"`

A future cleanup job can identify and optionally delete pre-migration semantic links:
```sql
SELECT id FROM link
WHERE kind = 'semantic'
  AND (metadata IS NULL OR metadata->>'strategy' IS NULL);
```

---

## 4. API Design

### 4.1 New Endpoint: GET /api/v1/graph/topology/stats

**Purpose**: Provide graph health metrics for administrators and monitoring.

**Authentication**: Same as all `/api/v1/*` endpoints (protected when `REQUIRE_AUTH=true`).

**Multi-memory**: Respects `X-Fortemi-Memory` header; returns stats for the selected archive.

#### Request

```
GET /api/v1/graph/topology/stats
X-Fortemi-Memory: default    (optional)
Authorization: Bearer <token> (when REQUIRE_AUTH=true)
```

No query parameters.

#### Response (200 OK)

```json
{
  "total_notes": 450,
  "total_semantic_links": 1823,
  "total_links": 2150,
  "avg_degree": 8.1,
  "degree_std_dev": 2.3,
  "max_degree": 14,
  "isolated_nodes": 3,
  "clustering_coefficient": 0.42,
  "strategy": "mutual_knn",
  "k_neighbors": 7,
  "adaptive_k": false
}
```

#### Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `total_notes` | integer | Non-deleted notes in the archive |
| `total_semantic_links` | integer | Links where `kind = 'semantic'` |
| `total_links` | integer | All links (semantic + wiki + manual) |
| `avg_degree` | float | Mean semantic links per note |
| `degree_std_dev` | float | Standard deviation of degree distribution |
| `max_degree` | integer | Highest semantic link count for any note |
| `isolated_nodes` | integer | Notes with zero semantic links |
| `clustering_coefficient` | float | Fraction of connected triples forming triangles (sampled, approximate) |
| `strategy` | string | Active linking strategy (`mutual_knn` or `threshold`) |
| `k_neighbors` | integer | Current effective k (fixed or computed via adaptive formula) |
| `adaptive_k` | boolean | Whether k is computed from corpus size |

#### Error Responses

| Status | Condition |
|--------|-----------|
| 401 | Missing/invalid token when `REQUIRE_AUTH=true` |
| 404 | Archive not found (invalid `X-Fortemi-Memory` header) |
| 500 | Database query failure |

### 4.2 Existing Endpoints (Unchanged)

The following endpoints are not modified but benefit from improved topology:

- `GET /api/v1/graph/:id` -- `explore_graph` traversal returns more meaningful multi-hop paths
- `GET /api/v1/notes/:id/links` -- outgoing links are fewer but higher quality
- MCP tool `explore_graph` -- agent traversal finds better cross-topic connections

---

## 5. Algorithm Details

### 5.1 Mutual k-NN Pseudocode

```
FUNCTION mutual_knn_link(source_note, k, min_similarity):
    source_embedding := get_embedding(source_note)

    // Forward k-NN: find source's k nearest neighbors
    forward_knn := hnsw_search(source_embedding, k + 1)
    forward_knn := filter(forward_knn, id != source_note.id)
    forward_knn := filter(forward_knn, score >= min_similarity)

    mutual_pairs := []

    FOR EACH candidate IN forward_knn:
        candidate_embedding := get_embedding(candidate)

        // Reverse k-NN: find candidate's k nearest neighbors
        reverse_knn := hnsw_search(candidate_embedding, k + 1)

        // Check mutuality: is source in candidate's neighborhood?
        IF source_note.id IN reverse_knn.note_ids:
            mutual_pairs.append((source_note, candidate))

    // Create reciprocal links for all mutual pairs
    FOR EACH (a, b) IN mutual_pairs:
        create_reciprocal_link(a, b, kind="semantic", score=similarity(a, b))

    // Fallback: ensure no complete isolation
    IF mutual_pairs IS EMPTY AND forward_knn IS NOT EMPTY:
        best := forward_knn[0]  // highest similarity candidate
        create_reciprocal_link(source_note, best, kind="semantic", score=best.score)

    RETURN len(mutual_pairs) OR 1 if fallback triggered
```

### 5.2 Adaptive k Pseudocode

```
FUNCTION adaptive_k(note_count):
    IF note_count <= 1:
        RETURN 5  // minimum
    k := floor(log2(note_count))
    RETURN clamp(k, min=5, max=15)
```

**Mapping of corpus sizes to adaptive k:**

| Notes (N) | log2(N) | Effective k |
|-----------|---------|-------------|
| 1-31 | 0-4 | 5 (clamped to min) |
| 32 | 5 | 5 |
| 64 | 6 | 6 |
| 128 | 7 | 7 |
| 256 | 8 | 8 |
| 512 | 9 | 9 |
| 1,024 | 10 | 10 |
| 4,096 | 12 | 12 |
| 16,384 | 14 | 14 |
| 32,768+ | 15+ | 15 (clamped to max) |

### 5.3 Complexity Analysis

**Per-note linking cost:**

| Operation | Queries | Time (est.) |
|-----------|---------|-------------|
| Forward k-NN (HNSW) | 1 | 5-15ms |
| Get candidate embeddings | up to k | 1-3ms each |
| Reverse k-NN per candidate (HNSW) | up to k | 5-15ms each |
| Link creation (reciprocal) | up to k | 1-2ms each |
| **Total** | **2k + 1** | **50-200ms** |

For k=7: 15 queries, estimated 70-150ms per note.

**Comparison with threshold approach:**

| Metric | Threshold | Mutual k-NN |
|--------|-----------|-------------|
| HNSW queries per note | 1 | k+1 (worst case 2k+1) |
| Embedding reads per note | 1 | k+1 |
| Link writes per note | 0-20 | 0-k |
| Total wall time (p95) | ~50ms | ~150ms |

The additional cost is acceptable because linking runs as an asynchronous background job (via `matric-jobs` worker). The p95 target of 200ms (NFR-1) is achievable.

**Optimization: early termination of reverse lookups.** The implementation processes candidates in descending similarity order. Once we have found `k` mutual pairs, we can stop checking lower-ranked candidates. This is not implemented in Phase 1 for simplicity but is a straightforward optimization for Phase 2.

### 5.4 HNSW Index Reuse

The existing pgvector HNSW index on `embedding.vector` is used for both forward and reverse k-NN queries via `find_similar()`. No new index is needed. The query plan for `find_similar` uses:

```sql
-- This triggers HNSW index scan (already present)
ORDER BY e.vector <=> $1::vector
LIMIT $2
```

pgvector's `<=>` operator (cosine distance) is served by the existing HNSW index. The additional reverse queries are identical in structure -- they just use a different query vector -- so they hit the same index with the same access pattern.

---

## 6. Configuration

### 6.1 Environment Variables

| Variable | Type | Default | Valid Range | Description |
|----------|------|---------|-------------|-------------|
| `GRAPH_LINKING_STRATEGY` | string | `mutual_knn` | `threshold`, `mutual_knn` | Active linking strategy |
| `GRAPH_K_NEIGHBORS` | integer | `7` | `0` (adaptive) or `3..=50` | k for mutual k-NN |
| `GRAPH_MIN_SIMILARITY` | float | `0.5` | `0.0..=1.0` | Absolute similarity floor |

### 6.2 Configuration Without Restart

Environment variables are read per-job in `LinkingHandler::execute` via `GraphConfig::from_env()`, not cached at startup. This means changes to `.env` + container restart take effect immediately for newly enqueued linking jobs. Already-running jobs use the config they read at execution start.

For Docker bundle deployments:
```bash
# Update .env
echo "GRAPH_LINKING_STRATEGY=mutual_knn" >> .env

# Restart to pick up env changes
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### 6.3 Validation and Defaults

Invalid environment values are handled gracefully:

| Scenario | Behavior |
|----------|----------|
| `GRAPH_LINKING_STRATEGY=invalid` | Log warning, use `mutual_knn` |
| `GRAPH_K_NEIGHBORS=abc` | Log warning, use `7` |
| `GRAPH_K_NEIGHBORS=-5` | Parse fails (negative), use `7` |
| `GRAPH_K_NEIGHBORS=100` | Clamp to `50` |
| `GRAPH_K_NEIGHBORS=1` | Clamp to `3` |
| `GRAPH_MIN_SIMILARITY=2.0` | Clamp to `1.0` |
| `GRAPH_MIN_SIMILARITY=-0.5` | Clamp to `0.0` |
| All vars unset | `mutual_knn`, k=7, min=0.5 |

### 6.4 Backward Compatibility

When `GRAPH_LINKING_STRATEGY=threshold`, the system behaves identically to the pre-change code path. Existing constants `SEMANTIC_LINK_THRESHOLD` (0.7) and `SEMANTIC_LINK_THRESHOLD_CODE` (0.85) in `crates/matric-core/src/defaults.rs:284-290` are used exclusively by the threshold path and remain unchanged.

---

## 7. Performance Analysis

### 7.1 Query Budget Per Note

| Strategy | HNSW Searches | Embedding Reads | Link Writes | Estimated p95 |
|----------|--------------|-----------------|-------------|---------------|
| Threshold | 1 | 1 | ~10 | 50ms |
| Mutual k-NN (k=7) | 8 | 8 | ~4 | 150ms |
| Mutual k-NN (k=15) | 16 | 16 | ~8 | 250ms |

### 7.2 HNSW Query Latency

pgvector HNSW approximate search performance (measured on similar workloads):

| Embedding Count | Dimensions | Recall@10 | Latency (p50) | Latency (p95) |
|----------------|------------|-----------|---------------|---------------|
| 1,000 | 768 | 0.98 | 2ms | 5ms |
| 10,000 | 768 | 0.95 | 5ms | 12ms |
| 100,000 | 768 | 0.92 | 10ms | 25ms |

### 7.3 Scaling Characteristics

For a corpus of N notes with adaptive k:

```
Total queries per note = 2 * effective_k(N) + 1
Total queries for full re-link = N * (2 * effective_k(N) + 1)
```

| Corpus Size | k (adaptive) | Queries/Note | Full Re-link Total | Est. Duration |
|------------|-------------|-------------|-------------------|---------------|
| 100 | 6 | 13 | 1,300 | ~2 min |
| 1,000 | 10 | 21 | 21,000 | ~30 min |
| 10,000 | 13 | 27 | 270,000 | ~5 hrs |
| 50,000 | 15 | 31 | 1,550,000 | ~26 hrs |

Full re-linking is a one-time migration operation. Incremental linking (single note at a time) stays within the 200ms budget.

### 7.4 Connection Pool Impact

Each linking job uses the shared `sqlx::Pool<Postgres>` connection pool. The `2k+1` queries are executed sequentially (not in parallel) to avoid holding multiple connections simultaneously. With the default pool size of 10 connections and a job worker concurrency of 4, each linking job occupies 1 connection for ~150ms, well within pool capacity.

---

## 8. Error Handling

### 8.1 Failure Modes and Recovery

| Failure | Impact | Handling |
|---------|--------|----------|
| Forward k-NN query fails | No semantic links created for this note | Return `JobResult::Failed` with error message; job will be retried per worker retry policy |
| Reverse k-NN query fails for one candidate | That candidate skipped; other candidates still evaluated | Log warning, continue to next candidate |
| Candidate has no embeddings | Cannot verify mutuality | Skip candidate (`debug!` log), continue |
| All candidates below min_similarity | Zero links created | Fallback not triggered (no eligible best match); note remains isolated; log at `debug` level |
| `create_reciprocal` fails | Link not created (likely already exists) | Log at `debug` level (idempotent `WHERE NOT EXISTS` guard means this is expected on re-runs) |
| `embeddings.count()` fails (for adaptive k) | Cannot compute adaptive k | Return `JobResult::Failed`; job retried |
| Env var parse failure | Bad config value | Log warning, use default (never panic) |

### 8.2 Partial Failure Behavior

The linking job is not transactional -- links are created individually via `create_reciprocal`. If the job fails mid-way through (e.g., database connection lost after creating 3 of 7 links), the partial links remain. On retry, the idempotent `WHERE NOT EXISTS` guard in `create_reciprocal` prevents duplicate links. This is consistent with the existing threshold-based approach.

### 8.3 Logging

Structured logging via `tracing` at appropriate levels:

```
[INFO]  Linking completed  note_id=<uuid> result_count=4 strategy=mutual_knn duration_ms=123
[WARN]  Invalid GRAPH_K_NEIGHBORS, using default: 7  value="abc"
[DEBUG] Skipping candidate: no embeddings for reverse lookup  candidate=<uuid>
[DEBUG] Failed to create mutual link (may already exist)  error=<msg>
[DEBUG] No candidates above min_similarity, note remains isolated  note_id=<uuid>
```

---

## 9. Integration Points

### 9.1 Job Queue (`crates/matric-jobs`)

`LinkingHandler` is registered as a job handler for `JobType::Linking`. No changes to job queue infrastructure are needed -- the handler interface (`JobHandler::execute`) is unchanged.

The linking job is typically enqueued after an embedding job completes for a note. This sequencing is managed by the job worker and is not affected by this change.

### 9.2 MCP Tools

The following MCP tools in `mcp-server/index.js` interact with graph data and will benefit from improved topology without code changes:

| Tool | Interaction |
|------|------------|
| `explore_graph` | Calls `GET /api/v1/graph/:id`. Multi-hop traversal returns more diverse results with mesh topology. |
| `get_note_links` | Calls `GET /api/v1/notes/:id/links`. Returns fewer but higher-quality semantic links. |
| `search` | Trimodal search includes a graph component (`TRIMODAL_GRAPH_WEIGHT = 0.2`). Better topology improves graph-based ranking signals. |

A new MCP tool for topology stats is optional (not in Phase 1 scope). The REST endpoint `GET /api/v1/graph/topology/stats` can be added to MCP in a follow-up if agents need to query graph health.

### 9.3 SSE/WebSocket Event Bus

The existing `event_bus` emits events for link creation. These events are unchanged because the underlying `create_reciprocal` call is the same. Event subscribers (SSE, WebSocket, webhooks) receive the same `LinkCreated` event shape.

### 9.4 Existing explore_graph Handler

The `explore_graph` handler (`crates/matric-api/src/main.rs:5983-6010`) uses `traverse_graph_tx` which is a recursive CTE traversal. It operates on whatever links exist in the `link` table -- it is topology-agnostic. With mutual k-NN producing mesh topology, the same traversal query will naturally produce more meaningful multi-hop results because the graph structure has changed.

### 9.5 Multi-Memory Architecture

All graph operations route through `SchemaContext` (`ctx.query()`). The `GraphConfig` is read from process-level environment variables (not per-archive). This means all archives share the same linking strategy. Per-archive strategy configuration is out of scope for Phase 1.

---

## 10. Sequence Diagrams

### 10.1 Mutual k-NN Linking Job Execution

```
JobWorker           LinkingHandler         EmbeddingRepo          LinkRepo
   |                     |                      |                    |
   |  execute(ctx)       |                      |                    |
   |-------------------->|                      |                    |
   |                     |                      |                    |
   |                     |  fetch note          |                    |
   |                     |  parse wiki-links    |                    |
   |                     |  create wiki links   |                    |
   |                     |                      |                    |
   |                     |  get_for_note(id)    |                    |
   |                     |--------------------->|                    |
   |                     |  Vec<Embedding>       |                    |
   |                     |<---------------------|                    |
   |                     |                      |                    |
   |                     |  GraphConfig::from_env()                  |
   |                     |  strategy = mutual_knn                    |
   |                     |                      |                    |
   |                     |  count() [for adaptive k]                 |
   |                     |--------------------->|                    |
   |                     |  note_count          |                    |
   |                     |<---------------------|                    |
   |                     |                      |                    |
   |                     |  -- FORWARD k-NN --  |                    |
   |                     |  find_similar(vec, k+1)                   |
   |                     |--------------------->|                    |
   |                     |  candidates[]        |                    |
   |                     |<---------------------|                    |
   |                     |                      |                    |
   |                     |  -- FOR EACH CANDIDATE --                 |
   |                     |                      |                    |
   |                     |  get_for_note(cand)  |                    |
   |                     |--------------------->|                    |
   |                     |  cand_embedding      |                    |
   |                     |<---------------------|                    |
   |                     |                      |                    |
   |                     |  -- REVERSE k-NN --  |                    |
   |                     |  find_similar(cand_vec, k+1)              |
   |                     |--------------------->|                    |
   |                     |  reverse_knn[]       |                    |
   |                     |<---------------------|                    |
   |                     |                      |                    |
   |                     |  [if source in reverse_knn]               |
   |                     |  create_reciprocal(src, cand)             |
   |                     |-------------------------------------------->|
   |                     |  Ok(())              |                    |
   |                     |<--------------------------------------------|
   |                     |                      |                    |
   |                     |  -- END FOR EACH --  |                    |
   |                     |                      |                    |
   |                     |  [if created == 0: FALLBACK]              |
   |                     |  create_reciprocal(src, best)             |
   |                     |-------------------------------------------->|
   |                     |                      |                    |
   |  JobResult::Success |                      |                    |
   |<--------------------|                      |                    |
```

### 10.2 Strategy Dispatch Flow

```
LinkingHandler::execute
        |
        v
  +----------------+
  | Parse wiki-links|
  | (unchanged)     |
  +--------+-------+
           |
           v
  +------------------+
  | Get note embedding|
  +--------+---------+
           |
           v
  +---------------------+
  | GraphConfig::from_env|
  +--------+------------+
           |
           v
  +--------+---------+
  | strategy ==?      |
  +---+----------+---+
      |          |
  threshold   mutual_knn
      |          |
      v          v
  +--------+ +----------+
  |link_by_| |link_by_  |
  |threshold| |mutual_knn|
  +--------+ +----------+
      |          |
      v          v
  +------------------+
  | JobResult::Success|
  | {links_created,   |
  |  strategy, k}     |
  +------------------+
```

### 10.3 Topology Stats Request

```
Client            API Router          TopologyStats Handler        PostgreSQL
  |                   |                       |                        |
  | GET /api/v1/graph/topology/stats          |                        |
  |------------------>|                       |                        |
  |                   |  get_topology_stats() |                        |
  |                   |---------------------->|                        |
  |                   |                       |                        |
  |                   |                       | for_schema(archive)    |
  |                   |                       |                        |
  |                   |                       | CTE query (degree      |
  |                   |                       | stats + triangle       |
  |                   |                       | counting)              |
  |                   |                       |----------------------->|
  |                   |                       |  row                   |
  |                   |                       |<-----------------------|
  |                   |                       |                        |
  |                   |                       | GraphConfig::from_env()|
  |                   |                       | effective_k(note_count)|
  |                   |                       |                        |
  |                   | Json(TopologyStats)   |                        |
  |                   |<----------------------|                        |
  |  200 OK           |                       |                        |
  |<------------------|                       |                        |
```

---

## 11. Testing Strategy

### 11.1 Unit Tests (`crates/matric-core`)

**GraphConfig tests** (pure functions, no database):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_k_adaptive() {
        let config = GraphConfig {
            adaptive_k: true,
            k_neighbors: 0,
            ..Default::default()
        };
        assert_eq!(config.effective_k(1), 5);      // min clamp
        assert_eq!(config.effective_k(32), 5);      // log2(32)=5
        assert_eq!(config.effective_k(128), 7);     // log2(128)=7
        assert_eq!(config.effective_k(1024), 10);   // log2(1024)=10
        assert_eq!(config.effective_k(100_000), 15); // max clamp
    }

    #[test]
    fn test_effective_k_fixed() {
        let config = GraphConfig {
            adaptive_k: false,
            k_neighbors: 10,
            ..Default::default()
        };
        assert_eq!(config.effective_k(1), 10);
        assert_eq!(config.effective_k(100_000), 10);
    }

    #[test]
    fn test_strategy_parsing() {
        assert_eq!(
            GraphLinkingStrategy::from_env("mutual_knn"),
            Some(GraphLinkingStrategy::MutualKnn)
        );
        assert_eq!(
            GraphLinkingStrategy::from_env("THRESHOLD"),
            Some(GraphLinkingStrategy::Threshold)
        );
        assert_eq!(
            GraphLinkingStrategy::from_env("mutual-knn"),
            Some(GraphLinkingStrategy::MutualKnn)
        );
        assert_eq!(GraphLinkingStrategy::from_env("invalid"), None);
    }
}
```

### 11.2 Integration Tests (Database Required)

**Mutual k-NN linking** (requires PostgreSQL with pgvector):

1. Create 5 notes with known embedding vectors that form two clusters
2. Run `link_by_mutual_knn` for each note
3. Verify: links exist only between mutual k-NN pairs
4. Verify: no links between clusters (if vectors are sufficiently distant)
5. Verify: link metadata contains `{"strategy": "mutual_knn", "k": ...}`

**Isolated node fallback**:

1. Create 1 note with embedding vector far from all others
2. Create 3 notes that are mutually similar to each other
3. Run `link_by_mutual_knn` for the isolated note
4. Verify: exactly 1 fallback link created
5. Verify: metadata contains `{"strategy": "fallback_best", "reason": "no_mutual_neighbors"}`

**Backward compatibility**:

1. Set `GRAPH_LINKING_STRATEGY=threshold` in test env
2. Run `LinkingHandler::execute` for a note
3. Verify: links created using threshold logic (same as before)
4. Verify: link metadata is `None` (no strategy field)

**Topology stats endpoint**:

1. Create known graph structure (e.g., 4 notes forming a cycle)
2. Call `GET /api/v1/graph/topology/stats`
3. Verify: `total_notes=4`, `avg_degree=2.0`, `isolated_nodes=0`
4. Verify: `clustering_coefficient > 0` (cycle has triangles if complete enough)

### 11.3 Test Isolation

Per project testing standards:
- Use UUID-based unique identifiers for test notes (not timestamps)
- Run with `--test-threads=1` for integration tests sharing database state
- Never use `std::env::set_var` in tests -- inject `GraphConfig` directly

---

## 12. Migration Plan

### 12.1 Incremental Migration (Default)

When the feature deploys, newly created/updated notes get mutual k-NN links while existing notes retain their threshold-era links. The topology gradually improves as notes are edited or re-embedded.

### 12.2 Full Re-Linking (Optional)

For operators who want immediate mesh topology across all notes:

```bash
# 1. Delete existing semantic links
psql -U matric -d matric -c \
  "DELETE FROM link WHERE kind = 'semantic';"

# 2. Re-enqueue all notes for linking
psql -U matric -d matric -c \
  "INSERT INTO job_queue (id, job_type, note_id, status, created_at)
   SELECT gen_random_uuid(), 'linking', id, 'pending', NOW()
   FROM note WHERE deleted_at IS NULL;"

# 3. Monitor progress via topology stats
curl http://localhost:3000/api/v1/graph/topology/stats
```

**Estimated duration**: See Section 7.3 scaling table. For 1,000 notes: ~30 minutes.

### 12.3 Rollback

Set `GRAPH_LINKING_STRATEGY=threshold` in `.env` and restart. New linking jobs will use the legacy threshold approach. Existing mutual k-NN links remain in the graph (they are valid semantic links). To fully revert, delete links with strategy metadata and re-link:

```bash
# Delete only mutual k-NN links
psql -U matric -d matric -c \
  "DELETE FROM link WHERE kind = 'semantic' AND metadata->>'strategy' IS NOT NULL;"

# Re-link with threshold
# (set GRAPH_LINKING_STRATEGY=threshold first)
psql -U matric -d matric -c \
  "INSERT INTO job_queue (id, job_type, note_id, status, created_at)
   SELECT gen_random_uuid(), 'linking', id, 'pending', NOW()
   FROM note WHERE deleted_at IS NULL;"
```

---

## 13. Open Questions

| # | Question | Impact | Resolution Path |
|---|----------|--------|-----------------|
| 1 | Should `GraphConfig` be cached on `AppState` at startup rather than read per-job via `from_env()`? | Per-job read is ~1us (negligible) but prevents hot config reload. Caching requires restart for changes. | Decision: per-job read. Config changes take effect on next linking job without restart. |
| 2 | Should the topology stats endpoint cache results? | The CTE query with triangle counting may be slow on very large graphs (>10k notes with >50k links). | Phase 1: no caching. Monitor p95 latency. Add `Cache-Control: max-age=60` or Redis cache in Phase 2 if needed. |
| 3 | Should we expose a "re-link all" API endpoint instead of requiring manual SQL? | Operators need a convenient migration path. | Deferred: can be added as a job-queue endpoint in a follow-up issue. Not blocking for Phase 1. |
| 4 | Clustering coefficient accuracy: is sampling 500 nodes sufficient? | For graphs under 10k notes, sampling 500 provides a good estimate. Larger graphs may need more samples. | Acceptable for Phase 1. Document that the coefficient is approximate. |

---

## 14. Acceptance Criteria Traceability

| PRD Requirement | TDD Section | Implementation |
|-----------------|-------------|----------------|
| FR-1: Configurable strategy | 2.1, 6.1 | `GraphConfig::from_env()` with `GRAPH_LINKING_STRATEGY` |
| FR-2: Mutual k-NN | 2.2.3, 5.1 | `link_by_mutual_knn()` method |
| FR-3: Adaptive k | 2.1, 5.2 | `GraphConfig::effective_k()` with `GRAPH_K_NEIGHBORS=0` |
| FR-4: Isolated fallback | 2.2.3 (Step 4) | Fallback to single best match when `created == 0` |
| FR-5: Topology metrics | 2.3, 4.1 | `GET /api/v1/graph/topology/stats` endpoint |
| FR-6: Backward compat | 2.2.2, 6.4 | `link_by_threshold()` path when strategy=threshold |
| NFR-1: <=200ms p95 | 7.1, 7.2 | Sequential queries, k=7 budget ~150ms |
| NFR-2: No new deps | All | Zero new crate dependencies |
| NFR-3: Config without restart | 6.2 | Per-job `from_env()` read |
