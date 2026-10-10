#[cfg(feature = "hosted-auth")]
use std::collections::BTreeSet;
#[cfg(feature = "hosted-auth")]
use std::net::IpAddr;

#[cfg(not(feature = "hosted-auth"))]
use axum::extract::Path;
#[cfg(feature = "hosted-auth")]
use axum::extract::{Path, State};
#[cfg(feature = "hosted-auth")]
use axum::Extension;
use axum::Json;
#[cfg(feature = "hosted-auth")]
use chrono::Duration;
use chrono::{DateTime, Utc};
#[cfg(feature = "hosted-auth")]
use fortemi_auth_core::PrincipalKind;
#[cfg(feature = "hosted-auth")]
use matric_core::audit::{AuditEvent, AuditFailurePolicy, AuditOutcome};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "hosted-auth")]
use crate::external_oidc::{principal_can_mint_pats, RequestPrincipal};
use crate::ApiError;
#[cfg(feature = "hosted-auth")]
use crate::AppState;

#[cfg(feature = "hosted-auth")]
const DEFAULT_PAT_EXPIRY_DAYS: i64 = 30;
#[cfg(feature = "hosted-auth")]
const MAX_PAT_EXPIRY_DAYS: i64 = 365;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[cfg_attr(not(feature = "hosted-auth"), allow(dead_code))]
pub struct CreatePersonalAccessTokenRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreatedPersonalAccessTokenResponse {
    pub id: Uuid,
    pub token: String,
    pub token_prefix: String,
    pub token_last4: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PersonalAccessTokenResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_prefix: String,
    pub token_last4: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_used_ip: Option<String>,
    pub use_count: i64,
}

