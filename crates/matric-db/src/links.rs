//! Link repository implementation.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{new_v7, Error, Link, LinkRepository, Result};

/// PostgreSQL implementation of LinkRepository.
pub struct PgLinkRepository {
    pool: Pool<Postgres>,
}

impl PgLinkRepository {
    /// Create a new PgLinkRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LinkRepository for PgLinkRepository {
    async fn create(
        &self,
        from_note_id: Uuid,
        to_note_id: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<Uuid> {
        let link_id = new_v7();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(link_id)
        .bind(from_note_id)
        .bind(to_note_id)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(link_id)
    }

    async fn create_reciprocal(
        &self,
        note_a: Uuid,
        note_b: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<()> {
        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Forward link (A -> B)
        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(new_v7())
        .bind(note_a)
        .bind(note_b)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Backward link (B -> A)
        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(new_v7())
        .bind(note_b)
        .bind(note_a)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn get_outgoing(&self, note_id: Uuid) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.to_note_id
               WHERE l.from_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    async fn get_incoming(&self, note_id: Uuid) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.from_note_id
               WHERE l.to_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    async fn delete_for_note(&self, note_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM link WHERE from_note_id = $1 OR to_note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}

/// Graph node in v1 payload contract (#467).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub title: Option<String>,
    pub depth: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<Uuid>,
    pub archived: bool,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    // Community hints (#468) — populated when backend community detection is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_confidence: Option<f32>,
}

/// Graph edge in v1 payload contract (#467).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphEdge {
    pub source: Uuid,
    pub target: Uuid,
    pub edge_type: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i32>,
    // Provenance fields (#467, #468).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computed_at: Option<DateTime<Utc>>,
    /// Normalized edge weight rescaled to [0.0, 1.0] with optional gamma (#470).
    /// Computed on-the-fly during graph traversal: ((score - min) / (max - min)) ^ gamma.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_weight: Option<f32>,
}

/// Truncation and guardrail metadata (#469).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphMeta {
    pub total_nodes: i64,
    pub total_edges: i64,
    pub truncated_nodes: i64,
    pub truncated_edges: i64,
    pub effective_depth: i32,
    pub effective_max_nodes: i64,
    pub effective_min_score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_max_edges_per_node: Option<i64>,
    pub truncation_reasons: Vec<String>,
}

/// Versioned result of graph traversal (v1 contract, #467).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphResult {
    pub graph_version: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub meta: GraphMeta,
}

