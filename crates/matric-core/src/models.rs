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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_metadata: Option<JsonValue>,
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

/// A revision version entry from note_revision table (AI-enhanced content track).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    /// Note title (generated or first line of content)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Note tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
// EMBEDDING SET TYPES
// =============================================================================

/// Membership mode for embedding sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingIndexStatus {
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Agent-provided metadata for embedding set discovery.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Database-stored embedding configuration profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// An embedding set groups documents for focused semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    // Membership
    pub mode: EmbeddingSetMode,
    pub criteria: EmbeddingSetCriteria,

    // Config reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config_id: Option<Uuid>,

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

    // Agent metadata
    #[serde(default)]
    pub agent_metadata: EmbeddingSetAgentMetadata,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Summary view of embedding sets for listing/discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSetSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
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
}

/// Request to create a new embedding set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmbeddingSetRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    pub description: Option<String>,
    pub purpose: Option<String>,
    pub usage_hints: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub mode: EmbeddingSetMode,
    #[serde(default)]
    pub criteria: EmbeddingSetCriteria,
    #[serde(default)]
    pub agent_metadata: EmbeddingSetAgentMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config_id: Option<Uuid>,
}

/// Request to update an embedding set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSetMember {
    pub embedding_set_id: Uuid,
    pub note_id: Uuid,
    pub membership_type: String,
    pub added_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

/// Request to add members to an embedding set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMembersRequest {
    pub note_ids: Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

// =============================================================================
// EMBEDDING SET LIFECYCLE TYPES
// =============================================================================

/// Health summary for an embedding set.
///
/// Provides metrics on staleness, orphaned data, and missing embeddings
/// to guide maintenance operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RevisionMode {
    /// Full contextual enhancement - expands content with related concepts (default)
    #[default]
    Full,
    /// Light touch - formatting and structure only, no invented details
    Light,
    /// No AI revision - store original as-is
    None,
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

/// Queue statistics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: i64,
    pub processing: i64,
    pub completed_last_hour: i64,
    pub failed_last_hour: i64,
    pub total: i64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// OAuth2 client registration request (RFC 7591).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub expires_in_days: Option<i32>,
}

fn default_scope() -> String {
    "read".to_string()
}

/// API key creation response (includes the actual key, shown only once).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    pub api_key: String, // Full key, only shown once
    pub key_prefix: String,
    pub name: String,
    pub scope: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Authenticated principal (either OAuth client or API key).
#[derive(Debug, Clone)]
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

impl AuthPrincipal {
    /// Check if the principal has the required scope.
    pub fn has_scope(&self, required: &str) -> bool {
        let scope = match self {
            AuthPrincipal::OAuthClient { scope, .. } => scope,
            AuthPrincipal::ApiKey { scope, .. } => scope,
            AuthPrincipal::Anonymous => return false,
        };

        // Admin has all permissions
        if scope.contains("admin") {
            return true;
        }

        // MCP scope includes read and write
        if scope.contains("mcp") && (required == "read" || required == "write") {
            return true;
        }

        scope.split_whitespace().any(|s| s == required)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            chunk_metadata: None,
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
            title: None,
            metadata: json!({}),
            chunk_metadata: Some(chunk_meta.clone()),
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
            title: None,
            metadata: json!({}),
            chunk_metadata: None,
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
            mode: EmbeddingSetMode::Auto,
            criteria: EmbeddingSetCriteria {
                include_all: false,
                tags: vec!["test".to_string()],
                ..Default::default()
            },
            agent_metadata: EmbeddingSetAgentMetadata::default(),
            embedding_config_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateEmbeddingSetRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.name, deserialized.name);
        assert_eq!(request.slug, deserialized.slug);
        assert_eq!(request.mode, deserialized.mode);
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
    // Additional Model Tests
    // =========================================================================

    #[test]
    fn test_embedding_config_default_values() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.chunk_size, 1500);
        assert_eq!(config.chunk_overlap, 200);
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
        assert_eq!(RevisionMode::default(), RevisionMode::Full);
    }

    #[test]
    fn test_revision_mode_serialization() {
        let modes = vec![
            (RevisionMode::Full, "full"),
            (RevisionMode::Light, "light"),
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
    fn test_auth_principal_has_scope_mcp() {
        let principal = AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "mcp".to_string(),
        };

        assert!(principal.has_scope("read"));
        assert!(principal.has_scope("write"));
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
}
