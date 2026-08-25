//! Resumable, audited user-secret DEK rewrap lifecycle.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use matric_core::{
    AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink, AuditSource,
    AuditVisibilityClass,
};
use matric_crypto::{EncryptedBlob, KeyFailureClass, KeyProvider};
use matric_db::{PgUserSecretRepository, TenantScopedConn, UserSecretRewrapJob};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::user_secrets::user_secret_context;

const LEASE_SECONDS: i64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserSecretRewrapWorkerConfig {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub batch_size: usize,
    pub retry_delay: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserSecretRewrapStatus {
    Pending,
    Running,
    Retryable,
    Completed,
    Failed,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSecretRewrapReceipt {
    pub job_id: Uuid,
    pub status: UserSecretRewrapStatus,
    pub scanned_count: u64,
    pub rewrapped_count: u64,
    pub skipped_count: u64,
    pub attempt_count: u32,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_class: Option<String>,
}

impl fmt::Debug for UserSecretRewrapReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSecretRewrapReceipt")
            .field("job_id_present", &!self.job_id.is_nil())
            .field("status", &self.status)
            .field("scanned_count", &self.scanned_count)
            .field("rewrapped_count", &self.rewrapped_count)
            .field("skipped_count", &self.skipped_count)
            .field("attempt_count", &self.attempt_count)
            .field("completed_at", &self.completed_at)
            .field("failure_class", &self.failure_class)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum UserSecretRotationError {
    #[error("rotation persistence is unavailable")]
    PersistenceUnavailable,
    #[error("rotation audit is unavailable")]
    AuditUnavailable,
    #[error("rotation job is not claimable")]
    NotClaimable,
    #[error("stored envelope is invalid")]
    InvalidEnvelope,
    #[error("key operation failed")]
    KeyOperation {
        class: KeyFailureClass,
        retryable: bool,
    },
}

pub async fn ensure_user_secret_rewrap_job(
    pool: &PgPool,
    tenant_id: Uuid,
    job_id: Uuid,
    batch_size: usize,
) -> Result<UserSecretRewrapReceipt, UserSecretRotationError> {
    let batch_size = i32::try_from(batch_size)
        .ok()
        .filter(|size| (1..=1000).contains(size))
        .ok_or(UserSecretRotationError::PersistenceUnavailable)?;
    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    let job = PgUserSecretRepository::ensure_rewrap_job_tx(
        scope.executor(),
        tenant_id,
        job_id,
        batch_size,
    )
    .await
    .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    scope
        .commit()
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    receipt(&job)
}

pub async fn run_user_secret_rewrap_batch(
    pool: &PgPool,
    audit_sink: &dyn AuditSink,
    key_provider: &dyn KeyProvider,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<UserSecretRewrapReceipt, UserSecretRotationError> {
    let lease_id = Uuid::now_v7();
    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    let job = PgUserSecretRepository::claim_rewrap_job_tx(
        scope.executor(),
        tenant_id,
        job_id,
        lease_id,
        LEASE_SECONDS,
    )
    .await
    .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?
    .ok_or(UserSecretRotationError::NotClaimable)?;
    scope
        .commit()
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;

    emit_rotation_audit(audit_sink, tenant_id, &job, "rewrap_batch_started", None).await?;

    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    let candidates = PgUserSecretRepository::next_rewrap_batch_tx(
        scope.executor(),
        tenant_id,
        job.cursor_created_at,
        job.cursor_id,
        i64::from(job.batch_size),
    )
    .await
    .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    scope
        .rollback()
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;

    let mut rewrapped = 0i64;
    let mut skipped = 0i64;
    for candidate in &candidates {
        let envelope: EncryptedBlob = match serde_json::from_value(candidate.encrypted_blob.clone())
        {
            Ok(envelope) => envelope,
            Err(_) => {
                fail_job_reason(
                    pool,
                    audit_sink,
                    tenant_id,
                    &job,
                    lease_id,
                    "invalid_envelope",
                    false,
                )
                .await?;
                return Err(UserSecretRotationError::InvalidEnvelope);
            }
        };
        let expected = serde_json::to_value(envelope.wrapped_key())
            .map_err(|_| UserSecretRotationError::InvalidEnvelope)?;
        let context = match user_secret_context(tenant_id, &candidate.user_id, candidate.id) {
            Ok(context) => context,
            Err(_) => {
                fail_job_reason(
                    pool,
                    audit_sink,
                    tenant_id,
                    &job,
                    lease_id,
                    "invalid_context",
                    false,
                )
                .await?;
                return Err(UserSecretRotationError::InvalidEnvelope);
            }
        };
        let next = match key_provider
            .rewrap_dek(envelope.wrapped_key(), &context)
            .await
        {
            Ok(next) => next,
            Err(error) => {
                let rotation_error = UserSecretRotationError::KeyOperation {
                    class: error.class(),
                    retryable: error.is_retryable(),
                };
                fail_job(pool, audit_sink, tenant_id, &job, lease_id, rotation_error).await?;
                return Err(rotation_error);
            }
        };
        let next =
            serde_json::to_value(next).map_err(|_| UserSecretRotationError::InvalidEnvelope)?;
        let mut scope = TenantScopedConn::begin(pool, tenant_id)
            .await
            .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
        let updated = PgUserSecretRepository::compare_and_swap_wrapped_key_tx(
            scope.executor(),
            tenant_id,
            &candidate.user_id,
            candidate.id,
            expected,
            next,
        )
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
        scope
            .commit()
            .await
            .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
        if updated {
            rewrapped += 1;
        } else {
            skipped += 1;
        }
    }

    let completed = candidates.len() < usize::try_from(job.batch_size).unwrap_or_default();
    let cursor = candidates.last();
    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    let checkpointed = PgUserSecretRepository::checkpoint_rewrap_job_tx(
        scope.executor(),
        tenant_id,
        job_id,
        lease_id,
        cursor.map(|candidate| candidate.created_at),
        cursor.map(|candidate| candidate.id),
        i64::try_from(candidates.len()).unwrap_or(i64::MAX),
        rewrapped,
        skipped,
        completed,
    )
    .await
    .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    if !checkpointed {
        return Err(UserSecretRotationError::NotClaimable);
    }
    let updated = PgUserSecretRepository::get_rewrap_job_tx(scope.executor(), tenant_id, job_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?
        .ok_or(UserSecretRotationError::PersistenceUnavailable)?;
    scope
        .commit()
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    emit_rotation_audit(
        audit_sink,
        tenant_id,
        &updated,
        if completed {
            "rewrap_completed"
        } else {
            "rewrap_checkpointed"
        },
        None,
    )
    .await?;
    receipt(&updated)
}

pub async fn run_user_secret_rewrap_worker(
    pool: PgPool,
    audit_sink: Arc<dyn AuditSink>,
    key_provider: Arc<dyn KeyProvider>,
    config: UserSecretRewrapWorkerConfig,
) -> Result<UserSecretRewrapReceipt, UserSecretRotationError> {
    let mut receipt =
        ensure_user_secret_rewrap_job(&pool, config.tenant_id, config.job_id, config.batch_size)
            .await?;
    loop {
        match receipt.status {
            UserSecretRewrapStatus::Completed => return Ok(receipt),
            UserSecretRewrapStatus::Failed => {
                return Err(UserSecretRotationError::PersistenceUnavailable)
            }
            UserSecretRewrapStatus::Pending
            | UserSecretRewrapStatus::Running
            | UserSecretRewrapStatus::Retryable => {}
        }
        match run_user_secret_rewrap_batch(
            &pool,
            audit_sink.as_ref(),
            key_provider.as_ref(),
            config.tenant_id,
            config.job_id,
        )
        .await
        {
            Ok(next) => receipt = next,
            Err(UserSecretRotationError::NotClaimable)
            | Err(UserSecretRotationError::AuditUnavailable)
            | Err(UserSecretRotationError::KeyOperation {
                retryable: true, ..
            }) => tokio::time::sleep(config.retry_delay).await,
            Err(error) => return Err(error),
        }
    }
}

async fn fail_job(
    pool: &PgPool,
    audit_sink: &dyn AuditSink,
    tenant_id: Uuid,
    job: &UserSecretRewrapJob,
    lease_id: Uuid,
    error: UserSecretRotationError,
) -> Result<(), UserSecretRotationError> {
    let UserSecretRotationError::KeyOperation { class, retryable } = error else {
        return Err(error);
    };
    let reason = key_failure_reason(class);
    fail_job_reason(
        pool, audit_sink, tenant_id, job, lease_id, reason, retryable,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn fail_job_reason(
    pool: &PgPool,
    audit_sink: &dyn AuditSink,
    tenant_id: Uuid,
    job: &UserSecretRewrapJob,
    lease_id: Uuid,
    reason: &'static str,
    retryable: bool,
) -> Result<(), UserSecretRotationError> {
    let mut scope = TenantScopedConn::begin(pool, tenant_id)
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    PgUserSecretRepository::fail_rewrap_job_tx(
        scope.executor(),
        tenant_id,
        job.id,
        lease_id,
        reason,
        retryable,
    )
    .await
    .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    scope
        .commit()
        .await
        .map_err(|_| UserSecretRotationError::PersistenceUnavailable)?;
    emit_rotation_audit(audit_sink, tenant_id, job, "rewrap_failed", Some(reason)).await
}

fn receipt(job: &UserSecretRewrapJob) -> Result<UserSecretRewrapReceipt, UserSecretRotationError> {
    Ok(UserSecretRewrapReceipt {
        job_id: job.id,
        status: match job.status.as_str() {
            "pending" => UserSecretRewrapStatus::Pending,
            "running" => UserSecretRewrapStatus::Running,
            "retryable" => UserSecretRewrapStatus::Retryable,
            "completed" => UserSecretRewrapStatus::Completed,
            "failed" => UserSecretRewrapStatus::Failed,
            _ => return Err(UserSecretRotationError::PersistenceUnavailable),
        },
        scanned_count: u64::try_from(job.scanned_count).unwrap_or_default(),
        rewrapped_count: u64::try_from(job.rewrapped_count).unwrap_or_default(),
        skipped_count: u64::try_from(job.skipped_count).unwrap_or_default(),
        attempt_count: u32::try_from(job.attempt_count).unwrap_or_default(),
        completed_at: job.completed_at,
        failure_class: job.last_failure_class.clone(),
    })
}

async fn emit_rotation_audit(
    sink: &dyn AuditSink,
    tenant_id: Uuid,
    job: &UserSecretRewrapJob,
    action: &'static str,
    reason: Option<&'static str>,
) -> Result<(), UserSecretRotationError> {
    let outcome = if reason.is_some() {
        AuditOutcome::Failure
    } else {
        AuditOutcome::Success
    };
    let mut event = AuditEvent::new("key_lifecycle", action, outcome)
        .with_tenant(tenant_id.to_string())
        .with_resource("user_secret_rewrap_job", job.id.to_string())
        .with_attr("scanned_count", job.scanned_count)
        .with_attr("rewrapped_count", job.rewrapped_count)
        .with_attr("skipped_count", job.skipped_count)
        .with_attr("attempt_count", i64::from(job.attempt_count))
        .with_failure_policy(AuditFailurePolicy::FailClosed);
    if let Some(reason) = reason {
        event.reason = Some(reason.to_string());
        event = event.with_attr("reason_code", reason);
    }
    event.source = AuditSource::Worker;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.severity = if reason.is_some() {
        AuditSeverity::Warn
    } else {
        AuditSeverity::Info
    };
    sink.emit(event.sanitized())
        .await
        .map_err(|_| UserSecretRotationError::AuditUnavailable)
}

fn key_failure_reason(class: KeyFailureClass) -> &'static str {
    match class {
        KeyFailureClass::ProviderUnavailable => "provider_unavailable",
        KeyFailureClass::AccessDenied => "access_denied",
        KeyFailureClass::KeyDisabled => "key_disabled",
        KeyFailureClass::KeyVersionUnavailable => "key_version_unavailable",
        KeyFailureClass::ContextMismatch => "context_mismatch",
        KeyFailureClass::InvalidCiphertext => "invalid_ciphertext",
        KeyFailureClass::Throttled => "throttled",
        KeyFailureClass::InvalidConfiguration
        | KeyFailureClass::InvalidContext
        | KeyFailureClass::UnsupportedVersion
        | KeyFailureClass::UnsupportedOperation
        | KeyFailureClass::ProviderFailure => "provider_failure",
    }
}
