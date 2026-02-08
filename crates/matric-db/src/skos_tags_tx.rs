//! Transaction-aware variants of SKOS repository methods.
//!
//! This module provides `_tx` variants of all repository methods that accept
//! an external transaction, allowing multiple operations to be composed within
//! a single database transaction.

use chrono::Utc;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{
    new_v7, BatchTagNoteRequest, CreateConceptRequest, CreateConceptSchemeRequest,
    CreateSemanticRelationRequest, CreateSkosCollectionRequest, Error, NoteSkosConceptTag,
    ResolvedTag, Result, SearchConceptsRequest, SearchConceptsResponse, SkosCollection,
    SkosCollectionMember, SkosCollectionWithMembers, SkosConceptFull, SkosConceptScheme,
    SkosConceptSchemeSummary, SkosConceptSummary, SkosConceptWithLabel, SkosGovernanceStats,
    SkosSemanticRelation, SkosSemanticRelationEdge, TagInput, TagNoteRequest, TagStatus,
    UpdateCollectionMembersRequest, UpdateConceptRequest, UpdateConceptSchemeRequest,
    UpdateSkosCollectionRequest, DEFAULT_SCHEME_NOTATION,
};

use crate::skos_tags::{PgSkosRepository, CONCEPT_COLUMNS};

impl PgSkosRepository {
    // ==========================================================================
    // CONCEPT SCHEME TRANSACTION METHODS
    // ==========================================================================

