//! Core data models for matric-memory.
//!
//! These types are shared across all matric-memory crates and represent
//! the core domain entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

// =============================================================================
// NOTE TYPES
// =============================================================================

/// Metadata for a note (without content).
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// Number of times this note has been accessed (read)
    #[serde(default)]
    pub access_count: i32,
    pub title: Option<String>,
    pub metadata: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_metadata: Option<JsonValue>,
    /// Associated document type for content-aware chunking and embedding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type_id: Option<Uuid>,
}

impl fmt::Debug for NoteMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteMeta")
            .field("id_set", &(!self.id.is_nil()))
            .field("collection_id_set", &self.collection_id.is_some())
            .field("format_len", &self.format.len())
            .field("source_len", &self.source.len())
            .field("created_at_utc", &self.created_at_utc)
            .field("updated_at_utc", &self.updated_at_utc)
            .field("starred", &self.starred)
            .field("archived", &self.archived)
            .field("last_accessed_at_set", &self.last_accessed_at.is_some())
            .field("access_count", &self.access_count)
            .field("title_len", &self.title.as_ref().map(String::len))
            .field("metadata_class", &json_value_class(&self.metadata))
            .field("metadata_len", &json_serialized_len(&self.metadata))
            .field(
                "chunk_metadata_class",
                &self.chunk_metadata.as_ref().map(json_value_class),
            )
            .field(
                "chunk_metadata_len",
                &self.chunk_metadata.as_ref().map(json_serialized_len),
            )
            .field("document_type_id_set", &self.document_type_id.is_some())
            .finish()
    }
}

/// Original immutable content of a note.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteOriginal {
    pub content: String,
    pub hash: String,
    pub user_created_at: Option<DateTime<Utc>>,
    pub user_last_edited_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for NoteOriginal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteOriginal")
            .field("content_len", &self.content.len())
            .field("hash_len", &self.hash.len())
            .field("user_created_at_set", &self.user_created_at.is_some())
            .field(
                "user_last_edited_at_set",
                &self.user_last_edited_at.is_some(),
            )
            .finish()
    }
}

/// Current revised/working version of a note.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for NoteRevised {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteRevised")
            .field("content_len", &self.content.len())
            .field("last_revision_id_set", &self.last_revision_id.is_some())
            .field(
                "ai_metadata_class",
                &self.ai_metadata.as_ref().map(json_value_class),
            )
            .field(
                "ai_metadata_len",
                &self.ai_metadata.as_ref().map(json_serialized_len),
            )
            .field("ai_generated_at_set", &self.ai_generated_at.is_some())
            .field(
                "user_last_edited_at_set",
                &self.user_last_edited_at.is_some(),
            )
            .field("is_user_edited", &self.is_user_edited)
            .field("generation_count", &self.generation_count)
            .field("model_len", &self.model.as_ref().map(String::len))
            .finish()
    }
}

/// Complete note with all components.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteFull {
    pub note: NoteMeta,
    pub original: NoteOriginal,
    pub revised: NoteRevised,
    pub tags: Vec<String>,
    /// SKOS concepts with full metadata (confidence, relevance, source).
    /// This is the rich superset; `tags` contains the flattened string union
    /// of legacy flat tags + SKOS concept notations for search/filter compatibility.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<NoteConceptSummary>,
    pub links: Vec<Link>,
}

impl fmt::Debug for NoteFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteFull")
            .field("note", &self.note)
            .field("original", &self.original)
            .field("revised", &self.revised)
            .field("tags_count", &self.tags.len())
            .field("concepts_count", &self.concepts.len())
            .field("links_count", &self.links.len())
            .finish()
    }
}

/// Lightweight SKOS concept summary for note responses.
/// Preserves the richness of the SKOS tagging data while being
/// suitable for inclusion in note detail responses.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteConceptSummary {
    pub concept_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pref_label: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub relevance_score: f32,
    pub is_primary: bool,
}

impl fmt::Debug for NoteConceptSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteConceptSummary")
            .field("concept_id_set", &true)
            .field("notation_len", &self.notation.as_ref().map(String::len))
            .field("pref_label_len", &self.pref_label.as_ref().map(String::len))
            .field("source_len", &self.source.len())
            .field("confidence", &self.confidence)
            .field("relevance_score", &self.relevance_score)
            .field("is_primary", &self.is_primary)
            .finish()
    }
}

/// A revision version entry from note_revision table (AI-enhanced content track).
#[derive(Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct RevisionVersion {
    pub id: Uuid,
    pub note_id: Uuid,
    pub revision_number: i32,
    pub content: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub revision_type: String,
    pub summary: Option<String>,
    pub rationale: Option<String>,
    pub created_at_utc: DateTime<Utc>,
    pub model: Option<String>,
    pub is_user_edited: bool,
}

impl fmt::Debug for RevisionVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevisionVersion")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("revision_number", &self.revision_number)
            .field("content_len", &self.content.len())
            .field("revision_type_len", &self.revision_type.len())
            .field("summary_len", &self.summary.as_ref().map(String::len))
            .field("rationale_len", &self.rationale.as_ref().map(String::len))
            .field("created_at_utc", &self.created_at_utc)
            .field("model_len", &self.model.as_ref().map(String::len))
            .field("is_user_edited", &self.is_user_edited)
            .finish()
    }
}

/// Summary view of a note for listing.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteSummary {
    pub id: Uuid,
    pub title: String,
    pub snippet: String,
    /// Embedding status for this note
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_status: Option<EmbeddingStatus>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub starred: bool,
    pub archived: bool,
    pub tags: Vec<String>,
    pub has_revision: bool,
    pub metadata: JsonValue,
    /// Associated document type ID for content-aware processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type_id: Option<Uuid>,
    /// Human-readable document type name (convenience field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type_name: Option<String>,
}

impl fmt::Debug for NoteSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteSummary")
            .field("id_set", &true)
            .field("title_len", &self.title.len())
            .field("snippet_len", &self.snippet.len())
            .field("embedding_status", &self.embedding_status)
            .field("created_at_utc", &self.created_at_utc)
            .field("updated_at_utc", &self.updated_at_utc)
            .field("starred", &self.starred)
            .field("archived", &self.archived)
            .field("tags_count", &self.tags.len())
            .field("has_revision", &self.has_revision)
            .field("metadata_class", &json_value_class(&self.metadata))
            .field("metadata_len", &json_serialized_len(&self.metadata))
            .field("document_type_id_set", &self.document_type_id.is_some())
            .field(
                "document_type_name_len",
                &self.document_type_name.as_ref().map(String::len),
            )
            .finish()
    }
}

// =============================================================================
// LINK TYPES
// =============================================================================

/// Link between notes or to external URLs.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("id_set", &true)
            .field("from_note_id_set", &true)
            .field("to_note_id_set", &self.to_note_id.is_some())
            .field("to_url_len", &self.to_url.as_ref().map(String::len))
            .field("kind_len", &self.kind.len())
            .field("score", &self.score)
            .field("created_at_utc", &self.created_at_utc)
            .field("snippet_len", &self.snippet.as_ref().map(String::len))
            .field(
                "metadata_class",
                &self.metadata.as_ref().map(json_value_class),
            )
            .field(
                "metadata_len",
                &self.metadata.as_ref().map(json_serialized_len),
            )
            .finish()
    }
}

// =============================================================================
// SEARCH TYPES
// =============================================================================

/// A search result hit.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchHit {
    pub note_id: Uuid,
    pub score: f32,
    pub snippet: Option<String>,
    /// Note title (generated or first line of content)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Note tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Embedding status for this note
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_status: Option<EmbeddingStatus>,
}

impl fmt::Debug for SearchHit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchHit")
            .field("note_id_set", &true)
            .field("score", &self.score)
            .field("snippet_len", &self.snippet.as_ref().map(String::len))
            .field("title_len", &self.title.as_ref().map(String::len))
            .field("tags_count", &self.tags.len())
            .field("embedding_status", &self.embedding_status)
            .finish()
    }
}

/// Search results response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchResponse {
    pub notes: Vec<SearchHit>,
    /// Whether semantic search was available for this query
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_available: Option<bool>,
    /// Warnings about search degradation or issues
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl fmt::Debug for SearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchResponse")
            .field("notes_count", &self.notes.len())
            .field("semantic_available", &self.semantic_available)
            .field("warnings_count", &self.warnings.len())
            .finish()
    }
}

/// Semantic search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SemanticResponse {
    pub similar: Vec<SearchHit>,
}

impl fmt::Debug for SemanticResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticResponse")
            .field("similar_count", &self.similar.len())
            .finish()
    }
}

/// Search mode for queries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Full-text search only
    Fts,
    /// Vector/semantic search only
    Vector,
    /// Hybrid: combines FTS and vector with RRF
    #[default]
    Hybrid,
}

/// Status of embeddings for a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingStatus {
    /// Embeddings are generated and ready
    Ready,
    /// Embedding generation is pending/queued
    Pending,
    /// Embedding generation failed
    Failed,
    /// No embeddings exist for this note
    None,
}

impl std::fmt::Display for EmbeddingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::Pending => write!(f, "pending"),
            Self::Failed => write!(f, "failed"),
            Self::None => write!(f, "none"),
        }
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
            chunk_size: crate::defaults::CHUNK_SIZE,
            chunk_overlap: crate::defaults::CHUNK_OVERLAP,
            model: crate::defaults::EMBED_MODEL.to_string(),
            dimension: crate::defaults::EMBED_DIMENSION,
        }
    }
}

// =============================================================================
// EMBEDDING SET TYPES
// =============================================================================

/// Type of embedding set: filter (shares embeddings) vs full (own embeddings).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingSetType {
    /// Filter set: Uses shared embeddings from default, filters by membership.
    /// No storage overhead, fast to create. Current behavior.
    #[default]
    Filter,
    /// Full set: Stores its own embeddings with dedicated model/config.
    /// Independent embeddings, domain-specific models possible.
    Full,
}

impl std::fmt::Display for EmbeddingSetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Filter => write!(f, "filter"),
            Self::Full => write!(f, "full"),
        }
    }
}

impl std::str::FromStr for EmbeddingSetType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "filter" => Ok(Self::Filter),
            "full" => Ok(Self::Full),
            _ => Err(format!("Invalid embedding set type: {}", s)),
        }
    }
}

/// Membership mode for embedding sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingSetMode {
    /// Automatically include notes matching criteria
    #[default]
    Auto,
    /// Only explicitly added notes
    Manual,
    /// Auto criteria + manual additions/exclusions
    Mixed,
}

impl std::fmt::Display for EmbeddingSetMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Manual => write!(f, "manual"),
            Self::Mixed => write!(f, "mixed"),
        }
    }
}

impl std::str::FromStr for EmbeddingSetMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            "mixed" => Ok(Self::Mixed),
            _ => Err(format!("Invalid embedding set mode: {}", s)),
        }
    }
}

/// Index build status for embedding sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingIndexStatus {
    /// No documents or embeddings in the set
    Empty,
    /// Needs initial build
    #[default]
    Pending,
    /// Currently building
    Building,
    /// Index is current
    Ready,
    /// Index needs rebuild (new members)
    Stale,
    /// No index (for very small sets)
    Disabled,
}

impl std::fmt::Display for EmbeddingIndexStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty"),
            Self::Pending => write!(f, "pending"),
            Self::Building => write!(f, "building"),
            Self::Ready => write!(f, "ready"),
            Self::Stale => write!(f, "stale"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

impl std::str::FromStr for EmbeddingIndexStatus {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "empty" => Ok(Self::Empty),
            "pending" => Ok(Self::Pending),
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "stale" => Ok(Self::Stale),
            "disabled" => Ok(Self::Disabled),
            _ => Err(format!("Invalid embedding index status: {}", s)),
        }
    }
}

/// Criteria for automatic embedding set membership.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSetCriteria {
    /// Include all notes (default set behavior)
    #[serde(default)]
    pub include_all: bool,

    /// Include notes with any of these tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Include notes in any of these collections
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<Uuid>,

    /// Include notes matching this FTS query
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_query: Option<String>,

    /// Include notes created after this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_after: Option<DateTime<Utc>>,

    /// Include notes created before this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_before: Option<DateTime<Utc>>,

    /// Exclude archived notes (default true)
    #[serde(default = "default_true")]
    pub exclude_archived: bool,
}

fn default_true() -> bool {
    true
}

/// Rules for automatic embedding generation in Full sets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AutoEmbedRules {
    /// Trigger embedding on note creation
    #[serde(default)]
    pub on_create: bool,

    /// Trigger embedding on note update
    #[serde(default)]
    pub on_update: bool,

    /// Re-embed if content changed more than this percentage (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_threshold_percent: Option<f32>,

    /// Maximum age in seconds before re-embedding (staleness threshold)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_embedding_age_secs: Option<i64>,

    /// Priority relative to other sets (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// Batch size for bulk operations
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Rate limit (embeddings per minute, None = no limit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<u32>,
}

fn default_priority() -> i32 {
    crate::defaults::AUTO_EMBED_PRIORITY
}

fn default_batch_size() -> usize {
    crate::defaults::AUTO_EMBED_BATCH_SIZE
}

/// Agent-provided metadata for embedding set discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSetAgentMetadata {
    /// Agent that created this set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_agent: Option<String>,

    /// Why this set was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,

    /// Performance notes for other agents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance_notes: Option<String>,

    /// Related embedding sets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_sets: Vec<String>,

    /// Example queries this set is good for
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_queries: Vec<String>,
}

// =============================================================================
// DOCUMENT COMPOSITION (#485)
// =============================================================================

/// Strategy for including tags in embedding text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TagStrategy {
    /// No tags in embedding (optimal default for graph quality).
    #[default]
    None,
    /// Include all tags.
    All,
    /// Include only tags matching specific SKOS scheme IDs.
    Schemes(Vec<Uuid>),
    /// Include only specific tag names.
    Specific(Vec<String>),
}

/// Controls what note properties are assembled into the embedding text.
///
/// The document composition is the single most important characteristic of an
/// embedding set — it entirely determines the semantic geometry of the vector space.
/// Different compositions produce fundamentally different clustering behaviors.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DocumentComposition {
    /// Include note title in embedding text.
    #[serde(default = "default_true")]
    pub include_title: bool,

    /// Include note body content in embedding text.
    #[serde(default = "default_true")]
    pub include_content: bool,

    /// Tag inclusion strategy for embedding text.
    #[serde(default)]
    pub tag_strategy: TagStrategy,

    /// Include SKOS concept labels (with optional TF-IDF filtering).
    #[serde(default)]
    pub include_concepts: bool,

    /// Max document frequency for concept TF-IDF filtering (only when include_concepts=true).
    /// Concepts appearing in more than this fraction of notes are excluded.
    #[serde(default = "default_concept_max_doc_freq")]
    pub concept_max_doc_freq: f64,

    /// Instruction prefix for the embedding model (e.g., "clustering:", "search_document:").
    #[serde(default = "default_embed_prefix")]
    pub instruction_prefix: String,
}

fn default_concept_max_doc_freq() -> f64 {
    crate::defaults::EMBED_CONCEPT_MAX_DOC_FREQ
}

fn default_embed_prefix() -> String {
    crate::defaults::EMBED_INSTRUCTION_PREFIX.to_string()
}

impl Default for DocumentComposition {
    fn default() -> Self {
        Self {
            include_title: true,
            include_content: true,
            tag_strategy: TagStrategy::None,
            include_concepts: false,
            concept_max_doc_freq: crate::defaults::EMBED_CONCEPT_MAX_DOC_FREQ,
            instruction_prefix: crate::defaults::EMBED_INSTRUCTION_PREFIX.to_string(),
        }
    }
}

impl DocumentComposition {
    /// Build the embedding text from note properties according to this composition.
    pub fn build_text(&self, title: &str, content: &str, concept_labels: &[String]) -> String {
        let mut parts = Vec::new();
        if self.include_title && !title.is_empty() {
            parts.push(title.to_string());
        }
        match &self.tag_strategy {
            TagStrategy::None => {}
            TagStrategy::All => {
                if !concept_labels.is_empty() {
                    parts.push(format!("Tags: {}", concept_labels.join(", ")));
                }
            }
            TagStrategy::Schemes(_) | TagStrategy::Specific(_) => {
                // Filtering by scheme/specific is handled upstream when fetching labels.
                // If labels were pre-filtered, include whatever was passed.
                if !concept_labels.is_empty() {
                    parts.push(format!("Tags: {}", concept_labels.join(", ")));
                }
            }
        }
        if self.include_concepts
            && !concept_labels.is_empty()
            && self.tag_strategy == TagStrategy::None
        {
            // include_concepts without tag_strategy means include concepts as metadata
            parts.push(format!("Concepts: {}", concept_labels.join(", ")));
        }
        if self.include_content {
            parts.push(content.to_string());
        }
        let body = parts.join("\n\n");
        if self.instruction_prefix.is_empty() {
            body
        } else {
            format!("{}{}", self.instruction_prefix, body)
        }
    }
}

/// Database-stored embedding configuration profile.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingConfigProfile {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub model: String,
    pub dimension: i32,
    pub chunk_size: i32,
    pub chunk_overlap: i32,
    pub hnsw_m: Option<i32>,
    pub hnsw_ef_construction: Option<i32>,
    pub ivfflat_lists: Option<i32>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // MRL (Matryoshka Representation Learning) support
    /// Whether this model supports Matryoshka dimension truncation
    #[serde(default)]
    pub supports_mrl: bool,
    /// Valid truncation dimensions for MRL models (ordered descending)
    /// e.g., [768, 512, 256, 128, 64]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matryoshka_dims: Option<Vec<i32>>,
    /// Default dimension to use if MRL truncation is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_truncate_dim: Option<i32>,

    // Provider support (Dynamic Embedding Config API - #392)
    /// Embedding generation provider (ollama, openai, voyage, cohere, custom)
    #[serde(default)]
    pub provider: crate::embedding_provider::EmbeddingProvider,
    /// Provider-specific configuration (API key env var, base URL, etc.)
    #[serde(default)]
    pub provider_config: JsonValue,
    /// Content types this config is optimized for (e.g., code, text, multilingual)
    #[serde(default)]
    pub content_types: Vec<String>,

    // Document composition (#485)
    /// Controls what note properties are assembled into the embedding text.
    /// Empty JSON object `{}` means use `DocumentComposition::default()` (title+content).
    #[serde(default)]
    pub document_composition: DocumentComposition,
}

impl EmbeddingConfigProfile {
    /// Validate that a truncation dimension is valid for this config.
    pub fn validate_truncate_dim(&self, dim: i32) -> Result<(), String> {
        if !self.supports_mrl {
            return Err("Model does not support MRL truncation".into());
        }

        if let Some(valid_dims) = &self.matryoshka_dims {
            if !valid_dims.contains(&dim) {
                return Err(format!(
                    "Invalid truncation dimension {}. Valid dimensions: {:?}",
                    dim, valid_dims
                ));
            }
        } else {
            return Err("No valid MRL dimensions configured".into());
        }

        Ok(())
    }

    /// Get the effective dimension (truncated or full).
    pub fn effective_dimension(&self, truncate_dim: Option<i32>) -> i32 {
        truncate_dim.unwrap_or(self.dimension)
    }
}

/// An embedding set groups documents for focused semantic search.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSet {
    pub id: Uuid,
    pub name: String,
    pub slug: String,

    // Agent-friendly discovery
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_hints: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    // Set type (filter vs full)
    #[serde(default)]
    pub set_type: EmbeddingSetType,

    // Membership
    pub mode: EmbeddingSetMode,
    pub criteria: EmbeddingSetCriteria,

    // Config reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config_id: Option<Uuid>,

    // MRL truncation (for Full sets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_dim: Option<i32>,

    // Auto-embedding rules (for Full sets)
    #[serde(default)]
    pub auto_embed_rules: AutoEmbedRules,

    // Stats
    pub document_count: i32,
    pub embedding_count: i32,
    pub index_status: EmbeddingIndexStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_size_bytes: Option<i64>,

    // Flags
    pub is_system: bool,
    pub is_active: bool,
    pub auto_refresh: bool,
    #[serde(default = "default_true")]
    pub embeddings_current: bool,

    // Agent metadata
    #[serde(default)]
    pub agent_metadata: EmbeddingSetAgentMetadata,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Summary view of embedding sets for listing/discovery.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSetSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default)]
    pub set_type: EmbeddingSetType,
    pub document_count: i32,
    pub embedding_count: i32,
    pub index_status: EmbeddingIndexStatus,
    pub is_system: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_dim: Option<i32>,
    #[serde(default)]
    pub supports_mrl: bool,
}

/// Request to create a new embedding set.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateEmbeddingSetRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    pub description: Option<String>,
    pub purpose: Option<String>,
    pub usage_hints: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Set type: filter (default, shares embeddings) or full (own embeddings)
    #[serde(default)]
    pub set_type: EmbeddingSetType,
    #[serde(default)]
    pub mode: EmbeddingSetMode,
    #[serde(default)]
    pub criteria: EmbeddingSetCriteria,
    #[serde(default)]
    pub agent_metadata: EmbeddingSetAgentMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config_id: Option<Uuid>,
    /// MRL truncation dimension (only for full sets with MRL-enabled config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_dim: Option<i32>,
    /// Auto-embedding rules (only for full sets)
    #[serde(default)]
    pub auto_embed_rules: AutoEmbedRules,
}

/// Request to update an embedding set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateEmbeddingSetRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_hints: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<EmbeddingSetMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<EmbeddingSetCriteria>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_metadata: Option<EmbeddingSetAgentMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_refresh: Option<bool>,
}

/// Embedding set membership record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSetMember {
    pub embedding_set_id: Uuid,
    pub note_id: Uuid,
    pub membership_type: String,
    pub added_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

/// Request to add members to an embedding set.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddMembersRequest {
    pub note_ids: Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

// =============================================================================
// DOCUMENT TYPE TYPES
// =============================================================================

/// Category of document type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DocumentCategory {
    Prose,
    Code,
    Config,
    Markup,
    Data,
    #[serde(rename = "api-spec")]
    ApiSpec,
    Iac,
    Database,
    Shell,
    Docs,
    Package,
    Observability,
    Legal,
    Communication,
    Research,
    Creative,
    Media,
    Personal,
    /// AI agentic primitives (agents, skills, commands, prompts)
    Agentic,
    Custom,
}

impl std::fmt::Display for DocumentCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prose => write!(f, "prose"),
            Self::Code => write!(f, "code"),
            Self::Config => write!(f, "config"),
            Self::Markup => write!(f, "markup"),
            Self::Data => write!(f, "data"),
            Self::ApiSpec => write!(f, "api-spec"),
            Self::Iac => write!(f, "iac"),
            Self::Database => write!(f, "database"),
            Self::Shell => write!(f, "shell"),
            Self::Docs => write!(f, "docs"),
            Self::Package => write!(f, "package"),
            Self::Observability => write!(f, "observability"),
            Self::Legal => write!(f, "legal"),
            Self::Communication => write!(f, "communication"),
            Self::Research => write!(f, "research"),
            Self::Creative => write!(f, "creative"),
            Self::Media => write!(f, "media"),
            Self::Personal => write!(f, "personal"),
            Self::Agentic => write!(f, "agentic"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl std::str::FromStr for DocumentCategory {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "prose" => Ok(Self::Prose),
            "code" => Ok(Self::Code),
            "config" => Ok(Self::Config),
            "markup" => Ok(Self::Markup),
            "data" => Ok(Self::Data),
            "api_spec" | "apispec" | "api-spec" => Ok(Self::ApiSpec),
            "iac" => Ok(Self::Iac),
            "database" => Ok(Self::Database),
            "shell" => Ok(Self::Shell),
            "docs" => Ok(Self::Docs),
            "package" => Ok(Self::Package),
            "observability" => Ok(Self::Observability),
            "legal" => Ok(Self::Legal),
            "communication" => Ok(Self::Communication),
            "research" => Ok(Self::Research),
            "creative" => Ok(Self::Creative),
            "media" => Ok(Self::Media),
            "personal" => Ok(Self::Personal),
            "agentic" => Ok(Self::Agentic),
            "custom" => Ok(Self::Custom),
            _ => Err(format!("Invalid document category: {}", s)),
        }
    }
}

/// Chunking strategy for document processing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChunkingStrategy {
    /// Split on paragraph/section boundaries (prose)
    #[default]
    Semantic,
    /// Split on AST boundaries via tree-sitter (code)
    Syntactic,
    /// Fixed token count with overlap
    Fixed,
    /// Combine semantic + syntactic
    Hybrid,
    /// Split by document section
    #[serde(rename = "per_section")]
    PerSection,
    /// Split by logical unit (function, class)
    #[serde(rename = "per_unit")]
    PerUnit,
    /// Keep entire document as single chunk
    Whole,
}

impl std::fmt::Display for ChunkingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Semantic => write!(f, "semantic"),
            Self::Syntactic => write!(f, "syntactic"),
            Self::Fixed => write!(f, "fixed"),
            Self::Hybrid => write!(f, "hybrid"),
            Self::PerSection => write!(f, "per_section"),
            Self::PerUnit => write!(f, "per_unit"),
            Self::Whole => write!(f, "whole"),
        }
    }
}

