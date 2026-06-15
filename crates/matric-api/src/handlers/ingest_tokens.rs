//! Mint + revoke endpoints for stream-scoped ingest bearer tokens (#829).
//!
//! These sit behind normal authentication (`RequireAuth`) — only an
//! authenticated caller may mint a stream token, which is then presented as the
//! `Authorization: Bearer` on `POST /api/v1/ingest/stream` (which does its own
//! inline token validation). The minted token is bound to the caller's archive
//! schema (via [`ArchiveContext`]) and carries a per-token lines/sec rate limit.
//!
//! - `POST /api/v1/ingest/tokens[?rate_limit=N]` → mint (201, returns the secret
//!   token once + a non-secret `token_id` for revocation).
//! - `DELETE /api/v1/ingest/tokens/{token_id}` → revoke (204).
//!
//! Storage is the Redis-backed [`IngestTokenStore`](matric_api::services::IngestTokenStore);
//! when Redis is unavailable, mint returns `503`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{ApiError, AppState, ArchiveContext, RequireAuth};

/// Query parameters for minting a stream token. `rate_limit` is lines/sec
/// (0 = unlimited); omitted → the store's configured default.
#[derive(Debug, Default, Deserialize)]
pub struct MintIngestTokenParams {
    #[serde(default)]
    pub rate_limit: Option<u64>,
}

/// POST /api/v1/ingest/tokens — mint a stream-scoped bearer token (#829).
///
/// Requires authentication. Binds the token to the request's archive schema and
/// the given (or default) lines/sec rate limit. Returns the secret `token`
/// exactly once.
#[utoipa::path(
    post,
    path = "/api/v1/ingest/tokens",
    tag = "Ingest",
    params(
        ("rate_limit" = Option<u64>, Query, description = "Per-token rate limit in lines/sec (0 = unlimited); omitted = store default"),
    ),
    responses(
        (status = 201, description = "Minted: {token, token_id, rate_limit, expires_in}"),
        (status = 401, description = "Authentication required"),
        (status = 503, description = "Token store unavailable (Redis required to mint stream tokens)"),
    )
)]
pub async fn mint_ingest_token(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Query(params): Query<MintIngestTokenParams>,
) -> Result<impl IntoResponse, ApiError> {
    let rate_limit = params
        .rate_limit
        .unwrap_or_else(|| state.ingest_token_store.default_rate_limit());
    let minted = state
        .ingest_token_store
        .mint(&archive_ctx.schema, rate_limit)
        .await
        .ok_or_else(|| {
            ApiError::ServiceUnavailable(
                "ingest token store unavailable (Redis required to mint stream tokens)".to_string(),
            )
        })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": minted.token,
            "token_id": minted.token_id,
            "rate_limit": minted.rate_limit,
            "expires_in": minted.ttl_seconds,
        })),
    ))
}

/// DELETE /api/v1/ingest/tokens/{token_id} — revoke a stream token (#829).
///
/// Requires authentication. Takes the non-secret `token_id` from mint; deletes
/// both the token and its reverse index. `404` if the id is unknown/expired.
#[utoipa::path(
    delete,
    path = "/api/v1/ingest/tokens/{token_id}",
    tag = "Ingest",
    params(
        ("token_id" = String, Path, description = "Non-secret token id returned by mint"),
    ),
    responses(
        (status = 204, description = "Revoked"),
        (status = 401, description = "Authentication required"),
        (status = 404, description = "Token not found or already expired"),
    )
)]
pub async fn revoke_ingest_token(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if state.ingest_token_store.revoke(&token_id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(
            "ingest token not found or already expired".to_string(),
        ))
    }
}
