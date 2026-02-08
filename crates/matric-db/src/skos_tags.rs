//! W3C SKOS-Compliant Hierarchical Tag Repository Implementation
//!
//! This module provides PostgreSQL repository implementations for the full
//! SKOS data model including concept schemes, concepts, labels, notes,
//! semantic relations, mapping relations, and note tagging.
//!
//! # Architecture
//!
//! The repository is split into several focused components:
//! - `PgSkosConceptSchemeRepository` - Vocabulary namespace management
//! - `PgSkosConceptRepository` - Core concept CRUD and search
//! - `PgSkosRelationRepository` - Semantic and mapping relations
//! - `PgSkosTaggingRepository` - Note-to-concept tagging
//! - `PgSkosGovernanceRepository` - Audit, merge, and governance
//!
//! All components are combined in `PgSkosRepository` for convenience.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{
    new_v7, AddLabelRequest, AddNoteRequest, BatchTagNoteRequest, CreateConceptRequest,
    CreateConceptSchemeRequest, CreateMappingRelationRequest, CreateSemanticRelationRequest, Error,
    MergeConceptsRequest, NoteSkosConceptTag, ResolvedTag, Result, SearchConceptsRequest,
    SearchConceptsResponse, SkosAuditLogEntry, SkosConcept, SkosConceptFull, SkosConceptHierarchy,
    SkosConceptLabel, SkosConceptMerge, SkosConceptNote, SkosConceptScheme,
    SkosConceptSchemeSummary, SkosConceptSummary, SkosConceptWithLabel, SkosGovernanceStats,
    SkosLabelType, SkosMappingRelation, SkosMappingRelationEdge, SkosNoteType,
    SkosSemanticRelation, SkosSemanticRelationEdge, SkosTagSpec, TagAntipattern, TagInput,
    TagNoteRequest, TagStatus, UpdateConceptRequest, UpdateConceptSchemeRequest,
    DEFAULT_SCHEME_NOTATION,
};

// =============================================================================
// SQL COLUMN CONSTANTS
// =============================================================================

/// Standard SELECT columns for skos_concept with proper type casting.
/// Use this to avoid type mismatch errors with enum arrays.
const CONCEPT_COLUMNS: &str = r#"
    c.id, c.primary_scheme_id, c.uri, c.notation,
    c.facet_type::text AS facet_type, c.facet_source, c.facet_domain, c.facet_scope,
    c.status::text AS status, c.promoted_at, c.deprecated_at, c.deprecation_reason,
    c.replaced_by_id, c.note_count, c.first_used_at, c.last_used_at,
    c.depth, c.broader_count, c.narrower_count, c.related_count,
    c.antipatterns::TEXT[] AS antipatterns, c.antipattern_checked_at,
    c.created_at, c.updated_at, c.embedding_model, c.embedded_at
"#;

// =============================================================================
// REPOSITORY TRAITS
// =============================================================================

/// Repository trait for SKOS concept scheme operations.
#[async_trait]
pub trait SkosConceptSchemeRepository: Send + Sync {
    /// Create a new concept scheme.
    async fn create_scheme(&self, req: CreateConceptSchemeRequest) -> Result<Uuid>;

    /// Get a concept scheme by ID.
    async fn get_scheme(&self, id: Uuid) -> Result<Option<SkosConceptScheme>>;

    /// Get a concept scheme by notation.
    async fn get_scheme_by_notation(&self, notation: &str) -> Result<Option<SkosConceptScheme>>;

    /// List all concept schemes.
    async fn list_schemes(&self, include_inactive: bool) -> Result<Vec<SkosConceptSchemeSummary>>;

    /// Update a concept scheme.
    async fn update_scheme(&self, id: Uuid, req: UpdateConceptSchemeRequest) -> Result<()>;

    /// Delete a concept scheme. If `force` is true, cascade-delete all concepts.
    async fn delete_scheme(&self, id: Uuid, force: bool) -> Result<()>;

    /// Get top concepts for a scheme.
    async fn get_top_concepts(&self, scheme_id: Uuid) -> Result<Vec<SkosConceptSummary>>;
}

/// Repository trait for SKOS concept operations.
#[async_trait]
pub trait SkosConceptRepository: Send + Sync {
    /// Create a new concept.
    async fn create_concept(&self, req: CreateConceptRequest) -> Result<Uuid>;

    /// Get a concept by ID.
    async fn get_concept(&self, id: Uuid) -> Result<Option<SkosConcept>>;

    /// Get a concept with its preferred label.
    async fn get_concept_with_label(&self, id: Uuid) -> Result<Option<SkosConceptWithLabel>>;

    /// Get a full concept with all relations.
    async fn get_concept_full(&self, id: Uuid) -> Result<Option<SkosConceptFull>>;

    /// Get a concept by notation within a scheme.
    async fn get_concept_by_notation(
        &self,
        scheme_id: Uuid,
        notation: &str,
    ) -> Result<Option<SkosConcept>>;

    /// Search concepts with filtering.
    async fn search_concepts(&self, req: SearchConceptsRequest) -> Result<SearchConceptsResponse>;

    /// Update a concept.
    async fn update_concept(&self, id: Uuid, req: UpdateConceptRequest) -> Result<()>;

    /// Deprecate a concept with optional replacement.
    async fn deprecate_concept(
        &self,
        id: Uuid,
        reason: &str,
        replaced_by_id: Option<Uuid>,
    ) -> Result<()>;

    /// Delete a concept (must have no tags).
    async fn delete_concept(&self, id: Uuid) -> Result<()>;

    /// Get concept hierarchy (ancestors and descendants).
    async fn get_hierarchy(&self, scheme_id: Uuid) -> Result<Vec<SkosConceptHierarchy>>;

    /// Get concepts with specific antipatterns.
    async fn get_concepts_with_antipattern(
        &self,
        antipattern: TagAntipattern,
        limit: i64,
    ) -> Result<Vec<SkosConceptWithLabel>>;

    /// Refresh antipattern detection for a concept.
    async fn refresh_antipatterns(&self, id: Uuid) -> Result<Vec<TagAntipattern>>;

    /// Update concept embedding.
    async fn update_embedding(&self, id: Uuid, embedding: &[f32], model: &str) -> Result<()>;
}

/// Repository trait for SKOS label operations.
#[async_trait]
pub trait SkosLabelRepository: Send + Sync {
    /// Add a label to a concept.
    async fn add_label(&self, req: AddLabelRequest) -> Result<Uuid>;

    /// Get all labels for a concept.
    async fn get_labels(&self, concept_id: Uuid) -> Result<Vec<SkosConceptLabel>>;

    /// Update a label.
    async fn update_label(&self, id: Uuid, value: &str) -> Result<()>;

    /// Delete a label.
    async fn delete_label(&self, id: Uuid) -> Result<()>;

    /// Search labels (returns matching concepts).
    async fn search_labels(&self, query: &str, limit: i64) -> Result<Vec<SkosConceptWithLabel>>;
}

/// Repository trait for SKOS documentation note operations.
#[async_trait]
pub trait SkosNoteRepository: Send + Sync {
    /// Add a note to a concept.
    async fn add_note(&self, req: AddNoteRequest) -> Result<Uuid>;

    /// Get all notes for a concept.
    async fn get_notes(&self, concept_id: Uuid) -> Result<Vec<SkosConceptNote>>;

    /// Update a note.
    async fn update_note(&self, id: Uuid, value: &str) -> Result<()>;

    /// Delete a note.
    async fn delete_note(&self, id: Uuid) -> Result<()>;
}

/// Repository trait for SKOS relation operations.
#[async_trait]
pub trait SkosRelationRepository: Send + Sync {
    /// Create a semantic relation.
    async fn create_semantic_relation(&self, req: CreateSemanticRelationRequest) -> Result<Uuid>;

    /// Get semantic relations for a concept.
    async fn get_semantic_relations(
        &self,
        concept_id: Uuid,
        relation_type: Option<SkosSemanticRelation>,
    ) -> Result<Vec<SkosSemanticRelationEdge>>;

    /// Delete a semantic relation.
    async fn delete_semantic_relation(&self, id: Uuid) -> Result<()>;

    /// Delete a semantic relation by subject, object, and relation type.
    async fn delete_semantic_relation_by_triple(
        &self,
        subject_id: Uuid,
        object_id: Uuid,
        relation_type: SkosSemanticRelation,
    ) -> Result<()>;

    /// Create a mapping relation.
    async fn create_mapping_relation(&self, req: CreateMappingRelationRequest) -> Result<Uuid>;

    /// Get mapping relations for a concept.
    async fn get_mapping_relations(&self, concept_id: Uuid)
        -> Result<Vec<SkosMappingRelationEdge>>;

    /// Delete a mapping relation.
    async fn delete_mapping_relation(&self, id: Uuid) -> Result<()>;

    /// Validate a mapping relation.
    async fn validate_mapping_relation(&self, id: Uuid, validated_by: &str) -> Result<()>;
}

/// Repository trait for note-concept tagging operations.
#[async_trait]
pub trait SkosTaggingRepository: Send + Sync {
    /// Tag a note with a concept.
    async fn tag_note(&self, req: TagNoteRequest) -> Result<()>;

    /// Tag a note with multiple concepts.
    async fn batch_tag_note(&self, req: BatchTagNoteRequest) -> Result<()>;

    /// Remove a tag from a note.
    async fn untag_note(&self, note_id: Uuid, concept_id: Uuid) -> Result<()>;

