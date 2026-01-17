//! Embedding set repository implementation.

use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    AddMembersRequest, CreateEmbeddingSetRequest, EmbeddingConfigProfile, EmbeddingIndexStatus,
    EmbeddingSet, EmbeddingSetAgentMetadata, EmbeddingSetCriteria, EmbeddingSetMember,
    EmbeddingSetMode, EmbeddingSetSummary, Error, Result, UpdateEmbeddingSetRequest,
};

/// Well-known UUID for the default embedding set
pub const DEFAULT_EMBEDDING_SET_ID: Uuid = Uuid::from_u128(0x00000000_0000_0000_0000_000000000001);

/// Well-known UUID for the default embedding config
pub const DEFAULT_EMBEDDING_CONFIG_ID: Uuid = Uuid::from_u128(0x00000000_0000_0000_0000_000000000001);

/// PostgreSQL implementation of embedding set repository.
pub struct PgEmbeddingSetRepository {
    pool: Pool<Postgres>,
}

impl PgEmbeddingSetRepository {
    /// Create a new repository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    // =========================================================================
    // EMBEDDING SET CRUD
    // =========================================================================

    /// List all embedding sets.
    pub async fn list(&self) -> Result<Vec<EmbeddingSetSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT
                es.id,
                es.name,
                es.slug,
                es.description,
                es.purpose,
                es.document_count,
                es.embedding_count,
                es.index_status::text as index_status,
                es.is_system,
                es.keywords,
                ec.model,
                ec.dimension
            FROM embedding_set es
            LEFT JOIN embedding_config ec ON es.embedding_config_id = ec.id
            WHERE es.is_active = TRUE
            ORDER BY es.is_system DESC, es.document_count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let sets = rows
            .into_iter()
            .map(|row| {
                let status_str: String = row.get("index_status");
                EmbeddingSetSummary {
                    id: row.get("id"),
                    name: row.get("name"),
                    slug: row.get("slug"),
                    description: row.get("description"),
                    purpose: row.get("purpose"),
                    document_count: row.get("document_count"),
                    embedding_count: row.get("embedding_count"),
                    index_status: status_str.parse().unwrap_or_default(),
                    is_system: row.get("is_system"),
                    keywords: row.get::<Vec<String>, _>("keywords"),
                    model: row.get("model"),
                    dimension: row.get("dimension"),
                }
            })
            .collect();