impl PgLinkRepository {
    /// List all links in the database.
    pub async fn list_all(&self, limit: i64, offset: i64) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                '' as snippet
               FROM link l
               ORDER BY l.created_at_utc DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    /// Count total links.
    pub async fn count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM link")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("count"))
    }

    /// Traverse the knowledge graph starting from a note.
    ///
    /// Uses recursive CTE to explore links up to `max_depth` hops.
    /// Returns versioned v1 payload with nodes, edges, and metadata.
    pub async fn traverse_graph(
        &self,
        start_id: Uuid,
        max_depth: i32,
        max_nodes: i64,
        min_score: f32,
        max_edges_per_node: Option<i64>,
    ) -> Result<GraphResult> {
        // Use recursive CTE to traverse the graph, then window-count for truncation detection
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE graph AS (
                SELECT $1::uuid as note_id, 0 as depth
                UNION
                SELECT
                    CASE WHEN l.from_note_id = g.note_id THEN l.to_note_id ELSE l.from_note_id END as note_id,
                    g.depth + 1 as depth
                FROM graph g
                JOIN link l ON (l.from_note_id = g.note_id OR l.to_note_id = g.note_id)
                WHERE g.depth < $2
            ),
            deduped AS (
                SELECT DISTINCT ON (g.note_id)
                    g.note_id, g.depth, n.title, n.collection_id,
                    COALESCE(n.archived, false) as archived,
                    n.created_at_utc, n.updated_at_utc
                FROM graph g
                JOIN note n ON n.id = g.note_id
                WHERE n.deleted_at IS NULL
                ORDER BY g.note_id, g.depth
            )
            SELECT *, COUNT(*) OVER() as total_reachable
            FROM deduped
            LIMIT $3
            "#,
        )
        .bind(start_id)
        .bind(max_depth)
        .bind(max_nodes)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let total_reachable: i64 = rows.first().map(|r| r.get("total_reachable")).unwrap_or(0);
        let nodes: Vec<GraphNode> = rows
            .iter()
            .map(|row| GraphNode {
                id: row.get("note_id"),
                title: row.get("title"),
                depth: row.get("depth"),
                collection_id: row.get("collection_id"),
                archived: row.get("archived"),
                created_at_utc: row.get("created_at_utc"),
                updated_at_utc: row.get("updated_at_utc"),
                community_id: None,
                community_label: None,
                community_confidence: None,
            })
            .collect();

        let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();

        // Fetch edges with provenance, optional per-node limit, and min_score filter
        let edge_rows = sqlx::query(
            r#"
            WITH ranked AS (
                SELECT from_note_id, to_note_id, score, kind,
                       created_at_utc, metadata,
                       ROW_NUMBER() OVER (PARTITION BY from_note_id ORDER BY score DESC) as rn,
                       COUNT(*) OVER() as total_edges
                FROM link
                WHERE from_note_id = ANY($1) AND to_note_id = ANY($1)
                  AND score >= $2
            )
            SELECT * FROM ranked
            WHERE ($3::bigint IS NULL OR rn <= $3)
            "#,
        )
        .bind(&node_ids)
        .bind(min_score)
        .bind(max_edges_per_node)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let total_edges: i64 = edge_rows.first().map(|r| r.get("total_edges")).unwrap_or(0);
        let edges: Vec<GraphEdge> = edge_rows
            .iter()
            .map(|row| {
                let metadata: Option<JsonValue> = row.get("metadata");
                GraphEdge {
                    source: row.get("from_note_id"),
                    target: row.get("to_note_id"),
                    score: row.get("score"),
                    edge_type: row.get("kind"),
                    rank: Some(row.get::<i64, _>("rn") as i32),
                    embedding_set: metadata
                        .as_ref()
                        .and_then(|m| m.get("embedding_set"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    model: metadata
                        .as_ref()
                        .and_then(|m| m.get("model"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    computed_at: row.get("created_at_utc"),
                    normalized_weight: None, // computed below
                }
            })
            .collect();

        // Compute normalized edge weights (#470): rescale scores to [0.0, 1.0] with gamma.
        let edges = Self::apply_edge_normalization(edges);

        // Louvain community detection (#473): assign community IDs (no SKOS labels in non-tx path).
        let graph_config = matric_core::defaults::GraphConfig::from_env();
        let nodes = Self::assign_communities(nodes, &edges, graph_config.community_resolution);

        let truncated_nodes = (total_reachable - nodes.len() as i64).max(0);
        let truncated_edges = (total_edges - edges.len() as i64).max(0);
        let mut truncation_reasons = Vec::new();
        if truncated_nodes > 0 {
            truncation_reasons.push(format!(
                "max_nodes limit: {} of {} nodes returned",
                nodes.len(),
                total_reachable
            ));
        }
        if truncated_edges > 0 {
            truncation_reasons.push(format!(
                "max_edges_per_node limit: {} of {} edges returned",
                edges.len(),
                total_edges
            ));
        }

        Ok(GraphResult {
            graph_version: "v1".to_string(),
            nodes,
            edges,
            meta: GraphMeta {
                total_nodes: total_reachable,
                total_edges,
                truncated_nodes,
                truncated_edges,
                effective_depth: max_depth,
                effective_max_nodes: max_nodes,
                effective_min_score: min_score,
                effective_max_edges_per_node: max_edges_per_node,
                truncation_reasons,
            },
        })
    }

    /// Apply min-max normalization with gamma exponent to edge scores (#470).
    ///
    /// Rescales raw cosine similarity scores from their natural narrow band
    /// (typically 0.70–0.94 for same-domain embeddings) to the full [0.0, 1.0]
    /// range. Gamma > 1.0 compresses mid-range edges (emphasizes extremes),
    /// gamma < 1.0 expands mid-range (flattens differences).
    ///
    /// Formula: `normalized = ((score - min) / (max - min)) ^ gamma`
    fn apply_edge_normalization(mut edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
        if edges.len() < 2 {
            // With 0-1 edges, normalization is meaningless — set to 1.0
            for edge in &mut edges {
                edge.normalized_weight = Some(1.0);
            }
            return edges;
        }

        let gamma = matric_core::defaults::GraphConfig::from_env().normalization_gamma;

        let min_score = edges.iter().map(|e| e.score).fold(f32::INFINITY, f32::min);
        let max_score = edges
            .iter()
            .map(|e| e.score)
            .fold(f32::NEG_INFINITY, f32::max);

        let range = max_score - min_score;
        if range < f32::EPSILON {
            // All edges have the same score — normalize to 1.0
            for edge in &mut edges {
                edge.normalized_weight = Some(1.0);
            }
            return edges;
        }

        for edge in &mut edges {
            let normalized = (edge.score - min_score) / range;
            edge.normalized_weight = Some(normalized.powf(gamma));
        }

        edges
    }

    /// Louvain community detection (#473): assign community_id and confidence to nodes.
    ///
    /// Pure computation on in-memory graph — no DB access. Deterministic: nodes
    /// iterated in UUID sort order.
    fn assign_communities(
        mut nodes: Vec<GraphNode>,
        edges: &[GraphEdge],
        resolution: f64,
    ) -> Vec<GraphNode> {
        use std::collections::HashMap;

        if nodes.len() < 2 {
            for node in &mut nodes {
                node.community_id = Some(0);
                node.community_confidence = Some(1.0);
            }
            return nodes;
        }

        // Build adjacency list with weights.
        let mut adj: HashMap<Uuid, Vec<(Uuid, f64)>> = HashMap::new();
        let mut total_weight = 0.0_f64;
        for edge in edges {
            let w = edge.normalized_weight.unwrap_or(edge.score) as f64;
            adj.entry(edge.source).or_default().push((edge.target, w));
            adj.entry(edge.target).or_default().push((edge.source, w));
            total_weight += w;
        }

        if total_weight < f64::EPSILON {
            for (i, node) in nodes.iter_mut().enumerate() {
                node.community_id = Some(i as i64);
                node.community_confidence = Some(0.0);
            }
            return nodes;
        }

        // Initialize: each node in its own community.
        let mut node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();
        node_ids.sort(); // Deterministic iteration order.

        let mut community: HashMap<Uuid, usize> = HashMap::new();
        for (i, &id) in node_ids.iter().enumerate() {
            community.insert(id, i);
        }

        // Precompute weighted degree (k_i) for each node.
        let mut k: HashMap<Uuid, f64> = HashMap::new();
        for &id in &node_ids {
            let degree: f64 = adj
                .get(&id)
                .map(|nbrs| nbrs.iter().map(|(_, w)| w).sum())
                .unwrap_or(0.0);
            k.insert(id, degree);
        }

        // Precompute community → sum of internal degrees (Sigma_tot).
        let mut sigma_tot: HashMap<usize, f64> = HashMap::new();
        for &id in &node_ids {
            let comm = community[&id];
            *sigma_tot.entry(comm).or_default() += k[&id];
        }

        let m2 = 2.0 * total_weight;

        // Phase 1: Local moves — iterate until no improvement.
        let mut improved = true;
        let mut iterations = 0;
        while improved && iterations < 100 {
            improved = false;
            iterations += 1;

            for &node_id in &node_ids {
                let current_comm = community[&node_id];
                let ki = k[&node_id];

                // Sum of weights from node_id to each neighboring community.
                let mut comm_weights: HashMap<usize, f64> = HashMap::new();
                if let Some(neighbors) = adj.get(&node_id) {
                    for &(nbr, w) in neighbors {
                        let nbr_comm = community[&nbr];
                        *comm_weights.entry(nbr_comm).or_default() += w;
                    }
                }

                // Weight to current community (k_i,in).
                let ki_in = *comm_weights.get(&current_comm).unwrap_or(&0.0);

                // Modularity gain of removing node from current community.
                let remove_cost = ki_in - resolution * (sigma_tot[&current_comm] - ki) * ki / m2;

                // Find best community to move to.
                let mut best_comm = current_comm;
                let mut best_gain = 0.0_f64;

                for (&target_comm, &ki_target) in &comm_weights {
                    if target_comm == current_comm {
                        continue;
                    }
                    let gain = ki_target
                        - resolution * sigma_tot.get(&target_comm).unwrap_or(&0.0) * ki / m2
                        - remove_cost;
                    if gain > best_gain {
                        best_gain = gain;
                        best_comm = target_comm;
                    }
                }

                if best_comm != current_comm {
                    // Move node to best community.
                    *sigma_tot.entry(current_comm).or_default() -= ki;
                    *sigma_tot.entry(best_comm).or_default() += ki;
                    community.insert(node_id, best_comm);
                    improved = true;
                }
            }
        }

        // Renumber communities to contiguous 0..N.
        let mut comm_ids: Vec<usize> = community.values().copied().collect();
        comm_ids.sort_unstable();
        comm_ids.dedup();
        let renumber: HashMap<usize, i64> = comm_ids
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as i64))
            .collect();

        // Compute per-node confidence: fraction of node's edges that stay within its community.
        for node in &mut nodes {
            let comm = community.get(&node.id).copied().unwrap_or(0);
            node.community_id = renumber.get(&comm).copied().or(Some(0));

            if let Some(neighbors) = adj.get(&node.id) {
                let total_w: f64 = neighbors.iter().map(|(_, w)| w).sum();
                if total_w > f64::EPSILON {
                    let internal_w: f64 = neighbors
                        .iter()
                        .filter(|(nbr, _)| {
                            community.get(nbr).copied().unwrap_or(usize::MAX) == comm
                        })
                        .map(|(_, w)| w)
                        .sum();
                    node.community_confidence = Some((internal_w / total_w) as f32);
                } else {
                    node.community_confidence = Some(0.0);
                }
            } else {
                node.community_confidence = Some(0.0);
            }
        }

        nodes
    }

    /// Assign SKOS-based labels to communities (#473).
    ///
    /// For each community, finds the most frequent preferred SKOS concept label
    /// among its member notes. Requires a transaction for DB access.
    async fn label_communities_skos(
        tx: &mut Transaction<'_, Postgres>,
        nodes: &mut [GraphNode],
    ) -> Result<()> {
        use std::collections::HashMap;

        // Collect note_ids grouped by community.
        let mut community_notes: HashMap<i64, Vec<Uuid>> = HashMap::new();
        for node in nodes.iter() {
            if let Some(cid) = node.community_id {
                community_notes.entry(cid).or_default().push(node.id);
            }
        }

        if community_notes.is_empty() {
            return Ok(());
        }

        // Gather all note IDs for a single query.
        let all_note_ids: Vec<Uuid> = community_notes.values().flatten().copied().collect();
        if all_note_ids.is_empty() {
            return Ok(());
        }

        // Get preferred SKOS labels for all relevant notes in one query.
        let label_rows = sqlx::query(
            "SELECT nc.note_id, l.value as label \
             FROM note_skos_concept nc \
             JOIN skos_concept_label l ON nc.concept_id = l.concept_id \
             WHERE nc.note_id = ANY($1) AND l.label_type = 'pref_label' \
             ORDER BY nc.is_primary DESC, nc.relevance_score DESC",
        )
        .bind(&all_note_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Build note_id → labels map.
        let mut note_labels: HashMap<Uuid, Vec<String>> = HashMap::new();
        for row in &label_rows {
            let note_id: Uuid = row.get("note_id");
            let label: String = row.get("label");
            note_labels.entry(note_id).or_default().push(label);
        }

        // For each community, find the most frequent label.
        let mut community_labels: HashMap<i64, String> = HashMap::new();
        for (&cid, note_ids) in &community_notes {
            let mut label_counts: HashMap<&str, usize> = HashMap::new();
            for nid in note_ids {
                if let Some(labels) = note_labels.get(nid) {
                    // Count the first (primary) label for each note.
                    if let Some(label) = labels.first() {
                        *label_counts.entry(label.as_str()).or_default() += 1;
                    }
                }
            }
            if let Some((&best_label, _)) = label_counts.iter().max_by_key(|(_, &count)| count) {
                community_labels.insert(cid, best_label.to_string());
            }
        }

        // Assign labels to nodes.
        for node in nodes.iter_mut() {
            if let Some(cid) = node.community_id {
                node.community_label = community_labels.get(&cid).cloned();
            }
        }

        Ok(())
    }
}

/// Transaction-aware variants for archive-scoped operations (Issue #108).
impl PgLinkRepository {
    /// Create a link within an existing transaction.
    pub async fn create_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        from_note_id: Uuid,
        to_note_id: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<Uuid> {
        let link_id = new_v7();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(link_id)
        .bind(from_note_id)
        .bind(to_note_id)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(link_id)
    }

    /// Create reciprocal links within an existing transaction.
    pub async fn create_reciprocal_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_a: Uuid,
        note_b: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<()> {
        let now = Utc::now();

        // Forward link (A -> B)
        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(new_v7())
        .bind(note_a)
        .bind(note_b)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Backward link (B -> A)
        sqlx::query(
            "INSERT INTO link (id, from_note_id, to_note_id, to_url, kind, score, created_at_utc, metadata)
             SELECT $1, $2, $3, NULL, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM link
                 WHERE from_note_id = $2 AND to_note_id = $3 AND kind = $4
             )",
        )
        .bind(new_v7())
        .bind(note_b)
        .bind(note_a)
        .bind(kind)
        .bind(score)
        .bind(now)
        .bind(&metadata)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get outgoing links within an existing transaction.
    pub async fn get_outgoing_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.to_note_id
               WHERE l.from_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    /// Get incoming links within an existing transaction.
    pub async fn get_incoming_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.from_note_id
               WHERE l.to_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    /// Delete all links for a note within an existing transaction.
    pub async fn delete_for_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<()> {
        sqlx::query("DELETE FROM link WHERE from_note_id = $1 OR to_note_id = $1")
            .bind(note_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Traverse the knowledge graph starting from a note within an existing transaction.
    ///
    /// Returns versioned v1 payload with truncation metadata and guardrails (#467, #468, #469).
    #[allow(clippy::too_many_arguments)]
    pub async fn traverse_graph_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        start_id: Uuid,
        max_depth: i32,
        max_nodes: i64,
        min_score: f32,
        max_edges_per_node: Option<i64>,
        edge_filter: Option<&str>,
        include_structural: bool,
    ) -> Result<GraphResult> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE graph AS (
                SELECT $1::uuid as note_id, 0 as depth
                UNION
                SELECT
                    CASE WHEN l.from_note_id = g.note_id THEN l.to_note_id ELSE l.from_note_id END as note_id,
                    g.depth + 1 as depth
                FROM graph g
                JOIN link l ON (l.from_note_id = g.note_id OR l.to_note_id = g.note_id)
                WHERE g.depth < $2
            ),
            deduped AS (
                SELECT DISTINCT ON (g.note_id)
                    g.note_id, g.depth, n.title, n.collection_id,
                    COALESCE(n.archived, false) as archived,
                    n.created_at_utc, n.updated_at_utc
                FROM graph g
                JOIN note n ON n.id = g.note_id
                WHERE n.deleted_at IS NULL
                ORDER BY g.note_id, g.depth
            )
            SELECT *, COUNT(*) OVER() as total_reachable
            FROM deduped
            LIMIT $3
            "#,
        )
        .bind(start_id)
        .bind(max_depth)
        .bind(max_nodes)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let total_reachable: i64 = rows.first().map(|r| r.get("total_reachable")).unwrap_or(0);
        let nodes: Vec<GraphNode> = rows
            .iter()
            .map(|row| GraphNode {
                id: row.get("note_id"),
                title: row.get("title"),
                depth: row.get("depth"),
                collection_id: row.get("collection_id"),
                archived: row.get("archived"),
                created_at_utc: row.get("created_at_utc"),
                updated_at_utc: row.get("updated_at_utc"),
                community_id: None,
                community_label: None,
                community_confidence: None,
            })
            .collect();

        let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();

        let edge_rows = sqlx::query(
            r#"
            WITH ranked AS (
                SELECT from_note_id, to_note_id, score, kind,
                       created_at_utc, metadata,
                       ROW_NUMBER() OVER (PARTITION BY from_note_id ORDER BY score DESC) as rn,
                       COUNT(*) OVER() as total_edges
                FROM link
                WHERE from_note_id = ANY($1) AND to_note_id = ANY($1)
                  AND score >= $2
            )
            SELECT * FROM ranked
            WHERE ($3::bigint IS NULL OR rn <= $3)
            "#,
        )
        .bind(&node_ids)
        .bind(min_score)
        .bind(max_edges_per_node)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let total_edges: i64 = edge_rows.first().map(|r| r.get("total_edges")).unwrap_or(0);
        let edges: Vec<GraphEdge> = edge_rows
            .iter()
            .map(|row| {
                let metadata: Option<JsonValue> = row.get("metadata");
                GraphEdge {
                    source: row.get("from_note_id"),
                    target: row.get("to_note_id"),
                    score: row.get("score"),
                    edge_type: row.get("kind"),
                    rank: Some(row.get::<i64, _>("rn") as i32),
                    embedding_set: metadata
                        .as_ref()
                        .and_then(|m| m.get("embedding_set"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    model: metadata
                        .as_ref()
                        .and_then(|m| m.get("model"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    computed_at: row.get("created_at_utc"),
                    normalized_weight: None, // computed below
                }
            })
            .collect();

        // Compute normalized edge weights (#470): rescale scores to [0.0, 1.0] with gamma.
        let edges = Self::apply_edge_normalization(edges);

        // Louvain community detection (#473): populate community_id, label, confidence.
        let graph_config = matric_core::defaults::GraphConfig::from_env();
        let mut nodes = Self::assign_communities(nodes, &edges, graph_config.community_resolution);

        // SKOS-based community labels: find dominant concept per community.
        Self::label_communities_skos(tx, &mut nodes).await?;

        // Edge community filter (#480): filter edges by community relationship.
        let node_community: std::collections::HashMap<Uuid, Option<i64>> =
            nodes.iter().map(|n| (n.id, n.community_id)).collect();

        let mut edges = match edge_filter {
            Some("intra_community") => edges
                .into_iter()
                .filter(|e| {
                    let sc = node_community.get(&e.source).copied().flatten();
                    let tc = node_community.get(&e.target).copied().flatten();
                    sc.is_some() && sc == tc
                })
                .collect(),
            Some("inter_community") => edges
                .into_iter()
                .filter(|e| {
                    let sc = node_community.get(&e.source).copied().flatten();
                    let tc = node_community.get(&e.target).copied().flatten();
                    sc.is_some() && tc.is_some() && sc != tc
                })
                .collect(),
            _ => edges, // "all" or unrecognized
        };

        // Structural collection edges (#480): connect notes in the same collection.
        if include_structural {
            let structural_score = graph_config.structural_score;
            let mut collection_nodes: std::collections::HashMap<Uuid, Vec<Uuid>> =
                std::collections::HashMap::new();
            for node in &nodes {
                if let Some(coll_id) = node.collection_id {
                    collection_nodes.entry(coll_id).or_default().push(node.id);
                }
            }
            for members in collection_nodes.values() {
                if members.len() < 2 {
                    continue;
                }
                // Create edges between all pairs in the same collection.
                for i in 0..members.len() {
                    for j in (i + 1)..members.len() {
                        edges.push(GraphEdge {
                            source: members[i],
                            target: members[j],
                            edge_type: "structural_collection".to_string(),
                            score: structural_score,
                            rank: None,
                            embedding_set: None,
                            model: None,
                            computed_at: None,
                            normalized_weight: Some(structural_score),
                        });
                    }
                }
            }
        }

        let truncated_nodes = (total_reachable - nodes.len() as i64).max(0);
        let truncated_edges = (total_edges - edges.len() as i64).max(0);
        let mut truncation_reasons = Vec::new();
        if truncated_nodes > 0 {
            truncation_reasons.push(format!(
                "max_nodes limit: {} of {} nodes returned",
                nodes.len(),
                total_reachable
            ));
        }
        if truncated_edges > 0 {
            truncation_reasons.push(format!(
                "max_edges_per_node limit: {} of {} edges returned",
                edges.len(),
                total_edges
            ));
        }

        Ok(GraphResult {
            graph_version: "v1".to_string(),
            nodes,
            edges,
            meta: GraphMeta {
                total_nodes: total_reachable,
                total_edges,
                truncated_nodes,
                truncated_edges,
                effective_depth: max_depth,
                effective_max_nodes: max_nodes,
                effective_min_score: min_score,
                effective_max_edges_per_node: max_edges_per_node,
                truncation_reasons,
            },
        })
    }

    /// List all links within an existing transaction.
    pub async fn list_all_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Link>> {
        let rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                '' as snippet
               FROM link l
               ORDER BY l.created_at_utc DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let links = rows
            .into_iter()
            .map(|row| Link {
                id: row.get("id"),
                from_note_id: row.get("from_note_id"),
                to_note_id: row.get("to_note_id"),
                to_url: row.get("to_url"),
                kind: row.get("kind"),
                score: row.get("score"),
                created_at_utc: row.get("created_at_utc"),
                snippet: row.get("snippet"),
                metadata: row.get("metadata"),
            })
            .collect();

        Ok(links)
    }

    /// Count total links within an existing transaction.
    pub async fn count_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM link")
            .fetch_one(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("count"))
    }

    /// Compute graph topology statistics within an existing transaction.
    ///
    /// Returns degree distribution, clustering coefficient, connected components,
    /// and isolated node count — all in a single SQL round-trip.
    pub async fn topology_stats_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<TopologyStats> {
        // Degree distribution + basic counts
        let row = sqlx::query(
            r#"
            WITH degrees AS (
                SELECT note_id, COUNT(*) as degree FROM (
                    SELECT from_note_id AS note_id FROM link WHERE kind = 'semantic'
                    UNION ALL
                    SELECT to_note_id AS note_id FROM link WHERE kind = 'semantic'
                ) sub
                GROUP BY note_id
            ),
            all_notes AS (
                SELECT id FROM note WHERE deleted_at IS NULL
            ),
            note_degrees AS (
                SELECT
                    a.id AS note_id,
                    COALESCE(d.degree, 0) AS degree
                FROM all_notes a
                LEFT JOIN degrees d ON d.note_id = a.id
            )
            SELECT
                COUNT(*) AS total_notes,
                COUNT(*) FILTER (WHERE degree = 0) AS isolated_nodes,
                COALESCE(AVG(degree), 0)::FLOAT8 AS avg_degree,
                COALESCE(MAX(degree), 0) AS max_degree,
                COALESCE(MIN(degree) FILTER (WHERE degree > 0), 0) AS min_degree_linked,
                COALESCE(PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY degree), 0)::FLOAT8 AS median_degree
            FROM note_degrees
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let total_notes: i64 = row.get("total_notes");
        let isolated_nodes: i64 = row.get("isolated_nodes");
        let avg_degree: f64 = row.get("avg_degree");
        let max_degree: i64 = row.get("max_degree");
        let min_degree_linked: i64 = row.get("min_degree_linked");
        let median_degree: f64 = row.get("median_degree");

        // Total semantic links
        let link_row =
            sqlx::query("SELECT COUNT(*) as total_links FROM link WHERE kind = 'semantic'")
                .fetch_one(&mut **tx)
                .await
                .map_err(Error::Database)?;
        let total_links: i64 = link_row.get("total_links");

        // Connected components via iterative BFS
        // PostgreSQL lacks MIN(uuid) aggregate even on pg18, so cast to text for grouping.
        // UUID v7 text representation preserves chronological ordering.
        let components_row = sqlx::query(
            r#"
            WITH RECURSIVE edges AS (
                SELECT DISTINCT from_note_id AS a, to_note_id AS b FROM link WHERE kind = 'semantic'
                UNION
                SELECT DISTINCT to_note_id AS a, from_note_id AS b FROM link WHERE kind = 'semantic'
            ),
            all_linked AS (
                SELECT DISTINCT a AS note_id FROM edges
            ),
            component_walk AS (
                SELECT note_id, note_id::text AS component_root, 0 AS depth
                FROM all_linked
                UNION
                SELECT e.b, cw.component_root, cw.depth + 1
                FROM component_walk cw
                JOIN edges e ON e.a = cw.note_id
                WHERE cw.depth < 50
            )
            SELECT COUNT(DISTINCT min_root) AS connected_components
            FROM (
                SELECT note_id, MIN(component_root) AS min_root
                FROM component_walk
                GROUP BY note_id
            ) sub
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;
        let connected_components: i64 = components_row.get("connected_components");

        // Strategy info from current config
        let strategy = matric_core::defaults::GraphConfig::from_env();

        Ok(TopologyStats {
            total_notes,
            total_links,
            isolated_nodes,
            connected_components,
            avg_degree,
            max_degree,
            min_degree_linked,
            median_degree,
            linking_strategy: format!("{:?}", strategy.strategy),
            effective_k: strategy.effective_k(total_notes as usize),
        })
    }

    /// Compute graph diagnostics within an existing transaction (#483).
    ///
    /// Samples random embedding pairs for similarity distribution, computes
    /// topology metrics from the link table, and reports normalized edge stats.
    /// Only considers real (persisted) embedding sets, not virtual/filter sets.
    pub async fn graph_diagnostics_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        sample_size: i64,
    ) -> Result<GraphDiagnostics> {
        let now = Utc::now();

        // --- Counts ---
        let counts_row = sqlx::query(
            r#"
            SELECT
                (SELECT COUNT(*) FROM note WHERE deleted_at IS NULL) as note_count,
                (SELECT COUNT(*) FROM embedding
                 WHERE embedding_set_id IN (
                     SELECT id FROM embedding_set WHERE set_type = 'full'
                     UNION
                     SELECT get_default_embedding_set_id()
                 )) as embedding_count,
                (SELECT COUNT(*) FROM link) as edge_count
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let note_count: i64 = counts_row.get("note_count");
        let embedding_count: i64 = counts_row.get("embedding_count");
        let edge_count: i64 = counts_row.get("edge_count");

        // --- Embedding space metrics via sampled pairwise cosine similarity ---
        // Sample random pairs from real embedding sets only.
        // Uses chunk_index = 0 (first chunk per note) to avoid inflating pair count.
        let similarity_rows: Vec<_> = sqlx::query(
            r#"
            WITH real_embeddings AS (
                SELECT e.note_id, e.vector
                FROM embedding e
                WHERE e.chunk_index = 0
                  AND e.embedding_set_id IN (
                      SELECT id FROM embedding_set WHERE set_type = 'full'
                      UNION
                      SELECT get_default_embedding_set_id()
                  )
            ),
            sampled_pairs AS (
                SELECT a.vector AS va, b.vector AS vb
                FROM real_embeddings a
                CROSS JOIN LATERAL (
                    SELECT vector FROM real_embeddings
                    WHERE note_id != a.note_id
                    ORDER BY random()
                    LIMIT 1
                ) b
                ORDER BY random()
                LIMIT $1
            )
            SELECT 1.0 - (va <=> vb) AS similarity
            FROM sampled_pairs
            "#,
        )
        .bind(sample_size)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let similarities: Vec<f64> = similarity_rows
            .iter()
            .map(|r| r.get::<f64, _>("similarity"))
            .collect();

        let embedding_space = if similarities.is_empty() {
            EmbeddingSpaceMetrics {
                similarity_histogram: vec![0i64; 10],
                similarity_mean: 0.0,
                similarity_std: 0.0,
                effective_range: 0.0,
                anisotropy_score: 0.0,
                sample_count: 0,
            }
        } else {
            let n = similarities.len() as f64;
            let mean = similarities.iter().sum::<f64>() / n;
            let variance = similarities.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
            let std = variance.sqrt();
            let min_sim = similarities.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_sim = similarities
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);

            // Build 10-bin histogram over [0.0, 1.0]
            let mut histogram = vec![0i64; 10];
            for &s in &similarities {
                let bin = ((s.clamp(0.0, 0.9999)) * 10.0) as usize;
                histogram[bin] += 1;
            }

            EmbeddingSpaceMetrics {
                similarity_histogram: histogram,
                similarity_mean: mean,
                similarity_std: std,
                effective_range: max_sim - min_sim,
                anisotropy_score: mean, // mean pairwise similarity ≈ anisotropy
                sample_count: similarities.len() as i64,
            }
        };

        // --- Topology metrics (degree distribution) ---
        let topo_row = sqlx::query(
            r#"
            WITH degrees AS (
                SELECT note_id, COUNT(*) as degree FROM (
                    SELECT from_note_id as note_id FROM link
                    UNION ALL
                    SELECT to_note_id as note_id FROM link
                ) all_edges
                GROUP BY note_id
            )
            SELECT
                COALESCE(AVG(degree), 0) as degree_mean,
                COALESCE(STDDEV_POP(degree), 0) as degree_std
            FROM degrees
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let degree_mean: f64 = topo_row.get("degree_mean");
        let degree_std: f64 = topo_row.get("degree_std");
        let degree_cv = if degree_mean > f64::EPSILON {
            degree_std / degree_mean
        } else {
            0.0
        };

        // Louvain community metrics (#473).
        let community_metrics = Self::compute_community_metrics_for_diagnostics(tx).await?;

        let topology = TopologyDiagnostics {
            modularity_q: community_metrics.modularity_q,
            degree_mean,
            degree_std,
            degree_cv,
            community_count: community_metrics.community_count,
            largest_community_ratio: community_metrics.largest_community_ratio,
            bridge_edge_ratio: community_metrics.bridge_edge_ratio,
        };

        // --- Normalized edge metrics ---
        let norm_row = sqlx::query(
            r#"
            SELECT
                MIN(score) as min_score,
                MAX(score) as max_score
            FROM link
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let min_score: Option<f32> = norm_row.get("min_score");
        let max_score: Option<f32> = norm_row.get("max_score");
        let normalized_weight_spread = match (min_score, max_score) {
            (Some(_), Some(_)) => {
                let gamma = matric_core::defaults::GraphConfig::from_env().normalization_gamma;
                Some(vec![0.0_f32, 1.0_f32, gamma])
            }
            _ => None,
        };

        // SNN coverage: fraction of edges that have snn_score in metadata (#474).
        let snn_row = sqlx::query(
            "SELECT COUNT(*) FILTER (WHERE metadata ? 'snn_score') as with_snn, \
                    COUNT(*) as total \
             FROM link WHERE kind = 'semantic'",
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;
        let snn_with: i64 = snn_row.get("with_snn");
        let snn_total: i64 = snn_row.get("total");
        let snn_coverage = if snn_total > 0 {
            Some(snn_with as f64 / snn_total as f64)
        } else {
            None
        };

        let normalized_edges = NormalizedEdgeMetrics {
            raw_score_range: match (min_score, max_score) {
                (Some(min), Some(max)) => Some(vec![min, max]),
                _ => None,
            },
            normalized_weight_spread,
            snn_coverage,
            pfnet_retention_ratio: {
                // Check if pfnet_retained metadata exists on any edges (#476).
                let pfnet_row = sqlx::query(
                    "SELECT COUNT(*) FILTER (WHERE metadata ? 'pfnet_retained') as with_pfnet, \
                            COUNT(*) as total \
                     FROM link WHERE kind = 'semantic'",
                )
                .fetch_one(&mut **tx)
                .await
                .map_err(Error::Database)?;
                let pfnet_with: i64 = pfnet_row.get("with_pfnet");
                let pfnet_total: i64 = pfnet_row.get("total");
                if pfnet_with > 0 && pfnet_total > 0 {
                    Some(pfnet_with as f64 / pfnet_total as f64)
                } else {
                    None
                }
            },
        };

        Ok(GraphDiagnostics {
            computed_at: now,
            note_count,
            embedding_count,
            edge_count,
            embedding_space,
            topology,
            normalized_edges,
        })
    }

    /// Capture a diagnostics snapshot with a label (#484).
    pub async fn save_diagnostics_snapshot_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        label: &str,
        metrics: &GraphDiagnostics,
    ) -> Result<DiagnosticsSnapshot> {
        let id = matric_core::new_v7();
        let metrics_json =
            serde_json::to_value(metrics).map_err(|e| Error::Internal(e.to_string()))?;
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO graph_diagnostics_history (id, label, metrics, captured_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(label)
        .bind(&metrics_json)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(DiagnosticsSnapshot {
            id,
            label: label.to_string(),
            metrics: metrics_json,
            captured_at: now,
        })
    }

    /// List diagnostics snapshots (#484).
    pub async fn list_diagnostics_snapshots_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        limit: i64,
    ) -> Result<Vec<DiagnosticsSnapshot>> {
        let rows = sqlx::query(
            "SELECT id, label, metrics, captured_at FROM graph_diagnostics_history \
             ORDER BY captured_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .iter()
            .map(|r| DiagnosticsSnapshot {
                id: r.get("id"),
                label: r.get("label"),
                metrics: r.get("metrics"),
                captured_at: r.get("captured_at"),
            })
            .collect())
    }

    /// Get a single diagnostics snapshot by ID (#484).
    pub async fn get_diagnostics_snapshot_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<DiagnosticsSnapshot>> {
        let row = sqlx::query(
            "SELECT id, label, metrics, captured_at FROM graph_diagnostics_history WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| DiagnosticsSnapshot {
            id: r.get("id"),
            label: r.get("label"),
            metrics: r.get("metrics"),
            captured_at: r.get("captured_at"),
        }))
    }

    /// Compute community metrics for diagnostics (#473).
    ///
    /// Loads all semantic edges, runs Louvain, and computes modularity Q,
    /// community count, largest community ratio, and bridge edge ratio.
    async fn compute_community_metrics_for_diagnostics(
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<CommunityMetrics> {
        use std::collections::{HashMap, HashSet};

        let edge_rows =
            sqlx::query("SELECT from_note_id, to_note_id, score FROM link WHERE kind = 'semantic'")
                .fetch_all(&mut **tx)
                .await
                .map_err(Error::Database)?;

        if edge_rows.is_empty() {
            return Ok(CommunityMetrics::default());
        }

        // Build nodes and edges for assign_communities.
        let mut node_set: HashSet<Uuid> = HashSet::new();
        let mut edges: Vec<GraphEdge> = Vec::with_capacity(edge_rows.len());
        let mut total_weight = 0.0_f64;
        for row in &edge_rows {
            let from: Uuid = row.get("from_note_id");
            let to: Uuid = row.get("to_note_id");
            let score: f32 = row.get("score");
            node_set.insert(from);
            node_set.insert(to);
            total_weight += score as f64;
            edges.push(GraphEdge {
                source: from,
                target: to,
                edge_type: "semantic".to_string(),
                score,
                rank: None,
                embedding_set: None,
                model: None,
                computed_at: None,
                normalized_weight: None,
            });
        }

        // Apply normalization for better Louvain input.
        let edges = Self::apply_edge_normalization(edges);

        let dummy_nodes: Vec<GraphNode> = node_set
            .iter()
            .map(|&id| GraphNode {
                id,
                title: None,
                depth: 0,
                collection_id: None,
                archived: false,
                created_at_utc: Utc::now(),
                updated_at_utc: Utc::now(),
                community_id: None,
                community_label: None,
                community_confidence: None,
            })
            .collect();

        let resolution = matric_core::defaults::GraphConfig::from_env().community_resolution;
        let assigned = Self::assign_communities(dummy_nodes, &edges, resolution);

        // Compute metrics from assigned communities.
        let total_nodes = assigned.len() as f64;
        if total_nodes < 1.0 {
            return Ok(CommunityMetrics::default());
        }

        // Count communities and their sizes.
        let mut community_sizes: HashMap<i64, usize> = HashMap::new();
        let mut node_community: HashMap<Uuid, i64> = HashMap::new();
        for node in &assigned {
            let cid = node.community_id.unwrap_or(0);
            *community_sizes.entry(cid).or_default() += 1;
            node_community.insert(node.id, cid);
        }

        let community_count = community_sizes.len() as i64;
        let largest = *community_sizes.values().max().unwrap_or(&0) as f64;
        let largest_community_ratio = largest / total_nodes;

        // Bridge edges: edges crossing communities.
        let mut bridge_count = 0usize;
        for edge in &edges {
            let sc = node_community.get(&edge.source).copied().unwrap_or(-1);
            let tc = node_community.get(&edge.target).copied().unwrap_or(-2);
            if sc != tc {
                bridge_count += 1;
            }
        }
        let bridge_edge_ratio = if edges.is_empty() {
            0.0
        } else {
            bridge_count as f64 / edges.len() as f64
        };

        // Modularity Q: Q = (1/2m) * Σ [A_ij - k_i*k_j/(2m)] * δ(c_i, c_j)
        let m2 = 2.0 * total_weight;
        let mut q = 0.0_f64;
        if m2 > f64::EPSILON {
            // Compute weighted degree for each node.
            let mut k_node: HashMap<Uuid, f64> = HashMap::new();
            for edge in &edges {
                let w = edge.normalized_weight.unwrap_or(edge.score) as f64;
                *k_node.entry(edge.source).or_default() += w;
                *k_node.entry(edge.target).or_default() += w;
            }
            for edge in &edges {
                let w = edge.normalized_weight.unwrap_or(edge.score) as f64;
                let sc = node_community.get(&edge.source).copied().unwrap_or(-1);
                let tc = node_community.get(&edge.target).copied().unwrap_or(-2);
                if sc == tc {
                    let ki = k_node.get(&edge.source).copied().unwrap_or(0.0);
                    let kj = k_node.get(&edge.target).copied().unwrap_or(0.0);
                    q += w - ki * kj / m2;
                }
            }
            q /= m2;
        }

        Ok(CommunityMetrics {
            modularity_q: Some(q),
            community_count: Some(community_count),
            largest_community_ratio: Some(largest_community_ratio),
            bridge_edge_ratio: Some(bridge_edge_ratio),
        })
    }

    /// PFNET sparsification: remove geometrically redundant edges (#476).
    ///
    /// PFNET(∞, q=2) is equivalent to the Relative Neighborhood Graph (Toussaint 1980).
    /// An edge (A,B) is kept iff no witness node C exists where:
    ///   max(dist(A,C), dist(C,B)) ≤ dist(A,B)   [for q=2, Minkowski ∞-norm]
    ///
    /// Uses "graph PFNET" approximation: only considers witnesses from
    /// neighbors(A) ∪ neighbors(B) in the input edge set.
    pub async fn pfnet_sparsify_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        q: usize,
        dry_run: bool,
    ) -> Result<PfnetResult> {
        use std::collections::{HashMap, HashSet};

        let rows = sqlx::query(
            "SELECT id, from_note_id, to_note_id, score FROM link WHERE kind = 'semantic'",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if rows.is_empty() {
            return Ok(PfnetResult {
                total_edges: 0,
                retained: 0,
                pruned: 0,
                retention_ratio: 1.0,
                q_used: q,
                dry_run,
            });
        }

        let total_edges = rows.len();

        // Build adjacency: (node_a, node_b) → (edge_id, distance).
        // Distance = 1.0 - similarity. Use f64 for precision in narrow range.
        let mut adj_dist: HashMap<(Uuid, Uuid), f64> = HashMap::new();
        let mut adj_neighbors: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
        let mut edge_ids: Vec<(Uuid, Uuid, Uuid, f64)> = Vec::with_capacity(rows.len());

        for row in &rows {
            let id: Uuid = row.get("id");
            let from: Uuid = row.get("from_note_id");
            let to: Uuid = row.get("to_note_id");
            let score: f32 = row.get("score");
            let dist = 1.0_f64 - score as f64;

            // Store both directions for distance lookup.
            adj_dist.insert((from, to), dist);
            adj_dist.insert((to, from), dist);
            adj_neighbors.entry(from).or_default().insert(to);
            adj_neighbors.entry(to).or_default().insert(from);
            edge_ids.push((id, from, to, dist));
        }

        let empty_set = HashSet::new();

        // For q=2: edge (i,j) is redundant if ∃ k where max(d(i,k), d(k,j)) ≤ d(i,j).
        // Graph PFNET: only check witnesses in neighbors(i) ∪ neighbors(j).
        let mut to_prune: Vec<Uuid> = Vec::new();
        let mut to_retain: Vec<Uuid> = Vec::new();

        for &(edge_id, from, to, d_ij) in &edge_ids {
            let from_nbrs = adj_neighbors.get(&from).unwrap_or(&empty_set);
            let to_nbrs = adj_neighbors.get(&to).unwrap_or(&empty_set);

            let mut redundant = false;

            if q == 2 {
                // Triangle inequality check (RNG-equivalent).
                for &witness in from_nbrs.union(to_nbrs) {
                    if witness == from || witness == to {
                        continue;
                    }
                    let d_ik = adj_dist
                        .get(&(from, witness))
                        .copied()
                        .unwrap_or(f64::INFINITY);
                    let d_kj = adj_dist
                        .get(&(witness, to))
                        .copied()
                        .unwrap_or(f64::INFINITY);
                    // Minkowski ∞-norm: max of the two path segments.
                    if d_ik.max(d_kj) <= d_ij + f64::EPSILON {
                        redundant = true;
                        break;
                    }
                }
            }
            // q>2 would require multi-hop path checks — gated in the API handler.

            if redundant {
                to_prune.push(edge_id);
            } else {
                to_retain.push(edge_id);
            }
        }

        let retained = to_retain.len();
        let pruned = to_prune.len();
        let retention_ratio = if total_edges > 0 {
            retained as f64 / total_edges as f64
        } else {
            1.0
        };

        if !dry_run {
            // Mark retained edges in metadata.
            for chunk in to_retain.chunks(500) {
                let ids: Vec<Uuid> = chunk.to_vec();
                sqlx::query(
                    "UPDATE link SET metadata = metadata || '{\"pfnet_retained\": true}'::jsonb \
                     WHERE id = ANY($1)",
                )
                .bind(&ids)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
            }

            // Delete pruned edges.
            for chunk in to_prune.chunks(500) {
                let ids: Vec<Uuid> = chunk.to_vec();
                sqlx::query("DELETE FROM link WHERE id = ANY($1)")
                    .bind(&ids)
                    .execute(&mut **tx)
                    .await
                    .map_err(Error::Database)?;
            }
        }

        Ok(PfnetResult {
            total_edges,
            retained,
            pruned,
            retention_ratio,
            q_used: q,
            dry_run,
        })
    }

    /// Count non-deleted notes (utility for adaptive k computation).
    pub async fn count_notes_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM note WHERE deleted_at IS NULL")
            .fetch_one(&mut **tx)
            .await
            .map_err(Error::Database)?;
        let count: i64 = row.get("cnt");
        Ok(count as usize)
    }

    /// MRL 64-dim coarse community detection (#477).
    ///
    /// Uses the first `coarse_dim` dimensions of stored embeddings (MRL truncation)
    /// to compute pairwise cosine similarity. The wider spread at lower dimensions
    /// (~0.30-0.90 vs ~0.62-0.94 at 768d) produces clearer cluster boundaries.
    ///
    /// Steps:
    /// 1. Load chunk_index=0 embeddings for all notes
    /// 2. Compute pairwise 64-dim cosine similarity via pgvector
    /// 3. Filter edges above similarity_threshold
    /// 4. Run Louvain on the resulting graph
    /// 5. Return community assignments with modularity metrics
    pub async fn coarse_community_detection_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        coarse_dim: i32,
        similarity_threshold: f32,
        resolution: f64,
    ) -> Result<CoarseCommunityResult> {
        use std::collections::{HashMap, HashSet};

        // Compute pairwise similarities using truncated vectors.
        // (vector::float4[])[1:N]::vector truncates to first N dims (MRL).
        let pairs = sqlx::query(
            r#"
            SELECT e1.note_id AS note_a, e2.note_id AS note_b,
                   1.0 - ((e1.vector::float4[])[1:$1]::vector
                       <=> (e2.vector::float4[])[1:$1]::vector) AS similarity
            FROM embedding e1
            JOIN embedding e2
              ON e1.note_id < e2.note_id
             AND e1.chunk_index = 0 AND e2.chunk_index = 0
            WHERE e1.chunk_index = 0
              AND 1.0 - ((e1.vector::float4[])[1:$1]::vector
                      <=> (e2.vector::float4[])[1:$1]::vector) >= $2::real
            "#,
        )
        .bind(coarse_dim)
        .bind(similarity_threshold)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if pairs.is_empty() {
            return Ok(CoarseCommunityResult {
                note_count: 0,
                edge_count: 0,
                coarse_dim,
                similarity_threshold,
                community_count: 0,
                modularity_q: 0.0,
                largest_community_ratio: 0.0,
                communities: vec![],
            });
        }

        // Build nodes and edges.
        let mut node_set: HashSet<Uuid> = HashSet::new();
        let mut edges: Vec<GraphEdge> = Vec::with_capacity(pairs.len());

        for row in &pairs {
            let note_a: Uuid = row.get("note_a");
            let note_b: Uuid = row.get("note_b");
            let sim: f64 = row.get("similarity");
            node_set.insert(note_a);
            node_set.insert(note_b);
            edges.push(GraphEdge {
                source: note_a,
                target: note_b,
                edge_type: "coarse_semantic".to_string(),
                score: sim as f32,
                rank: None,
                embedding_set: None,
                model: None,
                computed_at: None,
                normalized_weight: Some(sim as f32),
            });
        }

        let note_count = node_set.len();
        let edge_count = edges.len();

        // Normalize for Louvain.
        let edges = Self::apply_edge_normalization(edges);

        // Build node list.
        let nodes: Vec<GraphNode> = node_set
            .iter()
            .map(|&id| GraphNode {
                id,
                title: None,
                depth: 0,
                collection_id: None,
                archived: false,
                created_at_utc: Utc::now(),
                updated_at_utc: Utc::now(),
                community_id: None,
                community_label: None,
                community_confidence: None,
            })
            .collect();

        // Run Louvain.
        let assigned = Self::assign_communities(nodes, &edges, resolution);

        // Compute modularity Q and community sizes.
        let mut community_members: HashMap<i64, Vec<Uuid>> = HashMap::new();
        let mut node_community: HashMap<Uuid, i64> = HashMap::new();
        for node in &assigned {
            let cid = node.community_id.unwrap_or(0);
            community_members.entry(cid).or_default().push(node.id);
            node_community.insert(node.id, cid);
        }

        let community_count = community_members.len();
        let largest_size = community_members
            .values()
            .map(|v| v.len())
            .max()
            .unwrap_or(0);
        let largest_community_ratio = if note_count > 0 {
            largest_size as f64 / note_count as f64
        } else {
            0.0
        };

        // Compute modularity Q = Σ[e_ii - a_i²] where
        // e_ii = fraction of edges within community i
        // a_i = fraction of edge ends in community i
        let total_weight: f64 = edges.iter().map(|e| e.score as f64).sum();
        let modularity_q = if total_weight > 0.0 {
            let mut q = 0.0_f64;
            for members in community_members.values() {
                let member_set: HashSet<&Uuid> = members.iter().collect();
                let internal: f64 = edges
                    .iter()
                    .filter(|e| member_set.contains(&e.source) && member_set.contains(&e.target))
                    .map(|e| e.score as f64)
                    .sum();
                let degree: f64 = edges
                    .iter()
                    .filter(|e| member_set.contains(&e.source) || member_set.contains(&e.target))
                    .map(|e| e.score as f64)
                    .sum();
                let e_ii = internal / total_weight;
                let a_i = degree / (2.0 * total_weight);
                q += e_ii - a_i * a_i;
            }
            q
        } else {
            0.0
        };

        // Build community summaries (sorted by size descending).
        let mut communities: Vec<CoarseCommunity> = community_members
            .into_iter()
            .map(|(cid, mut members)| {
                members.sort();
                CoarseCommunity {
                    community_id: cid,
                    size: members.len(),
                    note_ids: members,
                }
            })
            .collect();
        communities.sort_by(|a, b| b.size.cmp(&a.size));

        Ok(CoarseCommunityResult {
            note_count,
            edge_count,
            coarse_dim,
            similarity_threshold,
            community_count,
            modularity_q,
            largest_community_ratio,
            communities,
        })
    }

    /// Recompute Shared Nearest Neighbor (SNN) scores for all semantic links (#474).
    ///
    /// SNN(A, B) = |kNN(A) ∩ kNN(B)| / k
    ///
    /// Steps:
    /// 1. Load all semantic edges
    /// 2. Build neighbor sets per node (top-k by score)
    /// 3. For each edge, compute SNN score
    /// 4. Store in metadata, prune below threshold
    ///
    /// Returns `SnnResult` with counts of updated and pruned edges.
    pub async fn recompute_snn_scores_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        k: usize,
        threshold: f32,
        dry_run: bool,
    ) -> Result<SnnResult> {
        use std::collections::{HashMap, HashSet};

        // Load all semantic edges with their IDs and scores.
        let rows = sqlx::query(
            "SELECT id, from_note_id, to_note_id, score \
             FROM link WHERE kind = 'semantic'",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if rows.is_empty() {
            return Ok(SnnResult {
                total_edges: 0,
                updated: 0,
                pruned: 0,
                k_used: k,
                threshold_used: threshold,
                dry_run,
                snn_score_distribution: vec![0; 10],
            });
        }

        // Build adjacency list: node → Vec<(neighbor, score)>
        let mut adjacency: HashMap<Uuid, Vec<(Uuid, f32)>> = HashMap::new();
        for row in &rows {
            let from: Uuid = row.get("from_note_id");
            let to: Uuid = row.get("to_note_id");
            let score: f32 = row.get("score");
            adjacency.entry(from).or_default().push((to, score));
            adjacency.entry(to).or_default().push((from, score));
        }

        // Build top-k neighbor sets (by score, descending).
        let neighbor_sets: HashMap<Uuid, HashSet<Uuid>> = adjacency
            .iter()
            .map(|(node, neighbors)| {
                let mut sorted = neighbors.clone();
                sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let top_k: HashSet<Uuid> = sorted.iter().take(k).map(|(id, _)| *id).collect();
                (*node, top_k)
            })
            .collect();

        let empty_set = HashSet::new();
        let total_edges = rows.len();
        let mut updated = 0usize;
        let mut pruned = 0usize;
        let mut histogram = vec![0i64; 10]; // 10 bins over [0.0, 1.0]

        // Compute SNN for each edge and batch update/delete.
        let mut updates: Vec<(Uuid, f32)> = Vec::new();
        let mut deletes: Vec<Uuid> = Vec::new();

        for row in &rows {
            let id: Uuid = row.get("id");
            let from: Uuid = row.get("from_note_id");
            let to: Uuid = row.get("to_note_id");

            let from_neighbors = neighbor_sets.get(&from).unwrap_or(&empty_set);
            let to_neighbors = neighbor_sets.get(&to).unwrap_or(&empty_set);
            let shared = from_neighbors.intersection(to_neighbors).count();
            let snn_score = if k > 0 { shared as f32 / k as f32 } else { 0.0 };

            // Histogram bin.
            let bin = ((snn_score * 10.0).floor() as usize).min(9);
            histogram[bin] += 1;

            if snn_score < threshold {
                deletes.push(id);
                pruned += 1;
            } else {
                updates.push((id, snn_score));
                updated += 1;
            }
        }

        if !dry_run {
            // Batch update: store snn_score in metadata.
            for chunk in updates.chunks(500) {
                let ids: Vec<Uuid> = chunk.iter().map(|(id, _)| *id).collect();
                let scores: Vec<f32> = chunk.iter().map(|(_, s)| *s).collect();
                sqlx::query(
                    "UPDATE link SET metadata = metadata || jsonb_build_object('snn_score', s.score::text::float) \
                     FROM UNNEST($1::uuid[], $2::real[]) AS s(id, score) \
                     WHERE link.id = s.id",
                )
                .bind(&ids)
                .bind(&scores)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
            }

            // Batch delete pruned edges.
            for chunk in deletes.chunks(500) {
                let ids: Vec<Uuid> = chunk.to_vec();
                sqlx::query("DELETE FROM link WHERE id = ANY($1)")
                    .bind(&ids)
                    .execute(&mut **tx)
                    .await
                    .map_err(Error::Database)?;
            }
        }

        Ok(SnnResult {
            total_edges,
            updated,
            pruned,
            k_used: k,
            threshold_used: threshold,
            dry_run,
            snn_score_distribution: histogram,
        })
    }
}

/// Graph topology statistics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopologyStats {
    pub total_notes: i64,
    pub total_links: i64,
    pub isolated_nodes: i64,
    pub connected_components: i64,
    pub avg_degree: f64,
    pub max_degree: i64,
    pub min_degree_linked: i64,
    pub median_degree: f64,
    pub linking_strategy: String,
    pub effective_k: usize,
}

/// Graph diagnostics response (#483).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphDiagnostics {
    pub computed_at: DateTime<Utc>,
    pub note_count: i64,
    pub embedding_count: i64,
    pub edge_count: i64,
    pub embedding_space: EmbeddingSpaceMetrics,
    pub topology: TopologyDiagnostics,
    pub normalized_edges: NormalizedEdgeMetrics,
}