impl std::str::FromStr for ChunkingStrategy {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "semantic" => Ok(Self::Semantic),
            "syntactic" => Ok(Self::Syntactic),
            "fixed" => Ok(Self::Fixed),
            "hybrid" => Ok(Self::Hybrid),
            "per_section" | "persection" => Ok(Self::PerSection),
            "per_unit" | "perunit" => Ok(Self::PerUnit),
            "whole" => Ok(Self::Whole),
            _ => Err(format!("Invalid chunking strategy: {}", s)),
        }
    }
}

/// AI-enhanced document generation metadata (Issue #422).
///
/// Provides structured prompts, required sections, context requirements,
/// and agent hints to guide AI agents in generating high-quality content
/// for specific document types.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, utoipa::ToSchema)]
pub struct AgenticConfig {
    /// Generation prompt to guide AI document creation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_prompt: Option<String>,

    /// Required sections that must be present
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_sections: Vec<String>,

    /// Optional sections that may be included
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_sections: Vec<String>,

    /// Reference to a template to use for generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<Uuid>,

    /// Context requirements for generation (e.g., needs_existing_code: true)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub context_requirements: HashMap<String, bool>,

    /// Validation rules (e.g., must_compile: true)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub validation_rules: HashMap<String, JsonValue>,

    /// Agent hints for generation (e.g., prefer_const: true)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_hints: HashMap<String, JsonValue>,

    /// Per-document-type revision chunking configuration (#573).
    /// When present, overrides system-wide defaults for this document type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_chunking: Option<RevisionChunkingConfig>,
}

/// Per-document-type revision chunking configuration (#573).
///
/// Controls how content is split into chunks for AI revision. Part of the
/// layered default resolution: per-call → document type → system → auto-computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RevisionChunkingConfig {
    /// Maximum characters per revision chunk. Null means use system default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    /// Character overlap between adjacent chunks. Default: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap: Option<usize>,
}

/// Extraction strategy for processing file attachments (Issue #436).
///
/// Determines how content is extracted from attached files for indexing and search.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionStrategy {
    /// Direct text extraction (plaintext, markdown) - no processing needed
    #[default]
    TextNative,
    /// PDF text extraction using pdftotext
    PdfText,
    /// Force OCR for scanned PDFs using tesseract
    PdfOcr,
    /// Image analysis using vision models (LLaVA)
    Vision,
    /// Audio transcription using Whisper
    AudioTranscribe,
    /// Video multimodal processing (frames + audio)
    VideoMultimodal,
    /// Code parsing using tree-sitter + LLM
    CodeAst,
    /// Office document conversion using pandoc
    OfficeConvert,
    /// Structured data extraction (JSON/YAML/XML)
    StructuredExtract,
    /// 3D model analysis via multi-view rendering + vision
    Glb3DModel,
    /// Email parsing (.eml, .mbox) with header extraction
    Email,
    /// Multi-sheet spreadsheet extraction (.xlsx, .xls, .ods)
    Spreadsheet,
    /// Archive content listing and text extraction (.zip, .tar.gz, .rar, .7z)
    Archive,
}

/// Strategy for extracting keyframes from video files.
///
/// Controls how FFmpeg selects frames for vision analysis.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum KeyframeStrategy {
    /// Extract one frame every N seconds (default: 10s).
    Interval {
        /// Seconds between keyframe captures.
        #[serde(default = "default_keyframe_interval")]
        every_n_secs: u64,
    },
    /// Detect scene changes using FFmpeg's scene filter.
    SceneDetection {
        /// Scene change threshold (0.0-1.0). Lower = more sensitive. Default: 0.3.
        #[serde(default = "default_scene_threshold")]
        threshold: f64,
    },
    /// Hybrid: scene detection with a minimum interval floor.
    Hybrid {
        /// Scene change threshold (0.0-1.0). Default: 0.3.
        #[serde(default = "default_scene_threshold")]
        scene_threshold: f64,
        /// Minimum seconds between frames even during rapid scene changes. Default: 2.
        #[serde(default = "default_min_interval")]
        min_interval_secs: u64,
    },
}

fn default_keyframe_interval() -> u64 {
    10
}
fn default_scene_threshold() -> f64 {
    0.3
}
fn default_min_interval() -> u64 {
    2
}

impl Default for KeyframeStrategy {
    fn default() -> Self {
        Self::Interval {
            every_n_secs: default_keyframe_interval(),
        }
    }
}

impl ExtractionStrategy {
    /// Determine extraction strategy from MIME type alone.
    ///
    /// Pure function — no database lookup needed. Maps the container format
    /// (how to extract content) not the semantic document type.
    pub fn from_mime_type(mime: &str) -> Self {
        let mime_lower = mime.to_lowercase();

        // PDF
        if mime_lower == "application/pdf" {
            return Self::PdfText;
        }

        // Images
        if mime_lower.starts_with("image/") {
            return Self::Vision;
        }

        // MIDI is structured data, not audio to transcribe
        if mime_lower == "audio/midi" || mime_lower == "audio/x-midi" {
            return Self::StructuredExtract;
        }

        // Audio
        if mime_lower.starts_with("audio/") {
            return Self::AudioTranscribe;
        }

        // Video
        if mime_lower.starts_with("video/") {
            return Self::VideoMultimodal;
        }

        // 3D models (GLB, glTF, OBJ, STL, etc.)
        if mime_lower.starts_with("model/") {
            return Self::Glb3DModel;
        }

        // Spreadsheets (before generic office check)
        if mime_lower.contains("spreadsheetml")
            || mime_lower == "application/vnd.ms-excel"
            || mime_lower == "application/vnd.oasis.opendocument.spreadsheet"
        {
            return Self::Spreadsheet;
        }

        // Office documents (non-spreadsheet) and binary diagram formats
        if mime_lower.contains("officedocument")
            || mime_lower.contains("msword")
            || mime_lower.contains("ms-powerpoint")
            || mime_lower == "application/rtf"
            || mime_lower.contains("ms-visio")
            || mime_lower == "application/vnd.visio"
        {
            return Self::OfficeConvert;
        }

        // Email / message formats
        if mime_lower.starts_with("message/")
            || mime_lower == "application/mbox"
            || mime_lower.contains("ms-outlook")
        {
            return Self::Email;
        }

        // Archives
        if mime_lower == "application/zip"
            || mime_lower == "application/x-tar"
            || mime_lower == "application/gzip"
            || mime_lower == "application/x-gzip"
            || mime_lower == "application/x-7z-compressed"
            || mime_lower == "application/x-rar-compressed"
            || mime_lower == "application/vnd.rar"
            || mime_lower == "application/x-bzip2"
            || mime_lower == "application/x-xz"
        {
            return Self::Archive;
        }

        // Structured data
        if matches!(
            mime_lower.as_str(),
            "application/json"
                | "application/xml"
                | "text/xml"
                | "application/yaml"
                | "text/yaml"
                | "text/csv"
                | "application/toml"
                | "application/x-bibtex"
                | "application/x-research-info-systems"
                | "application/avro"
                | "application/vnd.apache.parquet"
                | "application/x-ndjson"
                | "application/geo+json"
                | "application/x-drawio"
                | "application/x-drawio+xml"
                | "application/x-excalidraw+json"
                | "application/x-omnigraffle"
                | "text/calendar"
        ) {
            return Self::StructuredExtract;
        }

        // Plain text / markdown / code
        if mime_lower.starts_with("text/") {
            return Self::TextNative;
        }

        // application/octet-stream and other unknown types
        Self::TextNative
    }

    /// Determine extraction strategy from MIME type with file extension refinement.
    ///
    /// When the MIME type is ambiguous (e.g., `application/octet-stream`), the file
    /// extension can provide more specific information.
    pub fn from_mime_and_extension(mime: &str, extension: Option<&str>) -> Self {
        let base = Self::from_mime_type(mime);

        // Refine with extension when MIME is generic.
        //
        // IMPORTANT: When detect_content_type() returns application/octet-stream,
        // it may be because magic bytes contradicted a binary claim (e.g. random
        // garbage named "photo.jpg" claiming image/jpeg). In that case the extension
        // is equally untrustworthy, so we only promote to *cheap* strategies here.
        // Expensive strategies (Vision, AudioTranscribe, VideoMultimodal) are NOT
        // assigned from extension alone — they require magic byte confirmation via
        // detect_content_type() returning the actual media MIME type.
        if mime == "application/octet-stream" {
            if let Some(ext) = extension {
                return match ext.to_lowercase().as_str() {
                    // Cheap text-based extraction — safe even for misidentified files
                    "pdf" => Self::PdfText,
                    "xls" | "xlsx" | "ods" => Self::Spreadsheet,
                    "doc" | "docx" | "ppt" | "pptx" | "odt" | "odp" | "rtf" => Self::OfficeConvert,
                    "eml" | "mbox" => Self::Email,
                    "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "bz2" | "xz" => Self::Archive,
                    "json" | "xml" | "yaml" | "yml" | "csv" | "toml" => Self::StructuredExtract,
                    "ics" | "bib" | "geojson" | "ndjson" | "parquet" | "avro" | "mid" | "midi" => {
                        Self::StructuredExtract
                    }
                    // Diagram formats
                    "drawio" | "excalidraw" | "graffle" => Self::StructuredExtract,
                    "vsdx" | "vsd" => Self::OfficeConvert,
                    "d2" | "typ" | "mmd" | "mermaid" | "puml" | "plantuml" | "pu" | "dot"
                    | "gv" => Self::TextNative,
                    "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "rb"
                    | "swift" | "kt" | "scala" | "zig" | "hs" => Self::CodeAst,
                    "txt" | "md" | "markdown" | "rst" | "org" | "adoc" => Self::TextNative,
                    // Media extensions (jpg, mp3, mp4, etc.) are NOT promoted here.
                    // If magic bytes matched, detect_content_type() would have returned
                    // the actual media MIME type and we'd never reach this branch.
                    // Reaching here means the data doesn't match the extension claim.
                    _ => Self::TextNative,
                };
            }
        }

        // Refine code files that come as text/*
        if base == Self::TextNative {
            if let Some(ext) = extension {
                match ext.to_lowercase().as_str() {
                    "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "rb"
                    | "swift" | "kt" | "scala" | "zig" | "hs" => return Self::CodeAst,
                    _ => {}
                }
            }
        }

        base
    }
}

impl std::fmt::Display for ExtractionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextNative => write!(f, "text_native"),
            Self::PdfText => write!(f, "pdf_text"),
            Self::PdfOcr => write!(f, "pdf_ocr"),
            Self::Vision => write!(f, "vision"),
            Self::AudioTranscribe => write!(f, "audio_transcribe"),
            Self::VideoMultimodal => write!(f, "video_multimodal"),
            Self::CodeAst => write!(f, "code_ast"),
            Self::OfficeConvert => write!(f, "office_convert"),
            Self::StructuredExtract => write!(f, "structured_extract"),
            Self::Glb3DModel => write!(f, "glb_3d_model"),
            Self::Email => write!(f, "email"),
            Self::Spreadsheet => write!(f, "spreadsheet"),
            Self::Archive => write!(f, "archive"),
        }
    }
}

impl std::str::FromStr for ExtractionStrategy {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text_native" | "textnative" => Ok(Self::TextNative),
            "pdf_text" | "pdftext" => Ok(Self::PdfText),
            "pdf_ocr" | "pdfocr" | "pdf_scanned" => Ok(Self::PdfOcr),
            "vision" => Ok(Self::Vision),
            "audio_transcribe" | "audiotranscribe" => Ok(Self::AudioTranscribe),
            "video_multimodal" | "videomultimodal" => Ok(Self::VideoMultimodal),
            "code_ast" | "codeast" | "code_analysis" => Ok(Self::CodeAst),
            "office_convert" | "officeconvert" | "pandoc" => Ok(Self::OfficeConvert),
            "structured_extract" | "structuredextract" | "structured_data" => {
                Ok(Self::StructuredExtract)
            }
            "glb_3d_model" | "glb3dmodel" | "glb" | "3d_model" => Ok(Self::Glb3DModel),
            "email" | "eml" | "mbox" => Ok(Self::Email),
            "spreadsheet" | "xlsx" | "xls" | "ods" => Ok(Self::Spreadsheet),
            "archive" | "zip" | "tar" | "7z" | "rar" => Ok(Self::Archive),
            "none" => Ok(Self::TextNative),
            _ => Err(format!("Invalid extraction strategy: {}", s)),
        }
    }
}

/// A document type configuration.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DocumentType {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub category: DocumentCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // Detection rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mime_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub magic_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filename_patterns: Vec<String>,

    // Chunking configuration
    pub chunking_strategy: ChunkingStrategy,
    pub chunk_size_default: i32,
    pub chunk_overlap_default: i32,
    pub preserve_boundaries: bool,
    #[serde(default)]
    pub chunking_config: JsonValue,

    // Embedding recommendation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_config_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_types: Vec<String>,

    // Tree-sitter support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sitter_language: Option<String>,

    // Extraction configuration (Issue #436)
    /// Strategy for extracting content from file attachments
    #[serde(default)]
    pub extraction_strategy: ExtractionStrategy,
    /// Strategy-specific configuration (e.g., model, OCR settings)
    #[serde(default)]
    pub extraction_config: JsonValue,
    /// When true, notes of this type require a file attachment
    #[serde(default)]
    pub requires_attachment: bool,
    /// When true, attachment content becomes the primary note content
    #[serde(default)]
    pub attachment_generates_content: bool,

    // System vs user-defined
    pub is_system: bool,
    pub is_active: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    // AI generation metadata
    #[serde(default)]
    pub agentic_config: AgenticConfig,
}

/// Summary view of document types for listing.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DocumentTypeSummary {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub category: DocumentCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub chunking_strategy: ChunkingStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sitter_language: Option<String>,
    /// Strategy for extracting content from file attachments
    #[serde(default)]
    pub extraction_strategy: ExtractionStrategy,
    /// When true, notes of this type require a file attachment
    #[serde(default)]
    pub requires_attachment: bool,
    pub is_system: bool,
    pub is_active: bool,
}

/// Request to create a document type.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateDocumentTypeRequest {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub category: DocumentCategory,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub magic_patterns: Vec<String>,
    #[serde(default)]
    pub filename_patterns: Vec<String>,
    #[serde(default)]
    pub chunking_strategy: ChunkingStrategy,
    #[serde(default = "default_chunk_size")]
    pub chunk_size_default: i32,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap_default: i32,
    #[serde(default = "default_true")]
    pub preserve_boundaries: bool,
    #[serde(default)]
    pub chunking_config: Option<JsonValue>,
    #[serde(default)]
    pub recommended_config_id: Option<Uuid>,
    #[serde(default)]
    pub content_types: Vec<String>,
    #[serde(default)]
    pub tree_sitter_language: Option<String>,
    #[serde(default)]
    pub agentic_config: Option<AgenticConfig>,
    // Extraction configuration (Issue #436)
    #[serde(default)]
    pub extraction_strategy: ExtractionStrategy,
    #[serde(default)]
    pub extraction_config: Option<JsonValue>,
    #[serde(default)]
    pub requires_attachment: bool,
    #[serde(default)]
    pub attachment_generates_content: bool,
}

fn default_chunk_size() -> i32 {
    crate::defaults::CHUNK_SIZE_I32
}

fn default_chunk_overlap() -> i32 {
    crate::defaults::CHUNK_OVERLAP_I32
}

/// Request to update a document type.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDocumentTypeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magic_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunking_strategy: Option<ChunkingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size_default: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_overlap_default: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_boundaries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunking_config: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_config_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_sitter_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agentic_config: Option<AgenticConfig>,
    // Extraction configuration (Issue #436)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_strategy: Option<ExtractionStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_config: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_attachment: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_generates_content: Option<bool>,
}

/// Result from document type detection.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DetectDocumentTypeResult {
    pub document_type: DocumentTypeSummary,
    pub confidence: f32,
    pub detection_method: String,
}

// =============================================================================
// FILE ATTACHMENT TYPES (Issue #433)
// =============================================================================

/// Attachment blob for content-addressable storage.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AttachmentBlob {
    pub id: Uuid,
    pub content_hash: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_backend: String,
    pub storage_path: Option<String>,
    pub reference_count: i32,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for AttachmentBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentBlob")
            .field("id_set", &true)
            .field("content_hash_len", &self.content_hash.len())
            .field("content_type_len", &self.content_type.len())
            .field("size_bytes", &self.size_bytes)
            .field("storage_backend_len", &self.storage_backend.len())
            .field(
                "storage_path_len",
                &self.storage_path.as_ref().map(String::len),
            )
            .field("reference_count", &self.reference_count)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// File attachment metadata.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Attachment {
    pub id: Uuid,
    pub note_id: Uuid,
    pub blob_id: Uuid,
    pub filename: String,
    pub original_filename: Option<String>,
    pub document_type_id: Option<Uuid>,
    pub status: AttachmentStatus,
    pub extraction_strategy: Option<ExtractionStrategy>,
    pub extracted_text: Option<String>,
    pub extracted_metadata: Option<JsonValue>,
    /// AI-generated description from extraction adapters (Vision, Glb3DModel, etc.)
    pub ai_description: Option<String>,
    /// Model used to generate the AI description
    pub ai_model: Option<String>,
    pub has_preview: bool,
    pub is_canonical_content: bool,
    pub detected_document_type_id: Option<Uuid>,
    pub detection_confidence: Option<f32>,
    pub detection_method: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for Attachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attachment")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("blob_id_set", &true)
            .field("filename_len", &self.filename.len())
            .field(
                "original_filename_len",
                &self.original_filename.as_ref().map(String::len),
            )
            .field("document_type_id_set", &self.document_type_id.is_some())
            .field("status", &self.status)
            .field("extraction_strategy", &self.extraction_strategy)
            .field(
                "extracted_text_len",
                &self.extracted_text.as_ref().map(String::len),
            )
            .field(
                "extracted_metadata_class",
                &self.extracted_metadata.as_ref().map(json_value_class),
            )
            .field(
                "extracted_metadata_len",
                &self.extracted_metadata.as_ref().map(json_serialized_len),
            )
            .field(
                "ai_description_len",
                &self.ai_description.as_ref().map(String::len),
            )
            .field("ai_model_len", &self.ai_model.as_ref().map(String::len))
            .field("has_preview", &self.has_preview)
            .field("is_canonical_content", &self.is_canonical_content)
            .field(
                "detected_document_type_id_set",
                &self.detected_document_type_id.is_some(),
            )
            .field("detection_confidence", &self.detection_confidence)
            .field(
                "detection_method_len",
                &self.detection_method.as_ref().map(String::len),
            )
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Processing status for attachments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentStatus {
    #[default]
    Uploaded,
    Queued,
    Processing,
    Completed,
    Failed,
    Quarantined,
}

impl std::fmt::Display for AttachmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uploaded => write!(f, "uploaded"),
            Self::Queued => write!(f, "queued"),
            Self::Processing => write!(f, "processing"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Quarantined => write!(f, "quarantined"),
        }
    }
}

impl std::str::FromStr for AttachmentStatus {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "uploaded" => Ok(Self::Uploaded),
            "queued" => Ok(Self::Queued),
            "processing" => Ok(Self::Processing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "quarantined" => Ok(Self::Quarantined),
            _ => Err(format!("Invalid attachment status: {}", s)),
        }
    }
}

/// Summary for API responses.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AttachmentSummary {
    pub id: Uuid,
    pub note_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: AttachmentStatus,
    pub document_type_name: Option<String>,
    pub detected_document_type_name: Option<String>,
    pub detection_confidence: Option<f32>,
    pub has_preview: bool,
    pub is_canonical_content: bool,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for AttachmentSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentSummary")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("filename_len", &self.filename.len())
            .field("content_type_len", &self.content_type.len())
            .field("size_bytes", &self.size_bytes)
            .field("status", &self.status)
            .field(
                "document_type_name_len",
                &self.document_type_name.as_ref().map(String::len),
            )
            .field(
                "detected_document_type_name_len",
                &self.detected_document_type_name.as_ref().map(String::len),
            )
            .field("detection_confidence", &self.detection_confidence)
            .field("has_preview", &self.has_preview)
            .field("is_canonical_content", &self.is_canonical_content)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Summary for global attachment listing (includes note_title).
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GlobalAttachmentSummary {
    pub id: Uuid,
    pub note_id: Uuid,
    pub note_title: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: AttachmentStatus,
    pub document_type_name: Option<String>,
    pub detected_document_type_name: Option<String>,
    pub detection_confidence: Option<f32>,
    pub has_preview: bool,
    pub is_canonical_content: bool,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for GlobalAttachmentSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalAttachmentSummary")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("note_title_len", &self.note_title.as_ref().map(String::len))
            .field("filename_len", &self.filename.len())
            .field("content_type_len", &self.content_type.len())
            .field("size_bytes", &self.size_bytes)
            .field("status", &self.status)
            .field(
                "document_type_name_len",
                &self.document_type_name.as_ref().map(String::len),
            )
            .field(
                "detected_document_type_name_len",
                &self.detected_document_type_name.as_ref().map(String::len),
            )
            .field("detection_confidence", &self.detection_confidence)
            .field("has_preview", &self.has_preview)
            .field("is_canonical_content", &self.is_canonical_content)
            .field("created_at", &self.created_at)
            .finish()
    }
}
// =============================================================================
// TUS RESUMABLE UPLOADS
// =============================================================================

/// Tracks an in-progress tus resumable upload session.
///
/// Each row maps to a staging file on disk at `storage_path`. When the final
/// chunk is received (`current_offset == total_size`), the upload is finalized
/// into the standard attachment pipeline and this row is deleted.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema, sqlx::FromRow)]
pub struct TusUpload {
    pub id: Uuid,
    pub note_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub total_size: i64,
    pub current_offset: i64,
    pub storage_path: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for TusUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TusUpload")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("filename_len", &self.filename.len())
            .field("content_type_len", &self.content_type.len())
            .field("total_size", &self.total_size)
            .field("current_offset", &self.current_offset)
            .field("storage_path_len", &self.storage_path.len())
            .field("metadata_class", &json_value_class(&self.metadata))
            .field("metadata_len", &json_serialized_len(&self.metadata))
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

// =============================================================================
// REAL-TIME CALL SESSION TYPES
// =============================================================================

/// Provider-agnostic persisted real-time call session metadata.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema, sqlx::FromRow)]
pub struct CallSession {
    pub call_id: Uuid,
    pub provider: String,
    pub provider_call_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub asr_backend: Option<String>,
    pub remote_party: Option<String>,
    pub archive_id: Option<Uuid>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl fmt::Debug for CallSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallSession")
            .field("call_id_set", &true)
            .field("provider_len", &self.provider.len())
            .field("provider_call_id_len", &self.provider_call_id.len())
            .field("started_at", &self.started_at)
            .field("ended_at_set", &self.ended_at.is_some())
            .field("end_reason_len", &self.end_reason.as_ref().map(String::len))
            .field(
                "asr_backend_len",
                &self.asr_backend.as_ref().map(String::len),
            )
            .field(
                "remote_party_len",
                &self.remote_party.as_ref().map(String::len),
            )
            .field("archive_id_set", &self.archive_id.is_some())
            .field("metadata_class", &json_value_class(&self.metadata))
            .field("metadata_len", &json_serialized_len(&self.metadata))
            .finish()
    }
}

/// Request payload for creating a call session row.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateCallSessionRequest {
    pub provider: String,
    pub provider_call_id: String,
    pub started_at: Option<DateTime<Utc>>,
    pub asr_backend: Option<String>,
    pub remote_party: Option<String>,
    pub archive_id: Option<Uuid>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl fmt::Debug for CreateCallSessionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateCallSessionRequest")
            .field("provider_len", &self.provider.len())
            .field("provider_call_id_len", &self.provider_call_id.len())
            .field("started_at_set", &self.started_at.is_some())
            .field(
                "asr_backend_len",
                &self.asr_backend.as_ref().map(String::len),
            )
            .field(
                "remote_party_len",
                &self.remote_party.as_ref().map(String::len),
            )
            .field("archive_id_set", &self.archive_id.is_some())
            .field("metadata_class", &json_value_class(&self.metadata))
            .field("metadata_len", &json_serialized_len(&self.metadata))
            .finish()
    }
}

/// Partial update payload for ending or annotating a call session.
#[derive(Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateCallSessionRequest {
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub asr_backend: Option<String>,
    pub remote_party: Option<String>,
    pub archive_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

impl fmt::Debug for UpdateCallSessionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateCallSessionRequest")
            .field("ended_at_set", &self.ended_at.is_some())
            .field("end_reason_len", &self.end_reason.as_ref().map(String::len))
            .field(
                "asr_backend_len",
                &self.asr_backend.as_ref().map(String::len),
            )
            .field(
                "remote_party_len",
                &self.remote_party.as_ref().map(String::len),
            )
            .field("archive_id_set", &self.archive_id.is_some())
            .field(
                "metadata_class",
                &self.metadata.as_ref().map(json_value_class),
            )
            .field(
                "metadata_len",
                &self.metadata.as_ref().map(json_serialized_len),
            )
            .finish()
    }
}