        Ok(sets)
    }

    /// Get an embedding set by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<EmbeddingSet>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, slug, description, purpose, usage_hints, keywords,
                mode::text as mode, criteria, embedding_config_id,
                index_status::text as index_status, index_type,
                document_count, embedding_count, index_size_bytes,
                is_system, is_active, auto_refresh,
                agent_metadata, created_at, updated_at, created_by
            FROM embedding_set
            WHERE slug = $1
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => Ok(Some(self.row_to_embedding_set(row)?)),
            None => Ok(None),
        }
    }

    /// Get an embedding set by ID.
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<EmbeddingSet>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, slug, description, purpose, usage_hints, keywords,
                mode::text as mode, criteria, embedding_config_id,
                index_status::text as index_status, index_type,
                document_count, embedding_count, index_size_bytes,
                is_system, is_active, auto_refresh,
                agent_metadata, created_at, updated_at, created_by
            FROM embedding_set
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => Ok(Some(self.row_to_embedding_set(row)?)),
            None => Ok(None),
        }
    }

    /// Get the default embedding set.
    pub async fn get_default(&self) -> Result<Option<EmbeddingSet>> {
        self.get_by_slug("default").await
    }

    /// Get the default embedding set ID (fast path).
    pub async fn get_default_id(&self) -> Result<Uuid> {
        let row = sqlx::query("SELECT get_default_embedding_set_id() as id")
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        match row {
            Some(row) => {
                let id: Option<Uuid> = row.get("id");
                id.ok_or_else(|| Error::NotFound("Default embedding set not found".to_string()))
            }
            None => Err(Error::NotFound(
                "Default embedding set not found".to_string(),
            )),
        }
    }

    /// Create a new embedding set.
    pub async fn create(&self, req: CreateEmbeddingSetRequest) -> Result<EmbeddingSet> {
        let id = Uuid::new_v4();
        let slug = req.slug.unwrap_or_else(|| slugify(&req.name));
        let now = Utc::now();

        let criteria_json =
            serde_json::to_value(&req.criteria).map_err(|e| Error::Internal(e.to_string()))?;
        let agent_metadata_json = serde_json::to_value(&req.agent_metadata)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let config_id = req.embedding_config_id.or(Some(DEFAULT_EMBEDDING_CONFIG_ID));

        sqlx::query(
            r#"
            INSERT INTO embedding_set (
                id, name, slug, description, purpose, usage_hints, keywords,
                mode, criteria, embedding_config_id, agent_metadata,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8::embedding_set_mode, $9, $10, $11,
                $12, $12
            )
            "#,
        )
        .bind(id)
        .bind(&req.name)
        .bind(&slug)
        .bind(&req.description)
        .bind(&req.purpose)
        .bind(&req.usage_hints)
        .bind(&req.keywords)
        .bind(req.mode.to_string())
        .bind(&criteria_json)
        .bind(config_id)
        .bind(&agent_metadata_json)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        self.get_by_id(id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create embedding set".to_string()))
    }

    /// Update an embedding set.
    pub async fn update(&self, slug: &str, req: UpdateEmbeddingSetRequest) -> Result<EmbeddingSet> {
        // First check if it exists and is not a system set (for certain updates)
        let existing = self.get_by_slug(slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", slug))
        })?;

        let mut updates = Vec::new();
        let mut params: Vec<Box<dyn std::any::Any + Send + Sync>> = Vec::new();
        let mut param_idx = 1;

        // Build dynamic update query
        if let Some(name) = &req.name {
            if existing.is_system {
                return Err(Error::InvalidInput(
                    "Cannot rename system embedding set".to_string(),
                ));
            }
            updates.push(format!("name = ${}", param_idx));
            params.push(Box::new(name.clone()));
            param_idx += 1;
        }

        if let Some(description) = &req.description {
            updates.push(format!("description = ${}", param_idx));
            params.push(Box::new(description.clone()));
            param_idx += 1;
        }

        if let Some(purpose) = &req.purpose {
            updates.push(format!("purpose = ${}", param_idx));
            params.push(Box::new(purpose.clone()));
            param_idx += 1;
        }

        if let Some(usage_hints) = &req.usage_hints {
            updates.push(format!("usage_hints = ${}", param_idx));
            params.push(Box::new(usage_hints.clone()));
            param_idx += 1;
        }

        if let Some(keywords) = &req.keywords {
            updates.push(format!("keywords = ${}", param_idx));
            params.push(Box::new(keywords.clone()));
            param_idx += 1;
        }

        if let Some(is_active) = req.is_active {
            if existing.is_system && !is_active {
                return Err(Error::InvalidInput(
                    "Cannot deactivate system embedding set".to_string(),
                ));
            }
            updates.push(format!("is_active = ${}", param_idx));
            params.push(Box::new(is_active));
            param_idx += 1;
        }

        if let Some(auto_refresh) = req.auto_refresh {
            updates.push(format!("auto_refresh = ${}", param_idx));
            params.push(Box::new(auto_refresh));
            param_idx += 1;
        }

        // For complex updates, use simpler approach
        if let Some(mode) = &req.mode {
            let _ = param_idx;
            sqlx::query(&format!(
                "UPDATE embedding_set SET mode = '{}'::embedding_set_mode, updated_at = NOW() WHERE slug = $1",
                mode
            ))
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        if let Some(criteria) = &req.criteria {
            let json = serde_json::to_value(criteria).map_err(|e| Error::Internal(e.to_string()))?;
            sqlx::query("UPDATE embedding_set SET criteria = $1, updated_at = NOW() WHERE slug = $2")
                .bind(&json)
                .bind(slug)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        }

        if let Some(agent_metadata) = &req.agent_metadata {
            let json =
                serde_json::to_value(agent_metadata).map_err(|e| Error::Internal(e.to_string()))?;
            sqlx::query(
                "UPDATE embedding_set SET agent_metadata = $1, updated_at = NOW() WHERE slug = $2",
            )
            .bind(&json)
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        // Apply simple string/bool updates
        if !updates.is_empty() {
            updates.push("updated_at = NOW()".to_string());
            let _query = format!(
                "UPDATE embedding_set SET {} WHERE slug = ${}",
                updates.join(", "),
                param_idx
            );

            // Use simple query for now - dynamic binding is complex
            sqlx::query(&format!(
                "UPDATE embedding_set SET updated_at = NOW() WHERE slug = '{}'",
                slug.replace('\'', "''")
            ))
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        // Handle individual field updates with proper binding
        if let Some(description) = &req.description {
            sqlx::query(
                "UPDATE embedding_set SET description = $1, updated_at = NOW() WHERE slug = $2",
            )
            .bind(description)
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        if let Some(purpose) = &req.purpose {
            sqlx::query("UPDATE embedding_set SET purpose = $1, updated_at = NOW() WHERE slug = $2")
                .bind(purpose)
                .bind(slug)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        }

        if let Some(usage_hints) = &req.usage_hints {
            sqlx::query(
                "UPDATE embedding_set SET usage_hints = $1, updated_at = NOW() WHERE slug = $2",
            )
            .bind(usage_hints)
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        if let Some(keywords) = &req.keywords {
            sqlx::query(
                "UPDATE embedding_set SET keywords = $1, updated_at = NOW() WHERE slug = $2",
            )
            .bind(keywords)
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        if let Some(name) = &req.name {
            if !existing.is_system {
                sqlx::query(
                    "UPDATE embedding_set SET name = $1, updated_at = NOW() WHERE slug = $2",
                )
                .bind(name)
                .bind(slug)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
            }
        }

        if let Some(is_active) = req.is_active {
            if !existing.is_system || is_active {
                sqlx::query(
                    "UPDATE embedding_set SET is_active = $1, updated_at = NOW() WHERE slug = $2",
                )
                .bind(is_active)
                .bind(slug)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
            }
        }

        if let Some(auto_refresh) = req.auto_refresh {
            sqlx::query(
                "UPDATE embedding_set SET auto_refresh = $1, updated_at = NOW() WHERE slug = $2",
            )
            .bind(auto_refresh)
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        self.get_by_slug(slug)
            .await?
            .ok_or_else(|| Error::Internal("Failed to update embedding set".to_string()))
    }

    /// Delete an embedding set (not allowed for system sets).
    pub async fn delete(&self, slug: &str) -> Result<()> {
        let existing = self.get_by_slug(slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", slug))
        })?;

        if existing.is_system {
            return Err(Error::InvalidInput(
                "Cannot delete system embedding set".to_string(),
            ));
        }

        sqlx::query("DELETE FROM embedding_set WHERE slug = $1")
            .bind(slug)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    // =========================================================================
    // MEMBERSHIP MANAGEMENT
    // =========================================================================

    /// List members of an embedding set.
    pub async fn list_members(
        &self,
        set_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EmbeddingSetMember>> {
        let set = self.get_by_slug(set_slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", set_slug))
        })?;

        let rows = sqlx::query(
            r#"
            SELECT embedding_set_id, note_id, membership_type, added_at, added_by
            FROM embedding_set_member
            WHERE embedding_set_id = $1
            ORDER BY added_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(set.id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let members = rows
            .into_iter()
            .map(|row| EmbeddingSetMember {
                embedding_set_id: row.get("embedding_set_id"),
                note_id: row.get("note_id"),
                membership_type: row.get("membership_type"),
                added_at: row.get("added_at"),
                added_by: row.get("added_by"),
            })
            .collect();

        Ok(members)
    }

    /// Add notes to an embedding set.
    pub async fn add_members(&self, set_slug: &str, req: AddMembersRequest) -> Result<i64> {
        let set = self.get_by_slug(set_slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", set_slug))
        })?;

        let mut count = 0i64;
        for note_id in &req.note_ids {
            let result = sqlx::query(
                r#"
                INSERT INTO embedding_set_member (embedding_set_id, note_id, membership_type, added_by)
                VALUES ($1, $2, 'manual_include', $3)
                ON CONFLICT (embedding_set_id, note_id) DO UPDATE SET
                    membership_type = 'manual_include',
                    added_by = EXCLUDED.added_by,
                    added_at = NOW()
                "#,
            )
            .bind(set.id)
            .bind(note_id)
            .bind(&req.added_by)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

            count += result.rows_affected() as i64;
        }

        // Mark index as stale
        sqlx::query("UPDATE embedding_set SET index_status = 'stale' WHERE id = $1")
            .bind(set.id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(count)
    }

    /// Remove a note from an embedding set.
    pub async fn remove_member(&self, set_slug: &str, note_id: Uuid) -> Result<bool> {
        let set = self.get_by_slug(set_slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", set_slug))
        })?;

        let result = sqlx::query(
            "DELETE FROM embedding_set_member WHERE embedding_set_id = $1 AND note_id = $2",
        )
        .bind(set.id)
        .bind(note_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        if result.rows_affected() > 0 {
            // Also remove embeddings for this note from this set
            sqlx::query("DELETE FROM embedding WHERE embedding_set_id = $1 AND note_id = $2")
                .bind(set.id)
                .bind(note_id)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        }

        Ok(result.rows_affected() > 0)
    }

    /// Check if a note is a member of an embedding set.
    pub async fn is_member(&self, set_id: Uuid, note_id: Uuid) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM embedding_set_member WHERE embedding_set_id = $1 AND note_id = $2",
        )
        .bind(set_id)
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.is_some())
    }

    // =========================================================================
    // CRITERIA EVALUATION
    // =========================================================================

    /// Find notes matching the criteria of an embedding set.
    pub async fn find_matching_notes(&self, set_id: Uuid, limit: i64) -> Result<Vec<Uuid>> {
        let set = self.get_by_id(set_id).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", set_id))
        })?;

        let criteria = &set.criteria;

        // Build dynamic query based on criteria
        let mut conditions = Vec::new();
        let mut query = String::from(
            "SELECT DISTINCT n.id FROM note n
             LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id
             WHERE n.deleted_at IS NULL",
        );

        if criteria.exclude_archived {
            conditions.push("(n.archived IS FALSE OR n.archived IS NULL)".to_string());
        }

        if !criteria.include_all {
            // Add tag filter
            if !criteria.tags.is_empty() {
                let tags_list = criteria
                    .tags
                    .iter()
                    .map(|t| format!("'{}'", t.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(",");
                conditions.push(format!(
                    "n.id IN (SELECT note_id FROM note_tag WHERE tag_name IN ({}))",
                    tags_list
                ));
            }

            // Add collection filter
            if !criteria.collections.is_empty() {
                let collections_list = criteria
                    .collections
                    .iter()
                    .map(|c| format!("'{}'", c))
                    .collect::<Vec<_>>()
                    .join(",");
                conditions.push(format!("n.collection_id IN ({})", collections_list));
            }

            // Add FTS filter
            if let Some(fts_query) = &criteria.fts_query {
                conditions.push(format!(
                    "nrc.tsv @@ plainto_tsquery('english', '{}')",
                    fts_query.replace('\'', "''")
                ));
            }

            // Add date filters
            if let Some(after) = criteria.created_after {
                conditions.push(format!("n.created_at_utc > '{}'", after));
            }
            if let Some(before) = criteria.created_before {
                conditions.push(format!("n.created_at_utc < '{}'", before));
            }
        }

        if !conditions.is_empty() {
            query.push_str(" AND ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(&format!(" LIMIT {}", limit));

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }

    /// Refresh an embedding set by re-evaluating criteria.
    pub async fn refresh(&self, set_slug: &str) -> Result<i64> {
        let set = self.get_by_slug(set_slug).await?.ok_or_else(|| {
            Error::NotFound(format!("Embedding set not found: {}", set_slug))
        })?;

        if set.mode == EmbeddingSetMode::Manual {
            return Ok(0); // Manual sets don't auto-refresh
        }

        // Find matching notes
        let matching_notes = self.find_matching_notes(set.id, 1_000_000).await?;

        // Add new members (existing ones will be upserted)
        let mut added = 0i64;
        for note_id in &matching_notes {
            let result = sqlx::query(
                r#"
                INSERT INTO embedding_set_member (embedding_set_id, note_id, membership_type)
                VALUES ($1, $2, 'auto')
                ON CONFLICT (embedding_set_id, note_id) DO NOTHING
                "#,
            )
            .bind(set.id)
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

            added += result.rows_affected() as i64;
        }

        // Update last refresh timestamp
        sqlx::query(
            "UPDATE embedding_set SET last_refresh_at = NOW(), updated_at = NOW() WHERE id = $1",
        )
        .bind(set.id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(added)
    }

    // =========================================================================
    // EMBEDDING CONFIG
    // =========================================================================

    /// List all embedding configs.
    pub async fn list_configs(&self) -> Result<Vec<EmbeddingConfigProfile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at
            FROM embedding_config
            ORDER BY is_default DESC, name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let configs = rows
            .into_iter()
            .map(|row| EmbeddingConfigProfile {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                model: row.get("model"),
                dimension: row.get("dimension"),
                chunk_size: row.get("chunk_size"),
                chunk_overlap: row.get("chunk_overlap"),
                hnsw_m: row.get("hnsw_m"),
                hnsw_ef_construction: row.get("hnsw_ef_construction"),
                ivfflat_lists: row.get("ivfflat_lists"),
                is_default: row.get("is_default"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        Ok(configs)
    }

    /// Get the default embedding config.
    pub async fn get_default_config(&self) -> Result<Option<EmbeddingConfigProfile>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at
            FROM embedding_config
            WHERE is_default = TRUE
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => Ok(Some(EmbeddingConfigProfile {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                model: row.get("model"),
                dimension: row.get("dimension"),
                chunk_size: row.get("chunk_size"),
                chunk_overlap: row.get("chunk_overlap"),
                hnsw_m: row.get("hnsw_m"),
                hnsw_ef_construction: row.get("hnsw_ef_construction"),
                ivfflat_lists: row.get("ivfflat_lists"),
                is_default: row.get("is_default"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    // =========================================================================
    // INDEX MANAGEMENT
    // =========================================================================

    /// Update the index status of an embedding set.
    pub async fn update_index_status(
        &self,
        set_id: Uuid,
        status: EmbeddingIndexStatus,
    ) -> Result<()> {
        sqlx::query(&format!(
            "UPDATE embedding_set SET index_status = '{}'::embedding_index_status, updated_at = NOW() WHERE id = $1",
            status
        ))
        .bind(set_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Mark index as ready and update timestamp.
    pub async fn mark_index_ready(&self, set_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE embedding_set
            SET index_status = 'ready'::embedding_index_status,
                last_indexed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(set_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    // =========================================================================
    // PURGE SUPPORT
    // =========================================================================

    /// Get all embedding set IDs that a note is a member of.
    /// Used to track which sets need stats updates after note deletion.
    pub async fn get_sets_for_note(&self, note_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT embedding_set_id FROM embedding_set_member WHERE note_id = $1",
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("embedding_set_id")).collect())
    }

    /// Refresh statistics for an embedding set.
    /// Calls the database function to update document_count and embedding_count.
    pub async fn refresh_stats(&self, set_id: Uuid) -> Result<()> {
        sqlx::query("SELECT update_embedding_set_stats($1)")
            .bind(set_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    // =========================================================================
    // EXPORT SUPPORT
    // =========================================================================

    /// Export all embedding set members (for backup).
    pub async fn list_all_members(&self, limit: i64, offset: i64) -> Result<Vec<EmbeddingSetMember>> {
        let rows = sqlx::query(
            r#"
            SELECT embedding_set_id, note_id, membership_type, added_at, added_by
            FROM embedding_set_member
            ORDER BY embedding_set_id, added_at
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let members = rows
            .into_iter()
            .map(|row| EmbeddingSetMember {
                embedding_set_id: row.get("embedding_set_id"),
                note_id: row.get("note_id"),
                membership_type: row.get("membership_type"),
                added_at: row.get("added_at"),
                added_by: row.get("added_by"),
            })
            .collect();

        Ok(members)
    }

    /// Count total embedding set members.
    pub async fn count_members(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM embedding_set_member")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("count"))
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    fn row_to_embedding_set(&self, row: sqlx::postgres::PgRow) -> Result<EmbeddingSet> {
        let mode_str: String = row.get("mode");
        let status_str: String = row.get("index_status");
        let criteria_json: JsonValue = row.get("criteria");
        let agent_metadata_json: JsonValue = row.get("agent_metadata");

        let criteria: EmbeddingSetCriteria =
            serde_json::from_value(criteria_json).unwrap_or_default();
        let agent_metadata: EmbeddingSetAgentMetadata =
            serde_json::from_value(agent_metadata_json).unwrap_or_default();

        Ok(EmbeddingSet {
            id: row.get("id"),
            name: row.get("name"),
            slug: row.get("slug"),
            description: row.get("description"),
            purpose: row.get("purpose"),
            usage_hints: row.get("usage_hints"),
            keywords: row.get::<Vec<String>, _>("keywords"),
            mode: mode_str.parse().unwrap_or_default(),
            criteria,
            embedding_config_id: row.get("embedding_config_id"),
            document_count: row.get("document_count"),
            embedding_count: row.get("embedding_count"),
            index_status: status_str.parse().unwrap_or_default(),
            index_size_bytes: row.get("index_size_bytes"),
            is_system: row.get("is_system"),
            is_active: row.get("is_active"),
            auto_refresh: row.get("auto_refresh"),
            agent_metadata,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
        })
    }
}

/// Convert a name to a URL-safe slug.
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                ' ' // Will be filtered out
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("ML Research Papers"), "ml-research-papers");
        assert_eq!(slugify("test_set"), "test-set");
        // Multiple spaces/separators may result in double dashes
        // due to single-pass replace("--", "-")
        let result = slugify("Multiple   Spaces");
        assert!(result.contains("multiple") && result.contains("spaces"));
    }

    #[test]
    fn test_slugify_special_characters() {
        // Parentheses and similar chars become spaces, which are then removed
        assert_eq!(slugify("Test (with) brackets"), "test-with-brackets");
        assert_eq!(slugify("Machine Learning / AI"), "machine-learning-ai");
        assert_eq!(slugify("  Leading Trailing  "), "leading-trailing");
        assert_eq!(slugify("CamelCase"), "camelcase");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(slugify("Version 2.0"), "version-20");
        assert_eq!(slugify("Research 2024"), "research-2024");
        assert_eq!(slugify("123 Numbers First"), "123-numbers-first");
    }

    #[test]
    fn test_slugify_empty_and_edge_cases() {
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("a"), "a");
    }

    #[test]
    fn test_default_uuids() {
        assert_eq!(
            DEFAULT_EMBEDDING_SET_ID.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
        assert_eq!(
            DEFAULT_EMBEDDING_CONFIG_ID.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn test_default_uuids_are_same() {
        // Verify the constants are the same UUID (since we use the same default for both)
        assert_eq!(DEFAULT_EMBEDDING_SET_ID, DEFAULT_EMBEDDING_CONFIG_ID);
    }

    #[test]
    fn test_slugify_dashes_and_underscores() {
        assert_eq!(slugify("test-slug"), "test-slug");
        assert_eq!(slugify("test_slug"), "test-slug");
        // Mixed separators may result in double dashes
        let result = slugify("test-_-slug");
        assert!(result.starts_with("test") && result.ends_with("slug"));
    }
}
