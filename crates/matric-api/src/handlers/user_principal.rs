use axum::extract::Path;
#[cfg(feature = "hosted-auth")]
use axum::extract::State;
#[cfg(feature = "hosted-auth")]
use axum::Extension;
use axum::Json;
#[cfg(feature = "hosted-auth")]
use matric_core::audit::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSource, AuditVisibilityClass,
};
use serde::Serialize;
use uuid::Uuid;

use crate::ApiError;
#[cfg(feature = "hosted-auth")]
use crate::AppState;
#[cfg(feature = "hosted-auth")]
use fortemi_auth_core::PrincipalKind;
#[cfg(feature = "hosted-auth")]
use matric_api::external_oidc::RequestPrincipal;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    pub user_id: Option<Uuid>,
    pub iss: String,
    pub sub: String,
    pub kind: String,
    pub azp: Option<String>,
    pub credential_class: String,
    pub scopes: Vec<String>,
    pub pat_id: Option<Uuid>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub email_unverified: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdminUserResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub iss: String,
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub kind: String,
    pub azp: Option<String>,
    pub status: String,
    pub current_scopes: Vec<String>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_oidc_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "System",
    responses((status = 200, description = "Current principal", body = MeResponse))
)]
#[cfg(feature = "hosted-auth")]
pub async fn me(Extension(principal): Extension<RequestPrincipal>) -> Json<MeResponse> {
    let kind = principal.kind_label().to_string();
    Json(MeResponse {
        user_id: principal.user_id,
        iss: principal.iss,
        sub: principal.sub,
        kind,
        azp: principal.azp,
        credential_class: principal.credential_class.as_str().to_string(),
        scopes: principal.scopes,
        pat_id: principal.pat_id,
        email: principal.email,
        email_verified: principal.email_verified,
        email_unverified: !principal.email_verified,
    })
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "System",
    responses((status = 200, description = "Current principal", body = MeResponse))
)]
pub async fn me() -> Result<Json<MeResponse>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "User principals require the hosted-auth build profile.".into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "System",
    responses((status = 200, description = "Application users", body = [AdminUserResponse]))
)]
#[cfg(feature = "hosted-auth")]
pub async fn list_users(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<Vec<AdminUserResponse>>, ApiError> {
    require_human_admin(&principal)?;
    let tenant_id = principal_tenant(&principal)?;
    let users = state.app_users.list(tenant_id).await?;
    Ok(Json(
        users.into_iter().map(AdminUserResponse::from).collect(),
    ))
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "System",
    responses((status = 200, description = "Application users", body = [AdminUserResponse]))
)]
pub async fn list_users() -> Result<Json<Vec<AdminUserResponse>>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "User principals require the hosted-auth build profile.".into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/disable",
    tag = "System",
    params(("id" = Uuid, Path, description = "Application user id")),
    responses((status = 200, description = "Disabled user", body = AdminUserResponse))
)]
#[cfg(feature = "hosted-auth")]
pub async fn disable_user(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<AdminUserResponse>, ApiError> {
    require_human_admin(&principal)?;
    set_user_status(state, principal, id, "disabled").await
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/disable",
    tag = "System",
    params(("id" = Uuid, Path, description = "Application user id")),
    responses((status = 200, description = "Disabled user", body = AdminUserResponse))
)]
pub async fn disable_user(Path(_id): Path<Uuid>) -> Result<Json<AdminUserResponse>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "User principals require the hosted-auth build profile.".into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/enable",
    tag = "System",
    params(("id" = Uuid, Path, description = "Application user id")),
    responses((status = 200, description = "Enabled user", body = AdminUserResponse))
)]
#[cfg(feature = "hosted-auth")]
pub async fn enable_user(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<AdminUserResponse>, ApiError> {
    require_human_admin(&principal)?;
    set_user_status(state, principal, id, "active").await
}

#[cfg(not(feature = "hosted-auth"))]
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{id}/enable",
    tag = "System",
    params(("id" = Uuid, Path, description = "Application user id")),
    responses((status = 200, description = "Enabled user", body = AdminUserResponse))
)]
pub async fn enable_user(Path(_id): Path<Uuid>) -> Result<Json<AdminUserResponse>, ApiError> {
    Err(ApiError::ServiceUnavailable(
        "User principals require the hosted-auth build profile.".into(),
    ))
}