    /// Create a concept scheme within a transaction.
    pub async fn create_scheme_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateConceptSchemeRequest,
    ) -> Result<Uuid> {
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    /// Get a concept scheme by ID within a transaction.
    pub async fn get_scheme_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<SkosConceptScheme>> {
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
        .fetch_optional(&mut **tx)
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

    /// List concept schemes within a transaction.
    pub async fn list_schemes_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        include_inactive: bool,
    ) -> Result<Vec<SkosConceptSchemeSummary>> {
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
            .fetch_all(&mut **tx)
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
                updated_at: r.get("updated_at"),
                concept_count: r.get("concept_count"),
            })
            .collect())
    }

    /// Update a concept scheme within a transaction.
    pub async fn update_scheme_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: UpdateConceptSchemeRequest,
    ) -> Result<()> {
        let now = Utc::now();

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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Delete a concept scheme within a transaction.
    pub async fn delete_scheme_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        force: bool,
    ) -> Result<()> {
        // Check if system scheme
        let is_system: bool =
            sqlx::query_scalar("SELECT is_system FROM skos_concept_scheme WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut **tx)
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
                .fetch_one(&mut **tx)
                .await
                .map_err(Error::Database)?;

        if count > 0 && !force {
            return Err(Error::InvalidInput(format!(
                "Cannot delete scheme with {} concepts. Use force=true to cascade delete.",
                count
            )));
        }

        if count > 0 && force {
            // Cascade delete
            sqlx::query("DELETE FROM skos_concept_in_scheme WHERE scheme_id = $1")
                .bind(id)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

            sqlx::query("DELETE FROM skos_concept WHERE primary_scheme_id = $1")
                .bind(id)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
        }

        sqlx::query("DELETE FROM skos_concept_scheme WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Get top concepts for a scheme within a transaction.
    pub async fn get_top_concepts_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scheme_id: Uuid,
    ) -> Result<Vec<SkosConceptSummary>> {
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
        .fetch_all(&mut **tx)
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

    /// Get default scheme ID within a transaction.
    pub async fn get_default_scheme_id_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            "SELECT id FROM skos_concept_scheme WHERE is_system = TRUE AND notation = 'default'",
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        row.map(|r| r.get("id"))
            .ok_or_else(|| Error::NotFound("Default concept scheme not found".to_string()))
    }

    // ==========================================================================
    // CONCEPT TRANSACTION METHODS
    // ==========================================================================

    /// Create a concept within a transaction.
    pub async fn create_concept_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateConceptRequest,
    ) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();

        // Generate notation if not provided
        let notation = req.notation.unwrap_or_else(|| {
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
        .execute(&mut **tx)
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
        .execute(&mut **tx)
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
            .execute(&mut **tx)
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
            .execute(&mut **tx)
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
            .execute(&mut **tx)
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
        .bind(req.broader_ids.is_empty())
        .bind(now)
        .execute(&mut **tx)
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
            .execute(&mut **tx)
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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        Ok(id)
    }

    /// Search concepts within a transaction.
    pub async fn search_concepts_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: SearchConceptsRequest,
    ) -> Result<SearchConceptsResponse> {
        let mut conditions = vec!["1=1".to_string()];
        let mut param_idx = 1;

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

        let count_query = format!(
            r#"
            SELECT COUNT(DISTINCT c.id)
            FROM skos_concept c
            LEFT JOIN skos_concept_label l ON c.id = l.concept_id
            WHERE {}
            "#,
            where_clause
        );

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
            .fetch_one(&mut **tx)
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

        let rows = main_q.fetch_all(&mut **tx).await.map_err(Error::Database)?;

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

    /// Search labels within a transaction.
    pub async fn search_labels_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SkosConceptWithLabel>> {
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
            .fetch_all(&mut **tx)
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

    /// Get concept with label within a transaction.
    pub async fn get_concept_with_label_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<SkosConceptWithLabel>> {
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
            .fetch_optional(&mut **tx)
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

    /// Get full concept within a transaction.
    pub async fn get_concept_full_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<SkosConceptFull>> {
        // Note: This calls other methods that use self.pool, so we need to inline the queries
        // Get concept
        let concept_row = sqlx::query(
            r#"
            SELECT id, primary_scheme_id, uri, notation, facet_type::text AS facet_type,
                   facet_source, facet_domain, facet_scope, status::text AS status,
                   promoted_at, deprecated_at, deprecation_reason, replaced_by_id,
                   note_count, first_used_at, last_used_at, depth, broader_count,
                   narrower_count, related_count, antipatterns::TEXT[] AS antipatterns,
                   antipattern_checked_at, created_at, updated_at, embedding_model, embedded_at
            FROM skos_concept
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let concept = match concept_row {
            Some(r) => self.row_to_concept(&r),
            None => return Ok(None),
        };

        // Get labels
        let label_rows = sqlx::query(
            r#"
            SELECT id, concept_id, label_type::text AS label_type, value, language, created_at
            FROM skos_concept_label
            WHERE concept_id = $1
            ORDER BY label_type, value
            "#,
        )
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let labels = label_rows
            .into_iter()
            .map(|r| matric_core::SkosConceptLabel {
                id: r.get("id"),
                concept_id: r.get("concept_id"),
                label_type: r
                    .get::<String, _>("label_type")
                    .parse()
                    .unwrap_or(matric_core::SkosLabelType::PrefLabel),
                value: r.get("value"),
                language: r.get("language"),
                created_at: r.get("created_at"),
            })
            .collect();

        // Get notes
        let note_rows = sqlx::query(
            r#"
            SELECT id, concept_id, note_type::text AS note_type, value, language, author,
                   source, created_at, updated_at
            FROM skos_concept_note
            WHERE concept_id = $1
            ORDER BY note_type, created_at
            "#,
        )
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let notes = note_rows
            .into_iter()
            .map(|r| matric_core::SkosConceptNote {
                id: r.get("id"),
                concept_id: r.get("concept_id"),
                note_type: r
                    .get::<String, _>("note_type")
                    .parse()
                    .unwrap_or(matric_core::SkosNoteType::Note),
                value: r.get("value"),
                language: r.get("language"),
                author: r.get("author"),
                source: r.get("source"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

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
        .fetch_all(&mut **tx)
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
        .fetch_all(&mut **tx)
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
        .fetch_all(&mut **tx)
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

        Ok(Some(SkosConceptFull {
            concept,
            labels,
            notes,
            broader,
            narrower,
            related,
            mappings: vec![],
            schemes: vec![],
        }))
    }

    /// Update concept within a transaction.
    pub async fn update_concept_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: UpdateConceptRequest,
    ) -> Result<()> {
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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Delete concept within a transaction.
    pub async fn delete_concept_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<()> {
        // Check if concept has tags
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM note_skos_concept WHERE concept_id = $1")
                .bind(id)
                .fetch_one(&mut **tx)
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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    // ==========================================================================
    // SEMANTIC RELATION TRANSACTION METHODS
    // ==========================================================================

    /// Get semantic relations within a transaction.
    pub async fn get_semantic_relations_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
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
            .fetch_all(&mut **tx)
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
            .fetch_all(&mut **tx)
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

    /// Create semantic relation within a transaction.
    pub async fn create_semantic_relation_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateSemanticRelationRequest,
    ) -> Result<Uuid> {
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    /// Delete semantic relation by triple within a transaction.
    pub async fn delete_semantic_relation_by_triple_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subject_id: Uuid,
        object_id: Uuid,
        relation_type: SkosSemanticRelation,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM skos_semantic_relation_edge
             WHERE ((subject_id = $1 AND object_id = $2) OR (subject_id = $2 AND object_id = $1))
             AND relation_type = $3::skos_semantic_relation",
        )
        .bind(subject_id)
        .bind(object_id)
        .bind(relation_type.to_string())
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    // ==========================================================================
    // NOTE TAGGING TRANSACTION METHODS
    // ==========================================================================

    /// Get note tags with labels within a transaction.
    pub async fn get_note_tags_with_labels_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
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
            .fetch_all(&mut **tx)
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

    /// Tag note within a transaction.
    pub async fn tag_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: TagNoteRequest,
    ) -> Result<()> {
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Untag note within a transaction.
    pub async fn untag_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        concept_id: Uuid,
    ) -> Result<()> {
        sqlx::query("DELETE FROM note_skos_concept WHERE note_id = $1 AND concept_id = $2")
            .bind(note_id)
            .bind(concept_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Batch tag note within a transaction.
    pub async fn batch_tag_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: BatchTagNoteRequest,
    ) -> Result<()> {
        let now = Utc::now();

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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
    }

    // ==========================================================================
    // GOVERNANCE TRANSACTION METHODS
    // ==========================================================================

    /// Get governance stats within a transaction.
    pub async fn get_governance_stats_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scheme_id: Uuid,
    ) -> Result<SkosGovernanceStats> {
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
        .fetch_one(&mut **tx)
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

    // ==========================================================================
    // SKOS COLLECTION TRANSACTION METHODS
    // ==========================================================================

    /// List collections within a transaction.
    pub async fn list_collections_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scheme_id: Option<Uuid>,
    ) -> Result<Vec<SkosCollection>> {
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
        .fetch_all(&mut **tx)
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

    /// Create collection within a transaction.
    pub async fn create_collection_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateSkosCollectionRequest,
    ) -> Result<Uuid> {
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
        .fetch_one(&mut **tx)
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
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        Ok(id)
    }

    /// Get collection with members within a transaction.
    pub async fn get_collection_with_members_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<SkosCollectionWithMembers>> {
        // Get collection
        let collection_row = sqlx::query(
            r#"
            SELECT id, uri, pref_label, definition, is_ordered,
                   scheme_id, created_at, updated_at
            FROM skos_collection
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let collection = match collection_row {
            Some(r) => SkosCollection {
                id: r.get("id"),
                uri: r.get("uri"),
                pref_label: r.get("pref_label"),
                definition: r.get("definition"),
                is_ordered: r.get("is_ordered"),
                scheme_id: r.get("scheme_id"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            },
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
        .fetch_all(&mut **tx)
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

    /// Update collection within a transaction.
    pub async fn update_collection_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: UpdateSkosCollectionRequest,
    ) -> Result<()> {
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    /// Delete collection within a transaction.
    pub async fn delete_collection_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<()> {
        sqlx::query("DELETE FROM skos_collection WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    /// Replace collection members within a transaction.
    pub async fn replace_collection_members_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        collection_id: Uuid,
        req: UpdateCollectionMembersRequest,
    ) -> Result<()> {
        // Remove existing members
        sqlx::query("DELETE FROM skos_collection_member WHERE collection_id = $1")
            .bind(collection_id)
            .execute(&mut **tx)
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
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
    }

    /// Add collection member within a transaction.
    pub async fn add_collection_member_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
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
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    /// Remove collection member within a transaction.
    pub async fn remove_collection_member_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        collection_id: Uuid,
        concept_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM skos_collection_member WHERE collection_id = $1 AND concept_id = $2",
        )
        .bind(collection_id)
        .bind(concept_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    // ==========================================================================
    // TAG RESOLUTION TRANSACTION METHODS
    // ==========================================================================

    /// Helper: Get scheme by notation within a transaction.
    async fn get_scheme_by_notation_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        notation: &str,
    ) -> Result<Option<SkosConceptScheme>> {
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
        .fetch_optional(&mut **tx)
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

    /// Helper: Get concept by notation within a transaction.
    async fn get_concept_by_notation_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scheme_id: Uuid,
        notation: &str,
    ) -> Result<Option<matric_core::SkosConcept>> {
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
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_concept(&r)))
    }

    /// Resolve or create tag within a transaction.
    pub async fn resolve_or_create_tag_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        input: &TagInput,
    ) -> Result<ResolvedTag> {
        // Get the scheme (default if not specified)
        let scheme_notation = &input.scheme;
        let scheme = self.get_scheme_by_notation_tx(tx, scheme_notation).await?;

        let scheme_id = match scheme {
            Some(s) => s.id,
            None => {
                // Use default scheme if specified scheme doesn't exist
                if scheme_notation == DEFAULT_SCHEME_NOTATION {
                    self.get_default_scheme_id_tx(tx).await?
                } else {
                    // Create the scheme if it doesn't exist
                    let scheme_id = self
                        .create_scheme_tx(
                            tx,
                            CreateConceptSchemeRequest {
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
                            },
                        )
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
            let existing = self
                .get_concept_by_notation_tx(tx, scheme_id, &notation)
                .await?;

            let concept_id = match existing {
                Some(concept) => concept.id,
                None => {
                    // Create the concept
                    let mut broader_ids = Vec::new();
                    if let Some(pid) = parent_id {
                        broader_ids.push(pid);
                    }

                    let concept_id = self
                        .create_concept_tx(
                            tx,
                            CreateConceptRequest {
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
                            },
                        )
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
}
