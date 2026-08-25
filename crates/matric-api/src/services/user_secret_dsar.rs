//! Confirmed DSAR hard-deletion boundary for stored provider credentials.

use std::fmt;

use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass,
};
use matric_db::{PgUserSecretRepository, PostgresAuditSink, TenantScopedConn};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSecretErasureReceipt {
    pub request_id: Uuid,
    pub deleted_secret_rows: u64,
    pub local_secret_outcome: String,
    pub rotation_history_outcome: String,
    pub audit_history_outcome: String,
    pub provider_account_outcome: String,
}

impl fmt::Debug for UserSecretErasureReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSecretErasureReceipt")
            .field("request_id_present", &!self.request_id.is_nil())
            .field("deleted_secret_rows", &self.deleted_secret_rows)
            .field("local_secret_outcome", &self.local_secret_outcome)
            .field("rotation_history_outcome", &self.rotation_history_outcome)
            .field("audit_history_outcome", &self.audit_history_outcome)
            .field("provider_account_outcome", &self.provider_account_outcome)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum UserSecretErasureError {
    #[error("erasure audit is unavailable")]
    AuditUnavailable,
    #[error("erasure persistence is unavailable")]
    PersistenceUnavailable,
    #[error("erasure request context is invalid")]
    InvalidContext,
}

pub async fn erase_user_secrets_for_confirmed_dsar(
    pool: &PgPool,
    audit_sink: &PostgresAuditSink,
    tenant_id: Uuid,
    user_id: &str,
    request_id: Uuid,
) -> Result<UserSecretErasureReceipt, UserSecretErasureError> {
    if tenant_id.is_nil() || request_id.is_nil() || user_id.is_empty() {
        return Err(UserSecretErasureError::InvalidContext);
    }
    emit_erasure_audit(
        audit_sink,
        tenant_id,
        user_id,
        request_id,
        "dsar_secret_erasure_started",
        None,
    )
    .await?;
    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretErasureError::PersistenceUnavailable)?;
    let deleted =
        PgUserSecretRepository::hard_delete_subject_tx(scope.executor(), tenant_id, user_id)
            .await
            .map_err(|_| UserSecretErasureError::PersistenceUnavailable)?;
    let completed_event = erasure_audit_event(
        tenant_id,
        user_id,
        request_id,
        "dsar_secret_erasure_completed",
        Some(deleted),
    );
    audit_sink
        .emit_tx(scope.executor(), completed_event)
        .await
        .map_err(|_| UserSecretErasureError::AuditUnavailable)?;
    scope
        .commit()
        .await
        .map_err(|_| UserSecretErasureError::PersistenceUnavailable)?;
    Ok(UserSecretErasureReceipt {
        request_id,
        deleted_secret_rows: deleted,
        local_secret_outcome: "encrypted_secret_deleted".to_string(),
        rotation_history_outcome: "retained_aggregate_security_basis".to_string(),
        audit_history_outcome: "retained_compliance_security_basis".to_string(),
        provider_account_outcome: "provider_account_action_required".to_string(),
    })
}

async fn emit_erasure_audit(
    sink: &dyn AuditSink,
    tenant_id: Uuid,
    user_id: &str,
    request_id: Uuid,
    action: &'static str,
    deleted: Option<u64>,
) -> Result<(), UserSecretErasureError> {
    let event = erasure_audit_event(tenant_id, user_id, request_id, action, deleted);
    sink.emit(event)
        .await
        .map_err(|_| UserSecretErasureError::AuditUnavailable)
}

fn erasure_audit_event(
    tenant_id: Uuid,
    user_id: &str,
    request_id: Uuid,
    action: &'static str,
    deleted: Option<u64>,
) -> AuditEvent {
    let mut event = AuditEvent::new("privacy", action, AuditOutcome::Success)
        .with_tenant(tenant_id.to_string())
        .with_principal(format!("oauth_user:{user_id}"))
        .with_resource("dsar_request", request_id.to_string())
        .with_failure_policy(AuditFailurePolicy::FailClosed);
    if let Some(deleted) = deleted {
        event = event
            .with_attr("deleted_secret_rows", deleted)
            .with_attr("local_secret_outcome", "encrypted_secret_deleted")
            .with_attr(
                "rotation_history_outcome",
                "retained_aggregate_security_basis",
            )
            .with_attr(
                "provider_account_outcome",
                "provider_account_action_required",
            );
    }
    event.source = AuditSource::Worker;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.severity = AuditSeverity::Info;
    event.sanitized()
}
