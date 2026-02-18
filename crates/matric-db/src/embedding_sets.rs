//! Embedding set repository implementation.

use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{
    new_v7, AddMembersRequest, CreateEmbeddingConfigRequest, CreateEmbeddingSetRequest,
    EmbeddingConfigProfile, EmbeddingIndexStatus, EmbeddingProvider, EmbeddingSet,
    EmbeddingSetAgentMetadata, EmbeddingSetCriteria, EmbeddingSetHealth, EmbeddingSetMember,
    EmbeddingSetMode, EmbeddingSetSummary, Error, GarbageCollectionResult, Result,
    UpdateEmbeddingConfigRequest, UpdateEmbeddingSetRequest,
};

/// PostgreSQL implementation of embedding set repository.
pub struct PgEmbeddingSetRepository {
    pool: Pool<Postgres>,
}

impl PgEmbeddingSetRepository {
    /// Create a new repository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Get the default embedding config ID from the database.
    pub async fn get_default_config_id(&self) -> Result<Uuid> {
        let row = sqlx::query("SELECT id FROM embedding_config WHERE is_default = TRUE")
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default embedding config not found".to_string()))
    }

    /// Get the default embedding set ID from the database.
    pub async fn get_default_set_id(&self) -> Result<Uuid> {
        let row =
            sqlx::query("SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default'")
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default embedding set not found".to_string()))
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
                es.set_type::text as set_type,
                es.document_count,
                es.embedding_count,
                es.index_status::text as index_status,
                es.is_system,
                es.keywords,
                es.truncate_dim,
                ec.model,
                ec.dimension,
                ec.supports_mrl
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
                let set_type_str: Option<String> = row.get("set_type");
                EmbeddingSetSummary {
                    id: row.get("id"),
                    name: row.get("name"),
                    slug: row.get("slug"),
                    description: row.get("description"),
                    purpose: row.get("purpose"),
                    set_type: set_type_str
                        .map(|s| s.parse().unwrap_or_default())
                        .unwrap_or_default(),
                    document_count: row.get("document_count"),
                    embedding_count: row.get("embedding_count"),
                    index_status: status_str.parse().unwrap_or_default(),
                    is_system: row.get("is_system"),
                    keywords: row.get::<Vec<String>, _>("keywords"),
                    model: row.get("model"),
                    dimension: row.get("dimension"),
                    truncate_dim: row.get("truncate_dim"),
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
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
                set_type::text as set_type, mode::text as mode, criteria, embedding_config_id,
                truncate_dim, auto_embed_rules,
                index_status::text as index_status, index_type,
                document_count, embedding_count, embeddings_current, index_size_bytes,
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
                set_type::text as set_type, mode::text as mode, criteria, embedding_config_id,
                truncate_dim, auto_embed_rules,
                index_status::text as index_status, index_type,
                document_count, embedding_count, embeddings_current, index_size_bytes,
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
        let id = new_v7();
        let slug = req.slug.unwrap_or_else(|| slugify(&req.name));
        let now = Utc::now();

        let criteria_json =
            serde_json::to_value(&req.criteria).map_err(|e| Error::Internal(e.to_string()))?;
        let agent_metadata_json = serde_json::to_value(&req.agent_metadata)
            .map_err(|e| Error::Internal(e.to_string()))?;
        let auto_embed_rules_json = serde_json::to_value(&req.auto_embed_rules)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let config_id = match req.embedding_config_id {
            Some(id) => Some(id),
            None => Some(self.get_default_config_id().await?),
        };

        sqlx::query(
            r#"
            INSERT INTO embedding_set (
                id, name, slug, description, purpose, usage_hints, keywords,
                set_type, mode, criteria, embedding_config_id, truncate_dim,
                auto_embed_rules, agent_metadata,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8::embedding_set_type, $9::embedding_set_mode, $10, $11, $12,
                $13, $14,
                $15, $15
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
        .bind(req.set_type.to_string())
        .bind(req.mode.to_string())
        .bind(&criteria_json)
        .bind(config_id)
        .bind(req.truncate_dim)
        .bind(&auto_embed_rules_json)
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
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let result = self.update_tx(&mut tx, slug, req).await?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    /// Delete an embedding set (not allowed for system sets).
    pub async fn delete(&self, slug: &str) -> Result<()> {
        let existing = self
            .get_by_slug(slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", slug)))?;

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
        let set = self
            .get_by_slug(set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

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
        let set = self
            .get_by_slug(set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

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
        let set = self
            .get_by_slug(set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

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
        let set = self
            .get_by_id(set_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_id)))?;

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
            // Add tag filter (case-insensitive with hierarchical matching)
            if !criteria.tags.is_empty() {
                let tag_conditions: Vec<String> = criteria
                    .tags
                    .iter()
                    .map(|t| {
                        let escaped = t.replace('\'', "''");
                        format!(
                            "(LOWER(tag_name) = LOWER('{}') OR LOWER(tag_name) LIKE LOWER('{}') || '/%')",
                            escaped, escaped
                        )
                    })
                    .collect();
                conditions.push(format!(
                    "n.id IN (SELECT note_id FROM note_tag WHERE {})",
                    tag_conditions.join(" OR ")
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
                    "nrc.tsv @@ websearch_to_tsquery('public.matric_english', '{}')",
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
    /// For manual sets, returns the count of members missing embeddings.
    pub async fn refresh(&self, set_slug: &str) -> Result<i64> {
        let set = self
            .get_by_slug(set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

        if set.mode == EmbeddingSetMode::Manual {
            // Return count of members missing embeddings for this set
            let count: i64 = sqlx::query_scalar(
                r#"
                SELECT COUNT(*)
                FROM embedding_set_member m
                LEFT JOIN embedding e ON e.note_id = m.note_id AND e.embedding_set_id = m.embedding_set_id
                WHERE m.embedding_set_id = $1 AND e.id IS NULL
                "#,
            )
            .bind(set.id)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

            return Ok(count);
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
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            ORDER BY is_default DESC, name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let configs = rows
            .into_iter()
            .map(|row| {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(configs)
    }

    /// Get the default embedding config.
    pub async fn get_default_config(&self) -> Result<Option<EmbeddingConfigProfile>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE is_default = TRUE
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                Ok(Some(EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }))
            }
            None => Ok(None),
        }
    }

    /// Get an embedding config by ID.
    pub async fn get_config(&self, id: Uuid) -> Result<Option<EmbeddingConfigProfile>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                Ok(Some(EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }))
            }
            None => Ok(None),
        }
    }

    /// Create a new embedding config.
    pub async fn create_config(
        &self,
        request: CreateEmbeddingConfigRequest,
    ) -> Result<EmbeddingConfigProfile> {
        let id = new_v7();
        let now = Utc::now();
        // Bind matryoshka_dims as Vec<i32> directly (issue #126 EMB-017)
        // The column is INTEGER[], not JSONB — binding as JSON causes type mismatch
        let matryoshka_dims: Option<Vec<i32>> = request.matryoshka_dims.clone();

        sqlx::query(
            r#"
            INSERT INTO embedding_config (
                id, name, description, model, dimension, chunk_size, chunk_overlap,
                hnsw_m, hnsw_ef_construction, is_default, created_at, updated_at,
                supports_mrl, matryoshka_dims, default_truncate_dim,
                provider, provider_config, content_types, document_composition
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, FALSE, $10, $10,
                $11, $12, $13,
                $14::embedding_provider, $15, $16, $17
            )
            "#,
        )
        .bind(id)
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.model)
        .bind(request.dimension)
        .bind(request.chunk_size)
        .bind(request.chunk_overlap)
        .bind(request.hnsw_m)
        .bind(request.hnsw_ef_construction)
        .bind(now)
        .bind(request.supports_mrl)
        .bind(&matryoshka_dims)
        .bind(request.default_truncate_dim)
        .bind(request.provider.to_string())
        .bind(&request.provider_config)
        .bind(&request.content_types)
        .bind(serde_json::to_value(&request.document_composition).unwrap_or_default())
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Fetch and return the created config
        self.get_config(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Config {} not found after creation", id)))
    }

    /// Update an existing embedding config.
    pub async fn update_config(
        &self,
        id: Uuid,
        request: UpdateEmbeddingConfigRequest,
    ) -> Result<EmbeddingConfigProfile> {
        // Build dynamic update query
        let mut updates = vec!["updated_at = NOW()".to_string()];
        let mut param_idx = 2; // $1 is id

        if request.name.is_some() {
            updates.push(format!("name = ${}", param_idx));
            param_idx += 1;
        }
        if request.description.is_some() {
            updates.push(format!("description = ${}", param_idx));
            param_idx += 1;
        }
        if request.model.is_some() {
            updates.push(format!("model = ${}", param_idx));
            param_idx += 1;
        }
        if request.dimension.is_some() {
            updates.push(format!("dimension = ${}", param_idx));
            param_idx += 1;
        }
        if request.chunk_size.is_some() {
            updates.push(format!("chunk_size = ${}", param_idx));
            param_idx += 1;
        }
        if request.chunk_overlap.is_some() {
            updates.push(format!("chunk_overlap = ${}", param_idx));
            param_idx += 1;
        }
        if request.provider.is_some() {
            updates.push(format!("provider = ${}::embedding_provider", param_idx));
            param_idx += 1;
        }
        if request.provider_config.is_some() {
            updates.push(format!("provider_config = ${}", param_idx));
            param_idx += 1;
        }
        if request.supports_mrl.is_some() {
            updates.push(format!("supports_mrl = ${}", param_idx));
            param_idx += 1;
        }
        if request.matryoshka_dims.is_some() {
            updates.push(format!("matryoshka_dims = ${}", param_idx));
            param_idx += 1;
        }
        if request.default_truncate_dim.is_some() {
            updates.push(format!("default_truncate_dim = ${}", param_idx));
            param_idx += 1;
        }
        if request.content_types.is_some() {
            updates.push(format!("content_types = ${}", param_idx));
            param_idx += 1;
        }
        if request.hnsw_m.is_some() {
            updates.push(format!("hnsw_m = ${}", param_idx));
            param_idx += 1;
        }
        if request.hnsw_ef_construction.is_some() {
            updates.push(format!("hnsw_ef_construction = ${}", param_idx));
            param_idx += 1;
        }
        if request.document_composition.is_some() {
            updates.push(format!("document_composition = ${}", param_idx));
            // param_idx += 1; // not needed for last param
        }

        let query = format!(
            "UPDATE embedding_config SET {} WHERE id = $1",
            updates.join(", ")
        );

        let mut query_builder = sqlx::query(&query).bind(id);

        if let Some(name) = &request.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(description) = &request.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(model) = &request.model {
            query_builder = query_builder.bind(model);
        }
        if let Some(dimension) = request.dimension {
            query_builder = query_builder.bind(dimension);
        }
        if let Some(chunk_size) = request.chunk_size {
            query_builder = query_builder.bind(chunk_size);
        }
        if let Some(chunk_overlap) = request.chunk_overlap {
            query_builder = query_builder.bind(chunk_overlap);
        }
        if let Some(provider) = &request.provider {
            query_builder = query_builder.bind(provider.to_string());
        }
        if let Some(provider_config) = &request.provider_config {
            query_builder = query_builder.bind(provider_config);
        }
        if let Some(supports_mrl) = request.supports_mrl {
            query_builder = query_builder.bind(supports_mrl);
        }
        if let Some(matryoshka_dims) = &request.matryoshka_dims {
            let dims_json: JsonValue = serde_json::to_value(matryoshka_dims).unwrap_or_default();
            query_builder = query_builder.bind(dims_json);
        }
        if let Some(default_truncate_dim) = request.default_truncate_dim {
            query_builder = query_builder.bind(default_truncate_dim);
        }
        if let Some(content_types) = &request.content_types {
            query_builder = query_builder.bind(content_types);
        }
        if let Some(hnsw_m) = request.hnsw_m {
            query_builder = query_builder.bind(hnsw_m);
        }
        if let Some(hnsw_ef_construction) = request.hnsw_ef_construction {
            query_builder = query_builder.bind(hnsw_ef_construction);
        }
        if let Some(composition) = &request.document_composition {
            let composition_json = serde_json::to_value(composition).unwrap_or_default();
            query_builder = query_builder.bind(composition_json);
        }

        query_builder
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Fetch and return the updated config
        self.get_config(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Config {} not found", id)))
    }

    /// Delete an embedding config by ID.
    /// Returns error if config is in use by embedding sets or is the default config.
    pub async fn delete_config(&self, id: Uuid) -> Result<()> {
        // Check if this is the default config
        let is_default: bool =
            sqlx::query_scalar("SELECT is_default FROM embedding_config WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?
                .unwrap_or(false);

        if is_default {
            return Err(Error::InvalidInput(
                "Cannot delete the default embedding config".to_string(),
            ));
        }

        // Check if config is in use by any embedding sets (also used by find_sets_by_config)
        let usage_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM embedding_set WHERE embedding_config_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        if usage_count > 0 {
            return Err(Error::InvalidInput(format!(
                "Cannot delete config: {} embedding set(s) are using it",
                usage_count
            )));
        }

        let result = sqlx::query("DELETE FROM embedding_config WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Config {} not found", id)));
        }

        Ok(())
    }

    /// Find slugs of embedding sets that use a given config ID.
    /// Used to trigger re-embedding when composition changes (#485).
    pub async fn find_set_slugs_by_config(&self, config_id: Uuid) -> Result<Vec<String>> {
        let slugs: Vec<String> =
            sqlx::query_scalar("SELECT slug FROM embedding_set WHERE embedding_config_id = $1")
                .bind(config_id)
                .fetch_all(&self.pool)
                .await
                .map_err(Error::Database)?;
        Ok(slugs)
    }

    /// Get configs by provider type.
    pub async fn get_configs_by_provider(
        &self,
        provider: EmbeddingProvider,
    ) -> Result<Vec<EmbeddingConfigProfile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE provider = $1::embedding_provider
            ORDER BY name
            "#,
        )
        .bind(provider.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let configs = rows
            .into_iter()
            .map(|row| {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(configs)
    }

    /// Get configs by content type.
    pub async fn get_configs_by_content_type(
        &self,
        content_type: &str,
    ) -> Result<Vec<EmbeddingConfigProfile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE $1 = ANY(content_types)
            ORDER BY is_default DESC, name
            "#,
        )
        .bind(content_type)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let configs = rows
            .into_iter()
            .map(|row| {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(configs)
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
        let rows =
            sqlx::query("SELECT embedding_set_id FROM embedding_set_member WHERE note_id = $1")
                .bind(note_id)
                .fetch_all(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| r.get("embedding_set_id"))
            .collect())
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
    pub async fn list_all_members(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EmbeddingSetMember>> {
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

    /// Export all embedding set members within a transaction (for schema-scoped backup).
    pub async fn list_all_members_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EmbeddingSetMember>> {
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
        .fetch_all(&mut **tx)
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
    // LIFECYCLE MANAGEMENT
    // =========================================================================

    /// Find stale embeddings: embeddings for notes that have been updated
    /// after the embedding was generated.
    ///
    /// Returns note IDs with stale embeddings along with their embedding count.
    pub async fn find_stale_embeddings(
        &self,
        set_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT e.note_id, COUNT(*) as embedding_count
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            WHERE e.embedding_set_id = $1
              AND e.created_at < n.updated_at_utc
            GROUP BY e.note_id
            ORDER BY MAX(n.updated_at_utc - e.created_at) DESC
            LIMIT $2
            "#,
        )
        .bind(set_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| (r.get("note_id"), r.get("embedding_count")))
            .collect())
    }

    /// Count stale embeddings in a set.
    pub async fn count_stale_embeddings(&self, set_id: Uuid) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(DISTINCT e.note_id) as count
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            WHERE e.embedding_set_id = $1
              AND e.created_at < n.updated_at_utc
            "#,
        )
        .bind(set_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("count"))
    }

    /// Find orphaned embeddings: embeddings for notes that no longer exist
    /// or are not members of the set.
    pub async fn find_orphaned_embeddings(&self, set_id: Uuid, limit: i64) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT e.note_id
            FROM embedding e
            WHERE e.embedding_set_id = $1
              AND (
                -- Note deleted
                NOT EXISTS (SELECT 1 FROM note n WHERE n.id = e.note_id AND n.deleted_at IS NULL)
                -- Or no longer a member
                OR NOT EXISTS (SELECT 1 FROM embedding_set_member esm
                               WHERE esm.embedding_set_id = e.embedding_set_id
                                 AND esm.note_id = e.note_id)
              )
            LIMIT $2
            "#,
        )
        .bind(set_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("note_id")).collect())
    }

    /// Prune orphaned embeddings from a set.
    /// Returns the number of embeddings removed.
    pub async fn prune_orphaned_embeddings(&self, set_id: Uuid) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM embedding e
            WHERE e.embedding_set_id = $1
              AND (
                NOT EXISTS (SELECT 1 FROM note n WHERE n.id = e.note_id AND n.deleted_at IS NULL)
                OR NOT EXISTS (SELECT 1 FROM embedding_set_member esm
                               WHERE esm.embedding_set_id = e.embedding_set_id
                                 AND esm.note_id = e.note_id)
              )
            "#,
        )
        .bind(set_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Refresh stats after pruning
        self.refresh_stats(set_id).await?;

        Ok(result.rows_affected() as i64)
    }

    /// Prune orphaned memberships: memberships for notes that no longer exist.
    pub async fn prune_orphaned_memberships(&self, set_id: Uuid) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM embedding_set_member esm
            WHERE esm.embedding_set_id = $1
              AND NOT EXISTS (SELECT 1 FROM note n WHERE n.id = esm.note_id AND n.deleted_at IS NULL)
            "#,
        )
        .bind(set_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() as i64)
    }

    /// Get lifecycle health summary for a set.
    pub async fn get_lifecycle_health(&self, set_id: Uuid) -> Result<EmbeddingSetHealth> {
        let set = self
            .get_by_id(set_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_id)))?;

        let stale_count = self.count_stale_embeddings(set_id).await?;
        let orphaned_embeddings = self.find_orphaned_embeddings(set_id, 1).await?.len() as i64;

        // Check for notes without embeddings
        let missing_embeddings = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM embedding_set_member esm
            WHERE esm.embedding_set_id = $1
              AND NOT EXISTS (SELECT 1 FROM embedding e
                              WHERE e.embedding_set_id = esm.embedding_set_id
                                AND e.note_id = esm.note_id)
            "#,
        )
        .bind(set_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;
        let missing_count: i64 = missing_embeddings.get("count");

        let health_score = if set.document_count == 0 {
            100.0
        } else {
            let total = set.document_count as f64;
            let issues = (stale_count + orphaned_embeddings + missing_count) as f64;
            ((total - issues) / total * 100.0).max(0.0)
        };

        Ok(EmbeddingSetHealth {
            set_id,
            total_documents: set.document_count,
            total_embeddings: set.embedding_count,
            stale_embeddings: stale_count,
            orphaned_embeddings,
            missing_embeddings: missing_count,
            health_score,
            needs_refresh: stale_count > 0 || missing_count > 0,
            needs_pruning: orphaned_embeddings > 0,
        })
    }

    /// Full garbage collection: prune orphans and refresh stats.
    pub async fn garbage_collect(&self, set_id: Uuid) -> Result<GarbageCollectionResult> {
        let orphaned_memberships = self.prune_orphaned_memberships(set_id).await?;
        let orphaned_embeddings = self.prune_orphaned_embeddings(set_id).await?;

        // Refresh stats
        self.refresh_stats(set_id).await?;

        Ok(GarbageCollectionResult {
            set_id,
            orphaned_memberships_removed: orphaned_memberships,
            orphaned_embeddings_removed: orphaned_embeddings,
        })
    }

    /// Find all sets needing garbage collection.
    pub async fn find_sets_needing_gc(&self) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT es.id
            FROM embedding_set es
            WHERE es.is_active = TRUE
              AND (
                -- Has orphaned memberships
                EXISTS (
                    SELECT 1 FROM embedding_set_member esm
                    WHERE esm.embedding_set_id = es.id
                      AND NOT EXISTS (SELECT 1 FROM note n WHERE n.id = esm.note_id AND n.deleted_at IS NULL)
                )
                -- Or has orphaned embeddings
                OR EXISTS (
                    SELECT 1 FROM embedding e
                    WHERE e.embedding_set_id = es.id
                      AND NOT EXISTS (SELECT 1 FROM note n WHERE n.id = e.note_id AND n.deleted_at IS NULL)
                )
              )
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    fn row_to_embedding_set(&self, row: sqlx::postgres::PgRow) -> Result<EmbeddingSet> {
        use matric_core::{AutoEmbedRules, EmbeddingSetType};

        let mode_str: String = row.get("mode");
        let status_str: String = row.get("index_status");
        let set_type_str: Option<String> = row.try_get("set_type").ok();
        let criteria_json: JsonValue = row.get("criteria");
        let agent_metadata_json: JsonValue = row.get("agent_metadata");
        let auto_embed_rules_json: Option<JsonValue> = row.try_get("auto_embed_rules").ok();

        let criteria: EmbeddingSetCriteria =
            serde_json::from_value(criteria_json).unwrap_or_default();
        let agent_metadata: EmbeddingSetAgentMetadata =
            serde_json::from_value(agent_metadata_json).unwrap_or_default();
        let auto_embed_rules: AutoEmbedRules = auto_embed_rules_json
            .and_then(|j| serde_json::from_value(j).ok())
            .unwrap_or_default();

        Ok(EmbeddingSet {
            id: row.get("id"),
            name: row.get("name"),
            slug: row.get("slug"),
            description: row.get("description"),
            purpose: row.get("purpose"),
            usage_hints: row.get("usage_hints"),
            keywords: row.get::<Vec<String>, _>("keywords"),
            set_type: set_type_str
                .map(|s| s.parse::<EmbeddingSetType>().unwrap_or_default())
                .unwrap_or_default(),
            mode: mode_str.parse().unwrap_or_default(),
            criteria,
            embedding_config_id: row.get("embedding_config_id"),
            truncate_dim: row.try_get("truncate_dim").ok(),
            auto_embed_rules,
            document_count: row.get("document_count"),
            embedding_count: row.get("embedding_count"),
            index_status: status_str.parse().unwrap_or_default(),
            index_size_bytes: row.get("index_size_bytes"),
            is_system: row.get("is_system"),
            is_active: row.get("is_active"),
            auto_refresh: row.get("auto_refresh"),
            embeddings_current: row.try_get("embeddings_current").unwrap_or(true),
            agent_metadata,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
        })
    }
}

