//! Core traits for matric-memory abstractions.
//!
//! These traits define the interfaces that concrete implementations
//! must satisfy, enabling pluggable backends and testability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::Result;
use crate::models::*;

// =============================================================================
// NOTE REPOSITORY TRAITS
// =============================================================================

/// Request for listing notes.
#[derive(Debug, Clone, Default)]
pub struct ListNotesRequest {
    /// Field to sort by: "created_at_utc", "updated_at", "accessed_at"
    pub sort_by: Option<String>,
    /// Sort order: "asc" or "desc"
    pub sort_order: Option<String>,
    /// Filter: "all", "starred", "archived", "recent", "trash"
    pub filter: Option<String>,
    /// Maximum results
    pub limit: Option<i64>,
    /// Pagination offset
    pub offset: Option<i64>,
    /// Filter by collection
    pub collection_id: Option<Uuid>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
    /// Filter: notes created after this timestamp (ISO 8601)
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp (ISO 8601)
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated after this timestamp (ISO 8601)
    pub updated_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated before this timestamp (ISO 8601)
    pub updated_before: Option<chrono::DateTime<chrono::Utc>>,
}

/// Response for listing notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNotesResponse {
    pub notes: Vec<NoteSummary>,
    pub total: i64,
}

/// Request for updating note status.
#[derive(Debug, Clone, Default)]
pub struct UpdateNoteStatusRequest {
    pub starred: Option<bool>,
    pub archived: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

/// Request for creating a new note.
#[derive(Debug, Clone)]
pub struct CreateNoteRequest {
    pub content: String,
    pub format: String,
    pub source: String,
    pub collection_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    /// Optional document type ID for explicit typing
    pub document_type_id: Option<Uuid>,
}

/// Repository for note CRUD operations.
#[async_trait]
pub trait NoteRepository: Send + Sync {
    /// Insert a new note.
    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid>;

    /// Insert multiple notes in a single transaction.
    async fn insert_bulk(&self, notes: Vec<CreateNoteRequest>) -> Result<Vec<Uuid>>;

    /// Fetch a full note by ID.
    async fn fetch(&self, id: Uuid) -> Result<NoteFull>;

    /// List notes with filtering and pagination.
    async fn list(&self, req: ListNotesRequest) -> Result<ListNotesResponse>;

    /// Update note status (starred, archived).
    async fn update_status(&self, id: Uuid, req: UpdateNoteStatusRequest) -> Result<()>;

    /// Update original content.
    async fn update_original(&self, id: Uuid, content: &str) -> Result<()>;

    /// Update revised content with optional rationale.
    async fn update_revised(
        &self,
        id: Uuid,
        content: &str,
        rationale: Option<&str>,
    ) -> Result<Uuid>;

    /// Soft-delete a note.
    async fn soft_delete(&self, id: Uuid) -> Result<()>;

    /// Permanently delete a note.
    async fn hard_delete(&self, id: Uuid) -> Result<()>;

    /// Restore a soft-deleted note.
    async fn restore(&self, id: Uuid) -> Result<()>;

    /// Check if a note exists.
    async fn exists(&self, id: Uuid) -> Result<bool>;

    /// Update note title.
    async fn update_title(&self, id: Uuid, title: &str) -> Result<()>;

    /// List all active (non-deleted) note IDs.
    /// Used for bulk operations like re-embedding.
    async fn list_all_ids(&self) -> Result<Vec<Uuid>>;
}

// =============================================================================
// EMBEDDING REPOSITORY TRAITS
// =============================================================================

/// Repository for embedding storage and retrieval.
#[async_trait]
pub trait EmbeddingRepository: Send + Sync {
    /// Store embeddings for a note, replacing any existing ones.
    async fn store(&self, note_id: Uuid, chunks: Vec<(String, Vector)>, model: &str) -> Result<()>;

    /// Get all embeddings for a note.
    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<Embedding>>;

    /// Delete all embeddings for a note.
    async fn delete_for_note(&self, note_id: Uuid) -> Result<()>;