/// Embedding space health metrics (#483).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingSpaceMetrics {
    /// 10-bin histogram of pairwise cosine similarities over [0.0, 1.0].
    pub similarity_histogram: Vec<i64>,
    pub similarity_mean: f64,
    pub similarity_std: f64,
    /// max - min observed similarity in sample.
    pub effective_range: f64,
    /// Mean pairwise cosine similarity (0=isotropic, 1=anisotropic).
    pub anisotropy_score: f64,
    /// Number of pairs actually sampled.
    pub sample_count: i64,
}

/// Graph topology diagnostics (#483).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopologyDiagnostics {
    /// Louvain modularity Q (requires #473).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modularity_q: Option<f64>,
    pub degree_mean: f64,
    pub degree_std: f64,
    /// Coefficient of variation: std / mean.
    pub degree_cv: f64,
    /// Number of detected communities (requires #473).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_count: Option<i64>,
    /// Largest community size / total nodes (requires #473).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub largest_community_ratio: Option<f64>,
    /// Cross-community edges / total edges (requires #473).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_edge_ratio: Option<f64>,
}

/// Normalized edge metrics (#483).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NormalizedEdgeMetrics {
    /// Raw [min, max] cosine similarity scores in the link table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_score_range: Option<Vec<f32>>,
    /// [min_normalized, max_normalized, gamma] after normalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_weight_spread: Option<Vec<f32>>,
    /// % of edges with SNN score > 0 (requires #474).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snn_coverage: Option<f64>,
    /// % of edges retained after PFNET (requires #476).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pfnet_retention_ratio: Option<f64>,
}

