//! Collection repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{new_v7, Collection, CollectionRepository, Error, NoteSummary, Result};

/// PostgreSQL implementation of CollectionRepository.
pub struct PgCollectionRepository {
    pool: Pool<Postgres>,
}

impl PgCollectionRepository {
    /// Create a new PgCollectionRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CollectionRepository for PgCollectionRepository {
    async fn create(
        &self,
        name: &str,
        description: Option<&str>,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO collection (id, name, description, parent_id, created_at_utc)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(parent_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Collection>> {
        let row = sqlx::query(
            r#"
            SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                   COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
            FROM collection c
            WHERE c.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| Collection {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            parent_id: r.get("parent_id"),
            created_at_utc: r.get("created_at_utc"),
            note_count: r.get("note_count"),
        }))
    }

    async fn list(&self, parent_id: Option<Uuid>) -> Result<Vec<Collection>> {
        let rows = if let Some(pid) = parent_id {
            sqlx::query(
                r#"
                SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                       COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
                FROM collection c
                WHERE c.parent_id = $1
                ORDER BY c.name
                "#,
            )
            .bind(pid)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            // List root collections (no parent) or all if no filter
            sqlx::query(
                r#"
                SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                       COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
                FROM collection c
                WHERE c.parent_id IS NULL
                ORDER BY c.name
                "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        };

        Ok(rows
            .into_iter()
            .map(|r| Collection {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                parent_id: r.get("parent_id"),
                created_at_utc: r.get("created_at_utc"),
                note_count: r.get("note_count"),
            })
            .collect())
    }

    async fn update(&self, id: Uuid, name: &str, description: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE collection SET name = $1, description = $2 WHERE id = $3")
            .bind(name)
            .bind(description)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Move notes to uncategorized (set collection_id to NULL)
        sqlx::query("UPDATE note SET collection_id = NULL WHERE collection_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Move child collections to root (set parent_id to NULL)
        sqlx::query("UPDATE collection SET parent_id = NULL WHERE parent_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Delete the collection
        sqlx::query("DELETE FROM collection WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn get_notes(&self, id: Uuid, limit: i64, offset: i64) -> Result<Vec<NoteSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT
                n.id, n.created_at_utc, n.updated_at_utc, n.starred, n.archived,
                n.title, n.metadata,
                no.content as original_content,
                nrc.content as revised_content,
                COALESCE(
                    (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                    ''
                ) as tags
            FROM note n
            JOIN note_original no ON no.note_id = n.id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE n.collection_id = $1 AND n.deleted_at IS NULL
            ORDER BY n.created_at_utc DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let original_content: String = row.get("original_content");
                let revised_content: Option<String> = row.get("revised_content");
                let content = revised_content.as_ref().unwrap_or(&original_content);

                let stored_title: Option<String> = row.get("title");
                let title = stored_title.unwrap_or_else(|| {
                    content
                        .lines()
                        .next()
                        .map(|l| l.trim_start_matches('#').trim())
                        .unwrap_or("Untitled")
                        .to_string()
                });

                let snippet = content
                    .lines()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(200)
                    .collect();

                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };

                NoteSummary {
                    id: row.get("id"),
                    title,
                    snippet,
                    embedding_status: None,
                    created_at_utc: row.get("created_at_utc"),
                    updated_at_utc: row.get("updated_at_utc"),
                    starred: row.get::<Option<bool>, _>("starred").unwrap_or(false),
                    archived: row.get::<Option<bool>, _>("archived").unwrap_or(false),
                    tags,
                    has_revision: match &revised_content {
                        Some(revised) => revised != &original_content,
                        None => false,
                    },
                    metadata: row
                        .get::<Option<serde_json::Value>, _>("metadata")
                        .unwrap_or_else(|| serde_json::json!({})),
                    document_type_id: None, // Not fetched in collection notes query
                    document_type_name: None,
                }
            })
            .collect())
    }

    async fn move_note(&self, note_id: Uuid, collection_id: Option<Uuid>) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET collection_id = $1, updated_at_utc = $2 WHERE id = $3")
            .bind(collection_id)
            .bind(now)
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}

/// Transaction-aware variants for archive-scoped operations (Issue #108).
impl PgCollectionRepository {
    /// Create a collection within an existing transaction.
    pub async fn create_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        name: &str,
        description: Option<&str>,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO collection (id, name, description, parent_id, created_at_utc)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(parent_id)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    /// Get a collection by ID within an existing transaction.
    pub async fn get_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<Collection>> {
        let row = sqlx::query(
            r#"
            SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                   COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
            FROM collection c
            WHERE c.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| Collection {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            parent_id: r.get("parent_id"),
            created_at_utc: r.get("created_at_utc"),
            note_count: r.get("note_count"),
        }))
    }

    /// List collections within an existing transaction.
    pub async fn list_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        parent_id: Option<Uuid>,
    ) -> Result<Vec<Collection>> {
        let rows = if let Some(pid) = parent_id {
            sqlx::query(
                r#"
                SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                       COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
                FROM collection c
                WHERE c.parent_id = $1
                ORDER BY c.name
                "#,
            )
            .bind(pid)
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::query(
                r#"
                SELECT c.id, c.name, c.description, c.parent_id, c.created_at_utc,
                       COALESCE((SELECT COUNT(*) FROM note WHERE collection_id = c.id), 0) as note_count
                FROM collection c
                WHERE c.parent_id IS NULL
                ORDER BY c.name
                "#,
            )
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?
        };

        Ok(rows
            .into_iter()
            .map(|r| Collection {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                parent_id: r.get("parent_id"),
                created_at_utc: r.get("created_at_utc"),
                note_count: r.get("note_count"),
            })
            .collect())
    }

    /// Update a collection within an existing transaction.
    pub async fn update_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE collection SET name = $1, description = $2 WHERE id = $3")
            .bind(name)
            .bind(description)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Delete a collection within an existing transaction.
    pub async fn delete_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        // Move notes to uncategorized
        sqlx::query("UPDATE note SET collection_id = NULL WHERE collection_id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        // Move child collections to root
        sqlx::query("UPDATE collection SET parent_id = NULL WHERE parent_id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        // Delete the collection
        sqlx::query("DELETE FROM collection WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Move a note to a collection within an existing transaction.
    pub async fn move_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        collection_id: Option<Uuid>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET collection_id = $1, updated_at_utc = $2 WHERE id = $3")
            .bind(collection_id)
            .bind(now)
            .bind(note_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Get notes for a collection within an existing transaction.
    pub async fn get_notes_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NoteSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT
                n.id, n.created_at_utc, n.updated_at_utc, n.starred, n.archived,
                n.title, n.metadata,
                no.content as original_content,
                nrc.content as revised_content,
                COALESCE(
                    (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                    ''
                ) as tags
            FROM note n
            JOIN note_original no ON no.note_id = n.id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE n.collection_id = $1 AND n.deleted_at IS NULL
            ORDER BY n.created_at_utc DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let original_content: String = row.get("original_content");
                let revised_content: Option<String> = row.get("revised_content");
                let content = revised_content.as_ref().unwrap_or(&original_content);

                let stored_title: Option<String> = row.get("title");
                let title = stored_title.unwrap_or_else(|| {
                    content
                        .lines()
                        .next()
                        .map(|l| l.trim_start_matches('#').trim())
                        .unwrap_or("Untitled")
                        .to_string()
                });

                let snippet = content
                    .lines()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(200)
                    .collect();

                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };

                NoteSummary {
                    id: row.get("id"),
                    title,
                    snippet,
                    embedding_status: None,
                    created_at_utc: row.get("created_at_utc"),
                    updated_at_utc: row.get("updated_at_utc"),
                    starred: row.get::<Option<bool>, _>("starred").unwrap_or(false),
                    archived: row.get::<Option<bool>, _>("archived").unwrap_or(false),
                    tags,
                    has_revision: match &revised_content {
                        Some(revised) => revised != &original_content,
                        None => false,
                    },
                    metadata: row
                        .get::<Option<serde_json::Value>, _>("metadata")
                        .unwrap_or_else(|| serde_json::json!({})),
                    document_type_id: None, // Not fetched in collection notes query
                    document_type_name: None,
                }
            })
            .collect())
    }
}
