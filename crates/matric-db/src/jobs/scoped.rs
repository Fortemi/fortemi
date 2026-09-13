//! Hosted queue operations borrow a validated transaction, never an ambient pool.
use super::{reap_stale_on, PgJobRepository};
use crate::TenantScopedConn;
use chrono::{DateTime, Utc};
use matric_core::{
    Error, Job, JobFailureClass, JobRetryOutcome, JobRetryPolicy, JobType, Result, TierGroup,
};
use serde_json::Value;
use uuid::Uuid;

/// A claim whose routing came from tenant-visible storage, not payload authority.
pub struct ScopedClaimedJob {
    job: Job,
    tenant_id: Uuid,
    archive_schema: String,
    attempt_id: Uuid,
}

impl ScopedClaimedJob {
    pub fn job(&self) -> &Job {
        &self.job
    }
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    pub fn archive_schema(&self) -> &str {
        &self.archive_schema
    }
    pub fn attempt_number(&self) -> i32 {
        self.job.retry_count + 1
    }
    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }
}

// Both row locks are acquired before mutation. UUID fencing also rejects a claim
// that was rolled back and replaced without incrementing the attempt number.
const ACTIVE_CLAIM: &str = "SELECT q.id, q.retry_count, q.max_retries, a.id AS attempt_id
    FROM public.job_queue q JOIN public.job_attempt a ON a.job_id=q.id
    WHERE q.tenant_id=$1 AND q.id=$2 AND a.id=$3 AND a.attempt_number=$4
      AND q.retry_count=$4-1 AND q.status='running'::public.job_status
      AND a.outcome='running' AND COALESCE(q.payload->>'schema','public')=$5
      AND EXISTS(SELECT 1 FROM public.tenant_registry WHERE id=$1 AND status='active')
      AND ($5='public' OR EXISTS(SELECT 1 FROM public.archive_registry
                                WHERE tenant_id=$1 AND schema_name=$5))
    FOR UPDATE OF q,a";

enum Settlement<'a> {
    Complete(Option<Value>),
    Failure {
        error: &'a str,
        class: JobFailureClass,
        code: &'a str,
        retry_at: Option<DateTime<Utc>>,
    },
}

/// Queue work shares the caller's tenant transaction and commit/rollback boundary.
/// The caller must establish trusted worker tenant selection before opening scope.
/// Use a dedicated short queue transaction: this facade sets its local search path
/// to pg_catalog,public,pg_temp. Do not reuse it as an archive content transaction.
pub struct ScopedJobRepository<'scope, 'pool> {
    scope: &'scope mut TenantScopedConn<'pool>,
}

impl<'scope, 'pool> ScopedJobRepository<'scope, 'pool> {
    pub fn new(scope: &'scope mut TenantScopedConn<'pool>) -> Self {
        Self { scope }
    }

    async fn admit(&mut self) -> Result<Uuid> {
        let tenant = self.scope.tenant_id();
        // Explicitly put pg_temp last: omitting it would give it implicit priority.
        sqlx::query("SELECT set_config('search_path','pg_catalog,public,pg_temp',true)")
            .execute(self.scope.executor())
            .await
            .map_err(Error::Database)?;
        let active: bool = sqlx::query_scalar(
            "SELECT current_setting('app.current_tenant',true)=$1::uuid::text
                AND EXISTS(SELECT 1 FROM public.tenant_registry WHERE id=$1 AND status='active')",
        )
        .bind(tenant)
        .fetch_one(self.scope.executor())
        .await
        .map_err(Error::Database)?;
        if !active {
            return Err(Error::InvalidInput(
                "hosted worker tenant is inactive or transaction scope changed".into(),
            ));
        }
        Ok(tenant)
    }

    /// Claims and writes attempt evidence atomically in the caller's transaction.
    /// Invalid or foreign payload routing is ineligible and never selects a tenant.
    pub async fn claim_for_tier(
        &mut self,
        tier: TierGroup,
        job_types: &[JobType],
        excluded_archives: &[String],
    ) -> Result<Option<ScopedClaimedJob>> {
        let tenant_id = self.admit().await?;
        let job = PgJobRepository::claim_for_tier_on(
            self.scope.executor(),
            tier,
            job_types,
            excluded_archives,
            Some(tenant_id),
        )
        .await?;
        Ok(job.map(|(job, attempt_id)| {
            let archive_schema = job
                .payload
                .as_ref()
                .and_then(|p| p.get("schema"))
                .and_then(|v| v.as_str())
                .unwrap_or("public")
                .to_owned();
            ScopedClaimedJob {
                job,
                tenant_id,
                archive_schema,
                attempt_id,
            }
        }))
    }

    async fn admit_claim(&mut self, claim: &ScopedClaimedJob) -> Result<Uuid> {
        if self.scope.tenant_id() != claim.tenant_id {
            return Err(Error::InvalidInput(
                "worker claim belongs to another tenant".into(),
            ));
        }
        self.admit().await
    }

