//! Archive management HTTP handlers.
//!
//! Provides REST API endpoints for managing parallel memory archives (Epic #441).
//! Archives provide schema-level data isolation, allowing multiple independent
//! memory spaces within the same database.

#![allow(dead_code)]

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{ApiError, AppState};
use matric_core::{ArchiveInfo, ArchiveRepository, ServerEvent};

const ARCHIVE_ALREADY_EXISTS_MESSAGE: &str = "Archive already exists.";
const ARCHIVE_NOT_FOUND_MESSAGE: &str = "Archive not found.";
const LIVE_MEMORY_LIMIT_REACHED_MESSAGE: &str =
    "Live memory limit reached. Export and delete unused memories, or increase MAX_MEMORIES.";

fn live_memory_limit_reached() -> ApiError {
    ApiError::BadRequest(LIVE_MEMORY_LIMIT_REACHED_MESSAGE.to_string())
}

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

/// Request body for creating a new archive.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateArchiveRequest {
    /// Unique name for the archive
    pub name: String,
    /// Optional description of the archive's purpose
    pub description: Option<String>,
}

impl fmt::Debug for CreateArchiveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateArchiveRequest")
            .field("name_len", &self.name.len())
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Request body for updating archive metadata.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateArchiveRequest {
    /// Updated description (or null to clear)
    pub description: Option<String>,
}

impl fmt::Debug for UpdateArchiveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateArchiveRequest")
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Request body for cloning an archive.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CloneArchiveRequest {
    /// Name for the cloned archive
    pub new_name: String,
    /// Optional description for the clone
    pub description: Option<String>,
}

impl fmt::Debug for CloneArchiveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloneArchiveRequest")
            .field("new_name_len", &self.new_name.len())
            .field(
                "description_len",
                &self.description.as_ref().map(String::len),
            )
            .finish()
    }
}

/// Response for archive statistics.
#[derive(Serialize)]
pub struct ArchiveStatsResponse {
    /// Archive name
    pub name: String,
    /// Number of notes in the archive
    pub note_count: i32,
    /// Total size in bytes
    pub size_bytes: i64,
    /// Schema name in PostgreSQL
    pub schema_name: String,
}

impl fmt::Debug for ArchiveStatsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArchiveStatsResponse")
            .field("name_len", &self.name.len())
            .field("note_count", &self.note_count)
            .field("size_bytes", &self.size_bytes)
            .field("schema_name_len", &self.schema_name.len())
            .finish()
    }
}

// =============================================================================
// HANDLERS
// =============================================================================

/// List all archives.
///
/// Returns all memory archives with their metadata, including note counts,
/// size information, and default status.
///
/// # Returns
/// - 200 OK with array of archive information
/// - 500 Internal Server Error if database query fails
#[utoipa::path(get, path = "/api/v1/archives", tag = "Archives",
    responses((status = 200, description = "Success")))]
pub async fn list_archives(
    State(state): State<AppState>,
) -> Result<Json<Vec<ArchiveInfo>>, ApiError> {
    let archives = state.db.archives.list_archive_schemas().await?;
    Ok(Json(archives))
}

/// Get a specific archive by name.
///
/// # Path Parameters
/// - `name`: Archive name
///
/// # Returns
/// - 200 OK with archive information
/// - 404 Not Found if archive doesn't exist
/// - 500 Internal Server Error if database query fails
#[utoipa::path(get, path = "/api/v1/archives/{name}", tag = "Archives",
    params(("name" = String, Path, description = "Archive name")),
    responses((status = 200, description = "Success")))]
pub async fn get_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ArchiveInfo>, ApiError> {
    let archive = state
        .db
        .archives
        .get_archive_by_name(&name)
        .await?
        .ok_or_else(|| ApiError::NotFound(ARCHIVE_NOT_FOUND_MESSAGE.to_string()))?;
    Ok(Json(archive))
}

/// Create a new archive.
///
/// Creates a new PostgreSQL schema with complete table structure for isolated
/// memory storage. Each archive maintains its own notes, embeddings, collections,
/// and tags.
///
/// # Request Body
/// JSON object with archive configuration:
/// - `name`: Unique archive name (required)
/// - `description`: Optional description
///
/// # Returns
/// - 201 Created with `{ "id": "<uuid>", "schema_name": "..." }` on success
/// - 400 Bad Request if validation fails
/// - 409 Conflict if archive name already exists
/// - 500 Internal Server Error if schema creation fails
#[utoipa::path(post, path = "/api/v1/archives", tag = "Archives",
    request_body = CreateArchiveRequest,
    responses((status = 201, description = "Created")))]