    /// Get all tags for a note.
    async fn get_note_tags(&self, note_id: Uuid) -> Result<Vec<NoteSkosConceptTag>>;

    /// Get all tags for a note with labels.
    async fn get_note_tags_with_labels(
        &self,
        note_id: Uuid,
    ) -> Result<Vec<(NoteSkosConceptTag, SkosConceptWithLabel)>>;

    /// Get all notes tagged with a concept.
    async fn get_tagged_notes(
        &self,
        concept_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Uuid>>;

    /// Set the primary tag for a note.
    async fn set_primary_tag(&self, note_id: Uuid, concept_id: Uuid) -> Result<()>;

    /// Replace all tags for a note.
    async fn replace_note_tags(
        &self,
        note_id: Uuid,
        concept_ids: Vec<Uuid>,
        source: &str,
    ) -> Result<()>;
}

/// Repository trait for SKOS governance operations.
#[async_trait]
pub trait SkosGovernanceRepository: Send + Sync {
    /// Merge concepts into a target.
    async fn merge_concepts(&self, req: MergeConceptsRequest) -> Result<Uuid>;

    /// Get merge history for a concept.
    async fn get_merge_history(&self, concept_id: Uuid) -> Result<Vec<SkosConceptMerge>>;

    /// Get audit log for an entity.
    async fn get_audit_log(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        limit: i64,
    ) -> Result<Vec<SkosAuditLogEntry>>;

    /// Get governance statistics for a scheme.
    async fn get_governance_stats(&self, scheme_id: Uuid) -> Result<SkosGovernanceStats>;

    /// Get all governance statistics.
    async fn get_all_governance_stats(&self) -> Result<Vec<SkosGovernanceStats>>;

    /// Log an audit entry.
    async fn log_audit(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        action: &str,
        changes: Option<serde_json::Value>,
        actor: &str,
        actor_type: &str,
    ) -> Result<Uuid>;
}

// =============================================================================
// POSTGRESQL IMPLEMENTATION
// =============================================================================

/// Combined PostgreSQL SKOS repository.
#[derive(Clone)]
pub struct PgSkosRepository {
    pool: Pool<Postgres>,
}

impl PgSkosRepository {
    /// Create a new PgSkosRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Get the default scheme ID from the database.
    pub async fn get_default_scheme_id(&self) -> Result<Uuid> {
        let row = sqlx::query(
            "SELECT id FROM skos_concept_scheme WHERE is_system = TRUE AND notation = 'default'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default concept scheme not found".to_string()))
    }
}

