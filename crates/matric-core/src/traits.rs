//! Core traits for matric-memory abstractions.
//!
//! These traits define the interfaces that concrete implementations
//! must satisfy, enabling pluggable backends and testability.

use async_trait::async_trait;
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
}

/// Response for listing notes.
#[derive(Debug, Clone)]
pub struct ListNotesResponse {
    pub notes: Vec<NoteSummary>,
    pub total: i64,
}

/// Request for updating note status.
#[derive(Debug, Clone, Default)]
pub struct UpdateNoteStatusRequest {
    pub starred: Option<bool>,
    pub archived: Option<bool>,
}

/// Request for creating a new note.
#[derive(Debug, Clone)]
pub struct CreateNoteRequest {
    pub content: String,
    pub format: String,
    pub source: String,
    pub collection_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
}

/// Repository for note CRUD operations.
#[async_trait]
pub trait NoteRepository: Send + Sync {
    /// Insert a new note.
    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid>;

    /// Fetch a full note by ID.
    async fn fetch(&self, id: Uuid) -> Result<NoteFull>;

    /// List notes with filtering and pagination.
    async fn list(&self, req: ListNotesRequest) -> Result<ListNotesResponse>;

    /// Update note status (starred, archived).
    async fn update_status(&self, id: Uuid, req: UpdateNoteStatusRequest) -> Result<()>;

    /// Update original content.
    async fn update_original(&self, id: Uuid, content: &str) -> Result<()>;

    /// Update revised content with optional rationale.
    async fn update_revised(&self, id: Uuid, content: &str, rationale: Option<&str>) -> Result<Uuid>;

    /// Soft-delete a note.
    async fn soft_delete(&self, id: Uuid) -> Result<()>;

    /// Permanently delete a note.
    async fn hard_delete(&self, id: Uuid) -> Result<()>;

    /// Restore a soft-deleted note.
    async fn restore(&self, id: Uuid) -> Result<()>;

    /// Check if a note exists.
    async fn exists(&self, id: Uuid) -> Result<bool>;
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
    async fn find_similar(&self, query_vec: &Vector, limit: i64, exclude_archived: bool) -> Result<Vec<SearchHit>>;
}

// =============================================================================
// LINK REPOSITORY TRAITS
// =============================================================================

/// Repository for link storage and retrieval.
#[async_trait]
pub trait LinkRepository: Send + Sync {
    /// Create a link between notes.
    async fn create(&self, from_note_id: Uuid, to_note_id: Uuid, kind: &str, score: f32, metadata: Option<JsonValue>) -> Result<Uuid>;

    /// Create reciprocal links (bidirectional).
    async fn create_reciprocal(&self, note_a: Uuid, note_b: Uuid, kind: &str, score: f32, metadata: Option<JsonValue>) -> Result<()>;

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
// JOB REPOSITORY TRAITS
// =============================================================================

/// Repository for job queue operations.
#[async_trait]
pub trait JobRepository: Send + Sync {
    /// Queue a new job.
    async fn queue(&self, note_id: Option<Uuid>, job_type: JobType, priority: i32, payload: Option<JsonValue>) -> Result<Uuid>;

    /// Queue a job with deduplication (skip if same type+note pending).
    async fn queue_deduplicated(&self, note_id: Option<Uuid>, job_type: JobType, priority: i32, payload: Option<JsonValue>) -> Result<Option<Uuid>>;

    /// Claim the next pending job for processing.
    async fn claim_next(&self) -> Result<Option<Job>>;

    /// Update job progress.
    async fn update_progress(&self, job_id: Uuid, percent: i32, message: Option<&str>) -> Result<()>;

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
