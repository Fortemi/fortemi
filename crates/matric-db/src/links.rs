//! Link repository implementation.

use async_trait::async_trait;
use chrono::Utc;
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
                COALESCE(substring(nrc.content from 1 for 100), 'Linked note') as snippet
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
                COALESCE(substring(nrc.content from 1 for 100), 'Linked note') as snippet
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

/// Graph node with basic note info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub title: Option<String>,
    pub depth: i32,
}

/// Graph edge representing a link between notes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphEdge {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub score: f32,
    pub kind: String,
}

/// Result of graph traversal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
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
    /// Returns unique nodes and edges discovered.
    pub async fn traverse_graph(
        &self,
        start_id: Uuid,
        max_depth: i32,
        max_nodes: i64,
    ) -> Result<GraphResult> {
        // Use recursive CTE to traverse the graph
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE graph AS (
                -- Base case: starting node
                SELECT
                    $1::uuid as note_id,
                    0 as depth

                UNION

                -- Recursive case: follow links
                SELECT
                    CASE WHEN l.from_note_id = g.note_id THEN l.to_note_id ELSE l.from_note_id END as note_id,
                    g.depth + 1 as depth
                FROM graph g
                JOIN link l ON (l.from_note_id = g.note_id OR l.to_note_id = g.note_id)
                WHERE g.depth < $2
            )
            SELECT DISTINCT ON (g.note_id)
                g.note_id,
                g.depth,
                n.title
            FROM graph g
            JOIN note n ON n.id = g.note_id
            WHERE n.deleted_at IS NULL
            ORDER BY g.note_id, g.depth
            LIMIT $3
            "#,
        )
        .bind(start_id)
        .bind(max_depth)
        .bind(max_nodes)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let nodes: Vec<GraphNode> = rows
            .iter()
            .map(|row| GraphNode {
                id: row.get("note_id"),
                title: row.get("title"),
                depth: row.get("depth"),
            })
            .collect();

        // Get edges between discovered nodes
        let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();

        let edge_rows = sqlx::query(
            r#"
            SELECT DISTINCT from_note_id, to_note_id, score, kind
            FROM link
            WHERE from_note_id = ANY($1) AND to_note_id = ANY($1)
            "#,
        )
        .bind(&node_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let edges: Vec<GraphEdge> = edge_rows
            .iter()
            .map(|row| GraphEdge {
                from_id: row.get("from_note_id"),
                to_id: row.get("to_note_id"),
                score: row.get("score"),
                kind: row.get("kind"),
            })
            .collect();

        Ok(GraphResult { nodes, edges })
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
                COALESCE(substring(nrc.content from 1 for 100), 'Linked note') as snippet
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
                COALESCE(substring(nrc.content from 1 for 100), 'Linked note') as snippet
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
    pub async fn traverse_graph_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        start_id: Uuid,
        max_depth: i32,
        max_nodes: i64,
    ) -> Result<GraphResult> {
        // Use recursive CTE to traverse the graph
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE graph AS (
                -- Base case: starting node
                SELECT
                    $1::uuid as note_id,
                    0 as depth

                UNION

                -- Recursive case: follow links
                SELECT
                    CASE WHEN l.from_note_id = g.note_id THEN l.to_note_id ELSE l.from_note_id END as note_id,
                    g.depth + 1 as depth
                FROM graph g
                JOIN link l ON (l.from_note_id = g.note_id OR l.to_note_id = g.note_id)
                WHERE g.depth < $2
            )
            SELECT DISTINCT ON (g.note_id)
                g.note_id,
                g.depth,
                n.title
            FROM graph g
            JOIN note n ON n.id = g.note_id
            WHERE n.deleted_at IS NULL
            ORDER BY g.note_id, g.depth
            LIMIT $3
            "#,
        )
        .bind(start_id)
        .bind(max_depth)
        .bind(max_nodes)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let nodes: Vec<GraphNode> = rows
            .iter()
            .map(|row| GraphNode {
                id: row.get("note_id"),
                title: row.get("title"),
                depth: row.get("depth"),
            })
            .collect();

        // Get edges between discovered nodes
        let node_ids: Vec<Uuid> = nodes.iter().map(|n| n.id).collect();

        let edge_rows = sqlx::query(
            r#"
            SELECT DISTINCT from_note_id, to_note_id, score, kind
            FROM link
            WHERE from_note_id = ANY($1) AND to_note_id = ANY($1)
            "#,
        )
        .bind(&node_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let edges: Vec<GraphEdge> = edge_rows
            .iter()
            .map(|row| GraphEdge {
                from_id: row.get("from_note_id"),
                to_id: row.get("to_note_id"),
                score: row.get("score"),
                kind: row.get("kind"),
            })
            .collect();

        Ok(GraphResult { nodes, edges })
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
}