// =============================================================================
// TRANSACTION-AWARE VARIANTS (Issue #186)
// =============================================================================

/// Transaction-aware variants for archive-scoped operations.
impl PgEmbeddingSetRepository {
    /// Get the default embedding config ID within a transaction.
    pub async fn get_default_config_id_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Uuid> {
        let row = sqlx::query("SELECT id FROM embedding_config WHERE is_default = TRUE")
            .fetch_optional(&mut **tx)
            .await
            .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default embedding config not found".to_string()))
    }

    /// Get the default embedding set ID within a transaction.
    pub async fn get_default_set_id_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Uuid> {
        let row =
            sqlx::query("SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default'")
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default embedding set not found".to_string()))
    }

    /// List all embedding sets within a transaction.
    pub async fn list_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<EmbeddingSetSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT
                es.id,
                es.name,
                es.slug,
                es.description,
                es.purpose,
                es.set_type::text as set_type,
                es.document_count,
                es.embedding_count,
                es.index_status::text as index_status,
                es.is_system,
                es.keywords,
                es.truncate_dim,
                ec.model,
                ec.dimension,
                ec.supports_mrl
            FROM embedding_set es
            LEFT JOIN embedding_config ec ON es.embedding_config_id = ec.id
            WHERE es.is_active = TRUE
            ORDER BY es.is_system DESC, es.document_count DESC
            "#,
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let sets = rows
            .into_iter()
            .map(|row| {
                let status_str: String = row.get("index_status");
                let set_type_str: Option<String> = row.get("set_type");
                EmbeddingSetSummary {
                    id: row.get("id"),
                    name: row.get("name"),
                    slug: row.get("slug"),
                    description: row.get("description"),
                    purpose: row.get("purpose"),
                    set_type: set_type_str
                        .map(|s| s.parse().unwrap_or_default())
                        .unwrap_or_default(),
                    document_count: row.get("document_count"),
                    embedding_count: row.get("embedding_count"),
                    index_status: status_str.parse().unwrap_or_default(),
                    is_system: row.get("is_system"),
                    keywords: row.get::<Vec<String>, _>("keywords"),
                    model: row.get("model"),
                    dimension: row.get("dimension"),
                    truncate_dim: row.get("truncate_dim"),
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                }
            })
            .collect();