    /// Find similar notes by vector.
    async fn find_similar(
        &self,
        query_vec: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>>;
}

// =============================================================================
// LINK REPOSITORY TRAITS
// =============================================================================

/// Repository for link storage and retrieval.
#[async_trait]
pub trait LinkRepository: Send + Sync {
    /// Create a link between notes.
    async fn create(
        &self,
        from_note_id: Uuid,
        to_note_id: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<Uuid>;

    /// Create reciprocal links (bidirectional).
    async fn create_reciprocal(
        &self,
        note_a: Uuid,
        note_b: Uuid,
        kind: &str,
        score: f32,
        metadata: Option<JsonValue>,
    ) -> Result<()>;

    /// Get all outgoing links from a note.
    async fn get_outgoing(&self, note_id: Uuid) -> Result<Vec<Link>>;

    /// Get all incoming links to a note.
    async fn get_incoming(&self, note_id: Uuid) -> Result<Vec<Link>>;

    /// Delete all links for a note (both directions).
    async fn delete_for_note(&self, note_id: Uuid) -> Result<()>;
}

// =============================================================================
// TAG REPOSITORY TRAITS
// =============================================================================

/// Repository for tag operations.
#[async_trait]
pub trait TagRepository: Send + Sync {
    /// Create a tag if it doesn't exist.
    async fn create(&self, name: &str) -> Result<()>;

    /// List all tags.
    async fn list(&self) -> Result<Vec<Tag>>;

    /// Add a tag to a note.
    async fn add_to_note(&self, note_id: Uuid, tag_name: &str, source: &str) -> Result<()>;

    /// Remove a tag from a note.
    async fn remove_from_note(&self, note_id: Uuid, tag_name: &str) -> Result<()>;

    /// Get all tags for a note.
    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<String>>;

    /// Set tags for a note (replace all).
    async fn set_for_note(&self, note_id: Uuid, tags: Vec<String>, source: &str) -> Result<()>;
}

// =============================================================================
// COLLECTION REPOSITORY TRAITS
// =============================================================================

/// Repository for collection (folder) operations.
#[async_trait]
pub trait CollectionRepository: Send + Sync {
    /// Create a new collection.
    async fn create(
        &self,
        name: &str,
        description: Option<&str>,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid>;

    /// Get a collection by ID.
    async fn get(&self, id: Uuid) -> Result<Option<crate::Collection>>;

    /// List all collections (optionally filtered by parent).
    async fn list(&self, parent_id: Option<Uuid>) -> Result<Vec<crate::Collection>>;

    /// Update a collection.
    async fn update(&self, id: Uuid, name: &str, description: Option<&str>) -> Result<()>;

    /// Delete a collection (moves notes to uncategorized).
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Get notes in a collection.
    async fn get_notes(&self, id: Uuid, limit: i64, offset: i64)
        -> Result<Vec<crate::NoteSummary>>;

    /// Move a note to a collection.
    async fn move_note(&self, note_id: Uuid, collection_id: Option<Uuid>) -> Result<()>;
}

// =============================================================================
// JOB REPOSITORY TRAITS
// =============================================================================

/// Repository for job queue operations.
#[async_trait]
pub trait JobRepository: Send + Sync {
    /// Queue a new job.
    async fn queue(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
    ) -> Result<Uuid>;

    /// Queue a job with deduplication (skip if same type+note pending).
    async fn queue_deduplicated(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
    ) -> Result<Option<Uuid>>;

    /// Claim the next pending job for processing.
    async fn claim_next(&self) -> Result<Option<Job>>;

    /// Claim the next pending job whose type is in `job_types`.
    /// An empty slice means "claim any type" (same as `claim_next`).
    async fn claim_next_for_types(&self, job_types: &[JobType]) -> Result<Option<Job>>;

    /// Update job progress.
    async fn update_progress(
        &self,
        job_id: Uuid,
        percent: i32,
        message: Option<&str>,
    ) -> Result<()>;

    /// Mark job as completed.
    async fn complete(&self, job_id: Uuid, result: Option<JsonValue>) -> Result<()>;

    /// Mark job as failed.
    async fn fail(&self, job_id: Uuid, error: &str) -> Result<()>;

    /// Get job by ID.
    async fn get(&self, job_id: Uuid) -> Result<Option<Job>>;

