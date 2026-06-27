//! Core traits for matric-memory abstractions.
//!
//! These traits define the interfaces that concrete implementations
//! must satisfy, enabling pluggable backends and testability.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;
use uuid::Uuid;

use crate::error::Result;
use crate::models::*;

// =============================================================================
// NOTE REPOSITORY TRAITS
// =============================================================================

/// Request for listing notes.
#[derive(Clone, Default)]
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

impl fmt::Debug for ListNotesRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListNotesRequest")
            .field("sort_by_len", &optional_str_len(self.sort_by.as_deref()))
            .field(
                "sort_order_len",
                &optional_str_len(self.sort_order.as_deref()),
            )
            .field("filter_len", &optional_str_len(self.filter.as_deref()))
            .field("limit", &self.limit)
            .field("offset", &self.offset)
            .field("collection_id_present", &self.collection_id.is_some())
            .field("tags_count", &self.tags.as_ref().map(Vec::len))
            .field(
                "tag_lens",
                &self
                    .tags
                    .as_ref()
                    .map(|tags| tags.iter().map(|tag| str_len(tag)).collect::<Vec<_>>()),
            )
            .field("created_after", &self.created_after)
            .field("created_before", &self.created_before)
            .field("updated_after", &self.updated_after)
            .field("updated_before", &self.updated_before)
            .finish()
    }
}

/// Response for listing notes.
#[derive(Clone, Serialize, Deserialize)]
pub struct ListNotesResponse {
    pub notes: Vec<NoteSummary>,
    pub total: i64,
}

impl fmt::Debug for ListNotesResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListNotesResponse")
            .field("notes_count", &self.notes.len())
            .field("total", &self.total)
            .finish()
    }
}

/// Response for listing global attachments.
#[derive(Clone, Serialize, Deserialize)]
pub struct ListGlobalAttachmentsResponse {
    pub attachments: Vec<GlobalAttachmentSummary>,
    pub total: i64,
}

impl fmt::Debug for ListGlobalAttachmentsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListGlobalAttachmentsResponse")
            .field("attachments_count", &self.attachments.len())
            .field("total", &self.total)
            .finish()
    }
}

/// Request for updating note status.
#[derive(Clone, Default)]
pub struct UpdateNoteStatusRequest {
    pub starred: Option<bool>,
    pub archived: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

impl fmt::Debug for UpdateNoteStatusRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateNoteStatusRequest")
            .field("starred", &self.starred)
            .field("archived", &self.archived)
            .field(
                "metadata_class",
                &self.metadata.as_ref().map(json_debug_class),
            )
            .field(
                "metadata_serialized_len",
                &self.metadata.as_ref().map(json_serialized_len),
            )
            .finish()
    }
}

/// Request for creating a new note.
#[derive(Clone)]
pub struct CreateNoteRequest {
    pub content: String,
    pub format: String,
    pub source: String,
    pub collection_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    /// Optional document type ID for explicit typing
    pub document_type_id: Option<Uuid>,
    /// Optional explicit title. When provided, the AI title-generation
    /// pipeline step is skipped — the caller's value is authoritative.
    /// When `None`, behavior follows `revision_mode` and document-type
    /// agent hints. Added for #675 so import paths (shard rebuild,
    /// bulk-load tools) can set titles deterministically without
    /// depending on inference.
    pub title: Option<String>,
}

