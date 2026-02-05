//! Document type HTTP handlers.
//!
//! Provides REST API endpoints for managing document types in the registry.
//! Document types control content-aware chunking, embedding model selection,
//! and automatic detection based on file extensions or content patterns.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{ApiError, AppState};
use matric_core::DocumentTypeRepository;

// NOTE: These types must be defined in matric-core before handlers will compile.
// See ADR-025 for the complete specification.
//
// Required in matric-core/src/models.rs:
// - DocumentType
// - DocumentTypeSummary
// - CreateDocumentTypeRequest
// - UpdateDocumentTypeRequest
// - DetectDocumentTypeResult
//
// Required implementation in matric-db/src/document_types.rs:
// - PgDocumentTypeRepository implementing DocumentTypeRepository trait

/// Query parameters for listing document types.
#[derive(Debug, Deserialize)]
pub struct ListDocumentTypesQuery {
    /// Filter by category: 'code', 'markup', 'config', 'prose', 'data'
    pub category: Option<String>,
}

/// Request body for document type detection.
#[derive(Debug, Deserialize)]
pub struct DetectDocumentTypeRequest {
    /// Optional filename to match against file extensions
    pub filename: Option<String>,
    /// Optional content sample for magic pattern matching
    pub content: Option<String>,
}

/// List all document types, optionally filtered by category.
///
/// # Query Parameters
/// - `category`: Filter by document category (optional)
///
/// # Returns
/// - 200 OK with array of document type summaries
/// - 500 Internal Server Error if database query fails
pub async fn list_document_types(
    State(state): State<AppState>,
    Query(query): Query<ListDocumentTypesQuery>,
) -> Result<Json<Vec<matric_core::DocumentTypeSummary>>, ApiError> {
    let types = if let Some(category) = query.category {
        state.db.document_types.list_by_category(&category).await?
    } else {
        state.db.document_types.list().await?
    };
    Ok(Json(types))
}

/// Get a document type by name.
///
/// # Path Parameters
/// - `name`: Unique document type name (e.g., "rust", "markdown")
///
/// # Returns
/// - 200 OK with full document type details
/// - 404 Not Found if document type doesn't exist
/// - 500 Internal Server Error if database query fails
pub async fn get_document_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<matric_core::DocumentType>, ApiError> {
    let doc_type = state
        .db
        .document_types
        .get_by_name(&name)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Document type '{}' not found", name)))?;
    Ok(Json(doc_type))
}

/// Create a new document type.
///
/// # Request Body
/// JSON object with document type configuration (see CreateDocumentTypeRequest)
///
/// # Returns
/// - 201 Created with `{ "id": "<uuid>" }` on success
/// - 400 Bad Request if validation fails
/// - 409 Conflict if document type name already exists
/// - 500 Internal Server Error if database insert fails
pub async fn create_document_type(
    State(state): State<AppState>,
    Json(req): Json<matric_core::CreateDocumentTypeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let id = state.db.document_types.create(req).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

/// Update an existing document type.
///
/// # Path Parameters
/// - `name`: Unique document type name to update
///
/// # Request Body
/// JSON object with fields to update (see UpdateDocumentTypeRequest)
///
/// # Returns
/// - 204 No Content on success
/// - 400 Bad Request if validation fails
/// - 403 Forbidden if attempting to update system document type
/// - 404 Not Found if document type doesn't exist
/// - 500 Internal Server Error if database update fails
pub async fn update_document_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<matric_core::UpdateDocumentTypeRequest>,
) -> Result<StatusCode, ApiError> {
    state.db.document_types.update(&name, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete a document type.
///
/// Only non-system document types can be deleted. System types
/// (e.g., "markdown", "plaintext") are protected.
///
/// # Path Parameters
/// - `name`: Unique document type name to delete
///
/// # Returns
/// - 204 No Content on success
/// - 403 Forbidden if attempting to delete system document type
/// - 404 Not Found if document type doesn't exist
/// - 500 Internal Server Error if database delete fails
pub async fn delete_document_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.db.document_types.delete(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Detect document type from filename and/or content.
///
/// Uses file extension matching and magic pattern detection to identify
/// the most appropriate document type for the given input.
///
/// # Request Body
/// - `filename`: Optional filename with extension
/// - `content`: Optional content sample for pattern matching
///
/// # Returns
/// - 200 OK with detection result (null if no match found)
/// - 400 Bad Request if both filename and content are missing
/// - 500 Internal Server Error if detection fails
pub async fn detect_document_type(
    State(state): State<AppState>,
    Json(req): Json<DetectDocumentTypeRequest>,
) -> Result<Json<Option<matric_core::DetectDocumentTypeResult>>, ApiError> {
    // Validate that at least one detection input is provided
    if req.filename.is_none() && req.content.is_none() {
        return Err(ApiError::BadRequest(
            "At least one of 'filename' or 'content' must be provided".to_string(),
        ));
    }

    let result = state
        .db
        .document_types
        .detect(req.filename.as_deref(), req.content.as_deref())
        .await?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    // NOTE: Unit tests require the following to be implemented first:
    // 1. DocumentType models in matric-core
    // 2. PgDocumentTypeRepository in matric-db
    // 3. Test fixtures/mocks for the repository
    //
    // Once prerequisites are complete, add tests for:
    // - list_document_types (all + filtered by category)
    // - get_document_type (found + not found)
    // - create_document_type (success + validation errors)
    // - update_document_type (success + forbidden for system types)
    // - delete_document_type (success + forbidden for system types)
    // - detect_document_type (by filename, by content, both, neither)
}