    /// Get all jobs for a note.
    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<Job>>;

    /// Get pending jobs count.
    async fn pending_count(&self) -> Result<i64>;

    /// List recent jobs.
    async fn list_recent(&self, limit: i64) -> Result<Vec<Job>>;

    /// List jobs with filtering.
    async fn list_filtered(
        &self,
        status: Option<&str>,
        job_type: Option<&str>,
        note_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Job>>;

    /// Get queue statistics.
    async fn queue_stats(&self) -> Result<QueueStats>;

    /// Clean up old completed/failed jobs.
    async fn cleanup(&self, keep_count: i64) -> Result<i64>;
}

// =============================================================================
// INFERENCE TRAITS
// =============================================================================

/// Backend for generating text embeddings.
#[async_trait]
pub trait EmbeddingBackend: Send + Sync {
    /// Generate embeddings for the given texts.
    ///
    /// Returns a vector of embedding vectors, one per input text.
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<crate::Vector>>;

    /// Get the expected dimension of embedding vectors.
    fn dimension(&self) -> usize;

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

/// Backend for text generation (LLM).
#[async_trait]
pub trait GenerationBackend: Send + Sync {
    /// Generate text given a prompt.
    async fn generate(&self, prompt: &str) -> Result<String>;

    /// Generate text with system context.
    async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String>;

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

/// Combined inference backend supporting both embedding and generation.
#[async_trait]
pub trait InferenceBackend: EmbeddingBackend + GenerationBackend {
    /// Check if the backend is available and responding.
    async fn health_check(&self) -> Result<bool>;
}

// =============================================================================
// SEARCH TRAITS
// =============================================================================

/// Configuration for search queries.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub query: String,
    pub mode: SearchMode,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub collection_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub include_archived: bool,
}

/// Provider for search operations.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Perform a search with the given query.
    async fn search(&self, query: SearchQuery) -> Result<SearchResponse>;

    /// Find notes semantically similar to the given text.
    async fn semantic_search(&self, text: &str, limit: i64) -> Result<SemanticResponse>;

    /// Find notes related to a specific note.
    async fn find_related(&self, note_id: Uuid, limit: i64) -> Result<Vec<SearchHit>>;
}

// =============================================================================
// JOB PROCESSING TRAITS
// =============================================================================

/// Notification handler for job status updates.
#[async_trait]
pub trait JobNotifier: Send + Sync {
    /// Called when a job is queued.
    async fn on_job_queued(&self, job: &Job);

    /// Called when a job starts processing.
    async fn on_job_started(&self, job: &Job);

    /// Called when job progress is updated.
    async fn on_job_progress(&self, job: &Job);

    /// Called when a job completes successfully.
    async fn on_job_completed(&self, job: &Job);

    /// Called when a job fails.
    async fn on_job_failed(&self, job: &Job);
}

/// No-op notifier for when notifications aren't needed.
pub struct NoOpNotifier;

#[async_trait]
impl JobNotifier for NoOpNotifier {
    async fn on_job_queued(&self, _job: &Job) {}
    async fn on_job_started(&self, _job: &Job) {}
    async fn on_job_progress(&self, _job: &Job) {}
    async fn on_job_completed(&self, _job: &Job) {}
    async fn on_job_failed(&self, _job: &Job) {}
}

// =============================================================================
// EXTRACTION ADAPTER TRAITS
// =============================================================================

/// Result of content extraction from a file attachment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionResult {
    /// Extracted text content, if any.
    pub extracted_text: Option<String>,
    /// Metadata about the extraction (format-specific).
    pub metadata: JsonValue,
    /// AI-generated description of the content.
    pub ai_description: Option<String>,
    /// Preview data (e.g., thumbnail bytes). Skipped in serialization.
    #[serde(skip)]
    pub preview_data: Option<Vec<u8>>,
}

/// Adapter for extracting content from file attachments.
///
/// Each adapter handles one extraction strategy (e.g., TextNative, PdfText).
/// Adapters are registered in an `ExtractionRegistry` and dispatched based
/// on the file's detected `ExtractionStrategy`.
#[async_trait]
pub trait ExtractionAdapter: Send + Sync {
    /// The extraction strategy this adapter handles.
    fn strategy(&self) -> crate::ExtractionStrategy;