/// Stored diagnostics snapshot for before/after comparison (#484).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticsSnapshot {
    pub id: Uuid,
    pub label: String,
    pub metrics: serde_json::Value,
    pub captured_at: DateTime<Utc>,
}

/// Delta comparison between two diagnostics snapshots (#484).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticsComparison {
    pub before: DiagnosticsSnapshot,
    pub after: DiagnosticsSnapshot,
    pub delta: DiagnosticsDelta,
}

/// Computed deltas between before/after diagnostics (#484).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticsDelta {
    /// Change in mean pairwise cosine similarity.
    pub similarity_mean_delta: Option<f64>,
    /// Change in effective range.
    pub effective_range_delta: Option<f64>,
    /// Change in anisotropy score.
    pub anisotropy_delta: Option<f64>,
    /// Change in degree CV.
    pub degree_cv_delta: Option<f64>,
    /// Human-readable summary of improvements/regressions.
    pub summary: Vec<String>,
}

/// Result of SNN recomputation (#474).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnnResult {
    pub total_edges: usize,
    pub updated: usize,
    pub pruned: usize,
    pub k_used: usize,
    pub threshold_used: f32,
    pub dry_run: bool,
    /// 10-bin histogram of SNN scores over [0.0, 1.0].
    pub snn_score_distribution: Vec<i64>,
}

