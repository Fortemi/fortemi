//! Committed tenant claims and short, attempt-fenced settlement transactions.
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use matric_core::{
    Error, Job, JobFailureClass, JobRetryOutcome, JobType, Result, ServerEvent, TierGroup,
};
use matric_db::{
    assert_hosted_runtime_role, PgNoteRepository, ScopedClaimedJob, ScopedJobRepository,
    TenantScopedConn,
};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::registry::TenantCursor;

#[derive(Clone, Copy, Debug)]
pub struct HostedDispatchLimits {
    pub tenants_per_pass: u32,
    pub statement_timeout: Duration,
    pub pass_timeout: Duration,
}

impl Default for HostedDispatchLimits {
    fn default() -> Self {
        Self {
            tenants_per_pass: 32,
            statement_timeout: Duration::from_secs(2),
            pass_timeout: Duration::from_secs(10),
        }
    }
}

impl HostedDispatchLimits {
    fn validate(self) -> Result<()> {
        if !(1..=64).contains(&self.tenants_per_pass)
            || self.statement_timeout < Duration::from_millis(1)
            || self.statement_timeout > Duration::from_secs(5)
            || self.pass_timeout < self.statement_timeout
            || self.pass_timeout > Duration::from_secs(30)
        {
            return Err(Error::Config(
                "hosted dispatch limits exceed permitted bounds".into(),
            ));
        }
        Ok(())
    }
}

/// A page may have failed tenants even when it returned an independent valid claim.
/// Empty/wrapped means no claim in this page, not a global pending count of zero.
#[derive(Default)]
pub struct HostedDispatchReport {
    pub claim: Option<Arc<HostedClaim>>,
    pub tenants_visited: u32,
    pub failed_tenants: u32,
    pub wrapped: bool,
    pub timed_out: bool,
}

/// Construct in service bootstrap with a non-bypass runtime pool. Callers supply
/// registered, hosted-capable handler types, never user-controlled tenant IDs.
pub struct HostedJobDispatcher {
    pool: PgPool,
    limits: HostedDispatchLimits,
    cursors: [Mutex<TenantCursor>; 6],
}

fn tier_index(tier: TierGroup) -> usize {
    match tier {
        TierGroup::CpuAndAgnostic => 0,
        TierGroup::FastGpu => 1,
        TierGroup::StandardGpu => 2,
        TierGroup::VisionGpu => 3,
        TierGroup::RenderGpu => 4,
        TierGroup::AudioGpu => 5,
    }
}

async fn bound_scope(scope: &mut TenantScopedConn<'_>, duration: Duration) -> Result<()> {
    sqlx::query(
        "SELECT set_config('statement_timeout',$1,true), set_config('lock_timeout','100ms',true)",
    )
    .bind(format!("{}ms", duration.as_millis()))
    .execute(scope.executor())
    .await
    .map_err(Error::Database)?;
    Ok(())
}

impl HostedJobDispatcher {
    pub async fn new(pool: PgPool, limits: HostedDispatchLimits) -> Result<Self> {
        limits.validate()?;
        assert_hosted_runtime_role(&pool).await?;
        Ok(Self {
            pool,
            limits,
            cursors: std::array::from_fn(|_| Mutex::new(TenantCursor::default())),
        })
    }

