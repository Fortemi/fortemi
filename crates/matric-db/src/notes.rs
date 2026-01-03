//! Note repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use hex;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    CreateNoteRequest, Error, Link, ListNotesRequest, ListNotesResponse, NoteFull, NoteMeta,
    NoteOriginal, NoteRepository, NoteRevised, NoteSummary, Result, UpdateNoteStatusRequest,
};

/// PostgreSQL implementation of NoteRepository.
pub struct PgNoteRepository {
    pool: Pool<Postgres>,
}

impl PgNoteRepository {
    /// Create a new PgNoteRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Compute SHA256 hash of content.
    fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }
}

#[async_trait]
impl NoteRepository for PgNoteRepository {
    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid> {
        let note_id = Uuid::new_v4();
        let now = Utc::now();
        let hash = Self::hash_content(&req.content);

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Insert note metadata
        sqlx::query(
            "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc)
             VALUES ($1, $2, $3, $4, $5, $5)",
        )
        .bind(note_id)
        .bind(req.collection_id)
        .bind(&req.format)
        .bind(&req.source)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Insert original content
        sqlx::query(
            "INSERT INTO note_original (id, note_id, content, hash) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(note_id)
        .bind(&req.content)
        .bind(&hash)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Insert initial revised content (same as original)
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, content, rationale, created_at) VALUES ($1, $2, $3, NULL, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(note_id)
        .bind(&req.content)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Log activity
        sqlx::query(
            "INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
             VALUES ($1, $2, 'user', 'create_note', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(now)
        .bind(note_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Add tags if provided
        if let Some(tags) = req.tags {
            for tag in tags {
                // Create tag if not exists
                sqlx::query(
                    "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                )
                .bind(&tag)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;

                // Link tag to note
                sqlx::query(
                    "INSERT INTO note_tag (note_id, tag_name, source) VALUES ($1, $2, 'user')",
                )
                .bind(note_id)
                .bind(&tag)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        tx.commit().await.map_err(Error::Database)?;

        Ok(note_id)
    }

    async fn fetch(&self, id: Uuid) -> Result<NoteFull> {
        // Update last accessed timestamp
        sqlx::query("UPDATE note SET last_accessed_at = $1, access_count = access_count + 1 WHERE id = $2")
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Fetch note metadata
        let note_row = sqlx::query(
            "SELECT id, collection_id, format, source, created_at_utc, updated_at_utc,
                    starred, archived, last_accessed_at, title, metadata
             FROM note WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::NotFound(format!("Note {} not found", id)))?;

        // Fetch original content
        let original_row = sqlx::query(
            "SELECT content, hash, user_created_at, user_last_edited_at
             FROM note_original WHERE note_id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Fetch current revision
        let revised_row = sqlx::query(
            "SELECT nrc.content, nrc.last_revision_id, nrc.ai_metadata, nr.model
             FROM note_revised_current nrc
             LEFT JOIN note_revision nr ON nr.id = nrc.last_revision_id
             WHERE nrc.note_id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Fetch tags
        let tags: Vec<String> = sqlx::query("SELECT tag_name FROM note_tag WHERE note_id = $1")
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
            .into_iter()
            .map(|r| r.get("tag_name"))
            .collect();

        // Fetch links
        let link_rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(substring(nrc.content from 1 for 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.to_note_id
               WHERE l.from_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let links: Vec<Link> = link_rows
            .into_iter()
            .map(|r| Link {
                id: r.get("id"),
                from_note_id: r.get("from_note_id"),
                to_note_id: r.get("to_note_id"),
                to_url: r.get("to_url"),
                kind: r.get("kind"),
                score: r.get("score"),
                created_at_utc: r.get("created_at_utc"),
                snippet: r.get("snippet"),
                metadata: r.get("metadata"),
            })
            .collect();

        Ok(NoteFull {
            note: NoteMeta {
                id: note_row.get("id"),
                collection_id: note_row.get("collection_id"),
                format: note_row.get("format"),
                source: note_row.get("source"),
                created_at_utc: note_row.get("created_at_utc"),
                updated_at_utc: note_row.get("updated_at_utc"),
                starred: note_row.get::<Option<bool>, _>("starred").unwrap_or(false),
                archived: note_row.get::<Option<bool>, _>("archived").unwrap_or(false),
                last_accessed_at: note_row.get("last_accessed_at"),
                title: note_row.get("title"),
                metadata: note_row.get::<Option<serde_json::Value>, _>("metadata").unwrap_or_else(|| serde_json::json!({})),
            },
            original: NoteOriginal {
                content: original_row.get("content"),
                hash: original_row.get("hash"),
                user_created_at: original_row.get("user_created_at"),
                user_last_edited_at: original_row.get("user_last_edited_at"),
            },
            revised: NoteRevised {
                content: revised_row.get("content"),
                last_revision_id: revised_row.get("last_revision_id"),
                ai_metadata: revised_row.get("ai_metadata"),
                ai_generated_at: None,
                user_last_edited_at: None,
                is_user_edited: false,
                generation_count: 1,
                model: revised_row.get("model"),
            },
            tags,
            links,
        })
    }

    async fn list(&self, req: ListNotesRequest) -> Result<ListNotesResponse> {
        let sort_by = req.sort_by.as_deref().unwrap_or("created_at_utc");
        let sort_order = req.sort_order.as_deref().unwrap_or("desc");
        let filter = req.filter.as_deref().unwrap_or("all");
        let limit = req.limit.unwrap_or(50).min(100);
        let offset = req.offset.unwrap_or(0);

        // Build filter clause
        let filter_clause = match filter {
            "starred" => "AND n.starred = true AND n.archived = false AND n.deleted_at IS NULL",
            "archived" => "AND n.archived = true AND n.deleted_at IS NULL",
            "recent" => "AND n.last_accessed_at IS NOT NULL AND n.archived = false AND n.deleted_at IS NULL",
            "deleted" | "trash" => "AND n.deleted_at IS NOT NULL",
            _ => "AND n.deleted_at IS NULL",
        };

        // Build order clause
        let order_clause = match sort_by {
            "updated_at" => format!("n.updated_at_utc {}", sort_order),
            "accessed_at" => format!("COALESCE(n.last_accessed_at, n.created_at_utc) {}", sort_order),
            _ => format!("n.created_at_utc {}", sort_order),
        };

        // Get total count
        let count_query = format!(
            "SELECT COUNT(*) as count FROM note n WHERE TRUE {}",
            filter_clause
        );
        let total: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Get notes
        let notes_query = format!(
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
            WHERE TRUE {}
            ORDER BY {}
            LIMIT {} OFFSET {}
            "#,
            filter_clause, order_clause, limit, offset
        );

        let rows = sqlx::query(&notes_query)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let notes: Vec<NoteSummary> = rows
            .into_iter()
            .map(|row| {
                let original_content: String = row.get("original_content");
                let revised_content: Option<String> = row.get("revised_content");
                let content = revised_content.as_ref().unwrap_or(&original_content);

                // Extract title
                let stored_title: Option<String> = row.get("title");
                let title = stored_title.unwrap_or_else(|| {
                    content
                        .lines()
                        .next()
                        .map(|l| l.trim_start_matches('#').trim())
                        .unwrap_or("Untitled")
                        .to_string()
                });

                // Create snippet
                let snippet = content
                    .lines()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(200)
                    .collect();

                // Parse tags
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
                    created_at_utc: row.get("created_at_utc"),
                    updated_at_utc: row.get("updated_at_utc"),
                    starred: row.get::<Option<bool>, _>("starred").unwrap_or(false),
                    archived: row.get::<Option<bool>, _>("archived").unwrap_or(false),
                    tags,
                    has_revision: revised_content.is_some(),
                    metadata: row.get::<Option<serde_json::Value>, _>("metadata").unwrap_or_else(|| serde_json::json!({})),
                }
            })
            .collect();

        Ok(ListNotesResponse { notes, total })
    }

    async fn update_status(&self, id: Uuid, req: UpdateNoteStatusRequest) -> Result<()> {
        let mut updates = vec!["updated_at_utc = $1"];
        let now = Utc::now();

        if req.starred.is_some() {
            updates.push("starred = $3");
        }
        if req.archived.is_some() {
            updates.push("archived = $4");
        }

        let query = format!(
            "UPDATE note SET {} WHERE id = $2",
            updates.join(", ")
        );

        let mut q = sqlx::query(&query).bind(now).bind(id);
        if let Some(starred) = req.starred {
            q = q.bind(starred);
        }
        if let Some(archived) = req.archived {
            q = q.bind(archived);
        }

        q.execute(&self.pool).await.map_err(Error::Database)?;
        Ok(())
    }

    async fn update_original(&self, id: Uuid, content: &str) -> Result<()> {
        let now = Utc::now();
        let hash = Self::hash_content(content);

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        sqlx::query(
            "UPDATE note_original SET content = $1, hash = $2, user_last_edited_at = $3 WHERE note_id = $4",
        )
        .bind(content)
        .bind(&hash)
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query("UPDATE note SET updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        sqlx::query(
            "INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
             VALUES ($1, $2, 'user', 'update_original', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn update_revised(&self, id: Uuid, content: &str, rationale: Option<&str>) -> Result<Uuid> {
        let now = Utc::now();
        let revision_id = Uuid::new_v4();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Insert revision record
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, parent_revision_id, revision_number, content, type, created_at_utc, rationale)
             VALUES ($1, $2,
                     (SELECT last_revision_id FROM note_revised_current WHERE note_id = $2),
                     COALESCE((SELECT MAX(revision_number) + 1 FROM note_revision WHERE note_id = $2), 1),
                     $3, 'manual', $4, $5)",
        )
        .bind(revision_id)
        .bind(id)
        .bind(content)
        .bind(now)
        .bind(rationale)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Update current revision
        sqlx::query(
            "UPDATE note_revised_current SET content = $1, last_revision_id = $2 WHERE note_id = $3",
        )
        .bind(content)
        .bind(revision_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Update note timestamp
        sqlx::query("UPDATE note SET updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Log activity
        sqlx::query(
            "INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
             VALUES ($1, $2, 'user', 'revise', $3, '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(revision_id)
    }

    async fn soft_delete(&self, id: Uuid) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET deleted_at = $1, updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn hard_delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn restore(&self, id: Uuid) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET deleted_at = NULL, updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn exists(&self, id: Uuid) -> Result<bool> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id = $1)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_content() {
        let hash = PgNoteRepository::hash_content("test");
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64); // "sha256:" + 64 hex chars
    }
}
