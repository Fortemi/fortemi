//! Mint + revoke endpoints for short-lived SSE query tokens (#953).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde_json::json;

use crate::{ApiError, AppState, ArchiveContext, RequireAuth};

/// POST /api/v1/events/tokens — mint a short-lived EventSource query token.
///
/// Requires normal authentication. The token is bound to the current archive
/// schema and captures the caller's scope so `/api/v1/events?token=...` can make
/// the same realtime/admin decision without accepting ordinary credentials in
/// the URL.
#[utoipa::path(
    post,
    path = "/api/v1/events/tokens",
    tag = "Events",
    responses(
        (status = 201, description = "Minted: {token, token_id, expires_in}"),
        (status = 401, description = "Authentication required"),
        (status = 503, description = "Token store unavailable (Redis required to mint stream tokens)"),
    )
)]
pub async fn mint_event_stream_token(
    auth: RequireAuth,
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
) -> Result<impl IntoResponse, ApiError> {
    let minted = state
        .ingest_token_store
        .mint_event_stream(&archive_ctx.schema, auth.principal.scope_str())
        .await
        .ok_or_else(|| {
            ApiError::ServiceUnavailable(
                "event stream token store unavailable (Redis required to mint stream tokens)"
                    .to_string(),
            )
        })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": minted.token,
            "token_id": minted.token_id,
            "expires_in": minted.ttl_seconds,
        })),
    ))
}

/// DELETE /api/v1/events/tokens/{token_id} — revoke an SSE query token.
#[utoipa::path(
    delete,
    path = "/api/v1/events/tokens/{token_id}",
    tag = "Events",
    params(
        ("token_id" = String, Path, description = "Non-secret token id returned by mint"),
    ),
    responses(
        (status = 204, description = "Revoked"),
        (status = 401, description = "Authentication required"),
        (status = 404, description = "Token not found or already expired"),
    )
)]
pub async fn revoke_event_stream_token(
    _auth: RequireAuth,
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if state.ingest_token_store.revoke(&token_id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(
            "event stream token not found or already expired".to_string(),
        ))
    }
}