impl fmt::Debug for CreateNoteRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateNoteRequest")
            .field("content_len", &str_len(&self.content))
            .field("format_len", &str_len(&self.format))
            .field("source_len", &str_len(&self.source))
            .field("collection_id_present", &self.collection_id.is_some())
            .field("tags_count", &self.tags.as_ref().map(Vec::len))
            .field(
                "tag_lens",
                &self
                    .tags
                    .as_ref()
                    .map(|tags| tags.iter().map(|tag| str_len(tag)).collect::<Vec<_>>()),
            )
            .field(
                "metadata_class",
                &self.metadata.as_ref().map(json_debug_class),
            )
            .field(
                "metadata_serialized_len",
                &self.metadata.as_ref().map(json_serialized_len),
            )
            .field("document_type_id_present", &self.document_type_id.is_some())
            .field("title_len", &optional_str_len(self.title.as_deref()))
            .finish()
    }
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

    /// Find similar notes by vector, returning embedding vectors alongside hits.
    ///
    /// Used by HNSW Algorithm 4 (diverse neighbor selection) which needs the
    /// actual vectors to compute inter-candidate distances for the diversity check.
    async fn find_similar_with_vectors(
        &self,
        query_vec: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<(SearchHit, Vector)>>;
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

    /// Move a collection to a new parent, with circular reference prevention.
    async fn move_collection(&self, id: Uuid, new_parent_id: Option<Uuid>) -> Result<()>;
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
        cost_tier: Option<i16>,
    ) -> Result<Uuid>;

    /// Queue a job with deduplication (skip if same type+note pending).
    async fn queue_deduplicated(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
        cost_tier: Option<i16>,
    ) -> Result<Option<Uuid>>;

    /// Claim the next pending job for processing.
    async fn claim_next(&self) -> Result<Option<Job>>;

    /// Claim the next pending job whose type is in `job_types`.
    /// An empty slice means "claim any type" (same as `claim_next`).
    async fn claim_next_for_types(&self, job_types: &[JobType]) -> Result<Option<Job>>;

    /// Claim the next pending job for a specific cost tier group.
    ///
    /// Tier groups:
    /// - `CpuAndAgnostic`: cost_tier IS NULL OR cost_tier = 0
    /// - `FastGpu`: cost_tier = 1
    /// - `StandardGpu`: cost_tier = 2
    /// - `RenderGpu`: cost_tier = 4
    /// - `VisionGpu`: cost_tier = 3
    async fn claim_next_for_tier(
        &self,
        tier_group: TierGroup,
        job_types: &[JobType],
    ) -> Result<Option<Job>>;

    /// Count pending jobs for a specific cost tier.
    async fn pending_count_for_tier(&self, tier: i16) -> Result<i64>;

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

    /// Reap stale running jobs that exceeded the timeout.
    ///
    /// On worker startup, jobs left in `running` status from a previous process
    /// (e.g. after a crash or restart) will never complete. This resets them to
    /// `pending` with incremented retry count so they get re-processed.
    ///
    /// Returns the number of jobs reaped.
    async fn reap_stale_running(&self, timeout_secs: u64) -> Result<i64>;
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

/// Owned, `Send`able async stream of text chunks — return type for streaming
/// generation methods. Implementations can build one from any stream via
/// `Box::pin(my_stream)`.
pub type GenerationStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = Result<String>> + Send + 'static>>;

/// Backend for text generation (LLM).
#[async_trait]
pub trait GenerationBackend: Send + Sync {
    /// Generate text given a prompt.
    async fn generate(&self, prompt: &str) -> Result<String>;

    /// Generate text with system context.
    async fn generate_with_system(&self, system: &str, prompt: &str) -> Result<String>;

    /// Generate text with JSON format enforcement.
    ///
    /// Backends that support constrained decoding (e.g., Ollama `format: "json"`)
    /// will guarantee valid JSON output. The default implementation falls back to
    /// `generate()` without format enforcement.
    async fn generate_json(&self, prompt: &str) -> Result<String> {
        self.generate(prompt).await
    }

    /// Generate text with system context and JSON format enforcement.
    ///
    /// See [`generate_json`] for details on format enforcement.
    async fn generate_json_with_system(&self, system: &str, prompt: &str) -> Result<String> {
        self.generate_with_system(system, prompt).await
    }

    /// Stream generated text chunk-by-chunk (#629).
    ///
    /// Backends that support native streaming (Ollama `stream: true`,
    /// OpenAI `stream: true` SSE) should override this to emit one item
    /// per token-ish chunk. The default implementation calls the blocking
    /// [`generate`] and yields one final chunk — wire-compatible but
    /// non-progressive.
    ///
    /// Consumers (e.g., `POST /api/v1/inference/stream`) emit each chunk
    /// as an SSE `delta` event, so even the fallback is correct; it just
    /// doesn't give the progressive UI the user expects.
    async fn stream_generate(&self, prompt: &str) -> Result<GenerationStream> {
        let full = self.generate(prompt).await?;
        Ok(Box::pin(futures::stream::once(async move { Ok(full) })))
    }

