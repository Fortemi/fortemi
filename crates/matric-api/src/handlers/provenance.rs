//! Provenance creation HTTP handlers (Issue #261, #262).
//!
//! Provides REST API endpoints for creating provenance records:
//! - Location records (prov_location)
//! - Named locations
//! - Device records (prov_agent_device)
//! - File provenance linking attachments to spatial-temporal context
//! - Note provenance linking notes to spatial-temporal context (#262)

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde_json::json;

use crate::{ApiError, AppState, ArchiveContext};
use matric_core::{
    CreateFileProvenanceRequest, CreateNamedLocationRequest, CreateNoteProvenanceRequest,
    CreateProvDeviceRequest, CreateProvLocationRequest,
};

/// Create a provenance location record.
///
/// POST /api/v1/provenance/locations
pub async fn create_prov_location(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(req): Json<CreateProvLocationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let id = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.create_prov_location_tx(tx, &req).await })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

/// Create a named location.
///
/// POST /api/v1/provenance/named-locations
pub async fn create_named_location(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(req): Json<CreateNamedLocationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let result = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.create_named_location_tx(tx, &req).await })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// Create a provenance device record.
///
/// POST /api/v1/provenance/devices
pub async fn create_prov_device(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(req): Json<CreateProvDeviceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let device = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.create_prov_agent_device_tx(tx, &req).await })
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": device.id,
            "device_make": device.device_make,
            "device_model": device.device_model,
            "device_os": device.device_os,
            "device_os_version": device.device_os_version,
            "software": device.software,
            "software_version": device.software_version,
            "device_name": device.device_name,
        })),
    ))
}

/// Create a file provenance record linking an attachment to spatial-temporal context.
///
/// POST /api/v1/provenance/files
pub async fn create_file_provenance(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(req): Json<CreateFileProvenanceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let id = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.create_file_provenance_tx(tx, &req).await })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

/// Create a note provenance record linking a note to spatial-temporal context.
///
/// POST /api/v1/provenance/notes
pub async fn create_note_provenance(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(req): Json<CreateNoteProvenanceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let memory_search = matric_db::PgMemorySearchRepository::new(state.db.pool.clone());
    let id = ctx
        .query(move |tx| {
            Box::pin(async move { memory_search.create_note_provenance_tx(tx, &req).await })
        })
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}
