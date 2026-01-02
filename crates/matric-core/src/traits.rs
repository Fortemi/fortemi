//! Core traits for matric-memory abstractions.
//!
//! These traits define the interfaces that concrete implementations
//! must satisfy, enabling pluggable backends and testability.

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::Result;
use crate::models::*;

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
