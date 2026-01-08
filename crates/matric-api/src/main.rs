//! matric-api - HTTP API server for matric-memory

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Form, Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

use matric_core::{
    AuthPrincipal, AuthorizationServerMetadata, ClientRegistrationRequest, CreateApiKeyRequest,
    CreateNoteRequest, JobRepository, JobType, LinkRepository, ListNotesRequest, NoteRepository,
    OAuthError, SearchHit, TagRepository, TokenRequest, UpdateNoteStatusRequest,
};
use matric_db::Database;
use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};

/// Application state shared across handlers.
#[derive(Clone)]
struct AppState {
    db: Database,
    search: Arc<HybridSearchEngine>,
    /// OAuth2 issuer URL (base URL of the server).
    issuer: String,
}

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Matric Memory API",
        version = "0.1.0",
        description = "AI-enhanced note storage and retrieval system with semantic search"
    ),
    servers((url = "https://memory.integrolabs.net")),
    tags(
        (name = "Notes", description = "Note CRUD operations"),
        (name = "Tags", description = "Tag management"),
        (name = "Search", description = "Full-text and semantic search"),
        (name = "Jobs", description = "Background job management")
    )
)]
struct ApiDoc;

/// Serve OpenAPI YAML spec
async fn openapi_yaml() -> impl IntoResponse {
    const SPEC: &str = include_str!("openapi.yaml");
    ([(header::CONTENT_TYPE, "application/yaml")], SPEC)
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
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/matric".to_string());
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

    // Get issuer URL from environment
    let issuer =
        std::env::var("ISSUER_URL").unwrap_or_else(|_| format!("http://{}:{}", host, port));

    // Create app state
    let state = AppState { db, search, issuer };

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // OpenAPI / Swagger UI
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/openapi.yaml", get(openapi_yaml))
        // Notes CRUD
        .route("/api/v1/notes", get(list_notes).post(create_note))
        .route(
            "/api/v1/notes/:id",
            get(get_note).patch(update_note).delete(delete_note),
        )
        .route("/api/v1/notes/:id/restore", post(restore_note))
        .route(
            "/api/v1/notes/:id/tags",
            get(get_note_tags).put(set_note_tags),
        )
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

    // Queue AI processing jobs for the new note
    let _ = state
        .db
        .jobs
        .queue_deduplicated(
            Some(note_id),
            JobType::AiRevision,
            JobType::AiRevision.default_priority(),
            None,
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": note_id })),
    ))
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
    let content_changed = body.content.is_some();
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

    // Queue AI revision if content changed
    if content_changed {
        let _ = state
            .db
            .jobs
            .queue_deduplicated(
                Some(id),
                JobType::AiRevision,
                JobType::AiRevision.default_priority(),
                None,
            )
            .await;
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

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": job_id })),
    ))
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
    #[allow(dead_code)]
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
// OAUTH2 HANDLERS
// =============================================================================

/// OAuth2 authorization server metadata (RFC 8414).
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
async fn oauth_protected_resource(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "resource": state.issuer.clone(),
        "authorization_servers": [format!("{}", state.issuer)],
        "bearer_methods_supported": ["header"],
        "scopes_supported": ["read", "write", "delete", "admin", "mcp"],
    }))
}

/// OAuth2 Dynamic Client Registration (RFC 7591).
async fn oauth_register(
    State(state): State<AppState>,
    Json(req): Json<ClientRegistrationRequest>,
) -> Result<impl IntoResponse, OAuthApiError> {
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

/// OAuth2 token endpoint.
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
            let (access_token, _, token) = state
                .db
                .oauth
                .create_token(&client_id, &scope, None, false)
                .await?;

            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 3600, // 1 hour
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
            let (access_token, refresh_token, token) = state
                .db
                .oauth
                .create_token(
                    &client_id,
                    &auth_code.scope,
                    auth_code.user_id.as_deref(),
                    true,
                )
                .await?;

            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
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

            let response = matric_core::TokenResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
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
async fn list_api_keys(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let keys = state.db.oauth.list_api_keys().await?;
    Ok(Json(serde_json::json!({ "api_keys": keys })))
}

/// Create a new API key.
async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let response = state.db.oauth.create_api_key(req).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// Revoke an API key.
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
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Get Authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());

        let principal = match auth_header {
            Some(header) if header.starts_with("Bearer ") => {
                let token = header.trim_start_matches("Bearer ").trim();

                // Try to validate as OAuth access token
                if token.starts_with("mm_at_") {
                    match state.db.oauth.validate_access_token(token).await {
                        Ok(Some(oauth_token)) => AuthPrincipal::OAuthClient {
                            client_id: oauth_token.client_id,
                            scope: oauth_token.scope,
                            user_id: oauth_token.user_id,
                        },
                        _ => AuthPrincipal::Anonymous,
                    }
                }
                // Try to validate as API key
                else if token.starts_with("mm_key_") {
                    match state.db.oauth.validate_api_key(token).await {
                        Ok(Some(api_key)) => AuthPrincipal::ApiKey {
                            key_id: api_key.id,
                            scope: api_key.scope,
                        },
                        _ => AuthPrincipal::Anonymous,
                    }
                }
                // Unknown token format
                else {
                    AuthPrincipal::Anonymous
                }
            }
            _ => AuthPrincipal::Anonymous,
        };

        Ok(Auth { principal })
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
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}
