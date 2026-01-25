//! Note repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use hex;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    new_v7, CreateNoteRequest, Error, Link, ListNotesRequest, ListNotesResponse, NoteFull,
    NoteMeta, NoteOriginal, NoteRepository, NoteRevised, NoteSummary, Result, StrictFilter,
    UpdateNoteStatusRequest,
};

use crate::strict_filter::QueryParam;
use crate::unified_filter::UnifiedFilterQueryBuilder;

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
        let note_id = new_v7();
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
        .bind(new_v7())
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
        .bind(new_v7())
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
        .bind(new_v7())
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

    async fn insert_bulk(&self, notes: Vec<CreateNoteRequest>) -> Result<Vec<Uuid>> {
        if notes.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let mut ids = Vec::with_capacity(notes.len());
        let now = Utc::now();

        for req in notes {
            let note_id = new_v7();
            let hash = Self::hash_content(&req.content);

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
            .bind(new_v7())
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
            .bind(new_v7())
            .bind(note_id)
            .bind(&req.content)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            // Add tags if provided
            if let Some(tags) = req.tags {
                for tag in tags {
                    sqlx::query(
                        "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    )
                    .bind(&tag)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(Error::Database)?;

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

            ids.push(note_id);
        }

        // Log bulk activity
        sqlx::query(
            "INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
             VALUES ($1, $2, 'user', 'bulk_create', NULL, $3::jsonb)",
        )
        .bind(new_v7())
        .bind(now)
        .bind(serde_json::json!({ "count": ids.len() }))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;

        Ok(ids)
    }

    async fn fetch(&self, id: Uuid) -> Result<NoteFull> {
        // Update last accessed timestamp
        sqlx::query(
            "UPDATE note SET last_accessed_at = $1, access_count = access_count + 1 WHERE id = $2",
        )
        .bind(Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Fetch note metadata
        let note_row = sqlx::query(
            "SELECT id, collection_id, format, source, created_at_utc, updated_at_utc,
                    starred, archived, last_accessed_at, title, metadata, chunk_metadata
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
                metadata: note_row
                    .get::<Option<serde_json::Value>, _>("metadata")
                    .unwrap_or_else(|| serde_json::json!({})),
                chunk_metadata: note_row.get("chunk_metadata"),
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
            "recent" => {
                "AND n.last_accessed_at IS NOT NULL AND n.archived = false AND n.deleted_at IS NULL"
            }
            "deleted" | "trash" => "AND n.deleted_at IS NOT NULL",
            _ => "AND n.deleted_at IS NULL",
        };

        // Build order clause
        let order_clause = match sort_by {
            "updated_at" => format!("n.updated_at_utc {}", sort_order),
            "accessed_at" => format!(
                "COALESCE(n.last_accessed_at, n.created_at_utc) {}",
                sort_order
            ),
            _ => format!("n.created_at_utc {}", sort_order),
        };

        // Count tag parameters for binding
        let tag_count = req.tags.as_ref().map(|t| t.len()).unwrap_or(0);

        // Build count query
        let mut count_query = format!(
            "SELECT COUNT(*) as count FROM note n WHERE TRUE {} ",
            filter_clause
        );
        let mut param_idx = 1;

        // Add tag filters to count query
        if tag_count > 0 {
            for _ in 0..tag_count {
                count_query.push_str(&format!(
                    "AND EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND nt.tag_name = ${}) ",
                    param_idx
                ));
                param_idx += 1;
            }
        }

        // Add date filters to count query
        if req.created_after.is_some() {
            count_query.push_str(&format!("AND n.created_at_utc >= ${} ", param_idx));
            param_idx += 1;
        }
        if req.created_before.is_some() {
            count_query.push_str(&format!("AND n.created_at_utc <= ${} ", param_idx));
            param_idx += 1;
        }
        if req.updated_after.is_some() {
            count_query.push_str(&format!("AND n.updated_at_utc >= ${} ", param_idx));
            param_idx += 1;
        }
        if req.updated_before.is_some() {
            count_query.push_str(&format!("AND n.updated_at_utc <= ${} ", param_idx));
            // param_idx += 1; // Not needed for count query
        }

        // Execute count query
        let total: i64 = {
            let mut q = sqlx::query_scalar(&count_query);
            if let Some(tags) = &req.tags {
                for tag in tags {
                    q = q.bind(tag);
                }
            }
            if let Some(dt) = &req.created_after {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.created_before {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.updated_after {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.updated_before {
                q = q.bind(dt);
            }
            q.fetch_one(&self.pool).await.map_err(Error::Database)?
        };

        // Build notes query
        let mut notes_query = format!(
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
            "#,
            filter_clause
        );
        param_idx = 1;

        // Add tag filters to notes query
        if tag_count > 0 {
            for _ in 0..tag_count {
                notes_query.push_str(&format!(
                    "AND EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND nt.tag_name = ${}) ",
                    param_idx
                ));
                param_idx += 1;
            }
        }

        // Add date filters to notes query
        if req.created_after.is_some() {
            notes_query.push_str(&format!("AND n.created_at_utc >= ${} ", param_idx));
            param_idx += 1;
        }
        if req.created_before.is_some() {
            notes_query.push_str(&format!("AND n.created_at_utc <= ${} ", param_idx));
            param_idx += 1;
        }
        if req.updated_after.is_some() {
            notes_query.push_str(&format!("AND n.updated_at_utc >= ${} ", param_idx));
            param_idx += 1;
        }
        if req.updated_before.is_some() {
            notes_query.push_str(&format!("AND n.updated_at_utc <= ${} ", param_idx));
            param_idx += 1;
        }

        // Add order and pagination
        notes_query.push_str(&format!(
            "ORDER BY {} LIMIT ${} OFFSET ${}",
            order_clause,
            param_idx,
            param_idx + 1
        ));

        // Execute notes query
        let rows = {
            let mut q = sqlx::query(&notes_query);
            if let Some(tags) = &req.tags {
                for tag in tags {
                    q = q.bind(tag);
                }
            }
            if let Some(dt) = &req.created_after {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.created_before {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.updated_after {
                q = q.bind(dt);
            }
            if let Some(dt) = &req.updated_before {
                q = q.bind(dt);
            }
            q = q.bind(limit).bind(offset);
            q.fetch_all(&self.pool).await.map_err(Error::Database)?
        };

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
                    metadata: row
                        .get::<Option<serde_json::Value>, _>("metadata")
                        .unwrap_or_else(|| serde_json::json!({})),
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

        let query = format!("UPDATE note SET {} WHERE id = $2", updates.join(", "));

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
        .bind(new_v7())
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn update_revised(
        &self,
        id: Uuid,
        content: &str,
        rationale: Option<&str>,
    ) -> Result<Uuid> {
        let now = Utc::now();
        let revision_id = new_v7();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Insert revision record (view note_revised_current automatically shows latest)
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, content, rationale, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(revision_id)
        .bind(id)
        .bind(content)
        .bind(rationale)
        .bind(now)
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
        .bind(new_v7())
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

    async fn update_title(&self, id: Uuid, title: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET title = $1, updated_at_utc = $2 WHERE id = $3")
            .bind(title)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}

// =============================================================================
// STRICT FILTER EXTENSION
// =============================================================================

/// Request for listing notes with unified strict filter.
#[derive(Debug, Clone, Default)]
pub struct ListNotesWithFilterRequest {
    /// Multi-dimensional strict filter.
    pub filter: StrictFilter,
    /// Maximum number of notes to return.
    pub limit: i64,
    /// Offset for pagination.
    pub offset: i64,
    /// Sort field: "created_at", "updated_at", "accessed_at", "title".
    pub sort_by: Option<String>,
    /// Sort order: "asc" or "desc".
    pub sort_order: Option<String>,
}

/// Response for filtered note listing.
#[derive(Debug, Clone)]
pub struct ListNotesWithFilterResponse {
    /// Total count of matching notes (before pagination).
    pub total: i64,
    /// Notes in this page.
    pub notes: Vec<NoteSummary>,
    /// Whether UUIDv7 temporal optimization was applied.
    pub used_uuid_temporal_opt: bool,
    /// Whether recursive CTE was used for collections.
    pub used_recursive_cte: bool,
    /// Number of active filter dimensions.
    pub active_dimensions: usize,
}

impl PgNoteRepository {
    /// List notes using the unified strict filter system.
    ///
    /// This method provides multi-dimensional filtering with optimizations:
    /// - UUIDv7 temporal optimization for created time filtering
    /// - Recursive CTE for hierarchical collection filtering
    /// - SKOS concept-based tag filtering
    /// - Security and visibility filtering
    pub async fn list_with_strict_filter(
        &self,
        req: ListNotesWithFilterRequest,
    ) -> Result<ListNotesWithFilterResponse> {
        let sort_by = req.sort_by.as_deref().unwrap_or("created_at");
        let sort_order = req.sort_order.as_deref().unwrap_or("desc");

        // Build filter query
        let builder = UnifiedFilterQueryBuilder::new(req.filter, 0);
        let filter_result = builder.build();

        // Build order clause
        let order_clause = match sort_by {
            "updated_at" => format!("n.updated_at_utc {}", sort_order),
            "accessed_at" => format!(
                "COALESCE(n.last_accessed_at, n.created_at_utc) {}",
                sort_order
            ),
            "title" => format!("n.title {} NULLS LAST", sort_order),
            _ => format!("n.created_at_utc {}", sort_order),
        };

        // Build full query with optional CTE
        let cte_prefix = filter_result
            .cte_clause
            .as_ref()
            .map(|cte| format!("WITH RECURSIVE {} ", cte))
            .unwrap_or_default();

        // Count query
        let count_sql = format!(
            "{}SELECT COUNT(*) as count FROM note n WHERE {}",
            cte_prefix, filter_result.where_clause
        );

        // Build and execute count query
        let mut count_q = sqlx::query(&count_sql);
        for param in &filter_result.params {
            count_q = match param {
                QueryParam::Uuid(id) => count_q.bind(id),
                QueryParam::UuidArray(ids) => count_q.bind(ids),
                QueryParam::Int(val) => count_q.bind(val),
                QueryParam::Timestamp(ts) => count_q.bind(ts),
                QueryParam::Bool(b) => count_q.bind(b),
                QueryParam::String(s) => count_q.bind(s),
            };
        }

        let count_row = count_q
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        let total: i64 = count_row.get("count");

        // Notes query with pagination
        let limit_param = filter_result.params.len() + 1;
        let offset_param = filter_result.params.len() + 2;

        let notes_sql = format!(
            r#"
            {}
            SELECT
                n.id, n.created_at_utc, n.updated_at_utc, n.starred, n.archived,
                n.title, n.metadata,
                no.content as original_content,
                nrc.content as revised_content,
                (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id) as tags
            FROM note n
            LEFT JOIN note_original no ON no.note_id = n.id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE {}
            ORDER BY {}
            LIMIT ${} OFFSET ${}
            "#,
            cte_prefix, filter_result.where_clause, order_clause, limit_param, offset_param
        );

        // Build and execute notes query
        let mut notes_q = sqlx::query(&notes_sql);
        for param in &filter_result.params {
            notes_q = match param {
                QueryParam::Uuid(id) => notes_q.bind(id),
                QueryParam::UuidArray(ids) => notes_q.bind(ids),
                QueryParam::Int(val) => notes_q.bind(val),
                QueryParam::Timestamp(ts) => notes_q.bind(ts),
                QueryParam::Bool(b) => notes_q.bind(b),
                QueryParam::String(s) => notes_q.bind(s),
            };
        }
        notes_q = notes_q.bind(req.limit).bind(req.offset);

        let rows = notes_q
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let notes = rows
            .into_iter()
            .map(|row| {
                let tags_str: Option<String> = row.get("tags");
                let tags = tags_str
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default();

                let revised: Option<String> = row.get("revised_content");
                let original: Option<String> = row.get("original_content");
                let has_revision = revised.is_some();
                let snippet = revised
                    .or(original)
                    .map(|c| c.chars().take(200).collect())
                    .unwrap_or_default();

                NoteSummary {
                    id: row.get("id"),
                    created_at_utc: row.get("created_at_utc"),
                    updated_at_utc: row.get("updated_at_utc"),
                    starred: row.get("starred"),
                    archived: row.get("archived"),
                    title: row.get::<Option<String>, _>("title").unwrap_or_default(),
                    snippet,
                    tags,
                    has_revision,
                    metadata: row.get("metadata"),
                }
            })
            .collect();

        Ok(ListNotesWithFilterResponse {
            total,
            notes,
            used_uuid_temporal_opt: filter_result.used_uuid_temporal_opt,
            used_recursive_cte: filter_result.used_recursive_cte,
            active_dimensions: filter_result.active_dimensions,
        })
    }

    /// Get note IDs matching a strict filter (lightweight version for batch operations).
    pub async fn get_ids_with_strict_filter(
        &self,
        filter: StrictFilter,
        limit: i64,
    ) -> Result<Vec<Uuid>> {
        let builder = UnifiedFilterQueryBuilder::new(filter, 0);
        let filter_result = builder.build();

        let cte_prefix = filter_result
            .cte_clause
            .as_ref()
            .map(|cte| format!("WITH RECURSIVE {} ", cte))
            .unwrap_or_default();

        let sql = format!(
            "{}SELECT n.id FROM note n WHERE {} ORDER BY n.created_at_utc DESC LIMIT ${}",
            cte_prefix,
            filter_result.where_clause,
            filter_result.params.len() + 1
        );

        let mut q = sqlx::query(&sql);
        for param in &filter_result.params {
            q = match param {
                QueryParam::Uuid(id) => q.bind(id),
                QueryParam::UuidArray(ids) => q.bind(ids),
                QueryParam::Int(val) => q.bind(val),
                QueryParam::Timestamp(ts) => q.bind(ts),
                QueryParam::Bool(b) => q.bind(b),
                QueryParam::String(s) => q.bind(s),
            };
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await.map_err(Error::Database)?;
        Ok(rows.into_iter().map(|r| r.get("id")).collect())
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