#[cfg(feature = "hosted-auth")]
async fn set_user_status(
    state: AppState,
    principal: RequestPrincipal,
    id: Uuid,
    status: &'static str,
) -> Result<Json<AdminUserResponse>, ApiError> {
    let tenant_id = principal_tenant(&principal)?;
    if status == "disabled" {
        revoke_user_pats_for_disable(&state, tenant_id, id).await?;
    }
    let user = state
        .app_users
        .set_status(tenant_id, id, status)
        .await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    let event = user_lifecycle_audit(&principal, tenant_id, id, status);
    state
        .audit_sink
        .emit(event)
        .await
        .map_err(|_| ApiError::ServiceUnavailable("User lifecycle audit is unavailable.".into()))?;
    Ok(Json(AdminUserResponse::from(user)))
}

#[cfg(feature = "hosted-auth")]
async fn revoke_user_pats_for_disable(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let Some(repo) = state.personal_access_tokens.as_ref() else {
        return Ok(());
    };
    repo.revoke_for_disabled_user(tenant_id, user_id).await?;
    Ok(())
}

#[cfg(feature = "hosted-auth")]
fn principal_tenant(principal: &RequestPrincipal) -> Result<Uuid, ApiError> {
    principal
        .tenant_id
        .ok_or_else(|| ApiError::Forbidden("Tenant-bound user principal is required.".into()))
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
fn user_lifecycle_audit(
    principal: &RequestPrincipal,
    tenant_id: Uuid,
    user_id: Uuid,
    status: &str,
) -> AuditEvent {
    let mut event = AuditEvent::new("identity", format!("user.{status}"), AuditOutcome::Success)
        .with_tenant(tenant_id.to_string())
        .with_resource("app_user", user_id.to_string())
        .with_failure_policy(AuditFailurePolicy::FailClosed)
        .with_attr("iss", principal.iss.clone())
        .with_attr("sub", principal.sub.clone())
        .with_attr(
            "app_user_id",
            serde_json::json!(principal.user_id.map(|id| id.to_string())),
        )
        .with_attr("kind", principal.kind_label())
        .with_attr("credential_class", principal.credential_class.as_str())
        .with_attr("azp", serde_json::json!(principal.azp.clone()))
        .with_attr(
            "pat_id",
            serde_json::json!(principal.pat_id.map(|id| id.to_string())),
        )
        .with_attr("jti", serde_json::json!(principal.jti.clone()));
    event.severity = AuditSeverity::Info;
    event.visibility = AuditVisibilityClass::SystemAudit;
    event.source = AuditSource::Api;
    event
}

impl From<matric_db::AppUser> for AdminUserResponse {
    fn from(user: matric_db::AppUser) -> Self {
        Self {
            id: user.id,
            tenant_id: user.tenant_id,
            iss: user.iss,
            sub: user.sub,
            email: user.email,
            email_verified: user.email_verified,
            kind: user.kind,
            azp: user.azp,
            status: user.status,
            current_scopes: user.current_scopes,
            first_seen_at: user.first_seen_at,
            last_seen_at: user.last_seen_at,
            last_oidc_at: user.last_oidc_at,
        }
    }
}

#[cfg(all(test, feature = "hosted-auth"))]
mod tests {
    use super::*;
    use crate::external_oidc::CredentialClass;

    fn principal(kind: PrincipalKind, email_verified: bool) -> RequestPrincipal {
        RequestPrincipal {
            user_id: Some(Uuid::new_v4()),
            tenant_id: Some(Uuid::new_v4()),
            iss: "https://issuer.example".to_string(),
            sub: "subject".to_string(),
            azp: Some("fortemi-web".to_string()),
            scopes: vec!["admin".to_string()],
            credential_class: CredentialClass::Oidc,
            kind,
            pat_id: None,
            jti: Some("jti".to_string()),
            email: Some("user@example.com".to_string()),
            email_verified,
        }
    }

    #[tokio::test]
    async fn me_reports_oidc_email_verification_state() {
        let Json(verified) = me(Extension(principal(PrincipalKind::Human, true))).await;
        assert_eq!(verified.kind, "user");
        assert_eq!(verified.email.as_deref(), Some("user@example.com"));
        assert!(verified.email_verified);
        assert!(!verified.email_unverified);

        let Json(unverified) = me(Extension(principal(PrincipalKind::Human, false))).await;
        assert!(!unverified.email_verified);
        assert!(unverified.email_unverified);
    }

    #[tokio::test]
    async fn me_reports_service_principal_kind() {
        let Json(body) = me(Extension(principal(PrincipalKind::Service, false))).await;

        assert_eq!(body.kind, "service");
        assert_eq!(body.credential_class, "oidc");
    }

    #[test]
    fn service_principal_cannot_use_admin_user_endpoints() {
        let principal = principal(PrincipalKind::Service, false);

        assert!(matches!(
            require_human_admin(&principal),
            Err(ApiError::Forbidden(_))
        ));
    }
}