/// Final transcript segment persisted for a call session.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema, sqlx::FromRow)]
pub struct TranscriptSegment {
    pub id: Uuid,
    pub call_id: Uuid,
    pub speaker_label: Option<String>,
    pub text: String,
    pub start_ts: Option<f64>,
    pub end_ts: Option<f64>,
    pub confidence: Option<f32>,
    pub sequence: i32,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for TranscriptSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TranscriptSegment")
            .field("id_set", &true)
            .field("call_id_set", &true)
            .field(
                "speaker_label_len",
                &self.speaker_label.as_ref().map(String::len),
            )
            .field("text_len", &self.text.len())
            .field("start_ts_set", &self.start_ts.is_some())
            .field("end_ts_set", &self.end_ts.is_some())
            .field("confidence", &self.confidence)
            .field("sequence", &self.sequence)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Request payload for persisting a final transcript segment.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateTranscriptSegmentRequest {
    pub call_id: Uuid,
    pub speaker_label: Option<String>,
    pub text: String,
    pub start_ts: Option<f64>,
    pub end_ts: Option<f64>,
    pub confidence: Option<f32>,
    pub sequence: i32,
}

impl fmt::Debug for CreateTranscriptSegmentRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateTranscriptSegmentRequest")
            .field("call_id_set", &true)
            .field(
                "speaker_label_len",
                &self.speaker_label.as_ref().map(String::len),
            )
            .field("text_len", &self.text.len())
            .field("start_ts_set", &self.start_ts.is_some())
            .field("end_ts_set", &self.end_ts.is_some())
            .field("confidence", &self.confidence)
            .field("sequence", &self.sequence)
            .finish()
    }
}

// =============================================================================
// ENTITY TYPES (TRI-MODAL SEARCH)
// =============================================================================

/// Entity types for named entity recognition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Product,
    Event,
    Date,
    Money,
    Percent,
    WorkOfArt,
    Language,
    Other,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Person => write!(f, "person"),
            Self::Organization => write!(f, "organization"),
            Self::Location => write!(f, "location"),
            Self::Product => write!(f, "product"),
            Self::Event => write!(f, "event"),
            Self::Date => write!(f, "date"),
            Self::Money => write!(f, "money"),
            Self::Percent => write!(f, "percent"),
            Self::WorkOfArt => write!(f, "work_of_art"),
            Self::Language => write!(f, "language"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl std::str::FromStr for EntityType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "person" => Ok(Self::Person),
            "organization" | "org" => Ok(Self::Organization),
            "location" | "loc" | "gpe" => Ok(Self::Location),
            "product" => Ok(Self::Product),
            "event" => Ok(Self::Event),
            "date" => Ok(Self::Date),
            "money" => Ok(Self::Money),
            "percent" => Ok(Self::Percent),
            "work_of_art" => Ok(Self::WorkOfArt),
            "language" => Ok(Self::Language),
            _ => Ok(Self::Other),
        }
    }
}

/// An extracted named entity from a note.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteEntity {
    pub id: Uuid,
    pub note_id: Uuid,
    pub entity_text: String,
    pub entity_type: EntityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for NoteEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteEntity")
            .field("id_set", &true)
            .field("note_id_set", &true)
            .field("entity_text_len", &self.entity_text.len())
            .field("entity_type", &self.entity_type)
            .field("start_offset_set", &self.start_offset.is_some())
            .field("end_offset_set", &self.end_offset.is_some())
            .field("confidence", &self.confidence)
            .field(
                "normalized_text_len",
                &self.normalized_text.as_ref().map(String::len),
            )
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Entity statistics for IDF weighting.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EntityStats {
    pub entity_text: String,
    pub doc_frequency: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idf_score: Option<f32>,
    pub last_updated: DateTime<Utc>,
}

impl fmt::Debug for EntityStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityStats")
            .field("entity_text_len", &self.entity_text.len())
            .field("doc_frequency", &self.doc_frequency)
            .field("idf_score", &self.idf_score)
            .field("last_updated", &self.last_updated)
            .finish()
    }
}

/// Graph embedding for a note (aggregated entity representation).
#[derive(Clone)]
pub struct NoteGraphEmbedding {
    pub note_id: Uuid,
    pub vector: Vector,
    pub entity_count: i32,
    pub entity_types: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for NoteGraphEmbedding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteGraphEmbedding")
            .field("note_id_set", &true)
            .field("vector_dimensions", &self.vector.as_slice().len())
            .field("entity_count", &self.entity_count)
            .field("entity_types_count", &self.entity_types.len())
            .field(
                "entity_type_lens",
                &self
                    .entity_types
                    .iter()
                    .map(String::len)
                    .collect::<Vec<_>>(),
            )
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

// =============================================================================
// FINE-TUNING TYPES
// =============================================================================

/// Status of a fine-tuning dataset generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FineTuningStatus {
    #[default]
    Pending,
    Generating,
    Completed,
    Failed,
}

/// Configuration for fine-tuning dataset generation.
#[derive(Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FineTuningConfig {
    /// Number of queries to generate per document
    #[serde(default = "default_queries_per_doc")]
    pub queries_per_doc: i32,
    /// Model to use for query generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_generator_model: Option<String>,
    /// Model to use for quality filtering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_filter_model: Option<String>,
    /// Minimum quality score to keep (1-5)
    #[serde(default = "default_min_quality")]
    pub min_quality_score: f32,
    /// Include hard negatives in training data
    #[serde(default)]
    pub include_hard_negatives: bool,
    /// Fraction of samples for validation (0.0-1.0)
    #[serde(default = "default_validation_split")]
    pub validation_split: f32,
}

impl fmt::Debug for FineTuningConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FineTuningConfig")
            .field("queries_per_doc", &self.queries_per_doc)
            .field(
                "query_generator_model_len",
                &self.query_generator_model.as_ref().map(String::len),
            )
            .field(
                "quality_filter_model_len",
                &self.quality_filter_model.as_ref().map(String::len),
            )
            .field("min_quality_score", &self.min_quality_score)
            .field("include_hard_negatives", &self.include_hard_negatives)
            .field("validation_split", &self.validation_split)
            .finish()
    }
}

fn default_queries_per_doc() -> i32 {
    crate::defaults::FINETUNE_QUERIES_PER_DOC
}

fn default_min_quality() -> f32 {
    crate::defaults::FINETUNE_MIN_QUALITY
}

fn default_validation_split() -> f32 {
    crate::defaults::FINETUNE_VALIDATION_SPLIT
}

/// A fine-tuning dataset configuration.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FineTuningDataset {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source_type: String,
    pub source_id: String,
    pub config: FineTuningConfig,
    pub status: FineTuningStatus,
    pub sample_count: i32,
    pub training_count: i32,
    pub validation_count: i32,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl fmt::Debug for FineTuningDataset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FineTuningDataset")
            .field("id_set", &true)
            .field("name_len", &self.name.len())
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .field("source_type_len", &self.source_type.len())
            .field("source_id_len", &self.source_id.len())
            .field("config", &self.config)
            .field("status", &self.status)
            .field("sample_count", &self.sample_count)
            .field("training_count", &self.training_count)
            .field("validation_count", &self.validation_count)
            .field("created_at", &self.created_at)
            .field("completed_at_set", &self.completed_at.is_some())
            .field(
                "error_message_len",
                &self.error_message.as_ref().map(String::len),
            )
            .finish()
    }
}

/// A query-document sample for fine-tuning.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FineTuningSample {
    pub id: Uuid,
    pub dataset_id: Uuid,
    pub note_id: Uuid,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    pub is_validation: bool,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for FineTuningSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FineTuningSample")
            .field("id_set", &true)
            .field("dataset_id_set", &true)
            .field("note_id_set", &true)
            .field("query_len", &self.query.len())
            .field("query_type_len", &self.query_type.as_ref().map(String::len))
            .field("quality_score", &self.quality_score)
            .field("is_validation", &self.is_validation)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Request to create a fine-tuning dataset.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateFineTuningDatasetRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source type: 'embedding_set', 'tag', or 'collection'
    pub source_type: String,
    /// Source identifier: slug, tag name, or collection id
    pub source_id: String,
    #[serde(default)]
    pub config: FineTuningConfig,
}

impl fmt::Debug for CreateFineTuningDatasetRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateFineTuningDatasetRequest")
            .field("name_len", &self.name.len())
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .field("source_type_len", &self.source_type.len())
            .field("source_id_len", &self.source_id.len())
            .field("config", &self.config)
            .finish()
    }
}

// =============================================================================
// COARSE EMBEDDING TYPES (TWO-STAGE RETRIEVAL)
// =============================================================================

/// Configuration for two-stage coarse-to-fine retrieval.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TwoStageSearchConfig {
    /// Dimension for coarse stage (must be MRL-compatible)
    #[serde(default = "default_coarse_dim")]
    pub coarse_dim: i32,
    /// Number of candidates from coarse stage
    #[serde(default = "default_coarse_k")]
    pub coarse_k: i32,
    /// HNSW ef_search for coarse stage
    #[serde(default = "default_coarse_ef_search")]
    pub coarse_ef_search: i32,
}

fn default_coarse_dim() -> i32 {
    crate::defaults::COARSE_DIM
}

fn default_coarse_k() -> i32 {
    crate::defaults::COARSE_K
}

fn default_coarse_ef_search() -> i32 {
    crate::defaults::COARSE_EF_SEARCH
}

impl Default for TwoStageSearchConfig {
    fn default() -> Self {
        Self {
            coarse_dim: default_coarse_dim(),
            coarse_k: default_coarse_k(),
            coarse_ef_search: default_coarse_ef_search(),
        }
    }
}

/// Coarse embedding for fast initial filtering in two-stage retrieval.
#[derive(Debug, Clone)]
pub struct CoarseEmbedding {
    pub note_id: Uuid,
    pub embedding_set_id: Option<Uuid>,
    pub chunk_index: i32,
    pub vector: Vector,
    pub created_at: DateTime<Utc>,
}

/// Tri-modal fusion weights for search.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TriModalWeights {
    /// Weight for semantic (dense vector) search
    #[serde(default = "default_semantic_weight")]
    pub semantic: f32,
    /// Weight for lexical (FTS/BM25) search
    #[serde(default = "default_lexical_weight")]
    pub lexical: f32,
    /// Weight for graph (entity) search
    #[serde(default = "default_graph_weight")]
    pub graph: f32,
}

fn default_semantic_weight() -> f32 {
    crate::defaults::TRIMODAL_SEMANTIC_WEIGHT
}

fn default_lexical_weight() -> f32 {
    crate::defaults::TRIMODAL_LEXICAL_WEIGHT
}

fn default_graph_weight() -> f32 {
    crate::defaults::TRIMODAL_GRAPH_WEIGHT
}

impl Default for TriModalWeights {
    fn default() -> Self {
        Self {
            semantic: default_semantic_weight(),
            lexical: default_lexical_weight(),
            graph: default_graph_weight(),
        }
    }
}

// =============================================================================
// EMBEDDING SET LIFECYCLE TYPES
// =============================================================================

/// Health summary for an embedding set.
///
/// Provides metrics on staleness, orphaned data, and missing embeddings
/// to guide maintenance operations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EmbeddingSetHealth {
    pub set_id: Uuid,
    /// Total documents in the set.
    pub total_documents: i32,
    /// Total embeddings in the set.
    pub total_embeddings: i32,
    /// Embeddings that are older than their source notes (need regeneration).
    pub stale_embeddings: i64,
    /// Embeddings for notes that no longer exist or are not members.
    pub orphaned_embeddings: i64,
    /// Members without any embeddings.
    pub missing_embeddings: i64,
    /// Health score (0-100): percentage of documents with current embeddings.
    pub health_score: f64,
    /// Whether the set needs a refresh operation.
    pub needs_refresh: bool,
    /// Whether the set needs garbage collection.
    pub needs_pruning: bool,
}

/// Result of a garbage collection operation on an embedding set.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GarbageCollectionResult {
    pub set_id: Uuid,
    /// Number of orphaned memberships removed.
    pub orphaned_memberships_removed: i64,
    /// Number of orphaned embeddings removed.
    pub orphaned_embeddings_removed: i64,
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

/// AI revision mode controlling enhancement aggressiveness.
///
/// Modes are ordered by how much transformation they apply:
/// - `None`: No revision, original preserved as-is
/// - `Light`: Formatting and structure only, no invented details
/// - `Standard`: Intelligent revision (summarization, key concepts) using ONLY the note's own content
/// - `Full`: Legacy alias for `Contextual` (backward compatibility)
/// - `Contextual`: Two-phase pipeline — isolated revision then cross-referencing with related notes
/// - `ContextualFiltered`: Same as Contextual but scoped to specific tags/collections
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RevisionMode {
    /// Full contextual enhancement - expands content with related concepts.
    /// Legacy alias for Contextual, kept for backward compatibility.
    Full,
    /// Light touch - formatting and structure only, no invented details
    Light,
    /// Intelligent revision with structural improvements, summarization, key concept extraction.
    /// Operates ONLY on the note's own content — no external context injection (default).
    #[default]
    Standard,
    /// Two-phase contextual enhancement: isolated revision → embed → find similar → re-revise
    /// with cross-references from related notes. Opt-in only.
    Contextual,
    /// Same as Contextual but scoped: related notes filtered by tags/collection.
    /// Requires `context_filter` in job payload.
    #[serde(rename = "contextual_filtered")]
    ContextualFiltered,
    /// No AI revision - store original as-is
    None,
}

impl RevisionMode {
    /// Returns true if this mode requires the two-phase contextual pipeline.
    /// Phase 1 (AiRevision with Standard) is always run first, then Phase 2
    /// (AiRevisionContextual) is queued automatically.
    pub fn is_contextual(&self) -> bool {
        matches!(
            self,
            RevisionMode::Full | RevisionMode::Contextual | RevisionMode::ContextualFiltered
        )
    }

    /// Returns the effective mode for Phase 1 (isolated revision).
    /// Contextual modes run Standard as Phase 1, then queue Phase 2 separately.
    pub fn phase1_mode(&self) -> RevisionMode {
        if self.is_contextual() {
            RevisionMode::Standard
        } else {
            *self
        }
    }
}

/// Type of job to process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Generate AI revision of content (isolated: Light or Standard mode)
    AiRevision,
    /// Phase 2 contextual re-revision: takes Phase 1 output, embeds, finds similar, re-revises
    /// with cross-references. Queued automatically by AiRevision when mode is Contextual/ContextualFiltered.
    AiRevisionContextual,
    /// Generate embeddings for content
    Embedding,
    /// Auto-detect and create links
    Linking,
    /// Update context/metadata
    ContextUpdate,
    /// Generate title from content
    TitleGeneration,
    /// Create a new embedding set (evaluate criteria, add members)
    CreateEmbeddingSet,
    /// Refresh an embedding set (re-evaluate criteria, update membership)
    RefreshEmbeddingSet,
    /// Build or rebuild the vector index for an embedding set
    BuildSetIndex,
    /// Permanently delete a note and all related data
    PurgeNote,
    /// Auto-generate SKOS concept tags using AI analysis
    ConceptTagging,
    /// Re-embed all notes (used during embedding model migration)
    ReEmbedAll,
    /// Extract named entities from note content for tri-modal search
    EntityExtraction,
    /// Generate synthetic query-document pairs for fine-tuning
    GenerateFineTuningData,
    /// Embed specific notes into a specific embedding set
    EmbedForSet,
    /// Generate graph embedding from extracted entities
    GenerateGraphEmbedding,
    /// Generate coarse (small-dimension) embedding for two-stage retrieval
    GenerateCoarseEmbedding,
    /// Extract EXIF metadata from images
    ExifExtraction,
    /// Extract content from file attachment
    Extraction,
    /// Classify attachment into a semantic document type using AI
    DocumentTypeInference,
    /// Extract rich metadata from note content using AI analysis
    MetadataExtraction,
    /// Infer SKOS related (associative) concept relationships using AI
    RelatedConceptInference,
    /// Extract named entity references (companies, people, tools, etc.) from content
    ReferenceExtraction,
    /// Graph maintenance: normalization, SNN, PFNET, Louvain pipeline (#482)
    GraphMaintenance,
    /// Run speaker diarization on audio after transcription (#497)
    SpeakerDiarization,
    /// Re-render transcript with user-assigned speaker names (#497)
    SpeakerRelabel,
    /// Pre-generate streamable media variants (faststart, remux, preview) (#506)
    MediaOptimize,
    /// Generate thumbnail sprite sheets and WebVTT map from keyframes (#525)
    ThumbnailSprite,
    /// Describe a single video keyframe using vision LLM (atomic, parallelizable) (#526)
    KeyframeVision,
    /// Identify characters/people in a single video keyframe via vision LLM (#550)
    KeyframeCharacterVision,
    /// Identify place, setting, and objects in a single video keyframe via vision LLM (#550)
    KeyframeSettingVision,
    /// Aggregate all keyframe descriptions and rebuild video markdown (#526)
    KeyframeAssembly,
    /// Describe a single 3D model rendered view using vision LLM (atomic, parallelizable) (#533)
    ViewVision,
    /// Aggregate all 3D view descriptions and rebuild model markdown (#533)
    ViewAssembly,
    /// Transcribe audio from a derived attachment via Whisper (atomic, retryable) (#542)
    AudioTranscription,
    /// Transcribe a single audio chunk via Whisper (atomic, parallelizable) (#543)
    AudioChunkTranscription,
}

impl JobType {
    /// Default priority for this job type (higher = more urgent)
    pub fn default_priority(&self) -> i32 {
        match self {
            JobType::AiRevision => 8,
            // Contextual re-revision runs after AiRevision, slightly lower priority
            JobType::AiRevisionContextual => 7,
            JobType::Embedding => 5,
            JobType::Linking => 3,
            JobType::TitleGeneration => 2,
            JobType::ContextUpdate => 1,
            // Embedding set jobs are lower priority (background tasks)
            JobType::CreateEmbeddingSet => 2,
            JobType::RefreshEmbeddingSet => 2,
            JobType::BuildSetIndex => 3,
            // Purge is high priority to complete cleanup quickly
            JobType::PurgeNote => 9,
            // Concept tagging runs after embedding (needs content analysis)
            JobType::ConceptTagging => 4,
            // Re-embed is low priority background migration task
            JobType::ReEmbedAll => 1,
            // Entity extraction runs after embedding (needs content analysis)
            JobType::EntityExtraction => 4,
            // Fine-tuning data generation is low priority background task
            JobType::GenerateFineTuningData => 1,
            // Embed for specific set - similar priority to regular embedding
            JobType::EmbedForSet => 5,
            // Graph embedding generation after entity extraction
            JobType::GenerateGraphEmbedding => 3,
            // Coarse embedding generation - batch background task
            JobType::GenerateCoarseEmbedding => 2,
            // EXIF extraction - medium priority for metadata processing
            JobType::ExifExtraction => 5,
            // Extraction is high priority since it gates downstream work
            JobType::Extraction => 7,
            // Document type inference - low priority, runs after content extraction
            JobType::DocumentTypeInference => 2,
            // Metadata extraction - runs in Phase 1 alongside tagging
            JobType::MetadataExtraction => 4,
            // Related concept inference - Phase 2, queued by ConceptTagging
            JobType::RelatedConceptInference => 4,
            // Reference extraction - Phase 1 peer alongside ConceptTagging
            JobType::ReferenceExtraction => 4,
            // Graph maintenance runs after linking, low urgency background task
            JobType::GraphMaintenance => 2,
            // Speaker diarization runs after transcription, medium priority
            JobType::SpeakerDiarization => 5,
            // Speaker relabel is user-triggered re-render, higher priority
            JobType::SpeakerRelabel => 6,
            // Media optimization runs after extraction, low priority background task
            JobType::MediaOptimize => 3,
            // Thumbnail sprite generation runs after extraction, low priority background task
            JobType::ThumbnailSprite => 3,
            // Keyframe vision is per-frame, medium priority (gates assembly)
            JobType::KeyframeVision
            | JobType::KeyframeCharacterVision
            | JobType::KeyframeSettingVision => 4,
            // Keyframe assembly runs after all vision jobs, low priority
            JobType::KeyframeAssembly => 3,
            // View vision is per-view, medium priority (gates assembly) (#533)
            JobType::ViewVision => 4,
            // View assembly runs after all vision jobs, low priority (#533)
            JobType::ViewAssembly => 3,
            // Audio transcription is medium-high priority (gates assembly + diarization) (#542)
            JobType::AudioTranscription => 6,
            JobType::AudioChunkTranscription => 6,
        }
    }

    /// Default cost tier for this job type.
    ///
    /// Returns the tier that should be used when queuing this job type.
    /// `None` means tier-agnostic (CPU or no GPU needed).
    pub fn default_cost_tier(&self) -> Option<i16> {
        match self {
            // CPU/NER tier: GLiNER concept extraction and reference NER
            JobType::ConceptTagging | JobType::ReferenceExtraction => Some(cost_tier::CPU_NER),
            // Fast GPU tier: title gen, metadata extraction
            JobType::TitleGeneration | JobType::MetadataExtraction => Some(cost_tier::FAST_GPU),
            // Standard GPU tier: AI revision uses the standard generation model.
            // Serialized by gpu_concurrent (default 1) to avoid VRAM contention.
            JobType::AiRevision | JobType::AiRevisionContextual => Some(cost_tier::STANDARD_GPU),
            // Vision GPU tier: per-frame/per-view vision LLM description.
            // Serialized by default (VISION_MAX_CONCURRENT=1) to prevent
            // VRAM contention on single-GPU systems.
            JobType::KeyframeVision
            | JobType::KeyframeCharacterVision
            | JobType::KeyframeSettingVision
            | JobType::ViewVision => Some(cost_tier::VISION_GPU),
            // Audio transcription: dedicated tier for sidecar lifecycle management (#576).
            // When GPU_EXCLUSIVE_MODE is enabled, sidecars start/stop at tier boundaries.
            JobType::AudioTranscription
            | JobType::AudioChunkTranscription
            | JobType::SpeakerDiarization => Some(cost_tier::AUDIO_GPU),
            // Everything else is tier-agnostic
            _ => None,
        }
    }
}

/// Tier groups for the worker drain loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierGroup {
    /// CPU-only and agnostic jobs (cost_tier IS NULL OR cost_tier = 0).
    CpuAndAgnostic,
    /// Audio GPU jobs (cost_tier = 5): whisper transcription + pyannote diarization.
    /// When GPU_EXCLUSIVE_MODE is enabled, sidecars are started before this tier
    /// and stopped after it drains, freeing ~6.6 GB VRAM for subsequent tiers. (#576)
    AudioGpu,
    /// Fast GPU jobs (cost_tier = 1).
    FastGpu,
    /// Standard GPU jobs (cost_tier = 2).
    StandardGpu,
    /// Render GPU jobs (cost_tier = 4): Open3D multi-view rendering of 3D models.
    /// Drains before VisionGpu so rendered images are available when vision
    /// descriptions start. No Ollama model warmup needed (uses HTTP renderer).
    RenderGpu,
    /// Vision GPU jobs (cost_tier = 3).
    /// Serialized by default to avoid VRAM contention on single-GPU systems.
    VisionGpu,
}

/// Cost tier constants for tiered atomic job architecture.
///
/// Each job step uses exactly one model/algorithm. The worker processes
/// all jobs of the same tier together, with model warmup between tier switches.
pub mod cost_tier {
    /// CPU/NER tier: GLiNER concept extraction, GLiNER reference NER (<300ms).
    pub const CPU_NER: i16 = 0;
    /// Fast GPU tier: qwen3.5:9b concept tagging, title generation (5-15s).
    pub const FAST_GPU: i16 = 1;
    /// Standard GPU tier: qwen3.5:27b AI revision, fallback extraction (60-105s).
    pub const STANDARD_GPU: i16 = 2;
    /// Vision GPU tier: vision LLM per-frame/per-view description (10-30s each).
    /// Serialized by default (`VISION_MAX_CONCURRENT=1`) to avoid VRAM contention
    /// on single-GPU systems with 6-8GB VRAM.
    pub const VISION_GPU: i16 = 3;
    /// Render GPU tier: Open3D multi-view rendering of 3D models.
    /// Separated from VISION_GPU to avoid GPU contention between the
    /// EGL rendering engine and the Ollama vision LLM. Drains before
    /// VISION_GPU in the worker loop so all rendered views are available
    /// when vision description jobs start.
    pub const RENDER_GPU: i16 = 4;
    /// Audio GPU tier: whisper transcription + pyannote diarization (#576).
    /// When GPU_EXCLUSIVE_MODE is enabled, the worker starts sidecars before
    /// this tier and stops them after, freeing ~6.6 GB VRAM for Ollama tiers.
    pub const AUDIO_GPU: i16 = 5;
}

/// A job in the processing queue.
#[derive(Clone, Serialize, Deserialize)]
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
    /// Cost tier for tiered job scheduling. None = agnostic (legacy/backward compat).
    pub cost_tier: Option<i16>,
}

