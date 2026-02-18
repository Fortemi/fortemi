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
                }
            })
            .collect();

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
    pub async fn traverse_graph_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        start_id: Uuid,
        max_depth: i32,
        max_nodes: i64,
        min_score: f32,
        max_edges_per_node: Option<i64>,
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
                }
            })
            .collect();

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