    /// Extract content from raw file data.
    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult>;

    /// Check if the adapter's external dependencies are available.
    async fn health_check(&self) -> Result<bool>;

    /// Human-readable name of this adapter.
    fn name(&self) -> &str;
}

// =============================================================================
// CONTENT PROCESSOR TRAITS
// =============================================================================

/// AI metadata extracted from content.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AiMetadata {
    pub categories: Vec<String>,
    pub topics: Vec<String>,
    pub keywords: Vec<String>,
    pub entities: serde_json::Value,
    pub summary: Option<String>,
}

/// Processor for AI-enhanced content operations.
#[async_trait]
pub trait ContentProcessor: Send + Sync {
    /// Generate an AI-enhanced revision of the content.
    async fn generate_revision(
        &self,
        original: &str,
        context_notes: &[NoteFull],
    ) -> Result<(String, AiMetadata)>;

    /// Generate a title from content.
    async fn generate_title(&self, content: &str) -> Result<String>;

    /// Extract tags from content.
    async fn extract_tags(&self, content: &str) -> Result<Vec<String>>;
}

// =============================================================================
// TEMPLATE REPOSITORY TRAITS
// =============================================================================

/// Request for creating a new template.
#[derive(Debug, Clone)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub format: Option<String>,
    pub default_tags: Option<Vec<String>>,
    pub collection_id: Option<Uuid>,
}

/// Request for updating a template.
#[derive(Debug, Clone, Default)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub default_tags: Option<Vec<String>>,
    pub collection_id: Option<Option<Uuid>>,
}

/// Repository for note template operations.
#[async_trait]
pub trait TemplateRepository: Send + Sync {
    /// Create a new template.
    async fn create(&self, req: CreateTemplateRequest) -> Result<Uuid>;

    /// Get a template by ID.
    async fn get(&self, id: Uuid) -> Result<Option<crate::NoteTemplate>>;

    /// Get a template by name.
    async fn get_by_name(&self, name: &str) -> Result<Option<crate::NoteTemplate>>;

    /// List all templates.
    async fn list(&self) -> Result<Vec<crate::NoteTemplate>>;

    /// Update a template.
    async fn update(&self, id: Uuid, req: UpdateTemplateRequest) -> Result<()>;

    /// Delete a template.
    async fn delete(&self, id: Uuid) -> Result<()>;
}

// =============================================================================
// DOCUMENT TYPE REPOSITORY TRAITS
// =============================================================================

/// Repository for document type operations.
#[async_trait]
pub trait DocumentTypeRepository: Send + Sync {
    /// List all document types.
    async fn list(&self) -> Result<Vec<crate::DocumentTypeSummary>>;

    /// List document types by category.
    async fn list_by_category(&self, category: &str) -> Result<Vec<crate::DocumentTypeSummary>>;

    /// Get a document type by ID.
    async fn get(&self, id: Uuid) -> Result<Option<crate::DocumentType>>;

    /// Get a document type by name.
    async fn get_by_name(&self, name: &str) -> Result<Option<crate::DocumentType>>;

    /// Create a new document type.
    async fn create(&self, req: crate::CreateDocumentTypeRequest) -> Result<Uuid>;

    /// Update a document type.
    async fn update(&self, name: &str, req: crate::UpdateDocumentTypeRequest) -> Result<()>;

    /// Delete a document type (only non-system types).
    async fn delete(&self, name: &str) -> Result<()>;

    /// Detect document type from filename, optional content, and/or MIME type.
    async fn detect(
        &self,
        filename: Option<&str>,
        content: Option<&str>,
        mime_type: Option<&str>,
    ) -> Result<Option<crate::DetectDocumentTypeResult>>;

    /// Get document type by file extension.
    async fn get_by_extension(&self, extension: &str) -> Result<Option<crate::DocumentType>>;