impl fmt::Debug for Job {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("id_set", &(!self.id.is_nil()))
            .field("note_id_set", &self.note_id.is_some())
            .field("job_type", &self.job_type)
            .field("status", &self.status)
            .field("priority", &self.priority)
            .field("payload_class", &self.payload.as_ref().map(job_json_class))
            .field(
                "payload_len",
                &self.payload.as_ref().map(json_serialized_len),
            )
            .field("result_class", &self.result.as_ref().map(job_json_class))
            .field("result_len", &self.result.as_ref().map(json_serialized_len))
            .field(
                "error_message_len",
                &self.error_message.as_ref().map(String::len),
            )
            .field("progress_percent", &self.progress_percent)
            .field(
                "progress_message_len",
                &self.progress_message.as_ref().map(String::len),
            )
            .field("retry_count", &self.retry_count)
            .field("max_retries", &self.max_retries)
            .field("created_at", &self.created_at)
            .field("started_at_set", &self.started_at.is_some())
            .field("completed_at_set", &self.completed_at.is_some())
            .field("cost_tier", &self.cost_tier)
            .finish()
    }
}

fn job_json_class(value: &JsonValue) -> &'static str {
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
    value.to_string().len()
}

/// Queue statistics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: i64,
    pub processing: i64,
    pub completed_last_hour: i64,
    pub failed_last_hour: i64,
    pub total: i64,
}

/// Job processing pause state for global and per-archive control (Issue #466).
///
/// Effective state: a job runs only if **both** global AND its archive are `RUNNING`.
/// If either is `PAUSED`, the job is skipped during claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPauseState {
    /// Global pause state: `"running"` or `"paused"`.
    pub global: String,
    /// Per-archive pause state. Only paused archives appear in this map.
    pub archives: HashMap<String, String>,
    /// Queue statistics with pause context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<JobPauseQueueStats>,
}

/// Queue statistics within the pause state response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPauseQueueStats {
    pub pending: i64,
    pub running: i64,
}

/// Extraction job statistics and analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionStats {
    pub total_jobs: i64,
    pub completed_jobs: i64,
    pub failed_jobs: i64,
    pub pending_jobs: i64,
    /// Average duration in seconds for completed extraction jobs.
    pub avg_duration_secs: Option<f64>,
    /// Count of jobs per extraction strategy.
    pub strategy_breakdown: HashMap<String, i64>,
}

// =============================================================================
// COLLECTION & TAG TYPES
// =============================================================================

/// A collection of notes (folder/hierarchy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Parent collection ID for nested hierarchy (None = root)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    pub created_at_utc: DateTime<Utc>,
    /// Number of notes in this collection (computed)
    #[serde(default)]
    pub note_count: i64,
}

/// A tag definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub created_at_utc: DateTime<Utc>,
    /// Number of notes with this tag (computed)
    #[serde(default)]
    pub note_count: i64,
}

/// Archive schema information for parallel memory archives.
///
/// Part of Epic #441: Parallel Memory Archives. Each archive maintains
/// an isolated PostgreSQL schema with its own tables for notes, embeddings,
/// collections, and tags.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArchiveInfo {
    pub id: Uuid,
    pub name: String,
    pub schema_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed: Option<DateTime<Utc>>,
    #[serde(default)]
    pub note_count: Option<i32>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
    #[serde(default)]
    pub is_default: bool,
    /// Schema version for auto-migration detection.
    /// Compared against current public schema table count to determine
    /// if the archive needs new tables from recent migrations.
    #[serde(default)]
    pub schema_version: i32,
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
// PROVENANCE TYPES (W3C PROV-DM)
// =============================================================================

/// W3C PROV relation types for provenance edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProvRelation {
    /// prov:wasDerivedFrom - the revision content was derived from a source note
    WasDerivedFrom,
    /// prov:used - the AI activity used content from a source note as context
    Used,
    /// prov:wasInformedBy - the activity was informed by another activity
    WasInformedBy,
    /// prov:wasGeneratedBy - the entity was generated by an activity
    WasGeneratedBy,
}

impl ProvRelation {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProvRelation::WasDerivedFrom => "wasDerivedFrom",
            ProvRelation::Used => "used",
            ProvRelation::WasInformedBy => "wasInformedBy",
            ProvRelation::WasGeneratedBy => "wasGeneratedBy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "wasDerivedFrom" => Some(ProvRelation::WasDerivedFrom),
            "used" => Some(ProvRelation::Used),
            "wasInformedBy" => Some(ProvRelation::WasInformedBy),
            "wasGeneratedBy" => Some(ProvRelation::WasGeneratedBy),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProvRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Edge in the provenance graph (W3C PROV Entity relationship).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceEdge {
    pub id: Uuid,
    pub revision_id: Uuid,
    pub source_note_id: Option<Uuid>,
    pub source_url: Option<String>,
    pub relation: String,
    pub created_at_utc: DateTime<Utc>,
}

/// W3C PROV Activity - tracks an AI processing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceActivity {
    pub id: Uuid,
    pub note_id: Uuid,
    pub revision_id: Option<Uuid>,
    pub activity_type: String,
    pub model_name: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

/// Complete provenance chain for a note's revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceChain {
    pub note_id: Uuid,
    pub revision_id: Uuid,
    pub activity: Option<ProvenanceActivity>,
    pub edges: Vec<ProvenanceEdge>,
}

/// Node in the revision tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionNode {
    pub id: Uuid,
    pub parent_revision_id: Option<Uuid>,
    pub created_at_utc: DateTime<Utc>,
}

// =============================================================================
// OAUTH2 TYPES
// =============================================================================

/// OAuth2 grant types supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrantType {
    AuthorizationCode,
    ClientCredentials,
    RefreshToken,
}

/// OAuth2 response types supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthResponseType {
    Code,
    Token,
}

/// Token endpoint authentication methods.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenAuthMethod {
    #[default]
    ClientSecretBasic,
    ClientSecretPost,
    None,
}

/// OAuth2 client registration (RFC 7591).
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub id: Uuid,
    pub client_id: String,
    pub client_name: String,
    pub client_uri: Option<String>,
    pub logo_uri: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub scope: String,
    pub token_endpoint_auth_method: String,
    pub software_id: Option<String>,
    pub software_version: Option<String>,
    pub contacts: Vec<String>,
    pub policy_uri: Option<String>,
    pub tos_uri: Option<String>,
    pub is_active: bool,
    pub is_confidential: bool,
    pub client_id_issued_at: DateTime<Utc>,
    pub client_secret_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for OAuthClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthClient")
            .field("id", &self.id)
            .field("client_id_len", &self.client_id.chars().count())
            .field("client_name_len", &self.client_name.chars().count())
            .field(
                "client_uri_len",
                &optional_debug_len(self.client_uri.as_ref()),
            )
            .field("logo_uri_len", &optional_debug_len(self.logo_uri.as_ref()))
            .field("redirect_uri_count", &self.redirect_uris.len())
            .field("grant_type_count", &self.grant_types.len())
            .field("response_type_count", &self.response_types.len())
            .field("scope_len", &self.scope.chars().count())
            .field(
                "token_endpoint_auth_method",
                &self.token_endpoint_auth_method,
            )
            .field(
                "software_id_len",
                &optional_debug_len(self.software_id.as_ref()),
            )
            .field(
                "software_version_len",
                &optional_debug_len(self.software_version.as_ref()),
            )
            .field("contact_count", &self.contacts.len())
            .field(
                "policy_uri_len",
                &optional_debug_len(self.policy_uri.as_ref()),
            )
            .field("tos_uri_len", &optional_debug_len(self.tos_uri.as_ref()))
            .field("is_active", &self.is_active)
            .field("is_confidential", &self.is_confidential)
            .field("client_id_issued_at", &self.client_id_issued_at)
            .field("client_secret_expires_at", &self.client_secret_expires_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// OAuth2 client registration request (RFC 7591).
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ClientRegistrationRequest {
    pub client_name: String,
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
    pub client_uri: Option<String>,
    pub logo_uri: Option<String>,
    pub contacts: Option<Vec<String>>,
    pub policy_uri: Option<String>,
    pub tos_uri: Option<String>,
    pub software_id: Option<String>,
    pub software_version: Option<String>,
    pub software_statement: Option<String>,
}

/// OAuth2 client registration response (RFC 7591).
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientRegistrationResponse {
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub client_id_issued_at: i64,
    pub client_secret_expires_at: i64,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub scope: String,
    pub token_endpoint_auth_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_client_uri: Option<String>,
}

/// OAuth2 authorization code.
#[derive(Clone)]
pub struct OAuthAuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub user_id: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// OAuth2 token.
#[derive(Clone)]
pub struct OAuthToken {
    pub id: Uuid,
    pub access_token_hash: String,
    pub refresh_token_hash: Option<String>,
    pub token_type: String,
    pub scope: String,
    pub client_id: String,
    pub user_id: Option<String>,
    pub access_token_expires_at: DateTime<Utc>,
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub created_at: DateTime<Utc>,
}

/// OAuth2 token request.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TokenRequest {
    pub grant_type: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

/// OAuth2 token response.
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

fn optional_debug_len(value: Option<&String>) -> Option<usize> {
    value.map(|value| value.chars().count())
}

impl std::fmt::Debug for ClientRegistrationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientRegistrationRequest")
            .field("client_name_len", &self.client_name.chars().count())
            .field("redirect_uri_count", &self.redirect_uris.len())
            .field("grant_types", &self.grant_types)
            .field("response_types", &self.response_types)
            .field("scope_len", &optional_debug_len(self.scope.as_ref()))
            .field(
                "token_endpoint_auth_method",
                &self.token_endpoint_auth_method,
            )
            .field(
                "client_uri_len",
                &optional_debug_len(self.client_uri.as_ref()),
            )
            .field("logo_uri_len", &optional_debug_len(self.logo_uri.as_ref()))
            .field("contact_count", &self.contacts.as_ref().map(Vec::len))
            .field(
                "policy_uri_len",
                &optional_debug_len(self.policy_uri.as_ref()),
            )
            .field("tos_uri_len", &optional_debug_len(self.tos_uri.as_ref()))
            .field(
                "software_id_len",
                &optional_debug_len(self.software_id.as_ref()),
            )
            .field(
                "software_version_len",
                &optional_debug_len(self.software_version.as_ref()),
            )
            .field("software_statement_set", &self.software_statement.is_some())
            .finish()
    }
}

impl std::fmt::Debug for ClientRegistrationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientRegistrationResponse")
            .field("client_id_len", &self.client_id.chars().count())
            .field("client_secret_set", &self.client_secret.is_some())
            .field("client_id_issued_at", &self.client_id_issued_at)
            .field("client_secret_expires_at", &self.client_secret_expires_at)
            .field("client_name_len", &self.client_name.chars().count())
            .field("redirect_uri_count", &self.redirect_uris.len())
            .field("grant_types", &self.grant_types)
            .field("response_types", &self.response_types)
            .field("scope_len", &self.scope.chars().count())
            .field(
                "token_endpoint_auth_method",
                &self.token_endpoint_auth_method,
            )
            .field(
                "registration_access_token_set",
                &self.registration_access_token.is_some(),
            )
            .field(
                "registration_client_uri_len",
                &optional_debug_len(self.registration_client_uri.as_ref()),
            )
            .finish()
    }
}

impl std::fmt::Debug for OAuthAuthorizationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthAuthorizationCode")
            .field("code_set", &!self.code.is_empty())
            .field("client_id_len", &self.client_id.chars().count())
            .field("redirect_uri_len", &self.redirect_uri.chars().count())
            .field("scope_len", &self.scope.chars().count())
            .field("state_set", &self.state.is_some())
            .field("code_challenge_set", &self.code_challenge.is_some())
            .field("code_challenge_method", &self.code_challenge_method)
            .field("user_id_len", &optional_debug_len(self.user_id.as_ref()))
            .field("expires_at", &self.expires_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl std::fmt::Debug for OAuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthToken")
            .field("id", &self.id)
            .field(
                "access_token_hash_len",
                &self.access_token_hash.chars().count(),
            )
            .field(
                "refresh_token_hash_len",
                &optional_debug_len(self.refresh_token_hash.as_ref()),
            )
            .field("token_type", &self.token_type)
            .field("scope_len", &self.scope.chars().count())
            .field("client_id_len", &self.client_id.chars().count())
            .field("user_id_len", &optional_debug_len(self.user_id.as_ref()))
            .field("access_token_expires_at", &self.access_token_expires_at)
            .field("refresh_token_expires_at", &self.refresh_token_expires_at)
            .field("revoked", &self.revoked)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl std::fmt::Debug for TokenRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRequest")
            .field("grant_type", &self.grant_type)
            .field("code_set", &self.code.is_some())
            .field(
                "redirect_uri_len",
                &optional_debug_len(self.redirect_uri.as_ref()),
            )
            .field("refresh_token_set", &self.refresh_token.is_some())
            .field("scope_len", &optional_debug_len(self.scope.as_ref()))
            .field("code_verifier_set", &self.code_verifier.is_some())
            .field(
                "client_id_len",
                &optional_debug_len(self.client_id.as_ref()),
            )
            .field("client_secret_set", &self.client_secret.is_some())
            .finish()
    }
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token_set", &!self.access_token.is_empty())
            .field("access_token_len", &self.access_token.chars().count())
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("refresh_token_set", &self.refresh_token.is_some())
            .field("scope_len", &optional_debug_len(self.scope.as_ref()))
            .finish()
    }
}

/// OAuth2 token introspection response (RFC 7662).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenIntrospectionResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

/// Token expiry information for warning headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExpiryInfo {
    /// Number of seconds until token expiry
    pub seconds_until_expiry: i64,
    /// Timestamp when token expires (ISO 8601)
    pub expires_at: DateTime<Utc>,
}

impl TokenExpiryInfo {
    /// Returns true if the token should trigger a warning (< 5 minutes remaining)
    pub fn should_warn(&self) -> bool {
        self.seconds_until_expiry < 300
    }
}

/// OAuth2 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
}

impl OAuthError {
    pub fn invalid_request(description: &str) -> Self {
        Self {
            error: "invalid_request".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn invalid_client(description: &str) -> Self {
        Self {
            error: "invalid_client".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn invalid_grant(description: &str) -> Self {
        Self {
            error: "invalid_grant".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn unauthorized_client(description: &str) -> Self {
        Self {
            error: "unauthorized_client".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn unsupported_grant_type(description: &str) -> Self {
        Self {
            error: "unsupported_grant_type".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn invalid_scope(description: &str) -> Self {
        Self {
            error: "invalid_scope".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn unsupported_response_type(description: &str) -> Self {
        Self {
            error: "unsupported_response_type".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }

    pub fn server_error(description: &str) -> Self {
        Self {
            error: "server_error".to_string(),
            error_description: Some(description.to_string()),
            error_uri: None,
        }
    }
}

/// OAuth2 authorization server metadata (RFC 8414).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub scopes_supported: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,
}

/// API key for simpler authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: String,
    pub rate_limit_per_minute: Option<i32>,
    pub rate_limit_per_hour: Option<i32>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: i64,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// API key creation request.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub expires_in_days: Option<i32>,
}

fn default_scope() -> String {
    crate::defaults::OAUTH_DEFAULT_SCOPE.to_string()
}

/// API key creation response (includes the actual key, shown only once).
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub api_key: String, // Full key, only shown once
    pub key_prefix: String,
    pub name: String,
    pub scope: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for CreateApiKeyResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateApiKeyResponse")
            .field("id", &self.id)
            .field("api_key_set", &!self.api_key.is_empty())
            .field("api_key_len", &self.api_key.chars().count())
            .field("key_prefix_len", &self.key_prefix.chars().count())
            .field("name_len", &self.name.chars().count())
            .field("scope_len", &self.scope.chars().count())
            .field("expires_at", &self.expires_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Authenticated principal (either OAuth client or API key).
#[derive(Clone)]
pub enum AuthPrincipal {
    OAuthClient {
        client_id: String,
        scope: String,
        user_id: Option<String>,
    },
    ApiKey {
        key_id: Uuid,
        scope: String,
    },
    Anonymous,
}

impl fmt::Debug for AuthPrincipal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthPrincipal::OAuthClient {
                client_id,
                scope,
                user_id,
            } => f
                .debug_struct("AuthPrincipal::OAuthClient")
                .field("client_id_len", &client_id.chars().count())
                .field("scope_count", &scope.split_whitespace().count())
                .field("scope_len", &scope.chars().count())
                .field(
                    "user_id_len",
                    &user_id.as_ref().map(|value| value.chars().count()),
                )
                .finish(),
            AuthPrincipal::ApiKey { key_id, scope } => f
                .debug_struct("AuthPrincipal::ApiKey")
                .field("key_id_set", &true)
                .field("scope_count", &scope.split_whitespace().count())
                .field("scope_len", &scope.chars().count())
                .field("key_id_version", &key_id.get_version_num())
                .finish(),
            AuthPrincipal::Anonymous => f.debug_struct("AuthPrincipal::Anonymous").finish(),
        }
    }
}

impl AuthPrincipal {
    /// Check if the principal has the required scope.
    ///
    /// Scope hierarchy: admin > write > read; MCP transport is separate.
    /// - `admin`: all operations
    /// - `write`: create, update, delete + read
    /// - `read`: list, get, search
    /// - `mcp`: MCP transport/session access only
    pub fn has_scope(&self, required: &str) -> bool {
        let scope = match self {
            AuthPrincipal::OAuthClient { scope, .. } => scope,
            AuthPrincipal::ApiKey { scope, .. } => scope,
            AuthPrincipal::Anonymous => return false,
        };

        // Check each granted scope against the hierarchy
        for granted in scope.split_whitespace() {
            match granted {
                "admin" => return true, // Admin has all permissions
                "mcp" if required == "mcp" => return true,
                "write" if required == "read" || required == "write" => {
                    // Write scope includes read.
                    return true;
                }
                s if s == required => return true,
                _ => {}
            }
        }

        false
    }

    /// Get the scope string for error messages.
    pub fn scope_str(&self) -> &str {
        match self {
            AuthPrincipal::OAuthClient { scope, .. } => scope,
            AuthPrincipal::ApiKey { scope, .. } => scope,
            AuthPrincipal::Anonymous => "none",
        }
    }

    /// Check if the principal is authenticated.
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, AuthPrincipal::Anonymous)
    }
}

// =============================================================================
// NOTE TEMPLATE TYPES
// =============================================================================

/// A reusable note template.
#[derive(Clone, Serialize, Deserialize)]
pub struct NoteTemplate {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<Uuid>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

impl fmt::Debug for NoteTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteTemplate")
            .field("id_set", &true)
            .field("name_len", &self.name.chars().count())
            .field(
                "description_len",
                &self.description.as_ref().map(|value| value.chars().count()),
            )
            .field("content_len", &self.content.chars().count())
            .field("format_len", &self.format.chars().count())
            .field("default_tags_count", &self.default_tags.len())
            .field(
                "default_tag_lens",
                &self
                    .default_tags
                    .iter()
                    .map(|tag| tag.chars().count())
                    .collect::<Vec<_>>(),
            )
            .field("collection_id_set", &self.collection_id.is_some())
            .field("created_at_utc", &self.created_at_utc)
            .field("updated_at_utc", &self.updated_at_utc)
            .finish()
    }
}

// =============================================================================
// MEMORY SEARCH TYPES (Issues #446, #437)
// =============================================================================

/// A memory result with temporal and spatial context.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemoryHit {
    /// Provenance record ID
    pub provenance_id: Uuid,
    /// Attachment ID
    pub attachment_id: Uuid,
    /// Associated note ID
    pub note_id: Uuid,
    /// Filename
    pub filename: String,
    /// Content type (MIME type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Capture time range (start/end for video, single instant for photo)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_time: Option<(DateTime<Utc>, Option<DateTime<Utc>>)>,
    /// Event type (photo, video, audio, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Event title/description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_title: Option<String>,
    /// Distance from query point (for spatial queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f64>,
    /// Named location name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_name: Option<String>,
}

impl fmt::Debug for MemoryHit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryHit")
            .field("provenance_id_set", &true)
            .field("attachment_id_set", &true)
            .field("note_id_set", &true)
            .field("filename_len", &self.filename.chars().count())
            .field(
                "content_type_len",
                &self
                    .content_type
                    .as_ref()
                    .map(|value| value.chars().count()),
            )
            .field("capture_time_set", &self.capture_time.is_some())
            .field(
                "event_type_len",
                &self.event_type.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "event_title_len",
                &self.event_title.as_ref().map(|value| value.chars().count()),
            )
            .field("distance_m_set", &self.distance_m.is_some())
            .field(
                "location_name_len",
                &self
                    .location_name
                    .as_ref()
                    .map(|value| value.chars().count()),
            )
            .finish()
    }
}

/// Memory search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemorySearchResponse {
    pub memories: Vec<MemoryHit>,
    pub total: usize,
}

impl fmt::Debug for MemorySearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemorySearchResponse")
            .field("memories_count", &self.memories.len())
            .field("total", &self.total)
            .finish()
    }
}

/// Timeline grouping response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TimelineResponse {
    pub groups: Vec<TimelineGroup>,
    pub total: usize,
}

impl fmt::Debug for TimelineResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimelineResponse")
            .field("groups_count", &self.groups.len())
            .field("total", &self.total)
            .finish()
    }
}

/// A group of memories within a time period.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TimelineGroup {
    /// Group period (e.g., "2024-01", "2024-W23", "2024-01-15")
    pub period: String,
    /// Start of period
    pub start: DateTime<Utc>,
    /// End of period
    pub end: DateTime<Utc>,
    /// Memories in this group
    pub memories: Vec<MemoryHit>,
    /// Count of memories in this group
    pub count: usize,
}

impl fmt::Debug for TimelineGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimelineGroup")
            .field("period_len", &self.period.chars().count())
            .field("start_set", &true)
            .field("end_set", &true)
            .field("memories_count", &self.memories.len())
            .field("count", &self.count)
            .finish()
    }
}

// =============================================================================
// CROSS-ARCHIVE SEARCH TYPES (Issue #446)
// =============================================================================

/// Cross-archive search request.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CrossArchiveSearchRequest {
    /// Search query
    pub query: String,
    /// Archive schemas to search (empty = all)
    #[serde(default)]
    pub archives: Vec<String>,
    /// Search mode (fts, vector, hybrid)
    #[serde(default)]
    pub mode: SearchMode,
    /// Maximum results per archive
    #[serde(default = "default_ca_limit")]
    pub limit: i64,
    /// Enable RRF fusion across archives
    #[serde(default)]
    pub enable_fusion: bool,
}

impl fmt::Debug for CrossArchiveSearchRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrossArchiveSearchRequest")
            .field("query_len", &self.query.chars().count())
            .field("archives_count", &self.archives.len())
            .field(
                "archive_lens",
                &self
                    .archives
                    .iter()
                    .map(|archive| archive.chars().count())
                    .collect::<Vec<_>>(),
            )
            .field("mode", &self.mode)
            .field("limit", &self.limit)
            .field("enable_fusion", &self.enable_fusion)
            .finish()
    }
}

fn default_ca_limit() -> i64 {
    crate::defaults::CROSS_ARCHIVE_LIMIT
}

/// Cross-archive search result.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CrossArchiveSearchResult {
    /// Archive name (schema)
    pub archive_name: String,
    /// Note ID
    pub note_id: Uuid,
    /// Search score (RRF score if fusion enabled)
    pub score: f32,
    /// Snippet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

impl fmt::Debug for CrossArchiveSearchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrossArchiveSearchResult")
            .field("archive_name_len", &self.archive_name.chars().count())
            .field("note_id_set", &true)
            .field("score", &self.score)
            .field(
                "snippet_len",
                &self.snippet.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "title_len",
                &self.title.as_ref().map(|value| value.chars().count()),
            )
            .field("tags_count", &self.tags.len())
            .field(
                "tag_lens",
                &self
                    .tags
                    .iter()
                    .map(|tag| tag.chars().count())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Cross-archive search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CrossArchiveSearchResponse {
    pub results: Vec<CrossArchiveSearchResult>,
    pub archives_searched: Vec<String>,
    pub total: usize,
}

impl fmt::Debug for CrossArchiveSearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrossArchiveSearchResponse")
            .field("results_count", &self.results.len())
            .field("archives_searched_count", &self.archives_searched.len())
            .field(
                "archives_searched_lens",
                &self
                    .archives_searched
                    .iter()
                    .map(|archive| archive.chars().count())
                    .collect::<Vec<_>>(),
            )
            .field("total", &self.total)
            .finish()
    }
}

// =============================================================================
// ATTACHMENT SEARCH TYPES (Issue #437)
// =============================================================================

/// Attachment search request.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AttachmentSearchRequest {
    /// Filter by note ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<Uuid>,
    /// Filter by content type (MIME type prefix, e.g., "image/", "video/")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Filter by event type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Filter by capture time range
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_after: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_before: Option<DateTime<Utc>>,
    /// Filter by location (radius search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near_lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near_lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_m: Option<f64>,
    /// Filter by named location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_name: Option<String>,
    /// Filter by device
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<Uuid>,
    /// Maximum results
    #[serde(default = "default_ca_limit")]
    pub limit: i64,
}

/// Attachment search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AttachmentSearchResponse {
    pub attachments: Vec<MemoryHit>,
    pub total: usize,
}

