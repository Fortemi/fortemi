//! Hosted-only user credential lifecycle handlers.

use std::fmt;

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use matric_api::services::{
    normalize_user_secret_name, normalize_user_secret_provider, seal_user_secret, user_secret_mask,
    UserSecretServiceError,
};
use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass, AuthPrincipal,
};
use matric_db::{PgUserSecretRepository, UserSecretMetadata};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::middleware::tenant_scope::TenantRequestScope;
use crate::{ApiError, AppState, Auth};

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct CreateUserSecretRequest {
    provider: String,
    name: String,
    key: String,
}

impl fmt::Debug for CreateUserSecretRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateUserSecretRequest")
            .field("provider_len", &self.provider.chars().count())
            .field("name_len", &self.name.chars().count())
            .field("key_present", &!self.key.is_empty())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct UserSecretResponse {
    id: Uuid,
    provider: String,
    name: String,
    masked: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    rewrapped_at: Option<DateTime<Utc>>,
}

impl From<UserSecretMetadata> for UserSecretResponse {
    fn from(metadata: UserSecretMetadata) -> Self {
        let masked = user_secret_mask(&metadata.provider);
        Self {
            id: metadata.id,
            provider: metadata.provider,
            name: metadata.name,
            masked,
            created_at: metadata.created_at,
            last_used_at: metadata.last_used_at,
            revoked_at: metadata.revoked_at,
            rewrapped_at: metadata.rewrapped_at,
        }
    }
}

pub async fn create_user_secret(
    auth: Auth,
    State(state): State<AppState>,
    Extension(scope): Extension<TenantRequestScope>,
    Json(mut request): Json<CreateUserSecretRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = hosted_user_id(&auth)?;
    let tenant_id = hosted_secret_tenant(&state, &scope)?;
    let secret_id = Uuid::now_v7();
    let provider = match normalize_user_secret_provider(&request.provider) {
        Ok(provider) => provider,
        Err(error) => {
            return Err(audited_user_credential_error(
                &state, tenant_id, &user_id, "create", None, error,
            )
            .await);
        }
    };
    let name = match normalize_user_secret_name(&request.name) {
        Ok(name) => name,
        Err(error) => {
            return Err(audited_user_credential_error(
                &state, tenant_id, &user_id, "create", None, error,
            )
            .await);
        }
    };
    let Some(key_provider) = state.key_provider.as_deref() else {
        return Err(audited_user_credential_error(
            &state,
            tenant_id,
            &user_id,
            "create",
            Some(secret_id),
            UserSecretServiceError::InvalidContext,
        )
        .await);
    };
    let sealed = match seal_user_secret(
        key_provider,
        tenant_id,
        &user_id,
        secret_id,
        &provider,
        &request.key,
    )
    .await
    {
        Ok(sealed) => sealed,
        Err(error) => {
            return Err(audited_user_credential_error(
                &state,
                tenant_id,
                &user_id,
                "create",
                Some(secret_id),
                error,
            )
            .await);
        }
    };
    request.key.zeroize();

    let encrypted_blob = sealed.encrypted_blob;
    let stored_provider = provider.clone();
    let stored_user_id = user_id.clone();
    let metadata = match scope
        .with_connection(move |connection| {
            Box::pin(async move {
                PgUserSecretRepository::create_tx(
                    connection,
                    secret_id,
                    tenant_id,
                    &stored_user_id,
                    &stored_provider,
                    &name,
                    encrypted_blob,
                )
                .await
            })
        })
        .await
    {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(audited_user_credential_api_error(
                &state,
                tenant_id,
                &user_id,
                "create",
                Some(secret_id),
                "storage_failure",
                ApiError::from(error),
            )
            .await);
        }
    };

    emit_user_credential_audit(
        state.audit_sink.as_ref(),
        user_credential_audit_event(
            tenant_id,
            &user_id,
            "create",
            AuditOutcome::Success,
            Some(secret_id),
            None,
            None,
        ),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(UserSecretResponse::from(metadata)),
    ))
}

