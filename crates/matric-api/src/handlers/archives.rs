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

use crate::{ApiError, AppState};
use matric_core::{ArchiveInfo, ArchiveRepository};

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

/// Request body for creating a new archive.
#[derive(Debug, Deserialize)]
pub struct CreateArchiveRequest {
    /// Unique name for the archive
    pub name: String,
    /// Optional description of the archive's purpose
    pub description: Option<String>,
}

/// Request body for updating archive metadata.
#[derive(Debug, Deserialize)]
pub struct UpdateArchiveRequest {
    /// Updated description (or null to clear)
    pub description: Option<String>,
}

/// Request body for cloning an archive.
#[derive(Debug, Deserialize)]
pub struct CloneArchiveRequest {
    /// Name for the cloned archive
    pub new_name: String,
    /// Optional description for the clone
    pub description: Option<String>,
}

/// Response for archive statistics.
#[derive(Debug, Serialize)]
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
pub async fn get_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ArchiveInfo>, ApiError> {
    let archive = state
        .db
        .archives
        .get_archive_by_name(&name)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Archive '{}' not found", name)))?;
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

    // Enforce MAX_MEMORIES limit (only prevents creation, not growth of existing memories)
    let current_count = state.db.archives.list_archive_schemas().await?.len() as i64;
    if current_count >= state.max_memories {
        return Err(ApiError::BadRequest(format!(
            "Memory limit reached ({}/{}). Delete unused memories or increase MAX_MEMORIES.",
            current_count, state.max_memories
        )));
    }

    // Check if archive already exists
    if state
        .db
        .archives
        .get_archive_by_name(&req.name)
        .await?
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "Archive '{}' already exists",
            req.name
        )));
    }

    let archive = state
        .db
        .archives
        .create_archive_schema(&req.name, req.description.as_deref())
        .await?;

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
pub async fn delete_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.db.archives.drop_archive_schema(&name).await?;
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
pub async fn set_default_archive(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.db.archives.set_default_archive(&name).await?;

    // Invalidate default archive cache to force refresh on next request (Issue #107)
    state.default_archive_cache.write().await.invalidate();

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
        .ok_or_else(|| ApiError::NotFound(format!("Archive '{}' not found", name)))?;

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

    // Enforce MAX_MEMORIES limit (clone creates a new memory)
    let current_count = state.db.archives.list_archive_schemas().await?.len() as i64;
    if current_count >= state.max_memories {
        return Err(ApiError::BadRequest(format!(
            "Memory limit reached ({}/{}). Delete unused memories or increase MAX_MEMORIES.",
            current_count, state.max_memories
        )));
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
}