#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    post,
    path = "/api/v1/me/tokens",
    tag = "OAuth",
    request_body = CreatePersonalAccessTokenRequest,
    responses(
        (status = 201, description = "Created personal access token", body = CreatedPersonalAccessTokenResponse),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn create_my_token(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(request): Json<CreatePersonalAccessTokenRequest>,
) -> Result<
    (
        axum::http::StatusCode,
        Json<CreatedPersonalAccessTokenResponse>,
    ),
    ApiError,
> {
    require_oidc_human_for_mint(&principal)?;
    let tenant_id = principal_tenant(&principal)?;
    let user_id = principal_user(&principal)?;
    let scopes = requested_scopes(&request, &principal)?;
    let expires_at = requested_expiry(&request)?;
    let repo = state
        .personal_access_tokens
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("PAT storage is not configured.".into()))?;
    let created = repo
        .create(tenant_id, user_id, request.name.trim(), &scopes, expires_at)
        .await?;
    emit_pat_audit(
        &state,
        pat_audit_event(
            &principal,
            tenant_id,
            created.record.id,
            "pat.mint",
            AuditOutcome::Success,
            None,
        ),
    )
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreatedPersonalAccessTokenResponse {
            id: created.record.id,
            token: created.token,
            token_prefix: created.record.token_prefix,
            token_last4: created.record.token_last4,
            name: created.record.name,
            scopes: created.record.scopes,
            expires_at: created.record.expires_at,
            created_at: created.record.created_at,
        }),
    ))
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    post,
    path = "/api/v1/me/tokens",
    tag = "OAuth",
    request_body = CreatePersonalAccessTokenRequest,
    responses(
        (status = 201, description = "Created personal access token", body = CreatedPersonalAccessTokenResponse),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn create_my_token(
    Json(_request): Json<CreatePersonalAccessTokenRequest>,
) -> Result<Json<CreatedPersonalAccessTokenResponse>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "Personal access tokens require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    get,
    path = "/api/v1/me/tokens",
    tag = "OAuth",
    responses(
        (status = 200, description = "Personal access tokens", body = [PersonalAccessTokenResponse]),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn list_my_tokens(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<Vec<PersonalAccessTokenResponse>>, ApiError> {
    let tenant_id = principal_tenant(&principal)?;
    let user_id = principal_user(&principal)?;
    let repo = state
        .personal_access_tokens
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("PAT storage is not configured.".into()))?;
    let tokens = repo.list_for_user(tenant_id, user_id).await?;
    Ok(Json(tokens.into_iter().map(Into::into).collect()))
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    get,
    path = "/api/v1/me/tokens",
    tag = "OAuth",
    responses(
        (status = 200, description = "Personal access tokens", body = [PersonalAccessTokenResponse]),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn list_my_tokens() -> Result<Json<Vec<PersonalAccessTokenResponse>>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "Personal access tokens require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    delete,
    path = "/api/v1/me/tokens/{id}",
    tag = "OAuth",
    params(("id" = Uuid, Path, description = "Personal access token id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "Not found"),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn revoke_my_token(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    let tenant_id = principal_tenant(&principal)?;
    let user_id = principal_user(&principal)?;
    let repo = state
        .personal_access_tokens
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("PAT storage is not configured.".into()))?;
    if !repo.revoke_for_user(tenant_id, user_id, id).await? {
        return Err(ApiError::NotFound(
            "Personal access token not found.".into(),
        ));
    }
    emit_pat_audit(
        &state,
        pat_audit_event(
            &principal,
            tenant_id,
            id,
            "pat.revoke",
            AuditOutcome::Success,
            None,
        ),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    delete,
    path = "/api/v1/me/tokens/{id}",
    tag = "OAuth",
    params(("id" = Uuid, Path, description = "Personal access token id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "Not found"),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn revoke_my_token(Path(_id): Path<Uuid>) -> Result<axum::http::StatusCode, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "Personal access tokens require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    get,
    path = "/api/v1/admin/tokens",
    tag = "OAuth",
    responses(
        (status = 200, description = "Tenant personal access tokens", body = [PersonalAccessTokenResponse]),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn list_admin_tokens(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<Vec<PersonalAccessTokenResponse>>, ApiError> {
    require_human_admin(&principal)?;
    let tenant_id = principal_tenant(&principal)?;
    let repo = state
        .personal_access_tokens
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("PAT storage is not configured.".into()))?;
    let tokens = repo.list_for_tenant(tenant_id).await?;
    Ok(Json(tokens.into_iter().map(Into::into).collect()))
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    get,
    path = "/api/v1/admin/tokens",
    tag = "OAuth",
    responses(
        (status = 200, description = "Tenant personal access tokens", body = [PersonalAccessTokenResponse]),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn list_admin_tokens() -> Result<Json<Vec<PersonalAccessTokenResponse>>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "Personal access tokens require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
#[utoipa::path(
    delete,
    path = "/api/v1/admin/tokens/{id}",
    tag = "OAuth",
    params(("id" = Uuid, Path, description = "Personal access token id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "Not found"),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn revoke_admin_token(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    require_human_admin(&principal)?;
    let tenant_id = principal_tenant(&principal)?;
    let repo = state
        .personal_access_tokens
        .as_ref()
        .ok_or_else(|| ApiError::ServiceUnavailable("PAT storage is not configured.".into()))?;
    if !repo.revoke_for_admin(tenant_id, id).await? {
        return Err(ApiError::NotFound(
            "Personal access token not found.".into(),
        ));
    }
    emit_pat_audit(
        &state,
        pat_audit_event(
            &principal,
            tenant_id,
            id,
            "pat.admin_revoke",
            AuditOutcome::Success,
            None,
        ),
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    delete,
    path = "/api/v1/admin/tokens/{id}",
    tag = "OAuth",
    params(("id" = Uuid, Path, description = "Personal access token id")),
    responses(
        (status = 204, description = "Revoked"),
        (status = 404, description = "Not found"),
        (status = 503, description = "Hosted auth required")
    )
)]
pub async fn revoke_admin_token(Path(_id): Path<Uuid>) -> Result<axum::http::StatusCode, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "Personal access tokens require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
fn require_oidc_human_for_mint(principal: &RequestPrincipal) -> Result<(), ApiError> {
    if !principal_can_mint_pats(principal) {
        return Err(ApiError::Forbidden(
            "PAT minting requires an OIDC-authenticated user.".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "hosted-auth")]
fn require_human_admin(principal: &RequestPrincipal) -> Result<(), ApiError> {
    if principal.kind != PrincipalKind::Human
        || !principal.scopes.iter().any(|scope| scope == "admin")
    {
        return Err(ApiError::Forbidden(
            "Human admin principal is required.".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "hosted-auth")]
fn principal_tenant(principal: &RequestPrincipal) -> Result<Uuid, ApiError> {
    principal
        .tenant_id
        .ok_or_else(|| ApiError::Forbidden("Tenant-bound user principal is required.".into()))
}

#[cfg(feature = "hosted-auth")]
fn principal_user(principal: &RequestPrincipal) -> Result<Uuid, ApiError> {
    principal
        .user_id
        .ok_or_else(|| ApiError::Forbidden("User-bound principal is required.".into()))
}

#[cfg(feature = "hosted-auth")]
fn requested_scopes(
    request: &CreatePersonalAccessTokenRequest,
    principal: &RequestPrincipal,
) -> Result<Vec<String>, ApiError> {
    let scopes = if request.scopes.is_empty() {
        principal.scopes.clone()
    } else {
        request.scopes.clone()
    };
    let current: BTreeSet<&str> = principal.scopes.iter().map(String::as_str).collect();
    if scopes.is_empty()
        || scopes
            .iter()
            .any(|scope| !current.contains(scope.as_str()) || scope.chars().any(char::is_control))
    {
        return Err(ApiError::Forbidden(
            "PAT scopes must be a subset of the caller's current scopes.".into(),
        ));
    }
    Ok(scopes)
}

#[cfg(feature = "hosted-auth")]
fn requested_expiry(request: &CreatePersonalAccessTokenRequest) -> Result<DateTime<Utc>, ApiError> {
    match (request.expires_at, request.expires_in_days) {
        (Some(_), Some(_)) => Err(ApiError::BadRequest(
            "Use either expires_at or expires_in_days, not both.".into(),
        )),
        (Some(expires_at), None) => {
            let max = Utc::now() + Duration::days(MAX_PAT_EXPIRY_DAYS);
            if expires_at <= Utc::now() || expires_at > max {
                return Err(ApiError::BadRequest(
                    "PAT expiry must be in the future and at most 365 days.".into(),
                ));
            }
            Ok(expires_at)
        }
        (None, days) => {
            let days = days.unwrap_or(DEFAULT_PAT_EXPIRY_DAYS);
            if !(1..=MAX_PAT_EXPIRY_DAYS).contains(&days) {
                return Err(ApiError::BadRequest(
                    "PAT expires_in_days must be between 1 and 365.".into(),
                ));
            }
            Ok(Utc::now() + Duration::days(days))
        }
    }
}

#[cfg(feature = "hosted-auth")]
fn pat_audit_event(
    principal: &RequestPrincipal,
    tenant_id: Uuid,
    pat_id: Uuid,
    action: &'static str,
    outcome: AuditOutcome,
    client_ip: Option<IpAddr>,
) -> AuditEvent {
    let mut event = AuditEvent::new("identity", action, outcome)
        .with_tenant(tenant_id.to_string())
        .with_principal(
            principal
                .user_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        )
        .with_resource("personal_access_token", pat_id.to_string())
        .with_failure_policy(AuditFailurePolicy::FailClosed)
        .with_attr("iss", principal.iss.clone())
        .with_attr("sub", principal.sub.clone())
        .with_attr(
            "app_user_id",
            serde_json::json!(principal.user_id.map(|id| id.to_string())),
        )
        .with_attr("kind", principal.kind_label())
        .with_attr("credential_class", principal.credential_class.as_str())
        .with_attr("pat_id", pat_id.to_string())
        .with_attr("azp", serde_json::json!(principal.azp.clone()))
        .with_attr("jti", serde_json::json!(principal.jti.clone()));
    if let Some(client_ip) = client_ip {
        event = event.with_attr("client_ip", client_ip.to_string());
    }
    event
}

#[cfg(feature = "hosted-auth")]
pub async fn emit_pat_audit(state: &AppState, event: AuditEvent) -> Result<(), ApiError> {
    state
        .audit_sink
        .emit(event)
        .await
        .map_err(|_| ApiError::ServiceUnavailable("PAT audit is unavailable.".into()))
}

impl From<matric_db::PersonalAccessToken> for PersonalAccessTokenResponse {
    fn from(token: matric_db::PersonalAccessToken) -> Self {
        Self {
            id: token.id,
            user_id: token.user_id,
            token_prefix: token.token_prefix,
            token_last4: token.token_last4,
            name: token.name,
            scopes: token.scopes,
            status: token.status,
            expires_at: token.expires_at,
            created_at: token.created_at,
            revoked_at: token.revoked_at,
            last_used_at: token.last_used_at,
            last_used_ip: token.last_used_ip,
            use_count: token.use_count,
        }
    }
}

#[cfg(all(test, feature = "hosted-auth"))]
mod tests {
    use super::*;

    use crate::external_oidc::CredentialClass;

    fn principal(class: CredentialClass, scopes: &[&str]) -> RequestPrincipal {
        RequestPrincipal {
            user_id: Some(Uuid::new_v4()),
            tenant_id: Some(Uuid::new_v4()),
            iss: "https://issuer.example".to_string(),
            sub: "subject".to_string(),
            azp: Some("fortemi-web".to_string()),
            scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
            credential_class: class,
            kind: PrincipalKind::Human,
            pat_id: None,
            jti: None,
            email: None,
            email_verified: false,
        }
    }

    #[test]
    fn sr28_mint_requires_oidc_human_and_subset_scopes() {
        let oidc = principal(CredentialClass::Oidc, &["read"]);
        assert!(require_oidc_human_for_mint(&oidc).is_ok());
        let request = CreatePersonalAccessTokenRequest {
            name: "tool".into(),
            scopes: vec!["write".into()],
            expires_at: None,
            expires_in_days: None,
        };
        assert!(requested_scopes(&request, &oidc).is_err());

        for class in [CredentialClass::Pat, CredentialClass::Legacy] {
            let mut principal = principal(class, &["read"]);
            principal.pat_id = (class == CredentialClass::Pat).then(Uuid::new_v4);
            assert!(require_oidc_human_for_mint(&principal).is_err());
        }
    }

    #[test]
    fn sr27_expiry_defaults_and_rejects_too_long() {
        let default = requested_expiry(&CreatePersonalAccessTokenRequest {
            name: "tool".into(),
            scopes: vec![],
            expires_at: None,
            expires_in_days: None,
        })
        .unwrap();
        assert!(default > Utc::now() + Duration::days(29));

        let too_long = CreatePersonalAccessTokenRequest {
            name: "tool".into(),
            scopes: vec![],
            expires_at: None,
            expires_in_days: Some(366),
        };
        assert!(requested_expiry(&too_long).is_err());
    }
}