    /// Stream generated text with a system context. See [`stream_generate`].
    async fn stream_generate_with_system(
        &self,
        system: &str,
        prompt: &str,
    ) -> Result<GenerationStream> {
        let full = self.generate_with_system(system, prompt).await?;
        Ok(Box::pin(futures::stream::once(async move { Ok(full) })))
    }

    /// Get the model name being used.
    fn model_name(&self) -> &str;
}

#[cfg(test)]
mod generation_backend_tests {
    use super::*;
    use futures::StreamExt;

    /// Minimal backend that counts generate() calls and returns a fixed
    /// string. Used to verify the default stream_* impls yield exactly
    /// one chunk and route through the blocking generate path.
    struct FakeGen {
        fixed: String,
    }

    #[async_trait]
    impl GenerationBackend for FakeGen {
        async fn generate(&self, _prompt: &str) -> Result<String> {
            Ok(self.fixed.clone())
        }
        async fn generate_with_system(&self, _s: &str, _p: &str) -> Result<String> {
            Ok(self.fixed.clone())
        }
        fn model_name(&self) -> &str {
            "fake"
        }
    }

    #[tokio::test]
    async fn default_stream_generate_yields_one_chunk() {
        let g = FakeGen {
            fixed: "hello world".to_string(),
        };
        let mut stream = g.stream_generate("ignored").await.unwrap();
        let mut chunks = Vec::new();
        while let Some(item) = stream.next().await {
            chunks.push(item.unwrap());
        }
        assert_eq!(chunks, vec!["hello world".to_string()]);
    }

    #[tokio::test]
    async fn default_stream_generate_with_system_yields_one_chunk() {
        let g = FakeGen {
            fixed: "with system".to_string(),
        };
        let mut stream = g
            .stream_generate_with_system("sys", "prompt")
            .await
            .unwrap();
        let mut chunks = Vec::new();
        while let Some(item) = stream.next().await {
            chunks.push(item.unwrap());
        }
        assert_eq!(chunks, vec!["with system".to_string()]);
    }
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
#[derive(Clone, Default)]
pub struct SearchQuery {
    pub query: String,
    pub mode: SearchMode,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub collection_id: Option<Uuid>,
    pub tags: Vec<String>,
    pub include_archived: bool,
}

impl fmt::Debug for SearchQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchQuery")
            .field("query_len", &str_len(&self.query))
            .field("mode", &self.mode)
            .field("limit", &self.limit)
            .field("offset", &self.offset)
            .field("collection_id_present", &self.collection_id.is_some())
            .field("tags_count", &self.tags.len())
            .field(
                "tag_lens",
                &self.tags.iter().map(|tag| str_len(tag)).collect::<Vec<_>>(),
            )
            .field("include_archived", &self.include_archived)
            .finish()
    }
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

/// A file extracted from within a parent attachment (e.g., email attachment, archive entry).
///
/// Used by adapters that decompose compound files into child attachments.
/// The extraction handler creates derived attachments for each entry.
#[derive(Clone)]
pub struct DerivedFile {
    /// Filename for the extracted file.
    pub filename: String,
    /// MIME type of the extracted file.
    pub content_type: String,
    /// Raw binary content. May be empty when `source_path` is set.
    pub data: Vec<u8>,
    /// Relationship to parent (e.g., "email_attachment", "archive_entry", "keyframe").
    pub derivation_type: String,
    /// Optional AI-generated description for this derived file.
    pub ai_description: Option<String>,
    /// Optional structured metadata merged into the derived attachment's `extracted_metadata`.
    /// Used for keyframe index/timestamp, view angles, etc.
    pub metadata: Option<JsonValue>,
    /// Optional path to file on disk instead of holding bytes in `data`.
    /// When set, the extraction handler reads from this path during persistence,
    /// allowing large files (keyframe JPEGs, audio tracks) to stay on disk
    /// instead of accumulating in memory.
    pub source_path: Option<std::path::PathBuf>,
}

