use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use matric_core::{Error, JobType, Result};
use matric_db::TenantScopedConn;
use serde_json::Value;
use uuid::Uuid;

use super::JobResult;
use crate::worker::HostedClaim;

/// Only typed transport/deadline errors and reviewed SQLSTATEs are retryable.
/// Authentication, configuration, constraints and unknown errors fail closed.
/// PostgreSQL condition names: https://www.postgresql.org/docs/18/errcodes-appendix.html
pub fn hosted_database_failure(error: &Error) -> JobResult {
    match error {
        Error::DeadlineExceeded | Error::Database(sqlx::Error::PoolTimedOut) => {
            JobResult::Retry("database_timeout".into())
        }
        Error::Database(sqlx::Error::Io(_)) => {
            JobResult::Retry("database_connection_failed".into())
        }
        Error::Database(sqlx::Error::Database(error)) => sqlstate_failure(error.code().as_deref()),
        _ => JobResult::Failed("hosted_database_operation_rejected".into()),
    }
}

fn sqlstate_failure(code: Option<&str>) -> JobResult {
    match code {
        Some("57014" | "25P03" | "25P04" | "55P03") => JobResult::Retry("database_timeout".into()),
        Some(
            "40001" | "40P01" | "40003" | "08000" | "08001" | "08003" | "08006" | "08007" | "53300"
            | "57P01" | "57P02" | "57P03",
        ) => JobResult::Retry("database_transient".into()),
        _ => JobResult::Failed("hosted_database_operation_rejected".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_database_failure_uses_typed_codes_not_messages() {
        for code in [
            "57014", "25P03", "25P04", "55P03", "40001", "40P01", "40003", "08000", "08001",
            "08003", "08006", "08007", "53300", "57P01", "57P02", "57P03",
        ] {
            assert!(
                matches!(sqlstate_failure(Some(code)), JobResult::Retry(_)),
                "{code}"
            );
        }
        for code in [
            None,
            Some("23514"),
            Some("23505"),
            Some("42501"),
            Some("28P01"),
            Some("08004"),
            Some("08P01"),
            Some("XX000"),
            Some("timeout"),
        ] {
            assert!(matches!(sqlstate_failure(code), JobResult::Failed(_)));
        }
        for e in [
            Error::DeadlineExceeded,
            Error::Database(sqlx::Error::PoolTimedOut),
            Error::Database(sqlx::Error::Io(std::io::Error::from(
                std::io::ErrorKind::ConnectionReset,
            ))),
        ] {
            assert!(matches!(hosted_database_failure(&e), JobResult::Retry(_)));
        }
        for e in [
            Error::InvalidInput("timeout".into()),
            Error::Forbidden("connection timeout".into()),
            Error::Config("timeout".into()),
            Error::Database(sqlx::Error::PoolClosed),
            Error::Database(sqlx::Error::RowNotFound),
        ] {
            assert!(matches!(hosted_database_failure(&e), JobResult::Failed(_)));
        }
    }
}

/// A hosted handler can receive only a committed capability, never an arbitrary
/// Job plus ambient database or global event sender. Worker retains settlement.
pub struct HostedJobContext {
    claim: Arc<HostedClaim>,
    progress: Option<Arc<dyn Fn(i32, Option<&str>) + Send + Sync>>,
}

impl HostedJobContext {
    pub fn from_claim(claim: Arc<HostedClaim>) -> Self {
        Self {
            claim,
            progress: None,
        }
    }
    pub(crate) fn with_progress_callback(
        mut self,
        callback: impl Fn(i32, Option<&str>) + Send + Sync + 'static,
    ) -> Self {
        self.progress = Some(Arc::new(callback));
        self
    }
    pub fn note_id(&self) -> Option<Uuid> {
        self.claim.job().note_id
    }
    pub fn job_id(&self) -> Uuid {
        self.claim.job().id
    }
    pub fn payload(&self) -> Option<&Value> {
        self.claim.job().payload.as_ref()
    }
    pub fn tenant_id(&self) -> Uuid {
        self.claim.tenant_id()
    }
    pub fn archive_schema(&self) -> &str {
        self.claim.archive_schema()
    }

    /// Emit progress only after its attempt-fenced transaction commits.
    pub async fn report_progress(&self, percent: i32, message: Option<&str>) -> Result<bool> {
        let applied = self.claim.update_progress(percent, message).await?;
        if applied {
            if let Some(callback) = &self.progress {
                callback(percent, message);
            }
        }
        Ok(applied)
    }

    /// Perform short database work in the claimed tenant/archive, never inference.
    pub async fn with_content<F, T>(&self, work: F) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnOnce(
                &'a mut TenantScopedConn<'_>,
            ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send,
    {
        self.claim.with_content(work).await
    }
}

/// Explicit hosted handler contract. There is deliberately no blanket adapter
/// from JobHandler: each implementation must migrate its content/follow-up work.
#[async_trait]
pub trait HostedJobHandler: Send + Sync {
    fn job_type(&self) -> JobType;
    async fn execute(&self, ctx: HostedJobContext) -> JobResult;
}