impl fmt::Debug for AttachmentSearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentSearchResponse")
            .field("attachments_count", &self.attachments.len())
            .field("total", &self.total)
            .finish()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_debug_excludes(debug: &str, secrets: &[&str]) {
        for secret in secrets {
            assert!(
                !debug.contains(secret),
                "debug output leaked secret `{secret}`: {debug}"
            );
        }
    }

    #[test]
    fn note_models_debug_redacts_content_metadata_and_tags() {
        let now = Utc::now();
        let meta = NoteMeta {
            id: Uuid::new_v4(),
            collection_id: Some(Uuid::new_v4()),
            format: "markdown-private-format".to_string(),
            source: "https://source.example.test/private?token=secret-token".to_string(),
            created_at_utc: now,
            updated_at_utc: now,
            starred: true,
            archived: false,
            last_accessed_at: Some(now),
            access_count: 7,
            title: Some("Private title private@example.test".to_string()),
            metadata: json!({
                "provider_url": "https://provider.example.test/v1?token=secret-token",
                "api_key": "sk-live-secret"
            }),
            chunk_metadata: Some(json!({
                "path": "/tmp/customer/chunk.json",
                "snippet": "Patient said 555-1212"
            })),
            document_type_id: Some(Uuid::new_v4()),
        };
        let original = NoteOriginal {
            content: "Original note content private@example.test sk-live-secret".to_string(),
            hash: "sha256-secret-hash-value".to_string(),
            user_created_at: Some(now),
            user_last_edited_at: Some(now),
        };
        let revised = NoteRevised {
            content: "Revised generated content contains 555-1212".to_string(),
            last_revision_id: Some(Uuid::new_v4()),
            ai_metadata: Some(json!({
                "model": "private-model-name",
                "response": "generated private@example.test"
            })),
            ai_generated_at: Some(now),
            user_last_edited_at: Some(now),
            is_user_edited: true,
            generation_count: 3,
            model: Some("private-model-name".to_string()),
        };
        let full = NoteFull {
            note: meta.clone(),
            original: original.clone(),
            revised: revised.clone(),
            tags: vec!["secret-tag-private@example.test".to_string()],
            concepts: Vec::new(),
            links: Vec::new(),
        };

        let debug = format!("{meta:?}{original:?}{revised:?}{full:?}");

        assert_debug_excludes(
            &debug,
            &[
                "markdown-private-format",
                "source.example.test",
                "secret-token",
                "Private title",
                "provider.example.test",
                "sk-live-secret",
                "/tmp/customer/chunk.json",
                "Patient said",
                "private@example.test",
                "Original note content",
                "sha256-secret-hash-value",
                "Revised generated content",
                "555-1212",
                "private-model-name",
                "generated private@example.test",
                "secret-tag-private@example.test",
            ],
        );

        for expected in [
            "title_len",
            "metadata_class",
            "chunk_metadata_len",
            "content_len",
            "hash_len",
            "ai_metadata_class",
            "model_len",
            "tags_count",
            "concepts_count",
            "links_count",
        ] {
            assert!(
                debug.contains(expected),
                "Note Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn note_revision_and_summary_debug_redacts_content_labels_and_metadata() {
        let now = Utc::now();
        let concept = NoteConceptSummary {
            concept_id: Uuid::new_v4(),
            notation: Some("secret-concept-notation-private@example.test".to_string()),
            pref_label: Some("Sensitive concept label 555-1212".to_string()),
            source: "https://taxonomy.example.test/private?token=secret-token".to_string(),
            confidence: Some(0.97),
            relevance_score: 0.86,
            is_primary: true,
        };
        let revision = RevisionVersion {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            revision_number: 4,
            content: "Revised note content includes sk-live-secret and /tmp/customer/file.md"
                .to_string(),
            revision_type: "ai-private-rewrite".to_string(),
            summary: Some("Private summary for private@example.test".to_string()),
            rationale: Some(
                "Rationale mentions https://provider.example.test/?token=secret".to_string(),
            ),
            created_at_utc: now,
            model: Some("private-model-name".to_string()),
            is_user_edited: true,
        };
        let summary = NoteSummary {
            id: Uuid::new_v4(),
            title: "Secret note title private@example.test".to_string(),
            snippet: "Snippet exposes 555-1212 and sk-live-secret".to_string(),
            embedding_status: Some(EmbeddingStatus::Ready),
            created_at_utc: now,
            updated_at_utc: now,
            starred: true,
            archived: false,
            tags: vec![
                "secret-tag-private@example.test".to_string(),
                "https://tags.example.test/?token=secret".to_string(),
            ],
            has_revision: true,
            metadata: json!({
                "provider_url": "https://provider.example.test/v1?token=secret-token",
                "path": "/tmp/customer/private-note.md",
                "api_key": "sk-live-secret"
            }),
            document_type_id: Some(Uuid::new_v4()),
            document_type_name: Some("Private document type".to_string()),
        };

        let debug = format!("{concept:?}{revision:?}{summary:?}");

        assert_debug_excludes(
            &debug,
            &[
                "secret-concept-notation",
                "Sensitive concept label",
                "taxonomy.example.test",
                "secret-token",
                "Revised note content",
                "sk-live-secret",
                "/tmp/customer/file.md",
                "ai-private-rewrite",
                "Private summary",
                "private@example.test",
                "Rationale mentions",
                "provider.example.test",
                "private-model-name",
                "Secret note title",
                "Snippet exposes",
                "555-1212",
                "secret-tag-private@example.test",
                "tags.example.test",
                "/tmp/customer/private-note.md",
                "Private document type",
            ],
        );

        for expected in [
            "concept_id_set",
            "notation_len",
            "pref_label_len",
            "source_len",
            "content_len",
            "revision_type_len",
            "summary_len",
            "rationale_len",
            "model_len",
            "title_len",
            "snippet_len",
            "tags_count",
            "metadata_class",
            "metadata_len",
            "document_type_id_set",
            "document_type_name_len",
        ] {
            assert!(
                debug.contains(expected),
                "Note revision/summary Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn link_and_search_debug_redacts_urls_snippets_tags_and_warnings() {
        let now = Utc::now();
        let link = Link {
            id: Uuid::new_v4(),
            from_note_id: Uuid::new_v4(),
            to_note_id: Some(Uuid::new_v4()),
            to_url: Some("https://links.example.test/private?token=secret-token".to_string()),
            kind: "private-reference-kind".to_string(),
            score: 0.74,
            created_at_utc: now,
            snippet: Some("Linked snippet includes private@example.test and 555-1212".to_string()),
            metadata: Some(json!({
                "path": "/tmp/customer/link.md",
                "api_key": "sk-live-secret",
                "source_url": "https://provider.example.test/?token=secret"
            })),
        };
        let hit = SearchHit {
            note_id: Uuid::new_v4(),
            score: 0.93,
            snippet: Some(
                "Search snippet includes sk-live-secret and /tmp/customer/result.md".to_string(),
            ),
            title: Some("Private search title private@example.test".to_string()),
            tags: vec![
                "secret-tag-private@example.test".to_string(),
                "https://tags.example.test/?token=secret".to_string(),
            ],
            embedding_status: Some(EmbeddingStatus::Ready),
        };
        let response = SearchResponse {
            notes: vec![hit.clone()],
            semantic_available: Some(true),
            warnings: vec![
                "Search warning includes https://warn.example.test/?token=secret".to_string(),
            ],
        };
        let semantic = SemanticResponse { similar: vec![hit] };

        let debug = format!("{link:?}{response:?}{semantic:?}");

        assert_debug_excludes(
            &debug,
            &[
                "links.example.test",
                "secret-token",
                "private-reference-kind",
                "Linked snippet",
                "private@example.test",
                "555-1212",
                "/tmp/customer/link.md",
                "sk-live-secret",
                "provider.example.test",
                "Search snippet",
                "/tmp/customer/result.md",
                "Private search title",
                "secret-tag-private@example.test",
                "tags.example.test",
                "Search warning",
                "warn.example.test",
            ],
        );

        for expected in [
            "to_url_len",
            "kind_len",
            "snippet_len",
            "metadata_class",
            "metadata_len",
            "notes_count",
            "warnings_count",
            "similar_count",
        ] {
            assert!(
                debug.contains(expected),
                "Link/search Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn attachment_debug_redacts_paths_filenames_text_and_metadata() {
        let now = Utc::now();
        let blob = AttachmentBlob {
            id: Uuid::new_v4(),
            content_hash: "sha256-secret-content-hash-private@example.test".to_string(),
            content_type: "application/private-report".to_string(),
            size_bytes: 4096,
            storage_backend: "customer-storage-secret".to_string(),
            storage_path: Some("/tmp/customer/private/sk-live-secret/report.pdf".to_string()),
            reference_count: 2,
            created_at: now,
        };
        let attachment = Attachment {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            blob_id: Uuid::new_v4(),
            filename: "private-report-sk-live-secret.pdf".to_string(),
            original_filename: Some("original-private@example.test.pdf".to_string()),
            document_type_id: Some(Uuid::new_v4()),
            status: AttachmentStatus::Completed,
            extraction_strategy: Some(ExtractionStrategy::PdfOcr),
            extracted_text: Some(
                "Extracted text includes 555-1212 and /tmp/customer/raw.txt".to_string(),
            ),
            extracted_metadata: Some(json!({
                "provider_url": "https://provider.example.test/?token=secret",
                "api_key": "sk-live-secret",
                "path": "/tmp/customer/raw.txt"
            })),
            ai_description: Some("AI description includes private@example.test".to_string()),
            ai_model: Some("private-model-name".to_string()),
            has_preview: true,
            is_canonical_content: true,
            detected_document_type_id: Some(Uuid::new_v4()),
            detection_confidence: Some(0.88),
            detection_method: Some("private-filename-pattern".to_string()),
            created_at: now,
            updated_at: now,
        };
        let summary = AttachmentSummary {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            filename: "summary-secret-file.pdf".to_string(),
            content_type: "application/private-summary".to_string(),
            size_bytes: 1024,
            status: AttachmentStatus::Processing,
            document_type_name: Some("Private document type".to_string()),
            detected_document_type_name: Some("Detected private type".to_string()),
            detection_confidence: Some(0.77),
            has_preview: false,
            is_canonical_content: false,
            created_at: now,
        };
        let global = GlobalAttachmentSummary {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            note_title: Some("Private note title private@example.test".to_string()),
            filename: "global-secret-file.pdf".to_string(),
            content_type: "application/private-global".to_string(),
            size_bytes: 2048,
            status: AttachmentStatus::Completed,
            document_type_name: Some("Global private document type".to_string()),
            detected_document_type_name: Some("Global detected private type".to_string()),
            detection_confidence: Some(0.79),
            has_preview: true,
            is_canonical_content: false,
            created_at: now,
        };

        let debug = format!("{blob:?}{attachment:?}{summary:?}{global:?}");

        assert_debug_excludes(
            &debug,
            &[
                "sha256-secret-content-hash",
                "private@example.test",
                "application/private-report",
                "customer-storage-secret",
                "/tmp/customer/private",
                "sk-live-secret",
                "private-report",
                "original-private",
                "Extracted text",
                "555-1212",
                "/tmp/customer/raw.txt",
                "provider.example.test",
                "AI description",
                "private-model-name",
                "private-filename-pattern",
                "summary-secret-file",
                "Private document type",
                "Detected private type",
                "Private note title",
                "global-secret-file",
                "Global private document type",
                "Global detected private type",
            ],
        );

        for expected in [
            "content_hash_len",
            "content_type_len",
            "storage_backend_len",
            "storage_path_len",
            "filename_len",
            "original_filename_len",
            "extracted_text_len",
            "extracted_metadata_class",
            "extracted_metadata_len",
            "ai_description_len",
            "ai_model_len",
            "document_type_name_len",
            "detected_document_type_name_len",
            "note_title_len",
        ] {
            assert!(
                debug.contains(expected),
                "Attachment Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn tus_upload_debug_redacts_paths_filenames_and_metadata() {
        let now = Utc::now();
        let upload = TusUpload {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            filename: "private-upload-sk-live-secret.pdf".to_string(),
            content_type: "application/private-upload".to_string(),
            total_size: 8192,
            current_offset: 4096,
            storage_path: "/tmp/customer/uploads/private@example.test/sk-live-secret.part"
                .to_string(),
            metadata: json!({
                "original_filename": "customer-private-file.pdf",
                "callback_url": "https://provider.example.test/upload?token=secret-token",
                "api_key": "sk-live-secret",
                "phone": "555-1212",
                "storage_path": "/tmp/customer/uploads/raw.bin"
            }),
            created_at: now,
            updated_at: now,
            expires_at: now,
        };

        let debug = format!("{upload:?}");

        assert_debug_excludes(
            &debug,
            &[
                "private-upload",
                "sk-live-secret",
                "application/private-upload",
                "/tmp/customer/uploads",
                "private@example.test",
                "customer-private-file",
                "provider.example.test",
                "secret-token",
                "555-1212",
                "raw.bin",
            ],
        );

        for expected in [
            "id_set",
            "note_id_set",
            "filename_len",
            "content_type_len",
            "total_size",
            "current_offset",
            "storage_path_len",
            "metadata_class",
            "metadata_len",
            "created_at",
            "updated_at",
            "expires_at",
        ] {
            assert!(
                debug.contains(expected),
                "TusUpload Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn call_session_debug_redacts_provider_parties_reasons_and_metadata() {
        let now = Utc::now();
        let session = CallSession {
            call_id: Uuid::new_v4(),
            provider: "twilio-private-provider".to_string(),
            provider_call_id: "CA-secret-provider-call-id".to_string(),
            started_at: now,
            ended_at: Some(now),
            end_reason: Some(
                "Ended after callback https://provider.example.test/?token=secret".to_string(),
            ),
            asr_backend: Some("deepgram-private-model".to_string()),
            remote_party: Some("+15551212 private@example.test".to_string()),
            archive_id: Some(Uuid::new_v4()),
            metadata: json!({
                "recording_url": "https://recordings.example.test/call?token=secret-token",
                "storage_path": "/tmp/customer/calls/private.wav",
                "api_key": "sk-live-secret"
            }),
        };
        let create = CreateCallSessionRequest {
            provider: "create-private-provider".to_string(),
            provider_call_id: "CA-create-secret-call-id".to_string(),
            started_at: Some(now),
            asr_backend: Some("create-private-asr".to_string()),
            remote_party: Some("+15559876 caller@example.test".to_string()),
            archive_id: Some(Uuid::new_v4()),
            metadata: json!({
                "webhook_url": "https://hooks.example.test/call?token=create-secret",
                "customer_path": "/tmp/customer/create-call.json"
            }),
        };
        let update = UpdateCallSessionRequest {
            ended_at: Some(now),
            end_reason: Some(
                "Update reason includes sk-live-secret and /tmp/customer/end.txt".to_string(),
            ),
            asr_backend: Some("updated-private-asr".to_string()),
            remote_party: Some("+15550000 updated@example.test".to_string()),
            archive_id: Some(Uuid::new_v4()),
            metadata: Some(json!({
                "provider_error": "https://provider.example.test/error?token=update-secret",
                "transcript_path": "/tmp/customer/transcript.txt"
            })),
        };

        let debug = format!("{session:?}{create:?}{update:?}");

        assert_debug_excludes(
            &debug,
            &[
                "twilio-private-provider",
                "CA-secret-provider-call-id",
                "Ended after callback",
                "provider.example.test",
                "deepgram-private-model",
                "+15551212",
                "private@example.test",
                "recordings.example.test",
                "secret-token",
                "/tmp/customer/calls",
                "sk-live-secret",
                "create-private-provider",
                "CA-create-secret-call-id",
                "create-private-asr",
                "+15559876",
                "caller@example.test",
                "hooks.example.test",
                "create-secret",
                "/tmp/customer/create-call.json",
                "Update reason",
                "/tmp/customer/end.txt",
                "updated-private-asr",
                "+15550000",
                "updated@example.test",
                "update-secret",
                "/tmp/customer/transcript.txt",
            ],
        );

        for expected in [
            "call_id_set",
            "provider_len",
            "provider_call_id_len",
            "started_at",
            "started_at_set",
            "ended_at_set",
            "end_reason_len",
            "asr_backend_len",
            "remote_party_len",
            "archive_id_set",
            "metadata_class",
            "metadata_len",
        ] {
            assert!(
                debug.contains(expected),
                "CallSession Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn entity_and_finetuning_debug_redacts_content_models_and_vectors() {
        let now = Utc::now();
        let entity = NoteEntity {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            entity_text: "Private Person private@example.test 555-1212".to_string(),
            entity_type: EntityType::Person,
            start_offset: Some(12),
            end_offset: Some(42),
            confidence: Some(0.93),
            normalized_text: Some("normalized-private-person-sk-live-secret".to_string()),
            created_at: now,
        };
        let stats = EntityStats {
            entity_text: "Private Organization https://provider.example.test/?token=secret"
                .to_string(),
            doc_frequency: 7,
            idf_score: Some(1.25),
            last_updated: now,
        };
        let graph = NoteGraphEmbedding {
            note_id: Uuid::new_v4(),
            vector: Vector::from(vec![0.12345, 0.67891, 0.22222]),
            entity_count: 3,
            entity_types: vec![
                "private-person-type".to_string(),
                "secret-org-type@example.test".to_string(),
            ],
            created_at: now,
            updated_at: now,
        };
        let config = FineTuningConfig {
            queries_per_doc: 4,
            query_generator_model: Some("private-query-model-sk-live-secret".to_string()),
            quality_filter_model: Some("private-quality-model".to_string()),
            min_quality_score: 3.5,
            include_hard_negatives: true,
            validation_split: 0.2,
        };
        let dataset = FineTuningDataset {
            id: Uuid::new_v4(),
            name: "Private dataset private@example.test".to_string(),
            description: Some("Dataset description includes /tmp/customer/source.md".to_string()),
            source_type: "private-source-type".to_string(),
            source_id: "source-id-token-secret".to_string(),
            config: config.clone(),
            status: FineTuningStatus::Failed,
            sample_count: 10,
            training_count: 8,
            validation_count: 2,
            created_at: now,
            completed_at: Some(now),
            error_message: Some(
                "Provider error at https://provider.example.test/?token=secret".to_string(),
            ),
        };
        let sample = FineTuningSample {
            id: Uuid::new_v4(),
            dataset_id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            query: "Generated query asks about private@example.test and sk-live-secret".to_string(),
            query_type: Some("private-query-type".to_string()),
            quality_score: Some(4.2),
            is_validation: true,
            created_at: now,
        };
        let request = CreateFineTuningDatasetRequest {
            name: "Create private dataset".to_string(),
            description: Some(
                "Create description https://create.example.test/?token=secret".to_string(),
            ),
            source_type: "tag".to_string(),
            source_id: "private-tag/source-id".to_string(),
            config,
        };

        let debug = format!("{entity:?}{stats:?}{graph:?}{dataset:?}{sample:?}{request:?}");

        assert_debug_excludes(
            &debug,
            &[
                "Private Person",
                "private@example.test",
                "555-1212",
                "normalized-private-person",
                "sk-live-secret",
                "Private Organization",
                "provider.example.test",
                "secret",
                "0.12345",
                "0.67891",
                "0.22222",
                "private-person-type",
                "secret-org-type",
                "private-query-model",
                "private-quality-model",
                "Private dataset",
                "/tmp/customer/source.md",
                "private-source-type",
                "source-id-token-secret",
                "Provider error",
                "Generated query",
                "private-query-type",
                "Create private dataset",
                "create.example.test",
                "private-tag/source-id",
            ],
        );

        for expected in [
            "entity_text_len",
            "normalized_text_len",
            "vector_dimensions",
            "entity_types_count",
            "entity_type_lens",
            "query_generator_model_len",
            "quality_filter_model_len",
            "name_len",
            "description_len",
            "source_id_len",
            "error_message_len",
            "query_len",
            "query_type_len",
        ] {
            assert!(
                debug.contains(expected),
                "Entity/fine-tuning Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn transcript_segment_debug_redacts_text_and_speaker_labels() {
        let now = Utc::now();
        let segment = TranscriptSegment {
            id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            speaker_label: Some("Private speaker private@example.test".to_string()),
            text: "Transcript includes 555-1212, sk-live-secret, and /tmp/customer/audio.wav"
                .to_string(),
            start_ts: Some(1.25),
            end_ts: Some(3.5),
            confidence: Some(0.91),
            sequence: 8,
            created_at: now,
        };
        let request = CreateTranscriptSegmentRequest {
            call_id: Uuid::new_v4(),
            speaker_label: Some("Caller with secret-token label".to_string()),
            text: "Request transcript mentions https://provider.example.test/?token=secret"
                .to_string(),
            start_ts: Some(5.0),
            end_ts: Some(8.0),
            confidence: Some(0.82),
            sequence: 9,
        };

        let debug = format!("{segment:?}{request:?}");

        assert_debug_excludes(
            &debug,
            &[
                "Private speaker",
                "private@example.test",
                "Transcript includes",
                "555-1212",
                "sk-live-secret",
                "/tmp/customer/audio.wav",
                "Caller with",
                "secret-token",
                "Request transcript",
                "provider.example.test",
            ],
        );

        for expected in [
            "speaker_label_len",
            "text_len",
            "start_ts_set",
            "end_ts_set",
            "confidence",
            "sequence",
            "created_at",
        ] {
            assert!(
                debug.contains(expected),
                "Transcript Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn oauth_model_debug_redacts_secret_material() {
        let now = Utc::now();
        let oauth_client = OAuthClient {
            id: Uuid::new_v4(),
            client_id: "oauth-client-secret-id".to_string(),
            client_name: "Sensitive OAuth Client secret label".to_string(),
            client_uri: Some("https://client.example/client-secret-path".to_string()),
            logo_uri: Some("https://client.example/logo-secret.png".to_string()),
            redirect_uris: vec!["https://client.example/callback?code=secret".to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            scope: "read write client-secret-scope".to_string(),
            token_endpoint_auth_method: "client_secret_post".to_string(),
            software_id: Some("software-secret-id".to_string()),
            software_version: Some("software-secret-version".to_string()),
            contacts: vec!["security-secret@example.com".to_string()],
            policy_uri: Some("https://client.example/policy?token=secret".to_string()),
            tos_uri: Some("https://client.example/tos?token=secret".to_string()),
            is_active: true,
            is_confidential: true,
            client_id_issued_at: now,
            client_secret_expires_at: Some(now),
            created_at: now,
        };
        let client_registration = ClientRegistrationRequest {
            client_name: "Sensitive OAuth Client".to_string(),
            redirect_uris: vec!["https://client.example/callback?code=secret".to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            scope: Some("read write".to_string()),
            token_endpoint_auth_method: Some("client_secret_post".to_string()),
            client_uri: Some("https://client.example/client-secret-path".to_string()),
            logo_uri: Some("https://client.example/logo-secret.png".to_string()),
            contacts: Some(vec!["security-secret@example.com".to_string()]),
            policy_uri: Some("https://client.example/policy?token=secret".to_string()),
            tos_uri: Some("https://client.example/tos?token=secret".to_string()),
            software_id: Some("software-secret-id".to_string()),
            software_version: Some("software-secret-version".to_string()),
            software_statement: Some("signed-software-statement-secret".to_string()),
        };
        let client_response = ClientRegistrationResponse {
            client_id: "oauth-client-secret-id".to_string(),
            client_secret: Some("client_secret_raw_value".to_string()),
            client_id_issued_at: 1,
            client_secret_expires_at: 0,
            client_name: "Sensitive OAuth Client".to_string(),
            redirect_uris: vec!["https://client.example/callback?code=secret".to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            scope: "read write".to_string(),
            token_endpoint_auth_method: "client_secret_post".to_string(),
            registration_access_token: Some("registration_access_token_raw".to_string()),
            registration_client_uri: Some(
                "https://issuer.example/oauth/register/client".to_string(),
            ),
        };
        let auth_code = OAuthAuthorizationCode {
            code: "authorization_code_secret".to_string(),
            client_id: "oauth-client-secret-id".to_string(),
            redirect_uri: "https://client.example/callback?code=secret".to_string(),
            scope: "read write".to_string(),
            state: Some("state-secret".to_string()),
            code_challenge: Some("pkce-challenge-secret".to_string()),
            code_challenge_method: Some("S256".to_string()),
            user_id: Some("user-secret-id".to_string()),
            expires_at: now,
            created_at: now,
        };
        let oauth_token = OAuthToken {
            id: Uuid::new_v4(),
            access_token_hash: "hashed-mm_at_secret_value".to_string(),
            refresh_token_hash: Some("hashed-mm_rt_secret_value".to_string()),
            token_type: "Bearer".to_string(),
            scope: "read write".to_string(),
            client_id: "oauth-client-secret-id".to_string(),
            user_id: Some("user-secret-id".to_string()),
            access_token_expires_at: now,
            refresh_token_expires_at: Some(now),
            revoked: false,
            created_at: now,
        };
        let token_request = TokenRequest {
            grant_type: "authorization_code".to_string(),
            code: Some("authorization_code_secret".to_string()),
            redirect_uri: Some("https://client.example/callback?code=secret".to_string()),
            refresh_token: Some("mm_rt_refresh_secret".to_string()),
            scope: Some("read write".to_string()),
            code_verifier: Some("pkce-verifier-secret".to_string()),
            client_id: Some("oauth-client-secret-id".to_string()),
            client_secret: Some("client_secret_raw_value".to_string()),
        };
        let token_response = TokenResponse {
            access_token: "mm_at_access_secret".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            refresh_token: Some("mm_rt_refresh_secret".to_string()),
            scope: Some("read write".to_string()),
        };

        let debug = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            oauth_client,
            client_registration,
            client_response,
            auth_code,
            oauth_token,
            token_request,
            token_response
        );

        assert_debug_excludes(
            &debug,
            &[
                "client_secret_raw_value",
                "registration_access_token_raw",
                "authorization_code_secret",
                "hashed-mm_at_secret_value",
                "hashed-mm_rt_secret_value",
                "mm_rt_refresh_secret",
                "pkce-verifier-secret",
                "pkce-challenge-secret",
                "signed-software-statement-secret",
                "state-secret",
                "https://client.example",
                "oauth-client-secret-id",
                "Sensitive OAuth Client secret label",
                "client-secret-scope",
                "software-secret-id",
                "software-secret-version",
                "security-secret@example.com",
            ],
        );
        assert!(debug.contains("client_name_len"));
        assert!(debug.contains("redirect_uri_count"));
        assert!(debug.contains("client_secret_set"));
        assert!(debug.contains("access_token_set"));
        assert!(debug.contains("refresh_token_hash_len"));
    }

    #[test]
    fn api_key_creation_response_debug_redacts_one_time_key() {
        let response = CreateApiKeyResponse {
            id: Uuid::new_v4(),
            api_key: "mm_key_super_secret_once".to_string(),
            key_prefix: "mm_key_super".to_string(),
            name: "Production key".to_string(),
            scope: "admin".to_string(),
            expires_at: Some(Utc::now()),
            created_at: Utc::now(),
        };

        let debug = format!("{response:?}");

        assert_debug_excludes(
            &debug,
            &["mm_key_super_secret_once", "mm_key_super", "Production key"],
        );
        assert!(debug.contains("api_key_set"));
        assert!(debug.contains("key_prefix_len"));
    }

    #[test]
    fn job_debug_redacts_payload_result_and_status_text() {
        let now = Utc::now();
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::AiRevision,
            status: JobStatus::Failed,
            priority: 10,
            payload: Some(json!({
                "prompt": "revise private@example.test using sk-live-secret",
                "provider_url": "https://provider.example.test/v1?token=secret-token"
            })),
            result: Some(json!({
                "generated": "Patient transcript said 555-1212",
                "storage_path": "/tmp/customer/audio.wav"
            })),
            error_message: Some(
                "provider failed at https://provider.example.test with sk-live-secret".to_string(),
            ),
            progress_percent: 42,
            progress_message: Some("processing private transcript chunk".to_string()),
            retry_count: 2,
            max_retries: 5,
            created_at: now,
            started_at: Some(now),
            completed_at: Some(now),
            cost_tier: Some(cost_tier::STANDARD_GPU),
        };

        let debug = format!("{job:?}");

        assert_debug_excludes(
            &debug,
            &[
                "private@example.test",
                "sk-live-secret",
                "provider.example.test",
                "secret-token",
                "Patient transcript",
                "555-1212",
                "/tmp/customer/audio.wav",
                "processing private transcript chunk",
            ],
        );

        for expected in [
            "payload_class",
            "payload_len",
            "result_class",
            "result_len",
            "error_message_len",
            "progress_message_len",
            "note_id_set",
        ] {
            assert!(
                debug.contains(expected),
                "Job Debug output should retain safe metadata field {expected:?}: {debug}"
            );
        }
    }

    #[test]
    fn webhook_model_debug_redacts_secret_material() {
        let now = Utc::now();
        let webhook = Webhook {
            id: Uuid::new_v4(),
            url: "https://hooks.example/tenant/path?token=webhook-url-secret".to_string(),
            secret: Some("outbound-webhook-signing-secret".to_string()),
            events: vec!["note.created.secret".to_string()],
            is_active: true,
            created_at: now,
            updated_at: now,
            last_triggered_at: Some(now),
            failure_count: 1,
            max_retries: 3,
        };
        let delivery = WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id: webhook.id,
            event_type: "incoming_webhook.received.secret".to_string(),
            payload: json!({
                "token": "payload-secret",
                "url": "https://provider.example/callback?api_key=payload-secret"
            }),
            status_code: Some(500),
            response_body: Some("provider response body secret".to_string()),
            delivered_at: now,
            success: false,
        };
        let create = CreateWebhookRequest {
            url: "https://hooks.example/create?token=create-url-secret".to_string(),
            secret: Some("create-webhook-secret".to_string()),
            events: vec!["note.updated.secret".to_string()],
            max_retries: 5,
        };
        let incoming = CreateIncomingWebhookReceiverRequest {
            slug: "sensitive-slug".to_string(),
            provider: "sensitive-provider".to_string(),
            schema_ref: "custom.secret.schema".to_string(),
            hmac_secret: "incoming-hmac-secret".to_string(),
            signature_header: "X-Secret-Signature".to_string(),
            is_active: true,
            schema_doc: Some(json!({"secret": "schema-doc-secret"})),
        };
        let receiver = IncomingWebhookReceiver {
            id: Uuid::new_v4(),
            slug: "stored-sensitive-slug".to_string(),
            provider: "stored-sensitive-provider".to_string(),
            schema_ref: "stored.custom.secret.schema".to_string(),
            signature_header: "X-Stored-Secret-Signature".to_string(),
            secret_set: true,
            is_active: true,
            schema_doc: Some(json!({"stored_secret": "stored-schema-doc-secret"})),
            created_at: now,
            updated_at: now,
        };
        let update = UpdateIncomingWebhookReceiverRequest {
            schema_ref: Some("updated.custom.secret.schema".to_string()),
            schema_doc: Some(json!({"updated_secret": "updated-schema-doc-secret"})),
            signature_header: Some("X-Updated-Secret-Signature".to_string()),
            is_active: Some(false),
        };
        let validation_request = ValidateIncomingWebhookPayloadRequest {
            schema_ref: "validate.custom.secret.schema".to_string(),
            payload: json!({
                "token": "validation-payload-secret",
                "url": "https://provider.example/validate?api_key=validation-secret"
            }),
        };
        let validation_response = IncomingWebhookValidationResponse {
            valid: false,
            schema_ref: "response.custom.secret.schema".to_string(),
            errors: vec![
                "validation error leaked token validation-response-secret".to_string(),
                "payload.url contained https://provider.example/error?api_key=secret".to_string(),
            ],
        };

        let debug = format!(
            "{webhook:?}{delivery:?}{create:?}{incoming:?}{receiver:?}{update:?}{validation_request:?}{validation_response:?}"
        );

        assert_debug_excludes(
            &debug,
            &[
                "webhook-url-secret",
                "outbound-webhook-signing-secret",
                "note.created.secret",
                "incoming_webhook.received.secret",
                "payload-secret",
                "provider response body secret",
                "create-url-secret",
                "create-webhook-secret",
                "note.updated.secret",
                "sensitive-slug",
                "sensitive-provider",
                "custom.secret.schema",
                "incoming-hmac-secret",
                "X-Secret-Signature",
                "schema-doc-secret",
                "stored-sensitive-slug",
                "stored-sensitive-provider",
                "stored.custom.secret.schema",
                "X-Stored-Secret-Signature",
                "stored-schema-doc-secret",
                "updated.custom.secret.schema",
                "updated-schema-doc-secret",
                "X-Updated-Secret-Signature",
                "validate.custom.secret.schema",
                "validation-payload-secret",
                "validation-secret",
                "response.custom.secret.schema",
                "validation-response-secret",
                "https://hooks.example",
                "https://provider.example",
            ],
        );
        assert!(debug.contains("secret_set"));
        assert!(debug.contains("hmac_secret_set"));
        assert!(debug.contains("payload_class"));
        assert!(debug.contains("response_body_len"));
        assert!(debug.contains("schema_doc_class"));
        assert!(debug.contains("schema_doc_len"));
        assert!(debug.contains("schema_ref_len"));
        assert!(debug.contains("error_count"));
        assert!(debug.contains("error_lens"));
    }

    #[test]
    fn inbound_source_debug_redacts_config_and_identifiers() {
        let now = Utc::now();
        let source = InboundSource {
            id: Uuid::new_v4(),
            name: "tenant-secret-inbound-source".to_string(),
            kind: "sse-secret-kind".to_string(),
            config: json!({
                "url": "https://user:pass@provider.example/stream?api_key=inbound-secret-token",
                "headers": {
                    "Authorization": "Bearer inbound-secret-token",
                    "X-Api-Key": "inbound-api-key-secret"
                },
                "event_type_field": "tenant_secret_event_type",
                "default_event_type": "secret.default.v1",
                "event_type_filter": ["tenant.secret.v1"]
            }),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        let create = CreateInboundSourceRequest {
            name: "create-secret-inbound-source".to_string(),
            kind: "redis-secret-kind".to_string(),
            config: json!({
                "redis_url": "redis://user:pass@redis.example:6379/0",
                "stream": "tenant-secret-stream",
                "group": "tenant-secret-group",
                "consumer": "tenant-secret-consumer",
                "event_type_field": "tenant_secret_event_type"
            }),
            enabled: true,
        };

        let debug = format!("{source:?}{create:?}");

        assert_debug_excludes(
            &debug,
            &[
                "tenant-secret-inbound-source",
                "sse-secret-kind",
                "create-secret-inbound-source",
                "redis-secret-kind",
                "https://user:pass@provider.example",
                "api_key=inbound-secret-token",
                "Authorization",
                "Bearer inbound-secret-token",
                "X-Api-Key",
                "inbound-api-key-secret",
                "tenant_secret_event_type",
                "secret.default.v1",
                "tenant.secret.v1",
                "redis://user:pass@redis.example",
                "tenant-secret-stream",
                "tenant-secret-group",
                "tenant-secret-consumer",
            ],
        );
        assert!(debug.contains("name_len"));
        assert!(debug.contains("kind_len"));
        assert!(debug.contains("config_class"));
        assert!(debug.contains("config_len"));
        assert!(debug.contains("config_key_count"));
    }

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
            access_count: 0,
            title: None,
            metadata: json!({}),
            chunk_metadata: None,
            document_type_id: None,
        };

        let serialized = serde_json::to_string(&note).unwrap();
        let deserialized: NoteMeta = serde_json::from_str(&serialized).unwrap();
        assert_eq!(note.id, deserialized.id);
    }

    #[test]
    fn test_note_meta_with_chunk_metadata() {
        let chunk_meta = json!({
            "total_chunks": 3,
            "chunking_strategy": "semantic",
            "chunk_sequence": ["uuid-1", "uuid-2", "uuid-3"]
        });

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
            access_count: 0,
            title: None,
            metadata: json!({}),
            chunk_metadata: Some(chunk_meta.clone()),
            document_type_id: None,
        };

        let serialized = serde_json::to_string(&note).unwrap();
        let deserialized: NoteMeta = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.chunk_metadata, Some(chunk_meta));
    }

    #[test]
    fn test_chunk_metadata_skips_when_none() {
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
            access_count: 0,
            title: None,
            metadata: json!({}),
            chunk_metadata: None,
            document_type_id: None,
        };

        let json_value = serde_json::to_value(&note).unwrap();
        // chunk_metadata should be skipped when None
        assert!(!json_value
            .as_object()
            .unwrap()
            .contains_key("chunk_metadata"));
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

    // =========================================================================
    // Embedding Set Tests
    // =========================================================================

    #[test]
    fn test_embedding_set_mode_serialization() {
        let modes = vec![
            (EmbeddingSetMode::Auto, "auto"),
            (EmbeddingSetMode::Manual, "manual"),
            (EmbeddingSetMode::Mixed, "mixed"),
        ];

        for (mode, expected) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let deserialized: EmbeddingSetMode = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, mode);
        }
    }

    #[test]
    fn test_embedding_index_status_serialization() {
        let statuses = vec![
            (EmbeddingIndexStatus::Pending, "pending"),
            (EmbeddingIndexStatus::Building, "building"),
            (EmbeddingIndexStatus::Ready, "ready"),
            (EmbeddingIndexStatus::Stale, "stale"),
            (EmbeddingIndexStatus::Disabled, "disabled"),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let deserialized: EmbeddingIndexStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_embedding_set_criteria_defaults() {
        let criteria = EmbeddingSetCriteria::default();
        assert!(!criteria.include_all);
        assert!(criteria.tags.is_empty());
        assert!(criteria.collections.is_empty());
        assert!(criteria.fts_query.is_none());
        assert!(criteria.created_after.is_none());
        assert!(criteria.created_before.is_none());
        // Note: Default derive gives false, but serde deserialize gives true
        assert!(!criteria.exclude_archived);
    }

    #[test]
    fn test_embedding_set_criteria_serde_defaults() {
        // When deserializing with missing fields, serde uses its defaults
        let json = r#"{"include_all": false}"#;
        let criteria: EmbeddingSetCriteria = serde_json::from_str(json).unwrap();
        // serde default for exclude_archived is true
        assert!(criteria.exclude_archived);
    }

    #[test]
    fn test_embedding_set_criteria_serialization() {
        let criteria = EmbeddingSetCriteria {
            include_all: false,
            tags: vec!["rust".to_string(), "programming".to_string()],
            collections: vec![],
            fts_query: Some("machine learning".to_string()),
            created_after: None,
            created_before: None,
            exclude_archived: true,
        };

        let json = serde_json::to_string(&criteria).unwrap();
        let deserialized: EmbeddingSetCriteria = serde_json::from_str(&json).unwrap();

        assert_eq!(criteria.include_all, deserialized.include_all);
        assert_eq!(criteria.tags, deserialized.tags);
        assert_eq!(criteria.fts_query, deserialized.fts_query);
        assert_eq!(criteria.exclude_archived, deserialized.exclude_archived);
    }

    #[test]
    fn test_create_embedding_set_request() {
        let request = CreateEmbeddingSetRequest {
            name: "Test Set".to_string(),
            slug: None, // Should be auto-generated
            description: Some("A test embedding set".to_string()),
            purpose: Some("Testing".to_string()),
            usage_hints: None,
            keywords: vec!["test".to_string()],
            set_type: EmbeddingSetType::Filter,
            mode: EmbeddingSetMode::Auto,
            criteria: EmbeddingSetCriteria {
                include_all: false,
                tags: vec!["test".to_string()],
                ..Default::default()
            },
            agent_metadata: EmbeddingSetAgentMetadata::default(),
            embedding_config_id: None,
            truncate_dim: None,
            auto_embed_rules: AutoEmbedRules::default(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateEmbeddingSetRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.name, deserialized.name);
        assert_eq!(request.slug, deserialized.slug);
        assert_eq!(request.mode, deserialized.mode);
        assert_eq!(request.set_type, deserialized.set_type);
    }

    #[test]
    fn test_embedding_set_agent_metadata() {
        let metadata = EmbeddingSetAgentMetadata {
            created_by_agent: Some("test-agent".to_string()),
            rationale: Some("Created for testing purposes".to_string()),
            performance_notes: None,
            related_sets: vec!["default".to_string()],
            suggested_queries: vec!["test query".to_string(), "another query".to_string()],
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: EmbeddingSetAgentMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata.created_by_agent, deserialized.created_by_agent);
        assert_eq!(metadata.related_sets, deserialized.related_sets);
        assert_eq!(metadata.suggested_queries, deserialized.suggested_queries);
    }

    #[test]
    fn test_add_members_request() {
        let request = AddMembersRequest {
            note_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
            added_by: Some("test-user".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AddMembersRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.note_ids.len(), deserialized.note_ids.len());
        assert_eq!(request.added_by, deserialized.added_by);
    }

    #[test]
    fn test_embedding_set_job_types_priority() {
        // Embedding set jobs should have lower priority (background tasks)
        assert!(
            JobType::Embedding.default_priority() > JobType::CreateEmbeddingSet.default_priority()
        );
        assert!(
            JobType::Embedding.default_priority() > JobType::RefreshEmbeddingSet.default_priority()
        );
        assert!(
            JobType::BuildSetIndex.default_priority()
                > JobType::CreateEmbeddingSet.default_priority()
        );
    }

    // =========================================================================
    // Extraction Strategy Regression Tests (#253)
    // =========================================================================

    #[test]
    fn test_strategy_real_jpeg_gets_vision() {
        // Real image/jpeg from magic bytes → Vision strategy
        let strategy = ExtractionStrategy::from_mime_and_extension("image/jpeg", Some("jpg"));
        assert_eq!(strategy, ExtractionStrategy::Vision);
    }

    #[test]
    fn test_strategy_real_png_gets_vision() {
        let strategy = ExtractionStrategy::from_mime_and_extension("image/png", Some("png"));
        assert_eq!(strategy, ExtractionStrategy::Vision);
    }

    #[test]
    fn test_strategy_fake_jpeg_no_vision() {
        // detect_content_type() returned octet-stream (magic bytes didn't match)
        // Extension .jpg should NOT promote to Vision — data is untrustworthy
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("jpg"));
        assert_ne!(
            strategy,
            ExtractionStrategy::Vision,
            "Random binary with .jpg extension should not get Vision strategy"
        );
        assert_eq!(strategy, ExtractionStrategy::TextNative);
    }

    #[test]
    fn test_strategy_fake_mp3_no_audio() {
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("mp3"));
        assert_ne!(
            strategy,
            ExtractionStrategy::AudioTranscribe,
            "Random binary with .mp3 extension should not get AudioTranscribe"
        );
        assert_eq!(strategy, ExtractionStrategy::TextNative);
    }

    #[test]
    fn test_strategy_fake_mp4_no_video() {
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("mp4"));
        assert_ne!(
            strategy,
            ExtractionStrategy::VideoMultimodal,
            "Random binary with .mp4 extension should not get VideoMultimodal"
        );
        assert_eq!(strategy, ExtractionStrategy::TextNative);
    }

    #[test]
    fn test_strategy_octet_stream_pdf_still_works() {
        // PDF is cheap text extraction, safe to assign from extension
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("pdf"));
        assert_eq!(strategy, ExtractionStrategy::PdfText);
    }

    #[test]
    fn test_strategy_octet_stream_docx_still_works() {
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("docx"));
        assert_eq!(strategy, ExtractionStrategy::OfficeConvert);
    }

    #[test]
    fn test_strategy_octet_stream_code_still_works() {
        let strategy =
            ExtractionStrategy::from_mime_and_extension("application/octet-stream", Some("py"));
        assert_eq!(strategy, ExtractionStrategy::CodeAst);
    }

    // =========================================================================
    // Additional Model Tests
    // =========================================================================

    #[test]
    fn test_embedding_config_default_values() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.chunk_size, 1000);
        assert_eq!(config.chunk_overlap, 100);
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimension, 768);
    }

    #[test]
    fn test_embedding_set_mode_display() {
        assert_eq!(EmbeddingSetMode::Auto.to_string(), "auto");
        assert_eq!(EmbeddingSetMode::Manual.to_string(), "manual");
        assert_eq!(EmbeddingSetMode::Mixed.to_string(), "mixed");
    }

    #[test]
    fn test_embedding_set_mode_from_str_valid() {
        assert_eq!(
            "auto".parse::<EmbeddingSetMode>().unwrap(),
            EmbeddingSetMode::Auto
        );
        assert_eq!(
            "AUTO".parse::<EmbeddingSetMode>().unwrap(),
            EmbeddingSetMode::Auto
        );
        assert_eq!(
            "manual".parse::<EmbeddingSetMode>().unwrap(),
            EmbeddingSetMode::Manual
        );
        assert_eq!(
            "mixed".parse::<EmbeddingSetMode>().unwrap(),
            EmbeddingSetMode::Mixed
        );
    }

    #[test]
    fn test_embedding_set_mode_from_str_invalid() {
        let result = "invalid".parse::<EmbeddingSetMode>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid embedding set mode"));
    }

    #[test]
    fn test_embedding_index_status_display() {
        assert_eq!(EmbeddingIndexStatus::Pending.to_string(), "pending");
        assert_eq!(EmbeddingIndexStatus::Building.to_string(), "building");
        assert_eq!(EmbeddingIndexStatus::Ready.to_string(), "ready");
        assert_eq!(EmbeddingIndexStatus::Stale.to_string(), "stale");
        assert_eq!(EmbeddingIndexStatus::Disabled.to_string(), "disabled");
    }

    #[test]
    fn test_embedding_index_status_from_str_valid() {
        assert_eq!(
            "pending".parse::<EmbeddingIndexStatus>().unwrap(),
            EmbeddingIndexStatus::Pending
        );
        assert_eq!(
            "BUILDING".parse::<EmbeddingIndexStatus>().unwrap(),
            EmbeddingIndexStatus::Building
        );
        assert_eq!(
            "ready".parse::<EmbeddingIndexStatus>().unwrap(),
            EmbeddingIndexStatus::Ready
        );
    }

    #[test]
    fn test_embedding_index_status_from_str_invalid() {
        let result = "unknown".parse::<EmbeddingIndexStatus>();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid embedding index status"));
    }

    #[test]
    fn test_job_status_serialization() {
        let statuses = vec![
            (JobStatus::Pending, "pending"),
            (JobStatus::Running, "running"),
            (JobStatus::Completed, "completed"),
            (JobStatus::Failed, "failed"),
            (JobStatus::Cancelled, "cancelled"),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_revision_mode_default() {
        assert_eq!(RevisionMode::default(), RevisionMode::Standard);
    }

    #[test]
    fn test_revision_mode_serialization() {
        let modes = vec![
            (RevisionMode::Full, "full"),
            (RevisionMode::Light, "light"),
            (RevisionMode::Standard, "standard"),
            (RevisionMode::Contextual, "contextual"),
            (RevisionMode::ContextualFiltered, "contextual_filtered"),
            (RevisionMode::None, "none"),
        ];

        for (mode, expected) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: RevisionMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn test_job_type_serialization() {
        let types = vec![
            (JobType::AiRevision, "ai_revision"),
            (JobType::AiRevisionContextual, "ai_revision_contextual"),
            (JobType::Embedding, "embedding"),
            (JobType::Linking, "linking"),
            (JobType::ContextUpdate, "context_update"),
            (JobType::TitleGeneration, "title_generation"),
            (JobType::CreateEmbeddingSet, "create_embedding_set"),
            (JobType::RefreshEmbeddingSet, "refresh_embedding_set"),
            (JobType::BuildSetIndex, "build_set_index"),
            (JobType::PurgeNote, "purge_note"),
            (JobType::ConceptTagging, "concept_tagging"),
            (JobType::ReEmbedAll, "re_embed_all"),
            (
                JobType::RelatedConceptInference,
                "related_concept_inference",
            ),
        ];

        for (job_type, expected) in types {
            let json = serde_json::to_string(&job_type).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: JobType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, job_type);
        }
    }

    #[test]
    fn test_job_type_all_priorities_are_positive() {
        let types = vec![
            JobType::AiRevision,
            JobType::Embedding,
            JobType::Linking,
            JobType::ContextUpdate,
            JobType::TitleGeneration,
            JobType::CreateEmbeddingSet,
            JobType::RefreshEmbeddingSet,
            JobType::BuildSetIndex,
            JobType::PurgeNote,
            JobType::ConceptTagging,
            JobType::ReEmbedAll,
            JobType::RelatedConceptInference,
            JobType::ReferenceExtraction,
            JobType::GraphMaintenance,
        ];

        for job_type in types {
            assert!(job_type.default_priority() > 0);
        }
    }

    #[test]
    fn test_purge_note_has_highest_priority() {
        let types = vec![
            JobType::AiRevision,
            JobType::Embedding,
            JobType::Linking,
            JobType::ContextUpdate,
            JobType::TitleGeneration,
            JobType::CreateEmbeddingSet,
            JobType::RefreshEmbeddingSet,
            JobType::BuildSetIndex,
            JobType::ConceptTagging,
            JobType::ReEmbedAll,
            JobType::RelatedConceptInference,
            JobType::ReferenceExtraction,
            JobType::GraphMaintenance,
        ];

        for job_type in types {
            assert!(JobType::PurgeNote.default_priority() >= job_type.default_priority());
        }
    }

    #[test]
    fn test_oauth_error_constructors() {
        let err = OAuthError::invalid_request("bad param");
        assert_eq!(err.error, "invalid_request");
        assert_eq!(err.error_description, Some("bad param".to_string()));

        let err = OAuthError::invalid_client("unknown client");
        assert_eq!(err.error, "invalid_client");

        let err = OAuthError::invalid_grant("expired code");
        assert_eq!(err.error, "invalid_grant");

        let err = OAuthError::unauthorized_client("not allowed");
        assert_eq!(err.error, "unauthorized_client");

        let err = OAuthError::unsupported_grant_type("unknown grant");
        assert_eq!(err.error, "unsupported_grant_type");

        let err = OAuthError::invalid_scope("bad scope");
        assert_eq!(err.error, "invalid_scope");

        let err = OAuthError::unsupported_response_type("bad type");
        assert_eq!(err.error, "unsupported_response_type");

        let err = OAuthError::server_error("internal");
        assert_eq!(err.error, "server_error");
    }

    #[test]
    fn test_oauth_grant_type_serialization() {
        let types = vec![
            (OAuthGrantType::AuthorizationCode, "authorization_code"),
            (OAuthGrantType::ClientCredentials, "client_credentials"),
            (OAuthGrantType::RefreshToken, "refresh_token"),
        ];

        for (grant_type, expected) in types {
            let json = serde_json::to_string(&grant_type).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: OAuthGrantType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, grant_type);
        }
    }

    #[test]
    fn test_oauth_response_type_serialization() {
        let types = vec![
            (OAuthResponseType::Code, "code"),
            (OAuthResponseType::Token, "token"),
        ];

        for (response_type, expected) in types {
            let json = serde_json::to_string(&response_type).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: OAuthResponseType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, response_type);
        }
    }

    #[test]
    fn test_token_auth_method_default() {
        assert_eq!(
            TokenAuthMethod::default(),
            TokenAuthMethod::ClientSecretBasic
        );
    }

    #[test]
    fn test_token_auth_method_serialization() {
        let methods = vec![
            (TokenAuthMethod::ClientSecretBasic, "client_secret_basic"),
            (TokenAuthMethod::ClientSecretPost, "client_secret_post"),
            (TokenAuthMethod::None, "none"),
        ];

        for (method, expected) in methods {
            let json = serde_json::to_string(&method).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: TokenAuthMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, method);
        }
    }

    #[test]
    fn test_auth_principal_has_scope_admin() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "test".to_string(),
            scope: "admin".to_string(),
            user_id: None,
        };

        assert!(principal.has_scope("read"));
        assert!(principal.has_scope("write"));
        assert!(principal.has_scope("delete"));
        assert!(principal.has_scope("anything"));
    }

    #[test]
    fn test_auth_principal_has_scope_mcp_transport_only() {
        let principal = AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "mcp".to_string(),
        };

        assert!(principal.has_scope("mcp"));
        assert!(!principal.has_scope("read"));
        assert!(!principal.has_scope("write"));
        assert!(!principal.has_scope("delete"));
    }

    #[test]
    fn test_auth_principal_has_scope_specific() {
        let principal = AuthPrincipal::OAuthClient {
            client_id: "test".to_string(),
            scope: "read write".to_string(),
            user_id: None,
        };

        assert!(principal.has_scope("read"));
        assert!(principal.has_scope("write"));
        assert!(!principal.has_scope("delete"));
    }

    #[test]
    fn test_auth_principal_anonymous_has_no_scope() {
        let principal = AuthPrincipal::Anonymous;

        assert!(!principal.has_scope("read"));
        assert!(!principal.has_scope("write"));
        assert!(!principal.has_scope("admin"));
    }

    #[test]
    fn test_auth_principal_is_authenticated() {
        let oauth = AuthPrincipal::OAuthClient {
            client_id: "test".to_string(),
            scope: "read".to_string(),
            user_id: None,
        };
        assert!(oauth.is_authenticated());

        let api_key = AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read".to_string(),
        };
        assert!(api_key.is_authenticated());

        let anon = AuthPrincipal::Anonymous;
        assert!(!anon.is_authenticated());
    }

    #[test]
    fn auth_principal_debug_redacts_identity_and_scope_values() {
        let key_id = Uuid::now_v7();
        let oauth = AuthPrincipal::OAuthClient {
            client_id: "oauth-client-secret-shaped@example.internal".to_string(),
            scope: "read write tenant:secret admin".to_string(),
            user_id: Some("user@example.internal/path/token-secret".to_string()),
        };
        let api_key = AuthPrincipal::ApiKey {
            key_id,
            scope: "mcp api-key-secret-scope".to_string(),
        };
        let anonymous = AuthPrincipal::Anonymous;

        let debug = format!("{oauth:?} {api_key:?} {anonymous:?}");

        assert!(debug.contains("AuthPrincipal::OAuthClient"));
        assert!(debug.contains("client_id_len"));
        assert!(debug.contains("scope_count"));
        assert!(debug.contains("scope_len"));
        assert!(debug.contains("user_id_len"));
        assert!(debug.contains("AuthPrincipal::ApiKey"));
        assert!(debug.contains("key_id_set"));
        assert!(debug.contains("key_id_version"));
        assert!(debug.contains("AuthPrincipal::Anonymous"));
        assert!(!debug.contains("oauth-client-secret-shaped"));
        assert!(!debug.contains("example.internal"));
        assert!(!debug.contains("tenant:secret"));
        assert!(!debug.contains("admin"));
        assert!(!debug.contains("token-secret"));
        assert!(!debug.contains("api-key-secret-scope"));
        assert!(!debug.contains(&key_id.to_string()));
    }

    #[test]
    fn note_template_debug_redacts_content_tags_and_identifiers() {
        let template_id = Uuid::now_v7();
        let collection_id = Uuid::now_v7();
        let template = NoteTemplate {
            id: template_id,
            name: "incident-template@example.internal".to_string(),
            description: Some("uses postgres://user:secret@db.internal/template".to_string()),
            content: "private template body with /srv/fortemi/path and bearer-secret".to_string(),
            format: "markdown-private".to_string(),
            default_tags: vec![
                "customer/email@example.internal".to_string(),
                "token/sk-secret-template".to_string(),
            ],
            collection_id: Some(collection_id),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
        };

        let debug = format!("{template:?}");

        assert!(debug.contains("NoteTemplate"));
        assert!(debug.contains("id_set"));
        assert!(debug.contains("name_len"));
        assert!(debug.contains("description_len"));
        assert!(debug.contains("content_len"));
        assert!(debug.contains("format_len"));
        assert!(debug.contains("default_tags_count"));
        assert!(debug.contains("default_tag_lens"));
        assert!(debug.contains("collection_id_set"));
        assert!(!debug.contains("incident-template"));
        assert!(!debug.contains("postgres://"));
        assert!(!debug.contains("db.internal"));
        assert!(!debug.contains("/srv/fortemi"));
        assert!(!debug.contains("bearer-secret"));
        assert!(!debug.contains("markdown-private"));
        assert!(!debug.contains("email@example.internal"));
        assert!(!debug.contains("sk-secret-template"));
        assert!(!debug.contains(&template_id.to_string()));
        assert!(!debug.contains(&collection_id.to_string()));
    }

    #[test]
    fn test_default_scope_function() {
        assert_eq!(default_scope(), "read");
    }

    #[test]
    fn test_default_true_function() {
        assert!(default_true());
    }

    #[test]
    fn test_search_mode_serialization() {
        let modes = vec![
            (SearchMode::Fts, "fts"),
            (SearchMode::Vector, "vector"),
            (SearchMode::Hybrid, "hybrid"),
        ];

        for (mode, expected) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: SearchMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn test_embedding_set_mode_default() {
        assert_eq!(EmbeddingSetMode::default(), EmbeddingSetMode::Auto);
    }

    #[test]
    fn test_embedding_index_status_default() {
        assert_eq!(
            EmbeddingIndexStatus::default(),
            EmbeddingIndexStatus::Pending
        );
    }

    #[test]
    fn test_embedding_set_agent_metadata_default() {
        let metadata = EmbeddingSetAgentMetadata::default();
        assert!(metadata.created_by_agent.is_none());
        assert!(metadata.rationale.is_none());
        assert!(metadata.performance_notes.is_none());
        assert!(metadata.related_sets.is_empty());
        assert!(metadata.suggested_queries.is_empty());
    }

    #[test]
    fn test_update_embedding_set_request_default() {
        let request = UpdateEmbeddingSetRequest::default();
        assert!(request.name.is_none());
        assert!(request.description.is_none());
        assert!(request.purpose.is_none());
        assert!(request.mode.is_none());
        assert!(request.is_active.is_none());
    }

    #[test]
    fn test_collection_serialization_with_note_count() {
        let collection = Collection {
            id: Uuid::new_v4(),
            name: "Test Collection".to_string(),
            description: Some("Description".to_string()),
            parent_id: None,
            created_at_utc: Utc::now(),
            note_count: 42,
        };

        let json = serde_json::to_string(&collection).unwrap();
        let parsed: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.note_count, 42);
        assert_eq!(parsed.name, "Test Collection");
    }

    #[test]
    fn test_search_hit_skip_serializing_empty_tags() {
        let hit = SearchHit {
            note_id: Uuid::new_v4(),
            score: 0.95,
            snippet: Some("snippet".to_string()),
            title: Some("title".to_string()),
            tags: vec![],
            embedding_status: None,
        };
        let json = serde_json::to_value(&hit).unwrap();
        // Empty tags should be skipped
        assert!(
            !json.as_object().unwrap().contains_key("tags")
                || json["tags"].as_array().is_none_or(|a| a.is_empty())
        );
    }

    #[test]
    fn test_note_original_serialization() {
        let original = NoteOriginal {
            content: "test content".to_string(),
            hash: "abc123".to_string(),
            user_created_at: None,
            user_last_edited_at: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: NoteOriginal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "test content");
        assert_eq!(parsed.hash, "abc123");
    }

    #[test]
    fn test_note_revised_serialization() {
        let revised = NoteRevised {
            content: "revised content".to_string(),
            last_revision_id: Some(Uuid::new_v4()),
            ai_metadata: Some(json!({"test": "data"})),
            ai_generated_at: None,
            user_last_edited_at: None,
            is_user_edited: false,
            generation_count: 1,
            model: Some("gpt-4".to_string()),
        };

        let json = serde_json::to_string(&revised).unwrap();
        let parsed: NoteRevised = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "revised content");
        assert_eq!(parsed.generation_count, 1);
        assert!(!parsed.is_user_edited);
    }

    // =========================================================================
    // Memory Search Types Tests
    // =========================================================================

    #[test]
    fn test_memory_hit_serialization() {
        let hit = MemoryHit {
            provenance_id: Uuid::new_v4(),
            attachment_id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            filename: "photo.jpg".to_string(),
            content_type: Some("image/jpeg".to_string()),
            capture_time: Some((Utc::now(), None)),
            event_type: Some("photo".to_string()),
            event_title: Some("Beach sunset".to_string()),
            distance_m: Some(150.5),
            location_name: Some("Santa Monica Beach".to_string()),
        };

        let json = serde_json::to_string(&hit).unwrap();
        let deserialized: MemoryHit = serde_json::from_str(&json).unwrap();

        assert_eq!(hit.provenance_id, deserialized.provenance_id);
        assert_eq!(hit.filename, deserialized.filename);
        assert_eq!(hit.content_type, deserialized.content_type);
        assert_eq!(hit.event_type, deserialized.event_type);
    }

    #[test]
    fn test_memory_hit_optional_fields() {
        let hit = MemoryHit {
            provenance_id: Uuid::new_v4(),
            attachment_id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            filename: "document.pdf".to_string(),
            content_type: None,
            capture_time: None,
            event_type: None,
            event_title: None,
            distance_m: None,
            location_name: None,
        };

        let json = serde_json::to_string(&hit).unwrap();
        assert!(!json.contains("content_type"));
        assert!(!json.contains("capture_time"));
        assert!(!json.contains("distance_m"));
    }

    #[test]
    fn test_cross_archive_request_defaults() {
        let req = CrossArchiveSearchRequest {
            query: "test".to_string(),
            archives: vec![],
            mode: Default::default(),
            limit: default_ca_limit(),
            enable_fusion: false,
        };

        assert_eq!(req.limit, 20);
        assert!(!req.enable_fusion);
        assert!(req.archives.is_empty());
    }

    #[test]
    fn test_cross_archive_request_serialization() {
        let req = CrossArchiveSearchRequest {
            query: "rust programming".to_string(),
            archives: vec!["archive_2024".to_string(), "archive_2025".to_string()],
            mode: SearchMode::Hybrid,
            limit: 50,
            enable_fusion: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CrossArchiveSearchRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.query, deserialized.query);
        assert_eq!(req.archives, deserialized.archives);
        assert_eq!(req.limit, deserialized.limit);
        assert_eq!(req.enable_fusion, deserialized.enable_fusion);
    }

    #[test]
    fn test_cross_archive_result_serialization() {
        let result = CrossArchiveSearchResult {
            archive_name: "archive_2024".to_string(),
            note_id: Uuid::new_v4(),
            score: 0.95,
            snippet: Some("This is a snippet".to_string()),
            title: Some("Test Note".to_string()),
            tags: vec!["rust".to_string(), "programming".to_string()],
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CrossArchiveSearchResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.archive_name, deserialized.archive_name);
        assert_eq!(result.note_id, deserialized.note_id);
        assert_eq!(result.tags, deserialized.tags);
    }

    #[test]
    fn cross_archive_debug_redacts_queries_results_and_identifiers() {
        let note_id = Uuid::now_v7();
        let request = CrossArchiveSearchRequest {
            query: "find private token sk-secret in postgres://user:secret@db.internal".to_string(),
            archives: vec![
                "tenant_archive@example.internal".to_string(),
                "/srv/fortemi/private/archive".to_string(),
            ],
            mode: SearchMode::Hybrid,
            limit: 50,
            enable_fusion: true,
        };
        let result = CrossArchiveSearchResult {
            archive_name: "archive_with_secret@example.internal".to_string(),
            note_id,
            score: 0.95,
            snippet: Some("private snippet with /srv/fortemi/path and bearer-secret".to_string()),
            title: Some("Sensitive title postgres://user:secret@db.internal".to_string()),
            tags: vec![
                "customer/email@example.internal".to_string(),
                "token/sk-secret-cross-archive".to_string(),
            ],
        };
        let response = CrossArchiveSearchResponse {
            results: vec![result.clone()],
            archives_searched: vec![
                "archive_with_secret@example.internal".to_string(),
                "postgres://user:secret@db.internal/archive".to_string(),
            ],
            total: 1,
        };

        let debug = format!("{request:?} {result:?} {response:?}");

        assert!(debug.contains("CrossArchiveSearchRequest"));
        assert!(debug.contains("query_len"));
        assert!(debug.contains("archives_count"));
        assert!(debug.contains("CrossArchiveSearchResult"));
        assert!(debug.contains("archive_name_len"));
        assert!(debug.contains("snippet_len"));
        assert!(debug.contains("title_len"));
        assert!(debug.contains("tag_lens"));
        assert!(debug.contains("CrossArchiveSearchResponse"));
        assert!(debug.contains("results_count"));
        assert!(debug.contains("archives_searched_count"));
        assert!(!debug.contains("sk-secret"));
        assert!(!debug.contains("postgres://"));
        assert!(!debug.contains("db.internal"));
        assert!(!debug.contains("tenant_archive"));
        assert!(!debug.contains("example.internal"));
        assert!(!debug.contains("/srv/fortemi"));
        assert!(!debug.contains("bearer-secret"));
        assert!(!debug.contains("Sensitive title"));
        assert!(!debug.contains(&note_id.to_string()));
    }

    #[test]
    fn test_attachment_search_request_optional_fields() {
        let req = AttachmentSearchRequest {
            note_id: None,
            content_type: Some("image/".to_string()),
            event_type: None,
            capture_after: None,
            capture_before: None,
            near_lat: Some(34.0),
            near_lon: Some(-118.0),
            radius_m: Some(5000.0),
            location_name: None,
            device_id: None,
            limit: 100,
        };

        assert!(req.note_id.is_none());
        assert!(req.content_type.is_some());
        assert_eq!(req.limit, 100);

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: AttachmentSearchRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.content_type, deserialized.content_type);
        assert_eq!(req.limit, deserialized.limit);
    }

    #[test]
    fn test_timeline_group_structure() {
        let group = TimelineGroup {
            period: "2024-01".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            memories: vec![],
            count: 0,
        };

        assert_eq!(group.period, "2024-01");
        assert_eq!(group.count, 0);
        assert!(group.memories.is_empty());

        let json = serde_json::to_string(&group).unwrap();
        let deserialized: TimelineGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(group.period, deserialized.period);
    }

    #[test]
    fn test_memory_search_response() {
        let response = MemorySearchResponse {
            memories: vec![],
            total: 0,
        };

        assert_eq!(response.total, 0);
        assert!(response.memories.is_empty());
    }

    #[test]
    fn test_timeline_response() {
        let response = TimelineResponse {
            groups: vec![],
            total: 0,
        };

        assert_eq!(response.total, 0);
        assert!(response.groups.is_empty());
    }

    #[test]
    fn memory_search_debug_redacts_hit_content_and_identifiers() {
        let provenance_id = Uuid::now_v7();
        let attachment_id = Uuid::now_v7();
        let note_id = Uuid::now_v7();
        let hit = MemoryHit {
            provenance_id,
            attachment_id,
            note_id,
            filename: "private-photo@example.internal-sk-secret.jpg".to_string(),
            content_type: Some("image/private-token".to_string()),
            capture_time: Some((Utc::now(), Some(Utc::now()))),
            event_type: Some("photo-secret-event".to_string()),
            event_title: Some("Beach near postgres://user:secret@db.internal".to_string()),
            distance_m: Some(150.5),
            location_name: Some("/srv/fortemi/private/location@example.internal".to_string()),
        };
        let memory_response = MemorySearchResponse {
            memories: vec![hit.clone()],
            total: 1,
        };
        let timeline_group = TimelineGroup {
            period: "2026-06-private@example.internal".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            memories: vec![hit.clone()],
            count: 1,
        };
        let timeline_response = TimelineResponse {
            groups: vec![timeline_group.clone()],
            total: 1,
        };
        let attachment_response = AttachmentSearchResponse {
            attachments: vec![hit.clone()],
            total: 1,
        };

        let debug = format!(
            "{hit:?} {memory_response:?} {timeline_group:?} {timeline_response:?} {attachment_response:?}"
        );

        assert!(debug.contains("MemoryHit"));
        assert!(debug.contains("filename_len"));
        assert!(debug.contains("event_title_len"));
        assert!(debug.contains("location_name_len"));
        assert!(debug.contains("MemorySearchResponse"));
        assert!(debug.contains("memories_count"));
        assert!(debug.contains("TimelineGroup"));
        assert!(debug.contains("period_len"));
        assert!(debug.contains("TimelineResponse"));
        assert!(debug.contains("groups_count"));
        assert!(debug.contains("AttachmentSearchResponse"));
        assert!(debug.contains("attachments_count"));
        assert!(!debug.contains("private-photo"));
        assert!(!debug.contains("example.internal"));
        assert!(!debug.contains("sk-secret"));
        assert!(!debug.contains("image/private-token"));
        assert!(!debug.contains("photo-secret-event"));
        assert!(!debug.contains("postgres://"));
        assert!(!debug.contains("db.internal"));
        assert!(!debug.contains("/srv/fortemi"));
        assert!(!debug.contains(&provenance_id.to_string()));
        assert!(!debug.contains(&attachment_id.to_string()));
        assert!(!debug.contains(&note_id.to_string()));
    }

    // =========================================================================
    // ExtractionStrategy Tests
    // =========================================================================

    #[test]
    fn test_extraction_strategy_display_fromstr_roundtrip() {
        // Test all 9 variants
        let variants = vec![
            ExtractionStrategy::TextNative,
            ExtractionStrategy::PdfText,
            ExtractionStrategy::PdfOcr,
            ExtractionStrategy::Vision,
            ExtractionStrategy::AudioTranscribe,
            ExtractionStrategy::VideoMultimodal,
            ExtractionStrategy::CodeAst,
            ExtractionStrategy::OfficeConvert,
            ExtractionStrategy::StructuredExtract,
        ];
        for variant in variants {
            let s = variant.to_string();
            let parsed: ExtractionStrategy = s.parse().unwrap();
            assert_eq!(parsed, variant, "Round-trip failed for {}", s);
        }
    }

    #[test]
    fn test_extraction_strategy_fromstr_aliases() {
        // squished forms
        assert_eq!(
            "textnative".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::TextNative
        );
        assert_eq!(
            "pdftext".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::PdfText
        );
        assert_eq!(
            "pdfocr".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::PdfOcr
        );
        assert_eq!(
            "pdf_scanned".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::PdfOcr
        );
        assert_eq!(
            "audiotranscribe".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::AudioTranscribe
        );
        assert_eq!(
            "videomultimodal".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::VideoMultimodal
        );
        assert_eq!(
            "codeast".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::CodeAst
        );
        assert_eq!(
            "officeconvert".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::OfficeConvert
        );
        assert_eq!(
            "structuredextract".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::StructuredExtract
        );
    }

    #[test]
    fn test_extraction_strategy_fromstr_case_insensitive() {
        assert_eq!(
            "TEXT_NATIVE".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::TextNative
        );
        assert_eq!(
            "Pdf_Text".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::PdfText
        );
        assert_eq!(
            "VISION".parse::<ExtractionStrategy>().unwrap(),
            ExtractionStrategy::Vision
        );
    }

    #[test]
    fn test_extraction_strategy_fromstr_invalid() {
        assert!("unknown".parse::<ExtractionStrategy>().is_err());
        assert!("".parse::<ExtractionStrategy>().is_err());
        assert!("pdf".parse::<ExtractionStrategy>().is_err());
    }

    #[test]
    fn test_extraction_strategy_default() {
        assert_eq!(
            ExtractionStrategy::default(),
            ExtractionStrategy::TextNative
        );
    }

    #[test]
    fn test_extraction_strategy_serde_roundtrip() {
        let variants = vec![
            ExtractionStrategy::TextNative,
            ExtractionStrategy::PdfText,
            ExtractionStrategy::PdfOcr,
            ExtractionStrategy::Vision,
            ExtractionStrategy::AudioTranscribe,
            ExtractionStrategy::VideoMultimodal,
            ExtractionStrategy::CodeAst,
            ExtractionStrategy::OfficeConvert,
            ExtractionStrategy::StructuredExtract,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: ExtractionStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant, "Serde round-trip failed for {:?}", variant);
        }
    }

    #[test]
    fn test_mime_pdf() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("application/pdf"),
            ExtractionStrategy::PdfText
        );
    }

    #[test]
    fn test_mime_images() {
        for mime in [
            "image/jpeg",
            "image/png",
            "image/gif",
            "image/webp",
            "image/svg+xml",
            "image/tiff",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::Vision,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_3d_models() {
        for mime in [
            "model/gltf+json",
            "model/gltf-binary",
            "model/obj",
            "model/stl",
            "model/step",
            "model/iges",
            "model/vnd.usdz+zip",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::Glb3DModel,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_audio() {
        for mime in [
            "audio/mpeg",
            "audio/wav",
            "audio/ogg",
            "audio/flac",
            "audio/aac",
            "audio/webm",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::AudioTranscribe,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_midi_is_structured() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("audio/midi"),
            ExtractionStrategy::StructuredExtract
        );
        assert_eq!(
            ExtractionStrategy::from_mime_type("audio/x-midi"),
            ExtractionStrategy::StructuredExtract
        );
    }

    #[test]
    fn test_mime_video() {
        for mime in [
            "video/mp4",
            "video/webm",
            "video/ogg",
            "video/quicktime",
            "video/x-msvideo",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::VideoMultimodal,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_office() {
        for mime in [
            "application/msword",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application/vnd.ms-powerpoint",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "application/rtf",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::OfficeConvert,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_outlook_is_email() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("application/vnd.ms-outlook"),
            ExtractionStrategy::Email
        );
    }

    #[test]
    fn test_mime_email() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("message/rfc822"),
            ExtractionStrategy::Email
        );
        assert_eq!(
            ExtractionStrategy::from_mime_type("application/mbox"),
            ExtractionStrategy::Email
        );
    }

    #[test]
    fn test_mime_spreadsheet() {
        for mime in [
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "application/vnd.ms-excel",
            "application/vnd.oasis.opendocument.spreadsheet",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::Spreadsheet,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_archive() {
        for mime in [
            "application/zip",
            "application/x-tar",
            "application/gzip",
            "application/x-7z-compressed",
            "application/x-rar-compressed",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::Archive,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_structured_data_core() {
        for mime in [
            "application/json",
            "application/xml",
            "text/xml",
            "application/yaml",
            "text/yaml",
            "text/csv",
            "application/toml",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::StructuredExtract,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_structured_data_extended() {
        for mime in [
            "application/x-bibtex",
            "application/x-research-info-systems",
            "application/avro",
            "application/vnd.apache.parquet",
            "application/x-ndjson",
            "application/geo+json",
            "application/x-drawio",
            "application/x-excalidraw+json",
            "text/calendar",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::StructuredExtract,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_mime_text_native() {
        for mime in [
            "text/plain",
            "text/markdown",
            "text/html",
            "text/css",
            "text/javascript",
            "text/x-python",
            "text/x-rust",
            "text/x-c",
            "text/x-java",
            "text/x-go",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                ExtractionStrategy::TextNative,
                "Failed for {}",
                mime
            );
        }
    }

    #[test]
    fn test_octet_stream_extension_refinement() {
        let octet = "application/octet-stream";
        // PDF
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension(octet, Some("pdf")),
            ExtractionStrategy::PdfText
        );
        // Images — octet-stream with image extension should NOT promote to Vision
        // (magic bytes didn't match, so data doesn't match the claimed extension)
        for ext in ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "svg"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::TextNative,
                "Failed for .{}",
                ext
            );
        }
        // 3D models — same: no promotion without magic byte confirmation
        for ext in ["glb", "gltf", "obj", "stl", "step", "iges"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::TextNative,
                "Failed for .{}",
                ext
            );
        }
        // MIDI
        for ext in ["mid", "midi"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::StructuredExtract,
                "Failed for .{}",
                ext
            );
        }
        // Audio — no promotion without magic byte confirmation
        for ext in ["mp3", "wav", "ogg", "flac", "aac", "m4a", "wma"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::TextNative,
                "Failed for .{}",
                ext
            );
        }
        // Video — no promotion without magic byte confirmation
        for ext in ["mp4", "avi", "mov", "mkv", "webm", "wmv"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::TextNative,
                "Failed for .{}",
                ext
            );
        }
        // Office (pandoc-based conversion)
        for ext in ["doc", "docx", "ppt", "pptx", "odt", "odp", "rtf"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::OfficeConvert,
                "Failed for .{}",
                ext
            );
        }
        // Spreadsheet (calamine)
        for ext in ["xls", "xlsx", "ods"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::Spreadsheet,
                "Failed for .{}",
                ext
            );
        }
        // Email (mailparse)
        for ext in ["eml", "mbox"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::Email,
                "Failed for .{}",
                ext
            );
        }
        // Archive (zip/tar/flate2)
        for ext in ["zip", "tar", "gz", "tgz", "7z", "rar", "bz2", "xz"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::Archive,
                "Failed for .{}",
                ext
            );
        }
        // Structured data
        for ext in ["json", "xml", "yaml", "yml", "csv", "toml"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::StructuredExtract,
                "Failed for .{}",
                ext
            );
        }
        // Extended structured
        for ext in ["ics", "bib", "geojson", "ndjson", "parquet", "avro"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::StructuredExtract,
                "Failed for .{}",
                ext
            );
        }
        // Code
        for ext in [
            "rs", "py", "js", "ts", "go", "java", "c", "cpp", "h", "rb", "swift", "kt", "scala",
            "zig", "hs",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::CodeAst,
                "Failed for .{}",
                ext
            );
        }
        // Text
        for ext in ["txt", "md", "markdown", "rst", "org", "adoc"] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension(octet, Some(ext)),
                ExtractionStrategy::TextNative,
                "Failed for .{}",
                ext
            );
        }
        // Unknown
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension(octet, Some("xyz")),
            ExtractionStrategy::TextNative
        );
        // No extension
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension(octet, None),
            ExtractionStrategy::TextNative
        );
    }

    #[test]
    fn test_text_plain_code_extension_refinement() {
        for ext in [
            "rs", "py", "js", "ts", "go", "java", "c", "cpp", "h", "rb", "swift", "kt", "scala",
            "zig", "hs",
        ] {
            assert_eq!(
                ExtractionStrategy::from_mime_and_extension("text/plain", Some(ext)),
                ExtractionStrategy::CodeAst,
                "text/plain + .{} should be CodeAst",
                ext
            );
        }
    }

    #[test]
    fn test_specific_mime_ignores_extension() {
        // A specific MIME type should not be overridden by extension
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension("application/pdf", Some("txt")),
            ExtractionStrategy::PdfText
        );
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension("image/png", Some("txt")),
            ExtractionStrategy::Vision
        );
        assert_eq!(
            ExtractionStrategy::from_mime_and_extension("audio/mpeg", Some("txt")),
            ExtractionStrategy::AudioTranscribe
        );
    }

    #[test]
    fn test_mime_empty_string() {
        assert_eq!(
            ExtractionStrategy::from_mime_type(""),
            ExtractionStrategy::TextNative
        );
    }

    #[test]
    fn test_mime_unknown_type() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("application/x-unknown-format"),
            ExtractionStrategy::TextNative
        );
    }

    #[test]
    fn test_mime_case_insensitive() {
        assert_eq!(
            ExtractionStrategy::from_mime_type("APPLICATION/PDF"),
            ExtractionStrategy::PdfText
        );
        assert_eq!(
            ExtractionStrategy::from_mime_type("Image/JPEG"),
            ExtractionStrategy::Vision
        );
        assert_eq!(
            ExtractionStrategy::from_mime_type("AUDIO/MPEG"),
            ExtractionStrategy::AudioTranscribe
        );
        assert_eq!(
            ExtractionStrategy::from_mime_type("VIDEO/MP4"),
            ExtractionStrategy::VideoMultimodal
        );
    }

    #[test]
    fn test_mime_with_parameters() {
        // MIME types with parameters - the function lowercases but doesn't strip params,
        // so "text/plain; charset=utf-8" starts_with "text/" → TextNative
        assert_eq!(
            ExtractionStrategy::from_mime_type("text/plain; charset=utf-8"),
            ExtractionStrategy::TextNative
        );
    }

    #[test]
    fn test_all_seeded_mime_types_have_known_strategy() {
        // This test ensures every MIME type from the seed migration has an expected strategy.
        // If a new MIME type is added to the seeds, add it here too.
        let expectations: Vec<(&str, ExtractionStrategy)> = vec![
            // PDF
            ("application/pdf", ExtractionStrategy::PdfText),
            // Images
            ("image/jpeg", ExtractionStrategy::Vision),
            ("image/png", ExtractionStrategy::Vision),
            ("image/gif", ExtractionStrategy::Vision),
            ("image/webp", ExtractionStrategy::Vision),
            ("image/svg+xml", ExtractionStrategy::Vision),
            ("image/tiff", ExtractionStrategy::Vision),
            // 3D Models
            ("model/gltf+json", ExtractionStrategy::Glb3DModel),
            ("model/gltf-binary", ExtractionStrategy::Glb3DModel),
            ("model/obj", ExtractionStrategy::Glb3DModel),
            ("model/stl", ExtractionStrategy::Glb3DModel),
            ("model/step", ExtractionStrategy::Glb3DModel),
            ("model/iges", ExtractionStrategy::Glb3DModel),
            ("model/vnd.usdz+zip", ExtractionStrategy::Glb3DModel),
            // Audio (non-MIDI)
            ("audio/mpeg", ExtractionStrategy::AudioTranscribe),
            ("audio/wav", ExtractionStrategy::AudioTranscribe),
            ("audio/ogg", ExtractionStrategy::AudioTranscribe),
            ("audio/flac", ExtractionStrategy::AudioTranscribe),
            ("audio/aac", ExtractionStrategy::AudioTranscribe),
            ("audio/webm", ExtractionStrategy::AudioTranscribe),
            // MIDI
            ("audio/midi", ExtractionStrategy::StructuredExtract),
            ("audio/x-midi", ExtractionStrategy::StructuredExtract),
            // Video
            ("video/mp4", ExtractionStrategy::VideoMultimodal),
            ("video/webm", ExtractionStrategy::VideoMultimodal),
            ("video/ogg", ExtractionStrategy::VideoMultimodal),
            ("video/quicktime", ExtractionStrategy::VideoMultimodal),
            ("video/x-msvideo", ExtractionStrategy::VideoMultimodal),
            // Office
            ("application/msword", ExtractionStrategy::OfficeConvert),
            (
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                ExtractionStrategy::OfficeConvert,
            ),
            ("application/vnd.ms-excel", ExtractionStrategy::Spreadsheet),
            (
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                ExtractionStrategy::Spreadsheet,
            ),
            (
                "application/vnd.ms-powerpoint",
                ExtractionStrategy::OfficeConvert,
            ),
            (
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                ExtractionStrategy::OfficeConvert,
            ),
            ("application/rtf", ExtractionStrategy::OfficeConvert),
            // Email/Message
            ("application/vnd.ms-outlook", ExtractionStrategy::Email),
            ("message/rfc822", ExtractionStrategy::Email),
            ("application/mbox", ExtractionStrategy::Email),
            // Structured data (core)
            ("application/json", ExtractionStrategy::StructuredExtract),
            ("application/xml", ExtractionStrategy::StructuredExtract),
            ("text/xml", ExtractionStrategy::StructuredExtract),
            ("application/yaml", ExtractionStrategy::StructuredExtract),
            ("text/yaml", ExtractionStrategy::StructuredExtract),
            ("text/csv", ExtractionStrategy::StructuredExtract),
            ("application/toml", ExtractionStrategy::StructuredExtract),
            // Structured data (extended)
            (
                "application/x-bibtex",
                ExtractionStrategy::StructuredExtract,
            ),
            (
                "application/x-research-info-systems",
                ExtractionStrategy::StructuredExtract,
            ),
            ("application/avro", ExtractionStrategy::StructuredExtract),
            (
                "application/vnd.apache.parquet",
                ExtractionStrategy::StructuredExtract,
            ),
            (
                "application/x-ndjson",
                ExtractionStrategy::StructuredExtract,
            ),
            (
                "application/geo+json",
                ExtractionStrategy::StructuredExtract,
            ),
            (
                "application/x-drawio",
                ExtractionStrategy::StructuredExtract,
            ),
            (
                "application/x-excalidraw+json",
                ExtractionStrategy::StructuredExtract,
            ),
            ("text/calendar", ExtractionStrategy::StructuredExtract),
            // Text/Native
            ("text/plain", ExtractionStrategy::TextNative),
            ("text/markdown", ExtractionStrategy::TextNative),
            ("text/html", ExtractionStrategy::TextNative),
            ("text/css", ExtractionStrategy::TextNative),
            ("text/javascript", ExtractionStrategy::TextNative),
            ("text/x-python", ExtractionStrategy::TextNative),
            ("text/x-rust", ExtractionStrategy::TextNative),
            ("text/x-c", ExtractionStrategy::TextNative),
            ("text/x-java", ExtractionStrategy::TextNative),
            ("text/x-go", ExtractionStrategy::TextNative),
        ];

        for (mime, expected) in &expectations {
            assert_eq!(
                ExtractionStrategy::from_mime_type(mime),
                *expected,
                "MIME type '{}' mapped to {:?}, expected {:?}",
                mime,
                ExtractionStrategy::from_mime_type(mime),
                expected
            );
        }
    }
}

