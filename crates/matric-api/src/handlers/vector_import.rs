//! External profile embedding run import handlers.

use std::{fmt, path::PathBuf};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    Extension, Json,
};
use matric_core::AuthPrincipal;
use serde::Deserialize;

use crate::{telemetry_text_len, ApiError, AppState, ArchiveContext, Auth};

const VECTOR_IMPORT_ROOT_ENV: &str = "FORTEMI_VECTOR_IMPORT_ROOT";
const VECTOR_IMPORT_MAX_LINE_BYTES_ENV: &str = "FORTEMI_VECTOR_IMPORT_MAX_LINE_BYTES";

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImportEmbeddingRunRequest {
    /// Server-local run folder containing manifest.json and JSONL/NDJSON files.
    pub path: String,
}

impl fmt::Debug for ImportEmbeddingRunRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportEmbeddingRunRequest")
            .field("path_len", &telemetry_text_len(&self.path))
            .finish()
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/embedding-sets/{slug}/runs",
    tag = "Embeddings",
    request_body = ImportEmbeddingRunRequest,
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 200, description = "Import run report", body = matric_db::EmbeddingImportRunReport))
)]
pub async fn import_embedding_run(
    auth: Auth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<impl IntoResponse, ApiError> {
    let repository = matric_db::PgEmbeddingImportRepository::new(state.db.pool.clone());
    let report = if is_ndjson_upload(&headers) {
        repository
            .import_run_ndjson(
                &archive_ctx.schema,
                &slug_or_id,
                body.into_data_stream(),
                vector_import_max_line_bytes(),
            )
            .await?
    } else if is_json_request(&headers) {
        require_admin_scope(&auth.principal)?;
        let body = axum::body::to_bytes(body, state.max_upload_size)
            .await
            .map_err(|error| {
                ApiError::BadRequest(format!("invalid import request body: {error}"))
            })?;
        let request: ImportEmbeddingRunRequest =
            serde_json::from_slice(&body).map_err(|error| {
                ApiError::BadRequest(format!("invalid import request JSON: {error}"))
            })?;
        let run_dir = confined_import_path(&request.path).await?;
        repository
            .import_run_folder(&archive_ctx.schema, &slug_or_id, run_dir)
            .await?
    } else {
        return Err(ApiError::BadRequest(
            "embedding import requires Content-Type application/x-ndjson or application/json"
                .to_string(),
        ));
    };
    Ok(Json(report))
}

#[utoipa::path(
    get,
    path = "/api/v1/embedding-sets/{slug}/runs",
    tag = "Embeddings",
    params(("slug" = String, Path, description = "Embedding set slug or UUID")),
    responses((status = 200, description = "Embedding import runs", body = Vec<matric_db::EmbeddingImportRunRecord>))
)]
pub async fn list_embedding_runs(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(slug_or_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let repository = matric_db::PgEmbeddingImportRepository::new(state.db.pool.clone());
    let runs = repository
        .list_runs(&archive_ctx.schema, &slug_or_id)
        .await?;
    Ok(Json(runs))
}

#[utoipa::path(
    get,
    path = "/api/v1/embedding-sets/{slug}/runs/{run_id}",
    tag = "Embeddings",
    params(
        ("slug" = String, Path, description = "Embedding set slug or UUID"),
        ("run_id" = String, Path, description = "Import run ID")
    ),
    responses((status = 200, description = "Embedding import run", body = matric_db::EmbeddingImportRunRecord))
)]
pub async fn get_embedding_run(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path((slug_or_id, run_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let repository = matric_db::PgEmbeddingImportRepository::new(state.db.pool.clone());
    let run = repository
        .get_run(&archive_ctx.schema, &slug_or_id, &run_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Embedding import run not found".to_string()))?;
    Ok(Json(run))
}

fn is_ndjson_upload(headers: &HeaderMap) -> bool {
    content_type(headers).is_some_and(|value| {
        value == "application/x-ndjson"
            || value == "application/ndjson"
            || value == "application/jsonl"
    })
}

fn is_json_request(headers: &HeaderMap) -> bool {
    content_type(headers).is_some_and(|value| value == "application/json")
}

fn content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .map(str::to_ascii_lowercase)
}

fn vector_import_max_line_bytes() -> usize {
    std::env::var(VECTOR_IMPORT_MAX_LINE_BYTES_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(matric_db::DEFAULT_VECTOR_IMPORT_MAX_LINE_BYTES)
}

pub(crate) fn require_admin_scope(principal: &AuthPrincipal) -> Result<(), ApiError> {
    if principal.has_scope("admin") {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "server-side embedding import paths require the admin scope".to_string(),
        ))
    }
}

pub(crate) async fn confined_import_path(path: &str) -> Result<PathBuf, ApiError> {
    let root = std::env::var_os(VECTOR_IMPORT_ROOT_ENV).ok_or_else(|| {
        ApiError::Forbidden(
            "server-side embedding import paths are disabled until FORTEMI_VECTOR_IMPORT_ROOT is set"
                .to_string(),
        )
    })?;
    let root = tokio::fs::canonicalize(PathBuf::from(root))
        .await
        .map_err(|error| {
            ApiError::Forbidden(format!(
                "server-side embedding import root is not readable: {error}"
            ))
        })?;
    let requested = PathBuf::from(path);
    let candidate = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    };
    let canonical = tokio::fs::canonicalize(&candidate).await.map_err(|error| {
        ApiError::BadRequest(format!(
            "server-side embedding import path is not readable: {error}"
        ))
    })?;
    if !canonical.starts_with(&root) {
        return Err(ApiError::Forbidden(
            "server-side embedding import path must stay under FORTEMI_VECTOR_IMPORT_ROOT"
                .to_string(),
        ));
    }
    Ok(canonical)
}
