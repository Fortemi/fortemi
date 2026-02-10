//! PostgreSQL implementation of DocumentTypeRepository.

use async_trait::async_trait;
use matric_core::{
    new_v7, AgenticConfig, ChunkingStrategy, CreateDocumentTypeRequest, DetectDocumentTypeResult,
    DocumentCategory, DocumentType, DocumentTypeRepository, DocumentTypeSummary, Error,
    ExtractionStrategy, Result, UpdateDocumentTypeRequest,
};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

pub struct PgDocumentTypeRepository {
    pool: Pool<Postgres>,
}

impl PgDocumentTypeRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    // Helper to parse category from string
    fn parse_category(s: &str) -> DocumentCategory {
        s.parse().unwrap_or(DocumentCategory::Custom)
    }

    // Helper to parse chunking strategy from string
    fn parse_chunking_strategy(s: &str) -> ChunkingStrategy {
        s.parse().unwrap_or(ChunkingStrategy::Semantic)
    }

    // Helper to parse extraction strategy from string
    fn parse_extraction_strategy(s: Option<&str>) -> ExtractionStrategy {
        s.and_then(|s| s.parse().ok())
            .unwrap_or(ExtractionStrategy::TextNative)
    }
}

#[async_trait]
impl DocumentTypeRepository for PgDocumentTypeRepository {
    async fn list(&self) -> Result<Vec<DocumentTypeSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active
            FROM document_type
            WHERE is_active = TRUE
            ORDER BY category, name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| DocumentTypeSummary {
                id: row.get("id"),
                name: row.get("name"),
                display_name: row.get("display_name"),
                category: Self::parse_category(row.get("category")),
                description: row.get("description"),
                chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
                tree_sitter_language: row.get("tree_sitter_language"),
                extraction_strategy: Self::parse_extraction_strategy(
                    row.get::<Option<&str>, _>("extraction_strategy"),
                ),
                requires_attachment: row
                    .get::<Option<bool>, _>("requires_attachment")
                    .unwrap_or(false),
                is_system: row.get("is_system"),
                is_active: row.get("is_active"),
            })
            .collect())
    }

    async fn list_by_category(&self, category: &str) -> Result<Vec<DocumentTypeSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active
            FROM document_type
            WHERE is_active = TRUE AND category::TEXT = $1
            ORDER BY name
            "#,
        )
        .bind(category)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| DocumentTypeSummary {
                id: row.get("id"),
                name: row.get("name"),
                display_name: row.get("display_name"),
                category: Self::parse_category(row.get("category")),
                description: row.get("description"),
                chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
                tree_sitter_language: row.get("tree_sitter_language"),
                extraction_strategy: Self::parse_extraction_strategy(
                    row.get::<Option<&str>, _>("extraction_strategy"),
                ),
                requires_attachment: row
                    .get::<Option<bool>, _>("requires_attachment")
                    .unwrap_or(false),
                is_system: row.get("is_system"),
                is_active: row.get("is_active"),
            })
            .collect())
    }

    async fn get(&self, id: Uuid) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM document_type
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_document_type(&r)))
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM document_type
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_document_type(&r)))
    }

    async fn create(&self, req: CreateDocumentTypeRequest) -> Result<Uuid> {
        let id = new_v7();
        // Auto-generate display_name from name if not provided (kebab-case → Title Case)
        let display_name = req.display_name.unwrap_or_else(|| {
            req.name
                .split(['-', '_'])
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        });

        sqlx::query(
            r#"
            INSERT INTO document_type (
                id, name, display_name, category, description,
                file_extensions, mime_types, magic_patterns, filename_patterns,
                chunking_strategy, chunk_size_default, chunk_overlap_default,
                preserve_boundaries, chunking_config, recommended_config_id,
                content_types, tree_sitter_language, is_system
            ) VALUES (
                $1, $2, $3, $4::document_category, $5,
                $6, $7, $8, $9,
                $10::chunking_strategy, $11, $12,
                $13, $14, $15,
                $16, $17, FALSE
            )
            "#,
        )
        .bind(id)
        .bind(&req.name)
        .bind(&display_name)
        .bind(req.category.to_string())
        .bind(&req.description)
        .bind(&req.file_extensions)
        .bind(&req.mime_types)
        .bind(&req.magic_patterns)
        .bind(&req.filename_patterns)
        .bind(req.chunking_strategy.to_string())
        .bind(req.chunk_size_default)
        .bind(req.chunk_overlap_default)
        .bind(req.preserve_boundaries)
        .bind(req.chunking_config.unwrap_or(serde_json::json!({})))
        .bind(req.recommended_config_id)
        .bind(&req.content_types)
        .bind(&req.tree_sitter_language)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn update(&self, name: &str, req: UpdateDocumentTypeRequest) -> Result<()> {
        // First check if it's a system type
        let existing = self.get_by_name(name).await?;
        if let Some(doc_type) = existing {
            if doc_type.is_system {
                return Err(Error::InvalidInput(
                    "Cannot modify system document type".into(),
                ));
            }
        } else {
            return Err(Error::NotFound(format!(
                "Document type '{}' not found",
                name
            )));
        }

        // Use simpler approach with conditional updates
        sqlx::query(
            r#"
            UPDATE document_type SET
                display_name = COALESCE($2, display_name),
                description = COALESCE($3, description),
                file_extensions = COALESCE($4, file_extensions),
                chunking_strategy = COALESCE($5::chunking_strategy, chunking_strategy),
                chunk_size_default = COALESCE($6, chunk_size_default),
                chunk_overlap_default = COALESCE($7, chunk_overlap_default),
                is_active = COALESCE($8, is_active),
                updated_at = NOW()
            WHERE name = $1 AND is_system = FALSE
            "#,
        )
        .bind(name)
        .bind(&req.display_name)
        .bind(&req.description)
        .bind(&req.file_extensions)
        .bind(req.chunking_strategy.map(|s| s.to_string()))
        .bind(req.chunk_size_default)
        .bind(req.chunk_overlap_default)
        .bind(req.is_active)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        // Check if system type
        let existing = self.get_by_name(name).await?;
        if let Some(doc_type) = existing {
            if doc_type.is_system {
                return Err(Error::InvalidInput(
                    "Cannot delete system document type".into(),
                ));
            }
        } else {
            return Err(Error::NotFound(format!(
                "Document type '{}' not found",
                name
            )));
        }

        // Check if in use by notes
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM note WHERE document_type_id = (SELECT id FROM document_type WHERE name = $1)"
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        if count > 0 {
            return Err(Error::InvalidInput(format!(
                "Cannot delete document type '{}': {} notes reference it",
                name, count
            )));
        }

        sqlx::query("DELETE FROM document_type WHERE name = $1 AND is_system = FALSE")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn detect(
        &self,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>> {
        // 1. Try filename pattern match first (highest confidence — exact filenames like "Dockerfile")
        if let Some(fname) = filename {
            if let Some(doc_type) = self.get_by_filename(fname).await? {
                return Ok(Some(DetectDocumentTypeResult {
                    document_type: self.to_summary(&doc_type),
                    confidence: matric_core::defaults::DETECT_CONFIDENCE_FILENAME,
                    detection_method: "filename_pattern".to_string(),
                }));
            }
        }

        // 2. Try MIME type match (high confidence for binary formats)
        if let Some(mime) = mime_type {
            if let Some(doc_type) = self.get_by_mime_type(mime).await? {
                return Ok(Some(DetectDocumentTypeResult {
                    document_type: self.to_summary(&doc_type),
                    confidence: matric_core::defaults::DETECT_CONFIDENCE_MIME,
                    detection_method: "mime_type".to_string(),
                }));
            }
        }

        // 3. Try extension match — file extensions are authoritative for specific
        //    types like .py, .rs, .go (issue #287: content patterns can misidentify
        //    code files, e.g. AsciiDoc patterns matching Python assignment operators).
        if let Some(fname) = filename {
            if let Some(ext) = std::path::Path::new(fname)
                .extension()
                .and_then(|e| e.to_str())
            {
                let ext_with_dot = format!(".{}", ext.to_lowercase());
                if let Some(doc_type) = self.get_by_extension(&ext_with_dot).await? {
                    return Ok(Some(DetectDocumentTypeResult {
                        document_type: self.to_summary(&doc_type),
                        confidence: matric_core::defaults::DETECT_CONFIDENCE_EXTENSION,
                        detection_method: "file_extension".to_string(),
                    }));
                }
            }
        }

        // 4. Try content pattern match (when extension didn't match or no filename)
        if let Some(text) = content {
            if let Some(result) = self.detect_by_content(text).await? {
                return Ok(Some(result));
            }
        }

        // 6. Default to plaintext
        if let Some(doc_type) = self.get_by_name("plaintext").await? {
            return Ok(Some(DetectDocumentTypeResult {
                document_type: self.to_summary(&doc_type),
                confidence: matric_core::defaults::DETECT_CONFIDENCE_DEFAULT,
                detection_method: "default".to_string(),
            }));
        }

        Ok(None)
    }

    async fn get_by_extension(&self, extension: &str) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM document_type
            WHERE is_active = TRUE AND $1 = ANY(file_extensions)
            ORDER BY
                -- Prefer generic types (no filename_patterns) over specific types
                (CASE WHEN filename_patterns IS NULL OR array_length(filename_patterns, 1) IS NULL THEN 0 ELSE 1 END),
                -- Among generic types, prefer fewer extensions (more specific to this extension)
                array_length(file_extensions, 1),
                name
            LIMIT 1
            "#,
        )
        .bind(extension)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_document_type(&r)))
    }

    async fn get_by_filename(&self, filename: &str) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM document_type
            WHERE is_active = TRUE AND $1 = ANY(filename_patterns)
            "#,
        )
        .bind(filename)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_document_type(&r)))
    }
}