impl fmt::Debug for DerivedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DerivedFile")
            .field("filename_len", &str_len(&self.filename))
            .field("content_type_len", &str_len(&self.content_type))
            .field("data_len", &self.data.len())
            .field("derivation_type_len", &str_len(&self.derivation_type))
            .field(
                "ai_description_len",
                &optional_str_len(self.ai_description.as_deref()),
            )
            .field(
                "metadata_class",
                &self.metadata.as_ref().map(json_debug_class),
            )
            .field(
                "metadata_serialized_len",
                &self.metadata.as_ref().map(json_serialized_len),
            )
            .field(
                "source_path_len",
                &self.source_path.as_ref().map(|path| path_display_len(path)),
            )
            .finish()
    }
}

/// Result of content extraction from a file attachment.
#[derive(Clone, Default, Serialize, Deserialize)]
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
    /// Files extracted from within this attachment (e.g., email attachments, archive entries).
    /// Each entry becomes a derived attachment linked to the parent.
    #[serde(skip)]
    pub derived_files: Vec<DerivedFile>,
}

impl fmt::Debug for ExtractionResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractionResult")
            .field(
                "extracted_text_len",
                &optional_str_len(self.extracted_text.as_deref()),
            )
            .field("metadata_class", &json_debug_class(&self.metadata))
            .field(
                "metadata_serialized_len",
                &json_serialized_len(&self.metadata),
            )
            .field(
                "ai_description_len",
                &optional_str_len(self.ai_description.as_deref()),
            )
            .field(
                "preview_data_len",
                &self.preview_data.as_ref().map(Vec::len),
            )
            .field("derived_files_count", &self.derived_files.len())
            .field(
                "derived_file_data_lens",
                &self
                    .derived_files
                    .iter()
                    .map(|file| file.data.len())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Progress callback for extraction adapters.
///
/// Reports `(percent 0-100, optional message)` during long-running extraction.
/// Used by adapters that process items sequentially (e.g., per-keyframe in video,
/// per-chunk in large documents) to give visibility into actual progress.
pub type ProgressFn = std::sync::Arc<dyn Fn(i32, Option<&str>) + Send + Sync>;

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

    /// Extract content with granular progress reporting.
    ///
    /// The `progress` callback receives (percent, message) updates as the
    /// adapter processes items (keyframes, chunks, pages). Percent values
    /// should be in the 0-100 range within the adapter's scope — the caller
    /// is responsible for mapping to the overall job progress range.
    ///
    /// Default implementation delegates to [`extract`] (no granular progress).
    async fn extract_with_progress(
        &self,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
        _progress: ProgressFn,
    ) -> Result<ExtractionResult> {
        self.extract(data, filename, mime_type, config).await
    }

    /// Check if the adapter's external dependencies are available.
    async fn health_check(&self) -> Result<bool>;

    /// Human-readable name of this adapter.
    fn name(&self) -> &str;
}

// =============================================================================
// CONTENT PROCESSOR TRAITS
// =============================================================================

/// AI metadata extracted from content.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AiMetadata {
    pub categories: Vec<String>,
    pub topics: Vec<String>,
    pub keywords: Vec<String>,
    pub entities: serde_json::Value,
    pub summary: Option<String>,
}

impl fmt::Debug for AiMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiMetadata")
            .field("categories_count", &self.categories.len())
            .field(
                "category_lens",
                &self
                    .categories
                    .iter()
                    .map(|category| str_len(category))
                    .collect::<Vec<_>>(),
            )
            .field("topics_count", &self.topics.len())
            .field(
                "topic_lens",
                &self
                    .topics
                    .iter()
                    .map(|topic| str_len(topic))
                    .collect::<Vec<_>>(),
            )
            .field("keywords_count", &self.keywords.len())
            .field(
                "keyword_lens",
                &self
                    .keywords
                    .iter()
                    .map(|keyword| str_len(keyword))
                    .collect::<Vec<_>>(),
            )
            .field("entities_class", &json_debug_class(&self.entities))
            .field(
                "entities_serialized_len",
                &json_serialized_len(&self.entities),
            )
            .field("summary_len", &optional_str_len(self.summary.as_deref()))
            .finish()
    }
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
#[derive(Clone)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub format: Option<String>,
    pub default_tags: Option<Vec<String>>,
    pub collection_id: Option<Uuid>,
}

