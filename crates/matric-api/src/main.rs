//! matric-api - HTTP API server for matric-memory

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use matric_core::{
    CreateNoteRequest, JobRepository, JobType, LinkRepository,
    ListNotesRequest, NoteRepository, SearchHit, TagRepository, UpdateNoteStatusRequest,
};
use matric_db::Database;
use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};

/// Application state shared across handlers.
#[derive(Clone)]
struct AppState {
    db: Database,
    search: Arc<HybridSearchEngine>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "matric_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Get configuration from environment
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/matric".to_string());
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(3000);

    // Connect to database
    info!("Connecting to database...");
    let db = Database::connect(&database_url).await?;
    info!("Database connected");

    // Create search engine
    let search = Arc::new(HybridSearchEngine::new(db.clone()));

    // Create app state
    let state = AppState { db, search };

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Notes CRUD
        .route("/api/v1/notes", get(list_notes).post(create_note))
        .route(
            "/api/v1/notes/:id",
            get(get_note).patch(update_note).delete(delete_note),
        )
        .route("/api/v1/notes/:id/restore", post(restore_note))
        .route("/api/v1/notes/:id/tags", get(get_note_tags).put(set_note_tags))
        .route("/api/v1/notes/:id/links", get(get_note_links))
        // Search
        .route("/api/v1/search", get(search_notes))
        // Note status shortcut
        .route("/api/v1/notes/:id/status", patch(update_note_status))
        // Jobs
        .route("/api/v1/jobs", get(list_jobs).post(create_job))
        .route("/api/v1/jobs/:id", get(get_job))
        .route("/api/v1/jobs/pending", get(pending_jobs_count))
        // Tags
        .route("/api/v1/tags", get(list_tags))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// =============================================================================
// HEALTH CHECK
// =============================================================================

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

// =============================================================================
// NOTE HANDLERS
// =============================================================================

#[derive(Debug, Deserialize)]
struct ListNotesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    filter: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    collection_id: Option<Uuid>,
}

async fn list_notes(
    State(state): State<AppState>,
    Query(query): Query<ListNotesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let req = ListNotesRequest {
        limit: query.limit,
        offset: query.offset,
        filter: query.filter,
        sort_by: query.sort_by,
        sort_order: query.sort_order,
        collection_id: query.collection_id,
        tags: None,
    };

    let response = state.db.notes.list(req).await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
struct CreateNoteBody {
    content: String,
    format: Option<String>,
    source: Option<String>,
    collection_id: Option<Uuid>,
    tags: Option<Vec<String>>,
}

async fn create_note(
    State(state): State<AppState>,
    Json(body): Json<CreateNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = CreateNoteRequest {
        content: body.content,
        format: body.format.unwrap_or_else(|| "markdown".to_string()),
        source: body.source.unwrap_or_else(|| "api".to_string()),
        collection_id: body.collection_id,
        tags: body.tags,
    };

    let note_id = state.db.notes.insert(req).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": note_id }))))
}

async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let note = state.db.notes.fetch(id).await?;
    Ok(Json(note))
}

#[derive(Debug, Deserialize)]
struct UpdateNoteBody {
    content: Option<String>,
    starred: Option<bool>,
    archived: Option<bool>,
}

async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Update content if provided
    if let Some(content) = body.content {
        state.db.notes.update_original(id, &content).await?;
    }

    // Update status if provided
    if body.starred.is_some() || body.archived.is_some() {
        let req = UpdateNoteStatusRequest {
            starred: body.starred,
            archived: body.archived,
        };
        state.db.notes.update_status(id, req).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.notes.soft_delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct UpdateStatusBody {
    starred: Option<bool>,
    archived: Option<bool>,
}

async fn update_note_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateStatusBody>,
) -> Result<impl IntoResponse, ApiError> {
    let req = UpdateNoteStatusRequest {
        starred: body.starred,
        archived: body.archived,
    };
    state.db.notes.update_status(id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.notes.restore(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// TAG HANDLERS
// =============================================================================

async fn get_note_tags(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let tags = state.db.tags.get_for_note(id).await?;
    Ok(Json(tags))
}

#[derive(Debug, Deserialize)]
struct SetTagsBody {
    tags: Vec<String>,
}

async fn set_note_tags(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetTagsBody>,
) -> Result<impl IntoResponse, ApiError> {
    state.db.tags.set_for_note(id, body.tags, "api").await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_tags(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let tags = state.db.tags.list().await?;
    Ok(Json(tags))
}

// =============================================================================
// LINK HANDLERS
// =============================================================================

#[derive(Debug, Serialize)]
struct NoteLinksResponse {
    outgoing: Vec<matric_core::Link>,
    incoming: Vec<matric_core::Link>,
}

async fn get_note_links(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let outgoing = state.db.links.get_outgoing(id).await?;
    let incoming = state.db.links.get_incoming(id).await?;
    Ok(Json(NoteLinksResponse { outgoing, incoming }))
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
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchHit>,
    query: String,
    total: usize,
}

async fn search_notes(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(20);

    let config = match query.mode.as_deref() {
        Some("fts") => HybridSearchConfig::fts_only(),
        Some("semantic") => HybridSearchConfig::semantic_only(),
        _ => HybridSearchConfig::default(),
    };

    let mut request = SearchRequest::new(&query.q)
        .with_limit(limit)
        .with_config(config);

    if let Some(filters) = &query.filters {
        request = request.with_filters(filters);
    }

    let results = request.execute(&state.search).await?;
    let total = results.len();

    Ok(Json(SearchResponse {
        results,
        query: query.q,
        total,
    }))
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
}

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
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid job type: {}",
                body.job_type
            )))
        }
    };

    let priority = body.priority.unwrap_or_else(|| job_type.default_priority());
    let job_id = state
        .db
        .jobs
        .queue(body.note_id, job_type, priority, body.payload)
        .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": job_id }))))
}

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

async fn pending_jobs_count(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let count = state.db.jobs.pending_count().await?;
    Ok(Json(serde_json::json!({ "pending": count })))
}

#[derive(Debug, Deserialize)]
struct ListJobsQuery {
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<ListJobsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50);
    let jobs = state.db.jobs.list_recent(limit).await?;
    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

#[derive(Debug)]
enum ApiError {
    Database(matric_core::Error),
    NotFound(String),
    BadRequest(String),
}

impl From<matric_core::Error> for ApiError {
    fn from(err: matric_core::Error) -> Self {
        match &err {
            matric_core::Error::NotFound(msg) => ApiError::NotFound(msg.clone()),
            _ => ApiError::Database(err),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::Database(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}