/// Extracted temporal and spatial provenance from file metadata (EXIF, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedProvenance {
    // Temporal
    pub capture_time: Option<chrono::DateTime<chrono::Utc>>,
    pub original_timezone: Option<String>,
    pub duration_seconds: Option<f64>,

    // Spatial
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude_m: Option<f64>,

    // Device
    pub device_make: Option<String>,
    pub device_model: Option<String>,
    pub software: Option<String>,

    // Raw metadata preservation
    pub raw_exif: serde_json::Value,
}

// =============================================================================
// SPECIALIZED MEDIA METADATA TYPES (Issues #438, #439)
// =============================================================================

/// 3D model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model3dMetadata {
    pub id: Uuid,
    pub attachment_id: Uuid,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertex_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds_min: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds_max: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_area: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_watertight: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_manifold: Option<bool>,
    #[serde(default)]
    pub material_count: i32,
    #[serde(default)]
    pub texture_count: i32,
    #[serde(default)]
    pub has_vertex_colors: bool,
    #[serde(default)]
    pub has_uv_mapping: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_attachment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Structured media metadata (SVG, MIDI, diagrams, trackers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredMediaMetadata {
    pub id: Uuid,
    pub attachment_id: Uuid,
    pub format: String,
    pub format_category: String,

    // SVG fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_element_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg_text_content: Option<String>,

    // MIDI fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_duration_seconds: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_tempo_bpm: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_time_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_track_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_channel_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_note_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_instrument_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_pitch_range_low: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_pitch_range_high: Option<i32>,

    // Tracker fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_pattern_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_order_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_channel_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_sample_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker_sample_names: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker_instrument_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracker_software: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demoscene_era: Option<String>,

    // Diagram fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_node_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram_edge_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagram_labels: Option<Vec<String>>,

    // Preview/render
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_attachment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_preview_attachment_id: Option<Uuid>,

    // Combined text for FTS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_combined: Option<String>,

    pub created_at: DateTime<Utc>,
}

// =============================================================================
// MEMORY SEARCH TYPES (Temporal-Spatial Provenance)
// =============================================================================

/// Result from spatial memory search (find_memories_near).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLocationResult {
    /// Provenance ID (None for note-metadata matches without file provenance).
    pub provenance_id: Option<Uuid>,
    /// Attachment ID (None for note-metadata matches without file provenance).
    pub attachment_id: Option<Uuid>,
    pub note_id: Uuid,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub distance_m: f64,
    pub capture_time_start: Option<DateTime<Utc>>,
    pub capture_time_end: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub event_type: Option<String>,
}