pub async fn create_archive(
    State(state): State<AppState>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    // Validate archive name
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Archive name cannot be empty".to_string(),
        ));
    }

    // Enforce MAX_MEMORIES limit on live (in-database) memories.
    // Users can export memories as shards, delete them to free slots, and re-import later.
    let current_count = state.db.archives.list_archive_schemas().await?.len() as i64;
    if current_count >= state.max_memories {
        return Err(live_memory_limit_reached());
    }

    // Check if archive already exists
    if state
        .db
        .archives
        .get_archive_by_name(&req.name)
        .await?
        .is_some()
    {
        return Err(ApiError::BadRequest(
            ARCHIVE_ALREADY_EXISTS_MESSAGE.to_string(),
        ));
    }

    let archive = state
        .db
        .archives
        .create_archive_schema(&req.name, req.description.as_deref())
        .await?;

    // Emit ArchiveCreated event (Issue #455)
    state.event_bus.emit(ServerEvent::ArchiveCreated {
        name: archive.name.clone(),
        archive_id: Some(archive.id),
    });

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": archive.id,
            "name": archive.name,
            "schema_name": archive.schema_name
        })),
    ))
}

/// Update archive metadata.
///
/// Currently supports updating the archive description. The archive name
/// and schema cannot be modified after creation.
///
/// # Path Parameters
/// - `name`: Archive name to update
///
/// # Request Body
/// JSON object with fields to update:
/// - `description`: New description (or null to clear)
///
/// # Returns
/// - 204 No Content on success
/// - 400 Bad Request if validation fails
/// - 404 Not Found if archive doesn't exist
/// - 500 Internal Server Error if database update fails
#[utoipa::path(patch, path = "/api/v1/archives/{name}", tag = "Archives",
    params(("name" = String, Path, description = "Archive name")),
    request_body = UpdateArchiveRequest,
    responses((status = 204, description = "No Content")))]
pub async fn update_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateArchiveRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .archives
        .update_archive_metadata(&name, req.description.as_deref())
        .await?;

    // Emit ArchiveUpdated event (Issue #455)
    state
        .event_bus
        .emit(ServerEvent::ArchiveUpdated { name: name.clone() });

    Ok(StatusCode::NO_CONTENT)
}

/// Delete an archive.
///
/// WARNING: This permanently deletes the entire archive schema including all
/// notes, embeddings, collections, tags, and links. This operation cannot be undone.
///
/// # Path Parameters
/// - `name`: Archive name to delete
///
/// # Returns
/// - 204 No Content on success
/// - 404 Not Found if archive doesn't exist
/// - 500 Internal Server Error if schema deletion fails
#[utoipa::path(delete, path = "/api/v1/archives/{name}", tag = "Archives",
    params(("name" = String, Path, description = "Archive name")),
    responses((status = 204, description = "No Content")))]
pub async fn delete_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Guard: cannot delete the default archive (fixes #240)
    let archive = state
        .db
        .archives
        .get_archive_by_name(&name)
        .await?
        .ok_or_else(|| ApiError::NotFound(ARCHIVE_NOT_FOUND_MESSAGE.to_string()))?;

    if archive.is_default {
        return Err(ApiError::BadRequest(
            "Cannot delete the default archive. Set another archive as default first.".into(),
        ));
    }

    state.db.archives.drop_archive_schema(&name).await?;

    // Emit ArchiveDeleted event (Issue #455)
    state
        .event_bus
        .emit(ServerEvent::ArchiveDeleted { name: name.clone() });

    Ok(StatusCode::NO_CONTENT)
}

/// Set an archive as the default.
///
/// The default archive is used when no specific archive is specified in operations.
/// Only one archive can be default at a time. Setting a new default will unset
/// the previous default.
///
/// # Path Parameters
/// - `name`: Archive name to set as default
///
/// # Returns
/// - 204 No Content on success
/// - 404 Not Found if archive doesn't exist
/// - 500 Internal Server Error if database update fails
#[utoipa::path(post, path = "/api/v1/archives/{name}/set-default", tag = "Archives",
    params(("name" = String, Path, description = "Archive name")),
    responses((status = 204, description = "No Content")))]
pub async fn set_default_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.db.archives.set_default_archive(&name).await?;

    // Invalidate default archive cache to force refresh on next request (Issue #107)
    state.default_archive_cache.write().await.invalidate();

    // Emit ArchiveDefaultChanged event (Issue #455)
    state
        .event_bus
        .emit(ServerEvent::ArchiveDefaultChanged { name: name.clone() });

    Ok(StatusCode::NO_CONTENT)
}