    /// Lock the current attempt for a short content transaction. Locks remain until
    /// caller commit/rollback, preventing recovery or another settlement mid-write.
    pub async fn lock_claim(&mut self, claim: &ScopedClaimedJob) -> Result<bool> {
        let tenant = self.admit_claim(claim).await?;
        Ok(sqlx::query(ACTIVE_CLAIM)
            .bind(tenant)
            .bind(claim.job.id)
            .bind(claim.attempt_id)
            .bind(claim.attempt_number())
            .bind(&claim.archive_schema)
            .fetch_optional(self.scope.executor())
            .await
            .map_err(Error::Database)?
            .is_some())
    }

    /// Validate the claimed content schema and then select it without public-table
    /// fallback for archives. Shared repositories must explicitly qualify public.
    /// Use lock_claim again before committing the caller's content transaction.
    pub async fn prepare_content(&mut self, claim: &ScopedClaimedJob) -> Result<bool> {
        if !self.lock_claim(claim).await? {
            return Ok(false);
        }
        crate::validate_schema_name(&claim.archive_schema)?;
        let tables: Vec<&str> = crate::TENANT_SCOPED_TABLES
            .iter()
            .copied()
            .filter(|table| {
                claim.archive_schema == "public" || !crate::archives::SHARED_TABLES.contains(table)
            })
            .collect();
        // Check the established inventory, not just a schema name or one note table.
        let valid: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
                SELECT 1 FROM unnest($2::text[]) expected(name)
                LEFT JOIN pg_catalog.pg_namespace n ON n.nspname=$1
                LEFT JOIN pg_catalog.pg_class c ON c.relnamespace=n.oid AND c.relname=expected.name
                LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid AND a.attname='tenant_id' AND a.attnum>0 AND NOT a.attisdropped
                WHERE c.oid IS NULL OR c.relkind<>'r' OR NOT c.relrowsecurity OR NOT c.relforcerowsecurity
                    OR a.attname IS NULL OR NOT a.attnotnull
                    OR NOT EXISTS(SELECT 1 FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid
                        AND pg_catalog.pg_get_expr(p.polqual,p.polrelid) LIKE '%app.current_tenant%'
                        AND pg_catalog.pg_get_expr(p.polwithcheck,p.polrelid) LIKE '%app.current_tenant%')
            )",
        )
        .bind(&claim.archive_schema)
        .bind(&tables)
        .fetch_one(self.scope.executor())
        .await
        .map_err(Error::Database)?;
        if !valid {
            return Err(Error::Config(
                "hosted worker content schema lacks the required tenant RLS inventory".into(),
            ));
        }
        sqlx::query("SELECT set_config('search_path',$1,true)")
            .bind(format!("pg_catalog,\"{}\",pg_temp", claim.archive_schema))
            .execute(self.scope.executor())
            .await
            .map_err(Error::Database)?;
        Ok(true)
    }

    /// Returns false if the attempt is no longer current or its archive is unavailable.
    pub async fn update_progress(
        &mut self,
        claim: &ScopedClaimedJob,
        percent: i32,
        message: Option<&str>,
    ) -> Result<bool> {
        if !(0..=100).contains(&percent) {
            return Err(Error::InvalidInput(
                "worker progress must be between 0 and 100".into(),
            ));
        }
        let tenant = self.admit_claim(claim).await?;
        let sql = format!(
            "WITH active AS MATERIALIZED ({ACTIVE_CLAIM})
            UPDATE public.job_queue q SET progress_percent=$6,progress_message=$7
            FROM active WHERE q.id=active.id"
        );
        let result = sqlx::query(&sql)
            .bind(tenant)
            .bind(claim.job.id)
            .bind(claim.attempt_id)
            .bind(claim.attempt_number())
            .bind(&claim.archive_schema)
            .bind(percent)
            .bind(message)
            .execute(self.scope.executor())
            .await
            .map_err(Error::Database)?;
        Ok(result.rows_affected() == 1)
    }

    /// Queue, attempt and history changes are one statement; the caller still commits.
    pub async fn complete(
        &mut self,
        claim: &ScopedClaimedJob,
        result: Option<Value>,
    ) -> Result<bool> {
        Ok(self
            .settle(claim, Settlement::Complete(result))
            .await?
            .is_some())
    }

    pub async fn fail(
        &mut self,
        claim: &ScopedClaimedJob,
        error: &str,
        class: JobFailureClass,
        code: &str,
    ) -> Result<bool> {
        Ok(self
            .settle(
                claim,
                Settlement::Failure {
                    error,
                    class,
                    code,
                    retry_at: None,
                },
            )
            .await?
            .is_some())
    }

    /// None means the claim lost its fence; it is not a successful retry or exhaustion.
    pub async fn retry(
        &mut self,
        claim: &ScopedClaimedJob,
        error: &str,
        class: JobFailureClass,
        code: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<Option<JobRetryOutcome>> {
        if !class.is_retryable() || retry_at <= Utc::now() {
            return Err(Error::InvalidInput(
                "worker retry requires a retryable failure and future deadline".into(),
            ));
        }
        let status = self
            .settle(
                claim,
                Settlement::Failure {
                    error,
                    class,
                    code,
                    retry_at: Some(retry_at),
                },
            )
            .await?;
        Ok(status.map(|status| {
            if status == "pending" {
                JobRetryOutcome::Scheduled {
                    next_attempt_at: retry_at,
                }
            } else {
                JobRetryOutcome::Exhausted
            }
        }))
    }

    async fn settle(
        &mut self,
        claim: &ScopedClaimedJob,
        settlement: Settlement<'_>,
    ) -> Result<Option<String>> {
        let (success, result, error, class, code, retry_at) = match settlement {
            Settlement::Complete(result) => (true, result, None, None, None, None),
            Settlement::Failure {
                error,
                class,
                code,
                retry_at,
            } => {
                PgJobRepository::validate_failure_metadata(class, code)?;
                (
                    false,
                    None,
                    Some(error),
                    Some(class.as_str()),
                    Some(code),
                    retry_at,
                )
            }
        };
        let tenant = self.admit_claim(claim).await?;
        let sql = format!("WITH active AS MATERIALIZED ({ACTIVE_CLAIM}),
            changed AS (
                UPDATE public.job_queue q SET
                    status=(CASE WHEN $6 THEN 'completed' WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count<active.max_retries THEN 'pending' ELSE 'failed' END)::public.job_status,
                    result=$7, error_message=$8, failure_class=$9,
                    failure_code=CASE WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count>=active.max_retries THEN 'retry_exhausted' ELSE $10 END,
                    retry_count=active.retry_count+CASE WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count<active.max_retries THEN 1 ELSE 0 END,
                    next_attempt_at=CASE WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count<active.max_retries THEN $11 ELSE NULL END,
                    started_at=CASE WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count<active.max_retries THEN NULL ELSE q.started_at END,
                    completed_at=CASE WHEN $11::timestamptz IS NOT NULL
                        AND active.retry_count<active.max_retries THEN NULL ELSE statement_timestamp() END,
                    progress_percent=CASE WHEN $6 THEN 100 ELSE 0 END, progress_message=NULL,
                    actual_duration_ms=LEAST(2147483647,GREATEST(0,
                        EXTRACT(EPOCH FROM (statement_timestamp()-q.started_at))*1000))::integer
                FROM active WHERE q.id=active.id
                RETURNING q.id,q.job_type,q.status,q.actual_duration_ms,q.failure_class,q.failure_code,q.next_attempt_at
            ), attempts AS (
                UPDATE public.job_attempt a SET
                    outcome=CASE changed.status WHEN 'completed' THEN 'completed'
                        WHEN 'pending' THEN 'retry_scheduled' ELSE 'terminal_failed' END,
                    completed_at=statement_timestamp(),retry_at=changed.next_attempt_at,
                    duration_ms=GREATEST(0,EXTRACT(EPOCH FROM (statement_timestamp()-a.started_at))*1000)::bigint,
                    failure_class=changed.failure_class,failure_code=changed.failure_code
                FROM changed,active WHERE a.id=active.attempt_id AND changed.id=active.id
                RETURNING a.job_id
            ), history AS (
                INSERT INTO public.job_history(id,job_type,duration_ms,payload_size,success,created_at)
                SELECT uuidv7(),job_type,actual_duration_ms,NULL,status='completed',statement_timestamp()
                FROM changed WHERE status<>'pending' RETURNING id
            )
            SELECT changed.status::text FROM changed JOIN attempts ON attempts.job_id=changed.id");
        sqlx::query_scalar(&sql)
            .bind(tenant)
            .bind(claim.job.id)
            .bind(claim.attempt_id)
            .bind(claim.attempt_number())
            .bind(&claim.archive_schema)
            .bind(success)
            .bind(result)
            .bind(error)
            .bind(class)
            .bind(code)
            .bind(retry_at)
            .fetch_optional(self.scope.executor())
            .await
            .map_err(Error::Database)
    }

    /// Recovers only this active tenant's jobs and corresponding attempt records.
    pub async fn reap_stale_running(
        &mut self,
        timeout_secs: u64,
        policy: &JobRetryPolicy,
    ) -> Result<i64> {
        if timeout_secs == 0 || timeout_secs > i32::MAX as u64 {
            return Err(Error::InvalidInput(
                "hosted worker recovery threshold is invalid".into(),
            ));
        }
        let tenant = self.admit().await?;
        reap_stale_on(
            self.scope.executor(),
            timeout_secs,
            policy,
            Some(tenant),
            None,
        )
        .await
    }

    /// Production recovery bounds affected rows per tenant, in oldest-first order.
    pub async fn reap_stale_batch(
        &mut self,
        timeout_secs: u64,
        policy: &JobRetryPolicy,
        limit: u32,
    ) -> Result<i64> {
        if timeout_secs == 0 || timeout_secs > i32::MAX as u64 || !(1..=256).contains(&limit) {
            return Err(Error::InvalidInput(
                "hosted recovery threshold or batch limit is invalid".into(),
            ));
        }
        let tenant = self.admit().await?;
        reap_stale_on(
            self.scope.executor(),
            timeout_secs,
            policy,
            Some(tenant),
            Some(limit),
        )
        .await
    }
}