/// Result from temporal memory search (find_memories_in_timerange).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimeResult {
    /// Provenance ID (None for note-metadata matches without file provenance).
    pub provenance_id: Option<Uuid>,
    /// Attachment ID (None for note-metadata matches without file provenance).
    pub attachment_id: Option<Uuid>,
    pub note_id: Uuid,
    pub capture_time_start: Option<DateTime<Utc>>,
    pub capture_time_end: Option<DateTime<Utc>>,
    pub event_type: Option<String>,
    pub location_name: Option<String>,
}

/// Location information for a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLocation {
    pub id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub horizontal_accuracy_m: Option<f32>,
    pub altitude_m: Option<f32>,
    pub vertical_accuracy_m: Option<f32>,
    pub heading_degrees: Option<f32>,
    pub speed_mps: Option<f32>,
    pub named_location_id: Option<Uuid>,
    pub named_location_name: Option<String>,
    pub source: String,
    pub confidence: String,
}

/// Device information for a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDevice {
    pub id: Uuid,
    pub device_make: Option<String>,
    pub device_model: Option<String>,
    pub device_os: Option<String>,
    pub device_os_version: Option<String>,
    pub software: Option<String>,
    pub software_version: Option<String>,
    pub device_name: Option<String>,
}

/// Provenance record with full context (supports both file and note targets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<Uuid>,
    pub capture_time_start: Option<DateTime<Utc>>,
    pub capture_time_end: Option<DateTime<Utc>>,
    pub capture_timezone: Option<String>,
    pub capture_duration_seconds: Option<f32>,
    pub time_source: Option<String>,
    pub time_confidence: String,
    pub location: Option<MemoryLocation>,
    pub device: Option<MemoryDevice>,
    pub event_type: Option<String>,
    pub event_title: Option<String>,
    pub event_description: Option<String>,
    pub user_corrected: bool,
    pub created_at: DateTime<Utc>,
}