impl PgDocumentTypeRepository {
    /// Helper: detect document type from content magic patterns (issue #124, #199).
    /// Scores each type by number of matching patterns rather than first-match-wins.
    async fn detect_by_content(&self, text: &str) -> Result<Option<DetectDocumentTypeResult>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active, magic_patterns
            FROM document_type
            WHERE is_active = TRUE AND array_length(magic_patterns, 1) > 0
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let mut best_idx: Option<usize> = None;
        let mut best_score: usize = 0;

        for (idx, row) in rows.iter().enumerate() {
            let patterns: Vec<String> = row
                .get::<Option<Vec<String>>, _>("magic_patterns")
                .unwrap_or_default();
            let score = patterns
                .iter()
                .filter(|p| text.contains(p.as_str()))
                .count();
            if score > best_score {
                best_score = score;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            let row = &rows[idx];
            return Ok(Some(DetectDocumentTypeResult {
                document_type: DocumentTypeSummary {
                    id: row.get("id"),
                    name: row.get("name"),
                    display_name: row.get("display_name"),
                    category: Self::parse_category(row.get("category")),
                    description: row.get("description"),
                    chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
                    tree_sitter_language: row.get("tree_sitter_language"),
                    extraction_strategy: Self::parse_extraction_strategy(
                        row.get::<Option<&str>, _>("extraction_strategy"),
                    ),
                    requires_attachment: row
                        .get::<Option<bool>, _>("requires_attachment")
                        .unwrap_or(false),
                    is_system: row.get("is_system"),
                    is_active: row.get("is_active"),
                },
                confidence: matric_core::defaults::DETECT_CONFIDENCE_CONTENT,
                detection_method: "content_pattern".to_string(),
            }));
        }

        Ok(None)
    }

