//! Service-owned registry selection followed by short tenant-scoped recovery.
use std::time::Duration;

use matric_core::{Error, JobRetryPolicy, Result};
use matric_db::{assert_hosted_runtime_role, ScopedJobRepository, TenantScopedConn};
use sqlx::PgPool;
use tokio::sync::Mutex;

use super::registry::TenantCursor;

#[derive(Clone, Copy, Debug)]
pub struct HostedRecoveryLimits {
    pub tenants_per_pass: u32,
    pub jobs_per_tenant: u32,
    pub statement_timeout: Duration,
    pub pass_timeout: Duration,
}

impl Default for HostedRecoveryLimits {
    fn default() -> Self {
        Self {
            tenants_per_pass: 32,
            jobs_per_tenant: 128,
            statement_timeout: Duration::from_secs(2),
            pass_timeout: Duration::from_secs(10),
        }
    }
}

impl HostedRecoveryLimits {
    fn validate(self) -> Result<()> {
        if !(1..=64).contains(&self.tenants_per_pass)
            || !(1..=256).contains(&self.jobs_per_tenant)
            || self.statement_timeout < Duration::from_millis(1)
            || self.statement_timeout > Duration::from_secs(5)
            || self.pass_timeout < self.statement_timeout
            || self.pass_timeout > Duration::from_secs(30)
        {
            return Err(Error::Config(
                "hosted recovery limits exceed permitted bounds".into(),
            ));
        }
        Ok(())
    }
}

/// Aggregate operational evidence only; no raw tenant/job identifiers or payloads.
#[derive(Default, Debug)]
pub struct HostedRecoveryReport {
    pub tenants_visited: u32,
    pub reaped_count: i64,
    pub failed_tenants: u32,
    pub wrapped: bool,
    pub timed_out: bool,
}

/// Construct only in service bootstrap, never from a user request or job payload.
/// Registry reads use the runtime role's existing system-table SELECT authority.
/// Tenant data is accessed exclusively via TenantScopedConn and forced RLS.
pub struct HostedJobRecovery {
    pool: PgPool,
    limits: HostedRecoveryLimits,
    cursor: Mutex<TenantCursor>,
}

impl HostedJobRecovery {
    pub async fn new(pool: PgPool, limits: HostedRecoveryLimits) -> Result<Self> {
        limits.validate()?;
        assert_hosted_runtime_role(&pool).await?;
        Ok(Self {
            pool,
            limits,
            cursor: Mutex::new(TenantCursor::default()),
        })
    }

    /// One bounded page per pass; subsequent passes continue after the last tenant.
    /// An error advances past that tenant so one failing tenant cannot starve others.
    /// A traversal's high-water mark prevents new tenants from delaying wraparound.
    /// Empty pages reset that mark for the following pass, admitting new tenants.
    pub async fn sweep(
        &self,
        timeout_secs: u64,
        policy: &JobRetryPolicy,
    ) -> Result<HostedRecoveryReport> {
        if timeout_secs == 0 || timeout_secs > i32::MAX as u64 {
            return Err(Error::InvalidInput(
                "hosted recovery threshold is invalid".into(),
            ));
        }
        let mut cursor = self
            .cursor
            .try_lock()
            .map_err(|_| Error::InvalidInput("hosted recovery pass is already running".into()))?;
        let mut report = HostedRecoveryReport::default();
        let work = async {
            // Registry enumeration is control-plane metadata, not an RLS bypass.
            // The reserved nil tenant belongs to personal mode and is never selected.
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
                let recovered = async {
                    let mut scope = TenantScopedConn::begin(&self.pool, tenant).await?;
                    sqlx::query("SELECT set_config('statement_timeout',$1,true), set_config('lock_timeout','100ms',true)")
                        .bind(format!("{}ms", self.limits.statement_timeout.as_millis()))
                        .execute(scope.executor()).await.map_err(Error::Database)?;
                    let count = ScopedJobRepository::new(&mut scope)
                        .reap_stale_batch(timeout_secs, policy, self.limits.jobs_per_tenant).await?;
                    scope.commit().await?;
                    Ok::<_, Error>(count)
                }.await;
                match recovered {
                    Ok(count) => report.reaped_count += count,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_recovery_limits_are_bounded() {
        let defaults = HostedRecoveryLimits::default();
        assert!(defaults.validate().is_ok());
        for invalid in [
            HostedRecoveryLimits {
                tenants_per_pass: 0,
                ..defaults
            },
            HostedRecoveryLimits {
                tenants_per_pass: 65,
                ..defaults
            },
            HostedRecoveryLimits {
                jobs_per_tenant: 0,
                ..defaults
            },
            HostedRecoveryLimits {
                jobs_per_tenant: 257,
                ..defaults
            },
            HostedRecoveryLimits {
                statement_timeout: Duration::ZERO,
                ..defaults
            },
            HostedRecoveryLimits {
                statement_timeout: Duration::from_secs(6),
                ..defaults
            },
            HostedRecoveryLimits {
                pass_timeout: Duration::from_secs(31),
                ..defaults
            },
            HostedRecoveryLimits {
                pass_timeout: Duration::from_millis(1),
                ..defaults
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}