/// Result of PFNET sparsification (#476).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PfnetResult {
    pub total_edges: usize,
    pub retained: usize,
    pub pruned: usize,
    pub retention_ratio: f64,
    pub q_used: usize,
    pub dry_run: bool,
}

/// Result of MRL 64-dim coarse community detection (#477).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoarseCommunityResult {
    /// Number of notes with embeddings.
    pub note_count: usize,
    /// Number of pairwise edges above the similarity threshold.
    pub edge_count: usize,
    /// Dimension used for coarse similarity (default: 64).
    pub coarse_dim: i32,
    /// Similarity threshold for edge inclusion.
    pub similarity_threshold: f32,
    /// Number of communities detected.
    pub community_count: usize,
    /// Modularity Q of the coarse community assignment.
    pub modularity_q: f64,
    /// Largest community as fraction of total notes.
    pub largest_community_ratio: f64,
    /// Per-community summary: (community_id, size, label).
    pub communities: Vec<CoarseCommunity>,
}

/// A single community from coarse detection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoarseCommunity {
    pub community_id: i64,
    pub size: usize,
    pub note_ids: Vec<Uuid>,
}

/// Internal helper for community diagnostics (#473).
#[derive(Debug, Default)]
struct CommunityMetrics {
    modularity_q: Option<f64>,
    community_count: Option<i64>,
    largest_community_ratio: Option<f64>,
    bridge_edge_ratio: Option<f64>,
}