    /// At most one claim, committed before return, across one bounded registry page.
    /// The tenant cursor advances on success, empty selection and tenant failure.
    /// Same-tier overlap is rejected; different cost tiers retain separate cursors.
    pub async fn claim_next(
        &self,
        tier: TierGroup,
        handler_types: &[JobType],
        excluded_archives: &[String],
    ) -> Result<HostedDispatchReport> {
        // The personal repository interprets [] as ALL; that is never safe here.
        if handler_types.is_empty()
            || handler_types.len() > JobType::ALL.len()
            || excluded_archives.len() > 256
            || excluded_archives.iter().any(|s| s.len() > 63)
        {
            return Err(Error::InvalidInput(
                "hosted dispatch requires bounded explicit handler and archive lists".into(),
            ));
        }
        let mut cursor = self.cursors[tier_index(tier)].try_lock().map_err(|_| {
            Error::InvalidInput("hosted dispatch tier already has an active pass".into())
        })?;
        let mut report = HostedDispatchReport::default();
        let work = async {
            let tenants = cursor
                .page(
                    &self.pool,
                    self.limits.tenants_per_pass,
                    self.limits.statement_timeout,
                )
                .await?;
            if tenants.is_empty() {
                *cursor = TenantCursor::default();
                report.wrapped = true;
            }
            for tenant in tenants {
                cursor.after = tenant;
                report.tenants_visited += 1;
                let selected = async {
                    let mut scope = TenantScopedConn::begin(&self.pool, tenant).await?;
                    bound_scope(&mut scope, self.limits.statement_timeout).await?;
                    let claim = ScopedJobRepository::new(&mut scope)
                        .claim_for_tier(tier, handler_types, excluded_archives)
                        .await?;
                    scope.commit().await?;
                    Ok::<_, Error>(claim)
                }
                .await;
                match selected {
                    Ok(Some(claim)) => {
                        report.claim = Some(Arc::new(HostedClaim {
                            claim,
                            pool: self.pool.clone(),
                            transaction_timeout: self.limits.statement_timeout,
                        }));
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => report.failed_tenants += 1,
                }
            }
            Ok::<_, Error>(())
        };
        match tokio::time::timeout(self.limits.pass_timeout, work).await {
            Ok(result) => result?,
            Err(_) => report.timed_out = true,
        }
        Ok(report)
    }
}

/// Immutable, non-serialized identity minted only after the claim commits.
/// No raw pool, mutable job or caller-selected tenant is exposed. Cloned Arcs
/// preserve the original attempt UUID; late callbacks cannot settle a new attempt.
pub struct HostedClaim {
    claim: ScopedClaimedJob,
    pool: PgPool,
    transaction_timeout: Duration,
}

enum Change<'a> {
    Progress(i32, Option<&'a str>),
    Complete(Option<Value>),
    Fail(&'a str, JobFailureClass, &'a str),
    Retry(&'a str, JobFailureClass, &'a str, DateTime<Utc>),
}

enum Changed {
    Applied(bool),
    Retry(Option<JobRetryOutcome>),
}

impl HostedClaim {
    /// Short database-only work under the committed attempt and its content schema.
    /// Keep inference/network/external side effects outside this closure. A lost
    /// claim returns None; errors, timeouts and cancellation roll back via SQLx.
    /// Cancellation during commit remains indeterminate, not exactly-once evidence.
    pub async fn with_content<F, T>(&self, work: F) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnOnce(
                &'a mut TenantScopedConn<'_>,
            ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>
            + Send,
    {
        tokio::time::timeout(self.transaction_timeout, async {
            let mut scope = TenantScopedConn::begin(&self.pool, self.tenant_id()).await?;
            bound_scope(&mut scope, self.transaction_timeout).await?;
            if !ScopedJobRepository::new(&mut scope)
                .prepare_content(&self.claim)
                .await?
            {
                scope.rollback().await?;
                return Ok(None);
            }
            let expected_path = format!("pg_catalog,\"{}\",pg_temp", self.archive_schema());
            let value = work(&mut scope).await?;
            let path: String = sqlx::query_scalar("SELECT current_setting('search_path')")
                .fetch_one(scope.executor())
                .await
                .map_err(Error::Database)?;
            if path != expected_path {
                return Err(Error::InvalidInput(
                    "hosted content search path changed during transaction".into(),
                ));
            }
            if !ScopedJobRepository::new(&mut scope)
                .lock_claim(&self.claim)
                .await?
            {
                scope.rollback().await?;
                return Ok(None);
            }
            scope.commit().await?;
            Ok(Some(value))
        })
        .await
        .map_err(|_| Error::DeadlineExceeded)?
    }

    pub fn job(&self) -> &Job {
        self.claim.job()
    }
    pub fn tenant_id(&self) -> Uuid {
        self.claim.tenant_id()
    }
    pub fn archive_schema(&self) -> &str {
        self.claim.archive_schema()
    }
    pub fn attempt_id(&self) -> Uuid {
        self.claim.attempt_id()
    }
    pub fn attempt_number(&self) -> i32 {
        self.claim.attempt_number()
    }

    async fn change(&self, change: Change<'_>) -> Result<Changed> {
        tokio::time::timeout(self.transaction_timeout, async {
            let mut scope = TenantScopedConn::begin(&self.pool, self.tenant_id()).await?;
            bound_scope(&mut scope, self.transaction_timeout).await?;
            let mut queue = ScopedJobRepository::new(&mut scope);
            let changed = match change {
                Change::Progress(percent, message) => {
                    Changed::Applied(queue.update_progress(&self.claim, percent, message).await?)
                }
                Change::Complete(result) => {
                    Changed::Applied(queue.complete(&self.claim, result).await?)
                }
                Change::Fail(error, class, code) => {
                    Changed::Applied(queue.fail(&self.claim, error, class, code).await?)
                }
                Change::Retry(error, class, code, at) => {
                    Changed::Retry(queue.retry(&self.claim, error, class, code, at).await?)
                }
            };
            scope.commit().await?;
            Ok(changed)
        })
        .await
        .map_err(|_| Error::DeadlineExceeded)?
    }

    pub async fn update_progress(&self, percent: i32, message: Option<&str>) -> Result<bool> {
        let Changed::Applied(applied) = self.change(Change::Progress(percent, message)).await?
        else {
            unreachable!()
        };
        Ok(applied)
    }

    pub async fn complete(&self, result: Option<Value>) -> Result<bool> {
        let Changed::Applied(applied) = self.change(Change::Complete(result)).await? else {
            unreachable!()
        };
        Ok(applied)
    }

    /// Outer None is a lost attempt; inner None is no visible active note.
    /// Owns the queue -> admitted content -> queue transition, with the claim
    /// lock held throughout. A snapshot is returned only after commit succeeds.
    pub(super) async fn complete_with_note_event(
        &self,
        result: Option<Value>,
    ) -> Result<Option<Option<ServerEvent>>> {
        tokio::time::timeout(self.transaction_timeout, async {
            let mut scope = TenantScopedConn::begin(&self.pool, self.tenant_id()).await?;
            bound_scope(&mut scope, self.transaction_timeout).await?;
            let event = if let Some(note_id) = self.job().note_id {
                if !ScopedJobRepository::new(&mut scope)
                    .prepare_content(&self.claim)
                    .await?
                {
                    scope.rollback().await?;
                    return Ok(None);
                }
                PgNoteRepository::updated_event_scoped(&mut scope, note_id).await?
            } else {
                None
            };
            if !ScopedJobRepository::new(&mut scope)
                .complete(&self.claim, result)
                .await?
            {
                scope.rollback().await?;
                return Ok(None);
            }
            scope.commit().await?;
            Ok(Some(event))
        })
        .await
        .map_err(|_| Error::DeadlineExceeded)?
    }

    pub async fn fail(&self, error: &str, class: JobFailureClass, code: &str) -> Result<bool> {
        let Changed::Applied(applied) = self.change(Change::Fail(error, class, code)).await? else {
            unreachable!()
        };
        Ok(applied)
    }

    pub async fn retry(
        &self,
        error: &str,
        class: JobFailureClass,
        code: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<JobRetryOutcome>> {
        let Changed::Retry(outcome) = self.change(Change::Retry(error, class, code, at)).await?
        else {
            unreachable!()
        };
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_limits_and_tier_cursors_are_bounded() {
        let default = HostedDispatchLimits::default();
        assert!(default.validate().is_ok());
        for limits in [
            HostedDispatchLimits {
                tenants_per_pass: 0,
                ..default
            },
            HostedDispatchLimits {
                tenants_per_pass: 65,
                ..default
            },
            HostedDispatchLimits {
                statement_timeout: Duration::ZERO,
                ..default
            },
            HostedDispatchLimits {
                statement_timeout: Duration::from_secs(6),
                ..default
            },
            HostedDispatchLimits {
                pass_timeout: Duration::from_secs(1),
                ..default
            },
            HostedDispatchLimits {
                pass_timeout: Duration::from_secs(31),
                ..default
            },
        ] {
            assert!(limits.validate().is_err());
        }
        let mut indices = [
            TierGroup::CpuAndAgnostic,
            TierGroup::AudioGpu,
            TierGroup::FastGpu,
            TierGroup::StandardGpu,
            TierGroup::RenderGpu,
            TierGroup::VisionGpu,
        ]
        .map(tier_index);
        indices.sort();
        assert_eq!(indices, [0, 1, 2, 3, 4, 5]);
    }
}
