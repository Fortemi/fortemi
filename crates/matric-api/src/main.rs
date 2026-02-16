//! matric-api - HTTP API server for matric-memory

mod handlers;
mod middleware;
mod query_types;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::Datelike;
use query_types::FlexibleDateTime;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Extension, Path, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Sse,
    },
    routing::{delete, get, patch, post, put},
    Form, Json, Router,
};
use base64::Engine;
use governor::{Quota, RateLimiter};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::{Config, SwaggerUi};
use uuid::Uuid;

use matric_core::EmbeddingBackend;
use matric_core::{
    AuthPrincipal, AuthorizationServerMetadata, BatchTagNoteRequest, ClientRegistrationRequest,
    CreateApiKeyRequest, CreateNoteRequest, DocumentTypeRepository, EventBus, ExtractionAdapter,
    ExtractionStrategy, JobRepository, JobType, LinkRepository, ListNotesRequest, NoteRepository,
    OAuthError, RevisionMode, ServerEvent, StrictTagFilterInput, TagInput, TagRepository,
    TokenRequest, UpdateNoteStatusRequest,
};
use matric_db::{Database, FilesystemBackend};
use middleware::archive_routing::{
    archive_routing_middleware, ArchiveContext, DefaultArchiveCache,
};
use tokio::sync::RwLock;

// =============================================================================
// REQUEST ID (UUIDv7)
// =============================================================================

/// Generates time-ordered UUIDv7 request correlation IDs.
///
/// UUIDv7 embeds a Unix timestamp, so IDs sort chronologically — useful for
/// log correlation, distributed tracing, and debugging production incidents.
#[derive(Clone, Default)]
struct MakeRequestUuidV7;

impl MakeRequestId for MakeRequestUuidV7 {
    fn make_request_id<B>(&mut self, _request: &axum::http::Request<B>) -> Option<RequestId> {
        let id = Uuid::now_v7().to_string().parse().ok()?;
        Some(RequestId::new(id))
    }
}

// =============================================================================
// NLP PIPELINE HELPER
// =============================================================================

/// Queue the full NLP pipeline for a note.
///
/// This function queues all necessary processing jobs for a note in the correct order:
/// 1. AI Revision (priority 8) - Enhance content with context
/// 2. Embedding (priority 5) - Generate vector embeddings for semantic search
/// 3. Title Generation (priority 2) - Generate descriptive title
/// 4. Linking (priority 3) - Create semantic links to related notes
///
/// The `revision_mode` parameter controls AI revision behavior:
/// - `Full`: Aggressive expansion with context from related notes (default)
/// - `Light`: Structure and formatting only, no invented details
/// - `None`: Skip AI revision entirely, store original as-is
///
/// The `schema` parameter specifies which PostgreSQL schema to use for job execution:
/// - `None` (default): Use the public schema (backward compatible)
/// - `Some("archive_name")`: Use the specified archive schema for parallel memory isolation
///
/// Queue an extraction job for a newly uploaded attachment.
///
/// This should be called after storing the attachment and committing the transaction.
/// The extraction handler will fetch the file data from storage using the attachment_id.
#[allow(clippy::too_many_arguments)]
async fn queue_extraction_job(
    db: &Database,
    note_id: Uuid,
    attachment_id: Uuid,
    strategy: ExtractionStrategy,
    filename: &str,
    content_type: &str,
    event_bus: &EventBus,
    schema: Option<&str>,
) {
    let mut payload = serde_json::json!({
        "strategy": strategy.to_string(),
        "attachment_id": attachment_id.to_string(),
        "filename": filename,
        "mime_type": content_type,
    });
    if let Some(s) = schema {
        payload["schema"] = serde_json::json!(s);
    }

    match db
        .jobs
        .queue_deduplicated(
            Some(note_id),
            JobType::Extraction,
            JobType::Extraction.default_priority(),
            Some(payload),
        )
        .await
    {
        Ok(Some(job_id)) => {
            event_bus.emit(ServerEvent::JobQueued {
                job_id,
                job_type: format!("{:?}", JobType::Extraction),
                note_id: Some(note_id),
            });
        }
        Ok(None) => {} // Deduplicated — job already pending
        Err(e) => {
            error!(%note_id, %attachment_id, error = %e, "Failed to queue extraction job");
        }
    }
}

/// Queue an EXIF extraction job for image attachments.
///
/// This should be called after storing image attachments. The handler extracts
/// EXIF metadata (camera info, GPS coordinates, datetime) and creates provenance
/// records (location, device, file).
async fn queue_exif_extraction_job(
    db: &Database,
    note_id: Uuid,
    attachment_id: Uuid,
    content_type: &str,
    event_bus: &EventBus,
    schema: Option<&str>,
) {
    // Only queue for image content types
    if !content_type.starts_with("image/") {
        return;
    }

    let mut payload = serde_json::json!({
        "attachment_id": attachment_id.to_string(),
    });
    if let Some(s) = schema {
        payload["schema"] = serde_json::json!(s);
    }

    match db
        .jobs
        .queue_deduplicated(
            Some(note_id),
            JobType::ExifExtraction,
            JobType::ExifExtraction.default_priority(),
            Some(payload),
        )
        .await
    {
        Ok(Some(job_id)) => {
            event_bus.emit(ServerEvent::JobQueued {
                job_id,
                job_type: format!("{:?}", JobType::ExifExtraction),
                note_id: Some(note_id),
            });
        }
        Ok(None) => {} // Deduplicated — job already pending
        Err(e) => {
            error!(%note_id, %attachment_id, error = %e, "Failed to queue EXIF extraction job");
        }
    }
}

/// This should be called after:
/// - Creating a new note
/// - Updating note content
/// - Any operation that modifies content requiring re-indexing
///
/// Uses deduplicated queuing to prevent duplicate jobs for the same note/type.
async fn queue_nlp_pipeline(
    db: &Database,
    note_id: Uuid,
    revision_mode: RevisionMode,
    event_bus: &EventBus,
    schema: Option<&str>,
) {
    // Queue AI revision with mode and schema in payload (unless mode is None)
    if revision_mode != RevisionMode::None {
        let mut payload = serde_json::json!({
            "revision_mode": revision_mode
        });
        if let Some(s) = schema {
            payload["schema"] = serde_json::json!(s);
        }
        match db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::AiRevision,
                JobType::AiRevision.default_priority(),
                Some(payload),
            )
            .await
        {
            Ok(Some(job_id)) => {
                event_bus.emit(ServerEvent::JobQueued {
                    job_id,
                    job_type: format!("{:?}", JobType::AiRevision),
                    note_id: Some(note_id),
                });
            }
            Ok(None) => {} // Deduplicated
            Err(e) => {
                error!(%note_id, error = %e, "Failed to queue AI revision job");
            }
        }
    }

    // Queue remaining pipeline jobs (always run these)
    for job_type in [
        JobType::Embedding,
        JobType::TitleGeneration,
        JobType::Linking,
        JobType::ConceptTagging, // SKOS auto-tagging
    ] {
        // Create payload with schema if provided
        let payload = schema.map(|s| serde_json::json!({ "schema": s }));

        match db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                job_type,
                job_type.default_priority(),
                payload,
            )
            .await
        {
            Ok(Some(job_id)) => {
                event_bus.emit(ServerEvent::JobQueued {
                    job_id,
                    job_type: format!("{:?}", job_type),
                    note_id: Some(note_id),
                });
            }
            Ok(None) => {} // Deduplicated
            Err(e) => {
                error!(%note_id, job_type = ?job_type, error = %e, "Failed to queue NLP pipeline job");
            }
        }
    }
}
use matric_api::services::TagResolver;
use matric_inference::transcription::WhisperBackend;
use matric_inference::{
    transcription::TranscriptionBackend, OllamaBackend, OllamaVisionBackend, VisionBackend,
};
use matric_jobs::{
    AudioTranscribeAdapter, CodeAstAdapter, ExtractionHandler, ExtractionRegistry,
    Glb3DModelAdapter, JobWorker, PdfTextAdapter, StructuredExtractAdapter, TextNativeAdapter,
    VideoMultimodalAdapter, VisionAdapter, WorkerConfig, WorkerEvent,
};
use matric_search::{EnhancedSearchHit, HybridSearchConfig, HybridSearchEngine, SearchRequest};

use handlers::{
    archives::{
        clone_archive, create_archive, delete_archive, get_archive, get_archive_stats,
        list_archives, set_default_archive, update_archive,
    },
    audio::transcribe_audio,
    document_types::{
        create_document_type, delete_document_type, detect_document_type, get_document_type,
        list_document_types, update_document_type,
    },
    pke::{
        create_keyset, delete_keyset, export_keyset, get_active_keyset, import_keyset,
        list_keysets, pke_address, pke_decrypt, pke_encrypt, pke_keygen, pke_recipients,
        pke_verify, set_active_keyset,
    },
    provenance::{
        create_file_provenance, create_named_location, create_note_provenance, create_prov_device,
        create_prov_location,
    },
    vision::describe_image,
    AiRevisionHandler, ConceptTaggingHandler, ContextUpdateHandler, EmbeddingHandler,
    ExifExtractionHandler, LinkingHandler, PurgeNoteHandler, ReEmbedAllHandler,
    RefreshEmbeddingSetHandler, TitleGenerationHandler,
};

/// Global rate limiter type (direct quota, no keyed bucketing for personal server).
type GlobalRateLimiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

/// Application state shared across handlers.
#[derive(Clone)]
struct AppState {
    db: Database,
    search: Arc<HybridSearchEngine>,
    /// OAuth2 issuer URL (base URL of the server).
    issuer: String,
    /// Global rate limiter (None if rate limiting is disabled).
    rate_limiter: Option<Arc<GlobalRateLimiter>>,
    /// Tag resolver for strict filter resolution.
    tag_resolver: TagResolver,
    /// Redis search cache (reduces latency for repeated queries).
    search_cache: matric_api::services::SearchCache,
    /// Event bus for real-time notifications (WebSocket, SSE, webhooks, telemetry).
    event_bus: Arc<EventBus>,
    /// Active WebSocket connection count (Issue #42).
    ws_connections: Arc<AtomicUsize>,
    /// Cached default archive for routing middleware (Issue #107).
    default_archive_cache: Arc<RwLock<DefaultArchiveCache>>,
    /// Require authentication for protected routes (Issue #114).
    require_auth: bool,
    /// OAuth access token lifetime (standard clients).
    oauth_token_lifetime: chrono::Duration,
    /// OAuth access token lifetime (MCP clients).
    oauth_mcp_token_lifetime: chrono::Duration,
    /// Maximum number of memories (archives) allowed.
    max_memories: i64,
    /// Maximum file upload size in bytes (for attachment validation).
    max_upload_size: usize,
    /// Vision backend for ad-hoc image description (None if OLLAMA_VISION_MODEL not set).
    vision_backend: Option<Arc<dyn VisionBackend>>,
    /// Transcription backend for ad-hoc audio transcription (None if WHISPER_BASE_URL not set).
    transcription_backend: Option<Arc<dyn TranscriptionBackend>>,
    /// Git commit SHA at build time.
    git_sha: String,
    /// Build date (ISO 8601).
    build_date: String,
    /// Extraction strategies that are actually registered and available.
    extraction_strategies: Vec<String>,
    /// Cached per-schema search engines for non-default archives.
    schema_engines:
        Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<HybridSearchEngine>>>>,
    /// Database URL for creating per-schema connection pools.
    database_url: String,
}

/// OpenAPI documentation generated by utoipa from handler annotations.
///
/// All paths and schemas are auto-derived from `#[utoipa::path]` and `#[derive(ToSchema)]`
/// annotations throughout the codebase. Run `cargo test openapi_spec_is_current` to verify.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Matric Memory API",
        version = "2026.2.9",
        description = "AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines"
    ),
    servers((url = "http://localhost:3000")),
    paths(
        create_webhook, list_webhooks, get_webhook, update_webhook,
        delete_webhook_handler, list_webhook_deliveries, test_webhook, rate_limit_status,
        health_check, get_notes_timeline, get_notes_activity, get_knowledge_health,
        get_orphan_tags, get_stale_notes, get_unlinked_notes, get_tag_cooccurrence,
        list_notes, create_note, bulk_create_notes, get_note,
        update_note, delete_note, purge_note, update_note_status,
        restore_note, reprocess_note, bulk_reprocess_notes, get_note_tags, set_note_tags,
        list_tags, list_concept_schemes, create_concept_scheme, get_concept_scheme,
        update_concept_scheme, delete_concept_scheme, get_top_concepts, search_concepts,
        create_concept, autocomplete_concepts, get_concept, get_concept_full,
        update_concept, delete_concept, get_ancestors, get_descendants,
        get_broader, get_narrower, get_related, add_broader,
        add_narrower, add_related, remove_broader, remove_narrower,
        remove_related, get_note_concepts, tag_note_with_concept, untag_note_concept,
        get_governance_stats, export_scheme_turtle, export_all_schemes_turtle, list_skos_collections,
        create_skos_collection, get_skos_collection, update_skos_collection, delete_skos_collection,
        replace_skos_collection_members, add_skos_collection_member, remove_skos_collection_member, list_collections,
        create_collection, get_collection, update_collection, delete_collection,
        get_collection_notes, export_collection, move_note_to_collection, explore_graph, graph_topology_stats,
        list_templates, create_template, get_template, update_template,
        delete_template, instantiate_template, get_note_links, get_note_backlinks,
        get_note_provenance, search_memories, get_memory_provenance_handler, export_note,
        get_full_document, list_note_versions, get_note_version, restore_note_version,
        delete_note_version, diff_note_versions, search_notes, federated_search,
        memories_overview, list_embedding_sets, get_embedding_set, create_embedding_set,
        update_embedding_set, delete_embedding_set, list_embedding_set_members, add_embedding_set_members,
        remove_embedding_set_member, refresh_embedding_set, list_embedding_configs, get_default_embedding_config,
        get_embedding_config, create_embedding_config, update_embedding_config, delete_embedding_config,
        create_job, get_job, pending_jobs_count, list_jobs,
        queue_stats, extraction_stats, oauth_discovery, oauth_protected_resource,
        oauth_register, oauth_token, oauth_introspect, oauth_revoke,
        oauth_authorize_get, oauth_authorize_post, list_api_keys, create_api_key,
        revoke_api_key, backup_export, backup_download, backup_import,
        backup_trigger, backup_status, knowledge_shard, knowledge_shard_import,
        list_attachments, upload_attachment, upload_attachment_multipart, get_attachment,
        download_attachment, delete_attachment, list_backups, get_backup_info,
        swap_backup, memory_backup_download, database_backup_download, database_backup_snapshot,
        database_backup_upload, database_backup_restore, knowledge_archive_download, knowledge_archive_upload,
        get_backup_metadata, update_backup_metadata, memory_info,
        // handlers::archives
        handlers::archives::list_archives, handlers::archives::get_archive,
        handlers::archives::create_archive, handlers::archives::update_archive,
        handlers::archives::delete_archive, handlers::archives::set_default_archive,
        handlers::archives::get_archive_stats, handlers::archives::clone_archive,
        // handlers::document_types
        handlers::document_types::list_document_types, handlers::document_types::get_document_type,
        handlers::document_types::create_document_type, handlers::document_types::update_document_type,
        handlers::document_types::delete_document_type, handlers::document_types::detect_document_type,
        // handlers::vision
        handlers::vision::describe_image,
        // handlers::audio
        handlers::audio::transcribe_audio,
        // handlers::pke
        handlers::pke::pke_keygen, handlers::pke::pke_address,
        handlers::pke::pke_encrypt, handlers::pke::pke_decrypt,
        handlers::pke::pke_recipients, handlers::pke::pke_verify,
        // handlers::provenance
        handlers::provenance::create_prov_location, handlers::provenance::create_named_location,
        handlers::provenance::create_prov_device, handlers::provenance::create_file_provenance,
        handlers::provenance::create_note_provenance,
    ),
    components(
        schemas(
            matric_core::AddLabelRequest, matric_core::AddMembersRequest, matric_core::AddNoteRequest,
            matric_core::AgenticConfig, matric_core::Attachment, matric_core::AttachmentBlob,
            matric_core::AttachmentSearchRequest, matric_core::AttachmentSearchResponse, matric_core::AttachmentSummary,
            matric_core::AutoEmbedRules, matric_core::BatchTagNoteRequest, matric_core::ClientRegistrationRequest,
            matric_core::CreateApiKeyRequest, matric_core::CreateConceptRequest, matric_core::CreateConceptSchemeRequest,
            matric_core::CreateDocumentTypeRequest, matric_core::CreateEmbeddingConfigRequest, matric_core::CreateEmbeddingSetRequest,
            matric_core::CreateFineTuningDatasetRequest, matric_core::CreateMappingRelationRequest, matric_core::CreateSemanticRelationRequest,
            matric_core::CreateSkosCollectionRequest, matric_core::CreateWebhookRequest, matric_core::CrossArchiveSearchRequest,
            matric_core::CrossArchiveSearchResponse, matric_core::CrossArchiveSearchResult, matric_core::DetectDocumentTypeResult,
            matric_core::DocumentType, matric_core::DocumentTypeSummary, matric_core::DublinCoreExport,
            matric_core::EmbeddingConfig, matric_core::EmbeddingConfigProfile, matric_core::EmbeddingSet,
            matric_core::EmbeddingSetAgentMetadata, matric_core::EmbeddingSetCriteria, matric_core::EmbeddingSetHealth,
            matric_core::EmbeddingSetMember, matric_core::EmbeddingSetSummary, matric_core::EntityStats,
            matric_core::FairScore, matric_core::FineTuningConfig, matric_core::FineTuningDataset,
            matric_core::FineTuningSample, matric_core::GarbageCollectionResult, matric_core::JsonLdContext,
            matric_core::JsonLdExport, matric_core::Link, matric_core::MemoryHit,
            matric_core::MemorySearchResponse, matric_core::MergeConceptsRequest, matric_core::NoteEntity,
            matric_core::NoteFull, matric_core::NoteMeta, matric_core::NoteOriginal,
            matric_core::NoteRevised, matric_core::NoteSkosConceptTag, matric_core::NoteSummary,
            matric_core::ResolvedTag, matric_core::RevisionVersion, matric_core::SearchConceptsRequest,
            matric_core::SearchConceptsResponse, matric_core::SearchHit, matric_core::SearchResponse,
            matric_core::SemanticResponse, matric_core::SkosAuditLogEntry, matric_core::SkosCollection,
            matric_core::SkosCollectionMember, matric_core::SkosCollectionWithMembers, matric_core::SkosConcept,
            matric_core::SkosConceptFull, matric_core::SkosConceptHierarchy, matric_core::SkosConceptLabel,
            matric_core::SkosConceptMerge, matric_core::SkosConceptNote, matric_core::SkosConceptScheme,
            matric_core::SkosConceptSchemeSummary, matric_core::SkosConceptSummary, matric_core::SkosConceptWithLabel,
            matric_core::SkosGovernanceStats, matric_core::SkosMappingRelationEdge, matric_core::SkosSemanticRelationEdge,
            matric_core::SkosTagSpec, matric_core::StrictTagFilter, matric_core::StrictTagFilterInput,
            matric_core::TagInput, matric_core::TagNoteRequest, matric_core::TimelineGroup,
            matric_core::TimelineResponse, matric_core::TriModalWeights, matric_core::TwoStageSearchConfig,
            matric_core::UpdateCollectionMembersRequest, matric_core::UpdateConceptRequest, matric_core::UpdateConceptSchemeRequest,
            matric_core::UpdateDocumentTypeRequest, matric_core::UpdateEmbeddingConfigRequest, matric_core::UpdateEmbeddingSetRequest,
            matric_core::UpdateSkosCollectionRequest, AddMemberBody, BackupImportBody,
            BulkCreateNotesBody, CreateNoteBody,
            PaginationMeta, ReprocessNoteBody, SetTagsBody,
            UpdateNoteBody, UpdateStatusBody, UpdateWebhookBody,
        )
    ),
    tags(
        (name = "Notes", description = "Note CRUD operations"),
        (name = "Tags", description = "Tag management"),
        (name = "Search", description = "Full-text and semantic search"),
        (name = "Jobs", description = "Background job management"),
        (name = "OAuth", description = "OAuth 2.1 authorization server"),
        (name = "System", description = "Health checks and system info"),
        (name = "SKOS", description = "W3C SKOS semantic taxonomy"),
        (name = "Collections", description = "Note collections and folders"),
        (name = "Embeddings", description = "Embedding sets and configurations"),
        (name = "Graph", description = "Knowledge graph exploration"),
        (name = "Backup", description = "Export, import, and backup"),
        (name = "Templates", description = "Note templates"),
        (name = "Webhooks", description = "Webhook management"),
        (name = "Attachments", description = "File attachments"),
        (name = "PKE", description = "Public key encryption"),
        (name = "Provenance", description = "W3C PROV provenance tracking"),
        (name = "Archives", description = "Memory archive management"),
        (name = "DocumentTypes", description = "Document type registry")
    )
)]
struct ApiDoc;

// =============================================================================
// STANDARD RESPONSE TYPES (Issue #465)
// =============================================================================

/// Standardized pagination metadata for list responses.
///
/// Provides consistent pagination information across all list endpoints,
/// enabling clients to implement proper pagination UI and infinite scrolling.
#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct PaginationMeta {
    /// Total number of items matching the query (across all pages)
    pub total: usize,
    /// Maximum number of items per page (request parameter)
    pub limit: usize,
    /// Number of items skipped (request parameter)
    pub offset: usize,
    /// True if more items are available after this page
    pub has_more: bool,
}

/// Standardized list response wrapper with pagination metadata.
///
/// All list endpoints should return this structure for consistency.
///
/// # Example Response
/// ```json
/// {
///   "data": [...],
///   "pagination": {
///     "total": 100,
///     "limit": 50,
///     "offset": 0,
///     "has_more": true
///   }
/// }
/// ```
#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct ListResponse<T> {
    /// The list of items for the current page
    pub data: Vec<T>,
    /// Pagination metadata
    pub pagination: PaginationMeta,
}

impl<T: Serialize> ListResponse<T> {
    /// Create a new paginated list response.
    ///
    /// Automatically calculates `has_more` based on offset, data length, and total count.
    ///
    /// # Arguments
    /// * `data` - The items for the current page
    /// * `total` - Total number of items across all pages
    /// * `limit` - Maximum items per page
    /// * `offset` - Number of items skipped
    pub fn new(data: Vec<T>, total: usize, limit: usize, offset: usize) -> Self {
        let has_more = offset + data.len() < total;
        Self {
            data,
            pagination: PaginationMeta {
                total,
                limit,
                offset,
                has_more,
            },
        }
    }
}

/// Serve OpenAPI YAML spec (auto-generated by utoipa from handler annotations).
async fn openapi_yaml() -> impl IntoResponse {
    let spec = ApiDoc::openapi()
        .to_yaml()
        .expect("OpenAPI YAML generation must not fail");
    ([(header::CONTENT_TYPE, "application/yaml")], spec)
}

// =============================================================================
// CORS CONFIGURATION HELPER (Issue #462)
// =============================================================================

/// Parse allowed origins from comma-separated environment variable.
///
/// # Security
/// This function enforces strict origin whitelisting for CORS, replacing the
/// insecure `.allow_origin(Any)` configuration that allowed any website to
/// make requests to the API.
///
/// # Environment Variable
/// `ALLOWED_ORIGINS` - Comma-separated list of allowed origins
///
/// # Default Origins
/// If not set or empty:
/// - http://localhost:3000
///
/// # Examples
/// ```bash
/// # Production
/// ALLOWED_ORIGINS=https://your-domain.com
///
/// # Development
/// ALLOWED_ORIGINS=https://your-domain.com,http://localhost:3000,https://staging.example.com
/// ```
fn parse_allowed_origins() -> Vec<HeaderValue> {
    let origins_str =
        std::env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:3000".to_string());

    if origins_str.trim().is_empty() {
        // Default origins
        return vec![HeaderValue::from_static("http://localhost:3000")];
    }

    origins_str
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            match trimmed.parse::<HeaderValue>() {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Invalid CORS origin '{}': {}", trimmed, e);
                    None
                }
            }
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing with configurable output
    //
    // Environment variables:
    //   LOG_FORMAT  - "json" or "text" (default: "text")
    //   LOG_FILE    - path to log file (optional, enables file logging)
    //   LOG_ANSI    - "true"/"false" override ANSI colors (auto-detected by default)
    //   RUST_LOG    - standard env filter (default: "matric_api=debug,tower_http=debug")
    let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());
    let log_file = std::env::var("LOG_FILE").ok();
    let log_ansi = std::env::var("LOG_ANSI")
        .ok()
        .map(|v| v == "true" || v == "1");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "matric_api=debug,tower_http=debug".into());

    let registry = tracing_subscriber::registry().with(env_filter);

    // Optionally create a file appender with daily rotation
    let _file_guard = if let Some(ref path) = log_file {
        let file_dir = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("matric-api.log");
        let file_appender = tracing_appender::rolling::daily(file_dir, file_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        if log_format == "json" {
            registry
                .with(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .with_writer(non_blocking),
                )
                .init();
        } else {
            let mut layer = tracing_subscriber::fmt::layer().with_writer(non_blocking);
            if let Some(ansi) = log_ansi {
                layer = layer.with_ansi(ansi);
            } else {
                layer = layer.with_ansi(false); // no ANSI in files
            }
            registry.with(layer).init();
        }
        Some(guard)
    } else {
        // Console-only output
        if log_format == "json" {
            registry
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        } else {
            let mut layer = tracing_subscriber::fmt::layer();
            if let Some(ansi) = log_ansi {
                layer = layer.with_ansi(ansi);
            }
            registry.with(layer).init();
        }
        None
    };

    info!(
        log_format = %log_format,
        log_file = log_file.as_deref().unwrap_or("(stdout)"),
        "Logging initialized"
    );

    // Get configuration from environment
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/matric".to_string());
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(matric_core::defaults::SERVER_PORT);

    // Rate limiting configuration (generous for personal server)
    // RATE_LIMIT_REQUESTS: requests per period (default: 100)
    // RATE_LIMIT_PERIOD_SECS: period in seconds (default: 60 = 1 minute)
    let rate_limit_requests: u64 = std::env::var("RATE_LIMIT_REQUESTS")
        .unwrap_or_else(|_| "100".to_string())
        .parse()
        .unwrap_or(matric_core::defaults::RATE_LIMIT_REQUESTS);
    let rate_limit_period_secs: u64 = std::env::var("RATE_LIMIT_PERIOD_SECS")
        .unwrap_or_else(|_| "60".to_string())
        .parse()
        .unwrap_or(matric_core::defaults::RATE_LIMIT_PERIOD_SECS);
    let rate_limit_enabled: bool = std::env::var("RATE_LIMIT_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    info!(
        "Rate limiting: {} ({} requests per {} seconds)",
        if rate_limit_enabled {
            "enabled"
        } else {
            "disabled"
        },
        rate_limit_requests,
        rate_limit_period_secs
    );

    // Connect to database
    info!("Connecting to database...");
    let db = Database::connect(&database_url).await?;
    info!("Database connected");

    // Run pending database migrations on startup
    info!("Running database migrations...");
    db.migrate().await?;
    info!("Database migrations complete");

    // Initialize file storage
    let file_storage_path =
        std::env::var("FILE_STORAGE_PATH").unwrap_or_else(|_| "/var/lib/matric/files".to_string());
    // Ensure storage directory exists on startup (issue #123, #154)
    if let Err(e) = tokio::fs::create_dir_all(&file_storage_path).await {
        error!(
            "Could not create file storage directory {}: {} — attachment uploads will fail!",
            file_storage_path, e
        );
    }
    // Validate storage works before accepting uploads (issue #150)
    {
        let backend = FilesystemBackend::new(&file_storage_path);
        match backend.validate().await {
            Ok(()) => info!("File storage validated at {}", file_storage_path),
            Err(e) => error!(
                "File storage validation FAILED at {}: {}",
                file_storage_path, e
            ),
        }
    }
    // Use with_filesystem_storage so Clone reconstructs the real backend (fix #154)
    let db = db.with_filesystem_storage(
        &file_storage_path,
        matric_core::defaults::FILE_INLINE_THRESHOLD as i64, // 1MB inline threshold
    );
    info!("File storage initialized at {}", file_storage_path);

    // Create search engine
    let search = Arc::new(HybridSearchEngine::new(db.clone()));

    // Verify inference backend is reachable
    {
        let backend = OllamaBackend::from_env();
        info!(
            "Inference backend initialized: {}",
            EmbeddingBackend::model_name(&backend)
        );
    }

    // Create and start job worker
    let worker_enabled = std::env::var("WORKER_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    // Read event bus capacity from env var with fallback
    let event_bus_capacity = std::env::var("MATRIC_EVENT_BUS_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(matric_core::defaults::EVENT_BUS_CAPACITY);

    // Create the event bus (Issue #38)
    let event_bus = Arc::new(EventBus::new(event_bus_capacity));

    // Create vision backend (shared between worker extraction pipeline and API describe endpoint).
    let vision_backend: Option<Arc<dyn VisionBackend>> =
        OllamaVisionBackend::from_env().map(|b| Arc::new(b) as Arc<dyn VisionBackend>);
    if let Some(ref backend) = vision_backend {
        info!("Vision backend available: model={}", backend.model_name());
    } else {
        info!("Vision backend disabled: OLLAMA_VISION_MODEL set to empty");
    }

    // Create transcription backend (shared between worker extraction pipeline and API transcribe endpoint).
    // We create a concrete WhisperBackend first for model preload, then wrap in Arc<dyn>.
    let whisper_preload = WhisperBackend::from_env();
    let transcription_backend: Option<Arc<dyn TranscriptionBackend>> =
        WhisperBackend::from_env().map(|b| Arc::new(b) as Arc<dyn TranscriptionBackend>);
    if let Some(ref backend) = transcription_backend {
        info!(
            "Transcription backend available: model={}",
            backend.model_name()
        );
    } else {
        info!("Transcription backend disabled: WHISPER_BASE_URL set to empty");
    }

    // Spawn background task to ensure whisper model is downloaded on the backend.
    // The speaches container doesn't honor PRELOAD_MODELS, so we trigger download via API.
    if let Some(wb) = whisper_preload {
        tokio::spawn(async move {
            match wb.ensure_model_available().await {
                Ok(()) => tracing::info!("Whisper model available and ready"),
                Err(e) => tracing::warn!("Whisper model preload failed: {e}"),
            }
        });
    }

    let mut active_extraction_strategies: Vec<String> = Vec::new();
    let _worker_handle = if worker_enabled {
        info!("Starting job worker...");

        // Build extraction adapter registry with conditional registration
        let mut extraction_registry = ExtractionRegistry::new();

        // Always available: text-native (no external deps)
        extraction_registry.register(Arc::new(TextNativeAdapter));
        info!("Extraction adapter registered: TextNative (always available)");

        // Always available: structured extract (no external deps)
        extraction_registry.register(Arc::new(StructuredExtractAdapter));
        info!("Extraction adapter registered: StructuredExtract (always available)");

        // Always available: code AST extraction for programming languages (no external deps)
        extraction_registry.register(Arc::new(CodeAstAdapter));
        info!("Extraction adapter registered: CodeAst (always available)");

        // Conditional: PdfText requires `pdftotext` binary in PATH
        if PdfTextAdapter.health_check().await.unwrap_or(false) {
            extraction_registry.register(Arc::new(PdfTextAdapter));
            info!("Extraction adapter registered: PdfText (pdftotext found)");
        } else {
            warn!("PdfTextAdapter disabled: pdftotext not found in PATH");
        }

        // Conditional: Vision model requires OLLAMA_VISION_MODEL env var
        // Reuses the shared vision_backend created above.
        if let Some(ref backend) = vision_backend {
            let adapter = VisionAdapter::new(Arc::clone(backend));
            info!(
                "Extraction adapter registered: Vision (model: {})",
                backend.model_name()
            );
            extraction_registry.register(Arc::new(adapter));
        }

        // Conditional: Audio transcription requires WHISPER_BASE_URL
        // Reuses the shared transcription_backend created above.
        if let Some(ref backend) = transcription_backend {
            let adapter = AudioTranscribeAdapter::new(Arc::clone(backend));
            info!(
                "Extraction adapter registered: AudioTranscribe (model: {})",
                backend.model_name()
            );
            extraction_registry.register(Arc::new(adapter));
        }

        // Conditional: Video multimodal requires ffmpeg + at least one backend (vision or transcription)
        // Reuses the shared vision_backend and transcription_backend created above.
        if vision_backend.is_some() || transcription_backend.is_some() {
            let adapter = VideoMultimodalAdapter::new(
                vision_backend.as_ref().map(Arc::clone),
                transcription_backend.as_ref().map(Arc::clone),
            );
            if adapter.health_check().await.unwrap_or(false) {
                info!(
                    "Extraction adapter registered: VideoMultimodal (vision: {}, transcription: {})",
                    vision_backend.is_some(),
                    transcription_backend.is_some(),
                );
                extraction_registry.register(Arc::new(adapter));
            } else {
                warn!("VideoMultimodalAdapter disabled: ffmpeg not found in PATH");
            }
        }

        // GLB 3D model requires Three.js renderer + vision backend.
        // Bundled in Docker bundle at http://localhost:8080, or set RENDERER_URL for external.
        if let Some(ref backend) = vision_backend {
            let adapter = Glb3DModelAdapter::new(Arc::clone(backend));
            let renderer_available = adapter.health_check().await.unwrap_or(false);
            let renderer_url = std::env::var("RENDERER_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string());
            if renderer_available {
                info!(
                    "Extraction adapter registered: Glb3DModel (model: {}, renderer: {})",
                    backend.model_name(),
                    renderer_url
                );
            } else {
                warn!(
                    "Extraction adapter registered: Glb3DModel (model: {}, renderer: NOT AVAILABLE at {} — \
                     ensure Three.js renderer is running)",
                    backend.model_name(),
                    renderer_url,
                );
            }
            extraction_registry.register(Arc::new(adapter));
        }

        // Conditional: OCR requires OCR_ENABLED=true
        let ocr_enabled = std::env::var("OCR_ENABLED")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        if ocr_enabled {
            info!("PdfOcrAdapter enabled via OCR_ENABLED=true");
            // Note: PdfOcrAdapter registration would go here when implemented
        } else {
            info!("PdfOcrAdapter disabled: OCR_ENABLED not set or false");
        }

        active_extraction_strategies = extraction_registry
            .available_strategies()
            .iter()
            .map(|s| s.to_string())
            .collect();
        info!(
            "Extraction adapters active: {:?}",
            active_extraction_strategies
        );
        info!("Unsupported MIME types will be stored without extraction");

        let worker = JobWorker::new(
            db.clone(),
            WorkerConfig::from_env(),
            Some(extraction_registry),
        );

        // Register handlers - create separate backend instances
        worker
            .register_handler(AiRevisionHandler::new(
                db.clone(),
                OllamaBackend::from_env(),
            ))
            .await;
        worker
            .register_handler(EmbeddingHandler::new(db.clone(), OllamaBackend::from_env()))
            .await;
        worker
            .register_handler(TitleGenerationHandler::new(
                db.clone(),
                OllamaBackend::from_env(),
            ))
            .await;
        worker
            .register_handler(LinkingHandler::new(db.clone()))
            .await;
        worker
            .register_handler(ContextUpdateHandler::new(
                db.clone(),
                OllamaBackend::from_env(),
            ))
            .await;
        worker
            .register_handler(PurgeNoteHandler::new(db.clone()))
            .await;
        worker
            .register_handler(ConceptTaggingHandler::new(
                db.clone(),
                OllamaBackend::from_env(),
            ))
            .await;
        worker
            .register_handler(ReEmbedAllHandler::new(db.clone()))
            .await;
        if let Some(registry) = worker.extraction_registry() {
            worker
                .register_handler(ExtractionHandler::new(db.clone(), registry.clone()))
                .await;
        }
        worker
            .register_handler(ExifExtractionHandler::new(db.clone()))
            .await;
        worker
            .register_handler(RefreshEmbeddingSetHandler::new(db.clone()))
            .await;

        let handle = worker.start();
        info!("Job worker started");

        // Bridge WorkerEvent → ServerEvent (Issue #40)
        let bridge_rx = handle.events();
        let bridge_bus = event_bus.clone();
        let bridge_db = db.clone();
        tokio::spawn(async move {
            bridge_worker_events(bridge_rx, bridge_bus, bridge_db).await;
        });

        // Periodic QueueStatus emission (Issue #40)
        let qs_bus = event_bus.clone();
        let qs_db = db.clone();
        tokio::spawn(async move {
            emit_periodic_queue_status(qs_bus, qs_db).await;
        });

        Some(handle)
    } else {
        info!("Job worker disabled");
        None
    };

    // Spawn webhook dispatcher (Issue #44)
    let wh_bus = event_bus.clone();
    let wh_db = db.clone();
    tokio::spawn(async move {
        webhook_dispatcher(wh_bus, wh_db).await;
    });

    // Spawn telemetry mirror (Issue #45)
    let tm_bus = event_bus.clone();
    tokio::spawn(async move {
        telemetry_mirror(tm_bus).await;
    });

    // Get issuer URL from environment
    let issuer =
        std::env::var("ISSUER_URL").unwrap_or_else(|_| format!("http://{}:{}", host, port));

    // OAuth token lifetimes (configurable via env vars, defaults to 1h / 4h)
    let oauth_token_lifetime_secs: u64 = std::env::var("OAUTH_TOKEN_LIFETIME_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(matric_core::defaults::OAUTH_TOKEN_LIFETIME_SECS);
    let oauth_mcp_token_lifetime_secs: u64 = std::env::var("OAUTH_MCP_TOKEN_LIFETIME_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(matric_core::defaults::OAUTH_MCP_TOKEN_LIFETIME_SECS);
    let oauth_token_lifetime = chrono::Duration::seconds(oauth_token_lifetime_secs as i64);
    let oauth_mcp_token_lifetime = chrono::Duration::seconds(oauth_mcp_token_lifetime_secs as i64);
    if oauth_token_lifetime_secs != matric_core::defaults::OAUTH_TOKEN_LIFETIME_SECS
        || oauth_mcp_token_lifetime_secs != matric_core::defaults::OAUTH_MCP_TOKEN_LIFETIME_SECS
    {
        info!(
            "OAuth token lifetimes: standard={}s, mcp={}s",
            oauth_token_lifetime_secs, oauth_mcp_token_lifetime_secs
        );
    }

    // Create rate limiter if enabled
    let rate_limiter = if rate_limit_enabled {
        let quota = Quota::with_period(std::time::Duration::from_secs(rate_limit_period_secs))
            .expect("Rate limit period must be non-zero")
            .allow_burst(
                NonZeroU32::new(rate_limit_requests as u32).expect("Rate limit must be non-zero"),
            );
        Some(Arc::new(RateLimiter::direct(quota)))
    } else {
        None
    };

    // Create app state
    let tag_resolver = TagResolver::new(db.clone());
    let search_cache = matric_api::services::SearchCache::from_env().await;
    let state = AppState {
        db,
        search,
        issuer,
        rate_limiter,
        tag_resolver,
        search_cache,
        event_bus,
        ws_connections: Arc::new(AtomicUsize::new(0)),
        default_archive_cache: Arc::new(RwLock::new(DefaultArchiveCache::new(
            std::env::var("DEFAULT_ARCHIVE_CACHE_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        ))),
        require_auth: std::env::var("REQUIRE_AUTH")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false),
        oauth_token_lifetime,
        oauth_mcp_token_lifetime,
        max_memories: std::env::var("MAX_MEMORIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(matric_core::defaults::MAX_MEMORIES),
        max_upload_size: std::env::var("MATRIC_MAX_UPLOAD_SIZE_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(matric_core::defaults::MAX_UPLOAD_SIZE_BYTES),
        vision_backend,
        transcription_backend,
        git_sha: std::env::var("MATRIC_GIT_SHA").unwrap_or_else(|_| "unknown".to_string()),
        build_date: std::env::var("MATRIC_BUILD_DATE").unwrap_or_else(|_| "unknown".to_string()),
        extraction_strategies: active_extraction_strategies,
        schema_engines: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        database_url: database_url.clone(),
    };

    // Read max body size from env var with fallback
    let max_body_size = std::env::var("MATRIC_MAX_BODY_SIZE_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(matric_core::defaults::MAX_BODY_SIZE_BYTES);

    let max_upload_size = state.max_upload_size;

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        .route("/health/live", get(health_check_live))
        // OpenAPI / Swagger UI
        .merge(
            SwaggerUi::new("/docs").config(
                Config::new(["/openapi.yaml"])
                    .try_it_out_enabled(true)
                    .filter(true)
                    .display_request_duration(true),
            ),
        )
        .route("/openapi.yaml", get(openapi_yaml))
        // Notes CRUD
        .route("/api/v1/notes", get(list_notes).post(create_note))
        .route("/api/v1/notes/bulk", post(bulk_create_notes))
        .route(
            "/api/v1/notes/:id",
            get(get_note).patch(update_note).delete(delete_note),
        )
        .route("/api/v1/notes/:id/restore", post(restore_note))
        .route("/api/v1/notes/:id/purge", post(purge_note))
        .route("/api/v1/notes/:id/reprocess", post(reprocess_note))
        .route("/api/v1/notes/reprocess", post(bulk_reprocess_notes))
        .route(
            "/api/v1/notes/:id/tags",
            get(get_note_tags).put(set_note_tags),
        )
        .route("/api/v1/notes/:id/links", get(get_note_links))
        .route("/api/v1/notes/:id/backlinks", get(get_note_backlinks))
        .route("/api/v1/notes/:id/export", get(export_note))
        .route("/api/v1/notes/:id/full", get(get_full_document))
        // Provenance (W3C PROV)
        .route("/api/v1/notes/:id/provenance", get(get_note_provenance))
        // Note versioning (#104)
        .route("/api/v1/notes/:id/versions", get(list_note_versions))
        .route(
            "/api/v1/notes/:id/versions/:version",
            get(get_note_version).delete(delete_note_version),
        )
        .route(
            "/api/v1/notes/:id/versions/:version/restore",
            post(restore_note_version),
        )
        .route("/api/v1/notes/:id/versions/diff", get(diff_note_versions))
        // Search
        .route("/api/v1/search", get(search_notes))
        .route("/api/v1/search/federated", post(federated_search))
        // Memory search (spatial/temporal provenance)
        .route("/api/v1/memories/search", get(search_memories))
        .route(
            "/api/v1/notes/:id/memory-provenance",
            get(get_memory_provenance_handler),
        )
        // Provenance creation (Issue #261)
        .route("/api/v1/provenance/locations", post(create_prov_location))
        .route(
            "/api/v1/provenance/named-locations",
            post(create_named_location),
        )
        .route("/api/v1/provenance/devices", post(create_prov_device))
        .route("/api/v1/provenance/files", post(create_file_provenance))
        .route("/api/v1/provenance/notes", post(create_note_provenance))
        // Temporal queries
        .route("/api/v1/notes/timeline", get(get_notes_timeline))
        .route("/api/v1/notes/activity", get(get_notes_activity))
        // Knowledge health dashboard
        .route("/api/v1/health/knowledge", get(get_knowledge_health))
        .route("/api/v1/health/orphan-tags", get(get_orphan_tags))
        .route("/api/v1/health/stale-notes", get(get_stale_notes))
        .route("/api/v1/health/unlinked-notes", get(get_unlinked_notes))
        .route("/api/v1/health/tag-cooccurrence", get(get_tag_cooccurrence))
        // Note status shortcut
        .route("/api/v1/notes/:id/status", patch(update_note_status))
        // Jobs
        .route("/api/v1/jobs", get(list_jobs).post(create_job))
        .route("/api/v1/jobs/:id", get(get_job))
        .route("/api/v1/jobs/pending", get(pending_jobs_count))
        .route("/api/v1/jobs/stats", get(queue_stats))
        // Extraction Analytics
        .route("/api/v1/extraction/stats", get(extraction_stats))
        // Vision (ad-hoc image description)
        .route(
            "/api/v1/vision/describe",
            post(describe_image).layer(DefaultBodyLimit::max(max_upload_size)),
        )
        // Audio (ad-hoc audio transcription)
        .route(
            "/api/v1/audio/transcribe",
            post(transcribe_audio).layer(DefaultBodyLimit::max(max_upload_size)),
        )
        // Document Types
        .route(
            "/api/v1/document-types",
            get(list_document_types).post(create_document_type),
        )
        .route(
            "/api/v1/document-types/:name",
            get(get_document_type)
                .patch(update_document_type)
                .delete(delete_document_type),
        )
        .route("/api/v1/document-types/detect", post(detect_document_type))
        // Archives
        .route("/api/v1/archives", get(list_archives).post(create_archive))
        .route(
            "/api/v1/archives/:name",
            get(get_archive)
                .patch(update_archive)
                .delete(delete_archive),
        )
        .route(
            "/api/v1/archives/:name/set-default",
            post(set_default_archive),
        )
        .route("/api/v1/archives/:name/stats", get(get_archive_stats))
        .route("/api/v1/archives/:name/clone", post(clone_archive))
        // Memories (aliases for archives - user-facing terminology, Issue #179)
        .route("/api/v1/memories", get(list_archives).post(create_archive))
        .route("/api/v1/memories/overview", get(memories_overview))
        .route(
            "/api/v1/memories/:name",
            get(get_archive)
                .patch(update_archive)
                .delete(delete_archive),
        )
        .route(
            "/api/v1/memories/:name/set-default",
            post(set_default_archive),
        )
        .route("/api/v1/memories/:name/stats", get(get_archive_stats))
        .route("/api/v1/memories/:name/clone", post(clone_archive))
        // PKE (Public Key Encryption)
        .route("/api/v1/pke/keygen", post(pke_keygen))
        .route("/api/v1/pke/address", post(pke_address))
        .route("/api/v1/pke/encrypt", post(pke_encrypt))
        .route("/api/v1/pke/decrypt", post(pke_decrypt))
        .route("/api/v1/pke/recipients", post(pke_recipients))
        .route("/api/v1/pke/verify/:address", get(pke_verify))
        // PKE Keysets (REST API keyset management - Issues #328, #332)
        .route("/api/v1/pke/keysets", get(list_keysets).post(create_keyset))
        .route("/api/v1/pke/keysets/active", get(get_active_keyset))
        .route("/api/v1/pke/keysets/import", post(import_keyset))
        .route("/api/v1/pke/keysets/:name_or_id", delete(delete_keyset))
        .route(
            "/api/v1/pke/keysets/:name_or_id/active",
            put(set_active_keyset),
        )
        .route("/api/v1/pke/keysets/:name_or_id/export", get(export_keyset))
        // Tags (legacy)
        .route("/api/v1/tags", get(list_tags))
        // SKOS Concept Schemes
        .route(
            "/api/v1/concepts/schemes",
            get(list_concept_schemes).post(create_concept_scheme),
        )
        .route(
            "/api/v1/concepts/schemes/:id",
            get(get_concept_scheme)
                .patch(update_concept_scheme)
                .delete(delete_concept_scheme),
        )
        .route(
            "/api/v1/concepts/schemes/:id/top-concepts",
            get(get_top_concepts),
        )
        // SKOS Concepts
        .route(
            "/api/v1/concepts",
            get(search_concepts).post(create_concept),
        )
        .route("/api/v1/concepts/autocomplete", get(autocomplete_concepts))
        .route(
            "/api/v1/concepts/:id",
            get(get_concept)
                .patch(update_concept)
                .delete(delete_concept),
        )
        .route("/api/v1/concepts/:id/full", get(get_concept_full))
        .route("/api/v1/concepts/:id/ancestors", get(get_ancestors))
        .route("/api/v1/concepts/:id/descendants", get(get_descendants))
        .route(
            "/api/v1/concepts/:id/broader",
            get(get_broader).post(add_broader),
        )
        .route(
            "/api/v1/concepts/:id/broader/:target_id",
            delete(remove_broader),
        )
        .route(
            "/api/v1/concepts/:id/narrower",
            get(get_narrower).post(add_narrower),
        )
        .route(
            "/api/v1/concepts/:id/narrower/:target_id",
            delete(remove_narrower),
        )
        .route(
            "/api/v1/concepts/:id/related",
            get(get_related).post(add_related),
        )
        .route(
            "/api/v1/concepts/:id/related/:target_id",
            delete(remove_related),
        )
        // SKOS Tagging
        .route(
            "/api/v1/notes/:id/concepts",
            get(get_note_concepts).post(tag_note_with_concept),
        )
        .route(
            "/api/v1/notes/:id/concepts/:concept_id",
            delete(untag_note_concept),
        )
        // File Attachments
        .route(
            "/api/v1/notes/:id/attachments",
            get(list_attachments).post(upload_attachment),
        )
        .route(
            "/api/v1/notes/:id/attachments/upload",
            post(upload_attachment_multipart).layer(DefaultBodyLimit::max(max_upload_size)),
        )
        .route(
            "/api/v1/attachments/:attachment_id",
            get(get_attachment).delete(delete_attachment),
        )
        .route(
            "/api/v1/attachments/:attachment_id/download",
            get(download_attachment),
        )
        // SKOS Governance
        .route("/api/v1/concepts/governance", get(get_governance_stats))
        // SKOS Export
        .route(
            "/api/v1/concepts/schemes/:id/export/turtle",
            get(export_scheme_turtle),
        )
        .route(
            "/api/v1/concepts/schemes/export/turtle",
            get(export_all_schemes_turtle),
        )
        // SKOS Collections (W3C SKOS Section 9)
        .route(
            "/api/v1/concepts/collections",
            get(list_skos_collections).post(create_skos_collection),
        )
        .route(
            "/api/v1/concepts/collections/:id",
            get(get_skos_collection)
                .patch(update_skos_collection)
                .delete(delete_skos_collection),
        )
        .route(
            "/api/v1/concepts/collections/:id/members",
            put(replace_skos_collection_members),
        )
        .route(
            "/api/v1/concepts/collections/:id/members/:concept_id",
            post(add_skos_collection_member).delete(remove_skos_collection_member),
        )
        // Collections
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/api/v1/collections/:id",
            get(get_collection)
                .patch(update_collection)
                .delete(delete_collection),
        )
        .route("/api/v1/collections/:id/notes", get(get_collection_notes))
        .route("/api/v1/collections/:id/export", get(export_collection))
        .route("/api/v1/notes/:id/move", post(move_note_to_collection))
        // Embedding sets
        .route(
            "/api/v1/embedding-sets",
            get(list_embedding_sets).post(create_embedding_set),
        )
        .route(
            "/api/v1/embedding-sets/:slug",
            get(get_embedding_set)
                .patch(update_embedding_set)
                .delete(delete_embedding_set),
        )
        .route(
            "/api/v1/embedding-sets/:slug/members",
            get(list_embedding_set_members).post(add_embedding_set_members),
        )
        .route(
            "/api/v1/embedding-sets/:slug/members/:note_id",
            delete(remove_embedding_set_member),
        )
        .route(
            "/api/v1/embedding-sets/:slug/refresh",
            post(refresh_embedding_set),
        )
        .route(
            "/api/v1/embedding-configs",
            get(list_embedding_configs).post(create_embedding_config),
        )
        .route(
            "/api/v1/embedding-configs/default",
            get(get_default_embedding_config),
        )
        .route(
            "/api/v1/embedding-configs/:id",
            get(get_embedding_config)
                .patch(update_embedding_config)
                .delete(delete_embedding_config),
        )
        // Graph exploration
        .route("/api/v1/graph/topology/stats", get(graph_topology_stats))
        .route("/api/v1/graph/:id", get(explore_graph))
        // Templates
        .route(
            "/api/v1/templates",
            get(list_templates).post(create_template),
        )
        .route(
            "/api/v1/templates/:id",
            get(get_template)
                .patch(update_template)
                .delete(delete_template),
        )
        .route(
            "/api/v1/templates/:id/instantiate",
            post(instantiate_template),
        )
        // OAuth2 endpoints
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_discovery),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource),
        )
        .route(
            "/oauth/authorize",
            get(oauth_authorize_get).post(oauth_authorize_post),
        )
        .route("/oauth/register", post(oauth_register))
        .route("/oauth/token", post(oauth_token))
        .route("/oauth/introspect", post(oauth_introspect))
        .route("/oauth/revoke", post(oauth_revoke))
        // API key management
        .route("/api/v1/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api/v1/api-keys/:id", delete(revoke_api_key))
        // Legacy backup endpoints (JSON export/import)
        .route("/api/v1/backup/export", get(backup_export))
        .route("/api/v1/backup/download", get(backup_download))
        .route("/api/v1/backup/import", post(backup_import))
        .route("/api/v1/backup/trigger", post(backup_trigger))
        .route("/api/v1/backup/status", get(backup_status))
        // Knowledge shards (portable, app-level exports)
        .route("/api/v1/backup/knowledge-shard", get(knowledge_shard))
        .route(
            "/api/v1/backup/knowledge-shard/import",
            post(knowledge_shard_import),
        )
        // Database backups (full pg_dump, includes embeddings)
        .route("/api/v1/backup/database", get(database_backup_download))
        .route(
            "/api/v1/backup/database/snapshot",
            post(database_backup_snapshot),
        )
        .route(
            "/api/v1/backup/database/upload",
            post(database_backup_upload),
        )
        .route(
            "/api/v1/backup/database/restore",
            post(database_backup_restore),
        )
        // Memory-scoped backup (single archive schema)
        .route("/api/v1/backup/memory/:name", get(memory_backup_download))
        // Knowledge archives (backup + metadata bundled as .archive)
        .route(
            "/api/v1/backup/knowledge-archive/:filename",
            get(knowledge_archive_download),
        )
        .route(
            "/api/v1/backup/knowledge-archive",
            post(knowledge_archive_upload),
        )
        // Backup browser (lists all backups)
        .route("/api/v1/backup/list", get(list_backups))
        .route("/api/v1/backup/list/:filename", get(get_backup_info))
        .route("/api/v1/backup/swap", post(swap_backup))
        // Backup metadata
        .route(
            "/api/v1/backup/metadata/:filename",
            get(get_backup_metadata),
        )
        .route(
            "/api/v1/backup/metadata/:filename",
            put(update_backup_metadata),
        )
        // Memory info
        .route("/api/v1/memory/info", get(memory_info))
        // WebSocket events (Issue #39)
        .route("/api/v1/ws", get(ws_handler))
        // SSE events (Issue #43)
        .route("/api/v1/events", get(sse_events))
        // Webhooks (Issue #44)
        .route("/api/v1/webhooks", post(create_webhook).get(list_webhooks))
        .route(
            "/api/v1/webhooks/:id",
            get(get_webhook)
                .patch(update_webhook)
                .delete(delete_webhook_handler),
        )
        .route(
            "/api/v1/webhooks/:id/deliveries",
            get(list_webhook_deliveries),
        )
        .route("/api/v1/webhooks/:id/test", post(test_webhook))
        // Rate limiting status endpoint
        .route("/api/v1/rate-limit/status", get(rate_limit_status))
        // Middleware
        .layer(axum::middleware::from_fn(cache_control_middleware))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuidV7))
        .layer({
            // Issue #462: Secure CORS configuration with origin whitelist
            let allowed_origins = parse_allowed_origins();

            CorsLayer::new()
                .allow_origin(AllowOrigin::list(allowed_origins))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
                .allow_credentials(true)
                .max_age(std::time::Duration::from_secs(
                    matric_core::defaults::CORS_MAX_AGE_SECS,
                ))
        })
        // Allow up to 2GB uploads for database backups and knowledge shards
        .layer(RequestBodyLimitLayer::new(max_body_size)) // 2 GB
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// =============================================================================
// EVENTING: WebSocket, SSE, and Event Bridge (Issues #38-#45)
// =============================================================================

/// Bridge WorkerEvent from the job worker to ServerEvent on the unified EventBus.
async fn bridge_worker_events(
    mut worker_rx: tokio::sync::broadcast::Receiver<WorkerEvent>,
    event_bus: Arc<EventBus>,
    db: Database,
) {
    use matric_core::{JobRepository, NoteRepository};
    loop {
        match worker_rx.recv().await {
            Ok(event) => {
                let server_event = match event {
                    WorkerEvent::JobStarted { job_id, job_type } => {
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobStarted {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                        }
                    }
                    WorkerEvent::JobProgress {
                        job_id,
                        percent,
                        message,
                    } => {
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobProgress {
                            job_id,
                            note_id,
                            progress: percent,
                            message,
                        }
                    }
                    WorkerEvent::JobCompleted { job_id, job_type } => {
                        let job = db.jobs.get(job_id).await.ok().flatten();
                        let note_id = job.as_ref().and_then(|j| j.note_id);
                        let duration_ms = job.as_ref().and_then(|j| {
                            j.completed_at
                                .and_then(|c| j.started_at.map(|s| (c - s).num_milliseconds()))
                        });

                        let evt = ServerEvent::JobCompleted {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                            duration_ms,
                        };
                        event_bus.emit(evt.clone());

                        // Also emit NoteUpdated if this was a note-related job (Issue #41)
                        if let Some(nid) = note_id {
                            if let Ok(note) = db.notes.fetch(nid).await {
                                event_bus.emit(ServerEvent::NoteUpdated {
                                    note_id: nid,
                                    title: note.note.title.clone(),
                                    tags: note.tags.clone(),
                                    has_ai_content: note.revised.ai_generated_at.is_some(),
                                    has_links: !note.links.is_empty(),
                                });
                            }
                        }
                        continue; // Already emitted above
                    }
                    WorkerEvent::JobFailed {
                        job_id,
                        job_type,
                        error,
                    } => {
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobFailed {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                            error,
                        }
                    }
                    WorkerEvent::WorkerStarted | WorkerEvent::WorkerStopped => continue,
                };
                event_bus.emit(server_event);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "Event bridge lagged, missed events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Worker event channel closed, bridge stopping");
                break;
            }
        }
    }
}

/// Periodically emit QueueStatus events.
async fn emit_periodic_queue_status(event_bus: Arc<EventBus>, db: Database) {
    use matric_core::JobRepository;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        interval.tick().await;
        // Only emit if there are subscribers
        if event_bus.subscriber_count() == 0 {
            continue;
        }
        if let Ok(stats) = db.jobs.queue_stats().await {
            event_bus.emit(ServerEvent::QueueStatus {
                total_jobs: stats.total,
                running: stats.processing,
                pending: stats.pending,
            });
        }
    }
}

/// WebSocket handler for real-time event streaming (Issue #39).
///
/// Clients connect to `/api/v1/ws` and receive JSON-encoded ServerEvents.
/// Sending "refresh" triggers an immediate QueueStatus response.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_connection(socket, state))
}

async fn handle_ws_connection(socket: WebSocket, state: AppState) {
    use futures::{SinkExt, StreamExt};

    let count = state.ws_connections.fetch_add(1, Ordering::Relaxed) + 1;
    tracing::info!(active = count, "WebSocket connection opened");

    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = state.event_bus.subscribe();

    // Spawn task to forward events to client
    let send_task = tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Ok(evt) => {
                            if let Ok(json) = serde_json::to_string(&evt) {
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(missed = n, "WebSocket client lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = ping_interval.tick() => {
                    if sender.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Handle incoming messages from client
    let event_bus = state.event_bus.clone();
    let db = state.db.clone();
    let recv_task = tokio::spawn(async move {
        use matric_core::JobRepository;
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(ref text) if text == "refresh" => {
                    // Send immediate queue status
                    if let Ok(stats) = db.jobs.queue_stats().await {
                        event_bus.emit(ServerEvent::QueueStatus {
                            total_jobs: stats.total,
                            running: stats.processing,
                            pending: stats.pending,
                        });
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
    let count = state.ws_connections.fetch_sub(1, Ordering::Relaxed) - 1;
    tracing::info!(active = count, "WebSocket connection closed");
}

/// SSE event stream handler (Issue #43).
///
/// Clients connect to `/api/v1/events` and receive Server-Sent Events.
async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.event_bus.subscribe();

    use tokio_stream::StreamExt as _;
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(
        |result: Result<ServerEvent, _>| {
            match result {
                Ok(event) => {
                    let event_type = event.event_type().to_string();
                    match serde_json::to_string(&event) {
                        Ok(json) => Some(Ok(Event::default().event(event_type).data(json))),
                        Err(_) => None,
                    }
                }
                Err(_) => None, // Skip lagged/closed errors
            }
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    )
}

// =============================================================================
// WEBHOOK HANDLERS (Issue #44)
// =============================================================================

/// Webhook dispatcher: subscribes to EventBus and delivers matching events to webhooks.
async fn webhook_dispatcher(event_bus: Arc<EventBus>, db: Database) {
    let webhook_timeout = std::env::var("MATRIC_WEBHOOK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(matric_core::defaults::WEBHOOK_TIMEOUT_SECS);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(webhook_timeout))
        .build()
        .unwrap_or_default();

    let mut rx = event_bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                let event_type = event.event_type();
                let webhooks = match db.webhooks.list_active_for_event(event_type).await {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to list webhooks");
                        continue;
                    }
                };

                if webhooks.is_empty() {
                    continue;
                }

                let payload = match serde_json::to_value(&event) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                for webhook in webhooks {
                    let client = client.clone();
                    let db = db.clone();
                    let payload = payload.clone();
                    let event_type = event_type.to_string();
                    tokio::spawn(async move {
                        deliver_webhook(&client, &db, &webhook, &event_type, &payload).await;
                    });
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "Webhook dispatcher lagged");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Deliver an event to a single webhook with HMAC signing and delivery recording.
async fn deliver_webhook(
    client: &reqwest::Client,
    db: &Database,
    webhook: &matric_core::Webhook,
    event_type: &str,
    payload: &serde_json::Value,
) {
    let body = serde_json::to_string(payload).unwrap_or_default();

    let mut request = client
        .post(&webhook.url)
        .header("Content-Type", "application/json")
        .header("X-Fortemi-Event", event_type);

    // HMAC-SHA256 signature if secret is configured
    if let Some(secret) = &webhook.secret {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        if let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) {
            mac.update(body.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());
            request = request.header("X-Fortemi-Signature", format!("sha256={}", signature));
        }
    }

    let result = request.body(body).send().await;

    match result {
        Ok(response) => {
            let status = response.status().as_u16() as i32;
            let success = response.status().is_success();
            let body = response.text().await.unwrap_or_default();
            let _ = db
                .webhooks
                .record_delivery(
                    webhook.id,
                    event_type,
                    payload,
                    Some(status),
                    Some(&body),
                    success,
                )
                .await;
        }
        Err(e) => {
            let _ = db
                .webhooks
                .record_delivery(
                    webhook.id,
                    event_type,
                    payload,
                    None,
                    Some(&e.to_string()),
                    false,
                )
                .await;
        }
    }
}

/// Telemetry mirror: structured tracing for all event bus events (Issue #45).
async fn telemetry_mirror(event_bus: Arc<EventBus>) {
    let mut rx = event_bus.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => match &event {
                ServerEvent::JobStarted {
                    job_id, job_type, ..
                } => {
                    tracing::info!(
                        target: "fortemi::events",
                        event = "job.started",
                        %job_id, %job_type,
                        "Job started"
                    );
                }
                ServerEvent::JobCompleted {
                    job_id,
                    job_type,
                    duration_ms,
                    ..
                } => {
                    tracing::info!(
                        target: "fortemi::events",
                        event = "job.completed",
                        %job_id, %job_type, ?duration_ms,
                        "Job completed"
                    );
                }
                ServerEvent::JobFailed {
                    job_id,
                    job_type,
                    error,
                    ..
                } => {
                    tracing::warn!(
                        target: "fortemi::events",
                        event = "job.failed",
                        %job_id, %job_type, %error,
                        "Job failed"
                    );
                }
                ServerEvent::NoteUpdated { note_id, .. } => {
                    tracing::info!(
                        target: "fortemi::events",
                        event = "note.updated",
                        %note_id,
                        "Note updated"
                    );
                }
                ServerEvent::QueueStatus {
                    running, pending, ..
                } => {
                    tracing::debug!(
                        target: "fortemi::events",
                        event = "queue.status",
                        running, pending,
                        "Queue status"
                    );
                }
                _ => {}
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "Telemetry mirror lagged");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct UpdateWebhookBody {
    url: Option<String>,
    events: Option<Vec<String>>,
    is_active: Option<bool>,
    secret: Option<String>,
}
#[utoipa::path(post, path = "/api/v1/webhooks", tag = "Webhooks",
    request_body = matric_core::CreateWebhookRequest,
    responses((status = 201, description = "Created")))]

async fn create_webhook(
    State(state): State<AppState>,
    Json(body): Json<matric_core::CreateWebhookRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let id = state.db.webhooks.create(body).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[utoipa::path(get, path = "/api/v1/webhooks", tag = "Webhooks",
    responses((status = 200, description = "Success")))]
async fn list_webhooks(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let webhooks = state.db.webhooks.list().await?;
    Ok(Json(webhooks))
}

#[utoipa::path(get, path = "/api/v1/webhooks/{id}", tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses((status = 200, description = "Success")))]
async fn get_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let webhook = state
        .db
        .webhooks
        .get(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Webhook {} not found", id)))?;
    Ok(Json(webhook))
}

#[utoipa::path(patch, path = "/api/v1/webhooks/{id}", tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    request_body = UpdateWebhookBody,
    responses((status = 200, description = "Success")))]
async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateWebhookBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify exists
    state
        .db
        .webhooks
        .get(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Webhook {} not found", id)))?;

    state
        .db
        .webhooks
        .update(
            id,
            body.url.as_deref(),
            body.events.as_deref(),
            body.secret.as_deref(),
            body.is_active,
        )
        .await?;

    let webhook = state.db.webhooks.get(id).await?.unwrap();
    Ok(Json(webhook))
}

#[utoipa::path(delete, path = "/api/v1/webhooks/{id}", tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses((status = 204, description = "No Content")))]
async fn delete_webhook_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.webhooks.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/webhooks/{id}/deliveries", tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses((status = 200, description = "Success")))]
async fn list_webhook_deliveries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<i64>().ok())
        .unwrap_or(matric_core::defaults::PAGE_LIMIT);
    let deliveries = state.db.webhooks.list_deliveries(id, limit).await?;
    Ok(Json(deliveries))
}

#[utoipa::path(post, path = "/api/v1/webhooks/{id}/test", tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses((status = 200, description = "Success")))]
async fn test_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let webhook = state
        .db
        .webhooks
        .get(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Webhook {} not found", id)))?;

    let webhook_timeout = std::env::var("MATRIC_WEBHOOK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(matric_core::defaults::WEBHOOK_TIMEOUT_SECS);

    let test_event = ServerEvent::QueueStatus {
        total_jobs: 0,
        running: 0,
        pending: 0,
    };
    let payload = serde_json::to_value(&test_event).unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(webhook_timeout))
        .build()
        .unwrap_or_default();

    deliver_webhook(&client, &state.db, &webhook, "test", &payload).await;

    Ok(Json(serde_json::json!({ "status": "delivered" })))
}

// =============================================================================
// RATE LIMITING MIDDLEWARE
// =============================================================================

async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // If rate limiting is disabled, pass through
    if let Some(limiter) = &state.rate_limiter {
        // Check rate limit
        if limiter.check().is_err() {
            tracing::warn!("Rate limit exceeded");
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "rate_limit_exceeded",
                    "error_description": "Too many requests. Please wait before retrying."
                })),
            ));
        }
    }
    Ok(next.run(request).await)
}

// =============================================================================
// CACHE CONTROL MIDDLEWARE (fixes #211)
// =============================================================================

/// Sets HTTP Cache-Control and ETag headers based on request method and path.
///
/// Cache-Control policy:
/// - Mutation requests (POST/PUT/PATCH/DELETE): `no-store`
/// - Stable reference data (document-types, concepts/schemes): `public, max-age=300`
/// - Single-resource GETs with ID: `private, no-cache` (allows conditional caching)
/// - List/search endpoints: `private, no-cache`
/// - Health endpoints: `no-cache, max-age=0`
///
/// ETag support:
/// - GET responses on /api/v1/ get a weak ETag derived from response body hash
/// - If-None-Match requests return 304 Not Modified when ETag matches
async fn cache_control_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::body::Body;

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut response = next.run(request).await;

    // Skip if handler already set Cache-Control
    if response.headers().contains_key(header::CACHE_CONTROL) {
        return response;
    }

    let is_get = method == Method::GET || method == Method::HEAD;

    let cache_value = if !is_get {
        // Mutations must never be cached
        "no-store"
    } else if path.starts_with("/api/v1/document-types")
        || path.starts_with("/api/v1/concepts/schemes")
    {
        // Stable reference data — cache for 5 minutes
        "public, max-age=300"
    } else if path.starts_with("/health") || path.starts_with("/api/v1/health") {
        // Health checks — always fresh
        "no-cache, max-age=0"
    } else if path.starts_with("/api/v1/") {
        // All other API GETs — allow private caching with revalidation
        "private, no-cache"
    } else {
        // Static assets (docs, openapi.yaml)
        "public, max-age=3600"
    };

    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, cache_value.parse().unwrap());

    // Responses vary by archive selection header
    if path.starts_with("/api/v1/") {
        response
            .headers_mut()
            .append(header::VARY, "X-Fortemi-Memory".parse().unwrap());
    }

    // ETag support for successful GET responses on API paths
    if is_get && path.starts_with("/api/v1/") && response.status().is_success() {
        // Collect body bytes to compute hash
        let (parts, body) = response.into_parts();
        if let Ok(bytes) = axum::body::to_bytes(body, 2 * 1024 * 1024).await {
            // Compute weak ETag from FNV-1a hash of body
            let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| {
                (h ^ b as u64).wrapping_mul(0x100000001b3)
            });
            let etag = format!("W/\"{:x}\"", hash);

            // Check If-None-Match
            if let Some(client_etag) = if_none_match {
                if client_etag.contains(&etag) || client_etag.trim() == etag {
                    let mut not_modified = axum::response::Response::new(Body::empty());
                    *not_modified.status_mut() = StatusCode::NOT_MODIFIED;
                    not_modified
                        .headers_mut()
                        .insert(header::ETAG, etag.parse().unwrap());
                    // Preserve cache headers
                    for (k, v) in parts.headers.iter() {
                        if k == header::CACHE_CONTROL || k == header::VARY {
                            not_modified.headers_mut().insert(k.clone(), v.clone());
                        }
                    }
                    return not_modified;
                }
            }

            let mut response = axum::response::Response::from_parts(parts, Body::from(bytes));
            response
                .headers_mut()
                .insert(header::ETAG, etag.parse().unwrap());
            return response;
        }
        // If body collection fails (too large), return without ETag
        axum::response::Response::from_parts(parts, Body::empty())
    } else {
        response
    }
}

// =============================================================================
// AUTHENTICATION MIDDLEWARE
// =============================================================================

/// Authentication middleware.
///
/// Always validates Bearer tokens when present, regardless of `REQUIRE_AUTH`.
/// This ensures scope enforcement works for authenticated requests even when
/// anonymous access is allowed.
///
/// Behavior:
/// - Public routes: bypass all auth (health, oauth, docs, SSE/WS)
/// - Bearer token present + valid: inject Auth with proper scope, enforce scopes
/// - Bearer token present + invalid: always reject with 401
/// - No token + REQUIRE_AUTH=true: reject with 401
/// - No token + REQUIRE_AUTH=false: allow as Anonymous
///
/// Scope enforcement (centralized):
/// - Mutation methods (POST/PUT/PATCH/DELETE) on non-read-only routes require "write" scope
/// - Admin routes (backup/*, api-keys/*) require "admin" scope
async fn auth_middleware(
    State(state): State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();

    // Public routes bypass all auth
    if is_public_route(&path) {
        return next.run(request).await;
    }

    // Always try to parse Bearer token if present
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let has_token = auth_header.is_some();

    let principal = match &auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = header.trim_start_matches("Bearer ").trim();

            if token.starts_with("mm_at_") {
                // OAuth access token
                match state.db.oauth.validate_access_token(token).await {
                    Ok(Some(oauth_token)) => {
                        // Sliding window refresh
                        let lifetime = state.token_lifetime_for_scope(&oauth_token.scope);
                        let _ = state
                            .db
                            .oauth
                            .validate_and_extend_token(token, lifetime)
                            .await;
                        Some(AuthPrincipal::OAuthClient {
                            client_id: oauth_token.client_id,
                            scope: oauth_token.scope,
                            user_id: oauth_token.user_id,
                        })
                    }
                    _ => None,
                }
            } else if token.starts_with("mm_key_") {
                // API key
                match state.db.oauth.validate_api_key(token).await {
                    Ok(Some(api_key)) => Some(AuthPrincipal::ApiKey {
                        key_id: api_key.id,
                        scope: api_key.scope,
                    }),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    };

    // Handle authentication result
    match principal {
        Some(p) => {
            // Valid token — enforce scope based on method and route
            if let Some(err_response) = check_scope_enforcement(&p, &method, &path) {
                return err_response;
            }

            // Inject auth info into request extensions
            let mut request = request;
            request.extensions_mut().insert(Auth { principal: p });
            next.run(request).await
        }
        None if has_token => {
            // Token was present but invalid — always reject
            let body = serde_json::json!({
                "error": "unauthorized",
                "error_description": "Invalid or expired bearer token."
            });
            (StatusCode::UNAUTHORIZED, Json(body)).into_response()
        }
        None => {
            // No token provided
            if state.require_auth {
                let body = serde_json::json!({
                    "error": "unauthorized",
                    "error_description": "Authentication required. Provide a valid Bearer token."
                });
                (StatusCode::UNAUTHORIZED, Json(body)).into_response()
            } else {
                // Anonymous access allowed — inject Anonymous principal
                let mut request = request;
                request.extensions_mut().insert(Auth {
                    principal: AuthPrincipal::Anonymous,
                });
                next.run(request).await
            }
        }
    }
}

/// Check scope enforcement for authenticated requests.
///
/// Returns Some(Response) if the request should be rejected, None if allowed.
fn check_scope_enforcement(
    principal: &AuthPrincipal,
    _method: &axum::http::Method,
    path: &str,
) -> Option<axum::response::Response> {
    // Admin routes require admin scope
    if is_admin_route(path) && !principal.has_scope("admin") {
        let body = serde_json::json!({
            "error": "forbidden",
            "error_description": format!(
                "Insufficient scope. Required: admin, have: {}",
                principal.scope_str()
            )
        });
        return Some((StatusCode::FORBIDDEN, Json(body)).into_response());
    }

    // NOTE: Write scope enforcement is intentionally disabled for now.
    // All authenticated users get read+write access. Scope infrastructure
    // is in place for future multi-tenancy. Only admin routes are gated.

    None
}

/// Check if a route is public (accessible without authentication).
fn is_public_route(path: &str) -> bool {
    // Health endpoints
    if path.starts_with("/health") || path.starts_with("/api/v1/health") {
        return true;
    }
    // OAuth endpoints
    if path.starts_with("/oauth/") || path.starts_with("/.well-known/") {
        return true;
    }
    // API documentation
    if path.starts_with("/swagger-ui") || path.starts_with("/api-docs") {
        return true;
    }
    // SSE and WebSocket endpoints (they have their own auth)
    if path == "/api/v1/events" || path == "/api/v1/ws" {
        return true;
    }
    false
}

/// Routes that require admin scope for all methods.
fn is_admin_route(path: &str) -> bool {
    // NOTE: Backup and metadata routes removed from admin enforcement for
    // pre-tenancy permissive mode. All authenticated users get full access.
    // Scope infrastructure kept in place for future multi-tenancy.
    // See: issues #129, #140, commit 49d9f3f

    // API key management (kept as admin — security-sensitive)
    if path.starts_with("/api/v1/api-keys") && !path.ends_with("/api-keys") {
        // POST /api/v1/api-keys (create) and DELETE /api/v1/api-keys/:id (revoke)
        return true;
    }
    false
}

/// Get rate limiting status.
#[utoipa::path(get, path = "/api/v1/rate-limit/status", tag = "System",
    responses((status = 200, description = "Success")))]
async fn rate_limit_status(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(_limiter) = &state.rate_limiter {
        Json(serde_json::json!({
            "enabled": true,
            "message": "Rate limiting is active"
        }))
    } else {
        Json(serde_json::json!({
            "enabled": false,
            "message": "Rate limiting is disabled"
        }))
    }
}

// =============================================================================
// HEALTH CHECK
// =============================================================================

#[utoipa::path(
    get,
    path = "/health",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "git_sha": state.git_sha,
        "build_date": state.build_date,
        "capabilities": {
            "vision": state.vision_backend.is_some(),
            "audio_transcription": state.transcription_backend.is_some(),
            "auth_required": state.require_auth,
            "extraction_strategies": state.extraction_strategies,
        },
    }))
}

/// Live health check endpoint (readiness probe).
///
/// Performs concurrent live connectivity checks on all dependent services:
/// - PostgreSQL (SELECT 1)
/// - Redis search cache (connection check)
/// - Ollama vision backend (if configured)
/// - Whisper transcription backend (if configured)
///
/// Returns 200 for healthy or degraded (optional services down),
/// 503 when critical services (PostgreSQL) are unreachable.
#[utoipa::path(
    get,
    path = "/health/live",
    tag = "System",
    responses(
        (status = 200, description = "Healthy or degraded"),
        (status = 503, description = "Critical service unavailable"),
    )
)]
async fn health_check_live(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();

    // Run all checks concurrently
    let (db_result, redis_result, vision_result, transcription_result) = tokio::join!(
        // PostgreSQL: SELECT 1 with 5s timeout
        async {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db.pool),
            )
            .await
            {
                Ok(Ok(1)) => ("ok", None),
                Ok(Ok(_)) => ("ok", None),
                Ok(Err(e)) => ("error", Some(format!("{e}"))),
                Err(_) => ("error", Some("timeout (5s)".to_string())),
            }
        },
        // Redis: connection check
        async {
            if state.search_cache.is_connected().await {
                ("ok", None)
            } else {
                ("unavailable", Some("not connected".to_string()))
            }
        },
        // Vision backend (optional)
        async {
            match &state.vision_backend {
                Some(backend) => match backend.health_check().await {
                    Ok(_) => ("ok", None),
                    Err(e) => ("error", Some(format!("{e}"))),
                },
                None => ("not_configured", None),
            }
        },
        // Transcription backend (optional)
        async {
            match &state.transcription_backend {
                Some(backend) => match backend.health_check().await {
                    Ok(_) => ("ok", None),
                    Err(e) => ("error", Some(format!("{e}"))),
                },
                None => ("not_configured", None),
            }
        },
    );

    let elapsed_ms = start.elapsed().as_millis();

    // PostgreSQL is critical — if it's down, the service is unhealthy
    let db_healthy = db_result.0 == "ok";

    // Determine overall status
    let (overall_status, http_status) = if !db_healthy {
        ("unhealthy", StatusCode::SERVICE_UNAVAILABLE)
    } else if redis_result.0 == "error"
        || (vision_result.0 == "error")
        || (transcription_result.0 == "error")
    {
        ("degraded", StatusCode::OK)
    } else {
        ("healthy", StatusCode::OK)
    };

    let mut services = serde_json::json!({
        "postgresql": {
            "status": db_result.0,
        },
        "redis": {
            "status": redis_result.0,
        },
        "vision": {
            "status": vision_result.0,
        },
        "transcription": {
            "status": transcription_result.0,
        },
    });

    // Add error details where present
    if let Some(err) = &db_result.1 {
        services["postgresql"]["error"] = serde_json::Value::String(err.clone());
    }
    if let Some(err) = &redis_result.1 {
        services["redis"]["error"] = serde_json::Value::String(err.clone());
    }
    if let Some(err) = &vision_result.1 {
        services["vision"]["error"] = serde_json::Value::String(err.clone());
    }
    if let Some(err) = &transcription_result.1 {
        services["transcription"]["error"] = serde_json::Value::String(err.clone());
    }

    let body = serde_json::json!({
        "status": overall_status,
        "version": env!("CARGO_PKG_VERSION"),
        "git_sha": state.git_sha,
        "build_date": state.build_date,
        "check_duration_ms": elapsed_ms,
        "services": services,
        "capabilities": {
            "vision": state.vision_backend.is_some(),
            "audio_transcription": state.transcription_backend.is_some(),
            "auth_required": state.require_auth,
            "extraction_strategies": state.extraction_strategies,
        },
    });

    (http_status, Json(body))
}

// =============================================================================
// NOTE HANDLERS
// =============================================================================

/// Parse relative time string (e.g., "7d", "1w", "2h") into a DateTime.
fn parse_relative_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    // Parse the number and unit
    let mut num_str = String::new();
    let mut unit = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
        } else {
            unit.push(c);
        }
    }

    let num: i64 = num_str.parse().ok()?;
    let duration = match unit.as_str() {
        "h" | "hr" | "hrs" | "hour" | "hours" => chrono::Duration::hours(num),
        "d" | "day" | "days" => chrono::Duration::days(num),
        "w" | "wk" | "week" | "weeks" => chrono::Duration::weeks(num),
        "m" | "mo" | "month" | "months" => chrono::Duration::days(num * 30),
        "min" | "mins" | "minute" | "minutes" => chrono::Duration::minutes(num),
        _ => return None,
    };

    Some(chrono::Utc::now() - duration)
}

// =============================================================================
// TEMPORAL QUERY HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct TimelineQuery {
    /// Time period to group by: "hour", "day", "week", "month" (default: "day")
    period: Option<String>,
    /// Alias for period (MCP tools send "granularity")
    granularity: Option<String>,
    /// How many periods to look back (default: 30)
    periods: Option<i64>,
    /// Relative time filter: "7d" (7 days), "1w" (1 week), "1m" (1 month)
    since: Option<String>,
}

#[derive(Debug, Serialize)]
struct TimelineBucket {
    period_start: chrono::DateTime<chrono::Utc>,
    period_end: chrono::DateTime<chrono::Utc>,
    count: i64,
    note_ids: Vec<Uuid>,
}

/// Get notes grouped by time periods for timeline visualization.
#[utoipa::path(
    get,
    path = "/api/v1/notes/timeline",
    tag = "Notes",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_notes_timeline(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<TimelineQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let period = query
        .granularity
        .as_deref()
        .or(query.period.as_deref())
        .unwrap_or("day");
    let periods_back = query
        .periods
        .unwrap_or(matric_core::defaults::TREND_PERIODS);

    // Calculate the cutoff date
    let since = query
        .since
        .as_ref()
        .and_then(|s| parse_relative_time(s))
        .unwrap_or_else(|| {
            let duration = match period {
                "hour" => chrono::Duration::hours(periods_back),
                "week" => chrono::Duration::weeks(periods_back),
                "month" => chrono::Duration::days(periods_back * 30),
                _ => chrono::Duration::days(periods_back), // default: day
            };
            chrono::Utc::now() - duration
        });

    // Query notes created since the cutoff
    let req = ListNotesRequest {
        limit: Some(1000), // reasonable limit
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: Some(since),
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let response = ctx
        .query(move |tx| Box::pin(async move { notes.list_tx(tx, req).await }))
        .await?;

    // Group notes by period
    let mut buckets: std::collections::HashMap<i64, Vec<Uuid>> = std::collections::HashMap::new();

    for note in &response.notes {
        let bucket_key = match period {
            "hour" => {
                // Hour: timestamp / seconds_per_hour
                note.created_at_utc.timestamp() / 3600
            }
            "week" => {
                // Get start of week (Monday)
                let days_since_monday = note.created_at_utc.weekday().num_days_from_monday() as i64;
                (note.created_at_utc - chrono::Duration::days(days_since_monday)).timestamp()
                    / 86400
                    / 7
            }
            "month" => {
                // Year-month as a single number
                note.created_at_utc.year() as i64 * 12 + note.created_at_utc.month() as i64
            }
            _ => {
                // Day: timestamp / seconds_per_day
                note.created_at_utc.timestamp() / 86400
            }
        };

        buckets.entry(bucket_key).or_default().push(note.id);
    }

    // Convert to response format
    let mut timeline: Vec<TimelineBucket> = buckets
        .into_iter()
        .map(|(key, note_ids)| {
            let (period_start, period_end) = match period {
                "hour" => {
                    let start = chrono::DateTime::from_timestamp(key * 3600, 0)
                        .unwrap_or_else(chrono::Utc::now);
                    let end = start + chrono::Duration::hours(1);
                    (start, end)
                }
                "week" => {
                    let start = chrono::DateTime::from_timestamp(key * 7 * 86400, 0)
                        .unwrap_or_else(chrono::Utc::now);
                    let end = start + chrono::Duration::weeks(1);
                    (start, end)
                }
                "month" => {
                    let year = (key / 12) as i32;
                    let month = ((key % 12) + 1) as u32;
                    let start = chrono::DateTime::parse_from_rfc3339(&format!(
                        "{:04}-{:02}-01T00:00:00Z",
                        year, month
                    ))
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                    let end = if month == 12 {
                        chrono::DateTime::parse_from_rfc3339(&format!(
                            "{:04}-01-01T00:00:00Z",
                            year + 1
                        ))
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now())
                    } else {
                        chrono::DateTime::parse_from_rfc3339(&format!(
                            "{:04}-{:02}-01T00:00:00Z",
                            year,
                            month + 1
                        ))
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now())
                    };
                    (start, end)
                }
                _ => {
                    let start = chrono::DateTime::from_timestamp(key * 86400, 0)
                        .unwrap_or_else(chrono::Utc::now);
                    let end = start + chrono::Duration::days(1);
                    (start, end)
                }
            };

            TimelineBucket {
                period_start,
                period_end,
                count: note_ids.len() as i64,
                note_ids,
            }
        })
        .collect();

    // Sort by period_start descending (most recent first)
    timeline.sort_by(|a, b| b.period_start.cmp(&a.period_start));

    Ok(Json(serde_json::json!({
        "period": period,
        "since": since,
        "total_notes": response.notes.len(),
        "buckets": timeline
    })))
}

#[derive(Debug, Deserialize)]
struct ActivityQuery {
    /// Relative time filter: "7d" (7 days), "1w" (1 week), "1m" (1 month)
    since: Option<String>,
    /// Limit number of results (default: 50)
    limit: Option<i64>,
    /// Comma-separated event types to filter by: created, updated
    event_types: Option<String>,
}

#[derive(Debug, Serialize)]
struct ActivityEntry {
    note_id: Uuid,
    title: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    is_recently_created: bool,
    is_recently_updated: bool,
}

/// Get recent note activity (created and modified notes).
#[utoipa::path(
    get,
    path = "/api/v1/notes/activity",
    tag = "Notes",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_notes_activity(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<ActivityQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let since = query
        .since
        .as_ref()
        .and_then(|s| parse_relative_time(s))
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));

    let limit = query.limit.unwrap_or(matric_core::defaults::PAGE_LIMIT);

    // Parse event_types filter (comma-separated: "created", "updated")
    let filter_event_types: Option<Vec<String>> = query.event_types.as_ref().map(|et| {
        et.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    });

    // Determine which queries are needed based on event_types filter
    let want_created = filter_event_types
        .as_ref()
        .is_none_or(|types| types.contains(&"created".to_string()));
    let want_updated = filter_event_types
        .as_ref()
        .is_none_or(|types| types.contains(&"updated".to_string()));

    // Only build and execute queries for requested event types
    let created_req = if want_created {
        Some(ListNotesRequest {
            limit: Some(limit),
            offset: None,
            filter: None,
            sort_by: Some("created_at".to_string()),
            sort_order: Some("desc".to_string()),
            collection_id: None,
            tags: None,
            created_after: Some(since),
            created_before: None,
            updated_after: None,
            updated_before: None,
        })
    } else {
        None
    };

    let updated_req = if want_updated {
        Some(ListNotesRequest {
            limit: Some(limit),
            offset: None,
            filter: None,
            sort_by: Some("updated_at".to_string()),
            sort_order: Some("desc".to_string()),
            collection_id: None,
            tags: None,
            created_after: None,
            created_before: None,
            updated_after: Some(since),
            updated_before: None,
        })
    } else {
        None
    };

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let (created_notes, updated_notes) = ctx
        .query(move |tx| {
            Box::pin(async move {
                let created = if let Some(req) = created_req {
                    notes.list_tx(tx, req).await?
                } else {
                    matric_core::ListNotesResponse {
                        notes: vec![],
                        total: 0,
                    }
                };
                let updated = if let Some(req) = updated_req {
                    notes.list_tx(tx, req).await?
                } else {
                    matric_core::ListNotesResponse {
                        notes: vec![],
                        total: 0,
                    }
                };
                Ok((created, updated))
            })
        })
        .await?;

    // Combine and deduplicate
    let mut activity: std::collections::HashMap<Uuid, ActivityEntry> =
        std::collections::HashMap::new();

    for note in &created_notes.notes {
        activity.insert(
            note.id,
            ActivityEntry {
                note_id: note.id,
                title: Some(note.title.clone()),
                created_at: note.created_at_utc,
                updated_at: note.updated_at_utc,
                is_recently_created: true,
                is_recently_updated: note.updated_at_utc
                    > note.created_at_utc + chrono::Duration::seconds(60),
            },
        );
    }

    for note in &updated_notes.notes {
        activity
            .entry(note.id)
            .and_modify(|e| e.is_recently_updated = true)
            .or_insert(ActivityEntry {
                note_id: note.id,
                title: Some(note.title.clone()),
                created_at: note.created_at_utc,
                updated_at: note.updated_at_utc,
                is_recently_created: note.created_at_utc >= since,
                is_recently_updated: true,
            });
    }

    // Sort by most recent activity
    let mut entries: Vec<ActivityEntry> = activity.into_values().collect();

    // Apply event_types filter if specified
    if let Some(ref types) = filter_event_types {
        let want_created = types.contains(&"created".to_string());
        let want_updated = types.contains(&"updated".to_string());
        entries.retain(|e| {
            (want_created && e.is_recently_created) || (want_updated && e.is_recently_updated)
        });
    }

    entries.sort_by(|a, b| {
        let a_time = a.updated_at.max(a.created_at);
        let b_time = b.updated_at.max(b.created_at);
        b_time.cmp(&a_time)
    });
    entries.truncate(limit as usize);

    Ok(Json(serde_json::json!({
        "since": since,
        "activity": entries,
        "created_count": created_notes.notes.len(),
        "updated_count": updated_notes.notes.len()
    })))
}

// =============================================================================
// KNOWLEDGE HEALTH DASHBOARD
// =============================================================================

#[derive(Debug, Deserialize)]
struct HealthQuery {
    /// Staleness threshold in days (default: 90)
    stale_days: Option<i64>,
    /// Limit for results (default: 100)
    limit: Option<i64>,
}

/// Get overall knowledge health metrics.
#[utoipa::path(
    get,
    path = "/api/v1/health/knowledge",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_knowledge_health(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<HealthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let stale_days = query
        .stale_days
        .unwrap_or(matric_core::defaults::STALE_DAYS);
    let stale_threshold = chrono::Utc::now() - chrono::Duration::days(stale_days);

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes_repo = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let links_repo = matric_db::PgLinkRepository::new(state.db.pool.clone());
    let tags_repo = matric_db::PgTagRepository::new(state.db.pool.clone());

    let mut tx = ctx.begin_tx().await?;

    // Get total note count
    let all_notes = notes_repo
        .list_tx(
            &mut tx,
            ListNotesRequest {
                limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT),
                offset: None,
                filter: None,
                sort_by: None,
                sort_order: None,
                collection_id: None,
                tags: None,
                created_after: None,
                created_before: None,
                updated_after: None,
                updated_before: None,
            },
        )
        .await?;

    let total_notes = all_notes.total;

    // Count stale notes
    let stale_count = all_notes
        .notes
        .iter()
        .filter(|n| n.updated_at_utc < stale_threshold)
        .count();

    // Get notes without any links (unlinked)
    let all_links = links_repo
        .list_all_tx(&mut tx, matric_core::defaults::INTERNAL_FETCH_LIMIT, 0)
        .await
        .unwrap_or_default();
    let linked_note_ids: std::collections::HashSet<Uuid> = all_links
        .iter()
        .flat_map(|link| {
            let mut ids = vec![link.from_note_id];
            if let Some(to_id) = link.to_note_id {
                ids.push(to_id);
            }
            ids
        })
        .collect();
    let unlinked_count = all_notes
        .notes
        .iter()
        .filter(|n| !linked_note_ids.contains(&n.id))
        .count();

    // Get orphan tags (tags with no notes) and notes-without-tags count
    let all_tags = tags_repo.list_tx(&mut tx).await.unwrap_or_default();
    let orphan_tag_count = all_tags.iter().filter(|t| t.note_count == 0).count();

    // Count notes that have at least one tag
    let mut notes_with_tags: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for note in &all_notes.notes {
        let note_tags = tags_repo
            .get_for_note_tx(&mut tx, note.id)
            .await
            .unwrap_or_default();
        if !note_tags.is_empty() {
            notes_with_tags.insert(note.id);
        }
    }
    let notes_without_tags = all_notes.notes.len() - notes_with_tags.len();

    drop(tx); // read-only, no commit needed

    // Calculate health score (0-100)
    let health_score = if total_notes > 0 {
        let stale_ratio = stale_count as f64 / total_notes as f64;
        let unlinked_ratio = unlinked_count as f64 / total_notes as f64;
        let untagged_ratio = notes_without_tags as f64 / total_notes as f64;

        let score = 100.0
            - (stale_ratio * matric_core::defaults::HEALTH_WEIGHT_STALE)
            - (unlinked_ratio * matric_core::defaults::HEALTH_WEIGHT_UNLINKED)
            - (untagged_ratio * matric_core::defaults::HEALTH_WEIGHT_UNTAGGED);
        score.clamp(0.0, 100.0) as i64
    } else {
        100
    };

    // Generate actionable recommendations based on health data
    let mut recommendations: Vec<serde_json::Value> = Vec::new();
    if total_notes > 0 {
        if stale_count > 0 {
            recommendations.push(serde_json::json!({
                "type": "stale_notes",
                "message": format!("{} notes haven't been updated in {} days", stale_count, stale_days),
                "action": "Review stale notes and update or archive them",
                "severity": if stale_count as f64 / total_notes as f64 > 0.5 { "high" } else { "medium" }
            }));
        }
        if unlinked_count > 0 {
            recommendations.push(serde_json::json!({
                "type": "unlinked_notes",
                "message": format!("{} notes have no semantic links", unlinked_count),
                "action": "Use reprocess_note with steps=[\"linking\"] to auto-link isolated notes",
                "severity": if unlinked_count as f64 / total_notes as f64 > 0.5 { "high" } else { "medium" }
            }));
        }
        if notes_without_tags > 0 {
            recommendations.push(serde_json::json!({
                "type": "untagged_notes",
                "message": format!("{} notes have no tags or concepts", notes_without_tags),
                "action": "Tag notes with relevant concepts for better discoverability",
                "severity": if notes_without_tags as f64 / total_notes as f64 > 0.5 { "high" } else { "medium" }
            }));
        }
    }
    if orphan_tag_count > 0 {
        recommendations.push(serde_json::json!({
            "type": "orphan_tags",
            "message": format!("{} tags are not used by any notes", orphan_tag_count),
            "action": "Review orphan tags via get_orphan_tags and clean up unused tags",
            "severity": "low"
        }));
    }

    Ok(Json(serde_json::json!({
        "health_score": health_score,
        "total_notes": total_notes,
        "stale_notes": stale_count,
        "stale_threshold_days": stale_days,
        "unlinked_notes": unlinked_count,
        "notes_without_tags": notes_without_tags,
        "orphan_tags": orphan_tag_count,
        "total_tags": all_tags.len(),
        "total_links": all_links.len(),
        "metrics": {
            "stale_ratio": if total_notes > 0 { stale_count as f64 / total_notes as f64 } else { 0.0 },
            "unlinked_ratio": if total_notes > 0 { unlinked_count as f64 / total_notes as f64 } else { 0.0 },
            "untagged_ratio": if total_notes > 0 { notes_without_tags as f64 / total_notes as f64 } else { 0.0 },
            "tag_coverage": if total_notes > 0 { 1.0 - (notes_without_tags as f64 / total_notes as f64) } else { 1.0 },
            "link_coverage": if total_notes > 0 { 1.0 - (unlinked_count as f64 / total_notes as f64) } else { 1.0 }
        },
        "recommendations": recommendations
    })))
}

/// Get tags not used by any notes (true orphan tags).
#[utoipa::path(
    get,
    path = "/api/v1/health/orphan-tags",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_orphan_tags(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<HealthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_LARGE) as usize;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let tags_repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    let mut tx = ctx.begin_tx().await?;

    // list_tx returns all tags with note_count via LEFT JOIN
    let all_tags = tags_repo.list_tx(&mut tx).await.unwrap_or_default();

    drop(tx);

    // Filter to true orphan tags (note_count == 0)
    let orphan_tags: Vec<serde_json::Value> = all_tags
        .into_iter()
        .filter(|tag| tag.note_count == 0)
        .take(limit)
        .map(|tag| {
            serde_json::json!({
                "tag": tag.name,
                "created_at": tag.created_at_utc
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "orphan_tags": orphan_tags,
        "count": orphan_tags.len()
    })))
}

/// Get notes not updated in a long time.
#[utoipa::path(
    get,
    path = "/api/v1/health/stale-notes",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_stale_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<HealthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let stale_days = query
        .stale_days
        .unwrap_or(matric_core::defaults::STALE_DAYS);
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_LARGE) as usize;
    let stale_threshold = chrono::Utc::now() - chrono::Duration::days(stale_days);

    let req_for_query = ListNotesRequest {
        limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT),
        offset: None,
        filter: None,
        sort_by: Some("updated_at".to_string()),
        sort_order: Some("asc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: Some(stale_threshold),
    };

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let all_notes = ctx
        .query(move |tx| Box::pin(async move { notes.list_tx(tx, req_for_query).await }))
        .await?;

    let stale_notes: Vec<serde_json::Value> = all_notes
        .notes
        .iter()
        .take(limit)
        .map(|n| {
            let days_stale = (chrono::Utc::now() - n.updated_at_utc).num_days();
            serde_json::json!({
                "id": n.id,
                "title": n.title,
                "updated_at": n.updated_at_utc,
                "days_stale": days_stale
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "stale_threshold_days": stale_days,
        "stale_notes": stale_notes,
        "count": stale_notes.len()
    })))
}

/// Get notes with no incoming or outgoing links.
#[utoipa::path(
    get,
    path = "/api/v1/health/unlinked-notes",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_unlinked_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<HealthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_LARGE) as usize;

    let req = ListNotesRequest {
        limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes_repo = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let links_repo = matric_db::PgLinkRepository::new(state.db.pool.clone());
    let (all_links, all_notes) = ctx
        .query(move |tx| {
            Box::pin(async move {
                let links = links_repo
                    .list_all_tx(tx, matric_core::defaults::INTERNAL_FETCH_LIMIT, 0)
                    .await
                    .unwrap_or_default();
                let notes = notes_repo.list_tx(tx, req).await?;
                Ok((links, notes))
            })
        })
        .await?;

    let linked_note_ids: std::collections::HashSet<Uuid> = all_links
        .iter()
        .flat_map(|link| {
            let mut ids = vec![link.from_note_id];
            if let Some(to_id) = link.to_note_id {
                ids.push(to_id);
            }
            ids
        })
        .collect();

    // Filter to unlinked notes
    let unlinked_notes: Vec<serde_json::Value> = all_notes
        .notes
        .iter()
        .filter(|n| !linked_note_ids.contains(&n.id))
        .take(limit)
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "title": n.title,
                "created_at": n.created_at_utc
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "unlinked_notes": unlinked_notes,
        "count": unlinked_notes.len(),
        "total_notes": all_notes.total,
        "linked_notes": linked_note_ids.len()
    })))
}

/// Get tag co-occurrence patterns.
#[utoipa::path(
    get,
    path = "/api/v1/health/tag-cooccurrence",
    tag = "System",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn get_tag_cooccurrence(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<HealthQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(matric_core::defaults::PAGE_LIMIT) as usize;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes_repo = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let tags_repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    let mut tx = ctx.begin_tx().await?;

    // Get all notes
    let all_notes = notes_repo
        .list_tx(
            &mut tx,
            ListNotesRequest {
                limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT),
                offset: None,
                filter: None,
                sort_by: None,
                sort_order: None,
                collection_id: None,
                tags: None,
                created_after: None,
                created_before: None,
                updated_after: None,
                updated_before: None,
            },
        )
        .await?;

    // Build co-occurrence matrix
    let mut cooccurrence: std::collections::HashMap<(String, String), i64> =
        std::collections::HashMap::new();

    for note in &all_notes.notes {
        let note_tags = tags_repo
            .get_for_note_tx(&mut tx, note.id)
            .await
            .unwrap_or_default();
        // Generate all pairs
        for i in 0..note_tags.len() {
            for j in (i + 1)..note_tags.len() {
                let (a, b) = if note_tags[i] < note_tags[j] {
                    (note_tags[i].clone(), note_tags[j].clone())
                } else {
                    (note_tags[j].clone(), note_tags[i].clone())
                };
                *cooccurrence.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    drop(tx);

    // Sort by frequency and take top pairs
    let mut pairs: Vec<_> = cooccurrence.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));

    let top_pairs: Vec<serde_json::Value> = pairs
        .into_iter()
        .take(limit)
        .map(|((tag_a, tag_b), count)| {
            serde_json::json!({
                "tag_a": tag_a,
                "tag_b": tag_b,
                "count": count
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "cooccurrence_pairs": top_pairs,
        "count": top_pairs.len()
    })))
}

#[derive(Debug, Deserialize)]
struct ListNotesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    filter: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    collection_id: Option<Uuid>,
    /// Filter by tags (comma-separated)
    tags: Option<String>,
    /// Filter: notes created after this timestamp (ISO 8601, timezone optional - assumes UTC)
    created_after: Option<FlexibleDateTime>,
    /// Filter: notes created before this timestamp (ISO 8601, timezone optional - assumes UTC)
    created_before: Option<FlexibleDateTime>,
    /// Filter: notes updated after this timestamp (ISO 8601, timezone optional - assumes UTC)
    updated_after: Option<FlexibleDateTime>,
    /// Filter: notes updated before this timestamp (ISO 8601, timezone optional - assumes UTC)
    updated_before: Option<FlexibleDateTime>,
    /// Relative time filter: "7d" (7 days), "1w" (1 week), "1m" (1 month), "2h" (2 hours)
    since: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/notes",
    tag = "Notes",
    responses(
        (status = 200, description = "Success"),
    )
)]
async fn list_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<ListNotesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Issue #271 + #29: Validate limit parameter before database query
    // limit=0 is valid and returns an empty array (count-only queries)
    if let Some(limit) = query.limit {
        if limit < 0 {
            return Err(ApiError::BadRequest("limit must be >= 0".into()));
        }
    }

    // Parse comma-separated tags into Vec
    let tags = query.tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    // Parse relative time (e.g., "7d", "1w") and use as created_after if provided
    // FlexibleDateTime converts to DateTime<Utc> via into_inner()
    let created_after = query
        .created_after
        .map(|dt| dt.into_inner())
        .or_else(|| query.since.as_ref().and_then(|s| parse_relative_time(s)));

    let req = ListNotesRequest {
        limit: query.limit,
        offset: query.offset,
        filter: query.filter,
        sort_by: query.sort_by,
        sort_order: query.sort_order,
        collection_id: query.collection_id,
        tags,
        created_after,
        created_before: query.created_before.map(|dt| dt.into_inner()),
        updated_after: query.updated_after.map(|dt| dt.into_inner()),
        updated_before: query.updated_before.map(|dt| dt.into_inner()),
    };

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let response = ctx
        .query(move |tx| Box::pin(async move { notes.list_tx(tx, req).await }))
        .await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CreateNoteBody {
    content: String,
    format: Option<String>,
    source: Option<String>,
    collection_id: Option<Uuid>,
    tags: Option<Vec<String>>,
    /// AI revision mode: "full" (default), "light", or "none"
    #[serde(default)]
    revision_mode: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    /// Optional document type ID for explicit typing (auto-detected if omitted)
    document_type_id: Option<Uuid>,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes",
    tag = "Notes",
    request_body = CreateNoteBody,
    responses(
        (status = 201, description = "Created"),
        (status = 400, description = "Bad request"),
    )
)]
async fn create_note(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<CreateNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Extract tags for SKOS processing
    let tags_for_skos = body.tags.clone();

    let req = CreateNoteRequest {
        content: body.content,
        format: body.format.unwrap_or_else(|| "markdown".to_string()),
        source: body.source.unwrap_or_else(|| "api".to_string()),
        collection_id: body.collection_id,
        tags: body.tags, // Legacy flat tags still inserted for backwards compatibility
        metadata: body.metadata,
        document_type_id: body.document_type_id,
    };

    // Parse revision mode (default to Light)
    let revision_mode = match body.revision_mode.as_deref() {
        Some("full") => RevisionMode::Full,
        Some("none") => RevisionMode::None,
        _ => RevisionMode::Light, // "light" or unspecified
    };

    // Insert note (archive-scoped via SchemaContext)
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let req_clone = req.clone();
    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req_clone).await }))
        .await?;

    // Determine schema for background jobs (Issue #109)
    let schema_for_jobs = if archive_ctx.schema != "public" {
        Some(archive_ctx.schema.as_str())
    } else {
        None
    };

    // Validate tag depth and length before processing (fixes #193, #189)
    if let Some(ref tags) = tags_for_skos {
        for tag in tags {
            if tag.len() > matric_core::defaults::TAG_NAME_MAX_LENGTH {
                return Err(ApiError::BadRequest(format!(
                    "Tag '{}...' exceeds {} character limit",
                    &tag[..50.min(tag.len())],
                    matric_core::defaults::TAG_NAME_MAX_LENGTH
                )));
            }
            let depth = tag
                .split('/')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .count();
            if depth > matric_core::tags::MAX_TAG_PATH_DEPTH {
                return Err(ApiError::BadRequest(format!(
                    "Tag '{}' exceeds maximum depth of {} levels",
                    tag,
                    matric_core::tags::MAX_TAG_PATH_DEPTH
                )));
            }
        }
    }

    // Resolve tags via SKOS and create parent hierarchy (fixes #301)
    if let Some(tags) = tags_for_skos {
        let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
        let ctx_for_tags = state.db.for_schema(&archive_ctx.schema)?;

        let _ = ctx_for_tags
            .execute(move |tx| {
                Box::pin(async move {
                    let mut concept_ids = Vec::new();
                    for tag in &tags {
                        // Parse hierarchical tag (e.g., "programming/rust" -> ["programming", "rust"])
                        let tag_input = TagInput::parse(tag);
                        // Resolve or create SKOS concepts (auto-creates parent tags)
                        if let Ok(resolved) = skos.resolve_or_create_tag_tx(tx, &tag_input).await {
                            concept_ids.push(resolved.concept_id);
                        }
                    }
                    // Tag the note with resolved SKOS concepts
                    if !concept_ids.is_empty() {
                        let batch_req = BatchTagNoteRequest {
                            note_id,
                            concept_ids,
                            source: "user".to_string(),
                            confidence: None,
                            created_by: None,
                        };
                        skos.batch_tag_note_tx(tx, batch_req).await?;
                    }
                    Ok::<_, matric_core::Error>(())
                })
            })
            .await;
    }

    // If mode is "none", copy original to revised (so embedding/search works on it)
    if revision_mode == RevisionMode::None {
        let _ = state
            .db
            .notes
            .update_revised(
                note_id,
                &req.content,
                Some("Original preserved (no AI revision)"),
            )
            .await;
    }

    // Queue NLP pipeline with archive context (Issue #109)
    queue_nlp_pipeline(
        &state.db,
        note_id,
        revision_mode,
        &state.event_bus,
        schema_for_jobs,
    )
    .await;

    // Emit NoteUpdated event (Issue #41)
    let tags_for_event = state
        .db
        .tags
        .get_for_note(note_id)
        .await
        .unwrap_or_default();
    state.event_bus.emit(ServerEvent::NoteUpdated {
        note_id,
        title: None, // Title not yet generated
        tags: tags_for_event,
        has_ai_content: false, // AI revision queued but not yet complete
        has_links: false,
    });

    // Invalidate search cache so new note appears in search results (#341)
    state.search_cache.invalidate_all().await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": note_id })),
    ))
}

#[derive(Debug, Deserialize)]
struct BulkCreateNoteItem {
    content: String,
    tags: Option<Vec<String>>,
    /// Optional JSON metadata for the note
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    /// AI revision mode: "full" (default), "light", or "none"
    #[serde(default)]
    revision_mode: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct BulkCreateNotesBody {
    notes: Vec<BulkCreateNoteItem>,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/bulk",
    tag = "Notes",
    request_body = BulkCreateNotesBody,
    responses(
        (status = 200, description = "Success"),
        (status = 400, description = "Bad request"),
    )
)]
async fn bulk_create_notes(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<BulkCreateNotesBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.notes.is_empty() {
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({ "ids": [], "count": 0 })),
        ));
    }

    if body.notes.len() > 100 {
        return Err(ApiError::BadRequest(
            "Maximum 100 notes per batch".to_string(),
        ));
    }

    // Validate tags in each note
    for (i, note) in body.notes.iter().enumerate() {
        // Validate tag depth and length (fixes #193, #189)
        if let Some(ref tags) = note.tags {
            for tag in tags {
                if tag.len() > matric_core::defaults::TAG_NAME_MAX_LENGTH {
                    return Err(ApiError::BadRequest(format!(
                        "Note at index {}: tag '{}...' exceeds {} character limit",
                        i,
                        &tag[..50.min(tag.len())],
                        matric_core::defaults::TAG_NAME_MAX_LENGTH
                    )));
                }
                let depth = tag
                    .split('/')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .count();
                if depth > matric_core::tags::MAX_TAG_PATH_DEPTH {
                    return Err(ApiError::BadRequest(format!(
                        "Note at index {}: tag '{}' exceeds maximum depth of {} levels",
                        i,
                        tag,
                        matric_core::tags::MAX_TAG_PATH_DEPTH
                    )));
                }
            }
        }
    }

    // Convert to CreateNoteRequest
    let requests: Vec<CreateNoteRequest> = body
        .notes
        .iter()
        .map(|item| CreateNoteRequest {
            content: item.content.clone(),
            format: "markdown".to_string(),
            source: "api_bulk".to_string(),
            collection_id: None,
            tags: item.tags.clone(),
            metadata: item.metadata.clone(),
            document_type_id: None,
        })
        .collect();

    // Bulk insert all notes (archive-scoped via SchemaContext)
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let reqs = requests.clone();
    let ids = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_bulk_tx(tx, reqs).await }))
        .await?;

    // Resolve tags via SKOS for each note (fixes #393: required_tags filter
    // returned 0 for bulk_create notes because SKOS concepts were never created)
    for (i, note_id) in ids.iter().enumerate() {
        if let Some(ref tags) = requests[i].tags {
            if !tags.is_empty() {
                let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
                let ctx_for_tags = state.db.for_schema(&archive_ctx.schema)?;
                let tags_owned = tags.clone();
                let nid = *note_id;

                let _ = ctx_for_tags
                    .execute(move |tx| {
                        Box::pin(async move {
                            let mut concept_ids = Vec::new();
                            for tag in &tags_owned {
                                let tag_input = TagInput::parse(tag);
                                if let Ok(resolved) =
                                    skos.resolve_or_create_tag_tx(tx, &tag_input).await
                                {
                                    concept_ids.push(resolved.concept_id);
                                }
                            }
                            if !concept_ids.is_empty() {
                                let batch_req = BatchTagNoteRequest {
                                    note_id: nid,
                                    concept_ids,
                                    source: "user".to_string(),
                                    confidence: None,
                                    created_by: None,
                                };
                                skos.batch_tag_note_tx(tx, batch_req).await?;
                            }
                            Ok::<_, matric_core::Error>(())
                        })
                    })
                    .await;
            }
        }
    }

    // Determine schema for background jobs
    let schema_for_jobs = if archive_ctx.schema != "public" {
        Some(archive_ctx.schema.as_str())
    } else {
        None
    };

    // Queue NLP pipeline for each note based on revision mode
    for (i, note_id) in ids.iter().enumerate() {
        let revision_mode = match body.notes[i].revision_mode.as_deref() {
            Some("light") => RevisionMode::Light,
            Some("none") => RevisionMode::None,
            _ => RevisionMode::Full,
        };

        // Note: insert_bulk_tx already populates note_revised_current with
        // original content, so no update_revised call needed for any mode.

        queue_nlp_pipeline(
            &state.db,
            *note_id,
            revision_mode,
            &state.event_bus,
            schema_for_jobs,
        )
        .await;
    }

    // Invalidate search cache so bulk-created notes appear in search results (#341)
    state.search_cache.invalidate_all().await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "ids": ids,
            "count": ids.len()
        })),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID")
    ),
    responses(
        (status = 200, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn get_note(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let note = ctx
        .query(move |tx| Box::pin(async move { notes.fetch_tx(tx, id).await }))
        .await?;
    Ok(Json(note))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateNoteBody {
    content: Option<String>,
    starred: Option<bool>,
    archived: Option<bool>,
    /// AI revision mode: "full" (default), "light", or "none"
    #[serde(default)]
    revision_mode: Option<String>,
    metadata: Option<serde_json::Value>,
    /// Replace all tags on this note (full replacement, not merge)
    tags: Option<Vec<String>>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID")
    ),
    request_body = UpdateNoteBody,
    responses(
        (status = 200, description = "Success"),
        (status = 404, description = "Not found"),
        (status = 400, description = "Bad request"),
    )
)]
async fn update_note(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let pool = state.db.pool.clone();

    // Update content if provided
    let content_changed = body.content.is_some();
    if let Some(content) = &body.content {
        let notes = matric_db::PgNoteRepository::new(pool.clone());
        let content_clone = content.clone();
        ctx.execute(move |tx| {
            Box::pin(async move { notes.update_original_tx(tx, id, &content_clone).await })
        })
        .await?;
    }

    // Update status if provided
    if body.starred.is_some() || body.archived.is_some() || body.metadata.is_some() {
        let req = UpdateNoteStatusRequest {
            starred: body.starred,
            archived: body.archived,
            metadata: body.metadata,
        };
        let notes = matric_db::PgNoteRepository::new(pool.clone());
        ctx.execute(move |tx| Box::pin(async move { notes.update_status_tx(tx, id, req).await }))
            .await?;
    }

    // Update tags if provided (#226)
    if let Some(tags) = body.tags {
        // Validate tags
        for tag in &tags {
            if tag.len() > matric_core::defaults::TAG_NAME_MAX_LENGTH {
                return Err(ApiError::BadRequest(format!(
                    "Tag '{}...' exceeds {} character limit",
                    &tag[..50.min(tag.len())],
                    matric_core::defaults::TAG_NAME_MAX_LENGTH
                )));
            }
        }
        let repo = matric_db::PgTagRepository::new(pool.clone());
        ctx.execute(move |tx| {
            Box::pin(async move { repo.set_for_note_tx(tx, id, tags, "api").await })
        })
        .await?;
    }

    // Queue full NLP pipeline if content changed
    if content_changed {
        // Parse revision mode (default to Light)
        let revision_mode = match body.revision_mode.as_deref() {
            Some("light") => RevisionMode::Light,
            Some("none") => RevisionMode::None,
            _ => RevisionMode::Full, // "full" or unspecified
        };

        // If mode is "none", copy original to revised (so embedding/search works on it)
        if revision_mode == RevisionMode::None {
            if let Some(content) = &body.content {
                let notes = matric_db::PgNoteRepository::new(pool.clone());
                let content_clone = content.clone();
                let _ = ctx
                    .execute(move |tx| {
                        Box::pin(async move {
                            notes
                                .update_revised_tx(
                                    tx,
                                    id,
                                    &content_clone,
                                    Some("Original preserved (no AI revision)"),
                                )
                                .await
                        })
                    })
                    .await;
            }
        }

        // Determine schema for background jobs (Issue #413)
        let schema_for_jobs = if archive_ctx.schema != "public" {
            Some(archive_ctx.schema.as_str())
        } else {
            None
        };
        queue_nlp_pipeline(
            &state.db,
            id,
            revision_mode,
            &state.event_bus,
            schema_for_jobs,
        )
        .await;
    }

    // Fetch and return the updated note
    let notes = matric_db::PgNoteRepository::new(pool);
    let note = ctx
        .query(move |tx| Box::pin(async move { notes.fetch_tx(tx, id).await }))
        .await?;

    // Emit NoteUpdated event (Issue #41)
    state.event_bus.emit(ServerEvent::NoteUpdated {
        note_id: id,
        title: note.note.title.clone(),
        tags: note.tags.clone(),
        has_ai_content: note.revised.ai_generated_at.is_some(),
        has_links: !note.links.is_empty(),
    });

    // Invalidate search cache so updated content appears in search results (#341)
    state.search_cache.invalidate_all().await;

    Ok(Json(note))
}

#[utoipa::path(
    delete,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID")
    ),
    responses(
        (status = 204, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn delete_note(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes.soft_delete_tx(tx, id).await }))
        .await?;

    // Invalidate search cache so deleted notes don't appear in results (#247)
    state.search_cache.invalidate_all().await;

    Ok(StatusCode::NO_CONTENT)
}

/// Permanently delete a note by queuing a purge job.
/// This triggers CASCADE DELETE on all related data.
#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/purge",
    tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID")
    ),
    responses(
        (status = 200, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn purge_note(
    _auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // NOTE: Write scope enforcement removed for pre-tenancy permissive mode.
    // Scope infrastructure kept in place for future multi-tenancy.
    // See: issue #129, commit 49d9f3f

    // Verify note exists first
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let exists = ctx
        .query(move |tx| Box::pin(async move { notes.exists_tx(tx, id).await }))
        .await?;

    if !exists {
        return Err(ApiError::NotFound(format!("Note {} not found", id)));
    }

    // Queue a high-priority purge job
    let job_id = state
        .db
        .jobs
        .queue(
            Some(id),
            JobType::PurgeNote,
            JobType::PurgeNote.default_priority(),
            None,
        )
        .await?;

    state.event_bus.emit(ServerEvent::JobQueued {
        job_id,
        job_type: format!("{:?}", JobType::PurgeNote),
        note_id: Some(id),
    });

    // Invalidate search cache (#247)
    state.search_cache.invalidate_all().await;

    Ok(Json(serde_json::json!({
        "status": "queued",
        "job_id": job_id.to_string(),
        "note_id": id.to_string()
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateStatusBody {
    starred: Option<bool>,
    archived: Option<bool>,
}

#[utoipa::path(patch, path = "/api/v1/notes/{id}/status", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body = UpdateStatusBody,
    responses(
        (status = 204, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn update_note_status(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = UpdateNoteStatusRequest {
        starred: body.starred,
        archived: body.archived,
        metadata: None,
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes.update_status_tx(tx, id, req).await }))
        .await?;

    // Invalidate search cache so status changes are reflected (#341)
    state.search_cache.invalidate_all().await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct RestoreNoteQuery {
    /// AI revision mode: "full" (default), "light", or "none"
    revision_mode: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/notes/{id}/restore", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses(
        (status = 200, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn restore_note(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<RestoreNoteQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes.restore_tx(tx, id).await }))
        .await?;

    // Parse revision mode (default to Light)
    let revision_mode = match query.revision_mode.as_deref() {
        Some("full") => RevisionMode::Full,
        Some("none") => RevisionMode::None,
        _ => RevisionMode::Light,
    };

    // Determine schema for background jobs
    let schema_for_jobs = if archive_ctx.schema != "public" {
        Some(archive_ctx.schema.as_str())
    } else {
        None
    };

    // Re-run NLP pipeline to ensure note is properly indexed
    queue_nlp_pipeline(
        &state.db,
        id,
        revision_mode,
        &state.event_bus,
        schema_for_jobs,
    )
    .await;

    // Invalidate search cache so restored note appears in results (#247)
    state.search_cache.invalidate_all().await;

    Ok(Json(serde_json::json!({ "restored": true, "id": id })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ReprocessNoteBody {
    /// AI revision mode: "full", "light" (default), or "none"
    revision_mode: Option<String>,
    /// Specific pipeline steps to run. If omitted or contains "all", runs all steps.
    steps: Option<Vec<String>>,
}

/// Manually trigger NLP pipeline steps for a note.
/// Useful for re-processing after model changes or fixing failed jobs.
/// When `steps` is provided, only the specified steps are queued.
#[utoipa::path(post, path = "/api/v1/notes/{id}/reprocess", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body(content = Option<ReprocessNoteBody>),
    responses(
        (status = 200, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn reprocess_note(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    body: Option<Json<ReprocessNoteBody>>,
) -> Result<impl IntoResponse, ApiError> {
    // Verify note exists
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let _ = ctx
        .query(move |tx| Box::pin(async move { notes.fetch_tx(tx, id).await }))
        .await?;

    let body = body.map(|b| b.0);

    // Parse revision mode (default to Light)
    let revision_mode = match body.as_ref().and_then(|b| b.revision_mode.as_deref()) {
        Some("full") => RevisionMode::Full,
        Some("none") => RevisionMode::None,
        _ => RevisionMode::Light,
    };

    // Determine which steps to run
    let requested_steps = body.as_ref().and_then(|b| b.steps.as_ref());
    let run_all = requested_steps.is_none()
        || requested_steps
            .map(|s| s.is_empty() || s.iter().any(|step| step == "all"))
            .unwrap_or(true);

    let should_run = |step: &str| -> bool {
        run_all
            || requested_steps
                .map(|s| s.iter().any(|rs| rs == step))
                .unwrap_or(false)
    };

    let mut jobs_queued = Vec::new();

    // Queue AI revision if requested and mode != None
    if revision_mode != RevisionMode::None && should_run("ai_revision") {
        let payload = serde_json::json!({ "revision_mode": revision_mode });
        if let Ok(Some(job_id)) = state
            .db
            .jobs
            .queue_deduplicated(
                Some(id),
                JobType::AiRevision,
                JobType::AiRevision.default_priority(),
                Some(payload),
            )
            .await
        {
            state.event_bus.emit(ServerEvent::JobQueued {
                job_id,
                job_type: format!("{:?}", JobType::AiRevision),
                note_id: Some(id),
            });
            jobs_queued.push("ai_revision");
        }
    }

    // Queue remaining pipeline steps selectively
    let step_types = [
        ("embedding", JobType::Embedding),
        ("title_generation", JobType::TitleGeneration),
        ("linking", JobType::Linking),
        ("concept_tagging", JobType::ConceptTagging),
    ];

    for (step_name, job_type) in &step_types {
        if should_run(step_name) {
            if let Ok(Some(job_id)) = state
                .db
                .jobs
                .queue_deduplicated(Some(id), *job_type, job_type.default_priority(), None)
                .await
            {
                state.event_bus.emit(ServerEvent::JobQueued {
                    job_id,
                    job_type: format!("{:?}", job_type),
                    note_id: Some(id),
                });
                jobs_queued.push(step_name);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "message": "NLP pipeline queued",
        "note_id": id,
        "revision_mode": revision_mode,
        "jobs_queued": jobs_queued
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct BulkReprocessBody {
    /// AI revision mode: "full", "light" (default), or "none"
    revision_mode: Option<String>,
    /// Specific note IDs to reprocess. If omitted, reprocesses all non-deleted notes.
    note_ids: Option<Vec<Uuid>>,
    /// Pipeline steps to run. Defaults to all steps.
    steps: Option<Vec<String>>,
    /// Maximum number of notes to process (safety limit). Default: 500
    limit: Option<i64>,
}

/// Bulk reprocess notes through the NLP pipeline.
///
/// The "Make Smarter" endpoint — triggers AI revision, re-embedding, and relinking
/// for multiple notes at once. Use `revision_mode: "full"` for deep contextual
/// enhancement, or omit for light formatting.
#[utoipa::path(post, path = "/api/v1/notes/reprocess", tag = "Notes",
    request_body(content = Option<BulkReprocessBody>),
    responses(
        (status = 200, description = "Jobs queued for bulk reprocessing"),
    )
)]
async fn bulk_reprocess_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    body: Option<Json<BulkReprocessBody>>,
) -> Result<impl IntoResponse, ApiError> {
    let body = body.map(|b| b.0);
    let limit = body.as_ref().and_then(|b| b.limit).unwrap_or(500).min(5000);

    // Parse revision mode (default to Light)
    let revision_mode = match body.as_ref().and_then(|b| b.revision_mode.as_deref()) {
        Some("full") => RevisionMode::Full,
        Some("none") => RevisionMode::None,
        _ => RevisionMode::Light, // "light" or unspecified
    };

    // Determine which steps to run
    let requested_steps = body.as_ref().and_then(|b| b.steps.as_ref());
    let run_all = requested_steps.is_none()
        || requested_steps
            .map(|s| s.is_empty() || s.iter().any(|step| step == "all"))
            .unwrap_or(true);

    let should_run = |step: &str| -> bool {
        run_all
            || requested_steps
                .map(|s| s.iter().any(|rs| rs == step))
                .unwrap_or(false)
    };

    // Get note IDs to process
    let note_ids: Vec<Uuid> = if let Some(ids) = body.as_ref().and_then(|b| b.note_ids.clone()) {
        ids.into_iter().take(limit as usize).collect()
    } else {
        // Fetch all active note IDs from this archive
        let ctx = state.db.for_schema(&archive_ctx.schema)?;
        let notes_repo = matric_db::PgNoteRepository::new(state.db.pool.clone());
        let notes = ctx
            .query(move |tx| {
                Box::pin(async move {
                    notes_repo
                        .list_tx(
                            tx,
                            ListNotesRequest {
                                limit: Some(limit),
                                filter: Some("all".to_string()),
                                ..Default::default()
                            },
                        )
                        .await
                })
            })
            .await?;
        notes.notes.into_iter().map(|n| n.id).collect()
    };

    let total = note_ids.len();
    let mut jobs_queued: usize = 0;

    for note_id in &note_ids {
        // Queue AI revision if requested and mode != None
        if revision_mode != RevisionMode::None && should_run("ai_revision") {
            let payload = serde_json::json!({
                "revision_mode": revision_mode,
                "schema": &archive_ctx.schema,
            });
            if let Ok(Some(job_id)) = state
                .db
                .jobs
                .queue_deduplicated(
                    Some(*note_id),
                    JobType::AiRevision,
                    JobType::AiRevision.default_priority(),
                    Some(payload),
                )
                .await
            {
                state.event_bus.emit(ServerEvent::JobQueued {
                    job_id,
                    job_type: format!("{:?}", JobType::AiRevision),
                    note_id: Some(*note_id),
                });
                jobs_queued += 1;
            }
        }

        // Queue remaining pipeline steps with schema context (Issue #413)
        let step_types = [
            ("embedding", JobType::Embedding),
            ("title_generation", JobType::TitleGeneration),
            ("linking", JobType::Linking),
            ("concept_tagging", JobType::ConceptTagging),
        ];

        let step_payload = if archive_ctx.schema != "public" {
            Some(serde_json::json!({ "schema": &archive_ctx.schema }))
        } else {
            None
        };

        for (step_name, job_type) in &step_types {
            if should_run(step_name) {
                if let Ok(Some(job_id)) = state
                    .db
                    .jobs
                    .queue_deduplicated(
                        Some(*note_id),
                        *job_type,
                        job_type.default_priority(),
                        step_payload.clone(),
                    )
                    .await
                {
                    state.event_bus.emit(ServerEvent::JobQueued {
                        job_id,
                        job_type: format!("{:?}", job_type),
                        note_id: Some(*note_id),
                    });
                    jobs_queued += 1;
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "message": "Bulk reprocessing queued",
        "notes_count": total,
        "jobs_queued": jobs_queued,
        "revision_mode": revision_mode,
    })))
}

// =============================================================================
// TAG HANDLERS
// =============================================================================

#[utoipa::path(get, path = "/api/v1/notes/{id}/tags", tag = "Tags",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success"))
)]
async fn get_note_tags(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    let tags = ctx
        .query(move |tx| Box::pin(async move { repo.get_for_note_tx(tx, id).await }))
        .await?;
    Ok(Json(tags))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct SetTagsBody {
    tags: Vec<String>,
}

#[utoipa::path(put, path = "/api/v1/notes/{id}/tags", tag = "Tags",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body = SetTagsBody,
    responses((status = 204, description = "Success"))
)]
async fn set_note_tags(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetTagsBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate tag depth and length (fixes #193, #189)
    for tag in &body.tags {
        if tag.len() > matric_core::defaults::TAG_NAME_MAX_LENGTH {
            return Err(ApiError::BadRequest(format!(
                "Tag '{}...' exceeds {} character limit",
                &tag[..50.min(tag.len())],
                matric_core::defaults::TAG_NAME_MAX_LENGTH
            )));
        }
        let depth = tag
            .split('/')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .count();
        if depth > matric_core::tags::MAX_TAG_PATH_DEPTH {
            return Err(ApiError::BadRequest(format!(
                "Tag '{}' exceeds maximum depth of {} levels",
                tag,
                matric_core::tags::MAX_TAG_PATH_DEPTH
            )));
        }
    }
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move { repo.set_for_note_tx(tx, id, body.tags, "api").await })
    })
    .await?;

    // Invalidate search cache so tag changes are reflected in filtered searches (#341)
    state.search_cache.invalidate_all().await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/tags", tag = "Tags",
    responses((status = 200, description = "Success"))
)]
async fn list_tags(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    let tags = ctx
        .query(move |tx| Box::pin(async move { repo.list_tx(tx).await }))
        .await?;
    Ok(Json(tags))
}

// =============================================================================
// SKOS CONCEPT HANDLERS
// =============================================================================

// --- Concept Scheme Handlers ---

#[utoipa::path(get, path = "/api/v1/concepts/schemes", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn list_concept_schemes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let schemes = ctx
        .query(move |tx| Box::pin(async move { skos.list_schemes_tx(tx, false).await }))
        .await?;
    Ok(Json(schemes))
}

#[utoipa::path(post, path = "/api/v1/concepts/schemes", tag = "SKOS",
    request_body = matric_core::CreateConceptSchemeRequest,
    responses((status = 201, description = "Success")))]
async fn create_concept_scheme(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<matric_core::CreateConceptSchemeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let id = ctx
        .execute(move |tx| Box::pin(async move { skos.create_scheme_tx(tx, body).await }))
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[utoipa::path(get, path = "/api/v1/concepts/schemes/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_concept_scheme(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let scheme = ctx
        .query(move |tx| Box::pin(async move { skos.get_scheme_tx(tx, id).await }))
        .await?
        .ok_or_else(|| ApiError::NotFound("Concept scheme not found".to_string()))?;
    Ok(Json(scheme))
}

#[utoipa::path(patch, path = "/api/v1/concepts/schemes/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    request_body = matric_core::UpdateConceptSchemeRequest,
    responses((status = 204, description = "Success")))]
async fn update_concept_scheme(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<matric_core::UpdateConceptSchemeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.update_scheme_tx(tx, id, body).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct DeleteSchemeQuery {
    force: Option<bool>,
}

#[utoipa::path(delete, path = "/api/v1/concepts/schemes/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn delete_concept_scheme(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteSchemeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let force = query.force.unwrap_or(false);
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.delete_scheme_tx(tx, id, force).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/v1/concepts/schemes/{id}/top-concepts", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_top_concepts(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let concepts = ctx
        .query(move |tx| Box::pin(async move { skos.get_top_concepts_tx(tx, id).await }))
        .await?;
    Ok(Json(concepts))
}

// --- Concept Handlers ---

#[derive(Debug, Deserialize)]
struct SearchConceptsQuery {
    q: Option<String>,
    scheme_id: Option<Uuid>,
    status: Option<String>,
    facet_type: Option<String>,
    top_only: Option<bool>,
    include_deprecated: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[utoipa::path(get, path = "/api/v1/concepts", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn search_concepts(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<SearchConceptsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let req = matric_core::SearchConceptsRequest {
        query: query.q,
        scheme_id: query.scheme_id,
        status: query.status.and_then(|s| s.parse().ok()),
        facet_type: query.facet_type.and_then(|f| f.parse().ok()),
        top_concepts_only: query.top_only.unwrap_or(false),
        include_deprecated: query.include_deprecated.unwrap_or(false),
        limit: query.limit.unwrap_or(matric_core::defaults::PAGE_LIMIT),
        offset: query.offset.unwrap_or(matric_core::defaults::PAGE_OFFSET),
        max_depth: None,
        has_antipattern: None,
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let result = ctx
        .query(move |tx| Box::pin(async move { skos.search_concepts_tx(tx, req).await }))
        .await?;
    Ok(Json(result))
}

#[utoipa::path(post, path = "/api/v1/concepts", tag = "SKOS",
    request_body = matric_core::CreateConceptRequest,
    responses((status = 201, description = "Success")))]
async fn create_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<matric_core::CreateConceptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let id = ctx
        .execute(move |tx| Box::pin(async move { skos.create_concept_tx(tx, body).await }))
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Debug, Deserialize)]
struct AutocompleteQuery {
    q: String,
    limit: Option<i64>,
}

#[utoipa::path(get, path = "/api/v1/concepts/autocomplete", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn autocomplete_concepts(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<AutocompleteQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let query_str = query.q.clone();
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_AUTOCOMPLETE);
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let concepts = ctx
        .query(move |tx| {
            Box::pin(async move { skos.search_labels_tx(tx, &query_str, limit).await })
        })
        .await?;
    Ok(Json(concepts))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let concept = ctx
        .query(move |tx| Box::pin(async move { skos.get_concept_with_label_tx(tx, id).await }))
        .await?
        .ok_or_else(|| ApiError::NotFound("Concept not found".to_string()))?;
    Ok(Json(concept))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}/full", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_concept_full(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let concept = ctx
        .query(move |tx| Box::pin(async move { skos.get_concept_full_tx(tx, id).await }))
        .await?
        .ok_or_else(|| ApiError::NotFound("Concept not found".to_string()))?;
    Ok(Json(concept))
}

#[utoipa::path(patch, path = "/api/v1/concepts/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    request_body = matric_core::UpdateConceptRequest,
    responses((status = 200, description = "Success")))]
async fn update_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<matric_core::UpdateConceptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    // Return updated concept for better REST semantics (issue #142)
    let concept = ctx
        .execute(move |tx| {
            Box::pin(async move {
                skos.update_concept_tx(tx, id, body).await?;
                skos.get_concept_with_label_tx(tx, id)
                    .await?
                    .ok_or_else(|| {
                        matric_core::Error::NotFound(format!("Concept {id} not found after update"))
                    })
            })
        })
        .await?;
    Ok(Json(concept))
}

#[utoipa::path(delete, path = "/api/v1/concepts/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn delete_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.delete_concept_tx(tx, id).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Hierarchy Handlers ---

#[utoipa::path(get, path = "/api/v1/concepts/{id}/ancestors", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_ancestors(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Get broader relations which represent ancestors
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let relations = ctx
        .query(move |tx| {
            Box::pin(async move {
                skos.get_semantic_relations_tx(
                    tx,
                    id,
                    Some(matric_core::SkosSemanticRelation::Broader),
                )
                .await
            })
        })
        .await?;
    Ok(Json(relations))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}/descendants", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_descendants(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Get narrower relations which represent descendants
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let relations = ctx
        .query(move |tx| {
            Box::pin(async move {
                skos.get_semantic_relations_tx(
                    tx,
                    id,
                    Some(matric_core::SkosSemanticRelation::Narrower),
                )
                .await
            })
        })
        .await?;
    Ok(Json(relations))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}/broader", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_broader(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let relations = ctx
        .query(move |tx| {
            Box::pin(async move {
                skos.get_semantic_relations_tx(
                    tx,
                    id,
                    Some(matric_core::SkosSemanticRelation::Broader),
                )
                .await
            })
        })
        .await?;
    Ok(Json(relations))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}/narrower", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_narrower(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let relations = ctx
        .query(move |tx| {
            Box::pin(async move {
                skos.get_semantic_relations_tx(
                    tx,
                    id,
                    Some(matric_core::SkosSemanticRelation::Narrower),
                )
                .await
            })
        })
        .await?;
    Ok(Json(relations))
}

#[utoipa::path(get, path = "/api/v1/concepts/{id}/related", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_related(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let relations = ctx
        .query(move |tx| {
            Box::pin(async move {
                skos.get_semantic_relations_tx(
                    tx,
                    id,
                    Some(matric_core::SkosSemanticRelation::Related),
                )
                .await
            })
        })
        .await?;
    Ok(Json(relations))
}

#[derive(Debug, Deserialize)]
struct AddRelationBody {
    target_id: Uuid,
}

#[utoipa::path(post, path = "/api/v1/concepts/{id}/broader", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 201, description = "Success")))]
async fn add_broader(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddRelationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = matric_core::CreateSemanticRelationRequest {
        subject_id: id,
        object_id: body.target_id,
        relation_type: matric_core::SkosSemanticRelation::Broader,
        inference_score: None,
        is_inferred: false,
        created_by: Some("api".to_string()),
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.create_semantic_relation_tx(tx, req).await }))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "success": true })),
    ))
}

#[utoipa::path(post, path = "/api/v1/concepts/{id}/narrower", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 201, description = "Success")))]
async fn add_narrower(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddRelationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = matric_core::CreateSemanticRelationRequest {
        subject_id: id,
        object_id: body.target_id,
        relation_type: matric_core::SkosSemanticRelation::Narrower,
        inference_score: None,
        is_inferred: false,
        created_by: Some("api".to_string()),
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.create_semantic_relation_tx(tx, req).await }))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "success": true })),
    ))
}

#[utoipa::path(post, path = "/api/v1/concepts/{id}/related", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 201, description = "Success")))]
async fn add_related(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<AddRelationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = matric_core::CreateSemanticRelationRequest {
        subject_id: id,
        object_id: body.target_id,
        relation_type: matric_core::SkosSemanticRelation::Related,
        inference_score: None,
        is_inferred: false,
        created_by: Some("api".to_string()),
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.create_semantic_relation_tx(tx, req).await }))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "success": true })),
    ))
}

#[utoipa::path(delete, path = "/api/v1/concepts/{id}/broader/{target_id}", tag = "SKOS",
    params(("id" = Uuid, Path,), ("target_id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn remove_broader(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            skos.delete_semantic_relation_by_triple_tx(
                tx,
                id,
                target_id,
                matric_core::SkosSemanticRelation::Broader,
            )
            .await
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/v1/concepts/{id}/narrower/{target_id}", tag = "SKOS",
    params(("id" = Uuid, Path,), ("target_id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn remove_narrower(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            skos.delete_semantic_relation_by_triple_tx(
                tx,
                id,
                target_id,
                matric_core::SkosSemanticRelation::Narrower,
            )
            .await
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/v1/concepts/{id}/related/{target_id}", tag = "SKOS",
    params(("id" = Uuid, Path,), ("target_id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn remove_related(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            skos.delete_semantic_relation_by_triple_tx(
                tx,
                id,
                target_id,
                matric_core::SkosSemanticRelation::Related,
            )
            .await
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Tagging Handlers ---

#[utoipa::path(get, path = "/api/v1/notes/{id}/concepts", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_note_concepts(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let concepts = ctx
        .query(move |tx| Box::pin(async move { skos.get_note_tags_with_labels_tx(tx, id).await }))
        .await?;
    Ok(Json(concepts))
}

#[derive(Debug, Deserialize)]
struct TagNoteBody {
    concept_id: Uuid,
    is_primary: Option<bool>,
}

#[utoipa::path(post, path = "/api/v1/notes/{id}/concepts", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 201, description = "Success")))]
async fn tag_note_with_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<TagNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = matric_core::TagNoteRequest {
        note_id: id,
        concept_id: body.concept_id,
        source: "api".to_string(),
        confidence: None,
        relevance_score: 1.0,
        is_primary: body.is_primary.unwrap_or(false),
        created_by: None,
    };
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.tag_note_tx(tx, req).await }))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "success": true })),
    ))
}

#[utoipa::path(delete, path = "/api/v1/notes/{id}/concepts/{concept_id}", tag = "SKOS",
    params(("id" = Uuid, Path,), ("concept_id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn untag_note_concept(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((note_id, concept_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move { skos.untag_note_tx(tx, note_id, concept_id).await })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Governance Handlers ---

#[derive(Debug, Deserialize)]
struct GovernanceQuery {
    scheme_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/api/v1/concepts/governance", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn get_governance_stats(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<GovernanceQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Use default scheme if none provided
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let scheme_id_opt = query.scheme_id;
    let stats = ctx
        .query(move |tx| {
            Box::pin(async move {
                let scheme_id = match scheme_id_opt {
                    Some(id) => id,
                    None => skos.get_default_scheme_id_tx(tx).await?,
                };
                skos.get_governance_stats_tx(tx, scheme_id).await
            })
        })
        .await?;
    Ok(Json(stats))
}

/// Export a concept scheme in W3C SKOS Turtle format.
#[utoipa::path(get, path = "/api/v1/concepts/schemes/{id}/export/turtle", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn export_scheme_turtle(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    // Get the scheme and concepts in a single transaction
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let (scheme, concepts_resp) = ctx
        .query(move |tx| {
            Box::pin(async move {
                let scheme = skos.get_scheme_tx(tx, id).await?.ok_or_else(|| {
                    matric_core::Error::NotFound("Concept scheme not found".to_string())
                })?;

                let search_req = matric_core::SearchConceptsRequest {
                    scheme_id: Some(id),
                    limit: matric_core::defaults::INTERNAL_FETCH_LIMIT,
                    ..Default::default()
                };
                let concepts_resp = skos.search_concepts_tx(tx, search_req).await?;

                Ok::<_, matric_core::Error>((scheme, concepts_resp))
            })
        })
        .await?;

    // Build Turtle output
    let mut turtle = String::new();

    // Prefixes
    turtle.push_str("@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n");
    turtle.push_str("@prefix dct: <http://purl.org/dc/terms/> .\n");
    turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
    turtle.push_str(&format!(
        "@prefix : <urn:matric:scheme:{}:> .\n\n",
        scheme.notation
    ));

    // Scheme definition
    turtle.push_str(&format!(
        ":scheme a skos:ConceptScheme ;\n    dct:title \"{}\"@en",
        escape_turtle(&scheme.title)
    ));
    if let Some(desc) = &scheme.description {
        turtle.push_str(&format!(
            " ;\n    dct:description \"{}\"@en",
            escape_turtle(desc)
        ));
    }
    turtle.push_str(" .\n\n");

    // Concepts
    for concept in &concepts_resp.concepts {
        let id_str = concept.concept.id.to_string();
        let notation = concept.concept.notation.as_deref().unwrap_or(&id_str);
        let concept_uri = format!(":{}", notation);

        turtle.push_str(&format!("{} a skos:Concept", concept_uri));
        turtle.push_str(" ;\n    skos:inScheme :scheme");

        // Preferred label
        if let Some(pref) = &concept.pref_label {
            turtle.push_str(&format!(
                " ;\n    skos:prefLabel \"{}\"@{}",
                escape_turtle(pref),
                concept.label_language.as_deref().unwrap_or("en")
            ));
        }

        // Status note
        turtle.push_str(&format!(
            " ;\n    skos:note \"status: {:?}\"@en",
            concept.concept.status
        ));

        // PMEST facets as notes
        if let Some(facet) = &concept.concept.facet_domain {
            turtle.push_str(&format!(
                " ;\n    skos:note \"domain: {}\"@en",
                escape_turtle(facet)
            ));
        }

        turtle.push_str(" .\n\n");
    }

    // Return with proper content type
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/turtle; charset=utf-8",
        )],
        turtle,
    ))
}

/// Export all concept schemes as a single Turtle document.
#[utoipa::path(get, path = "/api/v1/concepts/schemes/export/turtle", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn export_all_schemes_turtle(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    // Get all schemes and their concepts in a single transaction
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());

    let schemes_with_concepts = ctx
        .query(move |tx| {
            Box::pin(async move {
                let schemes = skos.list_schemes_tx(tx, false).await?;
                let mut result = Vec::new();

                for scheme in schemes {
                    let search_req = matric_core::SearchConceptsRequest {
                        scheme_id: Some(scheme.id),
                        limit: matric_core::defaults::INTERNAL_FETCH_LIMIT,
                        ..Default::default()
                    };
                    let concepts_resp = skos.search_concepts_tx(tx, search_req).await?;
                    result.push((scheme, concepts_resp));
                }

                Ok::<_, matric_core::Error>(result)
            })
        })
        .await?;

    let mut turtle = String::new();

    // Common prefixes (once at the top)
    turtle.push_str("@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n");
    turtle.push_str("@prefix dct: <http://purl.org/dc/terms/> .\n");
    turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

    for (scheme, _) in &schemes_with_concepts {
        let prefix = format!("s_{}", scheme.notation.replace('-', "_"));
        turtle.push_str(&format!(
            "@prefix {}: <urn:matric:scheme:{}:> .\n",
            prefix, scheme.notation
        ));
    }
    turtle.push('\n');

    for (scheme, concepts_resp) in &schemes_with_concepts {
        let prefix = format!("s_{}", scheme.notation.replace('-', "_"));

        // Scheme definition
        turtle.push_str(&format!(
            "{}:scheme a skos:ConceptScheme ;\n    dct:title \"{}\"@en",
            prefix,
            escape_turtle(&scheme.title)
        ));
        if let Some(desc) = &scheme.description {
            turtle.push_str(&format!(
                " ;\n    dct:description \"{}\"@en",
                escape_turtle(desc)
            ));
        }
        turtle.push_str(" .\n\n");

        // Concepts already fetched in transaction

        for concept in &concepts_resp.concepts {
            let id_str = concept.concept.id.to_string();
            let notation = concept.concept.notation.as_deref().unwrap_or(&id_str);
            let concept_uri = format!("{}:{}", prefix, notation);

            turtle.push_str(&format!("{} a skos:Concept", concept_uri));
            turtle.push_str(&format!(" ;\n    skos:inScheme {}:scheme", prefix));

            if let Some(pref) = &concept.pref_label {
                turtle.push_str(&format!(
                    " ;\n    skos:prefLabel \"{}\"@{}",
                    escape_turtle(pref),
                    concept.label_language.as_deref().unwrap_or("en")
                ));
            }

            turtle.push_str(&format!(
                " ;\n    skos:note \"status: {:?}\"@en",
                concept.concept.status
            ));

            turtle.push_str(" .\n\n");
        }
    }

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/turtle; charset=utf-8",
        )],
        turtle,
    ))
}

/// Escape special characters for Turtle string literals.
fn escape_turtle(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// =============================================================================
// SKOS COLLECTION HANDLERS (W3C SKOS Section 9)
// =============================================================================

#[derive(Debug, Deserialize)]
struct ListSkosCollectionsQuery {
    scheme_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/api/v1/concepts/collections", tag = "SKOS",
    responses((status = 200, description = "Success")))]
async fn list_skos_collections(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<ListSkosCollectionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let scheme_id = query.scheme_id;
    let collections = ctx
        .query(move |tx| Box::pin(async move { skos.list_collections_tx(tx, scheme_id).await }))
        .await?;
    Ok(Json(collections))
}

#[utoipa::path(post, path = "/api/v1/concepts/collections", tag = "SKOS",
    request_body = matric_core::CreateSkosCollectionRequest,
    responses((status = 201, description = "Success")))]
async fn create_skos_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<matric_core::CreateSkosCollectionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let id = ctx
        .execute(move |tx| Box::pin(async move { skos.create_collection_tx(tx, body).await }))
        .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[utoipa::path(get, path = "/api/v1/concepts/collections/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_skos_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    let collection = ctx
        .query(move |tx| Box::pin(async move { skos.get_collection_with_members_tx(tx, id).await }))
        .await?
        .ok_or_else(|| ApiError::NotFound("SKOS collection not found".to_string()))?;
    Ok(Json(collection))
}

#[utoipa::path(patch, path = "/api/v1/concepts/collections/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    request_body = matric_core::UpdateSkosCollectionRequest,
    responses((status = 204, description = "Success")))]
async fn update_skos_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<matric_core::UpdateSkosCollectionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.update_collection_tx(tx, id, body).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/v1/concepts/collections/{id}", tag = "SKOS",
    params(("id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn delete_skos_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { skos.delete_collection_tx(tx, id).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(put, path = "/api/v1/concepts/collections/{id}/members", tag = "SKOS",
    params(("id" = Uuid, Path, description = "Collection ID")),
    request_body = matric_core::UpdateCollectionMembersRequest,
    responses((status = 204, description = "Success"))
)]
async fn replace_skos_collection_members(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<matric_core::UpdateCollectionMembersRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move { skos.replace_collection_members_tx(tx, id, body).await })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddMemberBody {
    position: Option<i32>,
}

#[utoipa::path(post, path = "/api/v1/concepts/collections/{collection_id}/members/{concept_id}", tag = "SKOS",
    params(
        ("collection_id" = Uuid, Path, description = "Collection ID"),
        ("concept_id" = Uuid, Path, description = "Concept ID"),
    ),
    responses((status = 201, description = "Created"))
)]
async fn add_skos_collection_member(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((collection_id, concept_id)): Path<(Uuid, Uuid)>,
    body: Option<Json<AddMemberBody>>,
) -> Result<impl IntoResponse, ApiError> {
    let position = body.and_then(|b| b.position);
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            skos.add_collection_member_tx(tx, collection_id, concept_id, position)
                .await
        })
    })
    .await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(delete, path = "/api/v1/concepts/collections/{collection_id}/members/{concept_id}", tag = "SKOS",
    params(
        ("collection_id" = Uuid, Path, description = "Collection ID"),
        ("concept_id" = Uuid, Path, description = "Concept ID"),
    ),
    responses((status = 204, description = "Success"))
)]
async fn remove_skos_collection_member(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((collection_id, concept_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let skos = matric_db::PgSkosRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            skos.remove_collection_member_tx(tx, collection_id, concept_id)
                .await
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// COLLECTION HANDLERS
// =============================================================================

use matric_db::CollectionRepository;

#[derive(Debug, Deserialize)]
struct ListCollectionsQuery {
    parent_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/api/v1/collections", tag = "Collections",
    responses((status = 200, description = "Success")))]
async fn list_collections(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<ListCollectionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let parent_id = query.parent_id;
    let collections = ctx
        .query(move |tx| Box::pin(async move { repo.list_tx(tx, parent_id).await }))
        .await?;
    Ok(Json(collections))
}

#[derive(Debug, Deserialize)]
struct CreateCollectionBody {
    name: String,
    description: Option<String>,
    parent_id: Option<Uuid>,
}

#[utoipa::path(post, path = "/api/v1/collections", tag = "Collections",
    responses((status = 201, description = "Success")))]
async fn create_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<CreateCollectionBody>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let collection = ctx
        .query(move |tx| {
            Box::pin(async move {
                let id = repo
                    .create_tx(tx, &body.name, body.description.as_deref(), body.parent_id)
                    .await?;
                repo.get_tx(tx, id)
                    .await?
                    .ok_or_else(|| matric_core::Error::NotFound("Collection not found".into()))
            })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(collection)))
}

#[utoipa::path(get, path = "/api/v1/collections/{id}", tag = "Collections",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let collection = ctx
        .query(move |tx| Box::pin(async move { repo.get_tx(tx, id).await }))
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Collection {} not found", id)))?;
    Ok(Json(collection))
}

#[derive(Debug, Deserialize)]
struct UpdateCollectionBody {
    name: String,
    description: Option<String>,
}

#[utoipa::path(patch, path = "/api/v1/collections/{id}", tag = "Collections",
    params(("id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn update_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCollectionBody>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| {
        Box::pin(async move {
            repo.update_tx(tx, id, &body.name, body.description.as_deref())
                .await
        })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/v1/collections/{id}", tag = "Collections",
    params(("id" = Uuid, Path,), ("force" = bool, Query, description = "Force delete non-empty collection")),
    responses(
        (status = 204, description = "Success"),
        (status = 409, description = "Collection is not empty (use force=true to delete anyway)")
    )
)]
async fn delete_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteCollectionQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let force = query.force;
    ctx.execute(move |tx| {
        Box::pin(async move {
            if !force {
                let count = repo.count_notes_tx(tx, id).await?;
                if count > 0 {
                    return Err(matric_core::Error::InvalidInput(format!(
                        "Collection contains {} note(s). Use force=true to delete anyway.",
                        count
                    )));
                }
            }
            repo.delete_tx(tx, id).await
        })
    })
    .await
    .map_err(|err| match &err {
        matric_core::Error::InvalidInput(msg) if msg.contains("force=true") => {
            ApiError::Conflict(msg.clone())
        }
        _ => ApiError::from(err),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct DeleteCollectionQuery {
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize)]
struct CollectionNotesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[utoipa::path(get, path = "/api/v1/collections/{id}/notes", tag = "Collections",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn get_collection_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<CollectionNotesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let limit = query.limit.unwrap_or(matric_core::defaults::PAGE_LIMIT);
    let offset = query.offset.unwrap_or(matric_core::defaults::PAGE_OFFSET);
    let notes = ctx
        .query(move |tx| Box::pin(async move { repo.get_notes_tx(tx, id, limit, offset).await }))
        .await?;
    Ok(Json(
        serde_json::json!({ "notes": notes, "collection_id": id }),
    ))
}

/// Export all notes in a collection as markdown with YAML frontmatter.
#[utoipa::path(get, path = "/api/v1/collections/{id}/export", tag = "Collections",
    params(("id" = Uuid, Path,)),
    responses((status = 200, description = "Success")))]
async fn export_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(collection_id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;

    // Fetch all notes in this collection
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let notes_in_collection = ctx
        .query(move |tx| {
            Box::pin(async move { repo.get_notes_tx(tx, collection_id, 10000, 0).await })
        })
        .await?;

    let mut output = String::new();

    for note_summary in &notes_in_collection {
        let note_id = note_summary.id;
        let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
        let tag_repo = matric_db::PgTagRepository::new(state.db.pool.clone());
        let result = ctx
            .query(move |tx| {
                Box::pin(async move {
                    let note = notes.fetch_tx(tx, note_id).await?;
                    let tags = tag_repo.get_for_note_tx(tx, note_id).await?;
                    Ok((note, tags))
                })
            })
            .await;

        if let Ok((note_full, tags)) = result {
            // Add YAML frontmatter
            if query.include_frontmatter {
                output.push_str("---\n");
                output.push_str(&format!("id: {}\n", note_full.note.id));
                if let Some(ref title) = note_full.note.title {
                    let escaped_title = title.replace('\"', "\\\"");
                    output.push_str(&format!("title: \"{}\"\n", escaped_title));
                }
                output.push_str(&format!(
                    "created: {}\n",
                    note_full.note.created_at_utc.to_rfc3339()
                ));
                output.push_str(&format!(
                    "updated: {}\n",
                    note_full.note.updated_at_utc.to_rfc3339()
                ));
                if !tags.is_empty() {
                    output.push_str("tags:\n");
                    for tag in &tags {
                        output.push_str(&format!("  - {}\n", tag));
                    }
                }
                output.push_str("---\n\n");
            }

            let use_original = query.content.as_deref() == Some("original");
            let content = if use_original || note_full.revised.content.is_empty() {
                &note_full.original.content
            } else {
                &note_full.revised.content
            };
            output.push_str(content);
            output.push_str("\n\n---\n\n"); // separator between notes
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/markdown; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"collection-{}.md\"", collection_id)
            .parse()
            .unwrap(),
    );

    Ok((StatusCode::OK, headers, output))
}

#[derive(Debug, Deserialize)]
struct MoveNoteBody {
    collection_id: Option<Uuid>,
}

#[utoipa::path(post, path = "/api/v1/notes/{id}/move", tag = "Collections",
    params(("id" = Uuid, Path,)),
    responses((status = 204, description = "Success")))]
async fn move_note_to_collection(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(note_id): Path<Uuid>,
    Json(body): Json<MoveNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgCollectionRepository::new(state.db.pool.clone());
    let collection_id = body.collection_id;
    ctx.execute(move |tx| {
        Box::pin(async move { repo.move_note_tx(tx, note_id, collection_id).await })
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// GRAPH EXPLORATION HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct GraphQuery {
    /// Maximum depth to traverse (default: 2)
    #[serde(default = "default_depth")]
    depth: i32,
    /// Maximum nodes to return (default: 50)
    #[serde(default = "default_max_nodes")]
    max_nodes: i64,
}

fn default_depth() -> i32 {
    2
}

fn default_max_nodes() -> i64 {
    50
}

#[utoipa::path(get, path = "/api/v1/graph/{id}", tag = "Graph",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn explore_graph(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<GraphQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;

    // Verify starting note exists (fixes #388)
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let exists = ctx
        .query(move |tx| Box::pin(async move { notes.exists_tx(tx, id).await }))
        .await?;
    if !exists {
        return Err(ApiError::NotFound(format!("Note {} not found", id)));
    }

    let links = matric_db::PgLinkRepository::new(state.db.pool.clone());
    // Clamp resource limits to prevent DoS (fixes #218)
    let depth = query.depth.clamp(0, 10);
    let max_nodes = query.max_nodes.clamp(1, 1000);
    let result = ctx
        .query(move |tx| {
            Box::pin(async move { links.traverse_graph_tx(tx, id, depth, max_nodes).await })
        })
        .await?;
    Ok(Json(result))
}

/// Returns graph topology statistics including degree distribution, connected
/// components, isolated nodes, and current linking strategy configuration.
///
/// Response fields: total_notes, total_links, isolated_nodes, connected_components,
/// avg_degree, max_degree, min_degree_linked, median_degree, linking_strategy, effective_k.
#[utoipa::path(get, path = "/api/v1/graph/topology/stats", tag = "Graph",
    responses(
        (status = 200, description = "Graph topology statistics"),
        (status = 500, description = "Internal server error")
    ))]
async fn graph_topology_stats(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let links = matric_db::PgLinkRepository::new(state.db.pool.clone());
    let result = ctx
        .query(move |tx| Box::pin(async move { links.topology_stats_tx(tx).await }))
        .await?;
    Ok(Json(result))
}

// =============================================================================
// TEMPLATE HANDLERS
// =============================================================================

#[utoipa::path(get, path = "/api/v1/templates", tag = "Templates",
    responses((status = 200, description = "Success")))]
async fn list_templates(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    let result = ctx
        .query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
        .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct CreateTemplateBody {
    name: String,
    description: Option<String>,
    content: String,
    format: Option<String>,
    default_tags: Option<Vec<String>>,
    collection_id: Option<Uuid>,
}

#[utoipa::path(post, path = "/api/v1/templates", tag = "Templates",
    responses((status = 201, description = "Success")))]
async fn create_template(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<CreateTemplateBody>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::CreateTemplateRequest;

    // Validate template name (fixes #204)
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Template name is required".to_string(),
        ));
    }

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    let req = CreateTemplateRequest {
        name: body.name,
        description: body.description,
        content: body.content,
        format: body.format,
        default_tags: body.default_tags,
        collection_id: body.collection_id,
    };
    let id = ctx
        .execute(move |tx| Box::pin(async move { templates.create_tx(tx, req).await }))
        .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[utoipa::path(get, path = "/api/v1/templates/{id}", tag = "Templates",
    params(("id" = Uuid, Path, description = "Template ID")),
    responses((status = 200, description = "Success")))]
async fn get_template(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    let template = ctx
        .query(move |tx| Box::pin(async move { templates.get_tx(tx, id).await }))
        .await?
        .ok_or_else(|| matric_core::Error::NotFound(format!("Template {} not found", id)))?;

    Ok(Json(template))
}

#[derive(Debug, Deserialize)]
struct UpdateTemplateBody {
    name: Option<String>,
    description: Option<String>,
    content: Option<String>,
    default_tags: Option<Vec<String>>,
    collection_id: Option<Option<Uuid>>,
}

#[utoipa::path(patch, path = "/api/v1/templates/{id}", tag = "Templates",
    params(("id" = Uuid, Path, description = "Template ID")),
    responses((status = 204, description = "Success")))]
async fn update_template(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTemplateBody>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::UpdateTemplateRequest;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    let req = UpdateTemplateRequest {
        name: body.name,
        description: body.description,
        content: body.content,
        default_tags: body.default_tags,
        collection_id: body.collection_id,
    };
    ctx.execute(move |tx| Box::pin(async move { templates.update_tx(tx, id, req).await }))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/v1/templates/{id}", tag = "Templates",
    params(("id" = Uuid, Path, description = "Template ID")),
    responses((status = 204, description = "Success")))]
async fn delete_template(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { templates.delete_tx(tx, id).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct InstantiateTemplateBody {
    /// Variables to substitute in the template (placeholder -> value)
    #[serde(default)]
    variables: std::collections::HashMap<String, String>,
    /// Additional tags to merge with template default_tags
    tags: Option<Vec<String>>,
    /// Override default collection
    collection_id: Option<Uuid>,
    /// AI revision mode: "full" (default), "light", or "none"
    #[serde(default)]
    revision_mode: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/templates/{id}/instantiate", tag = "Templates",
    params(("id" = Uuid, Path, description = "Template ID")),
    responses((status = 201, description = "Success")))]
async fn instantiate_template(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<InstantiateTemplateBody>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::CreateNoteRequest;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
    let template_id = id;

    // Get the template
    let template = ctx
        .query(move |tx| Box::pin(async move { templates.get_tx(tx, template_id).await }))
        .await?
        .ok_or_else(|| matric_core::Error::NotFound(format!("Template {} not found", id)))?;

    // Substitute variables in the content
    let mut content = template.content.clone();
    for (key, value) in &body.variables {
        content = content.replace(&format!("{{{{{}}}}}", key), value);
    }

    // Merge provided tags with template defaults (deduplicated)
    let tags = match (body.tags, template.default_tags.is_empty()) {
        (Some(provided), false) => {
            let mut merged = template.default_tags.clone();
            for tag in provided {
                if !merged.contains(&tag) {
                    merged.push(tag);
                }
            }
            Some(merged)
        }
        (Some(provided), true) => Some(provided),
        (None, false) => Some(template.default_tags.clone()),
        (None, true) => None,
    };

    // Use provided collection_id or template default
    let collection_id = body.collection_id.or(template.collection_id);

    // Create the note
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let create_req = CreateNoteRequest {
        content,
        format: template.format.clone(),
        source: "template".to_string(),
        collection_id,
        tags,
        metadata: None,
        document_type_id: None,
    };
    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, create_req).await }))
        .await?;

    // Parse and queue NLP pipeline
    let revision_mode = match body.revision_mode.as_deref() {
        Some("none") => RevisionMode::None,
        Some("full") => RevisionMode::Full,
        _ => RevisionMode::Light,
    };
    // Determine schema for background jobs (Issue #413)
    let schema_for_jobs = if archive_ctx.schema != "public" {
        Some(archive_ctx.schema.as_str())
    } else {
        None
    };
    queue_nlp_pipeline(
        &state.db,
        note_id,
        revision_mode,
        &state.event_bus,
        schema_for_jobs,
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": note_id })),
    ))
}

// =============================================================================
// LINK HANDLERS
// =============================================================================

#[derive(Debug, Serialize)]
struct NoteLinksResponse {
    outgoing: Vec<matric_core::Link>,
    incoming: Vec<matric_core::Link>,
}

#[utoipa::path(get, path = "/api/v1/notes/{id}/links", tag = "Graph",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn get_note_links(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let pool = state.db.pool.clone();
    let note_id = id;

    // Verify note exists (fixes #388)
    let notes = matric_db::PgNoteRepository::new(pool.clone());
    let exists = ctx
        .query(move |tx| Box::pin(async move { notes.exists_tx(tx, note_id).await }))
        .await?;
    if !exists {
        return Err(ApiError::NotFound(format!("Note {} not found", note_id)));
    }

    let outgoing = {
        let links = matric_db::PgLinkRepository::new(pool.clone());
        ctx.query(move |tx| Box::pin(async move { links.get_outgoing_tx(tx, note_id).await }))
            .await?
    };

    let incoming = {
        let links = matric_db::PgLinkRepository::new(pool.clone());
        ctx.query(move |tx| Box::pin(async move { links.get_incoming_tx(tx, note_id).await }))
            .await?
    };

    Ok(Json(NoteLinksResponse { outgoing, incoming }))
}

/// Get backlinks (notes that link TO this note).
#[utoipa::path(get, path = "/api/v1/notes/{id}/backlinks", tag = "Graph",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn get_note_backlinks(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;

    // Verify note exists (fixes #388)
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let exists = ctx
        .query(move |tx| Box::pin(async move { notes.exists_tx(tx, id).await }))
        .await?;
    if !exists {
        return Err(ApiError::NotFound(format!("Note {} not found", id)));
    }

    let links = matric_db::PgLinkRepository::new(state.db.pool.clone());
    let backlinks = ctx
        .query(move |tx| Box::pin(async move { links.get_incoming_tx(tx, id).await }))
        .await?;
    Ok(Json(serde_json::json!({
        "note_id": id,
        "backlinks": backlinks,
        "count": backlinks.len()
    })))
}

// =============================================================================
// PROVENANCE HANDLERS (W3C PROV)
// =============================================================================

/// Get W3C PROV provenance chain for a note's AI revisions.
///
/// Returns the full provenance graph including:
/// - Activities (AI processing operations)
/// - Edges (derivation relationships to source notes)
#[utoipa::path(get, path = "/api/v1/notes/{id}/provenance", tag = "Provenance",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn get_note_provenance(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let pool = state.db.pool.clone();
    let note_id = id;

    let chain = {
        let prov = matric_db::PgProvenanceRepository::new(pool.clone());
        ctx.query(move |tx| Box::pin(async move { prov.get_chain_tx(tx, note_id).await }))
            .await?
    };

    let activities = {
        let prov = matric_db::PgProvenanceRepository::new(pool.clone());
        ctx.query(move |tx| {
            Box::pin(async move { prov.get_activities_for_note_tx(tx, note_id).await })
        })
        .await?
    };

    let edges = {
        let prov = matric_db::PgProvenanceRepository::new(pool.clone());
        ctx.query(move |tx| Box::pin(async move { prov.get_edges_for_note_tx(tx, note_id).await }))
            .await?
    };

    let derived = {
        let prov = matric_db::PgProvenanceRepository::new(pool.clone());
        ctx.query(move |tx| Box::pin(async move { prov.get_derived_notes_tx(tx, note_id).await }))
            .await?
    };

    Ok(Json(serde_json::json!({
        "note_id": id,
        "current_chain": chain,
        "all_activities": activities,
        "all_edges": edges,
        "derived_notes": derived,
        "derived_count": derived.len()
    })))
}

// =============================================================================
// MEMORY SEARCH HANDLERS (Spatial/Temporal Provenance)
// =============================================================================

#[derive(Debug, Deserialize)]
struct MemorySearchQuery {
    /// Latitude in decimal degrees (-90 to 90)
    lat: Option<f64>,
    /// Longitude in decimal degrees (-180 to 180)
    lon: Option<f64>,
    /// Search radius in meters (default: 1000)
    radius: Option<f64>,
    /// Start of time range (ISO 8601)
    start: Option<crate::query_types::FlexibleDateTime>,
    /// End of time range (ISO 8601)
    end: Option<crate::query_types::FlexibleDateTime>,
}

/// Search memories by location, time, or both.
///
/// Query parameter combinations:
/// - `lat` + `lon` + `radius` → spatial search (nearest memories)
/// - `start` + `end` → temporal search (memories in time range)
/// - All five → combined spatial + temporal search
#[utoipa::path(get, path = "/api/v1/memories/search", tag = "Search",
    responses((status = 200, description = "Success")))]
async fn search_memories(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<MemorySearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let has_location = query.lat.is_some() && query.lon.is_some();
    let has_time = query.start.is_some() && query.end.is_some();

    if !has_location && !has_time {
        return Err(ApiError::BadRequest(
            "At least one search dimension required. Provide lat+lon for spatial search, \
             start+end for temporal search, or all for combined search."
                .to_string(),
        ));
    }

    // Validate coordinate bounds (issue #148)
    if let Some(lat) = query.lat {
        if !(-90.0..=90.0).contains(&lat) {
            return Err(ApiError::BadRequest(format!(
                "Latitude must be between -90 and 90, got {lat}"
            )));
        }
    }
    if let Some(lon) = query.lon {
        if !(-180.0..=180.0).contains(&lon) {
            return Err(ApiError::BadRequest(format!(
                "Longitude must be between -180 and 180, got {lon}"
            )));
        }
    }
    if let Some(radius) = query.radius {
        if radius <= 0.0 {
            return Err(ApiError::BadRequest(format!(
                "Radius must be positive, got {radius}"
            )));
        }
    }
    // Return empty results for inverted time ranges (issue #296)
    if has_time {
        let start_dt = query.start.as_ref().unwrap().0;
        let end_dt = query.end.as_ref().unwrap().0;
        if end_dt < start_dt {
            let mode = if has_location { "combined" } else { "time" };
            return Ok(Json(serde_json::json!({
                "mode": mode,
                "results": [],
                "count": 0
            })));
        }
    }

    if has_location && has_time {
        // Combined spatial + temporal search
        let lat = query.lat.unwrap();
        let lon = query.lon.unwrap();
        let radius = query.radius.unwrap_or(1000.0);
        let start = query.start.unwrap().into_inner();
        let end = query.end.unwrap().into_inner();

        let ctx = state.db.for_schema(&archive_ctx.schema)?;
        let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
        let results = ctx
            .query(move |tx| {
                Box::pin(async move {
                    memory_search
                        .search_by_location_and_time_tx(tx, lat, lon, radius, start, end)
                        .await
                })
            })
            .await?;

        Ok(Json(serde_json::json!({
            "mode": "combined",
            "results": results,
            "count": results.len()
        })))
    } else if has_location {
        // Spatial-only search
        let lat = query.lat.unwrap();
        let lon = query.lon.unwrap();
        let radius = query.radius.unwrap_or(1000.0);

        let ctx = state.db.for_schema(&archive_ctx.schema)?;
        let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
        let results = ctx
            .query(move |tx| {
                Box::pin(async move {
                    memory_search
                        .search_by_location_tx(tx, lat, lon, radius)
                        .await
                })
            })
            .await?;

        Ok(Json(serde_json::json!({
            "mode": "location",
            "results": results,
            "count": results.len()
        })))
    } else {
        // Temporal-only search
        let start = query.start.unwrap().into_inner();
        let end = query.end.unwrap().into_inner();

        let ctx = state.db.for_schema(&archive_ctx.schema)?;
        let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
        let results = ctx
            .query(move |tx| {
                Box::pin(async move { memory_search.search_by_timerange_tx(tx, start, end).await })
            })
            .await?;

        Ok(Json(serde_json::json!({
            "mode": "time",
            "results": results,
            "count": results.len()
        })))
    }
}

/// Get the complete file provenance chain for a note's attachments.
///
/// Returns temporal-spatial provenance including location, device, and
/// capture time information for all file attachments.
#[utoipa::path(get, path = "/api/v1/notes/{id}/memory-provenance", tag = "Provenance",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn get_memory_provenance_handler(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let provenance = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.get_memory_provenance_tx(tx, id).await })
        })
        .await?;

    match provenance {
        Some(prov) => Ok(Json(serde_json::json!(prov))),
        None => Ok(Json(serde_json::json!({
            "note_id": id,
            "files": []
        }))),
    }
}

// =============================================================================
// EXPORT HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct ExportQuery {
    /// Include YAML frontmatter with metadata (default: true)
    #[serde(default = "default_true")]
    include_frontmatter: bool,
    /// Content version: "revised" (default) or "original"
    #[serde(default)]
    content: Option<String>,
}

fn default_true() -> bool {
    true
}

#[utoipa::path(get, path = "/api/v1/notes/{id}/export", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn export_note(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let tag_repo = matric_db::PgTagRepository::new(state.db.pool.clone());
    let (note_full, tags) = ctx
        .query(move |tx| {
            Box::pin(async move {
                let note = notes.fetch_tx(tx, id).await?;
                let tags = tag_repo.get_for_note_tx(tx, id).await?;
                Ok((note, tags))
            })
        })
        .await?;

    let mut output = String::new();

    // Add YAML frontmatter if requested
    if query.include_frontmatter {
        output.push_str("---\n");
        output.push_str(&format!("id: {}\n", note_full.note.id));
        if let Some(ref title) = note_full.note.title {
            // Escape title for YAML
            let escaped_title = title.replace('\"', "\\\"");
            output.push_str(&format!("title: \"{}\"\n", escaped_title));
        }
        output.push_str(&format!(
            "created: {}\n",
            note_full.note.created_at_utc.to_rfc3339()
        ));
        output.push_str(&format!(
            "updated: {}\n",
            note_full.note.updated_at_utc.to_rfc3339()
        ));
        if note_full.note.starred {
            output.push_str("starred: true\n");
        }
        if note_full.note.archived {
            output.push_str("archived: true\n");
        }
        if !tags.is_empty() {
            output.push_str("tags:\n");
            for tag in &tags {
                output.push_str(&format!("  - {}\n", tag));
            }
        }
        if let Some(collection_id) = note_full.note.collection_id {
            output.push_str(&format!("collection_id: {}\n", collection_id));
        }
        output.push_str("---\n\n");
    }

    // Add content (revised by default, original if requested)
    let use_original = query.content.as_deref() == Some("original");
    let content = if use_original || note_full.revised.content.is_empty() {
        &note_full.original.content
    } else {
        &note_full.revised.content
    };
    output.push_str(content);

    // Return as markdown with appropriate headers
    let filename = note_full
        .note
        .title
        .as_ref()
        .map(|t| t.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_"))
        .unwrap_or_else(|| id.to_string());

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/markdown; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}.md\"", filename)
            .parse()
            .unwrap(),
    );

    Ok((StatusCode::OK, headers, output))
}

// =============================================================================
// DOCUMENT RECONSTRUCTION HANDLER (#111)
// =============================================================================

/// Get the full reconstructed document for a note (works with both chunked and regular notes).
///
/// For chunked documents, this endpoint:
/// - Identifies all chunks in the document chain
/// - Stitches them back together in order
/// - Removes overlaps between chunks
/// - Returns the full content with metadata about chunks
///
/// For regular notes, it simply returns the note content as-is.
#[utoipa::path(get, path = "/api/v1/notes/{id}/full", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn get_full_document(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    if archive_ctx.schema != "public" {
        return Err(ApiError::BadRequest(
            "Document reconstruction not yet supported for non-default archives".to_string(),
        ));
    }
    use matric_api::services::ReconstructionService;

    let reconstruction_service = ReconstructionService::new(state.db.clone());
    let full_document = reconstruction_service.get_full_document(id).await?;

    Ok(Json(full_document))
}

// =============================================================================
// NOTE VERSIONING HANDLERS (#104)
// =============================================================================

/// List all versions for a note (both original and revision tracks).
#[utoipa::path(get, path = "/api/v1/notes/{id}/versions", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn list_note_versions(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let notes = matric_db::PgNoteRepository::new(state.db.pool.clone());
    let versioning = matric_db::VersioningRepository::new(state.db.pool.clone());
    let versions = ctx
        .query(move |tx| {
            Box::pin(async move {
                let _ = notes.fetch_tx(tx, id).await?;
                versioning.list_versions_tx(tx, id).await
            })
        })
        .await?;

    Ok(Json(serde_json::json!({
        "note_id": versions.note_id,
        "current_original_version": versions.current_original_version,
        "current_revision_number": versions.current_revision_number,
        "original_versions": versions.original_versions.iter().map(|v| serde_json::json!({
            "version_number": v.version_number,
            "created_at_utc": v.created_at_utc.to_rfc3339(),
            "created_by": v.created_by,
            "is_current": v.is_current
        })).collect::<Vec<_>>(),
        "revised_versions": versions.revised_versions.iter().map(|v| serde_json::json!({
            "id": v.id,
            "revision_number": v.revision_number,
            "created_at_utc": v.created_at_utc.to_rfc3339(),
            "model": v.model,
            "is_user_edited": v.is_user_edited
        })).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct GetVersionQuery {
    /// Track to get version from: "original" or "revision" (default: "original")
    track: Option<String>,
}

/// Get a specific version of a note.
#[utoipa::path(get, path = "/api/v1/notes/{id}/versions/{version}", tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID"),
        ("version" = i32, Path, description = "Version number")
    ),
    responses((status = 200, description = "Success")))]
async fn get_note_version(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, version)): Path<(Uuid, i32)>,
    Query(query): Query<GetVersionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let track = query.track.as_deref().unwrap_or("original");
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::VersioningRepository::new(state.db.pool.clone());

    match track {
        "original" => {
            let version_data = ctx
                .query(move |tx| {
                    Box::pin(async move { repo.get_original_version_tx(tx, id, version).await })
                })
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Version {} not found", version)))?;

            Ok(Json(serde_json::json!({
                "track": "original",
                "id": version_data.id,
                "note_id": version_data.note_id,
                "version_number": version_data.version_number,
                "content": version_data.content,
                "hash": version_data.hash,
                "created_at_utc": version_data.created_at_utc.to_rfc3339(),
                "created_by": version_data.created_by
            })))
        }
        "revision" => {
            let revision = ctx
                .query(move |tx| {
                    Box::pin(async move { repo.get_revision_version_tx(tx, id, version).await })
                })
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Revision {} not found", version)))?;

            Ok(Json(serde_json::json!({
                "track": "revision",
                "id": revision.id,
                "note_id": revision.note_id,
                "revision_number": revision.revision_number,
                "content": revision.content,
                "type": revision.revision_type,
                "summary": revision.summary,
                "rationale": revision.rationale,
                "created_at_utc": revision.created_at_utc.to_rfc3339(),
                "model": revision.model,
                "is_user_edited": revision.is_user_edited
            })))
        }
        _ => Err(ApiError::BadRequest(
            "Invalid track. Use 'original' or 'revision'".to_string(),
        )),
    }
}

#[derive(Debug, Default, Deserialize)]
struct RestoreVersionRequest {
    /// Whether to restore tags from the version snapshot (default: false)
    #[serde(default)]
    restore_tags: bool,
}

/// Restore a previous version of a note.
#[utoipa::path(post, path = "/api/v1/notes/{id}/versions/{version}/restore", tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID"),
        ("version" = i32, Path, description = "Version number")
    ),
    responses((status = 200, description = "Success")))]
async fn restore_note_version(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, version)): Path<(Uuid, i32)>,
    body: Option<Json<RestoreVersionRequest>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request = body.map(|j| j.0).unwrap_or_default();
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::VersioningRepository::new(state.db.pool.clone());
    let restore_tags = request.restore_tags;
    let new_version = ctx
        .execute(move |tx| {
            Box::pin(async move {
                repo.restore_original_version_tx(tx, id, version, restore_tags)
                    .await
            })
        })
        .await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "restored_from_version": version,
        "new_version": new_version,
        "restore_tags": request.restore_tags
    })))
}

/// Delete a specific version from history.
#[utoipa::path(delete, path = "/api/v1/notes/{id}/versions/{version}", tag = "Notes",
    params(
        ("id" = Uuid, Path, description = "Note ID"),
        ("version" = i32, Path, description = "Version number")
    ),
    responses((status = 200, description = "Success")))]
async fn delete_note_version(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((id, version)): Path<(Uuid, i32)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::VersioningRepository::new(state.db.pool.clone());
    let deleted = ctx
        .execute(move |tx| Box::pin(async move { repo.delete_version_tx(tx, id, version).await }))
        .await?;

    if deleted {
        Ok(Json(serde_json::json!({
            "success": true,
            "deleted_version": version
        })))
    } else {
        Err(ApiError::NotFound(format!("Version {} not found", version)))
    }
}

#[derive(Debug, Deserialize)]
struct DiffVersionsQuery {
    /// Version to diff from
    #[serde(alias = "from_version")]
    from: i32,
    /// Version to diff to
    #[serde(alias = "to_version")]
    to: i32,
}

/// Generate a diff between two versions.
#[utoipa::path(get, path = "/api/v1/notes/{id}/versions/diff", tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn diff_note_versions(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<DiffVersionsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::VersioningRepository::new(state.db.pool.clone());
    let from = query.from;
    let to = query.to;
    let diff = ctx
        .query(move |tx| Box::pin(async move { repo.diff_versions_tx(tx, id, from, to).await }))
        .await?;

    // Return as plain text (unified diff format)
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().unwrap(),
    );

    Ok((StatusCode::OK, headers, diff))
}

// =============================================================================
// SEARCH HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<i64>,
    filters: Option<String>,
    mode: Option<String>,
    /// Embedding set slug to search within (default: "default")
    #[serde(rename = "set")]
    embedding_set: Option<String>,
    /// Filter: notes created after this timestamp (ISO 8601)
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp (ISO 8601)
    created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated after this timestamp (ISO 8601)
    updated_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated before this timestamp (ISO 8601)
    updated_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Relative time filter: "7d" (7 days), "1w" (1 week), "1m" (1 month), "2h" (2 hours)
    since: Option<String>,
    /// Filter by tags (comma-separated). Notes must have ALL specified tags.
    tags: Option<String>,
    /// Strict tag filter for SKOS-based filtering (JSON string).
    /// Example: {"required_tags":["tag1"],"excluded_tags":["tag2"]}
    #[serde(default)]
    strict_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchResponse {
    results: Vec<EnhancedSearchHit>,
    query: String,
    total: usize,
}

/// Get or create a `HybridSearchEngine` for the given schema.
///
/// For the `public` schema, returns the default engine from `AppState`.
/// For non-default archives, lazily creates a per-schema connection pool
/// with `search_path` pinned to that schema, caches the engine, and returns it.
async fn search_engine_for_schema(
    state: &AppState,
    schema: &str,
) -> Result<Arc<HybridSearchEngine>, ApiError> {
    if schema == "public" {
        return Ok(state.search.clone());
    }
    // Fast path: check read cache
    {
        let engines = state.schema_engines.read().await;
        if let Some(engine) = engines.get(schema) {
            return Ok(engine.clone());
        }
    }
    // Slow path: create pool + engine, insert into cache
    let pool = matric_db::pool::create_pool_for_schema(&state.database_url, schema)
        .await
        .map_err(|e| {
            ApiError::Internal(format!(
                "Failed to create pool for schema {}: {}",
                schema, e
            ))
        })?;
    let db = Database::new(pool);
    let engine = Arc::new(HybridSearchEngine::new(db));
    state
        .schema_engines
        .write()
        .await
        .insert(schema.to_string(), engine.clone());
    Ok(engine)
}

#[utoipa::path(get, path = "/api/v1/search", tag = "Search",
    responses((status = 200, description = "Success")))]
async fn search_notes(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_SEARCH);

    // Generate cache key (before expensive operations)
    // Only cache pure text queries without strict filters or temporal filters
    let cache_key = if query.strict_filter.is_none()
        && query.tags.is_none()
        && query.created_after.is_none()
        && query.created_before.is_none()
        && query.updated_after.is_none()
        && query.updated_before.is_none()
        && query.since.is_none()
    {
        Some(state.search_cache.cache_key(
            &format!("{}:{}", archive_ctx.schema, query.q),
            query.filters.as_ref().map(|f| vec![f.clone()]).as_deref(),
            None,
        ))
    } else {
        None
    };

    // Check cache first
    if let Some(ref key) = cache_key {
        if let Some(cached) = state.search_cache.get::<SearchResponse>(key).await {
            tracing::debug!("Search cache HIT for query: {}", query.q);
            return Ok(Json(cached));
        }
    }

    let mut config = match query.mode.as_deref() {
        Some("fts") => HybridSearchConfig::fts_only(),
        Some("semantic") => HybridSearchConfig::semantic_only(),
        _ => HybridSearchConfig::default(),
    };

    // Get or create a schema-scoped search engine
    let engine = search_engine_for_schema(&state, &archive_ctx.schema).await?;

    // Resolve strict filter if provided (parse JSON string)
    // Use a schema-scoped TagResolver for non-default archives
    if let Some(filter_json) = &query.strict_filter {
        let filter_input: StrictTagFilterInput = serde_json::from_str(filter_json)
            .map_err(|e| ApiError::BadRequest(format!("Invalid strict_filter JSON: {}", e)))?;
        let tag_resolver = if archive_ctx.schema == "public" {
            state.tag_resolver.clone()
        } else {
            TagResolver::new(Database::new(engine.db().pool().clone()))
        };
        let strict_filter = tag_resolver.resolve_filter(filter_input).await?;
        config.strict_filter = Some(strict_filter);
    }
    // Generate query embedding for semantic/hybrid search
    let query_embedding = if config.semantic_weight > 0.0 && !query.q.trim().is_empty() {
        let backend = OllamaBackend::from_env();
        backend
            .embed_texts(std::slice::from_ref(&query.q))
            .await
            .ok()
            .and_then(|vecs| vecs.into_iter().next())
    } else {
        None
    };

    // Parse relative time (e.g., "7d", "1w") and use as created_after if provided
    let created_after = query
        .created_after
        .or_else(|| query.since.as_ref().and_then(|s| parse_relative_time(s)));

    let mut request = SearchRequest::new(&query.q)
        .with_limit(limit)
        .with_config(config);

    // Merge tags param into filters (comma-separated tags → "tag:x tag:y" format)
    let merged_filters = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref filters) = query.filters {
            parts.push(filters.clone());
        }
        if let Some(ref tags_str) = query.tags {
            for tag in tags_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                parts.push(format!("tag:{}", tag));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    };

    if let Some(filters) = &merged_filters {
        request = request.with_filters(filters);
    }

    // Resolve embedding set slug to UUID and apply filter (return 404 if slug is invalid)
    // Use schema-scoped DB for non-default archives
    if let Some(ref set_slug) = query.embedding_set {
        let search_db = if archive_ctx.schema == "public" {
            &state.db
        } else {
            engine.db()
        };
        let set = search_db
            .embedding_sets
            .get_by_slug(set_slug)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Embedding set not found: {}", set_slug)))?;
        request = request.with_embedding_set(set.id);
    }

    if let Some(vec) = query_embedding {
        request = request.with_embedding(vec);
    }

    // Apply temporal filters
    if let Some(ts) = created_after {
        request = request.with_created_after(ts);
    }
    if let Some(ts) = query.created_before {
        request = request.with_created_before(ts);
    }
    if let Some(ts) = query.updated_after {
        request = request.with_updated_after(ts);
    }
    if let Some(ts) = query.updated_before {
        request = request.with_updated_before(ts);
    }

    let results = request.execute(&engine).await?;
    let total = results.len();

    let response = SearchResponse {
        results,
        query: query.q,
        total,
    };

    // Store in cache (non-blocking, fire-and-forget)
    if let Some(ref key) = cache_key {
        let cache = state.search_cache.clone();
        let key = key.clone();
        let resp = response.clone();
        tokio::spawn(async move {
            cache.set(&key, &resp).await;
        });
    }

    Ok(Json(response))
}

// =============================================================================
// FEDERATED SEARCH (Cross-memory, Issue #177)
// =============================================================================

/// Request body for cross-memory federated search.
#[derive(Debug, Deserialize)]
struct FederatedSearchRequest {
    /// Search query string
    q: String,
    /// Memory names to search across, or ["all"] for all memories
    memories: Vec<String>,
    /// Maximum results per memory (default 10)
    limit: Option<i64>,
}

/// A single federated search hit annotated with its source memory.
#[derive(Debug, Clone, Serialize)]
struct FederatedSearchHit {
    note_id: Uuid,
    score: f32,
    snippet: Option<String>,
    title: Option<String>,
    tags: Vec<String>,
    /// Which memory this result came from
    memory: String,
}

/// Response for federated search across memories.
#[derive(Debug, Serialize)]
struct FederatedSearchResponse {
    results: Vec<FederatedSearchHit>,
    query: String,
    total: usize,
    memories_searched: Vec<String>,
}

/// Search across multiple memories using FTS.
///
/// Runs a full-text search query against each specified memory schema
/// and merges the results, sorted by relevance score. Each result is
/// annotated with its source memory name.
///
/// # Request Body
/// - `q`: Search query string
/// - `memories`: Array of memory names, or `["all"]` for all memories
/// - `limit`: Max results per memory (default 10)
#[utoipa::path(post, path = "/api/v1/search/federated", tag = "Search",
    responses((status = 200, description = "Success")))]
async fn federated_search(
    State(state): State<AppState>,
    Extension(_archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<FederatedSearchRequest>,
) -> Result<Json<FederatedSearchResponse>, ApiError> {
    use matric_core::ArchiveRepository;
    use sqlx::Row;

    let limit = body.limit.unwrap_or(10);

    // Resolve which schemas to search
    let schemas: Vec<(String, String)> = if body.memories.len() == 1 && body.memories[0] == "all" {
        // Search all memories plus public — deduplicate by schema_name
        let mut seen = std::collections::HashSet::new();
        let mut schemas = Vec::new();
        // Always include public first
        seen.insert("public".to_string());
        schemas.push(("public".to_string(), "public".to_string()));
        let archives = state.db.archives.list_archive_schemas().await?;
        for a in archives {
            if seen.insert(a.schema_name.clone()) {
                schemas.push((a.name, a.schema_name));
            }
        }
        schemas
    } else {
        let mut seen = std::collections::HashSet::new();
        let mut schemas = Vec::new();
        for name in &body.memories {
            if name == "public" {
                if seen.insert("public".to_string()) {
                    schemas.push(("public".to_string(), "public".to_string()));
                }
            } else {
                let archive = state
                    .db
                    .archives
                    .get_archive_by_name(name)
                    .await?
                    .ok_or_else(|| ApiError::NotFound(format!("Memory not found: {}", name)))?;
                if seen.insert(archive.schema_name.clone()) {
                    schemas.push((archive.name, archive.schema_name));
                }
            }
        }
        schemas
    };

    let memories_searched: Vec<String> = schemas.iter().map(|(name, _)| name.clone()).collect();
    let mut all_results: Vec<FederatedSearchHit> = Vec::new();

    // Search each memory schema using FTS within a transaction
    for (memory_name, schema_name) in &schemas {
        let mut tx = state
            .db
            .pool()
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to begin transaction: {}", e)))?;

        // Set search_path for this schema
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", schema_name))
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to set search_path: {}", e)))?;

        // Run FTS query (uses matric_english to match the tsv generated column config)
        let rows = sqlx::query(
            r#"
            SELECT
                n.id AS note_id,
                ts_rank_cd(nrc.tsv, websearch_to_tsquery('public.matric_english', $1)) AS score,
                left(convert_from(nrc.content::bytea, 'UTF8'), 200) AS snippet,
                n.title,
                COALESCE(
                    (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                    ''
                ) AS tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE n.deleted_at IS NULL
                AND (nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
                     OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1))
            ORDER BY score DESC
            LIMIT $2
            "#,
        )
        .bind(&body.q)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| {
            ApiError::Internal(format!("Search failed in memory '{}': {}", memory_name, e))
        })?;

        // Transaction automatically rolled back (read-only, no commit needed)
        drop(tx);

        for row in rows {
            let tags_str: String = row.get("tags");
            let tags = if tags_str.is_empty() {
                Vec::new()
            } else {
                tags_str.split(',').map(|s| s.trim().to_string()).collect()
            };
            all_results.push(FederatedSearchHit {
                note_id: row.get("note_id"),
                score: row.get("score"),
                snippet: row.get("snippet"),
                title: row.get("title"),
                tags,
                memory: memory_name.clone(),
            });
        }
    }

    // Sort by score descending across all memories
    all_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to requested limit (limit was per-memory, apply globally too)
    all_results.truncate(limit as usize);
    let total = all_results.len();

    Ok(Json(FederatedSearchResponse {
        results: all_results,
        query: body.q,
        total,
        memories_searched,
    }))
}

// =============================================================================
// MEMORIES OVERVIEW
// =============================================================================

/// Per-memory statistics in the overview response.
#[derive(Debug, Serialize)]
struct MemoryBreakdown {
    name: String,
    note_count: i32,
    size_bytes: i64,
    is_default: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Aggregate overview of all running memories.
#[derive(Debug, Serialize)]
struct MemoriesOverviewResponse {
    /// Total number of memories (archives) currently active.
    memory_count: i64,
    /// Maximum allowed memories (from MAX_MEMORIES config).
    max_memories: i64,
    /// How many more memories can be created.
    remaining_slots: i64,
    /// Aggregate note count across all memories + public.
    total_notes: i64,
    /// Aggregate storage bytes across all memories + public (table-level).
    total_size_bytes: i64,
    /// Human-readable total size.
    total_size_human: String,
    /// Total database size on disk (pg_database_size).
    database_size_bytes: i64,
    /// Human-readable database size.
    database_size_human: String,
    /// Per-memory breakdown sorted by note count descending.
    memories: Vec<MemoryBreakdown>,
}

/// GET /api/v1/memories/overview
///
/// Returns aggregate stats across all running memories, showing total capacity
/// usage, remaining slots, and per-memory breakdown. This is the primary
/// tool for users/agents to understand system overhead and plan memory allocation.
#[utoipa::path(get, path = "/api/v1/memories/overview", tag = "System",
    responses((status = 200, description = "Success")))]
async fn memories_overview(
    State(state): State<AppState>,
) -> Result<Json<MemoriesOverviewResponse>, ApiError> {
    use matric_core::ArchiveRepository;

    let archives = state.db.archives.list_archive_schemas().await?;

    // Update stats for each archive (best-effort)
    for archive in &archives {
        let _ = state.db.archives.update_archive_stats(&archive.name).await;
    }

    // Re-fetch with updated stats
    let archives = state.db.archives.list_archive_schemas().await?;

    // Count notes in public schema
    let public_notes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.note WHERE soft_deleted = false")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0);

    let public_size: i64 = sqlx::query_scalar(
        "SELECT pg_total_relation_size('public.note'::regclass) + pg_total_relation_size('public.embedding'::regclass)",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    let mut total_notes: i64 = public_notes;
    let mut total_size: i64 = public_size;
    let mut breakdowns = vec![MemoryBreakdown {
        name: "public".to_string(),
        note_count: public_notes as i32,
        size_bytes: public_size,
        is_default: archives.iter().all(|a| !a.is_default),
        created_at: chrono::Utc::now(), // public has no creation time
    }];

    for archive in &archives {
        let notes = archive.note_count.unwrap_or(0);
        let size = archive.size_bytes.unwrap_or(0);
        total_notes += notes as i64;
        total_size += size;
        breakdowns.push(MemoryBreakdown {
            name: archive.name.clone(),
            note_count: notes,
            size_bytes: size,
            is_default: archive.is_default,
            created_at: archive.created_at,
        });
    }

    // Sort by note_count descending
    breakdowns.sort_by(|a, b| b.note_count.cmp(&a.note_count));

    let memory_count = archives.len() as i64;
    let remaining = (state.max_memories - memory_count).max(0);

    // Get total database size on disk
    let db_size: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())")
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

    Ok(Json(MemoriesOverviewResponse {
        memory_count,
        max_memories: state.max_memories,
        remaining_slots: remaining,
        total_notes,
        total_size_bytes: total_size,
        total_size_human: format_size(total_size as u64),
        database_size_bytes: db_size,
        database_size_human: format_size(db_size as u64),
        memories: breakdowns,
    }))
}

// =============================================================================
// EMBEDDING SET HANDLERS
// =============================================================================

use matric_core::{AddMembersRequest, CreateEmbeddingSetRequest, UpdateEmbeddingSetRequest};

/// List all embedding sets for discovery
#[utoipa::path(get, path = "/api/v1/embedding-sets", tag = "Embeddings",
    responses((status = 200, description = "Success")))]
async fn list_embedding_sets(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    let sets = ctx
        .query(move |tx| Box::pin(async move { repo.list_tx(tx).await }))
        .await?;
    Ok(Json(sets))
}

/// Get an embedding set by slug or ID
#[utoipa::path(get, path = "/api/v1/embedding-sets/{slug}", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 200, description = "Success")))]
async fn get_embedding_set(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Try to parse as UUID first, then fall back to slug
    let set = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        ctx.query(move |tx| Box::pin(async move { repo.get_by_id_tx(tx, id).await }))
            .await?
    } else {
        ctx.query(move |tx| Box::pin(async move { repo.get_by_slug_tx(tx, &slug_or_id).await }))
            .await?
    };
    let set = set.ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?;
    Ok(Json(set))
}

/// Create a new embedding set
#[utoipa::path(post, path = "/api/v1/embedding-sets", tag = "Embeddings",
    responses((status = 201, description = "Success")))]
async fn create_embedding_set(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<CreateEmbeddingSetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate truncate_dim is positive if provided
    if let Some(dim) = body.truncate_dim {
        if dim <= 0 {
            return Err(ApiError::BadRequest(
                "truncate_dim must be a positive integer".to_string(),
            ));
        }
    }
    // Validate embedding_config_id exists if provided
    if let Some(config_id) = body.embedding_config_id {
        let config = state.db.embedding_sets.get_config(config_id).await?;
        if config.is_none() {
            return Err(ApiError::BadRequest(format!(
                "embedding_config_id '{}' does not exist",
                config_id
            )));
        }
    }
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    let set = ctx
        .execute(move |tx| Box::pin(async move { repo.create_tx(tx, body).await }))
        .await?;
    Ok((StatusCode::CREATED, Json(set)))
}

/// Update an embedding set
#[utoipa::path(patch, path = "/api/v1/embedding-sets/{slug}", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 204, description = "Success")))]
async fn update_embedding_set(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
    Json(body): Json<UpdateEmbeddingSetRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to actual slug first (needed for update_tx which uses slug)
    let slug = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        let repo_clone = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { repo_clone.get_by_id_tx(tx, id).await }))
            .await?
            .ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?
            .slug
    } else {
        slug_or_id
    };
    let set = ctx
        .execute(move |tx| Box::pin(async move { repo.update_tx(tx, &slug, body).await }))
        .await?;
    Ok(Json(set))
}

/// Delete an embedding set (not allowed for system sets)
#[utoipa::path(delete, path = "/api/v1/embedding-sets/{slug}", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 204, description = "Success")))]
async fn delete_embedding_set(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to actual slug first (needed for delete_tx which uses slug)
    let slug = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        let repo_clone = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { repo_clone.get_by_id_tx(tx, id).await }))
            .await?
            .ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?
            .slug
    } else {
        slug_or_id
    };
    ctx.execute(move |tx| Box::pin(async move { repo.delete_tx(tx, &slug).await }))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ListMembersQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// List members of an embedding set
#[utoipa::path(get, path = "/api/v1/embedding-sets/{slug}/members", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 200, description = "Success")))]
async fn list_embedding_set_members(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
    Query(query): Query<ListMembersQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_LARGE);
    let offset = query.offset.unwrap_or(matric_core::defaults::PAGE_OFFSET);
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to actual slug first
    let slug = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        let repo_clone = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { repo_clone.get_by_id_tx(tx, id).await }))
            .await?
            .ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?
            .slug
    } else {
        slug_or_id
    };
    let members = ctx
        .query(move |tx| {
            Box::pin(async move { repo.list_members_tx(tx, &slug, limit, offset).await })
        })
        .await?;
    Ok(Json(members))
}

/// Add notes to an embedding set
#[utoipa::path(post, path = "/api/v1/embedding-sets/{slug}/members", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 201, description = "Success")))]
async fn add_embedding_set_members(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
    Json(body): Json<AddMembersRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to actual slug first
    let slug = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        let repo_clone = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { repo_clone.get_by_id_tx(tx, id).await }))
            .await?
            .ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?
            .slug
    } else {
        slug_or_id
    };
    let note_ids = body.note_ids.clone();
    let slug_for_resolve = slug.clone();
    let count = ctx
        .execute(move |tx| Box::pin(async move { repo.add_members_tx(tx, &slug, body).await }))
        .await?;

    // Resolve set ID and queue embedding jobs for each added note
    let resolve_repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    if let Some(set) = resolve_repo.get_by_slug(&slug_for_resolve).await? {
        for note_id in &note_ids {
            let payload = serde_json::json!({ "embedding_set_id": set.id.to_string() });
            let _ = state
                .db
                .jobs
                .queue(
                    Some(*note_id),
                    matric_core::JobType::Embedding,
                    matric_core::JobType::Embedding.default_priority(),
                    Some(payload),
                )
                .await;
        }
    }

    Ok(Json(serde_json::json!({ "added": count })))
}

/// Remove a note from an embedding set
#[utoipa::path(delete, path = "/api/v1/embedding-sets/{slug}/members/{note_id}", tag = "Embeddings",
    params(
        ("slug" = String, Path, description = "Embedding set slug or UUID"),
        ("note_id" = Uuid, Path, description = "Note ID")
    ),
    responses((status = 204, description = "Success")))]
async fn remove_embedding_set_member(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((slug_or_id, note_id)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to actual slug first
    let slug = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        let repo_clone = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { repo_clone.get_by_id_tx(tx, id).await }))
            .await?
            .ok_or_else(|| ApiError::NotFound("Embedding set not found".to_string()))?
            .slug
    } else {
        slug_or_id
    };
    let removed = ctx
        .execute(move |tx| Box::pin(async move { repo.remove_member_tx(tx, &slug, note_id).await }))
        .await?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(
            "Note not found in embedding set".to_string(),
        ))
    }
}

/// Refresh an embedding set by re-evaluating criteria
#[utoipa::path(post, path = "/api/v1/embedding-sets/{slug}/refresh", tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 200, description = "Success")))]
async fn refresh_embedding_set(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // First check the mode within the archive schema
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    // Resolve to embedding set by ID or slug
    let set = if let Ok(id) = Uuid::parse_str(&slug_or_id) {
        ctx.query(move |tx| Box::pin(async move { repo.get_by_id_tx(tx, id).await }))
            .await?
    } else {
        let slug_clone = slug_or_id.clone();
        ctx.query(move |tx| Box::pin(async move { repo.get_by_slug_tx(tx, &slug_clone).await }))
            .await?
    };
    let set =
        set.ok_or_else(|| ApiError::NotFound(format!("Embedding set not found: {}", slug_or_id)))?;
    let slug = set.slug.clone();

    if set.mode == matric_core::EmbeddingSetMode::Manual {
        // Manual sets: queue a re-embed job (jobs are global, not per-schema)
        let job_id = state
            .db
            .jobs
            .queue(
                None,
                matric_core::JobType::RefreshEmbeddingSet,
                matric_core::JobType::RefreshEmbeddingSet.default_priority(),
                Some(serde_json::json!({ "set_slug": slug })),
            )
            .await?;
        return Ok(Json(serde_json::json!({
            "status": "queued",
            "job_id": job_id.to_string(),
            "mode": "manual",
            "message": "Re-embedding job queued for manual set members"
        })));
    }

    let repo = matric_db::PgEmbeddingSetRepository::new(state.db.pool.clone());
    let added = ctx
        .execute(move |tx| Box::pin(async move { repo.refresh_tx(tx, &slug).await }))
        .await?;
    Ok(Json(serde_json::json!({ "added": added })))
}

/// List embedding configs
#[utoipa::path(get, path = "/api/v1/embedding-configs", tag = "Embeddings",
    responses((status = 200, description = "Success")))]
async fn list_embedding_configs(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let configs = state.db.embedding_sets.list_configs().await?;
    Ok(Json(configs))
}

/// Get default embedding config
#[utoipa::path(get, path = "/api/v1/embedding-configs/default", tag = "Embeddings",
    responses((status = 200, description = "Success")))]
async fn get_default_embedding_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state
        .db
        .embedding_sets
        .get_default_config()
        .await?
        .ok_or_else(|| ApiError::NotFound("Default embedding config not found".to_string()))?;
    Ok(Json(config))
}

/// Get embedding config by ID
#[utoipa::path(get, path = "/api/v1/embedding-configs/{id}", tag = "Embeddings",
    params(("id" = Uuid, Path, description = "Config ID")),
    responses((status = 200, description = "Success")))]
async fn get_embedding_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state
        .db
        .embedding_sets
        .get_config(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Embedding config {} not found", id)))?;
    Ok(Json(config))
}

/// Create a new embedding config
#[utoipa::path(post, path = "/api/v1/embedding-configs", tag = "Embeddings",
    responses((status = 201, description = "Success")))]
async fn create_embedding_config(
    State(state): State<AppState>,
    Json(body): Json<matric_core::CreateEmbeddingConfigRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.db.embedding_sets.create_config(body).await?;
    Ok((StatusCode::CREATED, Json(config)))
}

/// Update an embedding config
#[utoipa::path(patch, path = "/api/v1/embedding-configs/{id}", tag = "Embeddings",
    params(("id" = Uuid, Path, description = "Config ID")),
    responses((status = 204, description = "Success")))]
async fn update_embedding_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<matric_core::UpdateEmbeddingConfigRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.db.embedding_sets.update_config(id, body).await?;
    Ok(Json(config))
}

/// Delete an embedding config
#[utoipa::path(delete, path = "/api/v1/embedding-configs/{id}", tag = "Embeddings",
    params(("id" = Uuid, Path, description = "Config ID")),
    responses((status = 204, description = "Success")))]
async fn delete_embedding_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.embedding_sets.delete_config(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// JOB HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct CreateJobBody {
    note_id: Option<Uuid>,
    job_type: String,
    priority: Option<i32>,
    payload: Option<serde_json::Value>,
    /// When true, skip if a pending job with the same note_id+job_type already exists.
    #[serde(default)]
    deduplicate: bool,
}

#[utoipa::path(post, path = "/api/v1/jobs", tag = "Jobs",
    responses((status = 201, description = "Success")))]
async fn create_job(
    State(state): State<AppState>,
    Json(body): Json<CreateJobBody>,
) -> Result<impl IntoResponse, ApiError> {
    let job_type = match body.job_type.as_str() {
        "ai_revision" => JobType::AiRevision,
        "embedding" => JobType::Embedding,
        "linking" => JobType::Linking,
        "context_update" => JobType::ContextUpdate,
        "title_generation" => JobType::TitleGeneration,
        "concept_tagging" => JobType::ConceptTagging,
        "re_embed_all" => JobType::ReEmbedAll,
        "extraction" => JobType::Extraction,
        "exif_extraction" => JobType::ExifExtraction,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid job type: {}",
                body.job_type
            )))
        }
    };

    // Validate note exists before queuing (avoids raw FK constraint error)
    if let Some(note_id) = body.note_id {
        state
            .db
            .notes
            .fetch(note_id)
            .await
            .map_err(|_| ApiError::NotFound(format!("Note not found: {}", note_id)))?;
    }

    let priority = body.priority.unwrap_or_else(|| job_type.default_priority());

    if body.deduplicate {
        // Deduplicated queuing: skip if same note_id+job_type already pending
        let maybe_id = state
            .db
            .jobs
            .queue_deduplicated(body.note_id, job_type, priority, body.payload)
            .await?;

        match maybe_id {
            Some(job_id) => {
                state.event_bus.emit(ServerEvent::JobQueued {
                    job_id,
                    job_type: format!("{:?}", job_type),
                    note_id: body.note_id,
                });
                Ok((
                    StatusCode::CREATED,
                    Json(serde_json::json!({
                        "id": job_id,
                        "status": "queued"
                    })),
                ))
            }
            None => Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "id": null,
                    "status": "already_pending"
                })),
            )),
        }
    } else {
        let job_id = state
            .db
            .jobs
            .queue(body.note_id, job_type, priority, body.payload)
            .await?;

        state.event_bus.emit(ServerEvent::JobQueued {
            job_id,
            job_type: format!("{:?}", job_type),
            note_id: body.note_id,
        });

        Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": job_id,
                "status": "queued",
                "message": format!("{:?} job queued", job_type),
            })),
        ))
    }
}

#[utoipa::path(get, path = "/api/v1/jobs/{id}", tag = "Jobs",
    params(("id" = Uuid, Path, description = "Job ID")),
    responses((status = 200, description = "Success")))]
async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let job = state
        .db
        .jobs
        .get(id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Job not found".to_string()))?;
    Ok(Json(job))
}

#[utoipa::path(get, path = "/api/v1/jobs/pending", tag = "Jobs",
    responses((status = 200, description = "Success")))]
async fn pending_jobs_count(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let count = state.db.jobs.pending_count().await?;
    Ok(Json(serde_json::json!({ "pending": count })))
}

#[derive(Debug, Deserialize)]
struct ListJobsQuery {
    status: Option<String>,
    job_type: Option<String>,
    note_id: Option<Uuid>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[utoipa::path(get, path = "/api/v1/jobs", tag = "Jobs",
    responses((status = 200, description = "Success")))]
async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<ListJobsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(matric_core::defaults::PAGE_LIMIT);
    let offset = query.offset.unwrap_or(matric_core::defaults::PAGE_OFFSET);

    let jobs = state
        .db
        .jobs
        .list_filtered(
            query.status.as_deref(),
            query.job_type.as_deref(),
            query.note_id,
            limit,
            offset,
        )
        .await?;

    // Get stats for summary
    let stats = state.db.jobs.queue_stats().await?;

    Ok(Json(serde_json::json!({
        "jobs": jobs,
        "total": stats.total,
        "pending": stats.pending,
        "processing": stats.processing,
        "completed_last_hour": stats.completed_last_hour,
        "failed_last_hour": stats.failed_last_hour
    })))
}

#[utoipa::path(get, path = "/api/v1/jobs/stats", tag = "Jobs",
    responses((status = 200, description = "Success")))]
async fn queue_stats(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let stats = state.db.jobs.queue_stats().await?;
    Ok(Json(stats))
}

/// Get extraction job statistics.
///
/// Returns analytics for extraction jobs:
/// - Total/completed/failed/pending counts
/// - Average duration for completed jobs
/// - Breakdown by extraction strategy
#[utoipa::path(get, path = "/api/v1/extraction/stats", tag = "Jobs",
    responses((status = 200, description = "Success")))]
async fn extraction_stats(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let stats = matric_db::get_extraction_stats(&state.db.pool).await?;
    Ok(Json(stats))
}

// =============================================================================
// OAUTH2 HANDLERS
// =============================================================================

/// OAuth2 authorization server metadata (RFC 8414).
#[utoipa::path(get, path = "/.well-known/oauth-authorization-server", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_discovery(State(state): State<AppState>) -> impl IntoResponse {
    let metadata = AuthorizationServerMetadata {
        issuer: state.issuer.clone(),
        authorization_endpoint: format!("{}/oauth/authorize", state.issuer),
        token_endpoint: format!("{}/oauth/token", state.issuer),
        registration_endpoint: Some(format!("{}/oauth/register", state.issuer)),
        introspection_endpoint: Some(format!("{}/oauth/introspect", state.issuer)),
        revocation_endpoint: Some(format!("{}/oauth/revoke", state.issuer)),
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: vec![
            "authorization_code".to_string(),
            "client_credentials".to_string(),
            "refresh_token".to_string(),
        ],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_basic".to_string(),
            "client_secret_post".to_string(),
        ],
        scopes_supported: vec![
            "read".to_string(),
            "write".to_string(),
            "delete".to_string(),
            "admin".to_string(),
            "mcp".to_string(),
        ],
        code_challenge_methods_supported: Some(vec!["S256".to_string(), "plain".to_string()]),
    };
    Json(metadata)
}

/// OAuth Protected Resource Metadata (RFC 9728).
/// Required by MCP OAuth clients to discover authorization server.
#[utoipa::path(get, path = "/.well-known/oauth-protected-resource", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_protected_resource(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "resource": state.issuer.clone(),
        "authorization_servers": [format!("{}", state.issuer)],
        "bearer_methods_supported": ["header"],
        "scopes_supported": ["read", "write", "delete", "admin", "mcp"],
    }))
}

/// OAuth2 Dynamic Client Registration (RFC 7591).
/// Allowed OAuth2 grant types for client registration.
const ALLOWED_GRANT_TYPES: &[&str] = &["authorization_code", "client_credentials", "refresh_token"];

/// Allowed OAuth2 scopes for API key and client registration.
const ALLOWED_SCOPES: &[&str] = &["read", "write", "admin", "mcp"];

#[utoipa::path(post, path = "/oauth/register", tag = "OAuth",
    request_body = ClientRegistrationRequest,
    responses((status = 201, description = "Created")))]
async fn oauth_register(
    State(state): State<AppState>,
    Json(req): Json<ClientRegistrationRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Validate grant types (Issue #115 — AUTH-004)
    for gt in &req.grant_types {
        if !ALLOWED_GRANT_TYPES.contains(&gt.as_str()) {
            let msg = format!(
                "Invalid grant_type: '{}'. Allowed: {}",
                gt,
                ALLOWED_GRANT_TYPES.join(", ")
            );
            return Err(OAuthApiError::OAuth(OAuthError::invalid_request(&msg)));
        }
    }

    // Validate scope if provided
    if let Some(ref scope) = req.scope {
        for s in scope.split_whitespace() {
            if !ALLOWED_SCOPES.contains(&s) {
                let msg = format!(
                    "Invalid scope: '{}'. Allowed: {}",
                    s,
                    ALLOWED_SCOPES.join(", ")
                );
                return Err(OAuthApiError::OAuth(OAuthError::invalid_request(&msg)));
            }
        }
    }

    let mut response = state.db.oauth.register_client(req).await?;

    // Set the registration_client_uri based on our issuer
    response.registration_client_uri = Some(format!(
        "{}/oauth/register/{}",
        state.issuer, response.client_id
    ));

    Ok((StatusCode::CREATED, Json(response)))
}

/// Parse client credentials from Authorization header or body.
fn parse_client_credentials(
    headers: &HeaderMap,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Option<(String, String)> {
    // Try Authorization header first (client_secret_basic)
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(basic_auth) = auth_str.strip_prefix("Basic ") {
                if let Ok(decoded) = base64_decode(basic_auth) {
                    if let Ok(decoded_str) = String::from_utf8(decoded) {
                        if let Some((id, secret)) = decoded_str.split_once(':') {
                            return Some((id.to_string(), secret.to_string()));
                        }
                    }
                }
            }
        }
    }

    // Fall back to body parameters (client_secret_post)
    if let (Some(id), Some(secret)) = (client_id, client_secret) {
        return Some((id.to_string(), secret.to_string()));
    }

    None
}

/// Base64 decode helper.
fn base64_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.decode(input)
}

impl AppState {
    /// Determine token lifetime based on scope.
    ///
    /// MCP clients (scope contains "mcp") get longer tokens to support interactive sessions.
    /// Lifetimes are configurable via `OAUTH_TOKEN_LIFETIME_SECS` and `OAUTH_MCP_TOKEN_LIFETIME_SECS`.
    fn token_lifetime_for_scope(&self, scope: &str) -> chrono::Duration {
        if scope.contains("mcp") {
            self.oauth_mcp_token_lifetime
        } else {
            self.oauth_token_lifetime
        }
    }
}

/// OAuth2 token endpoint.
#[utoipa::path(post, path = "/oauth/token", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(req): Form<TokenRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Parse client credentials
    let (client_id, client_secret) = parse_client_credentials(
        &headers,
        req.client_id.as_deref(),
        req.client_secret.as_deref(),
    )
    .ok_or_else(|| {
        OAuthApiError::OAuth(OAuthError::invalid_client("Missing client credentials"))
    })?;

    // Validate client credentials
    let valid = state
        .db
        .oauth
        .validate_client(&client_id, &client_secret)
        .await?;
    if !valid {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_client(
            "Invalid client credentials",
        )));
    }

    match req.grant_type.as_str() {
        "client_credentials" => {
            // Check if client supports this grant type
            if !state
                .db
                .oauth
                .client_supports_grant(&client_id, "client_credentials")
                .await?
            {
                return Err(OAuthApiError::OAuth(OAuthError::unauthorized_client(
                    "Client not authorized for client_credentials grant",
                )));
            }

            // Get client to determine scope
            let client = state
                .db
                .oauth
                .get_client(&client_id)
                .await?
                .ok_or_else(|| {
                    OAuthApiError::OAuth(OAuthError::invalid_client("Client not found"))
                })?;

            let scope = req.scope.unwrap_or(client.scope);
            let lifetime = state.token_lifetime_for_scope(&scope);
            let (access_token, _, token) = state
                .db
                .oauth
                .create_token_with_lifetime(&client_id, &scope, None, false, lifetime)
                .await?;

            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: lifetime.num_seconds(),
                refresh_token: None,
                scope: Some(token.scope),
            };
            Ok(Json(response))
        }

        "authorization_code" => {
            let code = req
                .code
                .as_deref()
                .ok_or_else(|| OAuthApiError::OAuth(OAuthError::invalid_request("Missing code")))?;
            let redirect_uri = req.redirect_uri.as_deref().ok_or_else(|| {
                OAuthApiError::OAuth(OAuthError::invalid_request("Missing redirect_uri"))
            })?;

            // Consume the authorization code
            let auth_code = state
                .db
                .oauth
                .consume_authorization_code(
                    code,
                    &client_id,
                    redirect_uri,
                    req.code_verifier.as_deref(),
                )
                .await
                .map_err(|_| {
                    OAuthApiError::OAuth(OAuthError::invalid_grant(
                        "Invalid or expired authorization code",
                    ))
                })?;

            // Create tokens
            let lifetime = state.token_lifetime_for_scope(&auth_code.scope);
            let (access_token, refresh_token, token) = state
                .db
                .oauth
                .create_token_with_lifetime(
                    &client_id,
                    &auth_code.scope,
                    auth_code.user_id.as_deref(),
                    true,
                    lifetime,
                )
                .await?;

            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: lifetime.num_seconds(),
                refresh_token,
                scope: Some(token.scope),
            };
            Ok(Json(response))
        }

        "refresh_token" => {
            let refresh_token = req.refresh_token.as_deref().ok_or_else(|| {
                OAuthApiError::OAuth(OAuthError::invalid_request("Missing refresh_token"))
            })?;

            let (access_token, new_refresh_token, token) = state
                .db
                .oauth
                .refresh_access_token(refresh_token, &client_id)
                .await
                .map_err(|_| {
                    OAuthApiError::OAuth(OAuthError::invalid_grant("Invalid refresh token"))
                })?;

            let lifetime = state.token_lifetime_for_scope(&token.scope);
            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: lifetime.num_seconds(),
                refresh_token: new_refresh_token,
                scope: Some(token.scope),
            };
            Ok(Json(response))
        }

        _ => Err(OAuthApiError::OAuth(OAuthError::unsupported_grant_type(
            &format!("Unsupported grant type: {}", req.grant_type),
        ))),
    }
}

/// Token introspection request.
#[derive(Debug, Deserialize)]
struct IntrospectRequest {
    token: String,
    #[serde(default)]
    #[allow(dead_code)]
    token_type_hint: Option<String>,
}

/// OAuth2 token introspection (RFC 7662).
#[utoipa::path(post, path = "/oauth/introspect", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_introspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(req): Form<IntrospectRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Introspection requires client authentication
    let (client_id, client_secret) =
        parse_client_credentials(&headers, None, None).ok_or_else(|| {
            OAuthApiError::OAuth(OAuthError::invalid_client("Missing client credentials"))
        })?;

    let valid = state
        .db
        .oauth
        .validate_client(&client_id, &client_secret)
        .await?;
    if !valid {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_client(
            "Invalid client credentials",
        )));
    }

    let mut response = state.db.oauth.introspect_token(&req.token).await?;
    response.iss = Some(state.issuer.clone());

    Ok(Json(response))
}

/// Token revocation request.
#[derive(Debug, Deserialize)]
struct RevokeRequest {
    token: String,
    #[serde(default)]
    token_type_hint: Option<String>,
}

/// OAuth2 token revocation (RFC 7009).
#[utoipa::path(post, path = "/oauth/revoke", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(req): Form<RevokeRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Revocation requires client authentication
    let (client_id, client_secret) =
        parse_client_credentials(&headers, None, None).ok_or_else(|| {
            OAuthApiError::OAuth(OAuthError::invalid_client("Missing client credentials"))
        })?;

    let valid = state
        .db
        .oauth
        .validate_client(&client_id, &client_secret)
        .await?;
    if !valid {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_client(
            "Invalid client credentials",
        )));
    }

    // Revoke the token (always returns 200 per RFC 7009)
    let _ = state
        .db
        .oauth
        .revoke_token(&req.token, req.token_type_hint.as_deref())
        .await;

    Ok(StatusCode::OK)
}

// =============================================================================
// AUTHORIZATION ENDPOINT (RFC 6749 Section 4.1)
// =============================================================================

/// Authorization request query parameters.
#[derive(Debug, Deserialize)]
struct AuthorizationRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    code_challenge: Option<String>,
    #[serde(default)]
    code_challenge_method: Option<String>,
}

/// GET /oauth/authorize - Display authorization consent page.
#[utoipa::path(get, path = "/oauth/authorize", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn oauth_authorize_get(
    State(state): State<AppState>,
    Query(req): Query<AuthorizationRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Validate response_type
    if req.response_type != "code" {
        return Err(OAuthApiError::OAuth(OAuthError::unsupported_response_type(
            "Only 'code' response_type is supported",
        )));
    }

    // Validate client exists and redirect_uri matches
    let client = state
        .db
        .oauth
        .get_client(&req.client_id)
        .await?
        .ok_or_else(|| OAuthApiError::OAuth(OAuthError::invalid_client("Client not found")))?;

    if !client.is_active {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_client(
            "Client is not active",
        )));
    }

    // Validate redirect_uri (with flexible localhost port matching)
    if !validate_redirect_uri(&req.redirect_uri, &client.redirect_uris) {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_request(
            "Invalid redirect_uri",
        )));
    }

    // Determine scope (use requested or client default)
    let scope = req.scope.as_deref().unwrap_or(&client.scope);

    // Build the consent page HTML
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Authorize - Matric Memory</title>
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }}
        .card {{
            background: #fff;
            border-radius: 16px;
            box-shadow: 0 10px 40px rgba(0,0,0,0.3);
            max-width: 420px;
            width: 100%;
            overflow: hidden;
        }}
        .header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 24px;
            text-align: center;
        }}
        .header h1 {{
            font-size: 24px;
            font-weight: 600;
            margin-bottom: 4px;
        }}
        .header p {{
            opacity: 0.9;
            font-size: 14px;
        }}
        .content {{
            padding: 24px;
        }}
        .client-info {{
            background: #f7f9fc;
            border-radius: 8px;
            padding: 16px;
            margin-bottom: 20px;
        }}
        .client-name {{
            font-weight: 600;
            font-size: 18px;
            color: #333;
            margin-bottom: 4px;
        }}
        .client-id {{
            font-size: 12px;
            color: #888;
            font-family: monospace;
        }}
        .scope-section {{
            margin-bottom: 20px;
        }}
        .scope-label {{
            font-size: 14px;
            font-weight: 500;
            color: #555;
            margin-bottom: 8px;
        }}
        .scope-list {{
            display: flex;
            flex-wrap: wrap;
            gap: 8px;
        }}
        .scope-badge {{
            background: #e8f4fd;
            color: #1976d2;
            padding: 6px 12px;
            border-radius: 16px;
            font-size: 13px;
            font-weight: 500;
        }}
        .warning {{
            background: #fff3e0;
            border-left: 4px solid #ff9800;
            padding: 12px;
            margin-bottom: 20px;
            border-radius: 0 8px 8px 0;
            font-size: 13px;
            color: #e65100;
        }}
        .actions {{
            display: flex;
            gap: 12px;
        }}
        button {{
            flex: 1;
            padding: 14px 20px;
            border-radius: 8px;
            font-size: 15px;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.2s;
            border: none;
        }}
        .btn-approve {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
        }}
        .btn-approve:hover {{
            transform: translateY(-1px);
            box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4);
        }}
        .btn-deny {{
            background: #f5f5f5;
            color: #666;
        }}
        .btn-deny:hover {{
            background: #e0e0e0;
        }}
        .footer {{
            text-align: center;
            padding: 16px;
            background: #fafafa;
            border-top: 1px solid #eee;
            font-size: 12px;
            color: #888;
        }}
    </style>
</head>
<body>
    <div class="card">
        <div class="header">
            <h1>Matric Memory</h1>
            <p>Authorization Request</p>
        </div>
        <div class="content">
            <div class="client-info">
                <div class="client-name">{client_name}</div>
                <div class="client-id">{client_id}</div>
            </div>

            <div class="scope-section">
                <div class="scope-label">This application is requesting access to:</div>
                <div class="scope-list">
                    {scope_badges}
                </div>
            </div>

            <div class="warning">
                Authorizing will allow this application to access your Matric Memory data with the permissions listed above.
            </div>

            <form method="POST" action="/oauth/authorize">
                <input type="hidden" name="client_id" value="{client_id}">
                <input type="hidden" name="redirect_uri" value="{redirect_uri}">
                <input type="hidden" name="scope" value="{scope}">
                <input type="hidden" name="state" value="{state}">
                <input type="hidden" name="code_challenge" value="{code_challenge}">
                <input type="hidden" name="code_challenge_method" value="{code_challenge_method}">
                <input type="hidden" name="response_type" value="code">
                <div class="actions">
                    <button type="button" class="btn-deny" onclick="denyAccess()">Deny</button>
                    <button type="submit" name="action" value="approve" class="btn-approve">Approve</button>
                </div>
            </form>
        </div>
        <div class="footer">
            Powered by Matric Memory &bull; {issuer}
        </div>
    </div>
    <script>
        function denyAccess() {{
            const redirectUri = "{redirect_uri}";
            const state = "{state}";
            const sep = redirectUri.includes('?') ? '&' : '?';
            window.location.href = redirectUri + sep + "error=access_denied&error_description=User+denied+the+request" + (state ? "&state=" + encodeURIComponent(state) : "");
        }}
    </script>
</body>
</html>"#,
        client_name = html_escape(&client.client_name),
        client_id = html_escape(&req.client_id),
        redirect_uri = html_escape(&req.redirect_uri),
        scope = html_escape(scope),
        state = html_escape(req.state.as_deref().unwrap_or("")),
        code_challenge = html_escape(req.code_challenge.as_deref().unwrap_or("")),
        code_challenge_method = html_escape(req.code_challenge_method.as_deref().unwrap_or("")),
        scope_badges = scope
            .split_whitespace()
            .map(|s| format!(r#"<span class="scope-badge">{}</span>"#, html_escape(s)))
            .collect::<Vec<_>>()
            .join(""),
        issuer = html_escape(&state.issuer),
    );

    Ok(([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html))
}

/// Authorization form submission.
#[derive(Debug, Deserialize)]
struct AuthorizationForm {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    code_challenge: Option<String>,
    #[serde(default)]
    code_challenge_method: Option<String>,
    action: String,
}

/// POST /oauth/authorize - Process authorization and redirect with code.
#[utoipa::path(post, path = "/oauth/authorize", tag = "OAuth",
    responses((status = 302, description = "Redirect")))]
async fn oauth_authorize_post(
    State(state): State<AppState>,
    Form(req): Form<AuthorizationForm>,
) -> Result<impl IntoResponse, OAuthApiError> {
    // Check if user denied
    if req.action != "approve" {
        let redirect = build_error_redirect(
            &req.redirect_uri,
            "access_denied",
            "User denied the request",
            req.state.as_deref(),
        );
        return Ok(axum::response::Redirect::to(&redirect).into_response());
    }

    // Validate response_type
    if req.response_type != "code" {
        return Err(OAuthApiError::OAuth(OAuthError::unsupported_response_type(
            "Only 'code' response_type is supported",
        )));
    }

    // Validate client
    let client = state
        .db
        .oauth
        .get_client(&req.client_id)
        .await?
        .ok_or_else(|| OAuthApiError::OAuth(OAuthError::invalid_client("Client not found")))?;

    if !client.is_active {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_client(
            "Client is not active",
        )));
    }

    // Validate redirect_uri (with flexible localhost port matching)
    if !validate_redirect_uri(&req.redirect_uri, &client.redirect_uris) {
        return Err(OAuthApiError::OAuth(OAuthError::invalid_request(
            "Invalid redirect_uri",
        )));
    }

    // Determine scope
    let scope = req.scope.as_deref().unwrap_or(&client.scope);

    // Create authorization code
    let code = state
        .db
        .oauth
        .create_authorization_code(
            &req.client_id,
            &req.redirect_uri,
            scope,
            req.state.as_deref(),
            req.code_challenge.as_deref(),
            req.code_challenge_method.as_deref(),
            None, // No user_id for now (system-level auth)
        )
        .await?;

    // Build redirect URL with code and state
    let sep = if req.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    let mut redirect_url = format!(
        "{}{}code={}",
        req.redirect_uri,
        sep,
        urlencoding::encode(&code)
    );
    if let Some(s) = &req.state {
        redirect_url.push_str(&format!("&state={}", urlencoding::encode(s)));
    }

    Ok(axum::response::Redirect::to(&redirect_url).into_response())
}

/// Build an error redirect URL.
fn build_error_redirect(
    redirect_uri: &str,
    error: &str,
    description: &str,
    state: Option<&str>,
) -> String {
    let sep = if redirect_uri.contains('?') { '&' } else { '?' };
    let mut url = format!(
        "{}{}error={}&error_description={}",
        redirect_uri,
        sep,
        urlencoding::encode(error),
        urlencoding::encode(description)
    );
    if let Some(s) = state {
        url.push_str(&format!("&state={}", urlencoding::encode(s)));
    }
    url
}

/// Simple HTML escaping for security.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Validate redirect_uri against registered URIs.
/// Allows flexible localhost port matching for development clients (MCP, etc).
fn validate_redirect_uri(redirect_uri: &str, registered_uris: &[String]) -> bool {
    // Exact match first
    if registered_uris.contains(&redirect_uri.to_string()) {
        return true;
    }

    // For localhost URIs, allow any port if a localhost URI is registered
    if redirect_uri.starts_with("http://localhost:")
        || redirect_uri.starts_with("http://127.0.0.1:")
    {
        // Extract path from redirect_uri
        let uri_parts: Vec<&str> = redirect_uri.splitn(4, '/').collect();
        let path = if uri_parts.len() >= 4 {
            format!("/{}", uri_parts[3])
        } else if uri_parts.len() == 3 && uri_parts[2].contains(':') {
            "/".to_string()
        } else {
            "".to_string()
        };

        for registered in registered_uris {
            if registered.starts_with("http://localhost:")
                || registered.starts_with("http://127.0.0.1:")
            {
                // Extract path from registered URI
                let reg_parts: Vec<&str> = registered.splitn(4, '/').collect();
                let reg_path = if reg_parts.len() >= 4 {
                    format!("/{}", reg_parts[3])
                } else if reg_parts.len() == 3 && reg_parts[2].contains(':') {
                    "/".to_string()
                } else {
                    "".to_string()
                };

                // Match if paths are the same (allowing different ports)
                if path == reg_path {
                    return true;
                }
            }
        }
    }

    false
}

// =============================================================================
// API KEY HANDLERS
// =============================================================================

/// List all API keys (requires admin scope).
#[utoipa::path(get, path = "/api/v1/api-keys", tag = "OAuth",
    responses((status = 200, description = "Success")))]
async fn list_api_keys(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let keys = state.db.oauth.list_api_keys().await?;
    Ok(Json(serde_json::json!({ "api_keys": keys })))
}

/// Create a new API key.
#[utoipa::path(post, path = "/api/v1/api-keys", tag = "OAuth",
    request_body = CreateApiKeyRequest,
    responses((status = 201, description = "Created")))]
async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let response = state.db.oauth.create_api_key(req).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// Revoke an API key.
#[utoipa::path(delete, path = "/api/v1/api-keys/{id}", tag = "OAuth",
    params(("id" = Uuid, Path, description = "API key ID")),
    responses(
        (status = 204, description = "Success"),
        (status = 404, description = "Not found"),
    )
)]
async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let revoked = state.db.oauth.revoke_api_key(id).await?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound("API key not found".to_string()))
    }
}

// =============================================================================
// OAUTH ERROR HANDLING
// =============================================================================

/// OAuth-specific API error.
#[derive(Debug)]
enum OAuthApiError {
    OAuth(OAuthError),
    Database(matric_core::Error),
}

impl From<matric_core::Error> for OAuthApiError {
    fn from(err: matric_core::Error) -> Self {
        OAuthApiError::Database(err)
    }
}

impl IntoResponse for OAuthApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            OAuthApiError::OAuth(err) => {
                let status = match err.error.as_str() {
                    "invalid_client" => StatusCode::UNAUTHORIZED,
                    "unauthorized_client" => StatusCode::FORBIDDEN,
                    "server_error" => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, Json(err)).into_response()
            }
            OAuthApiError::Database(err) => {
                let oauth_err = OAuthError::server_error(&err.to_string());
                (StatusCode::INTERNAL_SERVER_ERROR, Json(oauth_err)).into_response()
            }
        }
    }
}

// =============================================================================
// AUTHENTICATION MIDDLEWARE
// =============================================================================

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

/// Extractor for authenticated requests.
///
/// This extractor validates Bearer tokens (OAuth access tokens or API keys)
/// and provides the authenticated principal to handlers.
///
/// Usage:
/// ```ignore
/// async fn my_handler(auth: Auth) -> impl IntoResponse {
///     if !auth.principal.has_scope("write") {
///         return Err(ApiError::Forbidden("write scope required"));
///     }
///     // ... handler logic
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Auth {
    pub principal: AuthPrincipal,
}

#[axum::async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Read Auth from extensions (injected by auth_middleware).
        // The middleware always runs before handlers and sets Auth for all cases:
        // - Valid token → OAuthClient or ApiKey principal
        // - Invalid token → request rejected before reaching handler
        // - No token + REQUIRE_AUTH=true → request rejected before reaching handler
        // - No token + REQUIRE_AUTH=false → Anonymous principal
        if let Some(auth) = parts.extensions.get::<Auth>() {
            return Ok(auth.clone());
        }

        // Fallback: middleware didn't run (e.g., in tests without middleware layer).
        // Default to Anonymous.
        Ok(Auth {
            principal: AuthPrincipal::Anonymous,
        })
    }
}

/// Extractor that requires authentication.
///
/// Use this for endpoints that must have a valid token.
#[derive(Debug, Clone)]
pub struct RequireAuth {
    pub principal: AuthPrincipal,
}

#[axum::async_trait]
impl FromRequestParts<AppState> for RequireAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = Auth::from_request_parts(parts, state).await?;

        if !auth.principal.is_authenticated() {
            return Err(ApiError::Unauthorized(
                "Authentication required".to_string(),
            ));
        }

        Ok(RequireAuth {
            principal: auth.principal,
        })
    }
}

/// Helper trait for requiring specific scopes in handlers.
impl RequireAuth {
    /// Check if the authenticated principal has the required scope.
    #[allow(dead_code)]
    fn require_scope(&self, scope: &str) -> Result<(), ApiError> {
        if !self.principal.has_scope(scope) {
            return Err(ApiError::Forbidden(format!(
                "Missing required scope: {}",
                scope
            )));
        }
        Ok(())
    }
}

// =============================================================================
// BACKUP HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct BackupExportQuery {
    /// Only export starred notes
    #[serde(default)]
    starred_only: bool,
    /// Filter by tags (comma-separated)
    tags: Option<String>,
    /// Filter: notes created after this timestamp (ISO 8601)
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp (ISO 8601)
    created_before: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct BackupExportManifest {
    version: String,
    format: String,
    created_at: chrono::DateTime<chrono::Utc>,
    counts: BackupCounts,
}

#[derive(Debug, Serialize)]
struct BackupCounts {
    notes: usize,
    collections: usize,
    tags: usize,
    templates: usize,
}

#[derive(Debug, Serialize)]
struct BackupExportResponse {
    manifest: BackupExportManifest,
    notes: Vec<serde_json::Value>,
    collections: Vec<serde_json::Value>,
    tags: Vec<String>,
    templates: Vec<serde_json::Value>,
}

/// Export all notes as a JSON export.
#[utoipa::path(get, path = "/api/v1/backup/export", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn backup_export(
    State(state): State<AppState>,
    Query(query): Query<BackupExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::NoteRepository;

    // Build list request with filters
    let tags = query.tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    let mut filter = None;
    if query.starred_only {
        filter = Some("starred".to_string());
    }

    let list_req = ListNotesRequest {
        limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT), // Large limit for export
        offset: None,
        filter,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("asc".to_string()),
        collection_id: None,
        tags,
        created_after: query.created_after,
        created_before: query.created_before,
        updated_after: None,
        updated_before: None,
    };

    // Fetch all notes
    let notes_response = state.db.notes.list(list_req).await?;
    let mut exported_notes = Vec::new();

    for note in notes_response.notes {
        // Fetch full note with content
        if let Ok(full_note) = state.db.notes.fetch(note.id).await {
            let note_tags = state
                .db
                .tags
                .get_for_note(note.id)
                .await
                .unwrap_or_default();
            exported_notes.push(serde_json::json!({
                "id": full_note.note.id,
                "title": full_note.note.title,
                "original_content": full_note.original.content,
                "revised_content": full_note.revised.content,
                "format": full_note.note.format,
                "source": full_note.note.source,
                "starred": full_note.note.starred,
                "archived": full_note.note.archived,
                "collection_id": full_note.note.collection_id,
                "created_at": full_note.note.created_at_utc,
                "updated_at": full_note.note.updated_at_utc,
                "tags": note_tags,
            }));
        }
    }

    // Fetch collections
    let collections = state.db.collections.list(None).await.unwrap_or_default();
    let collections_json: Vec<serde_json::Value> = collections
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "description": c.description,
                "parent_id": c.parent_id,
                "created_at": c.created_at_utc,
                "note_count": c.note_count,
            })
        })
        .collect();

    // Fetch all tags (extract names)
    let all_tags = state.db.tags.list().await.unwrap_or_default();
    let tag_names: Vec<String> = all_tags.iter().map(|t| t.name.clone()).collect();

    // Fetch templates
    let templates = {
        let ctx = state.db.for_schema("public")?;
        let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
            .await
    }
    .unwrap_or_default();
    let templates_json: Vec<serde_json::Value> = templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "description": t.description,
                "content": t.content,
                "format": t.format,
                "default_tags": t.default_tags,
                "collection_id": t.collection_id,
                "created_at": t.created_at_utc,
                "updated_at": t.updated_at_utc,
            })
        })
        .collect();

    let response = BackupExportResponse {
        manifest: BackupExportManifest {
            version: "1.0.0".to_string(),
            format: "matric-backup".to_string(),
            created_at: chrono::Utc::now(),
            counts: BackupCounts {
                notes: exported_notes.len(),
                collections: collections_json.len(),
                tags: tag_names.len(),
                templates: templates_json.len(),
            },
        },
        notes: exported_notes,
        collections: collections_json,
        tags: tag_names,
        templates: templates_json,
    };

    Ok(Json(response))
}

/// Download backup as a file attachment.
#[utoipa::path(get, path = "/api/v1/backup/download", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn backup_download(
    State(state): State<AppState>,
    Query(query): Query<BackupExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::NoteRepository;

    // Build list request with filters (same as backup_export)
    let tags = query.tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });

    let mut filter = None;
    if query.starred_only {
        filter = Some("starred".to_string());
    }

    let list_req = ListNotesRequest {
        limit: Some(matric_core::defaults::INTERNAL_FETCH_LIMIT),
        offset: None,
        filter,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("asc".to_string()),
        collection_id: None,
        tags,
        created_after: query.created_after,
        created_before: query.created_before,
        updated_after: None,
        updated_before: None,
    };

    // Fetch all notes
    let notes_response = state.db.notes.list(list_req).await?;
    let mut exported_notes = Vec::new();

    for note in notes_response.notes {
        if let Ok(full_note) = state.db.notes.fetch(note.id).await {
            let note_tags = state
                .db
                .tags
                .get_for_note(note.id)
                .await
                .unwrap_or_default();
            exported_notes.push(serde_json::json!({
                "id": full_note.note.id,
                "title": full_note.note.title,
                "original_content": full_note.original.content,
                "revised_content": full_note.revised.content,
                "format": full_note.note.format,
                "source": full_note.note.source,
                "starred": full_note.note.starred,
                "archived": full_note.note.archived,
                "collection_id": full_note.note.collection_id,
                "created_at": full_note.note.created_at_utc,
                "updated_at": full_note.note.updated_at_utc,
                "tags": note_tags,
            }));
        }
    }

    // Fetch collections
    let collections = state.db.collections.list(None).await.unwrap_or_default();
    let collections_json: Vec<serde_json::Value> = collections
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "description": c.description,
                "parent_id": c.parent_id,
                "created_at": c.created_at_utc,
                "note_count": c.note_count,
            })
        })
        .collect();

    // Fetch all tags
    let all_tags = state.db.tags.list().await.unwrap_or_default();
    let tag_names: Vec<String> = all_tags.iter().map(|t| t.name.clone()).collect();

    // Fetch templates
    let templates = {
        let ctx = state.db.for_schema("public")?;
        let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
            .await
    }
    .unwrap_or_default();
    let templates_json: Vec<serde_json::Value> = templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "description": t.description,
                "content": t.content,
                "format": t.format,
                "default_tags": t.default_tags,
                "collection_id": t.collection_id,
                "created_at": t.created_at_utc,
                "updated_at": t.updated_at_utc,
            })
        })
        .collect();

    let response = BackupExportResponse {
        manifest: BackupExportManifest {
            version: "1.0.0".to_string(),
            format: "matric-backup".to_string(),
            created_at: chrono::Utc::now(),
            counts: BackupCounts {
                notes: exported_notes.len(),
                collections: collections_json.len(),
                tags: tag_names.len(),
                templates: templates_json.len(),
            },
        },
        notes: exported_notes,
        collections: collections_json,
        tags: tag_names,
        templates: templates_json,
    };

    // Serialize to JSON
    let json_content = serde_json::to_string_pretty(&response)
        .map_err(|e| ApiError::BadRequest(format!("Failed to serialize backup: {}", e)))?;

    // Generate filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("matric-backup-{}.json", timestamp);

    // Return as downloadable file
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/json; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename)
            .parse()
            .unwrap(),
    );

    Ok((StatusCode::OK, headers, json_content))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct BackupImportBody {
    /// The backup data to import
    backup: BackupImportData,
    /// Dry run mode - validate without importing
    #[serde(default)]
    dry_run: bool,
    /// Conflict resolution strategy
    #[serde(default)]
    on_conflict: ConflictStrategy,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields validated during deserialization, used for future features
struct BackupImportData {
    manifest: Option<serde_json::Value>,
    notes: Vec<BackupNoteData>,
    #[serde(default)]
    collections: Vec<serde_json::Value>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    templates: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BackupNoteData {
    id: Option<Uuid>,
    title: Option<String>,
    original_content: Option<String>,
    revised_content: Option<String>,
    content: Option<String>, // Fallback if original_content not present
    format: Option<String>,
    source: Option<String>,
    starred: Option<bool>,
    archived: Option<bool>,
    collection_id: Option<Uuid>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ConflictStrategy {
    /// Skip notes that already exist (by ID)
    #[default]
    Skip,
    /// Replace existing notes with imported data
    Replace,
    /// Merge: keep existing, add new
    Merge,
}

#[derive(Debug, Serialize)]
struct BackupImportResponse {
    status: String,
    dry_run: bool,
    imported: ImportCounts,
    skipped: ImportCounts,
    errors: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct ImportCounts {
    notes: usize,
    collections: usize,
    templates: usize,
}

/// Import a knowledge shard.
#[utoipa::path(post, path = "/api/v1/backup/import", tag = "Backup",
    request_body = BackupImportBody,
    responses((status = 200, description = "Success")))]
async fn backup_import(
    State(state): State<AppState>,
    Json(body): Json<BackupImportBody>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::{CreateNoteRequest, NoteRepository};

    let mut imported = ImportCounts::default();
    let mut skipped = ImportCounts::default();
    let mut errors: Vec<String> = Vec::new();

    // Import notes
    for note_data in &body.backup.notes {
        // Get content (prefer original_content, fall back to content)
        let content = note_data
            .original_content
            .as_ref()
            .or(note_data.content.as_ref())
            .cloned()
            .unwrap_or_default();

        if content.is_empty() {
            errors.push(format!(
                "Note {:?} has no content, skipping",
                note_data
                    .id
                    .or(note_data.title.as_ref().map(|_| Uuid::nil()))
            ));
            skipped.notes += 1;
            continue;
        }

        // Check if note exists (by ID)
        if let Some(id) = note_data.id {
            if state.db.notes.exists(id).await.unwrap_or(false) {
                match body.on_conflict {
                    ConflictStrategy::Skip => {
                        skipped.notes += 1;
                        continue;
                    }
                    ConflictStrategy::Replace => {
                        // Delete existing and re-create
                        if !body.dry_run {
                            let _ = state.db.notes.soft_delete(id).await;
                        }
                    }
                    ConflictStrategy::Merge => {
                        // Keep existing
                        skipped.notes += 1;
                        continue;
                    }
                }
            }
        }

        if !body.dry_run {
            // Create the note
            let req = CreateNoteRequest {
                content,
                format: note_data
                    .format
                    .clone()
                    .unwrap_or_else(|| "markdown".to_string()),
                source: note_data
                    .source
                    .clone()
                    .unwrap_or_else(|| "import".to_string()),
                collection_id: note_data.collection_id,
                tags: note_data.tags.clone(),
                metadata: None,
                document_type_id: None,
            };

            match state.db.notes.insert(req).await {
                Ok(new_id) => {
                    // Update status if specified
                    if note_data.starred.unwrap_or(false) || note_data.archived.unwrap_or(false) {
                        let status_req = matric_core::UpdateNoteStatusRequest {
                            starred: note_data.starred,
                            archived: note_data.archived,
                            metadata: None,
                        };
                        let _ = state.db.notes.update_status(new_id, status_req).await;
                    }

                    // If revised content exists, update it
                    if let Some(revised) = &note_data.revised_content {
                        if !revised.is_empty() {
                            let _ = state
                                .db
                                .notes
                                .update_revised(new_id, revised, Some("Imported from backup"))
                                .await;
                        }
                    }

                    // Queue NLP pipeline
                    queue_nlp_pipeline(
                        &state.db,
                        new_id,
                        RevisionMode::None,
                        &state.event_bus,
                        None,
                    )
                    .await;

                    imported.notes += 1;
                }
                Err(e) => {
                    errors.push(format!("Failed to import note: {}", e));
                }
            }
        } else {
            imported.notes += 1;
        }
    }

    // Import collections (basic support)
    for coll in &body.backup.collections {
        if let (Some(name), Some(_id)) = (
            coll.get("name").and_then(|v| v.as_str()),
            coll.get("id").and_then(|v| v.as_str()),
        ) {
            if !body.dry_run {
                let description = coll.get("description").and_then(|v| v.as_str());
                match state.db.collections.create(name, description, None).await {
                    Ok(_) => imported.collections += 1,
                    Err(_) => skipped.collections += 1,
                }
            } else {
                imported.collections += 1;
            }
        }
    }

    // Import templates (basic support)
    for tmpl in &body.backup.templates {
        if let (Some(name), Some(content)) = (
            tmpl.get("name").and_then(|v| v.as_str()),
            tmpl.get("content").and_then(|v| v.as_str()),
        ) {
            if !body.dry_run {
                use matric_core::CreateTemplateRequest;
                let req = CreateTemplateRequest {
                    name: name.to_string(),
                    description: tmpl
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    content: content.to_string(),
                    format: tmpl
                        .get("format")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    default_tags: None,
                    collection_id: None,
                };
                let res = {
                    let ctx = state.db.for_schema("public")?;
                    let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
                    ctx.execute(move |tx| {
                        Box::pin(async move { templates.create_tx(tx, req).await })
                    })
                    .await
                };
                match res {
                    Ok(_) => imported.templates += 1,
                    Err(_) => skipped.templates += 1,
                }
            } else {
                imported.templates += 1;
            }
        }
    }

    let status = if errors.is_empty() {
        "success"
    } else {
        "partial"
    };

    Ok(Json(BackupImportResponse {
        status: status.to_string(),
        dry_run: body.dry_run,
        imported,
        skipped,
        errors,
    }))
}

#[derive(Debug, Deserialize)]
struct BackupTriggerBody {
    /// Target destinations: local, s3, rsync, or all (reserved for future use)
    #[allow(dead_code)]
    destinations: Option<Vec<String>>,
    /// Dry run mode - show what would be done without executing
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct BackupTriggerResponse {
    status: String,
    output: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trigger an immediate database backup using native pg_dump.
///
/// This replaces the previous shell-script-based approach. The backup is created
/// as a compressed .sql.gz file in the backup directory, using the same format
/// as `database_backup_snapshot` but with the `auto_` prefix.
#[utoipa::path(post, path = "/api/v1/backup/trigger", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn backup_trigger(
    State(state): State<AppState>,
    body: Option<Json<BackupTriggerBody>>,
) -> Result<impl IntoResponse, ApiError> {
    let body = body.map(|b| b.0).unwrap_or(BackupTriggerBody {
        destinations: None,
        dry_run: false,
    });

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| ApiError::BadRequest(format!("Cannot create backup dir: {}", e)))?;

    let timestamp = chrono::Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S");
    let filename = format!("{}_database_{}.sql.gz", backup_prefix::AUTO, ts_str);
    let path = std::path::Path::new(&backup_dir).join(&filename);

    if body.dry_run {
        return Ok(Json(BackupTriggerResponse {
            status: "dry_run".to_string(),
            output: format!("Would create backup: {}", path.display()),
            timestamp,
        }));
    }

    // Get note count for metadata
    let note_count = state
        .db
        .notes
        .list(ListNotesRequest {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .map(|r| r.total)
        .ok();

    // Run pg_dump
    let output = std::process::Command::new("pg_dump")
        .args(["-U", "matric", "-h", "localhost", "matric"])
        .env("PGPASSWORD", "matric")
        .output()
        .map_err(|e| ApiError::BadRequest(format!("pg_dump failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(Json(BackupTriggerResponse {
            status: "failed".to_string(),
            output: format!("pg_dump error: {}", stderr),
            timestamp,
        }));
    }

    // Compress and save
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let file = std::fs::File::create(&path)
        .map_err(|e| ApiError::BadRequest(format!("Cannot create file: {}", e)))?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder
        .write_all(&output.stdout)
        .map_err(|e| ApiError::BadRequest(format!("Write failed: {}", e)))?;
    encoder
        .finish()
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    // Save metadata sidecar file
    let mut metadata = BackupMetadata::auto(note_count, None);
    if let Err(e) = metadata.populate_version_info(&state.db.pool).await {
        tracing::warn!("Failed to populate version info: {}", e);
    }
    if let Err(e) = metadata.save(&path) {
        tracing::warn!("Failed to save backup metadata: {}", e);
    }

    Ok(Json(BackupTriggerResponse {
        status: "success".to_string(),
        output: format!("Backup created: {} ({}).", filename, format_size(size)),
        timestamp,
    }))
}

#[derive(Debug, Serialize)]
struct BackupStatusResponse {
    backup_directory: String,
    /// Total size of all backups in bytes
    total_size_bytes: u64,
    /// Human-readable total size (e.g., "1.5 GB")
    total_size_human: String,
    /// Deprecated: use total_size_human instead
    disk_usage: Option<String>,
    backup_count: usize,
    /// Breakdown by type
    shard_count: usize,
    pgdump_count: usize,
    latest_backup: Option<LatestBackupInfo>,
    status: String,
}

#[derive(Debug, Serialize)]
struct LatestBackupInfo {
    path: String,
    filename: String,
    size_bytes: u64,
    modified: chrono::DateTime<chrono::Utc>,
}

/// Get the status of the backup system.
#[utoipa::path(get, path = "/api/v1/backup/status", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn backup_status(State(_state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    use std::fs;

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    let mut response = BackupStatusResponse {
        backup_directory: backup_dir.clone(),
        total_size_bytes: 0,
        total_size_human: "0 B".to_string(),
        disk_usage: None,
        backup_count: 0,
        shard_count: 0,
        pgdump_count: 0,
        latest_backup: None,
        status: "unknown".to_string(),
    };

    // Check if backup directory exists, create if missing
    let backup_path = std::path::Path::new(&backup_dir);
    if !backup_path.exists() {
        // Try to create directory, handle permission errors gracefully
        match std::fs::create_dir_all(backup_path) {
            Ok(_) => {
                response.status = "no_backups".to_string();
            }
            Err(e) => {
                response.status = format!("cannot_create_directory: {}", e);
                return Ok(Json(response));
            }
        }
    }

    // List ALL backup files (shards, pgdump, json)
    let mut backups: Vec<(String, std::fs::Metadata, &str)> = Vec::new();
    let mut total_size: u64 = 0;

    if let Ok(entries) = fs::read_dir(backup_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let backup_type = if name.ends_with(".tar.gz") {
                    Some("shard")
                } else if name.ends_with(".sql.gz") || name.ends_with(".sql") {
                    Some("pgdump")
                } else if name.ends_with(".json") {
                    Some("json")
                } else {
                    None
                };

                if let Some(btype) = backup_type {
                    if let Ok(meta) = entry.metadata() {
                        total_size += meta.len();
                        backups.push((path.to_string_lossy().to_string(), meta, btype));
                    }
                }
            }
        }
    }

    response.total_size_bytes = total_size;
    response.total_size_human = format_size(total_size);
    response.backup_count = backups.len();
    response.shard_count = backups.iter().filter(|(_, _, t)| *t == "shard").count();
    response.pgdump_count = backups.iter().filter(|(_, _, t)| *t == "pgdump").count();

    // Find latest backup (any type)
    if let Some((path, meta, _)) = backups
        .iter()
        .max_by_key(|(_, m, _)| m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH))
    {
        let filename = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let modified = meta
            .modified()
            .map(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                    .unwrap_or_else(chrono::Utc::now)
            })
            .unwrap_or_else(|_| chrono::Utc::now());

        response.latest_backup = Some(LatestBackupInfo {
            path: path.clone(),
            filename,
            size_bytes: meta.len(),
            modified,
        });
    }

    // Keep disk_usage for backwards compatibility
    response.disk_usage = Some(response.total_size_human.clone());

    // Determine status
    response.status = if response.backup_count > 0 {
        "healthy".to_string()
    } else {
        "no_backups".to_string()
    };

    Ok(Json(response))
}

// =============================================================================
// ARCHIVE EXPORT (Full backup with all components)
// =============================================================================

#[derive(Debug, Deserialize)]
struct ShardExportQuery {
    /// Components to include (comma-separated): notes,collections,tags,templates,links,embedding_sets,embeddings
    /// Default: notes,collections,tags,templates,links,embedding_sets
    include: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationHistoryEntry {
    from_version: String,
    to_version: String,
    migrated_at: chrono::DateTime<chrono::Utc>,
    migrated_by: String, // e.g., "matric-memory/2026.1.12"
    changes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShardManifest {
    /// Shard format version (e.g., "1.0.0")
    version: String,
    /// matric-memory application version that created this shard (e.g., "2026.1.12")
    #[serde(default)]
    matric_version: Option<String>,
    format: String,
    created_at: chrono::DateTime<chrono::Utc>,
    components: Vec<String>,
    counts: ShardCounts,
    checksums: std::collections::HashMap<String, String>,

    /// Minimum matric-memory version required to read this shard
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min_reader_version: Option<String>,

    /// Original schema version if this shard was migrated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    migrated_from: Option<String>,

    /// History of migrations applied to this shard
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    migration_history: Vec<MigrationHistoryEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ShardCounts {
    notes: usize,
    collections: usize,
    tags: usize,
    templates: usize,
    links: usize,
    embedding_sets: usize,
    embedding_set_members: usize,
    embeddings: usize,
    embedding_configs: usize,
}

/// Create a knowledge shard (portable tar.gz export) with selected components.
#[utoipa::path(get, path = "/api/v1/backup/knowledge-shard", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn knowledge_shard(
    State(state): State<AppState>,
    Query(query): Query<ShardExportQuery>,
) -> Result<impl IntoResponse, ApiError> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use matric_core::NoteRepository;
    use sha2::{Digest, Sha256};

    use tar::Builder;

    // Parse included components
    let include_str = query
        .include
        .unwrap_or_else(|| "notes,collections,tags,templates,links,embedding_sets".to_string());
    let components: Vec<&str> = include_str.split(',').map(|s| s.trim()).collect();

    let mut counts = ShardCounts::default();
    let mut checksums: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Create tar.gz in memory
    let mut shard_data = Vec::new();
    {
        let encoder = GzEncoder::new(&mut shard_data, Compression::default());
        let mut tar = Builder::new(encoder);

        // Helper to add JSON file to shard
        let mut add_json_file = |name: &str, data: &[u8]| -> std::io::Result<()> {
            // Calculate checksum
            let mut hasher = Sha256::new();
            hasher.update(data);
            let hash = hex::encode(hasher.finalize());
            checksums.insert(name.to_string(), hash);

            // Create header and add to tar
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(chrono::Utc::now().timestamp() as u64);
            header.set_cksum();
            tar.append_data(&mut header, name, data)?;
            Ok(())
        };

        // Export notes
        if components.contains(&"notes") {
            let list_req = ListNotesRequest {
                limit: Some(100000),
                ..Default::default()
            };
            let notes_response = state.db.notes.list(list_req).await?;
            let mut notes_json = Vec::new();

            for note in &notes_response.notes {
                if let Ok(full_note) = state.db.notes.fetch(note.id).await {
                    let note_tags = state
                        .db
                        .tags
                        .get_for_note(note.id)
                        .await
                        .unwrap_or_default();
                    let note_obj = serde_json::json!({
                        "id": full_note.note.id,
                        "title": full_note.note.title,
                        "original_content": full_note.original.content,
                        "revised_content": full_note.revised.content,
                        "format": full_note.note.format,
                        "source": full_note.note.source,
                        "starred": full_note.note.starred,
                        "archived": full_note.note.archived,
                        "collection_id": full_note.note.collection_id,
                        "created_at": full_note.note.created_at_utc,
                        "updated_at": full_note.note.updated_at_utc,
                        "tags": note_tags,
                    });
                    notes_json.push(serde_json::to_string(&note_obj).unwrap_or_default());
                }
            }
            counts.notes = notes_json.len();
            let notes_data = notes_json.join("\n").into_bytes();
            add_json_file("notes.jsonl", &notes_data).map_err(|e| {
                ApiError::BadRequest(format!("Failed to add notes to shard: {}", e))
            })?;
        }

        // Export collections
        if components.contains(&"collections") {
            let collections = state.db.collections.list(None).await.unwrap_or_default();
            counts.collections = collections.len();
            let collections_json: Vec<serde_json::Value> = collections
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "description": c.description,
                        "parent_id": c.parent_id,
                        "created_at": c.created_at_utc,
                        "note_count": c.note_count,
                    })
                })
                .collect();
            let data = serde_json::to_vec_pretty(&collections_json).unwrap_or_default();
            add_json_file("collections.json", &data)
                .map_err(|e| ApiError::BadRequest(format!("Failed to add collections: {}", e)))?;
        }

        // Export tags
        if components.contains(&"tags") {
            let tags = state.db.tags.list().await.unwrap_or_default();
            counts.tags = tags.len();
            let tags_json: Vec<serde_json::Value> = tags
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "created_at": t.created_at_utc,
                    })
                })
                .collect();
            let data = serde_json::to_vec_pretty(&tags_json).unwrap_or_default();
            add_json_file("tags.json", &data)
                .map_err(|e| ApiError::BadRequest(format!("Failed to add tags: {}", e)))?;
        }

        // Export templates
        if components.contains(&"templates") {
            let templates = {
                let ctx = state.db.for_schema("public")?;
                let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
                ctx.query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
                    .await
            }
            .unwrap_or_default();
            counts.templates = templates.len();
            let templates_json: Vec<serde_json::Value> = templates
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "name": t.name,
                        "description": t.description,
                        "content": t.content,
                        "format": t.format,
                        "default_tags": t.default_tags,
                        "collection_id": t.collection_id,
                        "created_at": t.created_at_utc,
                        "updated_at": t.updated_at_utc,
                    })
                })
                .collect();
            let data = serde_json::to_vec_pretty(&templates_json).unwrap_or_default();
            add_json_file("templates.json", &data)
                .map_err(|e| ApiError::BadRequest(format!("Failed to add templates: {}", e)))?;
        }

        // Export links
        if components.contains(&"links") {
            let links = state.db.links.list_all(100000, 0).await.unwrap_or_default();
            counts.links = links.len();
            let mut links_jsonl = Vec::new();
            for link in &links {
                let link_obj = serde_json::json!({
                    "id": link.id,
                    "from_note_id": link.from_note_id,
                    "to_note_id": link.to_note_id,
                    "to_url": link.to_url,
                    "kind": link.kind,
                    "score": link.score,
                    "created_at": link.created_at_utc,
                    "metadata": link.metadata,
                });
                links_jsonl.push(serde_json::to_string(&link_obj).unwrap_or_default());
            }
            let data = links_jsonl.join("\n").into_bytes();
            add_json_file("links.jsonl", &data)
                .map_err(|e| ApiError::BadRequest(format!("Failed to add links: {}", e)))?;
        }

        // Export embedding sets
        if components.contains(&"embedding_sets") {
            // Export set definitions
            let sets = state.db.embedding_sets.list().await.unwrap_or_default();
            counts.embedding_sets = sets.len();
            let sets_json: Vec<serde_json::Value> = sets
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "name": s.name,
                        "slug": s.slug,
                        "description": s.description,
                        "purpose": s.purpose,
                        "document_count": s.document_count,
                        "embedding_count": s.embedding_count,
                        "is_system": s.is_system,
                        "keywords": s.keywords,
                        "model": s.model,
                        "dimension": s.dimension,
                    })
                })
                .collect();
            let data = serde_json::to_vec_pretty(&sets_json).unwrap_or_default();
            add_json_file("embedding_sets.json", &data).map_err(|e| {
                ApiError::BadRequest(format!("Failed to add embedding sets: {}", e))
            })?;

            // Export set members
            let members = state
                .db
                .embedding_sets
                .list_all_members(100000, 0)
                .await
                .unwrap_or_default();
            counts.embedding_set_members = members.len();
            let mut members_jsonl = Vec::new();
            for m in &members {
                let member_obj = serde_json::json!({
                    "embedding_set_id": m.embedding_set_id,
                    "note_id": m.note_id,
                    "membership_type": m.membership_type,
                    "added_at": m.added_at,
                    "added_by": m.added_by,
                });
                members_jsonl.push(serde_json::to_string(&member_obj).unwrap_or_default());
            }
            let data = members_jsonl.join("\n").into_bytes();
            add_json_file("embedding_set_members.jsonl", &data).map_err(|e| {
                ApiError::BadRequest(format!("Failed to add embedding set members: {}", e))
            })?;

            // Export embedding configs
            let configs = state
                .db
                .embedding_sets
                .list_configs()
                .await
                .unwrap_or_default();
            counts.embedding_configs = configs.len();
            let configs_json: Vec<serde_json::Value> = configs
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "description": c.description,
                        "model": c.model,
                        "dimension": c.dimension,
                        "chunk_size": c.chunk_size,
                        "chunk_overlap": c.chunk_overlap,
                        "is_default": c.is_default,
                    })
                })
                .collect();
            let data = serde_json::to_vec_pretty(&configs_json).unwrap_or_default();
            add_json_file("embedding_configs.json", &data).map_err(|e| {
                ApiError::BadRequest(format!("Failed to add embedding configs: {}", e))
            })?;
        }

        // Export embeddings (optional, can be large)
        if components.contains(&"embeddings") {
            let embeddings = state
                .db
                .embeddings
                .list_all(100000, 0)
                .await
                .unwrap_or_default();
            counts.embeddings = embeddings.len();
            let mut embeddings_jsonl = Vec::new();
            for emb in &embeddings {
                let emb_obj = serde_json::json!({
                    "id": emb.id,
                    "note_id": emb.note_id,
                    "chunk_index": emb.chunk_index,
                    "text": emb.text,
                    "vector": emb.vector.as_slice(),
                    "model": emb.model,
                });
                embeddings_jsonl.push(serde_json::to_string(&emb_obj).unwrap_or_default());
            }
            let data = embeddings_jsonl.join("\n").into_bytes();
            add_json_file("embeddings.jsonl", &data)
                .map_err(|e| ApiError::BadRequest(format!("Failed to add embeddings: {}", e)))?;
        }

        // Create manifest (added last)
        let manifest = ShardManifest {
            version: "1.0.0".to_string(),
            matric_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            format: "matric-shard".to_string(),
            created_at: chrono::Utc::now(),
            components: components.iter().map(|s| s.to_string()).collect(),
            counts,
            checksums: checksums.clone(),
            min_reader_version: Some("1.0.0".to_string()),
            migrated_from: None,
            migration_history: vec![],
        };
        let manifest_data = serde_json::to_vec_pretty(&manifest).unwrap_or_default();

        // Add manifest to shard
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(chrono::Utc::now().timestamp() as u64);
        header.set_cksum();
        tar.append_data(&mut header, "manifest.json", manifest_data.as_slice())
            .map_err(|e| ApiError::BadRequest(format!("Failed to add manifest: {}", e)))?;

        // Finalize tar
        tar.finish()
            .map_err(|e| ApiError::BadRequest(format!("Failed to finalize shard: {}", e)))?;
    }

    // Generate filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("matric-shard-{}.shard", timestamp);

    // Return as downloadable file
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/gzip".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename)
            .parse()
            .unwrap(),
    );

    Ok((StatusCode::OK, headers, shard_data))
}

// =============================================================================
// ARCHIVE IMPORT (Full restore from knowledge shard)
// =============================================================================

#[derive(Debug, Deserialize)]
struct ShardImportBody {
    /// Base64-encoded knowledge shard data
    shard_base64: String,
    /// Components to import (comma-separated). If not specified, imports all available.
    include: Option<String>,
    /// Dry run - validate without importing
    #[serde(default)]
    dry_run: bool,
    /// Conflict resolution strategy for notes
    #[serde(default)]
    on_conflict: ConflictStrategy,
    /// Whether to skip embedding regeneration (use imported embeddings)
    #[serde(default)]
    skip_embedding_regen: bool,
}

#[derive(Debug, Serialize)]
struct ShardImportResponse {
    status: String,
    manifest: Option<ShardManifest>,
    imported: ShardImportCounts,
    skipped: ShardImportCounts,
    errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    dry_run: bool,
}

#[derive(Debug, Serialize, Default)]
struct ShardImportCounts {
    notes: usize,
    collections: usize,
    tags: usize,
    templates: usize,
    links: usize,
    embedding_sets: usize,
    embedding_set_members: usize,
    embeddings: usize,
}

/// Import a full knowledge shard from tar.gz.
#[utoipa::path(post, path = "/api/v1/backup/knowledge-shard/import", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn knowledge_shard_import(
    State(state): State<AppState>,
    Json(body): Json<ShardImportBody>,
) -> Result<impl IntoResponse, ApiError> {
    use base64::Engine;
    use flate2::read::GzDecoder;
    use matric_core::CreateNoteRequest;
    use tar::Archive;

    // Decode base64 shard
    let shard_bytes = base64::engine::general_purpose::STANDARD
        .decode(&body.shard_base64)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64 data: {}", e)))?;

    // Decompress gzip
    let decoder = GzDecoder::new(shard_bytes.as_slice());
    let mut tar_reader = Archive::new(decoder);

    // Parse included components filter
    let include_filter: Option<Vec<String>> = body
        .include
        .as_ref()
        .map(|s| s.split(',').map(|c| c.trim().to_lowercase()).collect());

    let mut imported = ShardImportCounts::default();
    let mut skipped = ShardImportCounts::default();
    let mut errors: Vec<String> = Vec::new();
    let mut manifest: Option<ShardManifest> = None;

    // First pass: read all entries into memory for processing
    let mut files: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();

    for entry_result in tar_reader
        .entries()
        .map_err(|e| ApiError::BadRequest(format!("Failed to read shard: {}", e)))?
    {
        let mut entry = entry_result
            .map_err(|e| ApiError::BadRequest(format!("Failed to read shard entry: {}", e)))?;

        let path = entry
            .path()
            .map_err(|e| ApiError::BadRequest(format!("Invalid path in shard: {}", e)))?;
        let filename = path.to_string_lossy().to_string();

        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut contents).map_err(|e| {
            ApiError::BadRequest(format!("Failed to read entry {}: {}", filename, e))
        })?;

        files.insert(filename, contents);
    }

    // Parse manifest first
    let mut warnings: Vec<String> = Vec::new();
    if let Some(manifest_data) = files.get("manifest.json") {
        match serde_json::from_slice::<ShardManifest>(manifest_data) {
            Ok(m) => {
                // Check for version mismatch
                let current_version = env!("CARGO_PKG_VERSION");
                if let Some(ref shard_version) = m.matric_version {
                    if shard_version != current_version {
                        warnings.push(format!(
                            "Version mismatch: shard created with matric-memory v{}, importing with v{}",
                            shard_version, current_version
                        ));
                    }
                } else {
                    warnings.push(
                        "Shard created with older matric-memory version (no version info in manifest)".to_string()
                    );
                }
                manifest = Some(m);
            }
            Err(e) => errors.push(format!("Failed to parse manifest: {}", e)),
        }
    }

    // Helper to check if component should be imported
    let should_import = |component: &str| -> bool {
        match &include_filter {
            Some(filter) => filter.contains(&component.to_lowercase()),
            None => true, // Import all if no filter specified
        }
    };

    // Import notes
    if should_import("notes") {
        if let Some(notes_data) = files.get("notes.jsonl") {
            let notes_str = String::from_utf8_lossy(notes_data);
            for line in notes_str.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(note_json) => {
                        let content = note_json
                            .get("original_content")
                            .or(note_json.get("content"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if content.is_empty() {
                            skipped.notes += 1;
                            continue;
                        }

                        // Check for existing note by ID
                        let existing_id = note_json
                            .get("id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok());

                        if let Some(id) = existing_id {
                            if state.db.notes.exists(id).await.unwrap_or(false) {
                                match body.on_conflict {
                                    ConflictStrategy::Skip => {
                                        skipped.notes += 1;
                                        continue;
                                    }
                                    ConflictStrategy::Replace => {
                                        if !body.dry_run {
                                            let _ = state.db.notes.soft_delete(id).await;
                                        }
                                    }
                                    ConflictStrategy::Merge => {
                                        skipped.notes += 1;
                                        continue;
                                    }
                                }
                            }
                        }

                        if !body.dry_run {
                            let req = CreateNoteRequest {
                                content,
                                format: note_json
                                    .get("format")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("markdown")
                                    .to_string(),
                                source: note_json
                                    .get("source")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("shard-import")
                                    .to_string(),
                                collection_id: note_json
                                    .get("collection_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok()),
                                tags: note_json.get("tags").and_then(|v| v.as_array()).map(|arr| {
                                    arr.iter()
                                        .filter_map(|t| t.as_str().map(String::from))
                                        .collect::<Vec<_>>()
                                }),
                                metadata: None,
                                document_type_id: None,
                            };

                            match state.db.notes.insert(req).await {
                                Ok(new_id) => {
                                    // Update status if specified
                                    let starred = note_json
                                        .get("starred")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    let archived = note_json
                                        .get("archived")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    if starred || archived {
                                        let status_req = matric_core::UpdateNoteStatusRequest {
                                            starred: Some(starred),
                                            archived: Some(archived),
                                            metadata: None,
                                        };
                                        let _ =
                                            state.db.notes.update_status(new_id, status_req).await;
                                    }

                                    // Update revised content if available
                                    if let Some(revised) =
                                        note_json.get("revised_content").and_then(|v| v.as_str())
                                    {
                                        if !revised.is_empty() {
                                            let _ = state
                                                .db
                                                .notes
                                                .update_revised(
                                                    new_id,
                                                    revised,
                                                    Some("Imported from shard"),
                                                )
                                                .await;
                                        }
                                    }

                                    // Queue NLP pipeline if not skipping regen
                                    if !body.skip_embedding_regen {
                                        queue_nlp_pipeline(
                                            &state.db,
                                            new_id,
                                            RevisionMode::None,
                                            &state.event_bus,
                                            None,
                                        )
                                        .await;
                                    }

                                    imported.notes += 1;
                                }
                                Err(e) => {
                                    errors.push(format!("Failed to import note: {}", e));
                                }
                            }
                        } else {
                            imported.notes += 1;
                        }
                    }
                    Err(e) => {
                        errors.push(format!("Invalid note JSON: {}", e));
                    }
                }
            }
        }
    }

    // Import collections
    if should_import("collections") {
        if let Some(collections_data) = files.get("collections.json") {
            match serde_json::from_slice::<Vec<serde_json::Value>>(collections_data) {
                Ok(collections) => {
                    for coll in collections {
                        let name = coll.get("name").and_then(|v| v.as_str());
                        let id = coll
                            .get("id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok());

                        if let (Some(name), Some(_id)) = (name, id) {
                            if !body.dry_run {
                                match state
                                    .db
                                    .collections
                                    .create(
                                        name,
                                        coll.get("description").and_then(|v| v.as_str()),
                                        coll.get("parent_id")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| Uuid::parse_str(s).ok()),
                                    )
                                    .await
                                {
                                    Ok(_) => imported.collections += 1,
                                    Err(e) => errors.push(format!(
                                        "Failed to import collection {}: {}",
                                        name, e
                                    )),
                                }
                            } else {
                                imported.collections += 1;
                            }
                        }
                    }
                }
                Err(e) => errors.push(format!("Failed to parse collections: {}", e)),
            }
        }
    }

    // Import templates
    if should_import("templates") {
        if let Some(templates_data) = files.get("templates.json") {
            match serde_json::from_slice::<Vec<serde_json::Value>>(templates_data) {
                Ok(templates) => {
                    for tmpl in templates {
                        if let (Some(name), Some(content)) = (
                            tmpl.get("name").and_then(|v| v.as_str()),
                            tmpl.get("content").and_then(|v| v.as_str()),
                        ) {
                            if !body.dry_run {
                                let req = matric_core::CreateTemplateRequest {
                                    name: name.to_string(),
                                    description: tmpl
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                    content: content.to_string(),
                                    format: Some(
                                        tmpl.get("format")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("markdown")
                                            .to_string(),
                                    ),
                                    default_tags: tmpl
                                        .get("default_tags")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|t| t.as_str().map(String::from))
                                                .collect::<Vec<_>>()
                                        }),
                                    collection_id: tmpl
                                        .get("collection_id")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| Uuid::parse_str(s).ok()),
                                };

                                let res = {
                                    let ctx = state.db.for_schema("public")?;
                                    let templates =
                                        matric_db::PgTemplateRepository::new(state.db.pool.clone());
                                    ctx.execute(move |tx| {
                                        Box::pin(async move { templates.create_tx(tx, req).await })
                                    })
                                    .await
                                };
                                match res {
                                    Ok(_) => imported.templates += 1,
                                    Err(e) => errors
                                        .push(format!("Failed to import template {}: {}", name, e)),
                                }
                            } else {
                                imported.templates += 1;
                            }
                        }
                    }
                }
                Err(e) => errors.push(format!("Failed to parse templates: {}", e)),
            }
        }
    }

    // Import links (if embeddings are being skipped, links help preserve graph structure)
    if should_import("links") {
        if let Some(links_data) = files.get("links.jsonl") {
            let links_str = String::from_utf8_lossy(links_data);
            for line in links_str.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(link_json) => {
                        let from_id = link_json
                            .get("from_note_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok());
                        let to_id = link_json
                            .get("to_note_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok());
                        let kind = link_json
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("semantic");
                        let score = link_json
                            .get("score")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.7) as f32;

                        if let (Some(from_id), Some(to_id)) = (from_id, to_id) {
                            if !body.dry_run {
                                let metadata = link_json.get("metadata").cloned();
                                match state
                                    .db
                                    .links
                                    .create(from_id, to_id, kind, score, metadata)
                                    .await
                                {
                                    Ok(_) => imported.links += 1,
                                    Err(_) => skipped.links += 1, // Link may already exist
                                }
                            } else {
                                imported.links += 1;
                            }
                        }
                    }
                    Err(e) => errors.push(format!("Invalid link JSON: {}", e)),
                }
            }
        }
    }

    // Note: Embedding sets, members, and raw embeddings import would require
    // additional repository methods. For now, we skip these as they can be
    // regenerated from the notes.

    if should_import("embedding_sets") && files.contains_key("embedding_sets.json") {
        warnings.push(
            "Embedding set import not yet implemented - sets will be regenerated".to_string(),
        );
    }

    if should_import("embeddings") && files.contains_key("embeddings.jsonl") {
        warnings.push(
            "Direct embedding import not yet implemented - embeddings will be regenerated"
                .to_string(),
        );
    }

    let status = if errors.is_empty() {
        "success".to_string()
    } else if imported.notes > 0 || imported.collections > 0 || imported.templates > 0 {
        "partial".to_string()
    } else {
        "failed".to_string()
    };

    Ok(Json(ShardImportResponse {
        status,
        manifest,
        imported,
        skipped,
        errors,
        warnings,
        dry_run: body.dry_run,
    }))
}

// =============================================================================
// FILE ATTACHMENT HANDLERS
// =============================================================================

/// Query parameters for attachment upload endpoints
#[derive(Debug, Deserialize)]
struct UploadAttachmentQuery {
    /// Optional explicit document type override via query parameter
    document_type_id: Option<Uuid>,
}

/// Request body for uploading file attachments
#[derive(Debug, Deserialize)]
struct UploadAttachmentBody {
    filename: String,
    content_type: String,
    /// Base64-encoded file data
    data: String,
    /// Optional explicit document type override (skips auto-detection)
    document_type_id: Option<Uuid>,
}

/// List all attachments for a note
#[utoipa::path(get, path = "/api/v1/notes/{id}/attachments", tag = "Attachments",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 200, description = "Success")))]
async fn list_attachments(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;
    let attachments = file_storage.list_by_note_tx(&mut tx, id).await?;
    drop(tx);
    Ok(Json(attachments))
}

/// Upload a file attachment to a note
#[utoipa::path(post, path = "/api/v1/notes/{id}/attachments", tag = "Attachments",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 201, description = "Created")))]
async fn upload_attachment(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(att_query): Query<UploadAttachmentQuery>,
    Json(body): Json<UploadAttachmentBody>,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    // Decode base64 data
    let data = base64::engine::general_purpose::STANDARD
        .decode(&body.data)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64 data: {}", e)))?;

    // Validate file safety — block executables and dangerous file types (fixes #241)
    let validation =
        matric_core::validate_file(&body.filename, &data, state.max_upload_size as u64);
    if !validation.allowed {
        return Err(ApiError::BadRequest(
            validation
                .block_reason
                .unwrap_or_else(|| "File blocked by safety policy".to_string()),
        ));
    }

    // Validate MIME content type format (issue #308)
    if !matric_core::is_valid_mime_type(&body.content_type) {
        return Err(ApiError::BadRequest(format!(
            "Invalid content_type: '{}' is not a valid MIME type (expected type/subtype format)",
            body.content_type
        )));
    }

    // Detect actual content type from magic bytes (fixes #253)
    let content_type = matric_core::detect_content_type(&body.filename, &data, &body.content_type);

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;

    // Store the file
    let mut attachment = file_storage
        .store_file_tx(&mut tx, id, &body.filename, &content_type, &data)
        .await?;

    // Phase 1: Determine extraction strategy from MIME type (pure function, no DB)
    let ext = std::path::Path::new(&body.filename)
        .extension()
        .and_then(|e| e.to_str());
    let strategy = ExtractionStrategy::from_mime_and_extension(&content_type, ext);
    file_storage
        .set_extraction_strategy_tx(&mut tx, attachment.id, strategy)
        .await?;
    attachment.extraction_strategy = Some(strategy);

    // Validate the document_type_id exists; reject if invalid (issue #309)
    // Query parameter takes priority over body field (issue #334)
    let effective_doc_type_id = att_query.document_type_id.or(body.document_type_id);
    if let Some(doc_type_id) = effective_doc_type_id {
        if state.db.document_types.get(doc_type_id).await?.is_some() {
            file_storage
                .set_document_type_tx(&mut tx, attachment.id, doc_type_id, None)
                .await?;
            attachment.document_type_id = Some(doc_type_id);
        } else {
            return Err(ApiError::BadRequest(format!(
                "Invalid document_type_id: {doc_type_id} does not reference a valid document type"
            )));
        }
    }
    // Document type classification happens asynchronously after extraction (Phase 2)

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Queue background extraction job for the uploaded attachment
    queue_extraction_job(
        &state.db,
        id,
        attachment.id,
        strategy,
        &body.filename,
        &content_type,
        &state.event_bus,
        Some(&archive_ctx.schema),
    )
    .await;

    // Queue EXIF extraction for image uploads (extracts camera info, GPS, datetime → provenance)
    queue_exif_extraction_job(
        &state.db,
        id,
        attachment.id,
        &content_type,
        &state.event_bus,
        Some(&archive_ctx.schema),
    )
    .await;

    Ok(Json(attachment))
}

/// Upload a file attachment via multipart/form-data.
///
/// Expects a multipart form with:
/// - `file`: the file data (required)
/// - `document_type_id`: optional UUID for explicit document type override
#[utoipa::path(post, path = "/api/v1/notes/{id}/attachments/upload", tag = "Attachments",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses((status = 201, description = "Created")))]
async fn upload_attachment_multipart(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(id): Path<Uuid>,
    Query(att_query): Query<UploadAttachmentQuery>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut data: Option<Vec<u8>> = None;
    let mut document_type_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let field_name = field.name().map(|n| n.to_string());
        match field_name.as_deref() {
            Some("file") => {
                filename = field.file_name().map(|n| n.to_string());
                content_type = field.content_type().map(|c| c.to_string());
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?
                        .to_vec(),
                );
            }
            Some("document_type_id") => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Read error: {}", e)))?;
                document_type_id = val.parse::<Uuid>().ok();
            }
            _ => {} // ignore unknown fields
        }
    }

    let filename = filename
        .ok_or_else(|| ApiError::BadRequest("Missing file in multipart form".to_string()))?;
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let data = data
        .ok_or_else(|| ApiError::BadRequest("Missing file data in multipart form".to_string()))?;

    // Validate MIME content type format (issue #308)
    if !matric_core::is_valid_mime_type(&content_type) {
        return Err(ApiError::BadRequest(format!(
            "Invalid content_type: '{}' is not a valid MIME type (expected type/subtype format)",
            content_type
        )));
    }

    // Validate file safety — block executables and dangerous file types (fixes #241)
    let validation = matric_core::validate_file(&filename, &data, state.max_upload_size as u64);
    if !validation.allowed {
        return Err(ApiError::BadRequest(
            validation
                .block_reason
                .unwrap_or_else(|| "File blocked by safety policy".to_string()),
        ));
    }

    // Detect actual content type from magic bytes (fixes #253)
    let content_type = matric_core::detect_content_type(&filename, &data, &content_type);

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;

    // Store the file
    let mut attachment = file_storage
        .store_file_tx(&mut tx, id, &filename, &content_type, &data)
        .await?;

    // Set extraction strategy (propagate errors — swallowing SQL errors aborts the PG transaction)
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str());
    let strategy = ExtractionStrategy::from_mime_and_extension(&content_type, ext);
    file_storage
        .set_extraction_strategy_tx(&mut tx, attachment.id, strategy)
        .await?;
    attachment.extraction_strategy = Some(strategy);

    // Validate the document_type_id exists; reject if invalid (issue #309)
    // Query parameter takes priority over form field (issue #334)
    let effective_doc_type_id = att_query.document_type_id.or(document_type_id);
    if let Some(doc_type_id) = effective_doc_type_id {
        if state.db.document_types.get(doc_type_id).await?.is_some() {
            file_storage
                .set_document_type_tx(&mut tx, attachment.id, doc_type_id, None)
                .await?;
            attachment.document_type_id = Some(doc_type_id);
        } else {
            return Err(ApiError::BadRequest(format!(
                "Invalid document_type_id: {doc_type_id} does not reference a valid document type"
            )));
        }
    }

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Queue background extraction job for the uploaded attachment
    queue_extraction_job(
        &state.db,
        id,
        attachment.id,
        strategy,
        &filename,
        &content_type,
        &state.event_bus,
        Some(&archive_ctx.schema),
    )
    .await;

    // Queue EXIF extraction for image uploads (extracts camera info, GPS, datetime → provenance)
    queue_exif_extraction_job(
        &state.db,
        id,
        attachment.id,
        &content_type,
        &state.event_bus,
        Some(&archive_ctx.schema),
    )
    .await;

    Ok(Json(attachment))
}

/// Get attachment metadata
#[utoipa::path(get, path = "/api/v1/attachments/{attachment_id}", tag = "Attachments",
    params(("attachment_id" = Uuid, Path, description = "Attachment ID")),
    responses((status = 200, description = "Success")))]
async fn get_attachment(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(attachment_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;
    let attachment = file_storage.get_tx(&mut tx, attachment_id).await?;
    drop(tx);
    Ok(Json(attachment))
}

/// Download a file attachment (returns raw binary with proper content headers).
///
/// Clients can use `curl -o filename URL` to download directly.
#[utoipa::path(get, path = "/api/v1/attachments/{attachment_id}/download", tag = "Attachments",
    params(("attachment_id" = Uuid, Path, description = "Attachment ID")),
    responses((status = 200, description = "Success")))]
async fn download_attachment(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(attachment_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;
    let (data, content_type, filename) = file_storage
        .download_file_tx(&mut tx, attachment_id)
        .await?;
    drop(tx);

    // Return raw binary with proper Content-Type and Content-Disposition headers
    let headers = [
        (
            axum::http::header::CONTENT_TYPE,
            content_type
                .parse::<axum::http::HeaderValue>()
                .unwrap_or_else(|_| {
                    axum::http::HeaderValue::from_static("application/octet-stream")
                }),
        ),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename.replace('"', "\\\""))
                .parse::<axum::http::HeaderValue>()
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
        ),
    ];

    Ok((headers, data))
}

/// Delete an attachment
#[utoipa::path(delete, path = "/api/v1/attachments/{attachment_id}", tag = "Attachments",
    params(("attachment_id" = Uuid, Path, description = "Attachment ID")),
    responses((status = 204, description = "No Content")))]
async fn delete_attachment(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(attachment_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let file_storage = state
        .db
        .file_storage
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("File storage not configured".to_string()))?;

    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;
    file_storage.delete_tx(&mut tx, attachment_id).await?;
    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Attachment deleted successfully"
    })))
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

#[derive(Debug)]
#[allow(dead_code)]
enum ApiError {
    Database(matric_core::Error),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    Internal(String),
    ServiceUnavailable(String),
}

impl From<matric_core::Error> for ApiError {
    fn from(err: matric_core::Error) -> Self {
        match &err {
            matric_core::Error::NotFound(msg) => ApiError::NotFound(msg.clone()),
            matric_core::Error::InvalidInput(msg) => ApiError::BadRequest(msg.clone()),
            matric_core::Error::Database(sqlx_err) => {
                let msg = sqlx_err.to_string();
                if msg.contains("duplicate key") || msg.contains("unique constraint") {
                    // Provide user-friendly error messages for known constraints
                    let friendly_msg =
                        if msg.contains("idx_unique_pref_label") || msg.contains("pref_label") {
                            "A concept with this prefLabel already exists in the scheme".to_string()
                        } else if msg.contains("valid_notation") || msg.contains("notation") {
                            "A concept with this notation already exists in the scheme".to_string()
                        } else if msg.contains("idx_unique_tag_name") || msg.contains("tag_name") {
                            "A tag with this name already exists".to_string()
                        } else if msg.contains("collection_name_key")
                            || msg.contains("collection") && msg.contains("unique")
                        {
                            "A collection with this name already exists".to_string()
                        } else {
                            // Generic friendly message - never expose raw constraint names
                            "A record with this value already exists".to_string()
                        };
                    return ApiError::Conflict(friendly_msg);
                }
                if msg.contains("Polyhierarchy limit") {
                    return ApiError::BadRequest(msg);
                }
                if msg.contains("foreign key") {
                    // Provide user-friendly messages for FK violations
                    let friendly_msg = if msg.contains("job_queue_note_id_fkey")
                        || msg.contains("job_queue") && msg.contains("note")
                    {
                        "Note not found".to_string()
                    } else if msg.contains("note_tag") {
                        "Referenced note or tag not found".to_string()
                    } else {
                        "Referenced resource not found".to_string()
                    };
                    return ApiError::BadRequest(friendly_msg);
                }
                ApiError::Database(err)
            }
            _ => ApiError::Database(err),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::Database(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

// =============================================================================
// BACKUP ARCHIVE BROWSER
// =============================================================================

/// Info about a single knowledge shard file
#[derive(Debug, Serialize)]
struct BackupShardInfo {
    filename: String,
    path: String,
    size_bytes: u64,
    size_human: String,
    modified: chrono::DateTime<chrono::Utc>,
    /// ISO 8601 timestamp string for easy display
    modified_iso: String,
    /// Shard type: snapshot, upload, prerestore, shard (tar.gz), pgdump, json_export, metadata, unknown
    shard_type: String,
    /// SHA256 hash of the file (first 16 chars for display)
    sha256_short: Option<String>,
    /// Full SHA256 hash
    sha256: Option<String>,
    /// Manifest info if available (for knowledge shards)
    manifest: Option<ShardManifest>,
    /// Associated metadata sidecar file (Issue #218)
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata_file: Option<String>,
    /// Title from metadata sidecar (Issue #218)
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// Description from metadata sidecar (Issue #218)
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListBackupArchivesResponse {
    backup_directory: String,
    shards: Vec<BackupShardInfo>,
    total_size_bytes: u64,
    total_size_human: String,
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// List all knowledge shards in the backup directory.
/// Issue #257: Use consistent shard_type values
/// Issue #218: Bundle primary files with metadata sidecars
#[utoipa::path(get, path = "/api/v1/backup/list", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn list_backups(State(_state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    use std::collections::HashMap;
    use std::fs;

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    let backup_path = std::path::Path::new(&backup_dir);
    if !backup_path.exists() {
        return Ok(Json(ListBackupArchivesResponse {
            backup_directory: backup_dir,
            shards: vec![],
            total_size_bytes: 0,
            total_size_human: "0 B".to_string(),
        }));
    }

    // Issue #257: Helper function for consistent shard_type detection
    fn detect_shard_type(name: &str) -> Option<&'static str> {
        if name.ends_with(".meta.json") {
            return None; // Skip metadata sidecars in main listing
        }
        if name.ends_with(".tar.gz") {
            Some("shard")
        } else if name.ends_with(".sql.gz") || name.ends_with(".sql") {
            if name.starts_with("snapshot_") {
                Some("snapshot")
            } else if name.starts_with("prerestore_") {
                Some("prerestore")
            } else if name.starts_with("upload_") {
                Some("upload")
            } else {
                Some("pgdump")
            }
        } else if name.ends_with(".json") {
            Some("json_export")
        } else {
            None
        }
    }

    // Issue #218: First pass - collect metadata sidecars
    let mut metadata_map: HashMap<String, serde_json::Value> = HashMap::new();
    if let Ok(entries) = fs::read_dir(backup_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".meta.json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Map primary filename -> metadata
                            let primary_name = name.trim_end_matches(".meta.json");
                            metadata_map.insert(primary_name.to_string(), json);
                        }
                    }
                }
            }
        }
    }

    let mut shards = Vec::new();
    let mut total_size = 0u64;

    if let Ok(entries) = fs::read_dir(backup_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let shard_type = match detect_shard_type(name) {
                    Some(t) => t,
                    None => continue, // Skip metadata sidecars and unknown files
                };

                if let Ok(meta) = entry.metadata() {
                    let size = meta.len();
                    total_size += size;

                    let modified = meta
                        .modified()
                        .map(|t| {
                            let duration =
                                t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                            chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                                .unwrap_or_else(chrono::Utc::now)
                        })
                        .unwrap_or_else(|_| chrono::Utc::now());

                    // Calculate SHA256 hash (only for reasonably sized files)
                    let (sha256, sha256_short) = if size < 500_000_000 {
                        // < 500MB
                        calculate_file_sha256(&path)
                            .map(|h| (Some(h.clone()), Some(h[..16].to_string())))
                            .unwrap_or((None, None))
                    } else {
                        (None, None)
                    };

                    // Try to extract manifest from knowledge shards
                    let manifest = if shard_type == "shard" {
                        extract_manifest_from_shard(&path).ok()
                    } else {
                        None
                    };

                    // Issue #218: Bundle with metadata sidecar
                    let (metadata_file, title, description) =
                        if let Some(meta_json) = metadata_map.get(name) {
                            let meta_filename = format!("{}.meta.json", name);
                            let title = meta_json
                                .get("title")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            let description = meta_json
                                .get("description")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            (Some(meta_filename), title, description)
                        } else {
                            (None, None, None)
                        };

                    shards.push(BackupShardInfo {
                        filename: name.to_string(),
                        path: path.to_string_lossy().to_string(),
                        size_bytes: size,
                        size_human: format_size(size),
                        modified,
                        modified_iso: modified.to_rfc3339(),
                        shard_type: shard_type.to_string(),
                        sha256_short,
                        sha256,
                        manifest,
                        metadata_file,
                        title,
                        description,
                    });
                }
            }
        }
    }

    // Sort by modified date, newest first
    shards.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(Json(ListBackupArchivesResponse {
        backup_directory: backup_dir,
        shards,
        total_size_bytes: total_size,
        total_size_human: format_size(total_size),
    }))
}

/// Calculate SHA256 hash of a file.
fn calculate_file_sha256(path: &std::path::Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::{BufReader, Read};

    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer).ok()?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Some(hex::encode(hasher.finalize()))
}

/// Extract manifest from a knowledge shard without loading entire file.
fn extract_manifest_from_shard(path: &std::path::Path) -> Result<ShardManifest, String> {
    use flate2::read::GzDecoder;
    use std::fs::File;
    use tar::Archive;

    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = GzDecoder::new(file);
    let mut tar_reader = Archive::new(decoder);

    for entry in tar_reader.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path().map_err(|e| e.to_string())?;

        if entry_path.to_string_lossy() == "manifest.json" {
            use std::io::Read;
            let mut contents = String::new();
            entry
                .read_to_string(&mut contents)
                .map_err(|e| e.to_string())?;
            return serde_json::from_str(&contents).map_err(|e| e.to_string());
        }
    }

    Err("No manifest.json found in shard".to_string())
}

/// Get detailed info about a specific knowledge shard.
#[utoipa::path(get, path = "/api/v1/backup/list/{filename}", tag = "Backup",
    params(("filename" = String, Path, description = "Backup filename")),
    responses((status = 200, description = "Success")))]
async fn get_backup_info(
    State(_state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use std::fs;

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: ensure filename doesn't contain path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let path = std::path::Path::new(&backup_dir).join(&filename);
    if !path.exists() {
        return Err(ApiError::NotFound(format!(
            "Archive not found: {}",
            filename
        )));
    }

    let meta = fs::metadata(&path)
        .map_err(|e| ApiError::BadRequest(format!("Cannot read file: {}", e)))?;

    // Issue #257: Use consistent shard_type values
    let shard_type = if filename.ends_with(".tar.gz") {
        "shard"
    } else if filename.ends_with(".sql.gz") || filename.ends_with(".sql") {
        if filename.starts_with("snapshot_") {
            "snapshot"
        } else if filename.starts_with("prerestore_") {
            "prerestore"
        } else if filename.starts_with("upload_") {
            "upload"
        } else {
            "pgdump"
        }
    } else if filename.ends_with(".meta.json") {
        "metadata"
    } else if filename.ends_with(".json") {
        "json_export"
    } else {
        "unknown"
    };

    let modified = meta
        .modified()
        .map(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                .unwrap_or_else(chrono::Utc::now)
        })
        .unwrap_or_else(|_| chrono::Utc::now());

    let manifest = if shard_type == "shard" {
        extract_manifest_from_shard(&path).ok()
    } else {
        None
    };

    // Calculate SHA256
    let (sha256, sha256_short) = calculate_file_sha256(&path)
        .map(|h| (Some(h.clone()), Some(h[..16].to_string())))
        .unwrap_or((None, None));

    // Issue #218: Check for metadata sidecar
    let meta_path = std::path::Path::new(&backup_dir).join(format!("{}.meta.json", filename));
    let (metadata_file, title, description) = if meta_path.exists() {
        if let Ok(content) = fs::read_to_string(&meta_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let title = json.get("title").and_then(|v| v.as_str()).map(String::from);
                let description = json
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                (Some(format!("{}.meta.json", filename)), title, description)
            } else {
                (Some(format!("{}.meta.json", filename)), None, None)
            }
        } else {
            (Some(format!("{}.meta.json", filename)), None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(Json(BackupShardInfo {
        filename,
        path: path.to_string_lossy().to_string(),
        size_bytes: meta.len(),
        size_human: format_size(meta.len()),
        modified,
        modified_iso: modified.to_rfc3339(),
        shard_type: shard_type.to_string(),
        sha256_short,
        sha256,
        manifest,
        metadata_file,
        title,
        description,
    }))
}

// =============================================================================
// BACKUP SWAP (HOT RESTORE)
// =============================================================================

#[derive(Debug, Deserialize)]
struct SwapBackupRequest {
    /// Filename of the shard to restore from
    filename: String,
    /// If true, just validate without actually restoring
    dry_run: Option<bool>,
    /// What to do with existing data: "wipe" (default) or "merge"
    strategy: Option<String>,
}

#[derive(Debug, Serialize)]
struct SwapBackupResponse {
    status: String,
    message: String,
    /// Stats about what was/would be restored
    stats: Option<ShardImportCounts>,
    dry_run: bool,
}

/// Swap to a different backup (restore from shard file on disk).
#[utoipa::path(post, path = "/api/v1/backup/swap", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn swap_backup(
    State(state): State<AppState>,
    Json(req): Json<SwapBackupRequest>,
) -> Result<impl IntoResponse, ApiError> {
    use std::fs::File;
    use std::io::Read;

    let dry_run = req.dry_run.unwrap_or(false);
    let strategy = req.strategy.as_deref().unwrap_or("wipe");

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: ensure filename doesn't contain path traversal
    if req.filename.contains("..") || req.filename.contains('/') || req.filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let path = std::path::Path::new(&backup_dir).join(&req.filename);
    if !path.exists() {
        return Err(ApiError::NotFound(format!(
            "Archive not found: {}",
            req.filename
        )));
    }

    // Only support knowledge shards for now
    if !req.filename.ends_with(".tar.gz") {
        return Err(ApiError::BadRequest(
            "Only knowledge shards are supported for swap. Use pg_restore for .sql.gz files."
                .to_string(),
        ));
    }

    // Read shard file
    let mut file =
        File::open(&path).map_err(|e| ApiError::BadRequest(format!("Cannot read shard: {}", e)))?;
    let mut shard_data = Vec::new();
    file.read_to_end(&mut shard_data)
        .map_err(|e| ApiError::BadRequest(format!("Cannot read shard: {}", e)))?;

    // Encode as base64 for the import handler
    let shard_base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &shard_data);

    // If strategy is "wipe", purge existing data first
    if strategy == "wipe" && !dry_run {
        // Purge all notes (which cascades to tags, embeddings, links)
        let list_req = ListNotesRequest {
            limit: Some(100000),
            filter: Some("all".to_string()), // Include archived
            ..Default::default()
        };
        let notes = state.db.notes.list(list_req).await?;
        for note in notes.notes {
            let _ = state.db.notes.hard_delete(note.id).await;
        }

        // Purge collections
        for coll in state.db.collections.list(None).await.unwrap_or_default() {
            let _ = state.db.collections.delete(coll.id).await;
        }

        // Purge templates
        let templates: Vec<matric_core::NoteTemplate> = {
            let ctx = state.db.for_schema("public")?;
            let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
            ctx.query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
                .await
        }
        .unwrap_or_default();
        for tmpl in templates {
            let _ = {
                let ctx = state.db.for_schema("public")?;
                let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
                let id = tmpl.id;
                ctx.execute(move |tx| Box::pin(async move { templates.delete_tx(tx, id).await }))
                    .await
            };
        }
    }

    // Import from shard
    let import_body = ShardImportBody {
        shard_base64,
        include: None,
        dry_run,
        on_conflict: ConflictStrategy::Replace,
        skip_embedding_regen: false,
    };

    // Call the shard import logic
    let result = knowledge_shard_import_internal(&state, import_body).await?;

    Ok(Json(SwapBackupResponse {
        status: result.status.clone(),
        message: if dry_run {
            format!(
                "Dry run: would restore {} notes, {} collections, {} templates, {} links",
                result.imported.notes,
                result.imported.collections,
                result.imported.templates,
                result.imported.links
            )
        } else {
            format!(
                "Restored {} notes, {} collections, {} templates, {} links",
                result.imported.notes,
                result.imported.collections,
                result.imported.templates,
                result.imported.links
            )
        },
        stats: Some(result.imported),
        dry_run,
    }))
}

/// Internal shard import function (reused by both endpoints).
async fn knowledge_shard_import_internal(
    state: &AppState,
    body: ShardImportBody,
) -> Result<ShardImportResponse, ApiError> {
    use flate2::read::GzDecoder;
    use std::collections::HashMap;
    use std::io::Read;
    use tar::Archive;

    // Decode base64 shard
    let shard_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &body.shard_base64,
    )
    .map_err(|e| ApiError::BadRequest(format!("Invalid base64: {}", e)))?;

    // Parse tar.gz
    let decoder = GzDecoder::new(&shard_bytes[..]);
    let mut tar_reader = Archive::new(decoder);

    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    for entry in tar_reader
        .entries()
        .map_err(|e| ApiError::BadRequest(format!("Invalid tar: {}", e)))?
    {
        let mut entry =
            entry.map_err(|e| ApiError::BadRequest(format!("Invalid tar entry: {}", e)))?;
        let entry_path = entry
            .path()
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let name = entry_path.to_string_lossy().to_string();

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        files.insert(name, contents);
    }

    // Parse manifest and check version
    let mut warnings: Vec<String> = Vec::new();
    let manifest = files
        .get("manifest.json")
        .and_then(|data| serde_json::from_slice::<ShardManifest>(data).ok());

    // Check for version mismatch
    if let Some(ref m) = manifest {
        let current_version = env!("CARGO_PKG_VERSION");
        if let Some(ref shard_version) = m.matric_version {
            if shard_version != current_version {
                warnings.push(format!(
                    "Version mismatch: shard created with matric-memory v{}, importing with v{}",
                    shard_version, current_version
                ));
            }
        } else {
            warnings.push(
                "Shard created with older matric-memory version (no version info in manifest)"
                    .to_string(),
            );
        }
    }

    let mut imported = ShardImportCounts::default();
    let mut skipped = ShardImportCounts::default();
    let mut errors: Vec<String> = Vec::new();

    // Determine what to import
    let include_str = body
        .include
        .as_deref()
        .unwrap_or("notes,collections,tags,templates,links");
    let components: Vec<&str> = include_str.split(',').map(|s| s.trim()).collect();
    let should_import = |c: &str| components.contains(&c) || components.contains(&"all");

    let on_conflict = &body.on_conflict;

    // Import notes
    if should_import("notes") {
        if let Some(notes_data) = files.get("notes.jsonl") {
            let notes_str = String::from_utf8_lossy(notes_data);
            for line in notes_str.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(note_json) => {
                        let original_id = note_json
                            .get("id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok());
                        let content = note_json
                            .get("original_content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        if content.is_empty() {
                            continue;
                        }

                        // Check if note exists
                        let exists = if let Some(id) = original_id {
                            state.db.notes.fetch(id).await.is_ok()
                        } else {
                            false
                        };

                        if exists {
                            match on_conflict {
                                ConflictStrategy::Skip => {
                                    skipped.notes += 1;
                                    continue;
                                }
                                ConflictStrategy::Replace => {
                                    if let Some(id) = original_id {
                                        if !body.dry_run {
                                            let _ = state.db.notes.hard_delete(id).await;
                                        }
                                    }
                                }
                                ConflictStrategy::Merge => {} // Keep existing, just add new
                            }
                        }

                        if !body.dry_run {
                            let req = CreateNoteRequest {
                                content: content.to_string(),
                                format: note_json
                                    .get("format")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("markdown")
                                    .to_string(),
                                source: note_json
                                    .get("source")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("import")
                                    .to_string(),
                                collection_id: note_json
                                    .get("collection_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok()),
                                tags: note_json.get("tags").and_then(|v| v.as_array()).map(|arr| {
                                    arr.iter()
                                        .filter_map(|t| t.as_str().map(String::from))
                                        .collect::<Vec<_>>()
                                }),
                                metadata: None,
                                document_type_id: None,
                            };

                            match state.db.notes.insert(req).await {
                                Ok(new_id) => {
                                    // Update starred status if present
                                    if let Some(starred) =
                                        note_json.get("starred").and_then(|v| v.as_bool())
                                    {
                                        let status_req = UpdateNoteStatusRequest {
                                            starred: Some(starred),
                                            archived: None,
                                            metadata: None,
                                        };
                                        let _ =
                                            state.db.notes.update_status(new_id, status_req).await;
                                    }
                                    // Update revised content if available
                                    if let Some(revised) =
                                        note_json.get("revised_content").and_then(|v| v.as_str())
                                    {
                                        if !revised.is_empty() {
                                            let _ = state
                                                .db
                                                .notes
                                                .update_revised(new_id, revised, Some("Imported"))
                                                .await;
                                        }
                                    }
                                    if !body.skip_embedding_regen {
                                        queue_nlp_pipeline(
                                            &state.db,
                                            new_id,
                                            RevisionMode::None,
                                            &state.event_bus,
                                            None,
                                        )
                                        .await;
                                    }
                                    imported.notes += 1;
                                }
                                Err(e) => errors.push(format!("Note import failed: {}", e)),
                            }
                        } else {
                            imported.notes += 1;
                        }
                    }
                    Err(e) => errors.push(format!("Invalid note JSON: {}", e)),
                }
            }
        }
    }

    // Import collections
    if should_import("collections") {
        if let Some(data) = files.get("collections.json") {
            if let Ok(collections) = serde_json::from_slice::<Vec<serde_json::Value>>(data) {
                for coll in collections {
                    if let Some(name) = coll.get("name").and_then(|v| v.as_str()) {
                        if !body.dry_run {
                            match state
                                .db
                                .collections
                                .create(
                                    name,
                                    coll.get("description").and_then(|v| v.as_str()),
                                    coll.get("parent_id")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| Uuid::parse_str(s).ok()),
                                )
                                .await
                            {
                                Ok(_) => imported.collections += 1,
                                Err(_) => skipped.collections += 1,
                            }
                        } else {
                            imported.collections += 1;
                        }
                    }
                }
            }
        }
    }

    // Import templates
    if should_import("templates") {
        if let Some(data) = files.get("templates.json") {
            if let Ok(templates) = serde_json::from_slice::<Vec<serde_json::Value>>(data) {
                for tmpl in templates {
                    if let (Some(name), Some(content)) = (
                        tmpl.get("name").and_then(|v| v.as_str()),
                        tmpl.get("content").and_then(|v| v.as_str()),
                    ) {
                        if !body.dry_run {
                            let req = matric_core::CreateTemplateRequest {
                                name: name.to_string(),
                                description: tmpl
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                content: content.to_string(),
                                format: Some(
                                    tmpl.get("format")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("markdown")
                                        .to_string(),
                                ),
                                default_tags: tmpl
                                    .get("default_tags")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|t| t.as_str().map(String::from))
                                            .collect()
                                    }),
                                collection_id: tmpl
                                    .get("collection_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok()),
                            };
                            let res = {
                                let ctx = state.db.for_schema("public")?;
                                let templates =
                                    matric_db::PgTemplateRepository::new(state.db.pool.clone());
                                ctx.execute(move |tx| {
                                    Box::pin(async move { templates.create_tx(tx, req).await })
                                })
                                .await
                            };
                            match res {
                                Ok(_) => imported.templates += 1,
                                Err(_) => skipped.templates += 1,
                            }
                        } else {
                            imported.templates += 1;
                        }
                    }
                }
            }
        }
    }

    // Import links
    if should_import("links") {
        if let Some(data) = files.get("links.jsonl") {
            let links_str = String::from_utf8_lossy(data);
            for line in links_str.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(link) = serde_json::from_str::<serde_json::Value>(line) {
                    if let (Some(from_id), Some(to_id)) = (
                        link.get("from_note_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok()),
                        link.get("to_note_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok()),
                    ) {
                        if !body.dry_run {
                            let kind = link
                                .get("kind")
                                .and_then(|v| v.as_str())
                                .unwrap_or("semantic");
                            let score =
                                link.get("score").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
                            match state
                                .db
                                .links
                                .create(from_id, to_id, kind, score, None)
                                .await
                            {
                                Ok(_) => imported.links += 1,
                                Err(_) => skipped.links += 1,
                            }
                        } else {
                            imported.links += 1;
                        }
                    }
                }
            }
        }
    }

    let status = if errors.is_empty() {
        "success"
    } else if imported.notes > 0 {
        "partial"
    } else {
        "failed"
    };

    Ok(ShardImportResponse {
        status: status.to_string(),
        manifest,
        imported,
        skipped,
        errors,
        warnings,
        dry_run: body.dry_run,
    })
}

// =============================================================================
// DATABASE BACKUP HANDLERS (Full pg_dump with embeddings)
// =============================================================================

/// Backup naming prefixes for identification
mod backup_prefix {
    pub const AUTO: &str = "auto"; // Automated/scheduled backup
    pub const SNAPSHOT: &str = "snapshot"; // User-requested snapshot
    pub const PRERESTORE: &str = "prerestore"; // Auto-created before restore
    pub const UPLOAD: &str = "upload"; // Uploaded by user
}

/// Metadata for a backup file (stored as .meta.json sidecar file)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupMetadata {
    /// Display title for the backup
    title: String,
    /// Detailed description of backup contents/purpose
    description: Option<String>,
    /// Backup type (snapshot, upload, prerestore, auto)
    backup_type: String,
    /// When the backup was created
    created_at: chrono::DateTime<chrono::Utc>,
    /// Number of notes at time of backup (if known)
    note_count: Option<i64>,
    /// Total database size at time of backup (if known)
    db_size_bytes: Option<i64>,
    /// System-generated or user-provided
    source: String,
    /// Additional key-value metadata
    #[serde(default)]
    extra: std::collections::HashMap<String, String>,

    // Version compatibility fields (issue #416)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version_min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matric_version_max: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pg_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_migration_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_migration: Option<String>,
}

impl BackupMetadata {
    /// Create metadata for an automated backup
    #[allow(dead_code)] // Reserved for scheduled backup feature
    fn auto(note_count: Option<i64>, db_size_bytes: Option<i64>) -> Self {
        Self {
            title: format!(
                "Automated backup {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M")
            ),
            description: Some("Scheduled backup created by matric-backup service".to_string()),
            backup_type: backup_prefix::AUTO.to_string(),
            created_at: chrono::Utc::now(),
            note_count,
            db_size_bytes,
            source: "system".to_string(),
            extra: Default::default(),
            matric_version: None,
            matric_version_min: None,
            matric_version_max: None,
            pg_version: None,
            schema_migration_count: None,
            last_migration: None,
        }
    }

    /// Create metadata for a user snapshot
    fn snapshot(
        title: Option<String>,
        description: Option<String>,
        note_count: Option<i64>,
    ) -> Self {
        Self {
            title: title.unwrap_or_else(|| {
                format!("Snapshot {}", chrono::Utc::now().format("%Y-%m-%d %H:%M"))
            }),
            description,
            backup_type: backup_prefix::SNAPSHOT.to_string(),
            created_at: chrono::Utc::now(),
            note_count,
            db_size_bytes: None,
            source: "user".to_string(),
            extra: Default::default(),
            matric_version: None,
            matric_version_min: None,
            matric_version_max: None,
            pg_version: None,
            schema_migration_count: None,
            last_migration: None,
        }
    }

    /// Create metadata for a pre-restore backup
    fn prerestore(restoring_from: &str, note_count: Option<i64>) -> Self {
        Self {
            title: "Pre-restore backup".to_string(),
            description: Some(format!(
                "Auto-created before restoring from: {}",
                restoring_from
            )),
            backup_type: backup_prefix::PRERESTORE.to_string(),
            created_at: chrono::Utc::now(),
            note_count,
            db_size_bytes: None,
            source: "system".to_string(),
            extra: [("restoring_from".to_string(), restoring_from.to_string())]
                .into_iter()
                .collect(),
            matric_version: None,
            matric_version_min: None,
            matric_version_max: None,
            pg_version: None,
            schema_migration_count: None,
            last_migration: None,
        }
    }

    /// Create metadata for an uploaded backup
    fn upload(title: Option<String>, description: Option<String>, original_filename: &str) -> Self {
        Self {
            title: title.unwrap_or_else(|| format!("Uploaded: {}", original_filename)),
            description,
            backup_type: backup_prefix::UPLOAD.to_string(),
            created_at: chrono::Utc::now(),
            note_count: None,
            db_size_bytes: None,
            source: "user".to_string(),
            extra: [(
                "original_filename".to_string(),
                original_filename.to_string(),
            )]
            .into_iter()
            .collect(),
            matric_version: None,
            matric_version_min: None,
            matric_version_max: None,
            pg_version: None,
            schema_migration_count: None,
            last_migration: None,
        }
    }

    /// Save metadata to sidecar file
    fn save(&self, backup_path: &std::path::Path) -> std::io::Result<()> {
        let meta_path = backup_path.with_extension(format!(
            "{}.meta.json",
            backup_path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&meta_path, json)?;
        Ok(())
    }

    /// Load metadata from sidecar file
    fn load(backup_path: &std::path::Path) -> Option<Self> {
        let meta_path = backup_path.with_extension(format!(
            "{}.meta.json",
            backup_path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Populate version compatibility information from database.
    /// Call this after creating metadata to add version fields.
    async fn populate_version_info(
        &mut self,
        pool: &sqlx::Pool<sqlx::Postgres>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get matric-memory version from Cargo.toml
        self.matric_version = Some(env!("CARGO_PKG_VERSION").to_string());
        self.matric_version_min = Some(env!("CARGO_PKG_VERSION").to_string());
        // matric_version_max stays None (no upper bound)

        // Query PostgreSQL version
        if let Ok(row) = sqlx::query_scalar::<_, String>("SELECT version()")
            .fetch_one(pool)
            .await
        {
            self.pg_version = Some(row);
        }

        // Query migration information
        if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
        {
            self.schema_migration_count = Some(count as i32);
        }

        // Get last migration name
        if let Ok(migration) = sqlx::query_scalar::<_, String>(
            "SELECT description FROM _sqlx_migrations ORDER BY installed_on DESC LIMIT 1",
        )
        .fetch_one(pool)
        .await
        {
            self.last_migration = Some(migration);
        }

        Ok(())
    }
}

/// Issue #242: Metadata echo in backup response
#[derive(Debug, Serialize)]
struct BackupMetadataEcho {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    metadata_file: String,
}

#[derive(Debug, Serialize)]
struct DatabaseBackupResponse {
    success: bool,
    filename: String,
    path: String,
    size_bytes: u64,
    size_human: String,
    backup_type: String,
    created_at: chrono::DateTime<chrono::Utc>,
    /// Issue #242: Echo metadata when title/description provided
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<BackupMetadataEcho>,
}

#[derive(Debug, Deserialize)]
struct SnapshotRequest {
    /// Optional name for the snapshot (will be sanitized for filename)
    name: Option<String>,
    /// Human-readable title for the backup
    title: Option<String>,
    /// Detailed description of the backup
    description: Option<String>,
}

/// Download a fresh database backup (pg_dump).
/// Download a backup of a single memory (archive schema).
///
/// Uses `pg_dump --schema` to export just the memory's tables, producing a
/// much smaller backup than a full database dump.
#[utoipa::path(get, path = "/api/v1/backup/memory/{name}", tag = "Backup",
    params(("name" = String, Path, description = "Memory name")),
    responses((status = 200, description = "Success")))]
async fn memory_backup_download(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use matric_core::ArchiveRepository;

    // Look up the archive to get the schema name
    let archive = state
        .db
        .archives
        .get_archive_by_name(&name)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to look up memory: {}", e)))?
        .ok_or_else(|| ApiError::NotFound(format!("Memory not found: {}", name)))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("memory_{}_{}.sql.gz", name, timestamp);

    // Run pg_dump with --schema to export only this memory's schema
    let output = std::process::Command::new("pg_dump")
        .args([
            "-U",
            "matric",
            "-h",
            "localhost",
            "--schema",
            &archive.schema_name,
            "matric",
        ])
        .env("PGPASSWORD", "matric")
        .output()
        .map_err(|e| ApiError::BadRequest(format!("pg_dump failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::BadRequest(format!("pg_dump error: {}", stderr)));
    }

    // Compress with gzip
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&output.stdout)
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;
    let compressed = encoder
        .finish()
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;

    let headers = [
        (header::CONTENT_TYPE, "application/gzip".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        ),
    ];

    Ok((headers, compressed))
}

#[utoipa::path(get, path = "/api/v1/backup/database", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn database_backup_download(
    State(_state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{}_database_{}.sql.gz", backup_prefix::SNAPSHOT, timestamp);

    // Run pg_dump and stream output
    let output = std::process::Command::new("pg_dump")
        .args(["-U", "matric", "-h", "localhost", "matric"])
        .env("PGPASSWORD", "matric")
        .output()
        .map_err(|e| ApiError::BadRequest(format!("pg_dump failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::BadRequest(format!("pg_dump error: {}", stderr)));
    }

    // Compress with gzip
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&output.stdout)
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;
    let compressed = encoder
        .finish()
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;

    let headers = [
        (header::CONTENT_TYPE, "application/gzip".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        ),
    ];

    Ok((headers, compressed))
}

/// Create a named snapshot and save to backup directory.
#[utoipa::path(post, path = "/api/v1/backup/database/snapshot", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn database_backup_snapshot(
    State(state): State<AppState>,
    Json(req): Json<SnapshotRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Ensure backup directory exists
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| ApiError::BadRequest(format!("Cannot create backup dir: {}", e)))?;

    let timestamp = chrono::Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S");

    // Get note count for metadata
    let note_count = state
        .db
        .notes
        .list(ListNotesRequest {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .map(|r| r.total)
        .ok();

    // Sanitize optional name
    let name_suffix = req
        .name
        .map(|n| {
            format!(
                "_{}",
                n.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .take(32)
                    .collect::<String>()
            )
        })
        .unwrap_or_default();

    let filename = format!(
        "{}_database_{}{}.sql.gz",
        backup_prefix::SNAPSHOT,
        ts_str,
        name_suffix
    );
    let path = std::path::Path::new(&backup_dir).join(&filename);

    // Run pg_dump
    let output = std::process::Command::new("pg_dump")
        .args(["-U", "matric", "-h", "localhost", "matric"])
        .env("PGPASSWORD", "matric")
        .output()
        .map_err(|e| ApiError::BadRequest(format!("pg_dump failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::BadRequest(format!("pg_dump error: {}", stderr)));
    }

    // Compress and save
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let file = std::fs::File::create(&path)
        .map_err(|e| ApiError::BadRequest(format!("Cannot create file: {}", e)))?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder
        .write_all(&output.stdout)
        .map_err(|e| ApiError::BadRequest(format!("Write failed: {}", e)))?;
    encoder
        .finish()
        .map_err(|e| ApiError::BadRequest(format!("Compression failed: {}", e)))?;

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    // Issue #242: Clone title/description for echo before moving into metadata
    let echo_title = req.title.clone();
    let echo_description = req.description.clone();

    // Save metadata sidecar file
    let mut metadata = BackupMetadata::snapshot(req.title, req.description, note_count);
    if let Err(e) = metadata.populate_version_info(&state.db.pool).await {
        tracing::warn!("Failed to populate version info: {}", e);
    }
    if let Err(e) = metadata.save(&path) {
        tracing::warn!("Failed to save backup metadata: {}", e);
    }

    // Issue #242: Build metadata echo if title or description provided
    let metadata_echo = if echo_title.is_some() || echo_description.is_some() {
        Some(BackupMetadataEcho {
            title: echo_title,
            description: echo_description,
            metadata_file: format!("{}.meta.json", filename),
        })
    } else {
        None
    };

    Ok(Json(DatabaseBackupResponse {
        success: true,
        filename,
        path: path.to_string_lossy().to_string(),
        size_bytes: size,
        size_human: format_size(size),
        backup_type: "snapshot".to_string(),
        created_at: timestamp,
        metadata: metadata_echo,
    }))
}

#[derive(Debug, Deserialize)]
struct DatabaseUploadRequest {
    /// Base64-encoded .sql.gz file
    data_base64: String,
    /// Original filename (for reference)
    original_filename: Option<String>,
    /// Human-readable title for the backup
    title: Option<String>,
    /// Detailed description of the backup
    description: Option<String>,
}

/// Upload a database backup file (adds to backup list, does not restore).
#[utoipa::path(post, path = "/api/v1/backup/database/upload", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn database_backup_upload(
    State(_state): State<AppState>,
    Json(req): Json<DatabaseUploadRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| ApiError::BadRequest(format!("Cannot create backup dir: {}", e)))?;

    // Decode base64
    let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.data_base64)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64: {}", e)))?;

    let timestamp = chrono::Utc::now();
    let ts_str = timestamp.format("%Y%m%d_%H%M%S");

    // Create filename with upload prefix
    let original_filename = req
        .original_filename
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let orig_suffix = req
        .original_filename
        .map(|n| {
            let sanitized: String = n
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                .take(50)
                .collect();
            format!(
                "_{}",
                sanitized
                    .trim_end_matches(".sql.gz")
                    .trim_end_matches(".sql")
            )
        })
        .unwrap_or_default();

    let filename = format!(
        "{}_database_{}{}.sql.gz",
        backup_prefix::UPLOAD,
        ts_str,
        orig_suffix
    );
    let path = std::path::Path::new(&backup_dir).join(&filename);

    // Write file
    std::fs::write(&path, &data)
        .map_err(|e| ApiError::BadRequest(format!("Cannot write file: {}", e)))?;

    // Issue #242: Clone title/description for echo before moving into metadata
    let echo_title = req.title.clone();
    let echo_description = req.description.clone();

    // Save metadata sidecar file
    let metadata = BackupMetadata::upload(req.title, req.description, &original_filename);
    if let Err(e) = metadata.save(&path) {
        tracing::warn!("Failed to save backup metadata: {}", e);
    }

    // Issue #242: Build metadata echo if title or description provided
    let metadata_echo = if echo_title.is_some() || echo_description.is_some() {
        Some(BackupMetadataEcho {
            title: echo_title,
            description: echo_description,
            metadata_file: format!("{}.meta.json", filename),
        })
    } else {
        None
    };

    Ok(Json(DatabaseBackupResponse {
        success: true,
        filename,
        path: path.to_string_lossy().to_string(),
        size_bytes: data.len() as u64,
        size_human: format_size(data.len() as u64),
        backup_type: "upload".to_string(),
        created_at: timestamp,
        metadata: metadata_echo,
    }))
}

#[derive(Debug, Deserialize)]
struct DatabaseRestoreRequest {
    /// Filename of the backup to restore
    filename: String,
    /// Skip creating a pre-restore snapshot (not recommended)
    #[serde(default)]
    skip_snapshot: bool,
    /// Optional: restore into a specific memory (archive schema).
    /// Uses clean DROP SCHEMA CASCADE + recreate instead of object enumeration.
    /// If omitted, restores to the public schema (full database restore).
    #[serde(default)]
    memory: Option<String>,
}

#[derive(Debug, Serialize)]
struct DatabaseRestoreResponse {
    success: bool,
    message: String,
    prerestore_backup: Option<String>,
    restored_from: String,
    /// Time to wait for DB reconnection
    reconnect_delay_ms: u64,
}

/// Restore a backup into a specific memory using DROP SCHEMA CASCADE.
///
/// This is dramatically simpler than public schema restore because:
/// 1. DROP SCHEMA CASCADE cleanly removes everything (no object enumeration)
/// 2. The dump SQL recreates the schema and all objects
/// 3. No risk of leftover objects from previous state
async fn memory_scoped_restore(
    state: &AppState,
    backup_path: &std::path::Path,
    filename: &str,
    memory_name: &str,
) -> Result<Json<DatabaseRestoreResponse>, ApiError> {
    use matric_core::ArchiveRepository;

    // Look up the archive to get the schema name
    let archive = state
        .db
        .archives
        .get_archive_by_name(memory_name)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to look up memory: {}", e)))?
        .ok_or_else(|| ApiError::NotFound(format!("Memory not found: {}", memory_name)))?;

    let schema_name = archive.schema_name.clone();

    // Guard: never DROP SCHEMA CASCADE on the public/default archive
    if archive.is_default || schema_name == "public" {
        return Err(ApiError::BadRequest(
            "Cannot restore over the default archive using memory-scoped restore. \
             Use the standard restore endpoint (without 'memory' parameter) instead."
                .to_string(),
        ));
    }

    // Decompress if needed
    let sql_content = if filename.ends_with(".sql.gz") {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let file = std::fs::File::open(backup_path)
            .map_err(|e| ApiError::BadRequest(format!("Cannot open backup: {}", e)))?;
        let mut decoder = GzDecoder::new(file);
        let mut content = String::new();
        decoder
            .read_to_string(&mut content)
            .map_err(|e| ApiError::BadRequest(format!("Cannot decompress: {}", e)))?;
        content
    } else {
        std::fs::read_to_string(backup_path)
            .map_err(|e| ApiError::BadRequest(format!("Cannot read backup: {}", e)))?
    };

    // Step 1: Drop the schema (CASCADE removes all tables, indexes, triggers, etc.)
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema_name))
        .execute(state.db.pool())
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to drop schema: {}", e)))?;

    // Step 2: Feed the dump SQL to psql.
    // The dump (from pg_dump --schema) will CREATE SCHEMA + all objects.
    let output = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new("psql")
            .args(["-U", "matric", "-h", "localhost", "-d", "matric"])
            .env("PGPASSWORD", "matric")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(sql_content.as_bytes());
            }
        });

        let output = child.wait_with_output();
        let _ = writer.join();
        output
    })
    .await
    .map_err(|e| ApiError::BadRequest(format!("psql task panicked: {}", e)))?
    .map_err(|e| ApiError::BadRequest(format!("psql failed: {}", e)))?;

    let success = output.status.success();

    // Step 3: If restore failed, recreate empty archive tables so the schema is usable
    if !success {
        tracing::warn!(
            "Memory restore for '{}' had issues, ensuring schema is usable",
            memory_name
        );
        // Recreate from public schema as fallback
        if let Err(e) = state
            .db
            .archives
            .create_archive_schema(memory_name, archive.description.as_deref())
            .await
        {
            tracing::error!(
                "Failed to recreate archive schema after failed restore: {}",
                e
            );
        }
    }

    // Step 4: Update schema version after restore
    if let Err(e) = state.db.archives.sync_archive_schema(memory_name).await {
        tracing::warn!("Failed to sync schema version after restore: {}", e);
    }

    Ok(Json(DatabaseRestoreResponse {
        success,
        message: if success {
            format!("Memory '{}' restored from {}", memory_name, filename)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "Memory restore may have issues: {}",
                stderr.chars().take(500).collect::<String>()
            )
        },
        prerestore_backup: None,
        restored_from: filename.to_string(),
        reconnect_delay_ms: 0,
    }))
}

/// Restore from a database backup file.
/// Creates a pre-restore snapshot first (unless skip_snapshot=true).
/// The API will attempt to reconnect after restore.
#[utoipa::path(post, path = "/api/v1/backup/database/restore", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn database_backup_restore(
    State(state): State<AppState>,
    Json(req): Json<DatabaseRestoreRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: prevent path traversal
    if req.filename.contains("..") || req.filename.contains('/') || req.filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let backup_path = std::path::Path::new(&backup_dir).join(&req.filename);
    if !backup_path.exists() {
        return Err(ApiError::NotFound(format!(
            "Backup not found: {}",
            req.filename
        )));
    }

    // Must be a .sql.gz or .sql file
    if !req.filename.ends_with(".sql.gz") && !req.filename.ends_with(".sql") {
        return Err(ApiError::BadRequest(
            "Only .sql.gz or .sql files can be restored".to_string(),
        ));
    }

    // Memory-scoped restore: use clean DROP SCHEMA CASCADE approach
    if let Some(ref memory_name) = req.memory {
        return memory_scoped_restore(&state, &backup_path, &req.filename, memory_name).await;
    }

    // Get current note count for metadata
    let note_count = state
        .db
        .notes
        .list(ListNotesRequest {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .map(|r| r.total)
        .ok();

    // Step 1: Create pre-restore snapshot
    let prerestore_filename = if !req.skip_snapshot {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!(
            "{}_database_{}.sql.gz",
            backup_prefix::PRERESTORE,
            timestamp
        );
        let prerestore_path = std::path::Path::new(&backup_dir).join(&filename);

        let output = std::process::Command::new("pg_dump")
            .args(["-U", "matric", "-h", "localhost", "matric"])
            .env("PGPASSWORD", "matric")
            .output()
            .map_err(|e| ApiError::BadRequest(format!("Pre-restore snapshot failed: {}", e)))?;

        if output.status.success() {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let file = std::fs::File::create(&prerestore_path)
                .map_err(|e| ApiError::BadRequest(format!("Cannot create snapshot: {}", e)))?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            let _ = encoder.write_all(&output.stdout);
            let _ = encoder.finish();

            // Save metadata for pre-restore backup
            let metadata = BackupMetadata::prerestore(&req.filename, note_count);
            if let Err(e) = metadata.save(&prerestore_path) {
                tracing::warn!("Failed to save prerestore metadata: {}", e);
            }
        }

        Some(filename)
    } else {
        None
    };

    // Step 2: Perform restore
    // First, decompress if needed
    let sql_content = if req.filename.ends_with(".sql.gz") {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let file = std::fs::File::open(&backup_path)
            .map_err(|e| ApiError::BadRequest(format!("Cannot open backup: {}", e)))?;
        let mut decoder = GzDecoder::new(file);
        let mut content = String::new();
        decoder
            .read_to_string(&mut content)
            .map_err(|e| ApiError::BadRequest(format!("Cannot decompress: {}", e)))?;
        content
    } else {
        std::fs::read_to_string(&backup_path)
            .map_err(|e| ApiError::BadRequest(format!("Cannot read backup: {}", e)))?
    };

    // Run psql to restore (drop and recreate).
    // IMPORTANT: stdin writing and stdout/stderr reading MUST happen concurrently.
    // The DROP script produces NOTICE messages for every dropped object, and the
    // dump SQL produces output for every CREATE/COPY. If stdout/stderr fills the
    // 64KB pipe buffer while we're still writing to stdin, both sides deadlock.
    // Solution: write stdin in a separate thread while wait_with_output() drains
    // stdout/stderr on the main thread.
    let output = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new("psql")
            .args(["-U", "matric", "-h", "localhost", "-d", "matric"])
            .env("PGPASSWORD", "matric")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Write to stdin in a separate thread to avoid pipe deadlock.
        let stdin = child.stdin.take();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            if let Some(mut stdin) = stdin {
                // First drop all user tables and types for a clean restore.
                // Extension-owned tables (e.g. spatial_ref_sys from PostGIS) must be
                // excluded — they cannot be dropped without dropping the extension itself,
                // and the dump will skip them via CREATE EXTENSION IF NOT EXISTS.
                let drop_script = r#"
-- Drop all user tables (exclude extension-owned like spatial_ref_sys)
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT c.relname AS tablename
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
          AND NOT EXISTS (
              SELECT 1 FROM pg_depend d
              WHERE d.objid = c.oid AND d.deptype = 'e'
          )
    ) LOOP
        EXECUTE 'DROP TABLE IF EXISTS public.' || quote_ident(r.tablename) || ' CASCADE';
    END LOOP;
END $$;

-- Drop all custom enum types so the dump can recreate them
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT t.typname
        FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = 'public' AND t.typtype = 'e'
    ) LOOP
        EXECUTE 'DROP TYPE IF EXISTS public.' || quote_ident(r.typname) || ' CASCADE';
    END LOOP;
END $$;

-- Drop all user-defined functions so the dump can recreate them
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT p.oid, p.proname, pg_get_function_identity_arguments(p.oid) AS args
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND NOT EXISTS (
              SELECT 1 FROM pg_depend d
              WHERE d.objid = p.oid AND d.deptype = 'e'
          )
    ) LOOP
        EXECUTE 'DROP FUNCTION IF EXISTS public.' || quote_ident(r.proname)
                || '(' || r.args || ') CASCADE';
    END LOOP;
END $$;

-- Drop all text search configurations (e.g. matric_english from migrations)
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT cfgname
        FROM pg_ts_config c
        JOIN pg_namespace n ON n.oid = c.cfgnamespace
        WHERE n.nspname = 'public'
    ) LOOP
        EXECUTE 'DROP TEXT SEARCH CONFIGURATION IF EXISTS public.' || quote_ident(r.cfgname) || ' CASCADE';
    END LOOP;
END $$;

-- Drop all text search dictionaries (referenced by configs above)
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT dictname
        FROM pg_ts_dict d
        JOIN pg_namespace n ON n.oid = d.dictnamespace
        WHERE n.nspname = 'public'
    ) LOOP
        EXECUTE 'DROP TEXT SEARCH DICTIONARY IF EXISTS public.' || quote_ident(r.dictname) || ' CASCADE';
    END LOOP;
END $$;

-- Drop all views and materialized views
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT c.relname, c.relkind
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind IN ('v', 'm')
          AND NOT EXISTS (
              SELECT 1 FROM pg_depend d
              WHERE d.objid = c.oid AND d.deptype = 'e'
          )
    ) LOOP
        IF r.relkind = 'v' THEN
            EXECUTE 'DROP VIEW IF EXISTS public.' || quote_ident(r.relname) || ' CASCADE';
        ELSE
            EXECUTE 'DROP MATERIALIZED VIEW IF EXISTS public.' || quote_ident(r.relname) || ' CASCADE';
        END IF;
    END LOOP;
END $$;

-- Drop all standalone sequences (table-owned ones already dropped via CASCADE)
DO $$ DECLARE
    r RECORD;
BEGIN
    FOR r IN (
        SELECT c.relname
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'S'
          AND NOT EXISTS (
              SELECT 1 FROM pg_depend d
              WHERE d.objid = c.oid AND d.deptype IN ('e', 'a', 'i')
          )
    ) LOOP
        EXECUTE 'DROP SEQUENCE IF EXISTS public.' || quote_ident(r.relname) || ' CASCADE';
    END LOOP;
END $$;
"#;
                let _ = stdin.write_all(drop_script.as_bytes());
                let _ = stdin.write_all(sql_content.as_bytes());
            }
            // stdin drops here, sending EOF to psql
        });

        // wait_with_output drains stdout+stderr concurrently, then waits for exit
        let output = child.wait_with_output();
        let _ = writer.join();
        output
    })
    .await
    .map_err(|e| ApiError::BadRequest(format!("psql task panicked: {}", e)))?
    .map_err(|e| ApiError::BadRequest(format!("psql failed: {}", e)))?;

    let reconnect_delay_ms = 2000; // 2 seconds for DB to stabilize

    // Step 3: Wait and attempt reconnection
    tokio::time::sleep(std::time::Duration::from_millis(reconnect_delay_ms)).await;

    // Try to verify connection by doing a simple query
    // The connection pool should auto-reconnect
    let db_ok = state
        .db
        .notes
        .list(ListNotesRequest {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .is_ok();

    // Step 4: Rebuild indexes and refresh query planner stats post-restore
    // Run each step separately so partial failures don't block the rest.
    if db_ok {
        let post_restore_sql = r#"
-- Rebuild all indexes (GIN, B-tree, GiST) to ensure consistency
REINDEX DATABASE matric;

-- Full vacuum + analyze to update planner statistics for all tables
VACUUM ANALYZE;

-- Explicit ANALYZE on FTS-critical tables for accurate GIN index stats
ANALYZE note_revised_current;
ANALYZE note;
ANALYZE note_tag;
ANALYZE note_original;
ANALYZE embedding;

-- Verify GIN indexes exist on tsvector columns (no-op if present)
CREATE INDEX IF NOT EXISTS idx_revised_current_tsv ON note_revised_current USING GIN(tsv);
"#;
        let post_sql = post_restore_sql.to_string();
        let reindex_result = tokio::task::spawn_blocking(move || {
            let mut child = std::process::Command::new("psql")
                .args(["-U", "matric", "-h", "localhost", "-d", "matric"])
                .env("PGPASSWORD", "matric")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let stdin = child.stdin.take();
            let writer = std::thread::spawn(move || {
                use std::io::Write;
                if let Some(mut stdin) = stdin {
                    let _ = stdin.write_all(post_sql.as_bytes());
                }
            });

            let output = child.wait_with_output();
            let _ = writer.join();
            output
        })
        .await
        .unwrap_or_else(|e| Err(std::io::Error::other(format!("task panicked: {}", e))));
        match &reindex_result {
            Ok(o) if !o.status.success() => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("Post-restore REINDEX/VACUUM had warnings: {}", stderr);
            }
            Err(e) => {
                tracing::warn!("Post-restore REINDEX/VACUUM failed: {}", e);
            }
            _ => {
                tracing::info!("Post-restore REINDEX, VACUUM ANALYZE, and FTS index rebuild completed successfully");
            }
        }
    }

    let success = output.status.success() && db_ok;

    Ok(Json(DatabaseRestoreResponse {
        success,
        message: if success {
            format!("Database restored from {}", req.filename)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!("Restore may have issues: {}", stderr)
        },
        prerestore_backup: prerestore_filename,
        restored_from: req.filename,
        reconnect_delay_ms,
    }))
}

// =============================================================================
// KNOWLEDGE ARCHIVE HANDLERS (.archive format)
// A knowledge archive bundles a backup file + its metadata sidecar into a single
// portable tar file with the .archive extension.
// =============================================================================

/// Download a backup as a knowledge archive (.archive).
/// Bundles the backup file and its metadata sidecar into a tar stream.
#[utoipa::path(get, path = "/api/v1/backup/knowledge-archive/{filename}", tag = "Backup",
    params(("filename" = String, Path, description = "Archive filename")),
    responses((status = 200, description = "Success")))]
async fn knowledge_archive_download(
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use axum::http::HeaderValue;
    use tar::Builder;

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let backup_path = std::path::Path::new(&backup_dir).join(&filename);
    if !backup_path.exists() {
        return Err(ApiError::NotFound(format!(
            "Backup not found: {}",
            filename
        )));
    }

    // Read backup file
    let backup_data = std::fs::read(&backup_path)
        .map_err(|e| ApiError::BadRequest(format!("Failed to read backup: {}", e)))?;

    // Load or create metadata
    let metadata = BackupMetadata::load(&backup_path).unwrap_or_else(|| {
        // Generate basic metadata if none exists
        let backup_type = if filename.starts_with(backup_prefix::AUTO) {
            backup_prefix::AUTO
        } else if filename.starts_with(backup_prefix::SNAPSHOT) {
            backup_prefix::SNAPSHOT
        } else if filename.starts_with(backup_prefix::PRERESTORE) {
            backup_prefix::PRERESTORE
        } else if filename.starts_with(backup_prefix::UPLOAD) {
            backup_prefix::UPLOAD
        } else {
            "unknown"
        };
        BackupMetadata {
            title: filename.clone(),
            description: None,
            backup_type: backup_type.to_string(),
            created_at: chrono::Utc::now(),
            note_count: None,
            db_size_bytes: Some(backup_data.len() as i64),
            source: "system".to_string(),
            extra: Default::default(),
            matric_version: None,
            matric_version_min: None,
            matric_version_max: None,
            pg_version: None,
            schema_migration_count: None,
            last_migration: None,
        }
    });

    let metadata_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| ApiError::BadRequest(format!("Failed to serialize metadata: {}", e)))?;

    // Create tar in memory
    let mut tar_data = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_data);

        // Add backup file
        let mut header = tar::Header::new_gnu();
        header.set_size(backup_data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(chrono::Utc::now().timestamp() as u64);
        header.set_cksum();
        tar.append_data(&mut header, &filename, backup_data.as_slice())
            .map_err(|e| ApiError::BadRequest(format!("Failed to add backup to archive: {}", e)))?;

        // Add metadata file
        let metadata_bytes = metadata_json.as_bytes();
        let mut meta_header = tar::Header::new_gnu();
        meta_header.set_size(metadata_bytes.len() as u64);
        meta_header.set_mode(0o644);
        meta_header.set_mtime(chrono::Utc::now().timestamp() as u64);
        meta_header.set_cksum();
        tar.append_data(&mut meta_header, "metadata.json", metadata_bytes)
            .map_err(|e| {
                ApiError::BadRequest(format!("Failed to add metadata to archive: {}", e))
            })?;

        tar.finish()
            .map_err(|e| ApiError::BadRequest(format!("Failed to finalize archive: {}", e)))?;
    }

    // Generate archive filename
    let archive_name = format!(
        "{}.archive",
        filename
            .trim_end_matches(".sql.gz")
            .trim_end_matches(".tar.gz")
    );
    let content_disposition = format!("attachment; filename=\"{}\"", archive_name);

    let headers = [
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-tar"),
        ),
        (
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&content_disposition).unwrap(),
        ),
    ];

    Ok((StatusCode::OK, headers, tar_data))
}

/// Upload a knowledge archive (.archive) and extract backup + metadata.
#[utoipa::path(post, path = "/api/v1/backup/knowledge-archive", tag = "Backup",
    responses((status = 200, description = "Success")))]
async fn knowledge_archive_upload(
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, ApiError> {
    use tar::Archive;

    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Ensure backup directory exists
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| ApiError::BadRequest(format!("Failed to create backup directory: {}", e)))?;

    let mut archive_data: Option<Vec<u8>> = None;
    let mut original_filename: Option<String> = None;

    // Read the multipart upload
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to read upload: {}", e)))?
    {
        if field.name() == Some("file") || field.name() == Some("archive") {
            original_filename = field.file_name().map(|s| s.to_string());
            archive_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Failed to read file data: {}", e)))?
                    .to_vec(),
            );
            break;
        }
    }

    let archive_data = archive_data.ok_or_else(|| {
        ApiError::BadRequest("No file uploaded. Use field name 'file' or 'archive'.".to_string())
    })?;

    // Parse the tar archive
    let mut tar_reader = Archive::new(archive_data.as_slice());

    let mut backup_filename: Option<String> = None;
    let mut backup_data: Option<Vec<u8>> = None;
    let mut metadata: Option<BackupMetadata> = None;

    for entry in tar_reader
        .entries()
        .map_err(|e| ApiError::BadRequest(format!("Invalid tar archive: {}", e)))?
    {
        let mut entry =
            entry.map_err(|e| ApiError::BadRequest(format!("Failed to read tar entry: {}", e)))?;
        let path = entry
            .path()
            .map_err(|e| ApiError::BadRequest(format!("Invalid path in archive: {}", e)))?
            .to_string_lossy()
            .to_string();

        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut contents)
            .map_err(|e| ApiError::BadRequest(format!("Failed to read entry contents: {}", e)))?;

        if path == "metadata.json" {
            metadata = serde_json::from_slice(&contents).ok();
        } else if path.ends_with(".sql.gz") || path.ends_with(".tar.gz") {
            backup_filename = Some(path);
            backup_data = Some(contents);
        }
    }

    let backup_filename = backup_filename.ok_or_else(|| {
        ApiError::BadRequest(
            "No backup file found in archive. Expected .sql.gz or .tar.gz file.".to_string(),
        )
    })?;
    let backup_data = backup_data.unwrap();

    // Security: prevent overwriting with path traversal
    if backup_filename.contains("..")
        || backup_filename.contains('/')
        || backup_filename.contains('\\')
    {
        return Err(ApiError::BadRequest(
            "Invalid filename in archive".to_string(),
        ));
    }

    // Rename with upload prefix if it doesn't have a recognized prefix
    let final_filename = if backup_filename.starts_with(backup_prefix::AUTO)
        || backup_filename.starts_with(backup_prefix::SNAPSHOT)
        || backup_filename.starts_with(backup_prefix::PRERESTORE)
        || backup_filename.starts_with(backup_prefix::UPLOAD)
    {
        backup_filename.clone()
    } else {
        format!("{}_{}", backup_prefix::UPLOAD, backup_filename)
    };

    let backup_path = std::path::Path::new(&backup_dir).join(&final_filename);

    // Write backup file
    std::fs::write(&backup_path, &backup_data)
        .map_err(|e| ApiError::BadRequest(format!("Failed to write backup: {}", e)))?;

    // Write or update metadata
    let final_metadata = metadata.unwrap_or_else(|| {
        BackupMetadata::upload(
            None,
            Some("Uploaded via knowledge archive".to_string()),
            original_filename.as_deref().unwrap_or("unknown.archive"),
        )
    });
    final_metadata
        .save(&backup_path)
        .map_err(|e| ApiError::BadRequest(format!("Failed to write metadata: {}", e)))?;

    let size_bytes = backup_data.len() as u64;
    let size_human = if size_bytes > 1024 * 1024 {
        format!("{:.2} MB", size_bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} KB", size_bytes as f64 / 1024.0)
    };

    Ok(Json(serde_json::json!({
        "success": true,
        "filename": final_filename,
        "path": backup_path.display().to_string(),
        "size_bytes": size_bytes,
        "size_human": size_human,
        "metadata": final_metadata,
        "message": "Knowledge archive uploaded and extracted successfully"
    })))
}

// =============================================================================
// BACKUP METADATA HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct UpdateMetadataRequest {
    /// Human-readable title for the backup
    title: Option<String>,
    /// Detailed description of the backup
    description: Option<String>,
}

/// Get metadata for a specific backup file.
#[utoipa::path(get, path = "/api/v1/backup/metadata/{filename}", tag = "Backup",
    params(("filename" = String, Path, description = "Backup filename")),
    responses((status = 200, description = "Success")))]
async fn get_backup_metadata(Path(filename): Path<String>) -> Result<impl IntoResponse, ApiError> {
    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let backup_path = std::path::Path::new(&backup_dir).join(&filename);
    if !backup_path.exists() {
        return Err(ApiError::NotFound(format!(
            "Backup not found: {}",
            filename
        )));
    }

    // Try to load metadata from sidecar file
    match BackupMetadata::load(&backup_path) {
        Some(meta) => Ok(Json(serde_json::json!({
            "has_metadata": true,
            "filename": filename,
            "metadata": meta
        }))),
        None => {
            // No metadata file - return basic info from filename
            let backup_type = if filename.starts_with(backup_prefix::AUTO) {
                "auto"
            } else if filename.starts_with(backup_prefix::SNAPSHOT) {
                "snapshot"
            } else if filename.starts_with(backup_prefix::PRERESTORE) {
                "prerestore"
            } else if filename.starts_with(backup_prefix::UPLOAD) {
                "upload"
            } else {
                "unknown"
            };

            Ok(Json(serde_json::json!({
                "has_metadata": false,
                "filename": filename,
                "backup_type": backup_type,
                "message": "No metadata file found. Use PUT to add metadata."
            })))
        }
    }
}

/// Update or create metadata for a backup file.
#[utoipa::path(put, path = "/api/v1/backup/metadata/{filename}", tag = "Backup",
    params(("filename" = String, Path, description = "Backup filename")),
    responses((status = 200, description = "Success")))]
async fn update_backup_metadata(
    Path(filename): Path<String>,
    Json(req): Json<UpdateMetadataRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let backup_dir =
        std::env::var("BACKUP_DEST").unwrap_or_else(|_| "/var/backups/matric-memory".to_string());

    // Security: prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(ApiError::BadRequest("Invalid filename".to_string()));
    }

    let backup_path = std::path::Path::new(&backup_dir).join(&filename);
    if !backup_path.exists() {
        return Err(ApiError::NotFound(format!(
            "Backup not found: {}",
            filename
        )));
    }

    // Determine backup type from filename prefix
    let backup_type = if filename.starts_with(backup_prefix::AUTO) {
        backup_prefix::AUTO
    } else if filename.starts_with(backup_prefix::SNAPSHOT) {
        backup_prefix::SNAPSHOT
    } else if filename.starts_with(backup_prefix::PRERESTORE) {
        backup_prefix::PRERESTORE
    } else if filename.starts_with(backup_prefix::UPLOAD) {
        backup_prefix::UPLOAD
    } else {
        "unknown"
    };

    // Load existing metadata or create new
    let mut metadata = BackupMetadata::load(&backup_path).unwrap_or_else(|| BackupMetadata {
        title: filename.clone(),
        description: None,
        backup_type: backup_type.to_string(),
        created_at: std::fs::metadata(&backup_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(chrono::DateTime::from)
            .unwrap_or_else(chrono::Utc::now),
        note_count: None,
        db_size_bytes: None,
        source: "user".to_string(),
        extra: Default::default(),
        matric_version: None,
        matric_version_min: None,
        matric_version_max: None,
        pg_version: None,
        schema_migration_count: None,
        last_migration: None,
    });

    // Update fields if provided
    if let Some(title) = req.title {
        metadata.title = title;
    }
    if let Some(description) = req.description {
        metadata.description = Some(description);
    }

    // Save updated metadata
    metadata
        .save(&backup_path)
        .map_err(|e| ApiError::BadRequest(format!("Failed to save metadata: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "filename": filename,
        "metadata": metadata
    })))
}

// =============================================================================
// MEMORY INFO (Detailed sizing for hardware planning)
// =============================================================================

#[derive(Debug, Serialize)]
struct MemoryInfoResponse {
    /// Summary statistics
    summary: MemorySummary,
    /// Embedding set details
    embedding_sets: Vec<EmbeddingSetInfo>,
    /// Storage breakdown
    storage: StorageBreakdown,
    /// Hardware recommendations
    recommendations: HardwareRecommendations,
}

#[derive(Debug, Serialize)]
struct MemorySummary {
    total_notes: i64,
    total_embeddings: i64,
    total_links: i64,
    total_collections: i64,
    total_tags: i64,
    total_templates: i64,
}

#[derive(Debug, Serialize)]
struct EmbeddingSetInfo {
    id: Uuid,
    name: String,
    slug: String,
    description: Option<String>,
    document_count: i32,
    embedding_count: i32,
    /// Embedding dimension (e.g., 768 for nomic-embed-text)
    dimension: i32,
    /// Estimated vector storage in bytes
    vector_storage_bytes: i64,
    vector_storage_human: String,
    /// Model name
    model: Option<String>,
    /// Index status
    index_status: String,
    /// Is this the system default set?
    is_system: bool,
}

#[derive(Debug, Serialize)]
struct StorageBreakdown {
    /// Total database size
    database_total_bytes: i64,
    database_total_human: String,
    /// Embedding table size (actual)
    embedding_table_bytes: i64,
    embedding_table_human: String,
    /// Embedding index size (estimated)
    embedding_index_bytes: i64,
    embedding_index_human: String,
    /// Notes table size
    notes_table_bytes: i64,
    notes_table_human: String,
    /// FTS index size
    fts_index_bytes: i64,
    fts_index_human: String,
}

#[derive(Debug, Serialize)]
struct HardwareRecommendations {
    /// Minimum RAM for embedding inference
    min_inference_ram_gb: f64,
    /// Recommended RAM for good performance
    recommended_ram_gb: f64,
    /// GPU VRAM needed for embedding model (if using GPU inference)
    gpu_vram_needed_gb: f64,
    /// Whether GPU is required
    gpu_required: bool,
    /// Notes about hardware requirements
    notes: Vec<String>,
}

/// Get detailed memory/storage info for hardware planning.
#[utoipa::path(get, path = "/api/v1/memory/info", tag = "System",
    responses((status = 200, description = "Success")))]
async fn memory_info(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    // Get summary counts
    let notes_req = ListNotesRequest {
        limit: Some(1),
        ..Default::default()
    };
    let notes_resp = state.db.notes.list(notes_req).await?;

    let _links = state.db.links.list_all(1, 0).await.unwrap_or_default();
    let link_count = state.db.links.count().await.unwrap_or(0);
    let collections = state.db.collections.list(None).await.unwrap_or_default();
    let tags = state.db.tags.list().await.unwrap_or_default();
    let templates: Vec<matric_core::NoteTemplate> = {
        let ctx = state.db.for_schema("public")?;
        let templates = matric_db::PgTemplateRepository::new(state.db.pool.clone());
        ctx.query(move |tx| Box::pin(async move { templates.list_tx(tx).await }))
            .await
    }
    .unwrap_or_default();

    // Get embedding set info
    let embedding_sets = state.db.embedding_sets.list().await.unwrap_or_default();
    let mut set_infos = Vec::new();

    for set in &embedding_sets {
        // Calculate vector storage: embedding_count * dimension * 4 bytes per float
        let vector_bytes = (set.embedding_count as i64) * (set.dimension.unwrap_or(768) as i64) * 4;

        set_infos.push(EmbeddingSetInfo {
            id: set.id,
            name: set.name.clone(),
            slug: set.slug.clone(),
            description: set.description.clone(),
            document_count: set.document_count,
            embedding_count: set.embedding_count,
            dimension: set.dimension.unwrap_or(768),
            vector_storage_bytes: vector_bytes,
            vector_storage_human: format_size(vector_bytes as u64),
            model: set.model.clone(),
            index_status: format!("{:?}", set.index_status),
            is_system: set.is_system,
        });
    }

    // Get storage sizes from database
    let storage = get_storage_breakdown(&state.db).await;

    // Calculate total embedding vector bytes
    let total_vector_bytes: i64 = set_infos.iter().map(|s| s.vector_storage_bytes).sum();

    // Hardware recommendations
    let embedding_model_size_gb = 0.5; // nomic-embed-text is ~500MB
    let recommended_ram_gb = 4.0 + (total_vector_bytes as f64 / 1_073_741_824.0); // 4GB base + vectors
    let min_ram_gb = 2.0 + (total_vector_bytes as f64 / 1_073_741_824.0);

    let total_embeddings: i32 = set_infos.iter().map(|s| s.embedding_count).sum();
    let dimension = set_infos.first().map(|s| s.dimension).unwrap_or(768);

    let recommendations = HardwareRecommendations {
        min_inference_ram_gb: min_ram_gb,
        recommended_ram_gb,
        gpu_vram_needed_gb: embedding_model_size_gb + 1.0, // Model + workspace
        gpu_required: false,                               // Ollama can run on CPU (slower)
        notes: vec![
            // Storage explanation
            format!(
                "Vector storage: {} ({} embeddings × {} dimensions × 4 bytes/float)",
                format_size(total_vector_bytes as u64),
                total_embeddings,
                dimension
            ),
            // GPU role - embedding GENERATION
            "GPU usage: EMBEDDING GENERATION via Ollama (runs the embedding model)".to_string(),
            format!(
                "  └─ With GPU: ~{}MB VRAM for nomic-embed-text + workspace",
                (embedding_model_size_gb * 1024.0) as i64
            ),
            "  └─ Without GPU: Falls back to CPU (functional but slower)".to_string(),
            // CPU role - vector SEARCH
            "CPU usage: VECTOR SEARCH via pgvector (runs in PostgreSQL)".to_string(),
            "  └─ HNSW index traversal and cosine similarity are CPU-bound".to_string(),
            "  └─ More RAM = more vectors cached = faster search performance".to_string(),
            // Practical summary
            "Practical: GPU speeds up creating/updating notes; all searches use CPU/RAM"
                .to_string(),
        ],
    };

    Ok(Json(MemoryInfoResponse {
        summary: MemorySummary {
            total_notes: notes_resp.total,
            total_embeddings: set_infos.iter().map(|s| s.embedding_count as i64).sum(),
            total_links: link_count,
            total_collections: collections.len() as i64,
            total_tags: tags.len() as i64,
            total_templates: templates.len() as i64,
        },
        embedding_sets: set_infos,
        storage,
        recommendations,
    }))
}

/// Get storage breakdown using system commands (avoids sqlx dependency).
async fn get_storage_breakdown(_db: &Database) -> StorageBreakdown {
    // Use psql to query sizes - more reliable and doesn't require sqlx in this crate
    let db_size = get_db_size_via_psql("pg_database_size(current_database())").unwrap_or(0);
    let embedding_size = get_db_size_via_psql("pg_total_relation_size('embedding')").unwrap_or(0);
    let notes_size = get_db_size_via_psql("pg_total_relation_size('note')").unwrap_or(0);
    let fts_size =
        get_db_size_via_psql("pg_total_relation_size('note_revised_current')").unwrap_or(0);

    // Estimate index size as 20% of table size (rough heuristic)
    let embedding_index_size = embedding_size / 5;

    StorageBreakdown {
        database_total_bytes: db_size,
        database_total_human: format_size(db_size as u64),
        embedding_table_bytes: embedding_size,
        embedding_table_human: format_size(embedding_size as u64),
        embedding_index_bytes: embedding_index_size,
        embedding_index_human: format_size(embedding_index_size as u64),
        notes_table_bytes: notes_size,
        notes_table_human: format_size(notes_size as u64),
        fts_index_bytes: fts_size,
        fts_index_human: format_size(fts_size as u64),
    }
}

/// Get database size using psql command.
fn get_db_size_via_psql(expr: &str) -> Option<i64> {
    let output = std::process::Command::new("psql")
        .args([
            "-U",
            "matric",
            "-h",
            "localhost",
            "-d",
            "matric",
            "-t", // Tuples only (no header)
            "-A", // Unaligned output
            "-c",
            &format!("SELECT {}", expr),
        ])
        .env("PGPASSWORD", "matric")
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<i64>()
            .ok()
    } else {
        None
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_export_manifest_serialization() {
        let manifest = BackupExportManifest {
            version: "1.0.0".to_string(),
            format: "matric-backup".to_string(),
            created_at: chrono::Utc::now(),
            counts: BackupCounts {
                notes: 10,
                collections: 3,
                tags: 5,
                templates: 2,
            },
        };

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"format\":\"matric-backup\""));
        assert!(json.contains("\"notes\":10"));
    }

    #[test]
    fn test_backup_status_response_serialization() {
        let response = BackupStatusResponse {
            backup_directory: "/var/backups/test".to_string(),
            total_size_bytes: 1258291,
            total_size_human: "1.20 MB".to_string(),
            disk_usage: Some("1.2G".to_string()),
            backup_count: 5,
            shard_count: 2,
            pgdump_count: 3,
            latest_backup: Some(LatestBackupInfo {
                path: "/var/backups/test/backup.sql.gz".to_string(),
                filename: "backup.sql.gz".to_string(),
                size_bytes: 1024,
                modified: chrono::Utc::now(),
            }),
            status: "healthy".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"backup_directory\":\"/var/backups/test\""));
        assert!(json.contains("\"disk_usage\":\"1.2G\""));
        assert!(json.contains("\"backup_count\":5"));
        assert!(json.contains("\"total_size_bytes\":1258291"));
        assert!(json.contains("\"status\":\"healthy\""));
    }

    #[test]
    fn test_backup_trigger_response_serialization() {
        let response = BackupTriggerResponse {
            status: "success".to_string(),
            output: "Backup completed".to_string(),
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"output\":\"Backup completed\""));
    }

    #[test]
    fn test_backup_export_query_defaults() {
        // Test default values for BackupExportQuery
        let query: BackupExportQuery = serde_json::from_str("{}").unwrap();
        assert!(!query.starred_only);
        assert!(query.tags.is_none());
        assert!(query.created_after.is_none());
        assert!(query.created_before.is_none());
    }

    #[test]
    fn test_backup_export_query_with_filters() {
        let json = r#"{
            "starred_only": true,
            "tags": "rust,api",
            "created_after": "2024-01-01T00:00:00Z"
        }"#;
        let query: BackupExportQuery = serde_json::from_str(json).unwrap();
        assert!(query.starred_only);
        assert_eq!(query.tags, Some("rust,api".to_string()));
        assert!(query.created_after.is_some());
    }

    #[test]
    fn test_backup_trigger_body_defaults() {
        let body: BackupTriggerBody = serde_json::from_str("{}").unwrap();
        assert!(body.destinations.is_none());
        assert!(!body.dry_run);
    }

    #[test]
    fn test_backup_trigger_body_with_options() {
        let json = r#"{
            "destinations": ["local", "s3"],
            "dry_run": true
        }"#;
        let body: BackupTriggerBody = serde_json::from_str(json).unwrap();
        assert_eq!(
            body.destinations,
            Some(vec!["local".to_string(), "s3".to_string()])
        );
        assert!(body.dry_run);
    }

    #[test]
    fn test_backup_import_response_serialization() {
        let response = BackupImportResponse {
            status: "success".to_string(),
            dry_run: false,
            imported: ImportCounts {
                notes: 10,
                collections: 2,
                templates: 1,
            },
            skipped: ImportCounts {
                notes: 2,
                collections: 0,
                templates: 0,
            },
            errors: vec![],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"dry_run\":false"));
        assert!(json.contains("\"notes\":10"));
    }

    #[test]
    fn test_backup_import_body_defaults() {
        let json = r#"{
            "backup": {
                "notes": []
            }
        }"#;
        let body: BackupImportBody = serde_json::from_str(json).unwrap();
        assert!(!body.dry_run);
        assert!(body.backup.notes.is_empty());
    }

    #[test]
    fn test_backup_import_body_with_options() {
        let json = r#"{
            "backup": {
                "notes": [
                    {
                        "content": "Test note",
                        "tags": ["test"]
                    }
                ]
            },
            "dry_run": true,
            "on_conflict": "replace"
        }"#;
        let body: BackupImportBody = serde_json::from_str(json).unwrap();
        assert!(body.dry_run);
        assert_eq!(body.backup.notes.len(), 1);
        assert_eq!(body.backup.notes[0].content, Some("Test note".to_string()));
    }

    #[test]
    fn test_conflict_strategy_deserialization() {
        let json = r#""skip""#;
        let strategy: ConflictStrategy = serde_json::from_str(json).unwrap();
        assert!(matches!(strategy, ConflictStrategy::Skip));

        let json = r#""replace""#;
        let strategy: ConflictStrategy = serde_json::from_str(json).unwrap();
        assert!(matches!(strategy, ConflictStrategy::Replace));

        let json = r#""merge""#;
        let strategy: ConflictStrategy = serde_json::from_str(json).unwrap();
        assert!(matches!(strategy, ConflictStrategy::Merge));
    }

    #[test]
    fn test_backup_note_data_deserialization() {
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "original_content": "Test content",
            "revised_content": "Enhanced content",
            "format": "markdown",
            "starred": true,
            "tags": ["tag1", "tag2"]
        }"#;
        let note: BackupNoteData = serde_json::from_str(json).unwrap();
        assert!(note.id.is_some());
        assert_eq!(note.original_content, Some("Test content".to_string()));
        assert_eq!(note.revised_content, Some("Enhanced content".to_string()));
        assert_eq!(note.format, Some("markdown".to_string()));
        assert_eq!(note.starred, Some(true));
        assert_eq!(
            note.tags,
            Some(vec!["tag1".to_string(), "tag2".to_string()])
        );
    }

    #[test]
    fn test_import_counts_default() {
        let counts = ImportCounts::default();
        assert_eq!(counts.notes, 0);
        assert_eq!(counts.collections, 0);
        assert_eq!(counts.templates, 0);
    }

    // =========================================================================
    // ARCHIVE EXPORT/IMPORT TESTS
    // =========================================================================

    #[test]
    fn test_shard_export_query_defaults() {
        let query: ShardExportQuery = serde_json::from_str("{}").unwrap();
        assert!(query.include.is_none());
    }

    #[test]
    fn test_shard_export_query_with_components() {
        let json = r#"{"include": "notes,links,embeddings"}"#;
        let query: ShardExportQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include, Some("notes,links,embeddings".to_string()));
    }

    #[test]
    fn test_shard_manifest_serialization() {
        let manifest = ShardManifest {
            version: "1.0.0".to_string(),
            matric_version: Some("2026.1.12".to_string()),
            format: "matric-shard".to_string(),
            created_at: chrono::Utc::now(),
            components: vec!["notes".to_string(), "links".to_string()],
            counts: ShardCounts {
                notes: 100,
                collections: 5,
                tags: 20,
                templates: 3,
                links: 50,
                embedding_sets: 2,
                embedding_set_members: 80,
                embeddings: 500,
                embedding_configs: 1,
            },
            checksums: std::collections::HashMap::new(),
            min_reader_version: Some("1.0.0".to_string()),
            migrated_from: None,
            migration_history: vec![],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"matric_version\":\"2026.1.12\""));
        assert!(json.contains("\"format\":\"matric-shard\""));
        assert!(json.contains("\"notes\":100"));
        assert!(json.contains("\"links\":50"));
        assert!(json.contains("\"embeddings\":500"));
    }

    #[test]
    fn test_shard_manifest_deserialization() {
        // Test with matric_version present
        let json = r#"{
            "version": "1.0.0",
            "matric_version": "2026.1.12",
            "format": "matric-shard",
            "created_at": "2024-01-15T10:30:00Z",
            "components": ["notes", "links"],
            "counts": {
                "notes": 10,
                "collections": 2,
                "tags": 5,
                "templates": 1,
                "links": 8,
                "embedding_sets": 1,
                "embedding_set_members": 10,
                "embeddings": 50,
                "embedding_configs": 1
            },
            "checksums": {}
        }"#;
        let manifest: ShardManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.matric_version, Some("2026.1.12".to_string()));
        assert_eq!(manifest.format, "matric-shard");
        assert_eq!(manifest.components.len(), 2);
        assert_eq!(manifest.counts.notes, 10);
        assert_eq!(manifest.counts.links, 8);
        assert_eq!(manifest.counts.embeddings, 50);
    }

    #[test]
    fn test_shard_manifest_deserialization_backward_compat() {
        // Test backward compatibility - matric_version missing (older shards)
        let json = r#"{
            "version": "1.0.0",
            "format": "matric-shard",
            "created_at": "2024-01-15T10:30:00Z",
            "components": ["notes"],
            "counts": {
                "notes": 5,
                "collections": 0,
                "tags": 0,
                "templates": 0,
                "links": 0,
                "embedding_sets": 0,
                "embedding_set_members": 0,
                "embeddings": 0,
                "embedding_configs": 0
            },
            "checksums": {}
        }"#;
        let manifest: ShardManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.matric_version.is_none()); // Backward compatible: None when missing
        assert_eq!(manifest.format, "matric-shard");
    }

    #[test]
    fn test_shard_counts_default() {
        let counts = ShardCounts::default();
        assert_eq!(counts.notes, 0);
        assert_eq!(counts.collections, 0);
        assert_eq!(counts.tags, 0);
        assert_eq!(counts.templates, 0);
        assert_eq!(counts.links, 0);
        assert_eq!(counts.embedding_sets, 0);
        assert_eq!(counts.embedding_set_members, 0);
        assert_eq!(counts.embeddings, 0);
        assert_eq!(counts.embedding_configs, 0);
    }

    #[test]
    fn test_knowledge_shard_import_body_defaults() {
        let json = r#"{"shard_base64": "H4sIAAAAAAAA"}"#;
        let body: ShardImportBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.shard_base64, "H4sIAAAAAAAA");
        assert!(body.include.is_none());
        assert!(!body.dry_run);
        assert!(matches!(body.on_conflict, ConflictStrategy::Skip));
        assert!(!body.skip_embedding_regen);
    }

    #[test]
    fn test_knowledge_shard_import_body_with_options() {
        let json = r#"{
            "shard_base64": "H4sIAAAAAAAA",
            "include": "notes,collections",
            "dry_run": true,
            "on_conflict": "replace",
            "skip_embedding_regen": true
        }"#;
        let body: ShardImportBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.shard_base64, "H4sIAAAAAAAA");
        assert_eq!(body.include, Some("notes,collections".to_string()));
        assert!(body.dry_run);
        assert!(matches!(body.on_conflict, ConflictStrategy::Replace));
        assert!(body.skip_embedding_regen);
    }

    #[test]
    fn test_knowledge_shard_import_response_serialization() {
        let response = ShardImportResponse {
            status: "success".to_string(),
            manifest: Some(ShardManifest {
                version: "1.0.0".to_string(),
                matric_version: Some("2026.1.12".to_string()),
                format: "matric-shard".to_string(),
                created_at: chrono::Utc::now(),
                components: vec!["notes".to_string()],
                counts: ShardCounts::default(),
                checksums: std::collections::HashMap::new(),
                min_reader_version: Some("1.0.0".to_string()),
                migrated_from: None,
                migration_history: vec![],
            }),
            imported: ShardImportCounts {
                notes: 10,
                collections: 2,
                tags: 0,
                templates: 1,
                links: 5,
                embedding_sets: 0,
                embedding_set_members: 0,
                embeddings: 0,
            },
            skipped: ShardImportCounts::default(),
            errors: vec![],
            warnings: vec![],
            dry_run: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"notes\":10"));
        assert!(json.contains("\"links\":5"));
        assert!(json.contains("\"dry_run\":false"));
    }

    #[test]
    fn test_knowledge_shard_import_counts_default() {
        let counts = ShardImportCounts::default();
        assert_eq!(counts.notes, 0);
        assert_eq!(counts.collections, 0);
        assert_eq!(counts.tags, 0);
        assert_eq!(counts.templates, 0);
        assert_eq!(counts.links, 0);
        assert_eq!(counts.embedding_sets, 0);
        assert_eq!(counts.embedding_set_members, 0);
        assert_eq!(counts.embeddings, 0);
    }

    // =========================================================================
    // EVENTING INTEGRATION TESTS (Issue #47)
    // =========================================================================

    /// Receive the next Text message from a WS stream, skipping Ping/Pong frames.
    async fn next_text_message(
        ws: &mut (impl futures::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin),
    ) -> String {
        use futures::StreamExt;
        let deadline = std::time::Duration::from_secs(5);
        let start = tokio::time::Instant::now();
        loop {
            let remaining = deadline.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                panic!("timeout waiting for WS text message");
            }
            let msg = tokio::time::timeout(remaining, ws.next())
                .await
                .expect("timeout waiting for WS message")
                .expect("stream ended")
                .expect("WS error");
            if msg.is_text() {
                return msg.into_text().unwrap();
            }
            // Skip Ping, Pong, Binary, etc.
        }
    }

    /// Build a minimal test server with only eventing routes.
    /// Returns the base URL (e.g., "http://127.0.0.1:PORT").
    async fn spawn_eventing_test_server() -> (String, Arc<EventBus>, Arc<AtomicUsize>) {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        let db = Database::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");
        let event_bus = Arc::new(EventBus::new(matric_core::defaults::EVENT_BUS_CAPACITY));
        let ws_connections = Arc::new(AtomicUsize::new(0));

        let state = AppState {
            db,
            search: Arc::new(matric_search::HybridSearchEngine::new(
                Database::connect(&database_url).await.unwrap(),
            )),
            issuer: "http://localhost:3000".to_string(),
            rate_limiter: None,
            tag_resolver: matric_api::services::TagResolver::new(
                Database::connect(&database_url).await.unwrap(),
            ),
            search_cache: matric_api::services::SearchCache::disabled(),
            event_bus: event_bus.clone(),
            ws_connections: ws_connections.clone(),
            default_archive_cache: Arc::new(RwLock::new(DefaultArchiveCache::new(60))),
            require_auth: false,
            oauth_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_TOKEN_LIFETIME_SECS as i64,
            ),
            oauth_mcp_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_MCP_TOKEN_LIFETIME_SECS as i64,
            ),
            max_memories: matric_core::defaults::MAX_MEMORIES,
            max_upload_size: matric_core::defaults::MAX_UPLOAD_SIZE_BYTES,
            vision_backend: None,
            transcription_backend: None,
            git_sha: "test".to_string(),
            build_date: "test".to_string(),
            extraction_strategies: Vec::new(),
            schema_engines: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            database_url: String::new(),
        };

        let router = Router::new()
            .route("/api/v1/ws", get(ws_handler))
            .route("/api/v1/events", get(sse_events))
            .route("/api/v1/webhooks", post(create_webhook).get(list_webhooks))
            .route(
                "/api/v1/webhooks/:id",
                get(get_webhook)
                    .patch(update_webhook)
                    .delete(delete_webhook_handler),
            )
            .route(
                "/api/v1/webhooks/:id/deliveries",
                get(list_webhook_deliveries),
            )
            .route("/api/v1/webhooks/:id/test", post(test_webhook))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        // Give server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        (base_url, event_bus, ws_connections)
    }

    // -- WebSocket Tests --

    #[tokio::test]
    async fn test_ws_upgrade_succeeds() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let ws_url = base_url.replace("http://", "ws://") + "/api/v1/ws";

        let (ws_stream, response) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        // tungstenite returns the upgrade response
        assert_eq!(response.status(), 101);
        drop(ws_stream);
    }

    #[tokio::test]
    async fn test_ws_receives_events() {
        let (base_url, bus, _conns) = spawn_eventing_test_server().await;
        let ws_url = base_url.replace("http://", "ws://") + "/api/v1/ws";

        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

        // Small delay to ensure subscription is registered
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Emit an event
        bus.emit(ServerEvent::NoteUpdated {
            note_id: Uuid::nil(),
            title: Some("Test".to_string()),
            tags: vec!["tag1".to_string()],
            has_ai_content: false,
            has_links: false,
        });

        // Receive it (skipping any Ping frames)
        let text = next_text_message(&mut ws_stream).await;
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "NoteUpdated");
        assert_eq!(parsed["title"], "Test");
    }

    #[tokio::test]
    async fn test_ws_refresh_command_triggers_queue_status() {
        use futures::SinkExt;

        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let ws_url = base_url.replace("http://", "ws://") + "/api/v1/ws";

        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

        // Send refresh command
        ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                "refresh".to_string(),
            ))
            .await
            .unwrap();

        // Should receive QueueStatus (skipping Ping frames)
        let text = next_text_message(&mut ws_stream).await;
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "QueueStatus");
    }

    #[tokio::test]
    async fn test_ws_connection_counter() {
        let (base_url, _bus, conns) = spawn_eventing_test_server().await;
        let ws_url = base_url.replace("http://", "ws://") + "/api/v1/ws";

        assert_eq!(conns.load(Ordering::Relaxed), 0);

        let (ws1, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(conns.load(Ordering::Relaxed), 1);

        let (ws2, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(conns.load(Ordering::Relaxed), 2);

        drop(ws1);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(conns.load(Ordering::Relaxed), 1);

        drop(ws2);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(conns.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_ws_multiple_clients_all_receive_events() {
        let (base_url, bus, _conns) = spawn_eventing_test_server().await;
        let ws_url = base_url.replace("http://", "ws://") + "/api/v1/ws";

        let (mut ws1, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        let (mut ws2, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        let (mut ws3, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        bus.emit(ServerEvent::JobStarted {
            job_id: Uuid::nil(),
            job_type: "Embedding".to_string(),
            note_id: None,
        });

        for ws in [&mut ws1, &mut ws2, &mut ws3] {
            let text = next_text_message(ws).await;
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed["type"], "JobStarted");
        }
    }

    // -- SSE Tests --

    #[tokio::test]
    async fn test_sse_endpoint_returns_event_stream() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v1/events", base_url))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/event-stream"));
    }

    #[tokio::test]
    async fn test_sse_receives_events() {
        let (base_url, bus, _conns) = spawn_eventing_test_server().await;

        let client = reqwest::Client::new();
        let mut response = client
            .get(format!("{}/api/v1/events", base_url))
            .send()
            .await
            .unwrap();

        // Emit an event after a brief delay
        let bus_clone = bus.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            bus_clone.emit(ServerEvent::JobFailed {
                job_id: Uuid::nil(),
                job_type: "Embedding".to_string(),
                note_id: None,
                error: "test error".to_string(),
            });
        });

        // Read SSE chunks until we find our event
        let mut collected = String::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_secs(3), response.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    collected.push_str(&String::from_utf8_lossy(&chunk));
                    if collected.contains("JobFailed") {
                        break;
                    }
                }
                _ => break,
            }
        }

        assert!(collected.contains("event: JobFailed"));
        assert!(collected.contains("test error"));
    }

    #[tokio::test]
    async fn test_sse_multiple_clients() {
        let (base_url, bus, _conns) = spawn_eventing_test_server().await;

        let client = reqwest::Client::new();
        let mut resp1 = client
            .get(format!("{}/api/v1/events", base_url))
            .send()
            .await
            .unwrap();
        let mut resp2 = client
            .get(format!("{}/api/v1/events", base_url))
            .send()
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        bus.emit(ServerEvent::QueueStatus {
            total_jobs: 42,
            running: 1,
            pending: 41,
        });

        for resp in [&mut resp1, &mut resp2] {
            let mut collected = String::new();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while tokio::time::Instant::now() < deadline {
                match tokio::time::timeout(std::time::Duration::from_secs(3), resp.chunk()).await {
                    Ok(Ok(Some(chunk))) => {
                        collected.push_str(&String::from_utf8_lossy(&chunk));
                        if collected.contains("QueueStatus") {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            assert!(collected.contains("QueueStatus"));
        }
    }

    // -- Webhook API Tests --

    #[tokio::test]
    async fn test_create_webhook_returns_201() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/v1/webhooks", base_url))
            .json(&serde_json::json!({
                "url": format!("https://test-create-{}.example.com", chrono::Utc::now().timestamp_millis()),
                "events": ["NoteUpdated"],
                "max_retries": 3
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 201);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn test_list_webhooks_returns_all() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let client = reqwest::Client::new();
        let suffix = Uuid::new_v4();

        // Create 2 webhooks
        for i in 0..2 {
            client
                .post(format!("{}/api/v1/webhooks", base_url))
                .json(&serde_json::json!({
                    "url": format!("https://list-test-{}-{}.example.com", suffix, i),
                    "secret": "my-secret",
                    "events": ["JobCompleted"],
                    "max_retries": 3
                }))
                .send()
                .await
                .unwrap();
        }

        let response = client
            .get(format!("{}/api/v1/webhooks", base_url))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: Vec<serde_json::Value> = response.json().await.unwrap();
        let ours: Vec<_> = body
            .iter()
            .filter(|w| {
                w["url"]
                    .as_str()
                    .unwrap_or("")
                    .contains(&suffix.to_string())
            })
            .collect();
        assert_eq!(ours.len(), 2);

        // Verify secret is NOT exposed in list response
        for w in &ours {
            assert!(w.get("secret").is_none() || w["secret"].is_null());
        }
    }

    #[tokio::test]
    async fn test_get_webhook_not_found() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v1/webhooks/{}", base_url, Uuid::nil()))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_update_webhook_partial() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let client = reqwest::Client::new();

        // Create
        let create_resp = client
            .post(format!("{}/api/v1/webhooks", base_url))
            .json(&serde_json::json!({
                "url": format!("https://update-test-{}.example.com", chrono::Utc::now().timestamp_millis()),
                "events": ["JobCompleted", "NoteUpdated"],
                "max_retries": 3
            }))
            .send()
            .await
            .unwrap();
        let id: String = create_resp.json::<serde_json::Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Update only URL
        let update_resp = client
            .patch(format!("{}/api/v1/webhooks/{}", base_url, id))
            .json(&serde_json::json!({
                "url": "https://updated.example.com"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(update_resp.status(), 200);
        let webhook: serde_json::Value = update_resp.json().await.unwrap();
        assert_eq!(webhook["url"], "https://updated.example.com");
        // Events should be unchanged
        let events: Vec<String> = serde_json::from_value(webhook["events"].clone()).unwrap();
        assert_eq!(events, vec!["JobCompleted", "NoteUpdated"]);
    }

    #[tokio::test]
    async fn test_delete_webhook_returns_204() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let client = reqwest::Client::new();

        // Create
        let create_resp = client
            .post(format!("{}/api/v1/webhooks", base_url))
            .json(&serde_json::json!({
                "url": format!("https://delete-test-{}.example.com", chrono::Utc::now().timestamp_millis()),
                "events": [],
                "max_retries": 3
            }))
            .send()
            .await
            .unwrap();
        let id: String = create_resp.json::<serde_json::Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Delete
        let del_resp = client
            .delete(format!("{}/api/v1/webhooks/{}", base_url, id))
            .send()
            .await
            .unwrap();
        assert_eq!(del_resp.status(), 204);

        // Verify gone
        let get_resp = client
            .get(format!("{}/api/v1/webhooks/{}", base_url, id))
            .send()
            .await
            .unwrap();
        assert_eq!(get_resp.status(), 404);
    }

    #[tokio::test]
    async fn test_list_deliveries_with_limit() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let client = reqwest::Client::new();

        // Create webhook
        let create_resp = client
            .post(format!("{}/api/v1/webhooks", base_url))
            .json(&serde_json::json!({
                "url": format!("https://delivery-test-{}.example.com", chrono::Utc::now().timestamp_millis()),
                "events": [],
                "max_retries": 3
            }))
            .send()
            .await
            .unwrap();
        let id: String = create_resp.json::<serde_json::Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        // List deliveries (should be empty)
        let resp = client
            .get(format!(
                "{}/api/v1/webhooks/{}/deliveries?limit=2",
                base_url, id
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let deliveries: Vec<serde_json::Value> = resp.json().await.unwrap();
        assert!(deliveries.is_empty());
    }

    #[tokio::test]
    async fn test_test_webhook_sends_delivery() {
        let (base_url, _bus, _conns) = spawn_eventing_test_server().await;
        let client = reqwest::Client::new();

        // Create webhook (pointing to a non-existent URL, delivery will fail but endpoint should work)
        let create_resp = client
            .post(format!("{}/api/v1/webhooks", base_url))
            .json(&serde_json::json!({
                "url": "http://127.0.0.1:1/nonexistent",
                "events": [],
                "max_retries": 1
            }))
            .send()
            .await
            .unwrap();
        let id: String = create_resp.json::<serde_json::Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Send test delivery
        let test_resp = client
            .post(format!("{}/api/v1/webhooks/{}/test", base_url, id))
            .send()
            .await
            .unwrap();
        assert_eq!(test_resp.status(), 200);
        let body: serde_json::Value = test_resp.json().await.unwrap();
        assert_eq!(body["status"], "delivered");
    }

    // -- Bridge Tests --

    /// Spawn the bridge_worker_events function with a fresh worker broadcast channel
    /// and return (worker_tx, event_bus_rx) for sending WorkerEvents and receiving ServerEvents.
    async fn spawn_bridge() -> (
        tokio::sync::broadcast::Sender<WorkerEvent>,
        tokio::sync::broadcast::Receiver<ServerEvent>,
        Arc<EventBus>,
    ) {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        let db = Database::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");

        let (worker_tx, worker_rx) = tokio::sync::broadcast::channel::<WorkerEvent>(32);
        let event_bus = Arc::new(EventBus::new(matric_core::defaults::EVENT_BUS_CAPACITY));
        let server_rx = event_bus.subscribe();

        let bus_clone = event_bus.clone();
        tokio::spawn(async move {
            bridge_worker_events(worker_rx, bus_clone, db).await;
        });

        // Small delay to ensure the bridge task is running
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        (worker_tx, server_rx, event_bus)
    }

    #[tokio::test]
    async fn test_bridge_maps_job_started() {
        let (worker_tx, mut server_rx, _bus) = spawn_bridge().await;

        worker_tx
            .send(WorkerEvent::JobStarted {
                job_id: Uuid::nil(),
                job_type: matric_core::JobType::Embedding,
            })
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(3), server_rx.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        match event {
            ServerEvent::JobStarted {
                job_id, job_type, ..
            } => {
                assert_eq!(job_id, Uuid::nil());
                assert_eq!(job_type, "Embedding");
            }
            other => panic!("Expected JobStarted, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_maps_job_progress() {
        let (worker_tx, mut server_rx, _bus) = spawn_bridge().await;

        worker_tx
            .send(WorkerEvent::JobProgress {
                job_id: Uuid::nil(),
                percent: 42,
                message: Some("processing chunk 5/12".to_string()),
            })
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(3), server_rx.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        match event {
            ServerEvent::JobProgress {
                progress, message, ..
            } => {
                assert_eq!(progress, 42);
                assert_eq!(message.unwrap(), "processing chunk 5/12");
            }
            other => panic!("Expected JobProgress, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_maps_job_completed() {
        let (worker_tx, mut server_rx, _bus) = spawn_bridge().await;

        worker_tx
            .send(WorkerEvent::JobCompleted {
                job_id: Uuid::nil(),
                job_type: matric_core::JobType::Linking,
            })
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(3), server_rx.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        match event {
            ServerEvent::JobCompleted {
                job_id, job_type, ..
            } => {
                assert_eq!(job_id, Uuid::nil());
                assert_eq!(job_type, "Linking");
            }
            other => panic!("Expected JobCompleted, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_maps_job_failed() {
        let (worker_tx, mut server_rx, _bus) = spawn_bridge().await;

        worker_tx
            .send(WorkerEvent::JobFailed {
                job_id: Uuid::nil(),
                job_type: matric_core::JobType::AiRevision,
                error: "inference timeout".to_string(),
            })
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(3), server_rx.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        match event {
            ServerEvent::JobFailed {
                job_id,
                job_type,
                error,
                ..
            } => {
                assert_eq!(job_id, Uuid::nil());
                assert_eq!(job_type, "AiRevision");
                assert_eq!(error, "inference timeout");
            }
            other => panic!("Expected JobFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_bridge_skips_worker_started_stopped() {
        let (worker_tx, mut server_rx, _bus) = spawn_bridge().await;

        // Send WorkerStarted and WorkerStopped — should be silently ignored
        worker_tx.send(WorkerEvent::WorkerStarted).unwrap();
        worker_tx.send(WorkerEvent::WorkerStopped).unwrap();

        // Now send a real event so we can verify the bridge is still alive
        worker_tx
            .send(WorkerEvent::JobStarted {
                job_id: Uuid::nil(),
                job_type: matric_core::JobType::Embedding,
            })
            .unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(3), server_rx.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        // The first event we see should be JobStarted, NOT anything from WorkerStarted/Stopped
        assert!(matches!(event, ServerEvent::JobStarted { .. }));
    }

    #[tokio::test]
    async fn test_bridge_stops_on_channel_close() {
        let (worker_tx, _server_rx, _bus) = spawn_bridge().await;

        // Drop the sender — bridge should detect Closed and stop gracefully
        drop(worker_tx);

        // Give it time to process the close
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // If we reach here without hanging, the bridge exited cleanly
    }

    // =========================================================================
    // E2E EVENT FLOW INTEGRATION TESTS
    // =========================================================================

    /// Spawn a test server with note creation + SSE + bridge (worker → EventBus → SSE).
    /// Returns (base_url, event_bus, worker_tx, db).
    async fn spawn_event_flow_test_server() -> (
        String,
        Arc<EventBus>,
        tokio::sync::broadcast::Sender<WorkerEvent>,
        Database,
    ) {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        let db = Database::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");
        let event_bus = Arc::new(EventBus::new(matric_core::defaults::EVENT_BUS_CAPACITY));
        let ws_connections = Arc::new(AtomicUsize::new(0));

        // Worker broadcast channel for injecting synthetic events
        let (worker_tx, worker_rx) = tokio::sync::broadcast::channel::<WorkerEvent>(32);

        // Start bridge: worker broadcast → EventBus
        let bridge_bus = event_bus.clone();
        let bridge_db = Database::connect(&database_url).await.unwrap();
        tokio::spawn(async move {
            bridge_worker_events(worker_rx, bridge_bus, bridge_db).await;
        });

        // Start periodic queue status emitter
        let qs_bus = event_bus.clone();
        let qs_db = Database::connect(&database_url).await.unwrap();
        tokio::spawn(async move {
            emit_periodic_queue_status(qs_bus, qs_db).await;
        });

        let state = AppState {
            db: db.clone(),
            search: Arc::new(matric_search::HybridSearchEngine::new(
                Database::connect(&database_url).await.unwrap(),
            )),
            issuer: "http://localhost:3000".to_string(),
            rate_limiter: None,
            tag_resolver: matric_api::services::TagResolver::new(
                Database::connect(&database_url).await.unwrap(),
            ),
            search_cache: matric_api::services::SearchCache::disabled(),
            event_bus: event_bus.clone(),
            ws_connections,
            default_archive_cache: Arc::new(RwLock::new(DefaultArchiveCache::new(60))),
            require_auth: false,
            oauth_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_TOKEN_LIFETIME_SECS as i64,
            ),
            oauth_mcp_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_MCP_TOKEN_LIFETIME_SECS as i64,
            ),
            max_memories: matric_core::defaults::MAX_MEMORIES,
            max_upload_size: matric_core::defaults::MAX_UPLOAD_SIZE_BYTES,
            vision_backend: None,
            transcription_backend: None,
            git_sha: "test".to_string(),
            build_date: "test".to_string(),
            extraction_strategies: Vec::new(),
            schema_engines: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            database_url: String::new(),
        };

        let router = Router::new()
            .route("/api/v1/notes", post(create_note))
            .route("/api/v1/events", get(sse_events))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                archive_routing_middleware,
            ))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        (base_url, event_bus, worker_tx, db)
    }

    /// Collect SSE events from the event stream until timeout.
    /// Returns parsed JSON values for each SSE data line.
    async fn collect_sse_events(
        base_url: &str,
        timeout: std::time::Duration,
    ) -> Vec<serde_json::Value> {
        let client = reqwest::Client::new();
        let mut response = client
            .get(format!("{}/api/v1/events", base_url))
            .send()
            .await
            .unwrap();

        let mut collected = String::new();
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            let remaining =
                (deadline - tokio::time::Instant::now()).max(std::time::Duration::from_millis(1));
            match tokio::time::timeout(remaining, response.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    collected.push_str(&String::from_utf8_lossy(&chunk));
                }
                _ => break,
            }
        }

        // Parse SSE format: "event: Type\ndata: {json}\n\n"
        let mut events = Vec::new();
        for line in collected.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    events.push(parsed);
                }
            }
        }
        events
    }

    /// Create a note via POST and return the note_id.
    async fn create_test_note(base_url: &str, content: &str, tags: &[&str]) -> Uuid {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/v1/notes", base_url))
            .json(&serde_json::json!({
                "content": content,
                "tags": tags,
                "revision_mode": "none"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 201, "Failed to create note");
        let body: serde_json::Value = response.json().await.unwrap();
        Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
    }

    /// Test A: Verify that creating a note via API emits NoteUpdated via SSE.
    #[tokio::test]
    async fn test_event_flow_create_note_emits_sse() {
        let (base_url, _bus, _worker_tx, _db) = spawn_event_flow_test_server().await;
        let test_id = Uuid::new_v4();
        let content = format!("E2E test note {}", test_id);
        let tag = format!("e2e-test-{}", test_id);

        // Start SSE collector in background
        let base_url_clone = base_url.clone();
        let collector = tokio::spawn(async move {
            collect_sse_events(&base_url_clone, std::time::Duration::from_secs(3)).await
        });

        // Give SSE client time to connect and subscribe
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Create note via POST
        let note_id = create_test_note(&base_url, &content, &[&tag]).await;

        // Collect events
        let events = collector.await.unwrap();

        // Assert: at least one NoteUpdated event with matching note_id
        let note_updated = events.iter().find(|e| {
            e["type"] == "NoteUpdated" && e["note_id"].as_str() == Some(&note_id.to_string())
        });
        assert!(
            note_updated.is_some(),
            "Expected NoteUpdated event for note_id={}, got events: {:?}",
            note_id,
            events
        );
    }

    /// Test B: Full cascade — worker events bridged through EventBus to SSE.
    #[tokio::test]
    async fn test_event_flow_full_job_cascade() {
        use matric_core::JobRepository;

        let (base_url, _bus, worker_tx, db) = spawn_event_flow_test_server().await;
        let test_id = Uuid::new_v4();
        let content = format!("E2E cascade test {}", test_id);

        // Start SSE collector (longer timeout for bridge propagation)
        let base_url_clone = base_url.clone();
        let collector = tokio::spawn(async move {
            collect_sse_events(&base_url_clone, std::time::Duration::from_secs(5)).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Create note (triggers NoteUpdated + queues NLP jobs)
        let note_id = create_test_note(&base_url, &content, &[]).await;

        // Find a queued job for this note to use as job_id
        let jobs = db.jobs.get_for_note(note_id).await.unwrap();
        let job_id = jobs.first().map(|j| j.id).unwrap_or_else(Uuid::new_v4);

        // Inject synthetic worker events via broadcast channel
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        worker_tx
            .send(WorkerEvent::JobStarted {
                job_id,
                job_type: matric_core::JobType::AiRevision,
            })
            .unwrap();
        worker_tx
            .send(WorkerEvent::JobProgress {
                job_id,
                percent: 50,
                message: Some("Processing...".to_string()),
            })
            .unwrap();
        worker_tx
            .send(WorkerEvent::JobCompleted {
                job_id,
                job_type: matric_core::JobType::AiRevision,
            })
            .unwrap();

        // Collect events
        let events = collector.await.unwrap();

        // Assert all expected event types present
        let has_note_updated = events.iter().any(|e| e["type"] == "NoteUpdated");
        let has_job_started = events.iter().any(|e| e["type"] == "JobStarted");
        let has_job_progress = events.iter().any(|e| e["type"] == "JobProgress");
        let has_job_completed = events.iter().any(|e| e["type"] == "JobCompleted");
        let has_job_queued = events.iter().any(|e| e["type"] == "JobQueued");

        assert!(
            has_job_queued,
            "Missing JobQueued event. Events: {:?}",
            events
        );
        assert!(
            has_note_updated,
            "Missing NoteUpdated event. Events: {:?}",
            events
        );
        assert!(
            has_job_started,
            "Missing JobStarted event. Events: {:?}",
            events
        );
        assert!(
            has_job_progress,
            "Missing JobProgress event. Events: {:?}",
            events
        );
        assert!(
            has_job_completed,
            "Missing JobCompleted event. Events: {:?}",
            events
        );

        // Assert job_id matches in bridge-emitted events
        let started = events.iter().find(|e| e["type"] == "JobStarted").unwrap();
        assert_eq!(started["job_id"], job_id.to_string());
    }

    /// Test C: Failure path — JobFailed event propagated through bridge to SSE.
    #[tokio::test]
    async fn test_event_flow_job_failure_cascade() {
        use matric_core::JobRepository;

        let (base_url, _bus, worker_tx, db) = spawn_event_flow_test_server().await;
        let test_id = Uuid::new_v4();
        let content = format!("E2E failure test {}", test_id);

        // Start SSE collector
        let base_url_clone = base_url.clone();
        let collector = tokio::spawn(async move {
            collect_sse_events(&base_url_clone, std::time::Duration::from_secs(5)).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Create note to get a real job_id
        let note_id = create_test_note(&base_url, &content, &[]).await;

        let jobs = db.jobs.get_for_note(note_id).await.unwrap();
        let job_id = jobs.first().map(|j| j.id).unwrap_or_else(Uuid::new_v4);

        // Inject JobFailed
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        worker_tx
            .send(WorkerEvent::JobFailed {
                job_id,
                job_type: matric_core::JobType::Embedding,
                error: "test inference timeout".to_string(),
            })
            .unwrap();

        // Collect events
        let events = collector.await.unwrap();

        // Assert JobFailed event appeared with correct error message
        let job_failed = events.iter().find(|e| e["type"] == "JobFailed");
        assert!(
            job_failed.is_some(),
            "Missing JobFailed event. Events: {:?}",
            events
        );

        let failed = job_failed.unwrap();
        assert_eq!(failed["job_id"], job_id.to_string());
        assert_eq!(failed["error"], "test inference timeout");
        assert_eq!(failed["job_type"], "Embedding");
    }

    // =========================================================================
    // Memory Search endpoint tests
    // =========================================================================

    /// Spawn a memory search test server. Returns (base_url, pool) so E2E tests
    /// can insert spatial data and verify HTTP results.
    async fn spawn_memory_search_test_server() -> (String, sqlx::PgPool) {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test DB");
        let db = Database::new(pool.clone());
        let event_bus = Arc::new(EventBus::new(16));
        let ws_connections = Arc::new(AtomicUsize::new(0));

        let state = AppState {
            db,
            search: Arc::new(matric_search::HybridSearchEngine::new(Database::new(
                pool.clone(),
            ))),
            issuer: "http://localhost:3000".to_string(),
            rate_limiter: None,
            tag_resolver: matric_api::services::TagResolver::new(Database::new(pool.clone())),
            search_cache: matric_api::services::SearchCache::disabled(),
            event_bus,
            ws_connections,
            default_archive_cache: Arc::new(RwLock::new(DefaultArchiveCache::new(60))),
            require_auth: false,
            oauth_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_TOKEN_LIFETIME_SECS as i64,
            ),
            oauth_mcp_token_lifetime: chrono::Duration::seconds(
                matric_core::defaults::OAUTH_MCP_TOKEN_LIFETIME_SECS as i64,
            ),
            max_memories: matric_core::defaults::MAX_MEMORIES,
            max_upload_size: matric_core::defaults::MAX_UPLOAD_SIZE_BYTES,
            vision_backend: None,
            transcription_backend: None,
            git_sha: "test".to_string(),
            build_date: "test".to_string(),
            extraction_strategies: Vec::new(),
            schema_engines: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            database_url: String::new(),
        };

        let router = Router::new()
            .route("/api/v1/memories/search", get(search_memories))
            .route(
                "/api/v1/notes/:id/memory-provenance",
                get(get_memory_provenance_handler),
            )
            .route("/api/v1/provenance/locations", post(create_prov_location))
            .route(
                "/api/v1/provenance/named-locations",
                post(create_named_location),
            )
            .route("/api/v1/provenance/devices", post(create_prov_device))
            .route("/api/v1/provenance/files", post(create_file_provenance))
            .route("/api/v1/provenance/notes", post(create_note_provenance))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                archive_routing_middleware,
            ))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        (base_url, pool)
    }

    /// Helper: insert a test note + attachment + spatial provenance.
    /// Returns (note_id, attachment_id) for cleanup.
    async fn insert_spatial_provenance(
        pool: &sqlx::PgPool,
        lat: f64,
        lon: f64,
        capture_time: chrono::DateTime<chrono::Utc>,
    ) -> (Uuid, Uuid) {
        let note_id = Uuid::new_v4();
        let blob_id = Uuid::new_v4();
        let attachment_id = Uuid::new_v4();
        let unique_hash = format!("hash-{}", note_id);
        let unique_blob_hash = format!("blob-{}", blob_id);

        // Create note
        sqlx::query(
            "INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
             VALUES ($1, 'markdown', 'test', NOW(), NOW())",
        )
        .bind(note_id)
        .execute(pool)
        .await
        .expect("insert note");

        sqlx::query(
            "INSERT INTO note_original (note_id, content, hash) VALUES ($1, 'E2E spatial test', $2)",
        )
        .bind(note_id)
        .bind(&unique_hash)
        .execute(pool)
        .await
        .expect("insert note_original");

        sqlx::query(
            "INSERT INTO note_revised_current (note_id, content) VALUES ($1, 'E2E spatial test')",
        )
        .bind(note_id)
        .execute(pool)
        .await
        .expect("insert note_revised_current");

        // Create attachment
        sqlx::query(
            "INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data)
             VALUES ($1, $2, 'image/jpeg', 1024, 'testdata')",
        )
        .bind(blob_id)
        .bind(&unique_blob_hash)
        .execute(pool)
        .await
        .expect("insert blob");

        sqlx::query(
            "INSERT INTO attachment (id, note_id, blob_id, filename) VALUES ($1, $2, $3, 'test.jpg')",
        )
        .bind(attachment_id)
        .bind(note_id)
        .bind(blob_id)
        .execute(pool)
        .await
        .expect("insert attachment");

        // Create location
        let location_id: Uuid = sqlx::query_scalar(
            "INSERT INTO prov_location (point, source, confidence)
             VALUES (ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography, 'gps_exif', 'high')
             RETURNING id",
        )
        .bind(lon)
        .bind(lat)
        .fetch_one(pool)
        .await
        .expect("insert location");

        // Create provenance
        sqlx::query(
            "INSERT INTO provenance (attachment_id, location_id, capture_time, event_type, time_confidence)
             VALUES ($1, $2, tstzrange($3, $3, '[]'), 'photo', 'high')",
        )
        .bind(attachment_id)
        .bind(location_id)
        .bind(capture_time)
        .execute(pool)
        .await
        .expect("insert provenance");

        (note_id, attachment_id)
    }

    /// Helper: clean up spatial test data.
    async fn cleanup_spatial_provenance(pool: &sqlx::PgPool, note_id: Uuid, attachment_id: Uuid) {
        let _ = sqlx::query("DELETE FROM provenance WHERE attachment_id = $1")
            .bind(attachment_id)
            .execute(pool)
            .await;
        let blob_id: Option<Uuid> =
            sqlx::query_scalar("SELECT blob_id FROM attachment WHERE id = $1")
                .bind(attachment_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        let _ = sqlx::query("DELETE FROM attachment WHERE id = $1")
            .bind(attachment_id)
            .execute(pool)
            .await;
        if let Some(bid) = blob_id {
            let _ = sqlx::query("DELETE FROM attachment_blob WHERE id = $1")
                .bind(bid)
                .execute(pool)
                .await;
        }
        let _ = sqlx::query("DELETE FROM note_revised_current WHERE note_id = $1")
            .bind(note_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM note_original WHERE note_id = $1")
            .bind(note_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM note WHERE id = $1")
            .bind(note_id)
            .execute(pool)
            .await;
    }

    /// Memory search with no params returns 400.
    #[tokio::test]
    async fn test_memory_search_requires_params() {
        let (base_url, _pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}/api/v1/memories/search", base_url))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 400);
    }

    /// Memory search with lat+lon returns location mode.
    #[tokio::test]
    async fn test_memory_search_location_mode() {
        let (base_url, _pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=0&lon=0&radius=1000",
                base_url
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "location");
        assert!(body["count"].is_number());
        assert!(body["results"].is_array());
    }

    /// Memory search with start+end returns time mode.
    #[tokio::test]
    async fn test_memory_search_time_mode() {
        let (base_url, _pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?start=2020-01-01&end=2030-01-01",
                base_url
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "time");
        assert!(body["count"].is_number());
        assert!(body["results"].is_array());
    }

    /// Memory search with all params returns combined mode.
    #[tokio::test]
    async fn test_memory_search_combined_mode() {
        let (base_url, _pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=0&lon=0&radius=1000&start=2020-01-01&end=2030-01-01",
                base_url
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "combined");
        assert!(body["count"].is_number());
        assert!(body["results"].is_array());
    }

    /// Memory provenance for non-existent note returns empty files array.
    #[tokio::test]
    async fn test_memory_provenance_not_found() {
        let (base_url, _pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();
        let random_id = Uuid::new_v4();

        let resp = client
            .get(format!(
                "{}/api/v1/notes/{}/memory-provenance",
                base_url, random_id
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["note_id"], random_id.to_string());
        assert!(body["files"].is_array());
        assert_eq!(body["files"].as_array().unwrap().len(), 0);
    }

    // =========================================================================
    // Memory Search E2E tests (real PostGIS spatial data through HTTP API)
    // =========================================================================

    /// E2E: Insert spatial provenance at Eiffel Tower, verify location search
    /// finds it via HTTP and does NOT find it when searching from NYC.
    #[tokio::test]
    async fn test_memory_search_location_e2e() {
        let (base_url, pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();
        let now = chrono::Utc::now();

        // Insert provenance at Eiffel Tower (48.8584°N, 2.2945°E)
        let (note_id, attachment_id) = insert_spatial_provenance(&pool, 48.8584, 2.2945, now).await;

        // Search near Eiffel Tower (should find it)
        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=48.8584&lon=2.2945&radius=1000",
                base_url
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "location");
        let results = body["results"].as_array().unwrap();
        let found = results
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            found,
            "Expected to find attachment {} near Eiffel Tower, got: {:?}",
            attachment_id, results
        );

        // Verify distance is small (should be ~0m since same coordinates)
        let our_result = results
            .iter()
            .find(|r| r["attachment_id"] == attachment_id.to_string())
            .unwrap();
        let distance: f64 = our_result["distance_m"].as_f64().unwrap();
        assert!(
            distance < 100.0,
            "Distance should be near 0, got {}",
            distance
        );

        // Search from NYC (should NOT find it)
        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=40.7128&lon=-74.006&radius=1000",
                base_url
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        let results = body["results"].as_array().unwrap();
        let found_in_nyc = results
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            !found_in_nyc,
            "Eiffel Tower attachment should NOT appear in NYC search"
        );

        cleanup_spatial_provenance(&pool, note_id, attachment_id).await;
    }

    /// E2E: Insert temporal provenance, verify time search finds it via HTTP
    /// and excludes it for a non-overlapping time range.
    #[tokio::test]
    async fn test_memory_search_time_e2e() {
        let (base_url, pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();
        let yesterday = chrono::Utc::now() - chrono::Duration::days(1);

        let (note_id, attachment_id) =
            insert_spatial_provenance(&pool, 48.8584, 2.2945, yesterday).await;

        // Search time range that includes yesterday
        let start = (chrono::Utc::now() - chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();
        let end = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?start={}&end={}",
                base_url, start, end
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "time");
        let results = body["results"].as_array().unwrap();
        let found = results
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            found,
            "Expected to find attachment {} in time range, got: {:?}",
            attachment_id, results
        );

        // Search time range that excludes yesterday (5-10 days ago)
        let old_start = (chrono::Utc::now() - chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let old_end = (chrono::Utc::now() - chrono::Duration::days(5))
            .format("%Y-%m-%d")
            .to_string();

        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?start={}&end={}",
                base_url, old_start, old_end
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        let results = body["results"].as_array().unwrap();
        let found_old = results
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(!found_old, "Attachment should NOT appear in old time range");

        cleanup_spatial_provenance(&pool, note_id, attachment_id).await;
    }

    /// E2E: Insert provenance with location + time, verify combined search
    /// requires BOTH dimensions to match.
    #[tokio::test]
    async fn test_memory_search_combined_e2e() {
        let (base_url, pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();
        let yesterday = chrono::Utc::now() - chrono::Duration::days(1);

        // Provenance at Eiffel Tower, captured yesterday
        let (note_id, attachment_id) =
            insert_spatial_provenance(&pool, 48.8584, 2.2945, yesterday).await;

        let start = (chrono::Utc::now() - chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();
        let end = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // Combined: right place + right time → found
        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=48.8584&lon=2.2945&radius=1000&start={}&end={}",
                base_url, start, end
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["mode"], "combined");
        let found = body["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            found,
            "Combined search (right place + right time) should find attachment"
        );

        // Combined: wrong place + right time → NOT found
        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=40.7128&lon=-74.006&radius=1000&start={}&end={}",
                base_url, start, end
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        let found = body["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            !found,
            "Combined search (NYC + right time) should NOT find Eiffel Tower attachment"
        );

        // Combined: right place + wrong time → NOT found
        let old_start = (chrono::Utc::now() - chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        let old_end = (chrono::Utc::now() - chrono::Duration::days(5))
            .format("%Y-%m-%d")
            .to_string();
        let resp = client
            .get(format!(
                "{}/api/v1/memories/search?lat=48.8584&lon=2.2945&radius=1000&start={}&end={}",
                base_url, old_start, old_end
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        let found = body["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["attachment_id"] == attachment_id.to_string());
        assert!(
            !found,
            "Combined search (right place + old time) should NOT find attachment"
        );

        cleanup_spatial_provenance(&pool, note_id, attachment_id).await;
    }

    /// E2E: Insert full provenance chain, verify memory-provenance endpoint
    /// returns location and device data through HTTP.
    #[tokio::test]
    async fn test_memory_provenance_e2e() {
        let (base_url, pool) = spawn_memory_search_test_server().await;
        let client = reqwest::Client::new();
        let now = chrono::Utc::now();

        let (note_id, attachment_id) = insert_spatial_provenance(&pool, 48.8584, 2.2945, now).await;

        // Add device info to the provenance record
        let device_id: Uuid = sqlx::query_scalar(
            "INSERT INTO prov_agent_device (device_make, device_model)
             VALUES ('Apple', 'iPhone 15 Pro')
             ON CONFLICT (device_make, device_model, owner_id) DO UPDATE SET device_make = EXCLUDED.device_make
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert device");

        sqlx::query(
            "UPDATE provenance SET device_id = $1, event_title = 'Eiffel Tower Visit'
             WHERE attachment_id = $2",
        )
        .bind(device_id)
        .bind(attachment_id)
        .execute(&pool)
        .await
        .expect("update provenance with device");

        // Get provenance via HTTP
        let resp = client
            .get(format!(
                "{}/api/v1/notes/{}/memory-provenance",
                base_url, note_id
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["note_id"], note_id.to_string());

        let files = body["files"].as_array().unwrap();
        assert_eq!(
            files.len(),
            1,
            "Should have exactly 1 file provenance record"
        );

        let file = &files[0];
        assert_eq!(file["attachment_id"], attachment_id.to_string());
        assert_eq!(file["event_type"], "photo");
        assert_eq!(file["event_title"], "Eiffel Tower Visit");

        // Verify location data is present
        assert!(file["location"].is_object(), "Location should be present");
        let loc = &file["location"];
        let lat: f64 = loc["latitude"].as_f64().unwrap();
        let lon: f64 = loc["longitude"].as_f64().unwrap();
        assert!(
            (lat - 48.8584).abs() < 0.001,
            "Latitude should be ~48.8584, got {}",
            lat
        );
        assert!(
            (lon - 2.2945).abs() < 0.001,
            "Longitude should be ~2.2945, got {}",
            lon
        );

        // Verify device data is present
        assert!(file["device"].is_object(), "Device should be present");
        assert_eq!(file["device"]["device_make"], "Apple");
        assert_eq!(file["device"]["device_model"], "iPhone 15 Pro");

        // Cleanup
        cleanup_spatial_provenance(&pool, note_id, attachment_id).await;
        let _ = sqlx::query("DELETE FROM prov_agent_device WHERE id = $1")
            .bind(device_id)
            .execute(&pool)
            .await;
    }

    /// Verify OpenAPI spec can be generated without panics and contains expected structure.
    ///
    /// This test catches drift between handler annotations and the ApiDoc wiring.
    /// If a handler is added/removed without updating ApiDoc, this will fail.
    #[test]
    fn openapi_spec_is_valid() {
        let spec = ApiDoc::openapi();
        let yaml = spec
            .to_yaml()
            .expect("OpenAPI YAML generation must not fail");

        // Verify basic structure
        assert!(yaml.contains("openapi: 3.0"), "Must be OpenAPI 3.0.x");
        assert!(
            yaml.contains("title: Matric Memory API"),
            "Title must match"
        );
        assert!(
            yaml.contains("version: '2026.2.9'") || yaml.contains("version: 2026.2.9"),
            "Version must match"
        );

        // Verify key endpoint groups are present
        assert!(yaml.contains("/api/v1/notes"), "Notes endpoints missing");
        assert!(yaml.contains("/api/v1/search"), "Search endpoints missing");
        assert!(yaml.contains("/api/v1/concepts"), "SKOS endpoints missing");
        assert!(
            yaml.contains("/api/v1/collections"),
            "Collections endpoints missing"
        );
        assert!(
            yaml.contains("/api/v1/templates"),
            "Templates endpoints missing"
        );
        assert!(
            yaml.contains("/api/v1/embedding-sets"),
            "Embedding sets endpoints missing"
        );
        assert!(yaml.contains("/api/v1/jobs"), "Jobs endpoints missing");
        assert!(yaml.contains("/api/v1/pke"), "PKE endpoints missing");
        assert!(
            yaml.contains("/api/v1/provenance"),
            "Provenance endpoints missing"
        );
        assert!(
            yaml.contains("/api/v1/archives"),
            "Archives endpoints missing"
        );
        assert!(
            yaml.contains("/api/v1/document-types"),
            "DocumentTypes endpoints missing"
        );
        assert!(
            yaml.contains("/api/v1/attachments"),
            "Attachments endpoints missing"
        );

        // Verify tags are present
        assert!(yaml.contains("name: Notes"), "Notes tag missing");
        assert!(yaml.contains("name: SKOS"), "SKOS tag missing");
        assert!(yaml.contains("name: PKE"), "PKE tag missing");
        assert!(yaml.contains("name: Provenance"), "Provenance tag missing");

        // Verify minimum path count (sanity check against accidental mass-deletion)
        let path_count = yaml.matches("\n  /api/v1/").count();
        assert!(
            path_count >= 100,
            "Expected at least 100 API paths, found {}. Did handler annotations get removed?",
            path_count
        );
    }
}