pub async fn list_user_secrets(
    auth: Auth,
    State(state): State<AppState>,
    Extension(scope): Extension<TenantRequestScope>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = hosted_user_id(&auth)?;
    let tenant_id = hosted_secret_tenant(&state, &scope)?;
    let stored_user_id = user_id.clone();
    let metadata = match scope
        .with_connection(move |connection| {
            Box::pin(async move {
                PgUserSecretRepository::list_tx(connection, tenant_id, &stored_user_id).await
            })
        })
        .await
    {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(audited_user_credential_api_error(
                &state,
                tenant_id,
                &user_id,
                "list",
                None,
                "storage_failure",
                ApiError::from(error),
            )
            .await);
        }
    };
    let count = metadata.len();

    emit_user_credential_audit(
        state.audit_sink.as_ref(),
        user_credential_audit_event(
            tenant_id,
            &user_id,
            "list",
            AuditOutcome::Success,
            None,
            Some(count),
            None,
        ),
    )
    .await?;

    Ok(Json(
        metadata
            .into_iter()
            .map(UserSecretResponse::from)
            .collect::<Vec<_>>(),
    ))
}

pub async fn revoke_user_secret(
    auth: Auth,
    State(state): State<AppState>,
    Extension(scope): Extension<TenantRequestScope>,
    Path(secret_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let user_id = hosted_user_id(&auth)?;
    let tenant_id = hosted_secret_tenant(&state, &scope)?;
    let stored_user_id = user_id.clone();
    if let Err(error) = scope
        .with_connection(move |connection| {
            Box::pin(async move {
                PgUserSecretRepository::revoke_tx(connection, tenant_id, &stored_user_id, secret_id)
                    .await
                    .map(|_| ())
            })
        })
        .await
    {
        return Err(audited_user_credential_api_error(
            &state,
            tenant_id,
            &user_id,
            "revoke",
            Some(secret_id),
            "storage_failure",
            ApiError::from(error),
        )
        .await);
    }

    emit_user_credential_audit(
        state.audit_sink.as_ref(),
        user_credential_audit_event(
            tenant_id,
            &user_id,
            "revoke",
            AuditOutcome::Success,
            Some(secret_id),
            None,
            None,
        ),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn hosted_user_id(auth: &Auth) -> Result<String, ApiError> {
    match &auth.principal {
        AuthPrincipal::OAuthClient {
            user_id: Some(user_id),
            ..
        } => Ok(user_id.clone()),
        _ => Err(ApiError::Forbidden(
            "A user-bound OAuth identity is required.".to_string(),
        )),
    }
}

fn hosted_secret_tenant(state: &AppState, scope: &TenantRequestScope) -> Result<Uuid, ApiError> {
    if !state.multi_tenant {
        return Err(ApiError::NotFound("Route not found.".to_string()));
    }
    Ok(scope.tenant().tenant_id())
}

fn user_secret_service_api_error(error: UserSecretServiceError) -> ApiError {
    match error {
        UserSecretServiceError::InvalidProvider
        | UserSecretServiceError::InvalidName
        | UserSecretServiceError::InvalidSecret => {
            ApiError::BadRequest("Stored credential input is invalid.".to_string())
        }
        UserSecretServiceError::InvalidContext
        | UserSecretServiceError::InvalidEnvelope
        | UserSecretServiceError::KeyOperation { .. } => {
            ApiError::ServiceUnavailable("Credential key service is unavailable.".to_string())
        }
    }
}

fn user_secret_failure_reason(error: UserSecretServiceError) -> &'static str {
    match error {
        UserSecretServiceError::InvalidProvider => "invalid_provider",
        UserSecretServiceError::InvalidName => "invalid_name",
        UserSecretServiceError::InvalidSecret => "invalid_credential",
        UserSecretServiceError::InvalidContext => "invalid_context",
        UserSecretServiceError::InvalidEnvelope => "invalid_envelope",
        UserSecretServiceError::KeyOperation {
            retryable: true, ..
        } => "key_service_temporarily_unavailable",
        UserSecretServiceError::KeyOperation {
            retryable: false, ..
        } => "key_operation_denied",
    }
}

async fn audited_user_credential_error(
    state: &AppState,
    tenant_id: Uuid,
    user_id: &str,
    action: &'static str,
    credential_id: Option<Uuid>,
    error: UserSecretServiceError,
) -> ApiError {
    let reason = user_secret_failure_reason(error);
    audited_user_credential_api_error(
        state,
        tenant_id,
        user_id,
        action,
        credential_id,
        reason,
        user_secret_service_api_error(error),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn audited_user_credential_api_error(
    state: &AppState,
    tenant_id: Uuid,
    user_id: &str,
    action: &'static str,
    credential_id: Option<Uuid>,
    reason: &'static str,
    error: ApiError,
) -> ApiError {
    let event = user_credential_audit_event(
        tenant_id,
        user_id,
        action,
        AuditOutcome::Failure,
        credential_id,
        None,
        Some(reason),
    );
    match emit_user_credential_audit(state.audit_sink.as_ref(), event).await {
        Ok(()) => error,
        Err(audit_error) => audit_error,
    }
}

async fn emit_user_credential_audit(
    sink: &dyn AuditSink,
    event: AuditEvent,
) -> Result<(), ApiError> {
    sink.emit(event).await.map_err(|_| {
        ApiError::ServiceUnavailable("Credential audit storage is unavailable.".to_string())
    })
}

fn user_credential_audit_event(
    tenant_id: Uuid,
    user_id: &str,
    action: &'static str,
    outcome: AuditOutcome,
    credential_id: Option<Uuid>,
    result_count: Option<usize>,
    reason: Option<&'static str>,
) -> AuditEvent {
    let mut event = AuditEvent::new("stored_credential", action, outcome)
        .with_tenant(tenant_id.to_string())
        .with_principal(format!("oauth_user:{user_id}"))
        .with_failure_policy(AuditFailurePolicy::FailClosed);
    if let Some(credential_id) = credential_id {
        event = event.with_resource("stored_credential", credential_id.to_string());
    }
    if let Some(result_count) = result_count {
        event = event.with_attr("result_count", result_count as i64);
    }
    if let Some(reason) = reason {
        event.reason = Some(reason.to_string());
        event = event.with_attr("reason_code", reason);
    }
    event.source = AuditSource::Api;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.severity = match outcome {
        AuditOutcome::Success => AuditSeverity::Info,
        _ => AuditSeverity::Warn,
    };
    event.sanitized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_debug_never_exposes_key_material_or_length() {
        let request = CreateUserSecretRequest {
            provider: "openai".to_string(),
            name: "personal".to_string(),
            key: "sk-never-log-this-value".to_string(),
        };
        let rendered = format!("{request:?}");
        assert!(rendered.contains("key_present: true"));
        assert!(!rendered.contains("sk-never-log-this-value"));
        assert!(!rendered.contains("key_len"));
    }

    #[test]
    fn only_user_bound_oauth_principals_can_manage_credentials() {
        let user = Auth {
            principal: AuthPrincipal::OAuthClient {
                client_id: "hosted-oidc".to_string(),
                scope: "write".to_string(),
                user_id: Some("user_123".to_string()),
            },
        };
        assert_eq!(hosted_user_id(&user).unwrap(), "user_123");

        for principal in [
            AuthPrincipal::OAuthClient {
                client_id: "client-only".to_string(),
                scope: "write".to_string(),
                user_id: None,
            },
            AuthPrincipal::ApiKey {
                key_id: Uuid::now_v7(),
                scope: "admin".to_string(),
            },
            AuthPrincipal::Anonymous,
        ] {
            assert!(hosted_user_id(&Auth { principal }).is_err());
        }
    }

    #[test]
    fn response_and_audit_contain_metadata_only() {
        let response = UserSecretResponse {
            id: Uuid::now_v7(),
            provider: "openai".to_string(),
            name: "personal".to_string(),
            masked: user_secret_mask("openai"),
            created_at: Utc::now(),
            last_used_at: None,
            revoked_at: None,
            rewrapped_at: None,
        };
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("encrypted"));
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains("key_material"));

        let event = user_credential_audit_event(
            Uuid::now_v7(),
            "user_123",
            "create",
            AuditOutcome::Success,
            Some(response.id),
            None,
            None,
        );
        let serialized = serde_json::to_string(&event).unwrap();
        assert_eq!(event.failure_policy, AuditFailurePolicy::FailClosed);
        assert!(!serialized.contains("encrypted"));
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains("api_key"));
    }

    #[test]
    fn failures_use_stable_metadata_only_reason_codes() {
        let event = user_credential_audit_event(
            Uuid::now_v7(),
            "user_123",
            "create",
            AuditOutcome::Failure,
            None,
            None,
            Some("invalid_credential"),
        );
        assert_eq!(event.reason.as_deref(), Some("invalid_credential"));
        assert_eq!(event.attrs["reason_code"], "invalid_credential");
        assert_eq!(event.failure_policy, AuditFailurePolicy::FailClosed);
    }
}