impl fmt::Debug for CreateTemplateRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateTemplateRequest")
            .field("name_len", &str_len(&self.name))
            .field(
                "description_len",
                &optional_str_len(self.description.as_deref()),
            )
            .field("content_len", &str_len(&self.content))
            .field("format_len", &optional_str_len(self.format.as_deref()))
            .field(
                "default_tags_count",
                &self.default_tags.as_ref().map(Vec::len),
            )
            .field(
                "default_tag_lens",
                &self
                    .default_tags
                    .as_ref()
                    .map(|tags| tags.iter().map(|tag| str_len(tag)).collect::<Vec<_>>()),
            )
            .field("collection_id_present", &self.collection_id.is_some())
            .finish()
    }
}

/// Request for updating a template.
#[derive(Clone, Default)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub default_tags: Option<Vec<String>>,
    pub collection_id: Option<Option<Uuid>>,
}

impl fmt::Debug for UpdateTemplateRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateTemplateRequest")
            .field("name_len", &optional_str_len(self.name.as_deref()))
            .field(
                "description_len",
                &optional_str_len(self.description.as_deref()),
            )
            .field("content_len", &optional_str_len(self.content.as_deref()))
            .field(
                "default_tags_count",
                &self.default_tags.as_ref().map(Vec::len),
            )
            .field(
                "default_tag_lens",
                &self
                    .default_tags
                    .as_ref()
                    .map(|tags| tags.iter().map(|tag| str_len(tag)).collect::<Vec<_>>()),
            )
            .field(
                "collection_id_update_present",
                &self.collection_id.as_ref().map(|value| value.is_some()),
            )
            .finish()
    }
}

fn optional_str_len(value: Option<&str>) -> Option<usize> {
    value.map(str_len)
}

fn str_len(value: &str) -> usize {
    value.chars().count()
}

fn json_debug_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn json_serialized_len(value: &JsonValue) -> usize {
    serde_json::to_string(value).map_or(0, |serialized| str_len(&serialized))
}

