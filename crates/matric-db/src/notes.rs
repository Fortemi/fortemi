//! Note repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use hex;
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres, Row, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

use matric_core::{
    new_v7, CreateNoteRequest, Error, Link, ListNotesRequest, ListNotesResponse,
    NoteConceptSummary, NoteFull, NoteMeta, NoteOriginal, NoteRepository, NoteRevised, NoteSummary,
    Result, StrictFilter, UpdateNoteStatusRequest,
};

use crate::hashtag_extraction::extract_inline_hashtags;
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
    /// Merge explicit tags with inline hashtags extracted from content.
    ///
    /// Returns a deduplicated, sorted vector of all tags.
    fn merge_tags(explicit_tags: Option<Vec<String>>, content: &str) -> Vec<String> {
        let mut all_tags = HashSet::new();

        // Add explicit tags
        if let Some(tags) = explicit_tags {
            for tag in tags {
                all_tags.insert(tag.to_lowercase());
            }
        }

        // Extract and add inline hashtags
        let inline_tags = extract_inline_hashtags(content);
        for tag in inline_tags {
            all_tags.insert(tag);
        }

        // Convert to sorted vector
        let mut result: Vec<String> = all_tags.into_iter().collect();
        result.sort();
        result
    }
}

// =============================================================================
// HELPER FUNCTIONS FOR LIST QUERY BUILDING (Issue #468)
// =============================================================================

/// Build the filter clause based on the filter type.
fn build_filter_clause(filter: &str) -> &'static str {
    match filter {
        "active" => "AND n.archived = false AND n.deleted_at IS NULL",
        "starred" => "AND n.starred = true AND n.archived = false AND n.deleted_at IS NULL",
        "archived" => "AND n.archived = true AND n.deleted_at IS NULL",
        "recent" => {
            "AND n.last_accessed_at IS NOT NULL AND n.archived = false AND n.deleted_at IS NULL"
        }
        "deleted" | "trash" => "AND n.deleted_at IS NOT NULL",
        _ => "AND n.deleted_at IS NULL",
    }
}

/// Build the order clause based on sort_by and sort_order.
fn validate_sort_order(sort_order: &str) -> &'static str {
    match sort_order.to_uppercase().as_str() {
        "ASC" => "ASC",
        _ => "DESC",
    }
}

fn build_order_clause(sort_by: &str, sort_order: &str) -> String {
    let validated = validate_sort_order(sort_order);
    match sort_by {
        "updated_at" => format!("n.updated_at_utc {}", validated),
        "accessed_at" => format!(
            "COALESCE(n.last_accessed_at, n.created_at_utc) {}",
            validated
        ),
        _ => format!("n.created_at_utc {}", validated),
    }
}

/// Add tag filters to the query string.
fn add_tag_filters(query: &mut String, param_idx: &mut usize, tag_count: usize) {
    for _ in 0..tag_count {
        query.push_str(&format!(
            "AND EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND (LOWER(nt.tag_name) = LOWER(${}) OR LOWER(nt.tag_name) LIKE LOWER(${}) || '/%' ESCAPE '\\')) ",
            param_idx, param_idx
        ));
        *param_idx += 1;
    }
}

/// Add date filters to the query string.
fn add_date_filters(
    query: &mut String,
    param_idx: &mut usize,
    has_created_after: bool,
    has_created_before: bool,
    has_updated_after: bool,
    has_updated_before: bool,
) {
    if has_created_after {
        query.push_str(&format!("AND n.created_at_utc >= ${} ", param_idx));
        *param_idx += 1;
    }
    if has_created_before {
        query.push_str(&format!("AND n.created_at_utc <= ${} ", param_idx));
        *param_idx += 1;
    }
    if has_updated_after {
        query.push_str(&format!("AND n.updated_at_utc >= ${} ", param_idx));
        *param_idx += 1;
    }
    if has_updated_before {
        query.push_str(&format!("AND n.updated_at_utc <= ${} ", param_idx));
        *param_idx += 1;
    }
}