    /// Get document type by filename pattern.
    async fn get_by_filename(&self, filename: &str) -> Result<Option<crate::DocumentType>>;
}

// =============================================================================
// ARCHIVE REPOSITORY TRAITS
// =============================================================================

/// Repository for archive schema operations (Epic #441: Parallel Memory Archives).
///
/// Manages isolated PostgreSQL schemas for parallel memory archives,
/// allowing complete data separation at the schema level.
#[async_trait]
pub trait ArchiveRepository: Send + Sync {
    /// Create a new archive schema with all necessary tables.
    ///
    /// Creates a new PostgreSQL schema and replicates the full table structure
    /// (notes, embeddings, collections, tags, etc.) within that schema.
    ///
    /// # Arguments
    /// * `name` - Human-readable name for the archive
    /// * `description` - Optional description of the archive's purpose
    ///
    /// # Returns
    /// Archive information including the generated schema name
    async fn create_archive_schema(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<crate::ArchiveInfo>;

    /// Drop an archive schema and all its data.
    ///
    /// **WARNING**: This permanently deletes all notes, embeddings, and metadata
    /// within the archive schema. Cannot be undone.
    ///
    /// # Arguments
    /// * `name` - Name of the archive to drop
    async fn drop_archive_schema(&self, name: &str) -> Result<()>;

    /// List all archive schemas.
    async fn list_archive_schemas(&self) -> Result<Vec<crate::ArchiveInfo>>;

    /// Get archive information by name.
    async fn get_archive_by_name(&self, name: &str) -> Result<Option<crate::ArchiveInfo>>;

    /// Get archive information by ID.
    async fn get_archive_by_id(&self, id: Uuid) -> Result<Option<crate::ArchiveInfo>>;

    /// Get the default archive.
    ///
    /// Returns the archive marked as default, or None if no default is set.
    /// Used by the archive routing middleware to determine which schema to use
    /// for requests that don't explicitly specify an archive.
    async fn get_default_archive(&self) -> Result<Option<crate::ArchiveInfo>>;

    /// Set an archive as the default.
    ///
    /// Only one archive can be default at a time. Setting a new default
    /// will unset the previous default.
    async fn set_default_archive(&self, name: &str) -> Result<()>;

    /// Update archive metadata (description, statistics).
    async fn update_archive_metadata(&self, name: &str, description: Option<&str>) -> Result<()>;

    /// Update archive statistics (note count, size).
    async fn update_archive_stats(&self, name: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    // =============================================================================
    // Request/Response Tests
    // =============================================================================

    #[test]
    fn test_list_notes_request_default() {
        let req = ListNotesRequest::default();
        assert!(req.sort_by.is_none());
        assert!(req.sort_order.is_none());
        assert!(req.filter.is_none());
        assert!(req.limit.is_none());
        assert!(req.offset.is_none());
        assert!(req.collection_id.is_none());
        assert!(req.tags.is_none());
        assert!(req.created_after.is_none());
        assert!(req.created_before.is_none());
        assert!(req.updated_after.is_none());
        assert!(req.updated_before.is_none());
    }

    #[test]
    fn test_list_notes_request_with_filters() {
        let collection_id = Uuid::new_v4();
        let now = Utc::now();

        let req = ListNotesRequest {
            sort_by: Some("created_at_utc".to_string()),
            sort_order: Some("desc".to_string()),
            filter: Some("starred".to_string()),
            limit: Some(50),
            offset: Some(0),
            collection_id: Some(collection_id),
            tags: Some(vec!["rust".to_string(), "programming".to_string()]),
            created_after: Some(now),
            created_before: None,
            updated_after: None,
            updated_before: None,
        };

        assert_eq!(req.sort_by.unwrap(), "created_at_utc");
        assert_eq!(req.filter.unwrap(), "starred");
        assert_eq!(req.limit.unwrap(), 50);
        assert_eq!(req.tags.unwrap().len(), 2);
    }

    #[test]
    fn test_list_notes_response_serialization() {
        let response = ListNotesResponse {
            notes: vec![],
            total: 0,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: ListNotesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total, 0);
        assert_eq!(parsed.notes.len(), 0);
    }

    #[test]
    fn test_update_note_status_request_default() {
        let req = UpdateNoteStatusRequest::default();
        assert!(req.starred.is_none());
        assert!(req.archived.is_none());
    }

    #[test]
    fn test_update_note_status_request_partial() {
        let req = UpdateNoteStatusRequest {
            starred: Some(true),
            archived: None,
            metadata: None,
        };
        assert_eq!(req.starred, Some(true));
        assert!(req.archived.is_none());
    }

    #[test]
    fn test_create_note_request() {
        let req = CreateNoteRequest {
            content: "Test content".to_string(),
            format: "markdown".to_string(),
            source: "manual".to_string(),
            collection_id: None,
            tags: Some(vec!["test".to_string()]),
            metadata: None,
            document_type_id: None,
        };

        assert_eq!(req.content, "Test content");
        assert_eq!(req.format, "markdown");
        assert_eq!(req.source, "manual");
        assert_eq!(req.tags.unwrap().len(), 1);
    }

    #[test]
    fn test_search_query_default() {
        let query = SearchQuery::default();
        assert_eq!(query.query, "");
        assert_eq!(query.mode, SearchMode::default());
        assert!(query.limit.is_none());
        assert!(query.offset.is_none());
        assert!(query.collection_id.is_none());
        assert!(query.tags.is_empty());
        assert!(!query.include_archived);
    }

    #[test]
    fn test_search_query_with_params() {
        let query = SearchQuery {
            query: "rust programming".to_string(),
            mode: SearchMode::Hybrid,
            limit: Some(20),
            offset: Some(0),
            collection_id: Some(Uuid::new_v4()),
            tags: vec!["rust".to_string()],
            include_archived: false,
        };

        assert_eq!(query.query, "rust programming");
        assert_eq!(query.mode, SearchMode::Hybrid);
        assert_eq!(query.limit.unwrap(), 20);
        assert!(!query.include_archived);
    }

    #[test]
    fn test_ai_metadata_default() {
        let metadata = AiMetadata::default();
        assert!(metadata.categories.is_empty());
        assert!(metadata.topics.is_empty());
        assert!(metadata.keywords.is_empty());
        assert!(metadata.summary.is_none());
    }

    #[test]
    fn test_ai_metadata_serialization() {
        let metadata = AiMetadata {
            categories: vec!["technology".to_string()],
            topics: vec!["rust".to_string(), "programming".to_string()],
            keywords: vec!["async".to_string(), "trait".to_string()],
            entities: json!({"person": ["John Doe"]}),
            summary: Some("A test summary".to_string()),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: AiMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.categories.len(), 1);
        assert_eq!(parsed.topics.len(), 2);
        assert_eq!(parsed.keywords.len(), 2);
        assert_eq!(parsed.summary.unwrap(), "A test summary");
    }

    #[test]
    fn test_create_template_request() {
        let req = CreateTemplateRequest {
            name: "Daily Note".to_string(),
            description: Some("Template for daily notes".to_string()),
            content: "# {{date}}\n\n## Notes\n\n".to_string(),
            format: Some("markdown".to_string()),
            default_tags: Some(vec!["daily".to_string()]),
            collection_id: None,
        };

        assert_eq!(req.name, "Daily Note");
        assert_eq!(req.format.unwrap(), "markdown");
        assert_eq!(req.default_tags.unwrap().len(), 1);
    }

    #[test]
    fn test_update_template_request_default() {
        let req = UpdateTemplateRequest::default();
        assert!(req.name.is_none());
        assert!(req.description.is_none());
        assert!(req.content.is_none());
        assert!(req.default_tags.is_none());
        assert!(req.collection_id.is_none());
    }

    #[test]
    fn test_update_template_request_partial() {
        let req = UpdateTemplateRequest {
            name: Some("Updated Name".to_string()),
            description: None,
            content: Some("New content".to_string()),
            default_tags: None,
            collection_id: Some(Some(Uuid::new_v4())),
        };

        assert_eq!(req.name.unwrap(), "Updated Name");
        assert_eq!(req.content.unwrap(), "New content");
        assert!(req.description.is_none());
    }

    // =============================================================================
    // NoOpNotifier Tests
    // =============================================================================

    #[tokio::test]
    async fn test_noop_notifier_on_job_queued() {
        let notifier = NoOpNotifier;
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: JobStatus::Pending,
            priority: 5,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        };

        // Should not panic
        notifier.on_job_queued(&job).await;
    }

    #[tokio::test]
    async fn test_noop_notifier_on_job_started() {
        let notifier = NoOpNotifier;
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::AiRevision,
            status: JobStatus::Running,
            priority: 8,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };

        notifier.on_job_started(&job).await;
    }

