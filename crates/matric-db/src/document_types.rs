//! PostgreSQL implementation of DocumentTypeRepository.

use async_trait::async_trait;
use matric_core::{
    new_v7, AgenticConfig, ChunkingStrategy, CreateDocumentTypeRequest, DetectDocumentTypeResult,
    DocumentCategory, DocumentType, DocumentTypeRepository, DocumentTypeSummary, Error,
    ExtractionStrategy, Result, UpdateDocumentTypeRequest,
};
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

fn document_type_not_found_error(name: &str) -> Error {
    Error::NotFound(format!(
        "Document type not found; name_len={}",
        name.chars().count()
    ))
}

fn document_type_in_use_error(name: &str, reference_count: i64) -> Error {
    Error::InvalidInput(format!(
        "Cannot delete document type; name_len={}; reference_count_present={}",
        name.chars().count(),
        reference_count > 0
    ))
}

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
        let mut conn = self.pool.acquire().await?;
        Self::list_tx(&mut conn).await
    }

    async fn list_by_category(&self, category: &str) -> Result<Vec<DocumentTypeSummary>> {
        let mut conn = self.pool.acquire().await?;
        Self::list_by_category_tx(&mut conn, category).await
    }

    async fn get(&self, id: Uuid) -> Result<Option<DocumentType>> {
        let mut conn = self.pool.acquire().await?;
        Self::get_tx(&mut conn, id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<DocumentType>> {
        let mut conn = self.pool.acquire().await?;
        Self::get_by_name_tx(&mut conn, name).await
    }

    async fn create(&self, req: CreateDocumentTypeRequest) -> Result<Uuid> {
        let mut conn = self.pool.acquire().await?;
        Self::create_tx(&mut conn, req).await
    }

    async fn update(&self, name: &str, req: UpdateDocumentTypeRequest) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        Self::update_tx(&mut conn, name, req).await
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        Self::delete_tx(&mut conn, name).await
    }

    async fn detect(
        &self,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>> {
        let mut conn = self.pool.acquire().await?;
        DetectionRepository {
            conn: &mut conn,
            active_default_only: false,
        }
        .detect(filename, content, mime_type)
        .await
    }

    async fn get_by_extension(&self, extension: &str) -> Result<Option<DocumentType>> {
        let mut conn = self.pool.acquire().await?;
        DetectionRepository {
            conn: &mut conn,
            active_default_only: false,
        }
        .get_by_extension(extension)
        .await
    }
    async fn get_by_filename(&self, filename: &str) -> Result<Option<DocumentType>> {
        let mut conn = self.pool.acquire().await?;
        DetectionRepository {
            conn: &mut conn,
            active_default_only: false,
        }
        .get_by_filename(filename)
        .await
    }
}

impl PgDocumentTypeRepository {
    fn row_to_document_type(row: &sqlx::postgres::PgRow) -> DocumentType {
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

    fn to_summary(doc_type: &DocumentType) -> DocumentTypeSummary {
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

mod catalog;
mod scoped;

impl PgDocumentTypeRepository {
    /// Detect using the same tenant and connection as the pending write.
    pub async fn detect_tx(
        conn: &mut sqlx::PgConnection,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>> {
        DetectionRepository {
            conn,
            active_default_only: false,
        }
        .detect(filename, content, mime_type)
        .await
    }

    /// Validate explicit assignments before writes, including privileged callers.
    pub(crate) async fn validate_assignment_tx(
        conn: &mut sqlx::PgConnection,
        document_type_id: Option<Uuid>,
    ) -> Result<()> {
        if let Some(id) = document_type_id {
            let visible: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM public.document_type
                 WHERE id = $1 AND tenant_id = current_setting('app.current_tenant')::uuid",
            )
            .bind(id)
            .fetch_optional(conn)
            .await?;
            if visible.is_none() {
                return Err(Error::InvalidInput("Unknown document type".into()));
            }
        }
        Ok(())
    }

    /// Detect on the caller's tenant transaction; the registry lives in public.
    /// Hosted fallback must not select an inactive tenant configuration.
    pub async fn detect_scoped(
        scope: &mut crate::TenantScopedConn<'_>,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>> {
        DetectionRepository {
            conn: scope.executor(),
            active_default_only: true,
        }
        .detect(filename, content, mime_type)
        .await
    }
}

// Both community and hosted entry points use the same precedence and scoring.
// The legacy default lookup retains its historical inactive-plaintext behavior.
struct DetectionRepository<'a> {
    conn: &'a mut sqlx::PgConnection,
    active_default_only: bool,
}

impl DetectionRepository<'_> {
    async fn detect(
        &mut self,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<DetectDocumentTypeResult>> {
        // 1. Try filename pattern match first (highest confidence — exact filenames like "Dockerfile")
        if let Some(fname) = filename {
            if let Some(doc_type) = self.get_by_filename(fname).await? {
                return Ok(Some(DetectDocumentTypeResult {
                    document_type: PgDocumentTypeRepository::to_summary(&doc_type),
                    confidence: matric_core::defaults::DETECT_CONFIDENCE_FILENAME,
                    detection_method: "filename_pattern".to_string(),
                }));
            }
        }

        // 2. Try MIME type match (high confidence for binary formats)
        if let Some(mime) = mime_type {
            if let Some(doc_type) = self.get_by_mime_type(mime).await? {
                return Ok(Some(DetectDocumentTypeResult {
                    document_type: PgDocumentTypeRepository::to_summary(&doc_type),
                    confidence: matric_core::defaults::DETECT_CONFIDENCE_MIME,
                    detection_method: "mime_type".to_string(),
                }));
            }
        }

        // 3. Try extension match — file extensions are authoritative for specific
        //    types like .py, .rs, .go (issue #287: content patterns can misidentify
        //    code files, e.g. AsciiDoc patterns matching Python assignment operators).
        //    However, for generic format types (.yaml, .json, .xml), content detection
        //    can find a more specific type (issue #312: .yaml + openapi content → openapi).
        if let Some(fname) = filename {
            if let Some(ext) = std::path::Path::new(fname)
                .extension()
                .and_then(|e| e.to_str())
            {
                let ext_with_dot = format!(".{}", ext.to_lowercase());
                if let Some(doc_type) = self.get_by_extension(&ext_with_dot).await? {
                    // For generic format types, also check content for a more specific match
                    let is_generic_format = matches!(
                        doc_type.name.as_str(),
                        "yaml"
                            | "json"
                            | "xml"
                            | "html"
                            | "markdown"
                            | "plaintext"
                            | "toml"
                            | "csv"
                            | "text"
                    );

                    if is_generic_format {
                        if let Some(text) = content {
                            if let Some(content_result) = self.detect_by_content(text).await? {
                                if content_result.document_type.name != doc_type.name {
                                    // Content found a more specific type — boost confidence
                                    // since both extension and content corroborate
                                    return Ok(Some(DetectDocumentTypeResult {
                                        document_type: content_result.document_type,
                                        confidence:
                                            matric_core::defaults::DETECT_CONFIDENCE_EXTENSION,
                                        detection_method: "content_pattern+file_extension"
                                            .to_string(),
                                    }));
                                }
                            }
                        }
                    }

                    return Ok(Some(DetectDocumentTypeResult {
                        document_type: PgDocumentTypeRepository::to_summary(&doc_type),
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
                document_type: PgDocumentTypeRepository::to_summary(&doc_type),
                confidence: matric_core::defaults::DETECT_CONFIDENCE_DEFAULT,
                detection_method: "default".to_string(),
            }));
        }

        Ok(None)
    }

    async fn get_by_extension(&mut self, extension: &str) -> Result<Option<DocumentType>> {
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
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE AND $1 = ANY(file_extensions)
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
        .fetch_optional(&mut *self.conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| PgDocumentTypeRepository::row_to_document_type(&r)))
    }

    async fn get_by_filename(&mut self, filename: &str) -> Result<Option<DocumentType>> {
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
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE AND $1 = ANY(filename_patterns)
            "#,
        )
        .bind(filename)
        .fetch_optional(&mut *self.conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| PgDocumentTypeRepository::row_to_document_type(&r)))
    }
    /// Helper: detect document type from content magic patterns (issue #124, #199).
    /// Scores each type by number of matching patterns rather than first-match-wins.
    async fn detect_by_content(&mut self, text: &str) -> Result<Option<DetectDocumentTypeResult>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active, magic_patterns
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE AND array_length(magic_patterns, 1) > 0
            ORDER BY name
            "#,
        )
        .fetch_all(&mut *self.conn)
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
                    category: PgDocumentTypeRepository::parse_category(row.get("category")),
                    description: row.get("description"),
                    chunking_strategy: PgDocumentTypeRepository::parse_chunking_strategy(
                        row.get("chunking_strategy"),
                    ),
                    tree_sitter_language: row.get("tree_sitter_language"),
                    extraction_strategy: PgDocumentTypeRepository::parse_extraction_strategy(
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
    async fn get_by_mime_type(&mut self, mime_type: &str) -> Result<Option<DocumentType>> {
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
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE AND $1 = ANY(mime_types)
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
        .fetch_optional(&mut *self.conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| PgDocumentTypeRepository::row_to_document_type(&r)))
    }

    async fn get_by_name(&mut self, name: &str) -> Result<Option<DocumentType>> {
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
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND name = $1 AND (NOT $2 OR is_active)
            "#,
        )
        .bind(name)
        .bind(self.active_default_only)
        .fetch_optional(&mut *self.conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| PgDocumentTypeRepository::row_to_document_type(&r)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_type_not_found_errors_report_metadata_without_raw_values() {
        let raw_name = "secret-type-sk-live-123/path@example.com";
        let err = document_type_not_found_error(raw_name);

        let Error::NotFound(message) = err else {
            panic!("expected not-found error");
        };

        assert!(message.contains("Document type not found"));
        assert!(message.contains("name_len=40"));
        assert!(!message.contains(raw_name));
        assert!(!message.contains("secret-type"));
        assert!(!message.contains("sk-live"));
        assert!(!message.contains("path@example.com"));
    }

    #[test]
    fn document_type_in_use_errors_report_metadata_without_raw_values() {
        let raw_name = "customer-report-sk-live-123/path@example.com";
        let err = document_type_in_use_error(raw_name, 7);

        let Error::InvalidInput(message) = err else {
            panic!("expected invalid-input error");
        };

        assert!(message.contains("Cannot delete document type"));
        assert!(message.contains("name_len=44"));
        assert!(message.contains("reference_count_present=true"));
        assert!(!message.contains("reference_count=7"));
        assert!(!message.contains(raw_name));
        assert!(!message.contains("customer-report"));
        assert!(!message.contains("sk-live"));
        assert!(!message.contains("path@example.com"));
    }
}