/// Macro to bind ListNotesRequest parameters to a query.
macro_rules! bind_list_request_params {
    ($query:expr, $req:expr) => {{
        let mut q = $query;
        if let Some(tags) = &$req.tags {
            for tag in tags {
                q = q.bind(tag);
            }
        }
        if let Some(dt) = &$req.created_after {
            q = q.bind(dt);
        }
        if let Some(dt) = &$req.created_before {
            q = q.bind(dt);
        }
        if let Some(dt) = &$req.updated_after {
            q = q.bind(dt);
        }
        if let Some(dt) = &$req.updated_before {
            q = q.bind(dt);
        }
        q
    }};
}

/// Map a database row to a NoteSummary (Issue #468).
fn map_row_to_note_summary(row: sqlx::postgres::PgRow) -> NoteSummary {
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

    // has_revision should only be true if revised content differs from original
    let has_revision = match &revised_content {
        Some(revised) => revised != &original_content,
        None => false,
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
        has_revision,
        metadata: row
            .get::<Option<serde_json::Value>, _>("metadata")
            .unwrap_or_else(|| serde_json::json!({})),
        document_type_id: row.get("document_type_id"),
        document_type_name: row.get("document_type_name"),
        embedding_status: None,
    }
}

#[async_trait]
impl NoteRepository for PgNoteRepository {
    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let result = self.insert_tx(&mut tx, req).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    async fn insert_bulk(&self, notes: Vec<CreateNoteRequest>) -> Result<Vec<Uuid>> {
        if notes.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let result = self.insert_bulk_tx(&mut tx, notes).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    async fn fetch(&self, id: Uuid) -> Result<NoteFull> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let result = self.fetch_tx(&mut tx, id).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    async fn list(&self, req: ListNotesRequest) -> Result<ListNotesResponse> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let result = self.list_tx(&mut tx, req).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    async fn update_status(&self, id: Uuid, req: UpdateNoteStatusRequest) -> Result<()> {
        // Check if note exists first (issue #362)
        if !self.exists(id).await? {
            return Err(Error::NotFound(format!("Note {} not found", id)));
        }
        let mut updates: Vec<String> = vec!["updated_at_utc = $1".to_string()];
        let now = Utc::now();
        // $1 = now, $2 = id, then dynamic params start at $3
        let mut param_idx = 3;

        if req.starred.is_some() {
            updates.push(format!("starred = ${}", param_idx));
            param_idx += 1;
        }
        if req.archived.is_some() {
            updates.push(format!("archived = ${}", param_idx));
            param_idx += 1;
        }
        if req.metadata.is_some() {
            // Merge new metadata keys into existing (issue #122) instead of replacing
            updates.push(format!(
                "metadata = COALESCE(metadata, '{{}}'::jsonb) || ${}",
                param_idx
            ));
        }

        let query = format!("UPDATE note SET {} WHERE id = $2", updates.join(", "));

        let mut q = sqlx::query(&query).bind(now).bind(id);
        if let Some(starred) = req.starred {
            q = q.bind(starred);
        }
        if let Some(archived) = req.archived {
            q = q.bind(archived);
        }
        if let Some(metadata) = req.metadata {
            q = q.bind(metadata);
        }

        q.execute(&self.pool).await.map_err(Error::Database)?;
        Ok(())
    }

    async fn update_original(&self, id: Uuid, content: &str) -> Result<()> {
        // Check if note exists first (issue #362)
        if !self.exists(id).await? {
            return Err(Error::NotFound(format!("Note {} not found", id)));
        }
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

        // Insert revision record into history table
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, content, rationale, created_at_utc, revision_number)
             VALUES ($1, $2, $3, $4, $5, COALESCE((SELECT MAX(revision_number) FROM note_revision WHERE note_id = $2), 0) + 1)",
        )
        .bind(revision_id)
        .bind(id)
        .bind(content)
        .bind(rationale)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Update the current revision snapshot (note_revised_current is a materialized table, not a view)
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
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        self.soft_delete_tx(&mut tx, id).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn hard_delete(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        self.hard_delete_tx(&mut tx, id).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn restore(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        self.restore_tx(&mut tx, id).await?;
        tx.commit().await.map_err(Error::Database)?;
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

    async fn list_all_ids(&self) -> Result<Vec<Uuid>> {
        let sql = "SELECT id FROM note WHERE deleted_at IS NULL ORDER BY created_at_utc DESC";
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }
}

// =============================================================================
// TRANSACTION-AWARE VARIANTS FOR ARCHIVE-SCOPED OPERATIONS
// =============================================================================

/// Transaction-aware variants for archive-scoped operations.
///
/// These methods accept an existing transaction, allowing multiple repository
/// operations to be composed within a single database transaction. This is
/// essential for archive operations where notes must be inserted and linked
/// within a transactional boundary.
impl PgNoteRepository {
    /// Insert a note within an existing transaction.
    pub async fn insert_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateNoteRequest,
    ) -> Result<Uuid> {
        let note_id = new_v7();
        let now = Utc::now();
        let hash = Self::hash_content(&req.content);

        // Merge explicit tags with inline hashtags
        let all_tags = Self::merge_tags(req.tags.clone(), &req.content);

        // Insert note metadata
        sqlx::query(
            "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc, metadata, document_type_id)
             VALUES ($1, $2, $3, $4, $5, $5, COALESCE($6, '{}'::jsonb), $7)",
        )
        .bind(note_id)
        .bind(req.collection_id)
        .bind(&req.format)
        .bind(&req.source)
        .bind(now)
        .bind(req.metadata.as_ref().unwrap_or(&serde_json::json!({})))
        .bind(req.document_type_id)
        .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Insert initial revised content (same as original)
        let revision_id = new_v7();
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, content, rationale, created_at_utc, revision_number) VALUES ($1, $2, $3, NULL, $4, 1)",
        )
        .bind(revision_id)
        .bind(note_id)
        .bind(&req.content)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Populate current revised content
        sqlx::query(
            "INSERT INTO note_revised_current (note_id, content, last_revision_id) VALUES ($1, $2, $3)",
        )
        .bind(note_id)
        .bind(&req.content)
        .bind(revision_id)
        .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Add tags (merged explicit + inline)
        for tag in all_tags {
            // Determine source: 'user' if explicitly provided, 'inline' if extracted
            let source = if req.tags.as_ref().is_some_and(|t| t.contains(&tag)) {
                "user"
            } else {
                "inline"
            };

            // Create tag if not exists
            sqlx::query(
                "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(&tag)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            // Link tag to note
            sqlx::query("INSERT INTO note_tag (note_id, tag_name, source) VALUES ($1, $2, $3)")
                .bind(note_id)
                .bind(&tag)
                .bind(source)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
        }

        Ok(note_id)
    }

