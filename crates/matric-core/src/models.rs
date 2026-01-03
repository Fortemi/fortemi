//! Core data models for matric-memory.
//!
//! These types are shared across all matric-memory crates and represent
//! the core domain entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// =============================================================================
// NOTE TYPES
// =============================================================================

/// Metadata for a note (without content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub id: Uuid,
    pub collection_id: Option<Uuid>,
    pub format: String,
    pub source: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub starred: bool,
    pub archived: bool,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub metadata: JsonValue,
}

/// Original immutable content of a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteOriginal {
    pub content: String,
    pub hash: String,
    pub user_created_at: Option<DateTime<Utc>>,
    pub user_last_edited_at: Option<DateTime<Utc>>,
}

/// Current revised/working version of a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRevised {
    pub content: String,
    pub last_revision_id: Option<Uuid>,
    pub ai_metadata: Option<JsonValue>,
    pub ai_generated_at: Option<DateTime<Utc>>,
    pub user_last_edited_at: Option<DateTime<Utc>>,
    pub is_user_edited: bool,
    pub generation_count: i32,
    pub model: Option<String>,
}

/// Complete note with all components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteFull {
    pub note: NoteMeta,
    pub original: NoteOriginal,
    pub revised: NoteRevised,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
}

/// Summary view of a note for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: Uuid,
    pub title: String,
    pub snippet: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub starred: bool,
    pub archived: bool,
    pub tags: Vec<String>,
    pub has_revision: bool,
    pub metadata: JsonValue,
}

// =============================================================================
// LINK TYPES
// =============================================================================

/// Link between notes or to external URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: Uuid,
    pub from_note_id: Uuid,
    pub to_note_id: Option<Uuid>,
    pub to_url: Option<String>,
    pub kind: String,
    pub score: f32,
    pub created_at_utc: DateTime<Utc>,
    pub snippet: Option<String>,
    pub metadata: Option<JsonValue>,
}

// =============================================================================
// SEARCH TYPES
// =============================================================================

/// A search result hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub note_id: Uuid,
    pub score: f32,
    pub snippet: Option<String>,
}

/// Search results response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub notes: Vec<SearchHit>,
}

/// Semantic search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticResponse {
    pub similar: Vec<SearchHit>,
}

/// Search mode for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Full-text search only
    Fts,
    /// Vector/semantic search only
    Vector,
    /// Hybrid: combines FTS and vector with RRF
    Hybrid,
}

impl Default for SearchMode {
    fn default() -> Self {
        SearchMode::Hybrid
    }
}

// =============================================================================
// EMBEDDING TYPES
// =============================================================================

/// Embedding vector type (re-exported from pgvector).
pub use pgvector::Vector;

/// An embedding record linking text to its vector representation.
#[derive(Debug, Clone)]
pub struct Embedding {
    pub id: Uuid,
    pub note_id: Uuid,
    pub chunk_index: i32,
    pub text: String,
    pub vector: Vector,
    pub model: String,
}

/// Configuration for embedding generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Maximum characters per chunk
    pub chunk_size: usize,
    /// Overlap between chunks (characters)
    pub chunk_overlap: usize,
    /// Embedding model name
    pub model: String,
    /// Expected vector dimension
    pub dimension: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1500,
            chunk_overlap: 200,
            model: "nomic-embed-text".to_string(),
            dimension: 768,
        }
    }
}

// =============================================================================
// JOB TYPES
// =============================================================================

/// Status of a job in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Type of job to process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Generate AI revision of content
    AiRevision,
    /// Generate embeddings for content
    Embedding,
    /// Auto-detect and create links
    Linking,
    /// Update context/metadata
    ContextUpdate,
    /// Generate title from content
    TitleGeneration,
}

impl JobType {
    /// Default priority for this job type (higher = more urgent)
    pub fn default_priority(&self) -> i32 {
        match self {
            JobType::AiRevision => 8,
            JobType::Embedding => 5,
            JobType::Linking => 3,
            JobType::TitleGeneration => 2,
            JobType::ContextUpdate => 1,
        }
    }
}

/// A job in the processing queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub note_id: Option<Uuid>,
    pub job_type: JobType,
    pub status: JobStatus,
    pub priority: i32,
    pub payload: Option<JsonValue>,
    pub result: Option<JsonValue>,
    pub error_message: Option<String>,
    pub progress_percent: i32,
    pub progress_message: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// =============================================================================
// COLLECTION & TAG TYPES
// =============================================================================

/// A collection of notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at_utc: DateTime<Utc>,
}

/// A tag definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub created_at_utc: DateTime<Utc>,
}

// =============================================================================
// USER METADATA TYPES
// =============================================================================

/// Custom user-defined label on a note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMetadataLabel {
    pub id: Uuid,
    pub note_id: Uuid,
    pub label: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// User configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    pub key: String,
    pub value: JsonValue,
    pub updated_at: DateTime<Utc>,
}

// =============================================================================
// PROVENANCE TYPES
// =============================================================================

/// Edge in the provenance graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEdge {
    pub id: Uuid,
    pub revision_id: Uuid,
    pub source_note_id: Option<Uuid>,
    pub source_url: Option<String>,
    pub relation: String,
    pub created_at_utc: DateTime<Utc>,
}

/// Node in the revision tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionNode {
    pub id: Uuid,
    pub parent_revision_id: Option<Uuid>,
    pub created_at_utc: DateTime<Utc>,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_note_meta_serialization() {
        let note = NoteMeta {
            id: Uuid::new_v4(),
            collection_id: None,
            format: "markdown".to_string(),
            source: "test".to_string(),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            starred: false,
            archived: false,
            last_accessed_at: None,
            title: None,
            metadata: json!({}),
        };

        let serialized = serde_json::to_string(&note).unwrap();
        let deserialized: NoteMeta = serde_json::from_str(&serialized).unwrap();
        assert_eq!(note.id, deserialized.id);
    }

    #[test]
    fn test_job_type_priority() {
        assert!(JobType::AiRevision.default_priority() > JobType::Embedding.default_priority());
        assert!(JobType::Embedding.default_priority() > JobType::Linking.default_priority());
    }

    #[test]
    fn test_search_mode_default() {
        assert_eq!(SearchMode::default(), SearchMode::Hybrid);
    }
}
