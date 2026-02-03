//! Tag repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Error, Result, Tag, TagRepository};

/// Validate a tag name.
///
/// Rules:
/// - Length between 1-100 characters
/// - Allowed characters: alphanumeric, hyphens (-), underscores (_), forward slashes (/)
/// - No spaces or other special characters
///
/// Returns Ok(()) if valid, Err with message if invalid.
pub fn validate_tag_name(tag: &str) -> std::result::Result<(), String> {
    if tag.is_empty() {
        return Err("Tag name cannot be empty".to_string());
    }
    if tag.len() > 100 {
        return Err("Tag name must be 100 characters or less".to_string());
    }

    let invalid_chars: Vec<char> = tag
        .chars()
        .filter(|c| !c.is_alphanumeric() && *c != '-' && *c != '_' && *c != '/')
        .collect();

    if !invalid_chars.is_empty() {
        let chars_display: String = invalid_chars
            .iter()
            .take(5)
            .map(|c| format!("'{}'", c))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Tag contains invalid characters: {}. Only alphanumeric characters, hyphens, underscores, and forward slashes are allowed",
            chars_display
        ));
    }

    Ok(())
}

/// PostgreSQL implementation of TagRepository.
pub struct PgTagRepository {
    pool: Pool<Postgres>,
}

impl PgTagRepository {
    /// Create a new PgTagRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TagRepository for PgTagRepository {
    async fn create(&self, name: &str) -> Result<()> {
        // Validate tag name
        validate_tag_name(name).map_err(Error::InvalidInput)?;

        let now = Utc::now();
        sqlx::query(
            "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(name)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Tag>> {
        let rows = sqlx::query(
            r#"
            SELECT
                t.name,
                t.created_at_utc,
                COUNT(n.id) as note_count
            FROM tag t
            LEFT JOIN note_tag nt ON t.name = nt.tag_name
            LEFT JOIN note n ON nt.note_id = n.id AND n.deleted_at IS NULL
            GROUP BY t.name, t.created_at_utc
            ORDER BY t.name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let tags = rows
            .into_iter()
            .map(|row| Tag {
                name: row.get("name"),
                created_at_utc: row.get("created_at_utc"),
                note_count: row.get("note_count"),
            })
            .collect();

        Ok(tags)
    }

    async fn add_to_note(&self, note_id: Uuid, tag_name: &str, source: &str) -> Result<()> {
        // Validate tag name
        validate_tag_name(tag_name).map_err(Error::InvalidInput)?;

        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Ensure tag exists
        sqlx::query(
            "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(tag_name)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Link tag to note
        sqlx::query(
            "INSERT INTO note_tag (note_id, tag_name, source) VALUES ($1, $2, $3)
             ON CONFLICT (note_id, tag_name) DO NOTHING",
        )
        .bind(note_id)
        .bind(tag_name)
        .bind(source)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn remove_from_note(&self, note_id: Uuid, tag_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM note_tag WHERE note_id = $1 AND LOWER(tag_name) = LOWER($2)")
            .bind(note_id)
            .bind(tag_name)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<String>> {
        let rows =
            sqlx::query("SELECT tag_name FROM note_tag WHERE note_id = $1 ORDER BY tag_name")
                .bind(note_id)
                .fetch_all(&self.pool)
                .await
                .map_err(Error::Database)?;

        let tags = rows.into_iter().map(|row| row.get("tag_name")).collect();
        Ok(tags)
    }

    async fn set_for_note(&self, note_id: Uuid, tags: Vec<String>, source: &str) -> Result<()> {
        // Validate all tag names first
        for tag_name in &tags {
            validate_tag_name(tag_name).map_err(Error::InvalidInput)?;
        }

        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Remove existing tags
        sqlx::query("DELETE FROM note_tag WHERE note_id = $1")
            .bind(note_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Add new tags
        for tag_name in tags {
            // Ensure tag exists
            sqlx::query(
                "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(&tag_name)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            // Link tag to note
            sqlx::query("INSERT INTO note_tag (note_id, tag_name, source) VALUES ($1, $2, $3)")
                .bind(note_id)
                .bind(&tag_name)
                .bind(source)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}