// =============================================================================
// CONCEPT SCHEME IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosConceptSchemeRepository for PgSkosRepository {
    async fn create_scheme(&self, req: CreateConceptSchemeRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_concept_scheme (
                id, notation, uri, title, description, creator, publisher,
                rights, version, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            "#,
        )
        .bind(id)
        .bind(&req.notation)
        .bind(&req.uri)
        .bind(&req.title)
        .bind(&req.description)
        .bind(&req.creator)
        .bind(&req.publisher)
        .bind(&req.rights)
        .bind(req.version.as_deref().unwrap_or("1.0.0"))
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_scheme(&self, id: Uuid) -> Result<Option<SkosConceptScheme>> {
        let row = sqlx::query(
            r#"
            SELECT id, uri, notation, title, description, creator, publisher,
                   rights, version, is_active, is_system, created_at, updated_at,
                   issued_at, modified_at
            FROM skos_concept_scheme
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| SkosConceptScheme {
            id: r.get("id"),
            uri: r.get("uri"),
            notation: r.get("notation"),
            title: r.get("title"),
            description: r.get("description"),
            creator: r.get("creator"),
            publisher: r.get("publisher"),
            rights: r.get("rights"),
            version: r.get("version"),
            is_active: r.get("is_active"),
            is_system: r.get("is_system"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            issued_at: r.get("issued_at"),
            modified_at: r.get("modified_at"),
        }))
    }

    async fn get_scheme_by_notation(&self, notation: &str) -> Result<Option<SkosConceptScheme>> {
        let row = sqlx::query(
            r#"
            SELECT id, uri, notation, title, description, creator, publisher,
                   rights, version, is_active, is_system, created_at, updated_at,
                   issued_at, modified_at
            FROM skos_concept_scheme
            WHERE notation = $1
            "#,
        )
        .bind(notation)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| SkosConceptScheme {
            id: r.get("id"),
            uri: r.get("uri"),
            notation: r.get("notation"),
            title: r.get("title"),
            description: r.get("description"),
            creator: r.get("creator"),
            publisher: r.get("publisher"),
            rights: r.get("rights"),
            version: r.get("version"),
            is_active: r.get("is_active"),
            is_system: r.get("is_system"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            issued_at: r.get("issued_at"),
            modified_at: r.get("modified_at"),
        }))
    }

    async fn list_schemes(&self, include_inactive: bool) -> Result<Vec<SkosConceptSchemeSummary>> {
        let query = if include_inactive {
            r#"
            SELECT s.id, s.notation, s.title, s.description, s.is_active, s.is_system,
                   s.updated_at,
                   COALESCE((SELECT COUNT(*) FROM skos_concept WHERE primary_scheme_id = s.id), 0) as concept_count
            FROM skos_concept_scheme s
            ORDER BY s.notation
            "#
        } else {
            r#"
            SELECT s.id, s.notation, s.title, s.description, s.is_active, s.is_system,
                   s.updated_at,
                   COALESCE((SELECT COUNT(*) FROM skos_concept WHERE primary_scheme_id = s.id), 0) as concept_count
            FROM skos_concept_scheme s
            WHERE s.is_active = TRUE
            ORDER BY s.notation
            "#
        };

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptSchemeSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                title: r.get("title"),
                description: r.get("description"),
                is_active: r.get("is_active"),
                is_system: r.get("is_system"),
                concept_count: r.get("concept_count"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    async fn update_scheme(&self, id: Uuid, req: UpdateConceptSchemeRequest) -> Result<()> {
        let now = Utc::now();

        // Build dynamic update query
        let mut updates = vec!["updated_at = $1".to_string()];
        let mut param_idx = 2;

        if req.title.is_some() {
            updates.push(format!("title = ${}", param_idx));
            param_idx += 1;
        }
        if req.description.is_some() {
            updates.push(format!("description = ${}", param_idx));
            param_idx += 1;
        }
        if req.creator.is_some() {
            updates.push(format!("creator = ${}", param_idx));
            param_idx += 1;
        }
        if req.publisher.is_some() {
            updates.push(format!("publisher = ${}", param_idx));
            param_idx += 1;
        }
        if req.rights.is_some() {
            updates.push(format!("rights = ${}", param_idx));
            param_idx += 1;
        }
        if req.version.is_some() {
            updates.push(format!("version = ${}", param_idx));
            param_idx += 1;
        }
        if req.is_active.is_some() {
            updates.push(format!("is_active = ${}", param_idx));
            param_idx += 1;
        }

        let query = format!(
            "UPDATE skos_concept_scheme SET {} WHERE id = ${}",
            updates.join(", "),
            param_idx
        );

        let mut q = sqlx::query(&query).bind(now);

        if let Some(ref v) = req.title {
            q = q.bind(v);
        }
        if let Some(ref v) = req.description {
            q = q.bind(v);
        }
        if let Some(ref v) = req.creator {
            q = q.bind(v);
        }
        if let Some(ref v) = req.publisher {
            q = q.bind(v);
        }
        if let Some(ref v) = req.rights {
            q = q.bind(v);
        }
        if let Some(ref v) = req.version {
            q = q.bind(v);
        }
        if let Some(v) = req.is_active {
            q = q.bind(v);
        }

        q.bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete_scheme(&self, id: Uuid, force: bool) -> Result<()> {
        // Check if system scheme
        let is_system: bool =
            sqlx::query_scalar("SELECT is_system FROM skos_concept_scheme WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?
                .unwrap_or(false);

        if is_system {
            return Err(Error::InvalidInput(
                "Cannot delete system scheme".to_string(),
            ));
        }

        // Check if scheme has concepts
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM skos_concept WHERE primary_scheme_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        if count > 0 && !force {
            return Err(Error::InvalidInput(format!(
                "Cannot delete scheme with {} concepts. Use force=true to cascade delete.",
                count
            )));
        }

        if count > 0 && force {
            // Cascade delete: remove all concepts in this scheme.
            // Relations are cleaned up via ON DELETE CASCADE on the foreign keys.
            // Delete concept_in_scheme memberships first, then concepts.
            sqlx::query("DELETE FROM skos_concept_in_scheme WHERE scheme_id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;

            sqlx::query("DELETE FROM skos_concept WHERE primary_scheme_id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        }

        sqlx::query("DELETE FROM skos_concept_scheme WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_top_concepts(&self, scheme_id: Uuid) -> Result<Vec<SkosConceptSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.notation, c.status::text AS status, c.note_count, c.depth,
                   l.value AS pref_label, s.notation AS scheme_notation
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE c.primary_scheme_id = $1 AND c.broader_count = 0
            ORDER BY l.value, c.notation
            "#,
        )
        .bind(scheme_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                pref_label: r.get("pref_label"),
                status: r
                    .get::<String, _>("status")
                    .parse()
                    .unwrap_or(TagStatus::Candidate),
                note_count: r.get("note_count"),
                depth: r.get("depth"),
                scheme_notation: r.get("scheme_notation"),
            })
            .collect())
    }
}

// =============================================================================
// CONCEPT IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosConceptRepository for PgSkosRepository {
    async fn create_concept(&self, req: CreateConceptRequest) -> Result<Uuid> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let id = new_v7();
        let now = Utc::now();

        // Generate notation if not provided
        let notation = req.notation.unwrap_or_else(|| {
            // Use slug of pref_label
            req.pref_label
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect()
        });

        // Insert concept
        sqlx::query(
            r#"
            INSERT INTO skos_concept (
                id, primary_scheme_id, notation, facet_type, facet_source,
                facet_domain, facet_scope, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4::pmest_facet, $5, $6, $7, $8::tag_status, $9, $9)
            "#,
        )
        .bind(id)
        .bind(req.scheme_id)
        .bind(&notation)
        .bind(req.facet_type.map(|f| f.to_string()))
        .bind(&req.facet_source)
        .bind(&req.facet_domain)
        .bind(&req.facet_scope)
        .bind(req.status.to_string())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Insert preferred label
        sqlx::query(
            r#"
            INSERT INTO skos_concept_label (concept_id, label_type, value, language, created_at)
            VALUES ($1, 'pref_label', $2, $3, $4)
            "#,
        )
        .bind(id)
        .bind(&req.pref_label)
        .bind(&req.language)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Insert alternative labels
        for alt in &req.alt_labels {
            sqlx::query(
                r#"
                INSERT INTO skos_concept_label (concept_id, label_type, value, language, created_at)
                VALUES ($1, 'alt_label', $2, $3, $4)
                "#,
            )
            .bind(id)
            .bind(alt)
            .bind(&req.language)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Insert definition note if provided
        if let Some(ref definition) = req.definition {
            sqlx::query(
                r#"
                INSERT INTO skos_concept_note (concept_id, note_type, value, language, created_at, updated_at)
                VALUES ($1, 'definition', $2, $3, $4, $4)
                "#,
            )
            .bind(id)
            .bind(definition)
            .bind(&req.language)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Insert scope note if provided
        if let Some(ref scope_note) = req.scope_note {
            sqlx::query(
                r#"
                INSERT INTO skos_concept_note (concept_id, note_type, value, language, created_at, updated_at)
                VALUES ($1, 'scope_note', $2, $3, $4, $4)
                "#,
            )
            .bind(id)
            .bind(scope_note)
            .bind(&req.language)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Add to concept-in-scheme
        sqlx::query(
            r#"
            INSERT INTO skos_concept_in_scheme (concept_id, scheme_id, is_top_concept, added_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(id)
        .bind(req.scheme_id)
        .bind(req.broader_ids.is_empty()) // Top concept if no broader
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Create broader relations
        for broader_id in &req.broader_ids {
            sqlx::query(
                r#"
                INSERT INTO skos_semantic_relation_edge (subject_id, object_id, relation_type, created_at)
                VALUES ($1, $2, 'broader', $3)
                "#,
            )
            .bind(id)
            .bind(broader_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Create related relations
        for related_id in &req.related_ids {
            sqlx::query(
                r#"
                INSERT INTO skos_semantic_relation_edge (subject_id, object_id, relation_type, created_at)
                VALUES ($1, $2, 'related', $3)
                "#,
            )
            .bind(id)
            .bind(related_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_concept(&self, id: Uuid) -> Result<Option<SkosConcept>> {
        let row = sqlx::query(
            r#"
            SELECT id, primary_scheme_id, uri, notation, facet_type::text AS facet_type,
                   facet_source, facet_domain, facet_scope, status::text AS status,
                   promoted_at, deprecated_at, deprecation_reason, replaced_by_id,
                   note_count, first_used_at, last_used_at, depth, broader_count,
                   narrower_count, related_count, antipatterns::TEXT[] AS antipatterns, antipattern_checked_at,
                   created_at, updated_at, embedding_model, embedded_at
            FROM skos_concept
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_concept(&r)))
    }

    async fn get_concept_with_label(&self, id: Uuid) -> Result<Option<SkosConceptWithLabel>> {
        let query = format!(
            r#"
            SELECT {},
                   l.value AS pref_label, l.language AS label_language,
                   s.notation AS scheme_notation, s.title AS scheme_title
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE c.id = $1
            "#,
            CONCEPT_COLUMNS
        );
        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(row.map(|r| SkosConceptWithLabel {
            concept: self.row_to_concept(&r),
            pref_label: r.get("pref_label"),
            label_language: r.get("label_language"),
            scheme_notation: r.get("scheme_notation"),
            scheme_title: r.get("scheme_title"),
        }))
    }

    async fn get_concept_full(&self, id: Uuid) -> Result<Option<SkosConceptFull>> {
        let concept = match self.get_concept(id).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        // Get labels
        let labels = self.get_labels(id).await?;

        // Get notes
        let notes = self.get_notes(id).await?;

        // Get broader concepts
        let broader_rows = sqlx::query(
            r#"
            SELECT c.id, c.notation, c.status::text AS status, c.note_count, c.depth,
                   l.value AS pref_label, s.notation AS scheme_notation
            FROM skos_semantic_relation_edge e
            JOIN skos_concept c ON e.object_id = c.id
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE e.subject_id = $1 AND e.relation_type = 'broader'
            ORDER BY l.value
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let broader: Vec<SkosConceptSummary> = broader_rows
            .into_iter()
            .map(|r| SkosConceptSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                pref_label: r.get("pref_label"),
                status: r
                    .get::<String, _>("status")
                    .parse()
                    .unwrap_or(TagStatus::Candidate),
                note_count: r.get("note_count"),
                depth: r.get("depth"),
                scheme_notation: r.get("scheme_notation"),
            })
            .collect();

        // Get narrower concepts
        let narrower_rows = sqlx::query(
            r#"
            SELECT c.id, c.notation, c.status::text AS status, c.note_count, c.depth,
                   l.value AS pref_label, s.notation AS scheme_notation
            FROM skos_semantic_relation_edge e
            JOIN skos_concept c ON e.object_id = c.id
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE e.subject_id = $1 AND e.relation_type = 'narrower'
            ORDER BY l.value
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let narrower: Vec<SkosConceptSummary> = narrower_rows
            .into_iter()
            .map(|r| SkosConceptSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                pref_label: r.get("pref_label"),
                status: r
                    .get::<String, _>("status")
                    .parse()
                    .unwrap_or(TagStatus::Candidate),
                note_count: r.get("note_count"),
                depth: r.get("depth"),
                scheme_notation: r.get("scheme_notation"),
            })
            .collect();

        // Get related concepts
        let related_rows = sqlx::query(
            r#"
            SELECT c.id, c.notation, c.status::text AS status, c.note_count, c.depth,
                   l.value AS pref_label, s.notation AS scheme_notation
            FROM skos_semantic_relation_edge e
            JOIN skos_concept c ON e.object_id = c.id
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE e.subject_id = $1 AND e.relation_type = 'related'
            ORDER BY l.value
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let related: Vec<SkosConceptSummary> = related_rows
            .into_iter()
            .map(|r| SkosConceptSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                pref_label: r.get("pref_label"),
                status: r
                    .get::<String, _>("status")
                    .parse()
                    .unwrap_or(TagStatus::Candidate),
                note_count: r.get("note_count"),
                depth: r.get("depth"),
                scheme_notation: r.get("scheme_notation"),
            })
            .collect();

        // Get mappings
        let mappings = self.get_mapping_relations(id).await?;

        // Get schemes
        let scheme_rows = sqlx::query(
            r#"
            SELECT s.id, s.notation, s.title, s.description, s.is_active, s.is_system,
                   s.updated_at,
                   COALESCE((SELECT COUNT(*) FROM skos_concept WHERE primary_scheme_id = s.id), 0) as concept_count
            FROM skos_concept_in_scheme cis
            JOIN skos_concept_scheme s ON cis.scheme_id = s.id
            WHERE cis.concept_id = $1
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let schemes: Vec<SkosConceptSchemeSummary> = scheme_rows
            .into_iter()
            .map(|r| SkosConceptSchemeSummary {
                id: r.get("id"),
                notation: r.get("notation"),
                title: r.get("title"),
                description: r.get("description"),
                is_active: r.get("is_active"),
                is_system: r.get("is_system"),
                concept_count: r.get("concept_count"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        Ok(Some(SkosConceptFull {
            concept,
            labels,
            notes,
            broader,
            narrower,
            related,
            mappings,
            schemes,
        }))
    }

    async fn get_concept_by_notation(
        &self,
        scheme_id: Uuid,
        notation: &str,
    ) -> Result<Option<SkosConcept>> {
        let row = sqlx::query(
            r#"
            SELECT id, primary_scheme_id, uri, notation, facet_type::text AS facet_type, facet_source,
                   facet_domain, facet_scope, status::text AS status, promoted_at, deprecated_at,
                   deprecation_reason, replaced_by_id, note_count, first_used_at,
                   last_used_at, depth, broader_count, narrower_count, related_count,
                   antipatterns::TEXT[] AS antipatterns, antipattern_checked_at, created_at, updated_at,
                   embedding_model, embedded_at
            FROM skos_concept
            WHERE primary_scheme_id = $1 AND notation = $2
            "#,
        )
        .bind(scheme_id)
        .bind(notation)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_concept(&r)))
    }

    async fn search_concepts(&self, req: SearchConceptsRequest) -> Result<SearchConceptsResponse> {
        let mut conditions = vec!["1=1".to_string()];
        let mut param_idx = 1;

        // Build WHERE conditions
        if req.scheme_id.is_some() {
            conditions.push(format!("c.primary_scheme_id = ${}", param_idx));
            param_idx += 1;
        }
        if req.status.is_some() {
            conditions.push(format!("c.status = ${}::tag_status", param_idx));
            param_idx += 1;
        }
        if req.facet_type.is_some() {
            conditions.push(format!("c.facet_type = ${}::pmest_facet", param_idx));
            param_idx += 1;
        }
        if req.max_depth.is_some() {
            conditions.push(format!("c.depth <= ${}", param_idx));
            param_idx += 1;
        }
        if req.top_concepts_only {
            conditions.push("c.broader_count = 0".to_string());
        }
        if req.has_antipattern.is_some() {
            conditions.push(format!(
                "${}::tag_antipattern = ANY(c.antipatterns)",
                param_idx
            ));
            param_idx += 1;
        }
        if !req.include_deprecated {
            conditions.push("c.status != 'deprecated' AND c.status != 'obsolete'".to_string());
        }
        if req.query.is_some() {
            conditions.push(format!(
                "l.tsv @@ websearch_to_tsquery('matric_english', ${})",
                param_idx
            ));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_query = format!(
            r#"
            SELECT COUNT(DISTINCT c.id)
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
            WHERE {}
            "#,
            where_clause
        );

        // Main query
        let main_query = format!(
            r#"
            SELECT DISTINCT {},
                   l2.value AS pref_label, l2.language AS label_language,
                   s.notation AS scheme_notation, s.title AS scheme_title
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
            LEFT JOIN skos_concept_label l2 ON c.id = l2.concept_id
                AND l2.label_type = 'pref_label' AND l2.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE {}
            ORDER BY l2.value, c.notation
            LIMIT ${} OFFSET ${}
            "#,
            CONCEPT_COLUMNS,
            where_clause,
            param_idx,
            param_idx + 1
        );

        // Execute count query
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
        if let Some(ref v) = req.scheme_id {
            count_q = count_q.bind(v);
        }
        if let Some(ref v) = req.status {
            count_q = count_q.bind(v.to_string());
        }
        if let Some(ref v) = req.facet_type {
            count_q = count_q.bind(v.to_string());
        }
        if let Some(v) = req.max_depth {
            count_q = count_q.bind(v);
        }
        if let Some(ref v) = req.has_antipattern {
            count_q = count_q.bind(v.to_string());
        }
        if let Some(ref v) = req.query {
            count_q = count_q.bind(v);
        }

        let total = count_q
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Execute main query
        let mut main_q = sqlx::query(&main_query);
        if let Some(ref v) = req.scheme_id {
            main_q = main_q.bind(v);
        }
        if let Some(ref v) = req.status {
            main_q = main_q.bind(v.to_string());
        }
        if let Some(ref v) = req.facet_type {
            main_q = main_q.bind(v.to_string());
        }
        if let Some(v) = req.max_depth {
            main_q = main_q.bind(v);
        }
        if let Some(ref v) = req.has_antipattern {
            main_q = main_q.bind(v.to_string());
        }
        if let Some(ref v) = req.query {
            main_q = main_q.bind(v);
        }
        main_q = main_q.bind(req.limit).bind(req.offset);

        let rows = main_q
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let concepts: Vec<SkosConceptWithLabel> = rows
            .into_iter()
            .map(|r| SkosConceptWithLabel {
                concept: self.row_to_concept(&r),
                pref_label: r.get("pref_label"),
                label_language: r.get("label_language"),
                scheme_notation: r.get("scheme_notation"),
                scheme_title: r.get("scheme_title"),
            })
            .collect();

        Ok(SearchConceptsResponse {
            concepts,
            total,
            limit: req.limit,
            offset: req.offset,
        })
    }

    async fn update_concept(&self, id: Uuid, req: UpdateConceptRequest) -> Result<()> {
        let now = Utc::now();

        let mut updates = vec!["updated_at = $1".to_string()];
        let mut param_idx = 2;

        if req.notation.is_some() {
            updates.push(format!("notation = ${}", param_idx));
            param_idx += 1;
        }
        if req.status.is_some() {
            updates.push(format!("status = ${}::tag_status", param_idx));
            param_idx += 1;
        }
        if req.deprecation_reason.is_some() {
            updates.push(format!("deprecation_reason = ${}", param_idx));
            param_idx += 1;
        }
        if req.replaced_by_id.is_some() {
            updates.push(format!("replaced_by_id = ${}", param_idx));
            param_idx += 1;
        }
        if req.facet_type.is_some() {
            updates.push(format!("facet_type = ${}::pmest_facet", param_idx));
            param_idx += 1;
        }
        if req.facet_source.is_some() {
            updates.push(format!("facet_source = ${}", param_idx));
            param_idx += 1;
        }
        if req.facet_domain.is_some() {
            updates.push(format!("facet_domain = ${}", param_idx));
            param_idx += 1;
        }
        if req.facet_scope.is_some() {
            updates.push(format!("facet_scope = ${}", param_idx));
            param_idx += 1;
        }

        let query = format!(
            "UPDATE skos_concept SET {} WHERE id = ${}",
            updates.join(", "),
            param_idx
        );

        let mut q = sqlx::query(&query).bind(now);

        if let Some(ref v) = req.notation {
            q = q.bind(v);
        }
        if let Some(ref v) = req.status {
            q = q.bind(v.to_string());
        }
        if let Some(ref v) = req.deprecation_reason {
            q = q.bind(v);
        }
        if let Some(v) = req.replaced_by_id {
            q = q.bind(v);
        }
        if let Some(ref v) = req.facet_type {
            q = q.bind(v.to_string());
        }
        if let Some(ref v) = req.facet_source {
            q = q.bind(v);
        }
        if let Some(ref v) = req.facet_domain {
            q = q.bind(v);
        }
        if let Some(ref v) = req.facet_scope {
            q = q.bind(v);
        }

        q.bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn deprecate_concept(
        &self,
        id: Uuid,
        reason: &str,
        replaced_by_id: Option<Uuid>,
    ) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE skos_concept
            SET status = 'deprecated', deprecated_at = $1, deprecation_reason = $2,
                replaced_by_id = $3, updated_at = $1
            WHERE id = $4
            "#,
        )
        .bind(now)
        .bind(reason)
        .bind(replaced_by_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete_concept(&self, id: Uuid) -> Result<()> {
        // Check if concept has tags
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM note_skos_concept WHERE concept_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        if count > 0 {
            return Err(Error::InvalidInput(format!(
                "Cannot delete concept with {} note tags",
                count
            )));
        }

        sqlx::query("DELETE FROM skos_concept WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_hierarchy(&self, scheme_id: Uuid) -> Result<Vec<SkosConceptHierarchy>> {
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE hierarchy AS (
                SELECT
                    c.id,
                    c.notation,
                    l.value AS label,
                    0 AS level,
                    ARRAY[c.id] AS path,
                    ARRAY[COALESCE(l.value, c.notation, '')] AS label_path
                FROM skos_concept c
                LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                    AND l.label_type = 'pref_label' AND l.language = 'en'
                WHERE c.primary_scheme_id = $1 AND c.broader_count = 0

                UNION ALL

                SELECT
                    c.id,
                    c.notation,
                    l.value AS label,
                    h.level + 1,
                    h.path || c.id,
                    h.label_path || COALESCE(l.value, c.notation, '')
                FROM skos_concept c
                JOIN skos_semantic_relation_edge e ON c.id = e.subject_id AND e.relation_type = 'broader'
                JOIN hierarchy h ON e.object_id = h.id
                LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                    AND l.label_type = 'pref_label' AND l.language = 'en'
                WHERE NOT c.id = ANY(h.path)
                  AND h.level < 6
            )
            SELECT DISTINCT ON (id) * FROM hierarchy ORDER BY id, level
            "#,
        )
        .bind(scheme_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptHierarchy {
                id: r.get("id"),
                notation: r.get("notation"),
                label: r.get("label"),
                level: r.get("level"),
                path: r.get("path"),
                label_path: r.get("label_path"),
            })
            .collect())
    }

    async fn get_concepts_with_antipattern(
        &self,
        antipattern: TagAntipattern,
        limit: i64,
    ) -> Result<Vec<SkosConceptWithLabel>> {
        let query = format!(
            r#"
            SELECT {},
                   l.value AS pref_label, l.language AS label_language,
                   s.notation AS scheme_notation, s.title AS scheme_title
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE $1::tag_antipattern = ANY(c.antipatterns)
            ORDER BY c.updated_at DESC
            LIMIT $2
            "#,
            CONCEPT_COLUMNS
        );
        let rows = sqlx::query(&query)
            .bind(antipattern.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptWithLabel {
                concept: self.row_to_concept(&r),
                pref_label: r.get("pref_label"),
                label_language: r.get("label_language"),
                scheme_notation: r.get("scheme_notation"),
                scheme_title: r.get("scheme_title"),
            })
            .collect())
    }

    async fn refresh_antipatterns(&self, id: Uuid) -> Result<Vec<TagAntipattern>> {
        // Cast tag_antipattern[] to TEXT[] for sqlx compatibility
        let row = sqlx::query("SELECT skos_detect_antipatterns($1)::TEXT[] AS antipatterns")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        let patterns: Vec<String> = row.get("antipatterns");
        let antipatterns: Vec<TagAntipattern> = patterns
            .clone()
            .into_iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        // Update the concept - cast TEXT[] to tag_antipattern[] for proper enum type
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE skos_concept
            SET antipatterns = $1::TEXT[]::tag_antipattern[],
                antipattern_checked_at = $2,
                updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&patterns)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(antipatterns)
    }

    async fn update_embedding(&self, id: Uuid, embedding: &[f32], model: &str) -> Result<()> {
        let now = Utc::now();
        let vec = pgvector::Vector::from(embedding.to_vec());

        sqlx::query(
            r#"
            UPDATE skos_concept
            SET embedding = $1, embedding_model = $2, embedded_at = $3, updated_at = $3
            WHERE id = $4
            "#,
        )
        .bind(&vec)
        .bind(model)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }
}

// =============================================================================
// LABEL IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosLabelRepository for PgSkosRepository {
    async fn add_label(&self, req: AddLabelRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_concept_label (id, concept_id, label_type, value, language, created_at)
            VALUES ($1, $2, $3::skos_label_type, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(req.concept_id)
        .bind(req.label_type.to_string())
        .bind(&req.value)
        .bind(&req.language)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_labels(&self, concept_id: Uuid) -> Result<Vec<SkosConceptLabel>> {
        let rows = sqlx::query(
            r#"
            SELECT id, concept_id, label_type::text AS label_type, value, language, created_at
            FROM skos_concept_label
            WHERE concept_id = $1
            ORDER BY label_type, language, value
            "#,
        )
        .bind(concept_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptLabel {
                id: r.get("id"),
                concept_id: r.get("concept_id"),
                label_type: r
                    .get::<String, _>("label_type")
                    .parse()
                    .unwrap_or(SkosLabelType::PrefLabel),
                value: r.get("value"),
                language: r.get("language"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn update_label(&self, id: Uuid, value: &str) -> Result<()> {
        sqlx::query("UPDATE skos_concept_label SET value = $1 WHERE id = $2")
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn delete_label(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM skos_concept_label WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn search_labels(&self, query: &str, limit: i64) -> Result<Vec<SkosConceptWithLabel>> {
        // Use ILIKE prefix matching for autocomplete (issue #132).
        // websearch_to_tsquery doesn't support prefix matching; short inputs
        // like "Mac" won't match "Machine Learning" via FTS stemming.
        let like_pattern = format!("{}%", query);
        let sql = format!(
            r#"
            SELECT DISTINCT {},
                   l2.value AS pref_label, l2.language AS label_language,
                   s.notation AS scheme_notation, s.title AS scheme_title
            FROM skos_concept_label l
            JOIN skos_concept c ON l.concept_id = c.id
            LEFT JOIN skos_concept_label l2 ON c.id = l2.concept_id
                AND l2.label_type = 'pref_label' AND l2.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE l.value ILIKE $1
            ORDER BY l2.value
            LIMIT $2
            "#,
            CONCEPT_COLUMNS
        );
        let rows = sqlx::query(&sql)
            .bind(&like_pattern)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptWithLabel {
                concept: self.row_to_concept(&r),
                pref_label: r.get("pref_label"),
                label_language: r.get("label_language"),
                scheme_notation: r.get("scheme_notation"),
                scheme_title: r.get("scheme_title"),
            })
            .collect())
    }
}

// =============================================================================
// NOTE IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosNoteRepository for PgSkosRepository {
    async fn add_note(&self, req: AddNoteRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_concept_note (id, concept_id, note_type, value, language, author, source, created_at, updated_at)
            VALUES ($1, $2, $3::skos_note_type, $4, $5, $6, $7, $8, $8)
            "#,
        )
        .bind(id)
        .bind(req.concept_id)
        .bind(req.note_type.to_string())
        .bind(&req.value)
        .bind(&req.language)
        .bind(&req.author)
        .bind(&req.source)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_notes(&self, concept_id: Uuid) -> Result<Vec<SkosConceptNote>> {
        let rows = sqlx::query(
            r#"
            SELECT id, concept_id, note_type::text AS note_type, value, language, author, source, created_at, updated_at
            FROM skos_concept_note
            WHERE concept_id = $1
            ORDER BY note_type, language, created_at
            "#,
        )
        .bind(concept_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptNote {
                id: r.get("id"),
                concept_id: r.get("concept_id"),
                note_type: r
                    .get::<String, _>("note_type")
                    .parse()
                    .unwrap_or(SkosNoteType::Note),
                value: r.get("value"),
                language: r.get("language"),
                author: r.get("author"),
                source: r.get("source"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    async fn update_note(&self, id: Uuid, value: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE skos_concept_note SET value = $1, updated_at = $2 WHERE id = $3")
            .bind(value)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn delete_note(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM skos_concept_note WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}

// =============================================================================
// RELATION IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosRelationRepository for PgSkosRepository {
    async fn create_semantic_relation(&self, req: CreateSemanticRelationRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_semantic_relation_edge (
                id, subject_id, object_id, relation_type, inference_score,
                is_inferred, created_at, created_by
            )
            VALUES ($1, $2, $3, $4::skos_semantic_relation, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(req.subject_id)
        .bind(req.object_id)
        .bind(req.relation_type.to_string())
        .bind(req.inference_score)
        .bind(req.is_inferred)
        .bind(now)
        .bind(&req.created_by)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_semantic_relations(
        &self,
        concept_id: Uuid,
        relation_type: Option<SkosSemanticRelation>,
    ) -> Result<Vec<SkosSemanticRelationEdge>> {
        let rows = if let Some(rel_type) = relation_type {
            sqlx::query(
                r#"
                SELECT id, subject_id, object_id, relation_type::text AS relation_type, inference_score,
                       is_inferred, is_validated, created_at, created_by
                FROM skos_semantic_relation_edge
                WHERE subject_id = $1 AND relation_type = $2::skos_semantic_relation
                "#,
            )
            .bind(concept_id)
            .bind(rel_type.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::query(
                r#"
                SELECT id, subject_id, object_id, relation_type::text AS relation_type, inference_score,
                       is_inferred, is_validated, created_at, created_by
                FROM skos_semantic_relation_edge
                WHERE subject_id = $1
                "#,
            )
            .bind(concept_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        };

        Ok(rows
            .into_iter()
            .map(|r| SkosSemanticRelationEdge {
                id: r.get("id"),
                subject_id: r.get("subject_id"),
                object_id: r.get("object_id"),
                relation_type: r
                    .get::<String, _>("relation_type")
                    .parse()
                    .unwrap_or(SkosSemanticRelation::Related),
                inference_score: r.get("inference_score"),
                is_inferred: r.get("is_inferred"),
                is_validated: r.get("is_validated"),
                created_at: r.get("created_at"),
                created_by: r.get("created_by"),
            })
            .collect())
    }

    async fn delete_semantic_relation(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM skos_semantic_relation_edge WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn delete_semantic_relation_by_triple(
        &self,
        subject_id: Uuid,
        object_id: Uuid,
        relation_type: SkosSemanticRelation,
    ) -> Result<()> {
        // For symmetric relations (Related), delete both directions.
        // skos:related is symmetric per W3C SKOS spec - removing one direction
        // must also remove the inferred inverse.
        sqlx::query(
            "DELETE FROM skos_semantic_relation_edge
             WHERE ((subject_id = $1 AND object_id = $2) OR (subject_id = $2 AND object_id = $1))
             AND relation_type = $3::skos_semantic_relation",
        )
        .bind(subject_id)
        .bind(object_id)
        .bind(relation_type.to_string())
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn create_mapping_relation(&self, req: CreateMappingRelationRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_mapping_relation_edge (
                id, concept_id, target_uri, target_scheme_uri, target_label,
                relation_type, confidence, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6::skos_mapping_relation, $7, $8)
            "#,
        )
        .bind(id)
        .bind(req.concept_id)
        .bind(&req.target_uri)
        .bind(&req.target_scheme_uri)
        .bind(&req.target_label)
        .bind(req.relation_type.to_string())
        .bind(req.confidence)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get_mapping_relations(
        &self,
        concept_id: Uuid,
    ) -> Result<Vec<SkosMappingRelationEdge>> {
        let rows = sqlx::query(
            r#"
            SELECT id, concept_id, target_uri, target_scheme_uri, target_label,
                   relation_type::text AS relation_type, confidence, is_validated, created_at,
                   validated_at, validated_by
            FROM skos_mapping_relation_edge
            WHERE concept_id = $1
            "#,
        )
        .bind(concept_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosMappingRelationEdge {
                id: r.get("id"),
                concept_id: r.get("concept_id"),
                target_uri: r.get("target_uri"),
                target_scheme_uri: r.get("target_scheme_uri"),
                target_label: r.get("target_label"),
                relation_type: r
                    .get::<String, _>("relation_type")
                    .parse()
                    .unwrap_or(SkosMappingRelation::RelatedMatch),
                confidence: r.get("confidence"),
                is_validated: r.get("is_validated"),
                created_at: r.get("created_at"),
                validated_at: r.get("validated_at"),
                validated_by: r.get("validated_by"),
            })
            .collect())
    }

    async fn delete_mapping_relation(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM skos_mapping_relation_edge WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn validate_mapping_relation(&self, id: Uuid, validated_by: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE skos_mapping_relation_edge
            SET is_validated = TRUE, validated_at = $1, validated_by = $2
            WHERE id = $3
            "#,
        )
        .bind(now)
        .bind(validated_by)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }
}

// =============================================================================
// TAGGING IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosTaggingRepository for PgSkosRepository {
    async fn tag_note(&self, req: TagNoteRequest) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO note_skos_concept (
                note_id, concept_id, source, confidence, relevance_score,
                is_primary, created_at, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (note_id, concept_id) DO UPDATE SET
                source = EXCLUDED.source,
                confidence = EXCLUDED.confidence,
                relevance_score = EXCLUDED.relevance_score,
                is_primary = EXCLUDED.is_primary
            "#,
        )
        .bind(req.note_id)
        .bind(req.concept_id)
        .bind(&req.source)
        .bind(req.confidence)
        .bind(req.relevance_score)
        .bind(req.is_primary)
        .bind(now)
        .bind(&req.created_by)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn batch_tag_note(&self, req: BatchTagNoteRequest) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        for concept_id in &req.concept_ids {
            sqlx::query(
                r#"
                INSERT INTO note_skos_concept (
                    note_id, concept_id, source, confidence, created_at, created_by
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (note_id, concept_id) DO NOTHING
                "#,
            )
            .bind(req.note_id)
            .bind(concept_id)
            .bind(&req.source)
            .bind(req.confidence)
            .bind(now)
            .bind(&req.created_by)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn untag_note(&self, note_id: Uuid, concept_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_skos_concept WHERE note_id = $1 AND concept_id = $2")
            .bind(note_id)
            .bind(concept_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn get_note_tags(&self, note_id: Uuid) -> Result<Vec<NoteSkosConceptTag>> {
        let rows = sqlx::query(
            r#"
            SELECT note_id, concept_id, source, confidence, relevance_score,
                   is_primary, created_at, created_by
            FROM note_skos_concept
            WHERE note_id = $1
            ORDER BY is_primary DESC, relevance_score DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| NoteSkosConceptTag {
                note_id: r.get("note_id"),
                concept_id: r.get("concept_id"),
                source: r.get("source"),
                confidence: r.get("confidence"),
                relevance_score: r.get("relevance_score"),
                is_primary: r.get("is_primary"),
                created_at: r.get("created_at"),
                created_by: r.get("created_by"),
            })
            .collect())
    }

    async fn get_note_tags_with_labels(
        &self,
        note_id: Uuid,
    ) -> Result<Vec<(NoteSkosConceptTag, SkosConceptWithLabel)>> {
        let query = format!(
            r#"
            SELECT nc.note_id, nc.concept_id, nc.source, nc.confidence, nc.relevance_score,
                   nc.is_primary, nc.created_at, nc.created_by,
                   {}, l.value AS pref_label, l.language AS label_language,
                   s.notation AS scheme_notation, s.title AS scheme_title
            FROM note_skos_concept nc
            JOIN skos_concept c ON nc.concept_id = c.id
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                AND l.label_type = 'pref_label' AND l.language = 'en'
            LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id
            WHERE nc.note_id = $1
            ORDER BY nc.is_primary DESC, nc.relevance_score DESC
            "#,
            CONCEPT_COLUMNS
        );
        let rows = sqlx::query(&query)
            .bind(note_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let tag = NoteSkosConceptTag {
                    note_id: r.get("note_id"),
                    concept_id: r.get("concept_id"),
                    source: r.get("source"),
                    confidence: r.get("confidence"),
                    relevance_score: r.get("relevance_score"),
                    is_primary: r.get("is_primary"),
                    created_at: r.get("created_at"),
                    created_by: r.get("created_by"),
                };
                let concept = SkosConceptWithLabel {
                    concept: self.row_to_concept(&r),
                    pref_label: r.get("pref_label"),
                    label_language: r.get("label_language"),
                    scheme_notation: r.get("scheme_notation"),
                    scheme_title: r.get("scheme_title"),
                };
                (tag, concept)
            })
            .collect())
    }

    async fn get_tagged_notes(
        &self,
        concept_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Uuid>> {
        let rows: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT note_id FROM note_skos_concept
            WHERE concept_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(concept_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows)
    }

    async fn set_primary_tag(&self, note_id: Uuid, concept_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Unset all primary tags for this note
        sqlx::query("UPDATE note_skos_concept SET is_primary = FALSE WHERE note_id = $1")
            .bind(note_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Set the new primary tag
        sqlx::query(
            "UPDATE note_skos_concept SET is_primary = TRUE WHERE note_id = $1 AND concept_id = $2",
        )
        .bind(note_id)
        .bind(concept_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn replace_note_tags(
        &self,
        note_id: Uuid,
        concept_ids: Vec<Uuid>,
        source: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Remove existing tags
        sqlx::query("DELETE FROM note_skos_concept WHERE note_id = $1")
            .bind(note_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Add new tags
        for concept_id in &concept_ids {
            sqlx::query(
                r#"
                INSERT INTO note_skos_concept (note_id, concept_id, source, created_at)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(note_id)
            .bind(concept_id)
            .bind(source)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}

// =============================================================================
// GOVERNANCE IMPLEMENTATION
// =============================================================================

#[async_trait]
impl SkosGovernanceRepository for PgSkosRepository {
    async fn merge_concepts(&self, req: MergeConceptsRequest) -> Result<Uuid> {
        let merge_id = new_v7();
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Move all note tags to target
        for source_id in &req.source_ids {
            // Use alias 'outer_row' to fix ambiguous reference in the NOT EXISTS subquery
            sqlx::query(
                r#"
                UPDATE note_skos_concept AS outer_row
                SET concept_id = $1
                WHERE outer_row.concept_id = $2 AND NOT EXISTS (
                    SELECT 1 FROM note_skos_concept
                    WHERE note_id = outer_row.note_id AND concept_id = $1
                )
                "#,
            )
            .bind(req.target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            // Delete duplicates
            sqlx::query("DELETE FROM note_skos_concept WHERE concept_id = $1")
                .bind(source_id)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;

            // Deprecate source concept
            sqlx::query(
                r#"
                UPDATE skos_concept
                SET status = 'obsolete', deprecated_at = $1, deprecation_reason = $2,
                    replaced_by_id = $3, updated_at = $1
                WHERE id = $4
                "#,
            )
            .bind(now)
            .bind(&req.reason)
            .bind(req.target_id)
            .bind(source_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Record the merge
        sqlx::query(
            r#"
            INSERT INTO skos_concept_merge (id, source_ids, target_id, reason, performed_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(merge_id)
        .bind(&req.source_ids)
        .bind(req.target_id)
        .bind(&req.reason)
        .bind(&req.performed_by)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;

        Ok(merge_id)
    }

    async fn get_merge_history(&self, concept_id: Uuid) -> Result<Vec<SkosConceptMerge>> {
        let rows = sqlx::query(
            r#"
            SELECT id, source_ids, target_id, reason, performed_by, created_at
            FROM skos_concept_merge
            WHERE target_id = $1 OR $1 = ANY(source_ids)
            ORDER BY created_at DESC
            "#,
        )
        .bind(concept_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosConceptMerge {
                id: r.get("id"),
                source_ids: r.get("source_ids"),
                target_id: r.get("target_id"),
                reason: r.get("reason"),
                performed_by: r.get("performed_by"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn get_audit_log(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        limit: i64,
    ) -> Result<Vec<SkosAuditLogEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT id, entity_type, entity_id, action, changes, actor, actor_type, created_at
            FROM skos_audit_log
            WHERE entity_type = $1 AND entity_id = $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosAuditLogEntry {
                id: r.get("id"),
                entity_type: r.get("entity_type"),
                entity_id: r.get("entity_id"),
                action: r.get("action"),
                changes: r.get("changes"),
                actor: r.get("actor"),
                actor_type: r.get("actor_type"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    async fn get_governance_stats(&self, scheme_id: Uuid) -> Result<SkosGovernanceStats> {
        let row = sqlx::query(
            r#"
            SELECT
                s.id AS scheme_id,
                s.notation AS scheme_notation,
                s.title AS scheme_title,
                COUNT(DISTINCT c.id) AS total_concepts,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'candidate') AS candidates,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'approved') AS approved,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'deprecated') AS deprecated,
                COUNT(DISTINCT c.id) FILTER (WHERE 'orphan' = ANY(c.antipatterns)) AS orphans,
                COUNT(DISTINCT c.id) FILTER (WHERE 'under_used' = ANY(c.antipatterns)) AS under_used,
                COUNT(DISTINCT c.id) FILTER (WHERE c.embedding IS NULL) AS missing_embeddings,
                COALESCE(AVG(c.note_count), 0)::FLOAT8 AS avg_note_count,
                COALESCE(MAX(c.depth), 0) AS max_depth
            FROM skos_concept_scheme s
            LEFT JOIN skos_concept c ON c.primary_scheme_id = s.id
            WHERE s.id = $1
            GROUP BY s.id, s.notation, s.title
            "#,
        )
        .bind(scheme_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(SkosGovernanceStats {
            scheme_id: row.get("scheme_id"),
            scheme_notation: row.get("scheme_notation"),
            scheme_title: row.get("scheme_title"),
            total_concepts: row.get("total_concepts"),
            candidates: row.get("candidates"),
            approved: row.get("approved"),
            deprecated: row.get("deprecated"),
            orphans: row.get("orphans"),
            under_used: row.get("under_used"),
            missing_embeddings: row.get("missing_embeddings"),
            avg_note_count: row.get("avg_note_count"),
            max_depth: row.get("max_depth"),
        })
    }

    async fn get_all_governance_stats(&self) -> Result<Vec<SkosGovernanceStats>> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.id AS scheme_id,
                s.notation AS scheme_notation,
                s.title AS scheme_title,
                COUNT(DISTINCT c.id) AS total_concepts,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'candidate') AS candidates,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'approved') AS approved,
                COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'deprecated') AS deprecated,
                COUNT(DISTINCT c.id) FILTER (WHERE 'orphan' = ANY(c.antipatterns)) AS orphans,
                COUNT(DISTINCT c.id) FILTER (WHERE 'under_used' = ANY(c.antipatterns)) AS under_used,
                COUNT(DISTINCT c.id) FILTER (WHERE c.embedding IS NULL) AS missing_embeddings,
                COALESCE(AVG(c.note_count), 0)::FLOAT8 AS avg_note_count,
                COALESCE(MAX(c.depth), 0) AS max_depth
            FROM skos_concept_scheme s
            LEFT JOIN skos_concept c ON c.primary_scheme_id = s.id
            GROUP BY s.id, s.notation, s.title
            ORDER BY s.notation
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosGovernanceStats {
                scheme_id: r.get("scheme_id"),
                scheme_notation: r.get("scheme_notation"),
                scheme_title: r.get("scheme_title"),
                total_concepts: r.get("total_concepts"),
                candidates: r.get("candidates"),
                approved: r.get("approved"),
                deprecated: r.get("deprecated"),
                orphans: r.get("orphans"),
                under_used: r.get("under_used"),
                missing_embeddings: r.get("missing_embeddings"),
                avg_note_count: r.get("avg_note_count"),
                max_depth: r.get("max_depth"),
            })
            .collect())
    }

    async fn log_audit(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        action: &str,
        changes: Option<serde_json::Value>,
        actor: &str,
        actor_type: &str,
    ) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO skos_audit_log (id, entity_type, entity_id, action, changes, actor, actor_type, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(action)
        .bind(&changes)
        .bind(actor)
        .bind(actor_type)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }
}

// =============================================================================
// HELPER METHODS
// =============================================================================

impl PgSkosRepository {
    /// Convert a database row to a SkosConcept.
    fn row_to_concept(&self, row: &sqlx::postgres::PgRow) -> SkosConcept {
        let antipatterns_str: Vec<String> = row.try_get("antipatterns").unwrap_or_default();
        let antipatterns: Vec<TagAntipattern> = antipatterns_str
            .into_iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        SkosConcept {
            id: row.get("id"),
            primary_scheme_id: row.get("primary_scheme_id"),
            uri: row.get("uri"),
            notation: row.get("notation"),
            facet_type: row
                .get::<Option<String>, _>("facet_type")
                .and_then(|s| s.parse().ok()),
            facet_source: row.get("facet_source"),
            facet_domain: row.get("facet_domain"),
            facet_scope: row.get("facet_scope"),
            status: row
                .get::<String, _>("status")
                .parse()
                .unwrap_or(TagStatus::Candidate),
            promoted_at: row.get("promoted_at"),
            deprecated_at: row.get("deprecated_at"),
            deprecation_reason: row.get("deprecation_reason"),
            replaced_by_id: row.get("replaced_by_id"),
            note_count: row.get("note_count"),
            first_used_at: row.get("first_used_at"),
            last_used_at: row.get("last_used_at"),
            depth: row.get("depth"),
            broader_count: row.get("broader_count"),
            narrower_count: row.get("narrower_count"),
            related_count: row.get("related_count"),
            antipatterns,
            antipattern_checked_at: row.get("antipattern_checked_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            embedding_model: row.get("embedding_model"),
            embedded_at: row.get("embedded_at"),
        }
    }
}

// =============================================================================
// TAG RESOLUTION SERVICE
// =============================================================================

/// Repository trait for resolving tag inputs to SKOS concepts.
///
/// This handles the conversion from user-friendly tag paths (e.g., "programming/rust")
/// to proper SKOS concepts with hierarchy relationships.
#[async_trait]
pub trait SkosTagResolutionRepository: Send + Sync {
    /// Resolve a tag input to an existing or new SKOS concept.
    ///
    /// For hierarchical tags like "programming/rust":
    /// 1. Creates "programming" concept if it doesn't exist
    /// 2. Creates "rust" concept if it doesn't exist
    /// 3. Sets up broader/narrower relationship between them
    /// 4. Returns the leaf concept ID ("rust")
    async fn resolve_or_create_tag(&self, input: &TagInput) -> Result<ResolvedTag>;

    /// Resolve multiple tag inputs.
    async fn resolve_or_create_tags(&self, inputs: &[TagInput]) -> Result<Vec<ResolvedTag>>;

    /// Resolve a full SKOS tag specification.
    async fn resolve_or_create_from_spec(&self, spec: &SkosTagSpec) -> Result<ResolvedTag>;

    /// Find an existing concept by its path.
    async fn find_concept_by_path(&self, scheme_id: Uuid, path: &[String]) -> Result<Option<Uuid>>;
}

#[async_trait]
impl SkosTagResolutionRepository for PgSkosRepository {
    async fn resolve_or_create_tag(&self, input: &TagInput) -> Result<ResolvedTag> {
        // Get the scheme (default if not specified)
        let scheme_notation = &input.scheme;
        let scheme = self.get_scheme_by_notation(scheme_notation).await?;

        let scheme_id = match scheme {
            Some(s) => s.id,
            None => {
                // Use default scheme if specified scheme doesn't exist
                if scheme_notation == DEFAULT_SCHEME_NOTATION {
                    self.get_default_scheme_id().await?
                } else {
                    // Create the scheme if it doesn't exist
                    let scheme_id = self
                        .create_scheme(CreateConceptSchemeRequest {
                            notation: scheme_notation.to_string(),
                            title: scheme_notation.to_string(),
                            uri: None,
                            description: Some(format!(
                                "Auto-created scheme for {} tags",
                                scheme_notation
                            )),
                            creator: None,
                            publisher: None,
                            rights: None,
                            version: None,
                        })
                        .await?;
                    scheme_id
                }
            }
        };

        // Process each component in the path, building hierarchy
        let mut parent_id: Option<Uuid> = None;
        let mut created = false;

        for (i, component) in input.path.iter().enumerate() {
            // Build the path up to this component for notation
            let current_path = &input.path[..=i];
            let notation = current_path.join("/").to_lowercase().replace(' ', "-");

            // Check if concept with this notation exists in scheme
            let existing = self.get_concept_by_notation(scheme_id, &notation).await?;

            let concept_id = match existing {
                Some(concept) => concept.id,
                None => {
                    // Create the concept
                    let mut broader_ids = Vec::new();
                    if let Some(pid) = parent_id {
                        broader_ids.push(pid);
                    }

                    let concept_id = self
                        .create_concept(CreateConceptRequest {
                            scheme_id,
                            notation: Some(notation),
                            pref_label: component.to_lowercase(),
                            language: "en".to_string(),
                            status: TagStatus::Candidate,
                            facet_type: None,
                            facet_source: None,
                            facet_domain: None,
                            facet_scope: None,
                            definition: None,
                            scope_note: None,
                            broader_ids,
                            related_ids: vec![],
                            alt_labels: vec![],
                        })
                        .await?;

                    created = true;
                    concept_id
                }
            };

            parent_id = Some(concept_id);
        }

        // Return the leaf concept
        let leaf_id = parent_id.ok_or_else(|| Error::InvalidInput("Empty tag path".to_string()))?;

        Ok(ResolvedTag {
            input: input.clone(),
            concept_id: leaf_id,
            scheme_id,
            created,
        })
    }

    async fn resolve_or_create_tags(&self, inputs: &[TagInput]) -> Result<Vec<ResolvedTag>> {
        let mut results = Vec::with_capacity(inputs.len());
        for input in inputs {
            results.push(self.resolve_or_create_tag(input).await?);
        }
        Ok(results)
    }

    async fn resolve_or_create_from_spec(&self, spec: &SkosTagSpec) -> Result<ResolvedTag> {
        // First resolve the main tag
        let input = spec.to_tag_input();
        let resolved = self.resolve_or_create_tag(&input).await?;

        // If this was newly created, add additional properties
        if resolved.created {
            // Add alternative labels
            for alt_label in &spec.alt_labels {
                self.add_label(AddLabelRequest {
                    concept_id: resolved.concept_id,
                    label_type: SkosLabelType::AltLabel,
                    value: alt_label.clone(),
                    language: "en".to_string(),
                })
                .await?;
            }

            // Add hidden labels
            for hidden_label in &spec.hidden_labels {
                self.add_label(AddLabelRequest {
                    concept_id: resolved.concept_id,
                    label_type: SkosLabelType::HiddenLabel,
                    value: hidden_label.clone(),
                    language: "en".to_string(),
                })
                .await?;
            }

            // Add definition
            if let Some(ref definition) = spec.definition {
                self.add_note(AddNoteRequest {
                    concept_id: resolved.concept_id,
                    note_type: SkosNoteType::Definition,
                    value: definition.clone(),
                    language: "en".to_string(),
                    author: None,
                    source: None,
                })
                .await?;
            }

            // Add scope note
            if let Some(ref scope_note) = spec.scope_note {
                self.add_note(AddNoteRequest {
                    concept_id: resolved.concept_id,
                    note_type: SkosNoteType::ScopeNote,
                    value: scope_note.clone(),
                    language: "en".to_string(),
                    author: None,
                    source: None,
                })
                .await?;
            }

            // Add example
            if let Some(ref example) = spec.example {
                self.add_note(AddNoteRequest {
                    concept_id: resolved.concept_id,
                    note_type: SkosNoteType::Example,
                    value: example.clone(),
                    language: "en".to_string(),
                    author: None,
                    source: None,
                })
                .await?;
            }

            // Add broader relations
            for broader_path in &spec.broader {
                let broader_input = TagInput::parse(broader_path);
                let broader_resolved = self.resolve_or_create_tag(&broader_input).await?;

                // Create broader relation
                self.create_semantic_relation(CreateSemanticRelationRequest {
                    subject_id: resolved.concept_id,
                    object_id: broader_resolved.concept_id,
                    relation_type: SkosSemanticRelation::Broader,
                    inference_score: None,
                    is_inferred: false,
                    created_by: Some("tag_resolution".to_string()),
                })
                .await?;
            }

            // Add related relations
            for related_path in &spec.related {
                let related_input = TagInput::parse(related_path);
                let related_resolved = self.resolve_or_create_tag(&related_input).await?;

                // Create related relation
                self.create_semantic_relation(CreateSemanticRelationRequest {
                    subject_id: resolved.concept_id,
                    object_id: related_resolved.concept_id,
                    relation_type: SkosSemanticRelation::Related,
                    inference_score: None,
                    is_inferred: false,
                    created_by: Some("tag_resolution".to_string()),
                })
                .await?;
            }

            // Update facet info if provided
            if spec.facet_type.is_some() || spec.facet_domain.is_some() {
                self.update_concept(
                    resolved.concept_id,
                    UpdateConceptRequest {
                        notation: None,
                        status: spec.status,
                        deprecation_reason: None,
                        replaced_by_id: None,
                        facet_type: spec.facet_type,
                        facet_source: None,
                        facet_domain: spec.facet_domain.clone(),
                        facet_scope: None,
                    },
                )
                .await?;
            }
        }

        Ok(resolved)
    }

    async fn find_concept_by_path(&self, scheme_id: Uuid, path: &[String]) -> Result<Option<Uuid>> {
        if path.is_empty() {
            return Ok(None);
        }

        let notation = path.join("/").to_lowercase().replace(' ', "-");
        let concept = self.get_concept_by_notation(scheme_id, &notation).await?;
        Ok(concept.map(|c| c.id))
    }
}

// =============================================================================
// SKOS COLLECTION REPOSITORY
// =============================================================================

use matric_core::{
    CreateSkosCollectionRequest, SkosCollection, SkosCollectionMember, SkosCollectionWithMembers,
    UpdateCollectionMembersRequest, UpdateSkosCollectionRequest,
};

/// Repository trait for SKOS Collection operations.
#[async_trait]
pub trait SkosCollectionRepository: Send + Sync {
    /// Create a new SKOS collection, optionally with initial members.
    async fn create_collection(&self, req: CreateSkosCollectionRequest) -> Result<Uuid>;

    /// Get a collection by ID (without members).
    async fn get_collection(&self, id: Uuid) -> Result<Option<SkosCollection>>;

    /// Get a collection with its member concepts.
    async fn get_collection_with_members(
        &self,
        id: Uuid,
    ) -> Result<Option<SkosCollectionWithMembers>>;

    /// List all collections, optionally filtered by scheme.
    async fn list_collections(&self, scheme_id: Option<Uuid>) -> Result<Vec<SkosCollection>>;

    /// Update a collection's metadata.
    async fn update_collection(&self, id: Uuid, req: UpdateSkosCollectionRequest) -> Result<()>;

    /// Delete a collection (members are cascade-deleted).
    async fn delete_collection(&self, id: Uuid) -> Result<()>;

    /// Add a concept to a collection.
    async fn add_collection_member(
        &self,
        collection_id: Uuid,
        concept_id: Uuid,
        position: Option<i32>,
    ) -> Result<()>;

    /// Remove a concept from a collection.
    async fn remove_collection_member(&self, collection_id: Uuid, concept_id: Uuid) -> Result<()>;

    /// Replace all members (for reordering ordered collections).
    async fn replace_collection_members(
        &self,
        collection_id: Uuid,
        req: UpdateCollectionMembersRequest,
    ) -> Result<()>;
}

#[async_trait]
impl SkosCollectionRepository for PgSkosRepository {
    async fn create_collection(&self, req: CreateSkosCollectionRequest) -> Result<Uuid> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        let row = sqlx::query(
            r#"
            INSERT INTO skos_collection (pref_label, definition, is_ordered, scheme_id)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(&req.pref_label)
        .bind(&req.definition)
        .bind(req.is_ordered)
        .bind(req.scheme_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;

        let id: Uuid = row.get("id");

        // Add initial members if provided
        if let Some(concept_ids) = req.concept_ids {
            for (idx, concept_id) in concept_ids.iter().enumerate() {
                let position = if req.is_ordered {
                    Some(idx as i32)
                } else {
                    None
                };
                sqlx::query(
                    r#"
                    INSERT INTO skos_collection_member (collection_id, concept_id, position)
                    VALUES ($1, $2, $3)
                    ON CONFLICT (collection_id, concept_id) DO NOTHING
                    "#,
                )
                .bind(id)
                .bind(concept_id)
                .bind(position)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(id)
    }

    async fn get_collection(&self, id: Uuid) -> Result<Option<SkosCollection>> {
        let row = sqlx::query(
            r#"
            SELECT id, uri, pref_label, definition, is_ordered,
                   scheme_id, created_at, updated_at
            FROM skos_collection
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| SkosCollection {
            id: r.get("id"),
            uri: r.get("uri"),
            pref_label: r.get("pref_label"),
            definition: r.get("definition"),
            is_ordered: r.get("is_ordered"),
            scheme_id: r.get("scheme_id"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn get_collection_with_members(
        &self,
        id: Uuid,
    ) -> Result<Option<SkosCollectionWithMembers>> {
        let collection = match self.get_collection(id).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        let rows = sqlx::query(
            r#"
            SELECT m.concept_id, l.value as pref_label, m.position, m.added_at
            FROM skos_collection_member m
            LEFT JOIN skos_concept_label l
                ON l.concept_id = m.concept_id
                AND l.label_type = 'pref_label'
                AND l.language = 'en'
            WHERE m.collection_id = $1
            ORDER BY CASE WHEN $2 THEN m.position ELSE 0 END, l.value
            "#,
        )
        .bind(id)
        .bind(collection.is_ordered)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let members = rows
            .into_iter()
            .map(|r| SkosCollectionMember {
                concept_id: r.get("concept_id"),
                pref_label: r.get("pref_label"),
                position: r.get("position"),
                added_at: r.get("added_at"),
            })
            .collect();

        Ok(Some(SkosCollectionWithMembers {
            collection,
            members,
        }))
    }

    async fn list_collections(&self, scheme_id: Option<Uuid>) -> Result<Vec<SkosCollection>> {
        let rows = sqlx::query(
            r#"
            SELECT id, uri, pref_label, definition, is_ordered,
                   scheme_id, created_at, updated_at
            FROM skos_collection
            WHERE ($1::uuid IS NULL OR scheme_id = $1)
            ORDER BY pref_label
            "#,
        )
        .bind(scheme_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| SkosCollection {
                id: r.get("id"),
                uri: r.get("uri"),
                pref_label: r.get("pref_label"),
                definition: r.get("definition"),
                is_ordered: r.get("is_ordered"),
                scheme_id: r.get("scheme_id"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    async fn update_collection(&self, id: Uuid, req: UpdateSkosCollectionRequest) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE skos_collection
            SET pref_label = COALESCE($2, pref_label),
                definition = COALESCE($3, definition),
                is_ordered = COALESCE($4, is_ordered)
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(&req.pref_label)
        .bind(&req.definition)
        .bind(req.is_ordered)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn delete_collection(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM skos_collection WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn add_collection_member(
        &self,
        collection_id: Uuid,
        concept_id: Uuid,
        position: Option<i32>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO skos_collection_member (collection_id, concept_id, position)
            VALUES ($1, $2, $3)
            ON CONFLICT (collection_id, concept_id) DO UPDATE SET position = $3
            "#,
        )
        .bind(collection_id)
        .bind(concept_id)
        .bind(position)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn remove_collection_member(&self, collection_id: Uuid, concept_id: Uuid) -> Result<()> {
        sqlx::query(
            "DELETE FROM skos_collection_member WHERE collection_id = $1 AND concept_id = $2",
        )
        .bind(collection_id)
        .bind(concept_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn replace_collection_members(
        &self,
        collection_id: Uuid,
        req: UpdateCollectionMembersRequest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Remove existing members
        sqlx::query("DELETE FROM skos_collection_member WHERE collection_id = $1")
            .bind(collection_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Insert new members with positions
        for (idx, concept_id) in req.concept_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO skos_collection_member (collection_id, concept_id, position)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(collection_id)
            .bind(concept_id)
            .bind(idx as i32)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}

// Default scheme ID tests removed - ID is now dynamically looked up from database