    /// Insert multiple notes within an existing transaction.
    pub async fn insert_bulk_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        notes: Vec<CreateNoteRequest>,
    ) -> Result<Vec<Uuid>> {
        if notes.is_empty() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::with_capacity(notes.len());
        let now = Utc::now();

        for req in notes {
            let note_id = new_v7();
            let hash = Self::hash_content(&req.content);

            // Merge explicit tags with inline hashtags
            let all_tags = Self::merge_tags(req.tags.clone(), &req.content);

            // Insert note metadata
            sqlx::query(
                "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc, metadata, document_type_id)
                 VALUES ($1, $2, $3, $4, $5, $5, COALESCE($6, '{}'::jsonb), $7)",
            )
            .bind(note_id)
            .bind(req.collection_id)
            .bind(&req.format)
            .bind(&req.source)
            .bind(now)
            .bind(req.metadata.as_ref().unwrap_or(&serde_json::json!({})))
            .bind(req.document_type_id)
            .execute(&mut **tx)
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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            // Insert initial revised content (same as original)
            let revision_id = new_v7();
            sqlx::query(
                "INSERT INTO note_revision (id, note_id, content, rationale, created_at_utc, revision_number) VALUES ($1, $2, $3, NULL, $4, 1)",
            )
            .bind(revision_id)
            .bind(note_id)
            .bind(&req.content)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            // Populate current revised content
            sqlx::query(
                "INSERT INTO note_revised_current (note_id, content, last_revision_id) VALUES ($1, $2, $3)",
            )
            .bind(note_id)
            .bind(&req.content)
            .bind(revision_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            // Add tags (merged explicit + inline)
            for tag in &all_tags {
                // Determine source: 'user' if explicitly provided, 'inline' if extracted
                let source = if req.tags.as_ref().is_some_and(|t| t.contains(tag)) {
                    "user"
                } else {
                    "inline"
                };

                sqlx::query(
                    "INSERT INTO tag (name, created_at_utc) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                )
                .bind(tag)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                sqlx::query("INSERT INTO note_tag (note_id, tag_name, source) VALUES ($1, $2, $3)")
                    .bind(note_id)
                    .bind(tag)
                    .bind(source)
                    .execute(&mut **tx)
                    .await
                    .map_err(Error::Database)?;
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(ids)
    }

    /// Fetch a note within an existing transaction.
    pub async fn fetch_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<NoteFull> {
        // Update last accessed timestamp
        sqlx::query(
            "UPDATE note SET last_accessed_at = $1, access_count = access_count + 1 WHERE id = $2",
        )
        .bind(Utc::now())
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Fetch note metadata
        let note_row = sqlx::query(
            "SELECT id, collection_id, format, source, created_at_utc, updated_at_utc,
                    starred, archived, last_accessed_at, title, metadata, chunk_metadata, document_type_id
             FROM note WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::NotFound(format!("Note {} not found", id)))?;

        // Fetch original content
        let original_row = sqlx::query(
            "SELECT content, hash, user_created_at, user_last_edited_at
             FROM note_original WHERE note_id = $1",
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Fetch current revision
        let revised_row = sqlx::query(
            "SELECT nrc.content, nrc.last_revision_id, nrc.ai_metadata, nr.model, COALESCE(nr.generation_count, 1) AS generation_count
             FROM note_revised_current nrc
             LEFT JOIN note_revision nr ON nr.id = nrc.last_revision_id
             WHERE nrc.note_id = $1",
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Fetch flat tags (user-created/legacy)
        let tags: Vec<String> = sqlx::query("SELECT tag_name FROM note_tag WHERE note_id = $1")
            .bind(id)
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?
            .into_iter()
            .map(|r| r.get("tag_name"))
            .collect();

        // Fetch SKOS concepts with full metadata — separate from flat tags.
        // Concepts are a different search/classification vector: they enrich
        // embeddings and provide semantic structure, but are not merged into tags.
        let concept_rows = sqlx::query(
            r#"SELECT c.id as concept_id, c.notation, nc.source, nc.confidence,
                      nc.relevance_score, nc.is_primary,
                      l.value as pref_label
               FROM note_skos_concept nc
               JOIN skos_concept c ON nc.concept_id = c.id
               LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                   AND l.label_type = 'pref_label' AND l.language = 'en'
               WHERE nc.note_id = $1
               ORDER BY nc.is_primary DESC, nc.relevance_score DESC"#,
        )
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let concepts: Vec<NoteConceptSummary> = concept_rows
            .iter()
            .map(|r| NoteConceptSummary {
                concept_id: r.get("concept_id"),
                notation: r.get("notation"),
                pref_label: r.get("pref_label"),
                source: r.get("source"),
                confidence: r.get("confidence"),
                relevance_score: r.get("relevance_score"),
                is_primary: r.get("is_primary"),
            })
            .collect();

        // Fetch links
        let link_rows = sqlx::query(
            r#"SELECT
                l.id, l.from_note_id, l.to_note_id, l.to_url, l.kind, l.score,
                l.created_at_utc, l.metadata,
                COALESCE(left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 100), 'Linked note') as snippet
               FROM link l
               LEFT JOIN note_revised_current nrc ON nrc.note_id = l.to_note_id
               WHERE l.from_note_id = $1
               ORDER BY l.score DESC, l.created_at_utc DESC"#,
        )
        .bind(id)
        .fetch_all(&mut **tx)
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
                document_type_id: note_row.get("document_type_id"),
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
                generation_count: revised_row.get("generation_count"),
                model: revised_row.get("model"),
            },
            tags,
            concepts,
            links,
        })
    }

    /// List notes within an existing transaction.
    pub async fn list_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: ListNotesRequest,
    ) -> Result<ListNotesResponse> {
        let sort_by = req.sort_by.as_deref().unwrap_or("created_at_utc");
        let sort_order = req.sort_order.as_deref().unwrap_or("desc");
        let filter = req.filter.as_deref().unwrap_or("all");
        let limit = req.limit.unwrap_or(50).min(100);
        let offset = req.offset.unwrap_or(0);

        let filter_clause = build_filter_clause(filter);
        let order_clause = build_order_clause(sort_by, sort_order);
        let tag_count = req.tags.as_ref().map(|t| t.len()).unwrap_or(0);

        // Build count query
        let mut count_query = format!(
            "SELECT COUNT(*) as count FROM note n WHERE TRUE {} ",
            filter_clause
        );
        let mut param_idx = 1;

        add_tag_filters(&mut count_query, &mut param_idx, tag_count);
        add_date_filters(
            &mut count_query,
            &mut param_idx,
            req.created_after.is_some(),
            req.created_before.is_some(),
            req.updated_after.is_some(),
            req.updated_before.is_some(),
        );

        // Execute count query
        let total: i64 = {
            let q = sqlx::query_scalar(&count_query);
            let q = bind_list_request_params!(q, req);
            q.fetch_one(&mut **tx).await.map_err(Error::Database)?
        };

        // Build notes query
        let mut notes_query = format!(
            r#"
            SELECT
                n.id, n.created_at_utc, n.updated_at_utc, n.starred, n.archived,
                n.title, n.metadata, n.document_type_id,
                dt.name as document_type_name,
                no.content as original_content,
                nrc.content as revised_content,
                COALESCE(
                    (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                    ''
                ) as tags
            FROM note n
            JOIN note_original no ON no.note_id = n.id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
            LEFT JOIN document_type dt ON dt.id = n.document_type_id
            WHERE TRUE {}
            "#,
            filter_clause
        );
        param_idx = 1;

        add_tag_filters(&mut notes_query, &mut param_idx, tag_count);
        add_date_filters(
            &mut notes_query,
            &mut param_idx,
            req.created_after.is_some(),
            req.created_before.is_some(),
            req.updated_after.is_some(),
            req.updated_before.is_some(),
        );

        notes_query.push_str(&format!(
            "ORDER BY {} LIMIT ${} OFFSET ${}",
            order_clause,
            param_idx,
            param_idx + 1
        ));

        // Execute notes query
        let rows = {
            let mut q = sqlx::query(&notes_query);
            q = bind_list_request_params!(q, req);
            q = q.bind(limit).bind(offset);
            q.fetch_all(&mut **tx).await.map_err(Error::Database)?
        };

        let notes: Vec<NoteSummary> = rows.into_iter().map(map_row_to_note_summary).collect();

        Ok(ListNotesResponse { notes, total })
    }

    /// Soft delete a note within an existing transaction.
    pub async fn soft_delete_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET deleted_at = $1, updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Hard delete a note within an existing transaction.
    pub async fn hard_delete_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Restore a soft-deleted note within an existing transaction.
    pub async fn restore_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET deleted_at = NULL, updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Update note status within an existing transaction.
    pub async fn update_status_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: UpdateNoteStatusRequest,
    ) -> Result<()> {
        // Check if note exists first (issue #362)
        if !self.exists_tx(tx, id).await? {
            return Err(Error::NotFound(format!("Note {} not found", id)));
        }
        let mut updates: Vec<String> = vec!["updated_at_utc = $1".to_string()];
        let now = Utc::now();
        // $1 = now, $2 = id, then dynamic params start at $3
        let mut param_idx = 3;

        if req.starred.is_some() {
            updates.push(format!("starred = ${}", param_idx));
            param_idx += 1;
        }
        if req.archived.is_some() {
            updates.push(format!("archived = ${}", param_idx));
            param_idx += 1;
        }
        if req.metadata.is_some() {
            // Merge new metadata keys into existing (issue #122) instead of replacing
            updates.push(format!(
                "metadata = COALESCE(metadata, '{{}}'::jsonb) || ${}",
                param_idx
            ));
        }

        let query = format!("UPDATE note SET {} WHERE id = $2", updates.join(", "));

        let mut q = sqlx::query(&query).bind(now).bind(id);
        if let Some(starred) = req.starred {
            q = q.bind(starred);
        }
        if let Some(archived) = req.archived {
            q = q.bind(archived);
        }
        if let Some(metadata) = req.metadata {
            q = q.bind(metadata);
        }

        q.execute(&mut **tx).await.map_err(Error::Database)?;
        Ok(())
    }

    /// Update original note content within an existing transaction.
    pub async fn update_original_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        content: &str,
    ) -> Result<()> {
        // Check if note exists first (issue #362)
        if !self.exists_tx(tx, id).await? {
            return Err(Error::NotFound(format!("Note {} not found", id)));
        }
        let now = Utc::now();
        let hash = Self::hash_content(content);

        sqlx::query(
            "UPDATE note_original SET content = $1, hash = $2, user_last_edited_at = $3 WHERE note_id = $4",
        )
        .bind(content)
        .bind(&hash)
        .bind(now)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query("UPDATE note SET updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        sqlx::query(
            "INSERT INTO activity_log (id, at_utc, actor, action, note_id, meta)
             VALUES ($1, $2, 'user', 'update_original', $3, '{}'::jsonb)",
        )
        .bind(new_v7())
        .bind(now)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Update revised note content within an existing transaction.
    pub async fn update_revised_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        content: &str,
        rationale: Option<&str>,
    ) -> Result<Uuid> {
        let now = Utc::now();
        let revision_id = new_v7();

        // Insert revision record into history table
        sqlx::query(
            "INSERT INTO note_revision (id, note_id, content, rationale, created_at_utc, revision_number)
             VALUES ($1, $2, $3, $4, $5, COALESCE((SELECT MAX(revision_number) FROM note_revision WHERE note_id = $2), 0) + 1)",
        )
        .bind(revision_id)
        .bind(id)
        .bind(content)
        .bind(rationale)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Update the current revision snapshot (note_revised_current is a materialized table, not a view)
        sqlx::query(
            "UPDATE note_revised_current SET content = $1, last_revision_id = $2 WHERE note_id = $3",
        )
        .bind(content)
        .bind(revision_id)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Update note timestamp
        sqlx::query("UPDATE note SET updated_at_utc = $1 WHERE id = $2")
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(revision_id)
    }

    /// Check if note exists within an existing transaction.
    pub async fn exists_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<bool> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM note WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(exists)
    }

    /// Update note title within an existing transaction.
    pub async fn update_title_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        title: &str,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE note SET title = $1, updated_at_utc = $2 WHERE id = $3")
            .bind(title)
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// List all note IDs within an existing transaction.
    pub async fn list_all_ids_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Vec<Uuid>> {
        let sql = "SELECT id FROM note WHERE deleted_at IS NULL ORDER BY created_at_utc DESC";
        let rows = sqlx::query(sql)
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(rows.into_iter().map(|r| r.get("id")).collect())
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
        let validated_order = validate_sort_order(sort_order);
        let order_clause = match sort_by {
            "updated_at" => format!("n.updated_at_utc {}", validated_order),
            "accessed_at" => format!(
                "COALESCE(n.last_accessed_at, n.created_at_utc) {}",
                validated_order
            ),
            "title" => format!("n.title {} NULLS LAST", validated_order),
            _ => format!("n.created_at_utc {}", validated_order),
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
                QueryParam::StringArray(arr) => count_q.bind(arr),
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
                QueryParam::StringArray(arr) => notes_q.bind(arr),
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
                // has_revision should only be true if revised content differs from original
                let has_revision = match (&revised, &original) {
                    (Some(r), Some(o)) => r != o,
                    (Some(_), None) => true,
                    _ => false,
                };
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
                    document_type_id: None, // Not fetched in strict filter query
                    document_type_name: None,
                    embedding_status: None,
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
                QueryParam::StringArray(arr) => q.bind(arr),
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