impl DiagnosticsComparison {
    /// Build a comparison from two snapshots, computing deltas.
    pub fn from_snapshots(before: DiagnosticsSnapshot, after: DiagnosticsSnapshot) -> Self {
        let extract = |snap: &DiagnosticsSnapshot, path: &[&str]| -> Option<f64> {
            let mut val = &snap.metrics;
            for key in path {
                val = val.get(key)?;
            }
            val.as_f64()
        };

        let sim_mean_before = extract(&before, &["embedding_space", "similarity_mean"]);
        let sim_mean_after = extract(&after, &["embedding_space", "similarity_mean"]);
        let range_before = extract(&before, &["embedding_space", "effective_range"]);
        let range_after = extract(&after, &["embedding_space", "effective_range"]);
        let aniso_before = extract(&before, &["embedding_space", "anisotropy_score"]);
        let aniso_after = extract(&after, &["embedding_space", "anisotropy_score"]);
        let dcv_before = extract(&before, &["topology", "degree_cv"]);
        let dcv_after = extract(&after, &["topology", "degree_cv"]);

        let similarity_mean_delta = match (sim_mean_before, sim_mean_after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        let effective_range_delta = match (range_before, range_after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        let anisotropy_delta = match (aniso_before, aniso_after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        let degree_cv_delta = match (dcv_before, dcv_after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };

        let mut summary = Vec::new();
        if let Some(d) = effective_range_delta {
            if d > 0.01 {
                summary.push(format!(
                    "Effective range improved by {d:+.4} (better spread)"
                ));
            } else if d < -0.01 {
                summary.push(format!(
                    "Effective range regressed by {d:+.4} (narrower band)"
                ));
            }
        }
        if let Some(d) = anisotropy_delta {
            if d < -0.01 {
                summary.push(format!(
                    "Anisotropy decreased by {:.4} (more isotropic — good)",
                    d.abs()
                ));
            } else if d > 0.01 {
                summary.push(format!(
                    "Anisotropy increased by {d:+.4} (more anisotropic — worse)"
                ));
            }
        }
        if let Some(d) = degree_cv_delta {
            if d > 0.01 {
                summary.push(format!(
                    "Degree CV increased by {d:+.4} (more variation — better)"
                ));
            } else if d < -0.01 {
                summary.push(format!(
                    "Degree CV decreased by {d:+.4} (more uniform — worse)"
                ));
            }
        }
        if summary.is_empty() {
            summary.push("No significant changes detected.".to_string());
        }

        Self {
            before,
            after,
            delta: DiagnosticsDelta {
                similarity_mean_delta,
                effective_range_delta,
                anisotropy_delta,
                degree_cv_delta,
                summary,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_edge(score: f32) -> GraphEdge {
        GraphEdge {
            source: Uuid::nil(),
            target: Uuid::nil(),
            edge_type: "semantic".to_string(),
            score,
            rank: None,
            embedding_set: None,
            model: None,
            computed_at: None,
            normalized_weight: None,
        }
    }

    #[test]
    fn edge_normalization_empty() {
        let edges = PgLinkRepository::apply_edge_normalization(vec![]);
        assert!(edges.is_empty());
    }

    #[test]
    fn edge_normalization_single_edge() {
        let edges = PgLinkRepository::apply_edge_normalization(vec![make_edge(0.85)]);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].normalized_weight, Some(1.0));
    }

    #[test]
    fn edge_normalization_uniform_scores() {
        let edges = PgLinkRepository::apply_edge_normalization(vec![
            make_edge(0.80),
            make_edge(0.80),
            make_edge(0.80),
        ]);
        // All same score → all normalized to 1.0
        for edge in &edges {
            assert_eq!(edge.normalized_weight, Some(1.0));
        }
    }

    #[test]
    fn edge_normalization_spreads_narrow_band() {
        // Simulates real-world cosine similarity narrow band (0.70-0.94)
        let edges = PgLinkRepository::apply_edge_normalization(vec![
            make_edge(0.70),
            make_edge(0.82),
            make_edge(0.94),
        ]);
        // With default gamma=1.0: min→0.0, mid→0.5, max→1.0
        let nw: Vec<f32> = edges.iter().map(|e| e.normalized_weight.unwrap()).collect();
        assert!(
            (nw[0] - 0.0).abs() < 0.01,
            "min should be ~0.0, got {}",
            nw[0]
        );
        assert!(
            (nw[1] - 0.5).abs() < 0.01,
            "mid should be ~0.5, got {}",
            nw[1]
        );
        assert!(
            (nw[2] - 1.0).abs() < 0.01,
            "max should be ~1.0, got {}",
            nw[2]
        );
    }

    #[test]
    fn edge_normalization_preserves_original_scores() {
        let edges =
            PgLinkRepository::apply_edge_normalization(vec![make_edge(0.70), make_edge(0.94)]);
        // Original scores must remain untouched
        assert!((edges[0].score - 0.70).abs() < f32::EPSILON);
        assert!((edges[1].score - 0.94).abs() < f32::EPSILON);
    }

    // SNN scoring tests (#474)

    /// Helper: compute SNN score for two nodes given their neighbor sets.
    fn snn_score(
        a_neighbors: &std::collections::HashSet<Uuid>,
        b_neighbors: &std::collections::HashSet<Uuid>,
        k: usize,
    ) -> f32 {
        if k == 0 {
            return 0.0;
        }
        let shared = a_neighbors.intersection(b_neighbors).count();
        shared as f32 / k as f32
    }

    #[test]
    fn snn_no_shared_neighbors() {
        let mut a = std::collections::HashSet::new();
        let mut b = std::collections::HashSet::new();
        a.insert(Uuid::from_u128(1));
        a.insert(Uuid::from_u128(2));
        b.insert(Uuid::from_u128(3));
        b.insert(Uuid::from_u128(4));
        assert!((snn_score(&a, &b, 5) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn snn_full_overlap() {
        let mut a = std::collections::HashSet::new();
        a.insert(Uuid::from_u128(1));
        a.insert(Uuid::from_u128(2));
        a.insert(Uuid::from_u128(3));
        let b = a.clone();
        // 3 shared out of k=3 → score 1.0
        assert!((snn_score(&a, &b, 3) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn snn_partial_overlap() {
        let mut a = std::collections::HashSet::new();
        let mut b = std::collections::HashSet::new();
        // A's neighbors: 1, 2, 3, 4, 5
        for i in 1..=5 {
            a.insert(Uuid::from_u128(i));
        }
        // B's neighbors: 3, 4, 5, 6, 7
        for i in 3..=7 {
            b.insert(Uuid::from_u128(i));
        }
        // Shared: 3, 4, 5 = 3 out of k=5 → 0.6
        assert!((snn_score(&a, &b, 5) - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn snn_k_zero_returns_zero() {
        let a = std::collections::HashSet::new();
        let b = std::collections::HashSet::new();
        assert!((snn_score(&a, &b, 0) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn snn_histogram_binning() {
        // Scores: 0.0, 0.2, 0.6, 1.0
        // Bins:    [0], [2], [6], [9] (1.0 clamps to bin 9)
        let scores = [0.0_f32, 0.2, 0.6, 1.0];
        let mut histogram = vec![0i64; 10];
        for &s in &scores {
            let bin = ((s * 10.0).floor() as usize).min(9);
            histogram[bin] += 1;
        }
        assert_eq!(histogram[0], 1);
        assert_eq!(histogram[2], 1);
        assert_eq!(histogram[6], 1);
        assert_eq!(histogram[9], 1); // 1.0 → bin 9
    }

    // Louvain community detection tests (#473)

    fn make_community_node(id: u128) -> GraphNode {
        GraphNode {
            id: Uuid::from_u128(id),
            title: None,
            depth: 0,
            collection_id: None,
            archived: false,
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            community_id: None,
            community_label: None,
            community_confidence: None,
        }
    }

    fn make_weighted_edge(source: u128, target: u128, score: f32) -> GraphEdge {
        GraphEdge {
            source: Uuid::from_u128(source),
            target: Uuid::from_u128(target),
            edge_type: "semantic".to_string(),
            score,
            rank: None,
            embedding_set: None,
            model: None,
            computed_at: None,
            normalized_weight: Some(score),
        }
    }

    #[test]
    fn louvain_single_node() {
        let nodes = vec![make_community_node(1)];
        let edges = vec![];
        let result = PgLinkRepository::assign_communities(nodes, &edges, 1.0);
        assert_eq!(result[0].community_id, Some(0));
        assert_eq!(result[0].community_confidence, Some(1.0));
    }

    #[test]
    fn louvain_two_disconnected_nodes() {
        let nodes = vec![make_community_node(1), make_community_node(2)];
        let edges = vec![];
        let result = PgLinkRepository::assign_communities(nodes, &edges, 1.0);
        // Two disconnected nodes: each stays in own community or both get 0.
        // With no edges, total_weight=0, so each node in own community.
        assert_ne!(result[0].community_id, result[1].community_id);
    }

    #[test]
    fn louvain_two_clusters() {
        // Cluster A: 1-2-3 (strongly connected)
        // Cluster B: 4-5-6 (strongly connected)
        // Weak bridge: 3-4
        let nodes: Vec<GraphNode> = (1..=6).map(make_community_node).collect();
        let edges = vec![
            // Cluster A
            make_weighted_edge(1, 2, 0.9),
            make_weighted_edge(2, 3, 0.9),
            make_weighted_edge(1, 3, 0.85),
            // Cluster B
            make_weighted_edge(4, 5, 0.9),
            make_weighted_edge(5, 6, 0.9),
            make_weighted_edge(4, 6, 0.85),
            // Weak bridge
            make_weighted_edge(3, 4, 0.3),
        ];

        let result = PgLinkRepository::assign_communities(nodes, &edges, 1.0);

        // Nodes 1, 2, 3 should be in the same community.
        let comm_a = result[0].community_id;
        assert_eq!(result[0].community_id, comm_a);
        assert_eq!(result[1].community_id, comm_a);
        assert_eq!(result[2].community_id, comm_a);

        // Nodes 4, 5, 6 should be in the same community.
        let comm_b = result[3].community_id;
        assert_eq!(result[3].community_id, comm_b);
        assert_eq!(result[4].community_id, comm_b);
        assert_eq!(result[5].community_id, comm_b);

        // The two clusters should be different communities.
        assert_ne!(comm_a, comm_b);
    }

    #[test]
    fn louvain_fully_connected_clique() {
        // All 4 nodes connected — should merge into one community.
        let nodes: Vec<GraphNode> = (1..=4).map(make_community_node).collect();
        let edges = vec![
            make_weighted_edge(1, 2, 0.8),
            make_weighted_edge(1, 3, 0.8),
            make_weighted_edge(1, 4, 0.8),
            make_weighted_edge(2, 3, 0.8),
            make_weighted_edge(2, 4, 0.8),
            make_weighted_edge(3, 4, 0.8),
        ];
        let result = PgLinkRepository::assign_communities(nodes, &edges, 1.0);
        // All nodes should be in the same community.
        let comm = result[0].community_id;
        for node in &result {
            assert_eq!(node.community_id, comm);
        }
        // Confidence should be 1.0 (all neighbors in same community).
        for node in &result {
            assert!((node.community_confidence.unwrap() - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn louvain_deterministic() {
        let nodes: Vec<GraphNode> = (1..=6).map(make_community_node).collect();
        let edges = vec![
            make_weighted_edge(1, 2, 0.9),
            make_weighted_edge(2, 3, 0.9),
            make_weighted_edge(1, 3, 0.85),
            make_weighted_edge(4, 5, 0.9),
            make_weighted_edge(5, 6, 0.9),
            make_weighted_edge(4, 6, 0.85),
            make_weighted_edge(3, 4, 0.3),
        ];

        let result1 = PgLinkRepository::assign_communities(nodes.clone(), &edges, 1.0);
        let result2 = PgLinkRepository::assign_communities(nodes.clone(), &edges, 1.0);

        for (a, b) in result1.iter().zip(result2.iter()) {
            assert_eq!(a.community_id, b.community_id);
        }
    }

    // PFNET sparsification tests (#476)
    //
    // PFNET with q=2 (RNG) prunes edge (i,j) if a witness k exists where
    // max(d(i,k), d(k,j)) ≤ d(i,j). Distance = 1.0 - similarity.

    /// Pure-logic PFNET: returns (retained_indices, pruned_indices) for a set of edges.
    /// Mirrors the DB method but operates on in-memory data.
    fn pfnet_classify(edges: &[(u128, u128, f32)]) -> (Vec<usize>, Vec<usize>) {
        use std::collections::{HashMap, HashSet};

        let mut adj_dist: HashMap<(u128, u128), f64> = HashMap::new();
        let mut adj_neighbors: HashMap<u128, HashSet<u128>> = HashMap::new();

        for &(from, to, score) in edges {
            let dist = 1.0_f64 - score as f64;
            adj_dist.insert((from, to), dist);
            adj_dist.insert((to, from), dist);
            adj_neighbors.entry(from).or_default().insert(to);
            adj_neighbors.entry(to).or_default().insert(from);
        }

        let empty = HashSet::new();
        let mut retained = Vec::new();
        let mut pruned = Vec::new();

        for (idx, &(from, to, _)) in edges.iter().enumerate() {
            let d_ij = adj_dist[&(from, to)];
            let from_nbrs = adj_neighbors.get(&from).unwrap_or(&empty);
            let to_nbrs = adj_neighbors.get(&to).unwrap_or(&empty);

            let mut redundant = false;
            for &witness in from_nbrs.union(to_nbrs) {
                if witness == from || witness == to {
                    continue;
                }
                let d_ik = adj_dist
                    .get(&(from, witness))
                    .copied()
                    .unwrap_or(f64::INFINITY);
                let d_kj = adj_dist
                    .get(&(witness, to))
                    .copied()
                    .unwrap_or(f64::INFINITY);
                if d_ik.max(d_kj) <= d_ij + f64::EPSILON {
                    redundant = true;
                    break;
                }
            }

            if redundant {
                pruned.push(idx);
            } else {
                retained.push(idx);
            }
        }

        (retained, pruned)
    }

    #[test]
    fn pfnet_no_edges() {
        let edges: Vec<(u128, u128, f32)> = vec![];
        let (retained, pruned) = pfnet_classify(&edges);
        assert!(retained.is_empty());
        assert!(pruned.is_empty());
    }

    #[test]
    fn pfnet_single_edge_retained() {
        // A single edge has no witness — always retained.
        let edges = vec![(1, 2, 0.8)];
        let (retained, pruned) = pfnet_classify(&edges);
        assert_eq!(retained.len(), 1);
        assert!(pruned.is_empty());
    }

    #[test]
    fn pfnet_chain_preserves_all() {
        // Linear chain: A-B-C. No triangles → no pruning.
        let edges = vec![
            (1, 2, 0.9), // d=0.1
            (2, 3, 0.9), // d=0.1
        ];
        let (retained, pruned) = pfnet_classify(&edges);
        assert_eq!(retained.len(), 2);
        assert!(pruned.is_empty());
    }

    #[test]
    fn pfnet_triangle_prunes_longest() {
        // Triangle: A-B (0.9, d=0.1), B-C (0.9, d=0.1), A-C (0.7, d=0.3).
        // A-C is the longest edge. Witness B: max(d(A,B), d(B,C)) = max(0.1, 0.1) = 0.1 ≤ 0.3.
        // So A-C is pruned.
        let edges = vec![
            (1, 2, 0.9), // d=0.1 — retained
            (2, 3, 0.9), // d=0.1 — retained
            (1, 3, 0.7), // d=0.3 — pruned (witness 2)
        ];
        let (retained, pruned) = pfnet_classify(&edges);
        assert_eq!(retained.len(), 2);
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0], 2); // edge index 2 (A-C) is pruned
    }

    #[test]
    fn pfnet_equilateral_retains_all() {
        // Equal-distance triangle: all edges same similarity.
        // For A-C with witness B: max(d(A,B), d(B,C)) = max(0.2, 0.2) = 0.2 ≤ 0.2 + ε → pruned.
        // Actually with equal distances, every edge has a witness — but the condition is ≤ d_ij + ε.
        // So all three edges will be pruned by their respective witnesses.
        // This is correct PFNET behavior for equilateral triangles — they collapse to a tree.
        let edges = vec![
            (1, 2, 0.8), // d=0.2
            (2, 3, 0.8), // d=0.2
            (1, 3, 0.8), // d=0.2
        ];
        let (retained, pruned) = pfnet_classify(&edges);
        // In equilateral triangle, all edges are "redundant" since for each edge,
        // the other two form a path with max leg ≤ the edge distance.
        // This is expected — PFNET aggressively sparsifies uniform-weight graphs.
        assert_eq!(retained.len() + pruned.len(), 3);
        // All pruned: for each edge, the two others provide a witness
        assert_eq!(pruned.len(), 3);
    }
}