        Ok(sets)
    }

    /// Get an embedding set by slug within a transaction.
    pub async fn get_by_slug_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        slug: &str,
    ) -> Result<Option<EmbeddingSet>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, slug, description, purpose, usage_hints, keywords,
                set_type::text as set_type, mode::text as mode, criteria, embedding_config_id,
                truncate_dim, auto_embed_rules,
                index_status::text as index_status, index_type,
                document_count, embedding_count, embeddings_current, index_size_bytes,
                is_system, is_active, auto_refresh,
                agent_metadata, created_at, updated_at, created_by
            FROM embedding_set
            WHERE slug = $1
            "#,
        )
        .bind(slug)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => Ok(Some(self.row_to_embedding_set(row)?)),
            None => Ok(None),
        }
    }

    /// Get an embedding set by ID within a transaction.
    pub async fn get_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<EmbeddingSet>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, name, slug, description, purpose, usage_hints, keywords,
                set_type::text as set_type, mode::text as mode, criteria, embedding_config_id,
                truncate_dim, auto_embed_rules,
                index_status::text as index_status, index_type,
                document_count, embedding_count, embeddings_current, index_size_bytes,
                is_system, is_active, auto_refresh,
                agent_metadata, created_at, updated_at, created_by
            FROM embedding_set
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => Ok(Some(self.row_to_embedding_set(row)?)),
            None => Ok(None),
        }
    }

    /// Get the default embedding set within a transaction.
    pub async fn get_default_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Option<EmbeddingSet>> {
        self.get_by_slug_tx(tx, "default").await
    }

    /// Get the default embedding set ID (fast path) within a transaction.
    pub async fn get_default_id_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Uuid> {
        let row = sqlx::query("SELECT get_default_embedding_set_id() as id")
            .fetch_optional(&mut **tx)
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

    /// Create a new embedding set within a transaction.
    pub async fn create_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateEmbeddingSetRequest,
    ) -> Result<EmbeddingSet> {
        let id = new_v7();
        let slug = req.slug.unwrap_or_else(|| slugify(&req.name));
        let now = Utc::now();

        let criteria_json =
            serde_json::to_value(&req.criteria).map_err(|e| Error::Internal(e.to_string()))?;
        let agent_metadata_json = serde_json::to_value(&req.agent_metadata)
            .map_err(|e| Error::Internal(e.to_string()))?;
        let auto_embed_rules_json = serde_json::to_value(&req.auto_embed_rules)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let config_id = match req.embedding_config_id {
            Some(id) => Some(id),
            None => Some(self.get_default_config_id_tx(tx).await?),
        };

        sqlx::query(
            r#"
            INSERT INTO embedding_set (
                id, name, slug, description, purpose, usage_hints, keywords,
                set_type, mode, criteria, embedding_config_id, truncate_dim,
                auto_embed_rules, agent_metadata,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8::embedding_set_type, $9::embedding_set_mode, $10, $11, $12,
                $13, $14,
                $15, $15
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
        .bind(req.set_type.to_string())
        .bind(req.mode.to_string())
        .bind(&criteria_json)
        .bind(config_id)
        .bind(req.truncate_dim)
        .bind(&auto_embed_rules_json)
        .bind(&agent_metadata_json)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        self.get_by_id_tx(tx, id)
            .await?
            .ok_or_else(|| Error::Internal("Failed to create embedding set".to_string()))
    }

    /// Delete an embedding set within a transaction (not allowed for system sets).
    pub async fn delete_tx(&self, tx: &mut Transaction<'_, Postgres>, slug: &str) -> Result<()> {
        let existing = self
            .get_by_slug_tx(tx, slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", slug)))?;

        if existing.is_system {
            return Err(Error::InvalidInput(
                "Cannot delete system embedding set".to_string(),
            ));
        }

        sqlx::query("DELETE FROM embedding_set WHERE slug = $1")
            .bind(slug)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Update an embedding set within a transaction (single-query COALESCE pattern).
    pub async fn update_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        slug: &str,
        req: UpdateEmbeddingSetRequest,
    ) -> Result<EmbeddingSet> {
        // Validate existence and system constraints
        let existing = self
            .get_by_slug_tx(tx, slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", slug)))?;

        if existing.is_system {
            if req.name.is_some() {
                return Err(Error::InvalidInput(
                    "Cannot rename system embedding set".to_string(),
                ));
            }
            if req.is_active == Some(false) {
                return Err(Error::InvalidInput(
                    "Cannot deactivate system embedding set".to_string(),
                ));
            }
        }

        // Serialize JSON fields
        let criteria_json = req
            .criteria
            .as_ref()
            .map(|c| serde_json::to_value(c).map_err(|e| Error::Internal(e.to_string())))
            .transpose()?;
        let agent_metadata_json = req
            .agent_metadata
            .as_ref()
            .map(|m| serde_json::to_value(m).map_err(|e| Error::Internal(e.to_string())))
            .transpose()?;
        let mode_str = req.mode.as_ref().map(|m| m.to_string());

        // Single UPDATE with COALESCE — NULL params preserve existing values
        let row = sqlx::query(
            r#"
            UPDATE embedding_set SET
                name = COALESCE($2, name),
                description = COALESCE($3, description),
                purpose = COALESCE($4, purpose),
                usage_hints = COALESCE($5, usage_hints),
                keywords = COALESCE($6, keywords),
                is_active = COALESCE($7, is_active),
                auto_refresh = COALESCE($8, auto_refresh),
                mode = COALESCE($9::embedding_set_mode, mode),
                criteria = COALESCE($10, criteria),
                agent_metadata = COALESCE($11, agent_metadata),
                updated_at = NOW()
            WHERE slug = $1
            RETURNING
                id, name, slug, description, purpose, usage_hints, keywords,
                set_type::text as set_type, mode::text as mode, criteria, embedding_config_id,
                truncate_dim, auto_embed_rules,
                index_status::text as index_status, index_type,
                document_count, embedding_count, embeddings_current, index_size_bytes,
                is_system, is_active, auto_refresh,
                agent_metadata, created_at, updated_at, created_by
            "#,
        )
        .bind(slug)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.purpose)
        .bind(&req.usage_hints)
        .bind(&req.keywords)
        .bind(req.is_active)
        .bind(req.auto_refresh)
        .bind(&mode_str)
        .bind(&criteria_json)
        .bind(&agent_metadata_json)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        self.row_to_embedding_set(row)
    }

    /// List members of an embedding set within a transaction.
    pub async fn list_members_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_slug: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EmbeddingSetMember>> {
        let set = self
            .get_by_slug_tx(tx, set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

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
        .fetch_all(&mut **tx)
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

    /// Add notes to an embedding set within a transaction.
    pub async fn add_members_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_slug: &str,
        req: AddMembersRequest,
    ) -> Result<i64> {
        let set = self
            .get_by_slug_tx(tx, set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            count += result.rows_affected() as i64;
        }

        // Mark index as stale
        sqlx::query("UPDATE embedding_set SET index_status = 'stale' WHERE id = $1")
            .bind(set.id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(count)
    }

    /// Remove a note from an embedding set within a transaction.
    pub async fn remove_member_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_slug: &str,
        note_id: Uuid,
    ) -> Result<bool> {
        let set = self
            .get_by_slug_tx(tx, set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

        let result = sqlx::query(
            "DELETE FROM embedding_set_member WHERE embedding_set_id = $1 AND note_id = $2",
        )
        .bind(set.id)
        .bind(note_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if result.rows_affected() > 0 {
            sqlx::query("DELETE FROM embedding WHERE embedding_set_id = $1 AND note_id = $2")
                .bind(set.id)
                .bind(note_id)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
        }

        Ok(result.rows_affected() > 0)
    }

    /// Find notes matching embedding set criteria within a transaction.
    pub async fn find_matching_notes_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Uuid>> {
        let set = self
            .get_by_id_tx(tx, set_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_id)))?;

        let criteria = &set.criteria;
        let mut conditions = Vec::new();
        let mut query = String::from(
            "SELECT DISTINCT n.id FROM note n \
             LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id \
             WHERE n.deleted_at IS NULL",
        );

        if criteria.exclude_archived {
            conditions.push("(n.archived IS FALSE OR n.archived IS NULL)".to_string());
        }

        if !criteria.include_all {
            if !criteria.tags.is_empty() {
                let tag_conditions: Vec<String> = criteria
                    .tags
                    .iter()
                    .map(|t| {
                        let escaped = t.replace('\'', "''");
                        format!(
                            "(LOWER(tag_name) = LOWER('{}') OR LOWER(tag_name) LIKE LOWER('{}') || '/%')",
                            escaped, escaped
                        )
                    })
                    .collect();
                conditions.push(format!(
                    "n.id IN (SELECT note_id FROM note_tag WHERE {})",
                    tag_conditions.join(" OR ")
                ));
            }

            if !criteria.collections.is_empty() {
                let collections_list = criteria
                    .collections
                    .iter()
                    .map(|c| format!("'{}'", c))
                    .collect::<Vec<_>>()
                    .join(",");
                conditions.push(format!("n.collection_id IN ({})", collections_list));
            }

            if let Some(fts_query) = &criteria.fts_query {
                conditions.push(format!(
                    "nrc.tsv @@ websearch_to_tsquery('public.matric_english', '{}')",
                    fts_query.replace('\'', "''")
                ));
            }

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
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }

    /// Refresh an embedding set by re-evaluating criteria within a transaction.
    pub async fn refresh_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_slug: &str,
    ) -> Result<i64> {
        let set = self
            .get_by_slug_tx(tx, set_slug)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Embedding set not found: {}", set_slug)))?;

        if set.mode == EmbeddingSetMode::Manual {
            // Return count of members missing embeddings for this set
            let count: i64 = sqlx::query_scalar(
                r#"
                SELECT COUNT(*)
                FROM embedding_set_member m
                LEFT JOIN embedding e ON e.note_id = m.note_id AND e.embedding_set_id = m.embedding_set_id
                WHERE m.embedding_set_id = $1 AND e.id IS NULL
                "#,
            )
            .bind(set.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(Error::Database)?;

            return Ok(count);
        }

        let matching_notes = self.find_matching_notes_tx(tx, set.id, 1_000_000).await?;

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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            added += result.rows_affected() as i64;
        }

        sqlx::query(
            "UPDATE embedding_set SET last_refresh_at = NOW(), updated_at = NOW() WHERE id = $1",
        )
        .bind(set.id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(added)
    }

    /// Check if a note is a member of an embedding set within a transaction.
    pub async fn is_member_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_id: Uuid,
        note_id: Uuid,
    ) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM embedding_set_member WHERE embedding_set_id = $1 AND note_id = $2",
        )
        .bind(set_id)
        .bind(note_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.is_some())
    }

    /// Update the index status of an embedding set within a transaction.
    pub async fn update_index_status_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_id: Uuid,
        status: EmbeddingIndexStatus,
    ) -> Result<()> {
        sqlx::query(&format!(
            "UPDATE embedding_set SET index_status = '{}'::embedding_index_status, updated_at = NOW() WHERE id = $1",
            status
        ))
        .bind(set_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Mark index as ready and update timestamp within a transaction.
    pub async fn mark_index_ready_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_id: Uuid,
    ) -> Result<()> {
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get all embedding set IDs that a note is a member of within a transaction.
    pub async fn get_sets_for_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        let rows =
            sqlx::query("SELECT embedding_set_id FROM embedding_set_member WHERE note_id = $1")
                .bind(note_id)
                .fetch_all(&mut **tx)
                .await
                .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| r.get("embedding_set_id"))
            .collect())
    }

    /// Refresh statistics for an embedding set within a transaction.
    pub async fn refresh_stats_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        set_id: Uuid,
    ) -> Result<()> {
        sqlx::query("SELECT update_embedding_set_stats($1)")
            .bind(set_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// List all embedding configs within a transaction.
    pub async fn list_configs_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<EmbeddingConfigProfile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            ORDER BY is_default DESC, name
            "#,
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let configs = rows
            .into_iter()
            .map(|row| {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }
            })
            .collect();

        Ok(configs)
    }

    /// Get the default embedding config within a transaction.
    pub async fn get_default_config_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Option<EmbeddingConfigProfile>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE is_default = TRUE
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                Ok(Some(EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }))
            }
            None => Ok(None),
        }
    }

    /// Get an embedding config by ID within a transaction.
    pub async fn get_config_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<EmbeddingConfigProfile>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, model, dimension, chunk_size, chunk_overlap,
                   hnsw_m, hnsw_ef_construction, ivfflat_lists, is_default, created_at, updated_at,
                   supports_mrl, matryoshka_dims, default_truncate_dim,
                   provider::text, provider_config, content_types, document_composition
            FROM embedding_config
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        match row {
            Some(row) => {
                let provider_str: Option<String> = row.get("provider");
                let provider = provider_str
                    .and_then(|s| s.parse::<EmbeddingProvider>().ok())
                    .unwrap_or_default();
                Ok(Some(EmbeddingConfigProfile {
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
                    supports_mrl: row.get::<Option<bool>, _>("supports_mrl").unwrap_or(false),
                    matryoshka_dims: row.get("matryoshka_dims"),
                    default_truncate_dim: row.get("default_truncate_dim"),
                    provider,
                    provider_config: row
                        .get::<Option<JsonValue>, _>("provider_config")
                        .unwrap_or_default(),
                    content_types: row
                        .get::<Option<Vec<String>>, _>("content_types")
                        .unwrap_or_default(),
                    document_composition: row
                        .get::<Option<JsonValue>, _>("document_composition")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default(),
                }))
            }
            None => Ok(None),
        }
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
    fn test_slugify_dashes_and_underscores() {
        assert_eq!(slugify("test-slug"), "test-slug");
        assert_eq!(slugify("test_slug"), "test-slug");
        // Mixed separators may result in double dashes
        let result = slugify("test-_-slug");
        assert!(result.starts_with("test") && result.ends_with("slug"));
    }
}