/// Get archive statistics.
///
/// Computes and returns current statistics for the archive, including note count
/// and storage size. This triggers a database query to calculate fresh statistics.
///
/// # Path Parameters
/// - `name`: Archive name
///
/// # Returns
/// - 200 OK with statistics object
/// - 404 Not Found if archive doesn't exist
/// - 500 Internal Server Error if stats calculation fails
#[utoipa::path(get, path = "/api/v1/archives/{name}/stats", tag = "Archives",
    params(("name" = String, Path, description = "Archive name")),
    responses((status = 200, description = "Success")))]
pub async fn get_archive_stats(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ArchiveStatsResponse>, ApiError> {
    // Update stats first
    state.db.archives.update_archive_stats(&name).await?;

    // Retrieve updated archive info
    let archive = state
        .db
        .archives
        .get_archive_by_name(&name)
        .await?
        .ok_or_else(|| ApiError::NotFound(ARCHIVE_NOT_FOUND_MESSAGE.to_string()))?;

    Ok(Json(ArchiveStatsResponse {
        name: archive.name,
        note_count: archive.note_count.unwrap_or(0),
        size_bytes: archive.size_bytes.unwrap_or(0),
        schema_name: archive.schema_name,
    }))
}

/// Clone an archive (deep copy with data).
///
/// Creates a new archive that is a complete copy of the source, including all
/// notes, embeddings, collections, tags, and links. The schema structure is
/// cloned first, then all data is bulk-copied with FK checks deferred.
///
/// # Path Parameters
/// - `name`: Source archive name to clone from
///
/// # Request Body
/// JSON object with:
/// - `new_name`: Name for the cloned archive (required)
/// - `description`: Optional description
///
/// # Returns
/// - 201 Created with new archive information
/// - 400 Bad Request if validation fails or target name already exists
/// - 404 Not Found if source archive doesn't exist
/// - 500 Internal Server Error if cloning fails
#[utoipa::path(post, path = "/api/v1/archives/{name}/clone", tag = "Archives",
    params(("name" = String, Path, description = "Source archive name")),
    request_body = CloneArchiveRequest,
    responses((status = 201, description = "Created")))]
