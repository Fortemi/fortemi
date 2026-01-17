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
        assert!(JobType::Embedding.default_priority() > JobType::CreateEmbeddingSet.default_priority());
        assert!(JobType::Embedding.default_priority() > JobType::RefreshEmbeddingSet.default_priority());
        assert!(JobType::BuildSetIndex.default_priority() > JobType::CreateEmbeddingSet.default_priority());
    }
}