/// Backward-compatible alias.
pub type FileProvenanceRecord = ProvenanceRecord;

/// Complete provenance chain for a note's memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryProvenance {
    pub note_id: Uuid,
    pub files: Vec<ProvenanceRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<ProvenanceRecord>,
}

// =============================================================================
// PROVENANCE REQUEST TYPES
// =============================================================================

/// Request to create a provenance location record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateProvLocationRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: Option<f32>,
    pub horizontal_accuracy_m: Option<f32>,
    pub vertical_accuracy_m: Option<f32>,
    pub heading_degrees: Option<f32>,
    pub speed_mps: Option<f32>,
    pub named_location_id: Option<Uuid>,
    pub source: String, // gps_exif, device_api, user_manual, geocoded, ai_estimated
    pub confidence: String, // high, medium, low, unknown
}

/// Request to create a named location (landmark, address).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNamedLocationRequest {
    pub name: String,
    pub location_type: String, // home, work, poi, city, region, country
    pub latitude: f64,
    pub longitude: f64,
    pub radius_m: Option<f64>,
    pub address_line: Option<String>,
    pub locality: Option<String>,
    pub admin_area: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub postal_code: Option<String>,
    pub timezone: Option<String>,
    pub altitude_m: Option<f32>,
    pub is_private: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

/// Request to create a provenance device record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateProvDeviceRequest {
    pub device_make: String,
    pub device_model: String,
    pub device_os: Option<String>,
    pub device_os_version: Option<String>,
    pub software: Option<String>,
    pub software_version: Option<String>,
    pub has_gps: Option<bool>,
    pub has_accelerometer: Option<bool>,
    pub sensor_metadata: Option<serde_json::Value>,
    pub device_name: Option<String>,
}

/// Request to create a file provenance record linking an attachment to spatial-temporal context.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateFileProvenanceRequest {
    pub attachment_id: Uuid,
    pub note_id: Option<Uuid>,
    pub capture_time_start: Option<DateTime<Utc>>,
    pub capture_time_end: Option<DateTime<Utc>>,
    pub capture_timezone: Option<String>,
    pub capture_duration_seconds: Option<f32>,
    pub time_source: Option<String>, // exif, file_mtime, user_manual, ai_estimated, device_clock
    pub time_confidence: Option<String>, // high, medium, low, unknown
    pub location_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub event_type: Option<String>, // photo, video, audio, scan, screenshot, recording
    pub event_title: Option<String>,
    pub event_description: Option<String>,
    pub raw_metadata: Option<serde_json::Value>,
}

/// Request to create a note provenance record linking a note to spatial-temporal context.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNoteProvenanceRequest {
    pub note_id: Uuid,
    pub capture_time_start: Option<DateTime<Utc>>,
    pub capture_time_end: Option<DateTime<Utc>>,
    pub capture_timezone: Option<String>,
    pub time_source: Option<String>, // gps, network, manual, file_metadata, device_clock
    pub time_confidence: Option<String>, // exact, approximate, estimated
    pub location_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub event_type: Option<String>, // created, modified, accessed, shared
    pub event_title: Option<String>,
    pub event_description: Option<String>,
}

// =============================================================================
// WEBHOOK TYPES (Issue #44)
// =============================================================================

/// Webhook configuration for outbound HTTP notifications.
#[derive(Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: Uuid,
    pub url: String,
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub failure_count: i32,
    pub max_retries: i32,
}

impl std::fmt::Debug for Webhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Webhook")
            .field("id", &self.id)
            .field("url_len", &self.url.chars().count())
            .field("secret_set", &self.secret.is_some())
            .field("event_count", &self.events.len())
            .field("is_active", &self.is_active)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("last_triggered_at", &self.last_triggered_at)
            .field("failure_count", &self.failure_count)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

/// A record of a webhook delivery attempt.
#[derive(Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event_type: String,
    pub payload: JsonValue,
    pub status_code: Option<i32>,
    pub response_body: Option<String>,
    pub delivered_at: DateTime<Utc>,
    pub success: bool,
}

impl std::fmt::Debug for WebhookDelivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookDelivery")
            .field("id", &self.id)
            .field("webhook_id", &self.webhook_id)
            .field("event_type_len", &self.event_type.chars().count())
            .field("payload_class", &json_value_class(&self.payload))
            .field("payload_len", &self.payload.to_string().chars().count())
            .field("status_code", &self.status_code)
            .field(
                "response_body_len",
                &self.response_body.as_ref().map(|body| body.chars().count()),
            )
            .field("delivered_at", &self.delivered_at)
            .field("success", &self.success)
            .finish()
    }
}

/// Request to create a new webhook.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateWebhookRequest {
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
}

impl std::fmt::Debug for CreateWebhookRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateWebhookRequest")
            .field("url_len", &self.url.chars().count())
            .field("secret_set", &self.secret.is_some())
            .field("event_count", &self.events.len())
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

fn default_max_retries() -> i32 {
    crate::defaults::JOB_MAX_RETRIES
}

/// Registration for an incoming webhook receiver.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IncomingWebhookReceiver {
    pub id: Uuid,
    pub slug: String,
    pub provider: String,
    pub schema_ref: String,
    pub signature_header: String,
    pub secret_set: bool,
    pub is_active: bool,
    /// Custom JSON Schema document validated against incoming bodies (#821).
    /// When absent, validation falls back to the built-in schema named by
    /// `schema_ref` (e.g. `twilio.voice.v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_doc: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for IncomingWebhookReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncomingWebhookReceiver")
            .field("id", &self.id)
            .field("slug_len", &self.slug.chars().count())
            .field("provider_len", &self.provider.chars().count())
            .field("schema_ref_len", &self.schema_ref.chars().count())
            .field(
                "signature_header_len",
                &self.signature_header.chars().count(),
            )
            .field("secret_set", &self.secret_set)
            .field("is_active", &self.is_active)
            .field(
                "schema_doc_class",
                &self.schema_doc.as_ref().map(json_value_class),
            )
            .field(
                "schema_doc_len",
                &self
                    .schema_doc
                    .as_ref()
                    .map(|schema_doc| schema_doc.to_string().chars().count()),
            )
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Request to register an incoming webhook receiver.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateIncomingWebhookReceiverRequest {
    pub slug: String,
    pub provider: String,
    pub schema_ref: String,
    pub hmac_secret: String,
    #[serde(default = "default_incoming_webhook_signature_header")]
    pub signature_header: String,
    #[serde(default = "default_incoming_webhook_active")]
    pub is_active: bool,
    /// Optional custom JSON Schema document (#821). When provided, incoming
    /// bodies are validated against it via the `jsonschema` crate. When
    /// omitted, `schema_ref` must name a built-in schema.
    #[serde(default)]
    pub schema_doc: Option<JsonValue>,
}

impl std::fmt::Debug for CreateIncomingWebhookReceiverRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateIncomingWebhookReceiverRequest")
            .field("slug_len", &self.slug.chars().count())
            .field("provider_len", &self.provider.chars().count())
            .field("schema_ref_len", &self.schema_ref.chars().count())
            .field("hmac_secret_set", &!self.hmac_secret.is_empty())
            .field(
                "signature_header_len",
                &self.signature_header.chars().count(),
            )
            .field("is_active", &self.is_active)
            .field(
                "schema_doc_class",
                &self.schema_doc.as_ref().map(json_value_class),
            )
            .finish()
    }
}

/// Request to update an incoming webhook receiver in place (#821 PATCH).
///
/// All fields are optional; only the provided fields change. The receiver's
/// slug, provider, and HMAC secret are preserved across updates.
#[derive(Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateIncomingWebhookReceiverRequest {
    #[serde(default)]
    pub schema_ref: Option<String>,
    /// Replace the custom JSON Schema document. Omitting it (or sending
    /// `null`) leaves the existing schema unchanged.
    #[serde(default)]
    pub schema_doc: Option<JsonValue>,
    #[serde(default)]
    pub signature_header: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

impl std::fmt::Debug for UpdateIncomingWebhookReceiverRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateIncomingWebhookReceiverRequest")
            .field(
                "schema_ref_len",
                &self.schema_ref.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "schema_doc_class",
                &self.schema_doc.as_ref().map(json_value_class),
            )
            .field(
                "schema_doc_len",
                &self
                    .schema_doc
                    .as_ref()
                    .map(|schema_doc| schema_doc.to_string().chars().count()),
            )
            .field(
                "signature_header_len",
                &self
                    .signature_header
                    .as_ref()
                    .map(|value| value.chars().count()),
            )
            .field("is_active", &self.is_active)
            .finish()
    }
}

/// Request to validate a payload against a registered incoming webhook schema.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct ValidateIncomingWebhookPayloadRequest {
    pub schema_ref: String,
    pub payload: JsonValue,
}

impl std::fmt::Debug for ValidateIncomingWebhookPayloadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateIncomingWebhookPayloadRequest")
            .field("schema_ref_len", &self.schema_ref.chars().count())
            .field("payload_class", &json_value_class(&self.payload))
            .field("payload_len", &self.payload.to_string().chars().count())
            .finish()
    }
}

/// Response returned by incoming webhook schema validation.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IncomingWebhookValidationResponse {
    pub valid: bool,
    pub schema_ref: String,
    pub errors: Vec<String>,
}

impl std::fmt::Debug for IncomingWebhookValidationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncomingWebhookValidationResponse")
            .field("valid", &self.valid)
            .field("schema_ref_len", &self.schema_ref.chars().count())
            .field("error_count", &self.errors.len())
            .field(
                "error_lens",
                &self
                    .errors
                    .iter()
                    .map(|error| error.chars().count())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

fn default_incoming_webhook_signature_header() -> String {
    "X-Fortemi-Signature".to_string()
}

fn default_incoming_webhook_active() -> bool {
    true
}

fn json_value_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

/// A registered inbound external event source connector (#833, Phase D).
///
/// `kind` selects a connector implementation (e.g. `redis-stream`, `sse`,
/// `kafka`); `config` is an opaque JSON document interpreted by that connector.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboundSource {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub config: JsonValue,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for InboundSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InboundSource")
            .field("id", &self.id)
            .field("name_len", &self.name.chars().count())
            .field("kind_len", &self.kind.chars().count())
            .field("config_class", &json_value_class(&self.config))
            .field("config_len", &self.config.to_string().chars().count())
            .field(
                "config_key_count",
                &self.config.as_object().map(serde_json::Map::len),
            )
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Request to register an inbound event source connector (#833).
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateInboundSourceRequest {
    pub name: String,
    pub kind: String,
    #[serde(default = "default_inbound_source_config")]
    pub config: JsonValue,
    #[serde(default = "default_inbound_source_enabled")]
    pub enabled: bool,
}

impl std::fmt::Debug for CreateInboundSourceRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateInboundSourceRequest")
            .field("name_len", &self.name.chars().count())
            .field("kind_len", &self.kind.chars().count())
            .field("config_class", &json_value_class(&self.config))
            .field("config_len", &self.config.to_string().chars().count())
            .field(
                "config_key_count",
                &self.config.as_object().map(serde_json::Map::len),
            )
            .field("enabled", &self.enabled)
            .finish()
    }
}

fn default_inbound_source_config() -> JsonValue {
    JsonValue::Object(serde_json::Map::new())
}

fn default_inbound_source_enabled() -> bool {
    true
}