pub async fn clone_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<CloneArchiveRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if req.new_name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Clone name cannot be empty".to_string(),
        ));
    }

    // Enforce MAX_MEMORIES limit on live memories (clone creates a new one)
    let current_count = state.db.archives.list_archive_schemas().await?.len() as i64;
    if current_count >= state.max_memories {
        return Err(live_memory_limit_reached());
    }

    let archive = state
        .db
        .archives
        .clone_archive_schema(&name, &req.new_name, req.description.as_deref())
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": archive.id,
            "name": archive.name,
            "schema_name": archive.schema_name,
            "cloned_from": name
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;
    use axum::response::IntoResponse;

    #[test]
    fn test_create_archive_request_deserialization() {
        let json = r#"{"name":"test-archive","description":"Test description"}"#;
        let req: CreateArchiveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "test-archive");
        assert_eq!(req.description, Some("Test description".to_string()));
    }

    #[test]
    fn test_create_archive_request_without_description() {
        let json = r#"{"name":"test-archive"}"#;
        let req: CreateArchiveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "test-archive");
        assert!(req.description.is_none());
    }

    #[test]
    fn test_update_archive_request_deserialization() {
        let json = r#"{"description":"Updated description"}"#;
        let req: UpdateArchiveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.description, Some("Updated description".to_string()));
    }

    #[test]
    fn test_archive_stats_response_serialization() {
        let stats = ArchiveStatsResponse {
            name: "test".to_string(),
            note_count: 42,
            size_bytes: 1024,
            schema_name: "archive_test".to_string(),
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"note_count\":42"));
        assert!(json.contains("\"size_bytes\":1024"));
    }

    #[test]
    fn archive_stats_response_debug_redacts_archive_and_schema_names() {
        let stats = ArchiveStatsResponse {
            name: "tenant-alpha/customer@example.com/postgres://user:pass@db.internal/app"
                .to_string(),
            note_count: 42,
            size_bytes: 1024,
            schema_name: "archive_tenant_alpha_private_schema_sk_live_secret".to_string(),
        };

        let rendered = format!("{stats:?}");

        assert!(rendered.contains("ArchiveStatsResponse"));
        assert!(rendered.contains("name_len"));
        assert!(rendered.contains("note_count"));
        assert!(rendered.contains("size_bytes"));
        assert!(rendered.contains("schema_name_len"));

        for raw in [
            "tenant-alpha",
            "customer@example.com",
            "postgres://user:pass",
            "db.internal",
            "archive_tenant_alpha_private_schema",
            "sk_live_secret",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }

    #[tokio::test]
    async fn archive_duplicate_validation_does_not_echo_archive_name() {
        let private_archive_name = "tenant-alpha/client-private-memory";
        let response =
            ApiError::BadRequest(ARCHIVE_ALREADY_EXISTS_MESSAGE.to_string()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/validation-error"
        );
        assert_eq!(problem["detail"], ARCHIVE_ALREADY_EXISTS_MESSAGE);

        let serialized = problem.to_string();
        assert!(!serialized.contains(private_archive_name));
        assert!(!serialized.contains("tenant-alpha"));
        assert!(!serialized.contains("client-private-memory"));
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
    }

    #[tokio::test]
    async fn live_memory_limit_validation_does_not_echo_counts() {
        let current_count = 17;
        let configured_limit = 17;
        let response = live_memory_limit_reached().into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            problem["type"],
            "https://fortemi.com/problems/validation-error"
        );
        assert_eq!(problem["detail"], LIVE_MEMORY_LIMIT_REACHED_MESSAGE);

        let serialized = problem.to_string();
        assert!(!serialized.contains(&current_count.to_string()));
        assert!(!serialized.contains(&configured_limit.to_string()));
        assert!(!serialized.contains("(17/17)"));
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
    }

    #[tokio::test]
    async fn archive_not_found_does_not_echo_archive_name() {
        let private_archive_name = "tenant-alpha/client-private-memory";
        let response = ApiError::NotFound(ARCHIVE_NOT_FOUND_MESSAGE.to_string()).into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(problem["type"], "https://fortemi.com/problems/not-found");
        assert_eq!(problem["detail"], ARCHIVE_NOT_FOUND_MESSAGE);

        let serialized = problem.to_string();
        assert!(!serialized.contains(private_archive_name));
        assert!(!serialized.contains("tenant-alpha"));
        assert!(!serialized.contains("client-private-memory"));
        assert!(problem.get("error").is_none());
        assert!(problem.get("error_description").is_none());
    }

    // =============================================================================
    // NEW UNIT TESTS FOR MULTI-MEMORY CAPABILITIES
    // =============================================================================

    #[test]
    fn test_clone_archive_request_deserialization() {
        let json = r#"{"new_name":"cloned-archive","description":"Cloned description"}"#;
        let req: CloneArchiveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.new_name, "cloned-archive");
        assert_eq!(req.description, Some("Cloned description".to_string()));
    }

    #[test]
    fn test_clone_archive_request_without_description() {
        let json = r#"{"new_name":"cloned-archive"}"#;
        let req: CloneArchiveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.new_name, "cloned-archive");
        assert!(req.description.is_none());
    }

    #[test]
    fn test_clone_archive_request_missing_new_name_fails() {
        let json = r#"{"description":"Should fail"}"#;
        let result: Result<CloneArchiveRequest, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Missing new_name should fail deserialization"
        );
    }

    #[test]
    fn test_update_archive_request_null_description() {
        let json = r#"{"description":null}"#;
        let req: UpdateArchiveRequest = serde_json::from_str(json).unwrap();
        assert!(req.description.is_none());
    }

    #[test]
    fn archive_request_debug_redacts_names_and_descriptions() {
        let create = CreateArchiveRequest {
            name: "tenant-alpha/customer@example.com/postgres://user:pass@db.internal/app"
                .to_string(),
            description: Some("Archive stored at /srv/private/mm_key_archive".to_string()),
        };
        let update = UpdateArchiveRequest {
            description: Some("Updated archive notes with sk-live-archive-secret".to_string()),
        };
        let clone = CloneArchiveRequest {
            new_name: "tenant-alpha-clone/private-path".to_string(),
            description: Some("Clone for customer@example.com".to_string()),
        };

        let rendered_create = format!("{create:?}");
        let rendered_update = format!("{update:?}");
        let rendered_clone = format!("{clone:?}");
        let combined = format!("{rendered_create}\n{rendered_update}\n{rendered_clone}");

        assert!(rendered_create.contains("CreateArchiveRequest"));
        assert!(rendered_create.contains("name_len"));
        assert!(rendered_create.contains("description_len"));
        assert!(rendered_update.contains("UpdateArchiveRequest"));
        assert!(rendered_update.contains("description_len"));
        assert!(rendered_clone.contains("CloneArchiveRequest"));
        assert!(rendered_clone.contains("new_name_len"));

        for raw in [
            "tenant-alpha",
            "customer@example.com",
            "postgres://user:pass",
            "db.internal",
            "/srv/private",
            "mm_key_archive",
            "sk-live-archive-secret",
            "tenant-alpha-clone",
            "private-path",
        ] {
            assert!(!combined.contains(raw), "raw value leaked: {raw}");
        }
    }
}