fn path_display_len(path: &std::path::Path) -> usize {
    str_len(&path.display().to_string())
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

    /// Synchronize an archive schema with the current public schema.
    ///
    /// Detects tables that exist in public but are missing from the archive
    /// (due to new migrations since the archive was created) and creates them.
    /// Called automatically when an archive is accessed and its schema_version
    /// is outdated.
    async fn sync_archive_schema(&self, name: &str) -> Result<()>;

    /// Clone an existing archive to a new archive.
    ///
    /// Creates a new archive with the same schema structure as the source,
    /// then copies all data from the source archive's tables into the new one.
    ///
    /// # Arguments
    /// * `source_name` - Name of the archive to clone from
    /// * `new_name` - Name for the cloned archive
    /// * `description` - Optional description for the new archive
    async fn clone_archive_schema(
        &self,
        source_name: &str,
        new_name: &str,
        description: Option<&str>,
    ) -> Result<crate::ArchiveInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::path::PathBuf;

    fn assert_debug_excludes(debug: &str, secrets: &[&str]) {
        for secret in secrets {
            assert!(
                !debug.contains(secret),
                "debug output leaked secret `{secret}`: {debug}"
            );
        }
    }

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
            title: None,
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

    #[test]
    fn trait_request_and_extraction_debug_redacts_content_metadata_and_paths() {
        let note_req = CreateNoteRequest {
            content: "秘密 note content includes owner@example.internal and sk-live-secret"
                .to_string(),
            format: "markdown-秘密-format".to_string(),
            source: "https://source.example.test/import?token=秘密".to_string(),
            collection_id: Some(Uuid::new_v4()),
            tags: Some(vec![
                "秘密-tag-owner@example.internal".to_string(),
                "postgres://user:秘密@db.internal/fortemi".to_string(),
            ]),
            metadata: Some(json!({
                "provider_url": "https://provider.example.test/v1?token=秘密",
                "api_key": "sk-live-secret"
            })),
            document_type_id: Some(Uuid::new_v4()),
            title: Some("秘密 roadmap title".to_string()),
        };
        let note_debug = format!("{note_req:?}");
        assert!(note_debug.contains("CreateNoteRequest"));
        assert!(note_debug.contains("content_len: 66"));
        assert!(note_debug.contains("format_len: 18"));
        assert!(note_debug.contains("source_len: 43"));
        assert!(note_debug.contains("tag_lens: Some([29, 38])"));
        assert!(note_debug.contains("title_len: Some(16)"));
        assert!(note_debug.contains("content_len"));
        assert!(note_debug.contains("metadata_class"));
        assert_debug_excludes(
            &note_debug,
            &[
                "秘密 note content",
                "owner@example.internal",
                "sk-live-secret",
                "markdown-秘密-format",
                "https://source.example.test/import?token=秘密",
                "秘密-tag-owner@example.internal",
                "postgres://user:秘密@db.internal/fortemi",
                "https://provider.example.test/v1?token=秘密",
                "秘密 roadmap title",
            ],
        );

        let list_req = ListNotesRequest {
            sort_by: Some("秘密-sort-field".to_string()),
            sort_order: Some("desc-秘密".to_string()),
            filter: Some("filter-owner@example.internal".to_string()),
            limit: Some(25),
            offset: Some(5),
            collection_id: Some(Uuid::new_v4()),
            tags: Some(vec!["sk-list-秘密".to_string()]),
            created_after: Some(Utc::now()),
            created_before: None,
            updated_after: None,
            updated_before: None,
        };
        let list_debug = format!("{list_req:?}");
        assert!(list_debug.contains("ListNotesRequest"));
        assert!(list_debug.contains("tags_count"));
        assert!(list_debug.contains("sort_by_len: Some(13)"));
        assert!(list_debug.contains("sort_order_len: Some(7)"));
        assert!(list_debug.contains("filter_len: Some(29)"));
        assert!(list_debug.contains("tag_lens: Some([10])"));
        assert_debug_excludes(
            &list_debug,
            &[
                "秘密-sort-field",
                "desc-秘密",
                "filter-owner@example.internal",
                "sk-list-秘密",
            ],
        );

        let status_req = UpdateNoteStatusRequest {
            starred: Some(true),
            archived: Some(false),
            metadata: Some(json!({
                "path": "/srv/fortemi/private/status.json",
                "token": "sk-status-secret"
            })),
        };
        let status_debug = format!("{status_req:?}");
        assert!(status_debug.contains("UpdateNoteStatusRequest"));
        assert!(status_debug.contains("metadata_serialized_len"));
        assert_debug_excludes(
            &status_debug,
            &["/srv/fortemi/private/status.json", "sk-status-secret"],
        );

        let search_query = SearchQuery {
            query: "探す private@example.internal with sk-search-secret".to_string(),
            mode: SearchMode::Hybrid,
            limit: Some(10),
            offset: Some(0),
            collection_id: Some(Uuid::new_v4()),
            tags: vec!["秘密-search-tag".to_string()],
            include_archived: true,
        };
        let search_debug = format!("{search_query:?}");
        assert!(search_debug.contains("SearchQuery"));
        assert!(search_debug.contains("query_len: 49"));
        assert!(search_debug.contains("tag_lens: [13]"));
        assert!(search_debug.contains("query_len"));
        assert_debug_excludes(
            &search_debug,
            &[
                "private@example.internal",
                "sk-search-secret",
                "秘密-search-tag",
            ],
        );

        let derived_file = DerivedFile {
            filename: "秘密-keyframe-owner@example.internal.jpg".to_string(),
            content_type: "image/秘密-jpeg".to_string(),
            data: b"binary-secret-sk-derived".to_vec(),
            derivation_type: "秘密-keyframe".to_string(),
            ai_description: Some("Generated description mentions sk-derived-秘密".to_string()),
            metadata: Some(json!({
                "source": "https://metadata.example.test/?token=秘密"
            })),
            source_path: Some(PathBuf::from("/tmp/fortemi/private/秘密-keyframe.jpg")),
        };
        let derived_debug = format!("{derived_file:?}");
        assert!(derived_debug.contains("DerivedFile"));
        assert!(derived_debug.contains("filename_len: 38"));
        assert!(derived_debug.contains("content_type_len: 13"));
        assert!(derived_debug.contains("derivation_type_len: 11"));
        assert!(derived_debug.contains("ai_description_len: Some(44)"));
        assert!(derived_debug.contains("source_path_len: Some(36)"));
        assert!(derived_debug.contains("filename_len"));
        assert_debug_excludes(
            &derived_debug,
            &[
                "秘密-keyframe-owner@example.internal.jpg",
                "image/秘密-jpeg",
                "binary-secret-sk-derived",
                "秘密-keyframe",
                "sk-derived-秘密",
                "https://metadata.example.test/?token=秘密",
                "/tmp/fortemi/private/秘密-keyframe.jpg",
            ],
        );

        let extraction = ExtractionResult {
            extracted_text: Some(
                "Extracted text includes customer@example.internal and sk-extract-秘密".to_string(),
            ),
            metadata: json!({
                "filename": "/srv/customer/private.pdf",
                "api_key": "sk-extract-秘密"
            }),
            ai_description: Some("AI description includes private 診断".to_string()),
            preview_data: Some(b"preview-secret".to_vec()),
            derived_files: vec![derived_file],
        };
        let extraction_debug = format!("{extraction:?}");
        assert!(extraction_debug.contains("ExtractionResult"));
        assert!(extraction_debug.contains("extracted_text_len: Some(67)"));
        assert!(extraction_debug.contains("ai_description_len: Some(34)"));
        assert!(extraction_debug.contains("derived_files_count"));
        assert_debug_excludes(
            &extraction_debug,
            &[
                "Extracted text",
                "customer@example.internal",
                "sk-extract-秘密",
                "/srv/customer/private.pdf",
                "private 診断",
                "preview-secret",
                "秘密-keyframe-owner@example.internal.jpg",
            ],
        );

        let ai_metadata = AiMetadata {
            categories: vec!["秘密-category@example.internal".to_string()],
            topics: vec!["https://topic.example.test/?token=秘密".to_string()],
            keywords: vec!["sk-keyword-秘密".to_string()],
            entities: json!({
                "person": ["Private Person"],
                "url": "https://entity.example.test/?token=秘密"
            }),
            summary: Some("Summary includes private@example.internal 秘密".to_string()),
        };
        let ai_debug = format!("{ai_metadata:?}");
        assert!(ai_debug.contains("AiMetadata"));
        assert!(ai_debug.contains("category_lens: [28]"));
        assert!(ai_debug.contains("topic_lens: [36]"));
        assert!(ai_debug.contains("keyword_lens: [13]"));
        assert!(ai_debug.contains("summary_len: Some(44)"));
        assert!(ai_debug.contains("entities_class"));
        assert_debug_excludes(
            &ai_debug,
            &[
                "秘密-category@example.internal",
                "https://topic.example.test/?token=秘密",
                "sk-keyword-秘密",
                "Private Person",
                "https://entity.example.test/?token=秘密",
                "private@example.internal",
            ],
        );

        let create_template = CreateTemplateRequest {
            name: "秘密-template-name".to_string(),
            description: Some("Template description with secret@example.internal 秘密".to_string()),
            content: "Template body with sk-template-秘密".to_string(),
            format: Some("秘密-template-format".to_string()),
            default_tags: Some(vec!["秘密-default-tag".to_string()]),
            collection_id: Some(Uuid::new_v4()),
        };
        let template_debug = format!("{create_template:?}");
        assert!(template_debug.contains("CreateTemplateRequest"));
        assert!(template_debug.contains("name_len: 16"));
        assert!(template_debug.contains("description_len: Some(52)"));
        assert!(template_debug.contains("content_len: 33"));
        assert!(template_debug.contains("format_len: Some(18)"));
        assert!(template_debug.contains("default_tag_lens: Some([14])"));
        assert!(template_debug.contains("content_len"));
        assert_debug_excludes(
            &template_debug,
            &[
                "秘密-template-name",
                "secret@example.internal",
                "sk-template-秘密",
                "秘密-template-format",
                "秘密-default-tag",
            ],
        );

        let update_template = UpdateTemplateRequest {
            name: Some("updated-秘密-template".to_string()),
            description: Some("Updated description with private@example.internal 秘密".to_string()),
            content: Some("Updated body with sk-updated-template-秘密".to_string()),
            default_tags: Some(vec!["updated-秘密-tag".to_string()]),
            collection_id: Some(Some(Uuid::new_v4())),
        };
        let update_template_debug = format!("{update_template:?}");
        assert!(update_template_debug.contains("UpdateTemplateRequest"));
        assert!(update_template_debug.contains("name_len: Some(19)"));
        assert!(update_template_debug.contains("description_len: Some(52)"));
        assert!(update_template_debug.contains("content_len: Some(40)"));
        assert!(update_template_debug.contains("default_tag_lens: Some([14])"));
        assert!(update_template_debug.contains("collection_id_update_present"));
        assert_debug_excludes(
            &update_template_debug,
            &[
                "updated-秘密-template",
                "private@example.internal",
                "sk-updated-template-秘密",
                "updated-秘密-tag",
            ],
        );
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
            cost_tier: None,
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
            cost_tier: None,
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
            cost_tier: None,
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
            cost_tier: None,
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
            cost_tier: None,
        };

        notifier.on_job_failed(&job).await;
    }

    // =============================================================================
    // Type Tests
    // =============================================================================

    #[test]
    fn test_search_query_debug_format() {
        let query = SearchQuery {
            query: "test秘密".to_string(),
            mode: SearchMode::Hybrid,
            limit: Some(10),
            offset: Some(0),
            collection_id: None,
            tags: vec![],
            include_archived: false,
        };

        let debug_str = format!("{:?}", query);
        assert!(debug_str.contains("SearchQuery"));
        assert!(debug_str.contains("query_len: 6"));
        assert!(!debug_str.contains("test秘密"));
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
            title: None,
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
    fn list_response_debug_redacts_nested_note_and_attachment_content() {
        let note_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa").unwrap();
        let attachment_id = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb").unwrap();
        let summary = NoteSummary {
            id: note_id,
            title: "Payroll customer@example.com sk-live-title".to_string(),
            snippet: "Snippet with postgres://user:secret@db.internal/app".to_string(),
            embedding_status: Some(EmbeddingStatus::Ready),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            starred: true,
            archived: false,
            tags: vec![
                "customer@example.com".to_string(),
                "sk-live-tag".to_string(),
            ],
            has_revision: true,
            metadata: json!({
                "path": "/srv/private/note.md",
                "token": "sk-live-metadata"
            }),
            document_type_id: Some(
                Uuid::parse_str("cccccccc-cccc-4ccc-cccc-cccccccccccc").unwrap(),
            ),
            document_type_name: Some("Private Contract customer@example.com".to_string()),
        };
        let attachment = GlobalAttachmentSummary {
            id: attachment_id,
            note_id,
            note_title: Some("Payroll customer@example.com sk-live-title".to_string()),
            filename: "private/customer@example.com/sk-live.pdf".to_string(),
            content_type: "application/x-private-token".to_string(),
            size_bytes: 42,
            status: AttachmentStatus::Uploaded,
            document_type_name: Some("Private Contract customer@example.com".to_string()),
            detected_document_type_name: Some("Detected sk-live contract".to_string()),
            detection_confidence: Some(0.99),
            has_preview: true,
            is_canonical_content: false,
            created_at: Utc::now(),
        };
        let notes = ListNotesResponse {
            notes: vec![summary],
            total: 1,
        };
        let attachments = ListGlobalAttachmentsResponse {
            attachments: vec![attachment],
            total: 1,
        };

        let rendered = format!("{notes:?} {attachments:?}");

        assert!(rendered.contains("ListNotesResponse"));
        assert!(rendered.contains("notes_count"));
        assert!(rendered.contains("ListGlobalAttachmentsResponse"));
        assert!(rendered.contains("attachments_count"));
        assert!(rendered.contains("total"));

        let leaked_values = vec![
            note_id.to_string(),
            attachment_id.to_string(),
            "Payroll".to_string(),
            "customer@example.com".to_string(),
            "sk-live".to_string(),
            "postgres://user:secret".to_string(),
            "db.internal".to_string(),
            "/srv/private".to_string(),
            "Private Contract".to_string(),
            "private/customer".to_string(),
            "application/x-private-token".to_string(),
            "Detected".to_string(),
        ];
        for raw in leaked_values {
            assert!(!rendered.contains(&raw), "Debug leaked {raw}: {rendered}");
        }
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