    #[tokio::test]
    async fn test_noop_notifier_on_job_progress() {
        let notifier = NoOpNotifier;
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::Linking,
            status: JobStatus::Running,
            priority: 3,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 50,
            progress_message: Some("Halfway done".to_string()),
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };

        notifier.on_job_progress(&job).await;
    }

    #[tokio::test]
    async fn test_noop_notifier_on_job_completed() {
        let notifier = NoOpNotifier;
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::TitleGeneration,
            status: JobStatus::Completed,
            priority: 2,
            payload: None,
            result: Some(json!({"title": "Generated Title"})),
            error_message: None,
            progress_percent: 100,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        };

        notifier.on_job_completed(&job).await;
    }

    #[tokio::test]
    async fn test_noop_notifier_on_job_failed() {
        let notifier = NoOpNotifier;
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::Embedding,
            status: JobStatus::Failed,
            priority: 5,
            payload: None,
            result: None,
            error_message: Some("Model timeout".to_string()),
            progress_percent: 25,
            progress_message: None,
            retry_count: 1,
            max_retries: 3,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        };

        notifier.on_job_failed(&job).await;
    }

    // =============================================================================
    // Type Tests
    // =============================================================================

    #[test]
    fn test_search_query_debug_format() {
        let query = SearchQuery {
            query: "test".to_string(),
            mode: SearchMode::Hybrid,
            limit: Some(10),
            offset: Some(0),
            collection_id: None,
            tags: vec![],
            include_archived: false,
        };

        let debug_str = format!("{:?}", query);
        assert!(debug_str.contains("SearchQuery"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_create_note_request_debug_format() {
        let req = CreateNoteRequest {
            content: "test".to_string(),
            format: "markdown".to_string(),
            source: "manual".to_string(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
        };

        let debug_str = format!("{:?}", req);
        assert!(debug_str.contains("CreateNoteRequest"));
    }

    #[test]
    fn test_ai_metadata_with_entities() {
        let metadata = AiMetadata {
            categories: vec![],
            topics: vec![],
            keywords: vec![],
            entities: json!({
                "person": ["Alice", "Bob"],
                "organization": ["ACME Corp"],
                "location": ["New York"]
            }),
            summary: None,
        };

        assert!(metadata.entities.is_object());
        assert_eq!(metadata.entities["person"][0], "Alice");
    }

    #[test]
    fn test_list_notes_request_clone() {
        let req1 = ListNotesRequest {
            sort_by: Some("created_at_utc".to_string()),
            sort_order: Some("asc".to_string()),
            ..Default::default()
        };

        let req2 = req1.clone();
        assert_eq!(req1.sort_by, req2.sort_by);
        assert_eq!(req1.sort_order, req2.sort_order);
    }

    #[test]
    fn test_update_note_status_request_clone() {
        let req1 = UpdateNoteStatusRequest {
            starred: Some(true),
            archived: Some(false),
            metadata: None,
        };

        let req2 = req1.clone();
        assert_eq!(req1.starred, req2.starred);
        assert_eq!(req1.archived, req2.archived);
    }

    #[test]
    fn test_search_query_clone() {
        let query1 = SearchQuery {
            query: "test".to_string(),
            mode: SearchMode::Fts,
            limit: Some(10),
            offset: Some(5),
            collection_id: Some(Uuid::new_v4()),
            tags: vec!["tag1".to_string()],
            include_archived: true,
        };

        let query2 = query1.clone();
        assert_eq!(query1.query, query2.query);
        assert_eq!(query1.mode, query2.mode);
        assert_eq!(query1.limit, query2.limit);
        assert_eq!(query1.include_archived, query2.include_archived);
    }
}