    /// Find a document type by MIME type.
    async fn get_by_mime_type(&self, mime_type: &str) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM document_type
            WHERE is_active = TRUE AND $1 = ANY(mime_types)
            ORDER BY
                -- Prefer generic types (no filename_patterns) over specific types
                (CASE WHEN filename_patterns IS NULL OR array_length(filename_patterns, 1) IS NULL THEN 0 ELSE 1 END),
                -- Among generic types, prefer fewer MIME types (more specific to this MIME)
                array_length(mime_types, 1),
                name
            LIMIT 1
            "#,
        )
        .bind(mime_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| self.row_to_document_type(&r)))
    }

    fn row_to_document_type(&self, row: &sqlx::postgres::PgRow) -> DocumentType {
        // Try to load agentic_config from database, fall back to default on error
        let agentic_config: AgenticConfig = row
            .try_get::<serde_json::Value, _>("agentic_config")
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Load extraction_config, fall back to empty object
        let extraction_config: serde_json::Value = row
            .try_get::<serde_json::Value, _>("extraction_config")
            .ok()
            .unwrap_or_else(|| serde_json::json!({}));

        DocumentType {
            id: row.get("id"),
            name: row.get("name"),
            display_name: row.get("display_name"),
            category: Self::parse_category(row.get("category")),
            description: row.get("description"),
            file_extensions: row
                .get::<Option<Vec<String>>, _>("file_extensions")
                .unwrap_or_default(),
            mime_types: row
                .get::<Option<Vec<String>>, _>("mime_types")
                .unwrap_or_default(),
            magic_patterns: row
                .get::<Option<Vec<String>>, _>("magic_patterns")
                .unwrap_or_default(),
            filename_patterns: row
                .get::<Option<Vec<String>>, _>("filename_patterns")
                .unwrap_or_default(),
            chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
            chunk_size_default: row.get("chunk_size_default"),
            chunk_overlap_default: row.get("chunk_overlap_default"),
            preserve_boundaries: row.get("preserve_boundaries"),
            chunking_config: row.get("chunking_config"),
            recommended_config_id: row.get("recommended_config_id"),
            content_types: row
                .get::<Option<Vec<String>>, _>("content_types")
                .unwrap_or_default(),
            tree_sitter_language: row.get("tree_sitter_language"),
            extraction_strategy: Self::parse_extraction_strategy(
                row.get::<Option<&str>, _>("extraction_strategy"),
            ),
            extraction_config,
            requires_attachment: row
                .get::<Option<bool>, _>("requires_attachment")
                .unwrap_or(false),
            attachment_generates_content: row
                .get::<Option<bool>, _>("attachment_generates_content")
                .unwrap_or(false),
            is_system: row.get("is_system"),
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            created_by: row.get("created_by"),
            agentic_config,
        }
    }

    fn to_summary(&self, doc_type: &DocumentType) -> DocumentTypeSummary {
        DocumentTypeSummary {
            id: doc_type.id,
            name: doc_type.name.clone(),
            display_name: doc_type.display_name.clone(),
            category: doc_type.category,
            description: doc_type.description.clone(),
            chunking_strategy: doc_type.chunking_strategy,
            tree_sitter_language: doc_type.tree_sitter_language.clone(),
            extraction_strategy: doc_type.extraction_strategy,
            requires_attachment: doc_type.requires_attachment,
            is_system: doc_type.is_system,
            is_active: doc_type.is_active,
        }
    }
}
