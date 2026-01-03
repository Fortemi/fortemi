//! Link repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Error, Link, LinkRepository, Result};

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
        let link_id = Uuid::new_v4();
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
        .bind(Uuid::new_v4())
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
        .bind(Uuid::new_v4())
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
