//! Job repository implementation.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row};
use tokio::sync::Notify;
use uuid::Uuid;

use matric_core::{
    new_v7, Error, Job, JobFailureClass, JobRepository, JobRetryOutcome, JobRetryPolicy, JobStatus,
    JobType, QueueStats, Result, TierGroup,
};

/// PostgreSQL implementation of JobRepository.
pub struct PgJobRepository {
    pool: Pool<Postgres>,
    /// Notify handle for event-driven worker wake (Issue #417).
    notify: Arc<Notify>,
}

impl PgJobRepository {
    /// Create a new PgJobRepository with the given connection pool and notify handle.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self {
            pool,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Create a new PgJobRepository sharing an existing notify handle.
    pub fn with_notify(pool: Pool<Postgres>, notify: Arc<Notify>) -> Self {
        Self { pool, notify }
    }

    /// Get the job notification handle for event-driven waking.
    pub fn job_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }

    /// Claim next job for a tier group, excluding jobs from paused archives (Issue #466).
    ///
    /// Jobs with `payload->>'schema'` matching any of `excluded_schemas` are skipped.
    /// Jobs without a schema in their payload (public schema) are never excluded.
    pub async fn claim_next_for_tier_excluding(
        &self,
        tier_group: TierGroup,
        job_types: &[JobType],
        excluded_schemas: &[String],
    ) -> Result<Option<Job>> {
        let now = Utc::now();
        let type_strings = Self::claim_type_strings(job_types);

        let tier_clause = match tier_group {
            TierGroup::CpuAndAgnostic => "(cost_tier IS NULL OR cost_tier = 0)",
            TierGroup::AudioGpu => "cost_tier = 5",
            TierGroup::FastGpu => "cost_tier = 1",
            TierGroup::StandardGpu => "cost_tier = 2",
            TierGroup::RenderGpu => "cost_tier = 4",
            TierGroup::VisionGpu => "cost_tier = 3",
        };

        let query = format!(
            "WITH claimed AS (
             UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1,
                 next_attempt_at = NULL, failure_class = NULL, failure_code = NULL
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                   AND (next_attempt_at IS NULL OR next_attempt_at <= $1)
                   AND {tier_clause}
                   AND job_type::text = ANY($2)
                   AND (payload->>'schema' IS NULL
                        OR payload->>'schema' NOT IN (SELECT unnest($3::text[])))
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at, cost_tier
             ),
             attempt AS (
                 INSERT INTO job_attempt (
                     id, job_id, attempt_number, started_at, payload_size,
                     payload_fingerprint, archive_schema
                 )
                 SELECT uuidv7(), id, retry_count + 1, $1,
                        octet_length(payload::text),
                        encode(sha256(convert_to(payload::text, 'UTF8')), 'hex'),
                        CASE
                            WHEN payload->>'schema' ~ '^[a-z][a-z0-9_]{{0,62}}$'
                            THEN payload->>'schema'
                            ELSE NULL
                        END
                 FROM claimed
                 RETURNING job_id
             )
             SELECT claimed.* FROM claimed
             JOIN attempt ON attempt.job_id = claimed.id"
        );

        let row = sqlx::query(&query)
            .bind(now)
            .bind(&type_strings)
            .bind(excluded_schemas)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        row.map(Self::parse_job_row).transpose()
    }

    /// Convert JobType to string for database.
    fn job_type_to_str(job_type: JobType) -> &'static str {
        job_type.as_str()
    }

    /// Convert string from database to JobType.
    fn str_to_job_type(value: &str) -> std::result::Result<JobType, String> {
        value.parse()
    }

    /// Convert JobStatus to string for database.
    #[allow(dead_code)]
    fn job_status_to_str(status: JobStatus) -> &'static str {
        status.as_str()
    }

    /// Convert string from database to JobStatus.
    fn str_to_job_status(value: &str) -> std::result::Result<JobStatus, String> {
        value.parse()
    }

    fn supported_job_type_strings() -> Vec<String> {
        JobType::ALL
            .into_iter()
            .map(|job_type| job_type.as_str().to_string())
            .collect()
    }

    fn supported_job_status_strings() -> Vec<String> {
        JobStatus::ALL
            .into_iter()
            .map(|status| status.as_str().to_string())
            .collect()
    }

    fn claim_type_strings(job_types: &[JobType]) -> Vec<String> {
        if job_types.is_empty() {
            Self::supported_job_type_strings()
        } else {
            job_types
                .iter()
                .map(|job_type| job_type.as_str().to_string())
                .collect()
        }
    }

    fn enum_diagnostic_value(value: &str) -> String {
        const MAX_ENUM_VALUE_CHARS: usize = 64;
        if value.chars().count() <= MAX_ENUM_VALUE_CHARS
            && value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
        {
            value.to_string()
        } else {
            "<redacted>".to_string()
        }
    }

    fn incompatible_job_row(job_id: Uuid, field: &'static str, value: &str) -> Error {
        let value_len = value.chars().count();
        tracing::error!(
            target: "fortemi.jobs",
            event = "job.incompatible_row",
            %job_id,
            field,
            value = %Self::enum_diagnostic_value(value),
            value_len,
            "persisted job row rejected"
        );
        Error::IncompatibleJobRow {
            job_id,
            field,
            value_len,
        }
    }

    fn validate_failure_metadata(failure_class: JobFailureClass, failure_code: &str) -> Result<()> {
        let mut characters = failure_code.chars();
        let valid = failure_code.len() <= 64
            && matches!(characters.next(), Some('a'..='z'))
            && characters.all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '_' | '.' | '-')
            });
        if !valid {
            return Err(Error::InvalidInput(
                "Job failure code must use bounded lowercase identifier syntax".to_string(),
            ));
        }
        let _ = failure_class;
        Ok(())
    }

    /// Parse a job row into a Job struct.
    fn parse_job_row(row: sqlx::postgres::PgRow) -> Result<Job> {
        let id: Uuid = row.get("id");
        let job_type_value: String = row.get("job_type");
        let status_value: String = row.get("status");
        let job_type = Self::str_to_job_type(&job_type_value)
            .map_err(|_| Self::incompatible_job_row(id, "job_type", &job_type_value))?;
        let status = Self::str_to_job_status(&status_value)
            .map_err(|_| Self::incompatible_job_row(id, "status", &status_value))?;

        Ok(Job {
            id,
            note_id: row.get("note_id"),
            job_type,
            status,
            priority: row.get("priority"),
            payload: row.get("payload"),
            result: row.get("result"),
            error_message: row.get("error_message"),
            progress_percent: row.get("progress_percent"),
            progress_message: row.get("progress_message"),
            retry_count: row.get("retry_count"),
            max_retries: row.get("max_retries"),
            created_at: row.get("created_at"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            cost_tier: row.get("cost_tier"),
        })
    }
}

#[async_trait]
impl JobRepository for PgJobRepository {
    async fn queue(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
        cost_tier: Option<i16>,
    ) -> Result<Uuid> {
        let job_id = new_v7();
        let now = Utc::now();
        let job_type_str = Self::job_type_to_str(job_type);

        // Get estimated duration
        let estimated_duration: Option<i32> =
            sqlx::query_scalar("SELECT estimate_job_duration($1::job_type, NULL)")
                .bind(job_type_str)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?
                .flatten();

        sqlx::query(
            "INSERT INTO job_queue (id, note_id, job_type, status, priority, payload, estimated_duration_ms, created_at, cost_tier)
             VALUES ($1, $2, $3::job_type, 'pending'::job_status, $4, $5, $6, $7, $8)",
        )
        .bind(job_id)
        .bind(note_id)
        .bind(job_type_str)
        .bind(priority)
        .bind(&payload)
        .bind(estimated_duration)
        .bind(now)
        .bind(cost_tier)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        self.notify.notify_waiters();
        Ok(job_id)
    }

    async fn queue_deduplicated(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
        cost_tier: Option<i16>,
    ) -> Result<Option<Uuid>> {
        let job_type_str = Self::job_type_to_str(job_type);

        // Atomic check-and-insert using INSERT ... WHERE NOT EXISTS to prevent
        // TOCTOU race conditions when concurrent requests try to queue the same job.
        // Only deduplicates when note_id is present; without note_id, always insert.
        if let Some(nid) = note_id {
            let job_id = new_v7();
            let now = Utc::now();

            let estimated_duration: Option<i32> =
                sqlx::query_scalar("SELECT estimate_job_duration($1::job_type, NULL)")
                    .bind(job_type_str)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(Error::Database)?
                    .flatten();

            let result = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO job_queue (id, note_id, job_type, status, priority, payload, estimated_duration_ms, created_at, cost_tier)
                 SELECT $1, $2, $3::job_type, 'pending'::job_status, $4, $5, $6, $7, $8
                 WHERE NOT EXISTS (
                     SELECT 1 FROM job_queue
                     WHERE note_id = $2 AND job_type = $3::job_type
                       AND status IN ('pending'::job_status, 'running'::job_status)
                 )
                 RETURNING id",
            )
            .bind(job_id)
            .bind(nid)
            .bind(job_type_str)
            .bind(priority)
            .bind(&payload)
            .bind(estimated_duration)
            .bind(now)
            .bind(cost_tier)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

            if result.is_some() {
                self.notify.notify_waiters();
            }
            Ok(result)
        } else {
            // No note_id — deduplicate by job_type alone (at most one pending/running
            // instance per job_type, e.g. GraphMaintenance).
            let job_id = new_v7();
            let now = Utc::now();

            let estimated_duration: Option<i32> =
                sqlx::query_scalar("SELECT estimate_job_duration($1::job_type, NULL)")
                    .bind(job_type_str)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(Error::Database)?
                    .flatten();

            let result = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO job_queue (id, note_id, job_type, status, priority, payload, estimated_duration_ms, created_at, cost_tier)
                 SELECT $1, NULL, $2::job_type, 'pending'::job_status, $3, $4, $5, $6, $7
                 WHERE NOT EXISTS (
                     SELECT 1 FROM job_queue
                     WHERE note_id IS NULL AND job_type = $2::job_type
                       AND status IN ('pending'::job_status, 'running'::job_status)
                 )
                 RETURNING id",
            )
            .bind(job_id)
            .bind(job_type_str)
            .bind(priority)
            .bind(&payload)
            .bind(estimated_duration)
            .bind(now)
            .bind(cost_tier)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

            if result.is_some() {
                self.notify.notify_waiters();
            }
            Ok(result)
        }
    }

    async fn queue_attachment_once(
        &self,
        attachment_id: Uuid,
        schema: &str,
        note_id: Option<Uuid>,
        job_type: JobType,
        payload: Option<JsonValue>,
    ) -> Result<Option<Uuid>> {
        let job_type_str = Self::job_type_to_str(job_type);
        let priority = job_type.default_priority();
        let cost_tier = job_type.default_cost_tier();
        let release_key = format!("{schema}:{attachment_id}:{job_type_str}");
        let mut payload = payload.unwrap_or_else(|| JsonValue::Object(Default::default()));
        let payload_object = payload.as_object_mut().ok_or_else(|| {
            Error::InvalidInput("Attachment downstream job payload must be an object".to_string())
        })?;
        payload_object.insert(
            "scan_release_key".to_string(),
            JsonValue::String(release_key.clone()),
        );

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&release_key)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        let already_queued: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                FROM job_queue
                WHERE payload->>'scan_release_key' = $1
            )",
        )
        .bind(&release_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        if already_queued {
            tx.commit().await.map_err(Error::Database)?;
            return Ok(None);
        }

        let job_id = new_v7();
        let now = Utc::now();
        let estimated_duration: Option<i32> =
            sqlx::query_scalar("SELECT estimate_job_duration($1::job_type, NULL)")
                .bind(job_type_str)
                .fetch_optional(&mut *tx)
                .await
                .map_err(Error::Database)?
                .flatten();

        sqlx::query(
            "INSERT INTO job_queue (id, note_id, job_type, status, priority, payload, estimated_duration_ms, created_at, cost_tier)
             VALUES ($1, $2, $3::job_type, 'pending'::job_status, $4, $5, $6, $7, $8)",
        )
        .bind(job_id)
        .bind(note_id)
        .bind(job_type_str)
        .bind(priority)
        .bind(payload)
        .bind(estimated_duration)
        .bind(now)
        .bind(cost_tier)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;

        self.notify.notify_waiters();
        Ok(Some(job_id))
    }

    async fn claim_next(&self) -> Result<Option<Job>> {
        self.claim_next_for_types(&[]).await
    }

    async fn claim_next_for_types(&self, job_types: &[JobType]) -> Result<Option<Job>> {
        let now = Utc::now();
        let type_strings = Self::claim_type_strings(job_types);

        // Use FOR UPDATE SKIP LOCKED for concurrent processing.
        // Filter by job type BEFORE locking (proven 20x faster than lock-then-filter
        // per graphile-worker benchmarks). An empty caller filter expands to
        // every type supported by this binary, never arbitrary database enum values.
        let row = sqlx::query(
            "WITH claimed AS (
             UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1,
                 next_attempt_at = NULL, failure_class = NULL, failure_code = NULL
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                   AND (next_attempt_at IS NULL OR next_attempt_at <= $1)
                   AND job_type::text = ANY($2)
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at, cost_tier
             ),
             attempt AS (
                 INSERT INTO job_attempt (
                     id, job_id, attempt_number, started_at, payload_size,
                     payload_fingerprint, archive_schema
                 )
                 SELECT uuidv7(), id, retry_count + 1, $1,
                        octet_length(payload::text),
                        encode(sha256(convert_to(payload::text, 'UTF8')), 'hex'),
                        CASE
                            WHEN payload->>'schema' ~ '^[a-z][a-z0-9_]{0,62}$'
                            THEN payload->>'schema'
                            ELSE NULL
                        END
                 FROM claimed
                 RETURNING job_id
             )
             SELECT claimed.* FROM claimed
             JOIN attempt ON attempt.job_id = claimed.id",
        )
        .bind(now)
        .bind(&type_strings)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        row.map(Self::parse_job_row).transpose()
    }

    async fn claim_next_for_tier(
        &self,
        tier_group: TierGroup,
        job_types: &[JobType],
    ) -> Result<Option<Job>> {
        let now = Utc::now();
        let type_strings = Self::claim_type_strings(job_types);

        // Build tier filter clause based on group.
        let tier_clause = match tier_group {
            TierGroup::CpuAndAgnostic => "(cost_tier IS NULL OR cost_tier = 0)",
            TierGroup::AudioGpu => "cost_tier = 5",
            TierGroup::FastGpu => "cost_tier = 1",
            TierGroup::StandardGpu => "cost_tier = 2",
            TierGroup::RenderGpu => "cost_tier = 4",
            TierGroup::VisionGpu => "cost_tier = 3",
        };

        let query = format!(
            "WITH claimed AS (
             UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1,
                 next_attempt_at = NULL, failure_class = NULL, failure_code = NULL
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                   AND (next_attempt_at IS NULL OR next_attempt_at <= $1)
                   AND {tier_clause}
                   AND job_type::text = ANY($2)
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at, cost_tier
             ),
             attempt AS (
                 INSERT INTO job_attempt (
                     id, job_id, attempt_number, started_at, payload_size,
                     payload_fingerprint, archive_schema
                 )
                 SELECT uuidv7(), id, retry_count + 1, $1,
                        octet_length(payload::text),
                        encode(sha256(convert_to(payload::text, 'UTF8')), 'hex'),
                        CASE
                            WHEN payload->>'schema' ~ '^[a-z][a-z0-9_]{{0,62}}$'
                            THEN payload->>'schema'
                            ELSE NULL
                        END
                 FROM claimed
                 RETURNING job_id
             )
             SELECT claimed.* FROM claimed
             JOIN attempt ON attempt.job_id = claimed.id"
        );

        let row = sqlx::query(&query)
            .bind(now)
            .bind(&type_strings)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        row.map(Self::parse_job_row).transpose()
    }

    async fn pending_count_for_tier(&self, tier: i16) -> Result<i64> {
        let supported_types = Self::supported_job_type_strings();
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM job_queue
             WHERE status = 'pending'::job_status
               AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
               AND cost_tier = $1
               AND job_type::text = ANY($2)",
        )
        .bind(tier)
        .bind(&supported_types)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(count.0)
    }

    async fn update_progress(
        &self,
        job_id: Uuid,
        percent: i32,
        message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE job_queue
             SET progress_percent = $1, progress_message = $2
             WHERE id = $3 AND status = 'running'::job_status",
        )
        .bind(percent)
        .bind(message)
        .bind(job_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(())
    }

    async fn complete(&self, job_id: Uuid, result: Option<JsonValue>) -> Result<()> {
        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Lock the active execution so a late handler cannot overwrite a stale-reaper retry.
        let job_row = sqlx::query(
            "SELECT job_type::text, payload, started_at
             FROM job_queue
             WHERE id = $1 AND status = 'running'::job_status
             FOR UPDATE",
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;

        let job_type: String = job_row.get("job_type");
        let started_at: Option<chrono::DateTime<Utc>> = job_row.get("started_at");
        let duration_ms = started_at.map(|s| (now - s).num_milliseconds() as i32);

        // Update job
        sqlx::query(
            "UPDATE job_queue
             SET status = 'completed'::job_status, completed_at = $1, result = $2,
                 progress_percent = 100, actual_duration_ms = $3
             WHERE id = $4 AND status = 'running'::job_status",
        )
        .bind(now)
        .bind(&result)
        .bind(duration_ms)
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query(
            "UPDATE job_attempt
             SET outcome = 'completed', completed_at = $1,
                 duration_ms = GREATEST(0, EXTRACT(EPOCH FROM ($1 - started_at)) * 1000)::BIGINT
             WHERE job_id = $2 AND outcome = 'running'",
        )
        .bind(now)
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Record in history
        if let Some(duration) = duration_ms {
            sqlx::query(
                "INSERT INTO job_history (id, job_type, duration_ms, payload_size, success, created_at)
                 VALUES ($1, $2::job_type, $3, NULL, true, $4)",
            )
            .bind(new_v7())
            .bind(&job_type)
            .bind(duration)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn retry(
        &self,
        job_id: Uuid,
        error: &str,
        failure_class: JobFailureClass,
        failure_code: &str,
        retry_at: chrono::DateTime<Utc>,
    ) -> Result<JobRetryOutcome> {
        Self::validate_failure_metadata(failure_class, failure_code)?;
        if !failure_class.is_retryable() {
            return Err(Error::InvalidInput(
                "Non-retryable job failure class cannot schedule a retry".to_string(),
            ));
        }

        let now = Utc::now();
        if retry_at <= now {
            return Err(Error::InvalidInput(
                "Job retry time must be in the future".to_string(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        let (retry_count, max_retries): (i32, i32) = sqlx::query_as(
            "SELECT retry_count, max_retries FROM job_queue
                 WHERE id = $1 AND status = 'running'::job_status
                 FOR UPDATE",
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;

        if retry_count < max_retries {
            sqlx::query(
                "UPDATE job_queue
                 SET status = 'pending'::job_status, retry_count = $1, error_message = $2,
                     started_at = NULL, progress_percent = 0, progress_message = NULL,
                     next_attempt_at = $3, failure_class = $4, failure_code = $5
                 WHERE id = $6",
            )
            .bind(retry_count + 1)
            .bind(error)
            .bind(retry_at)
            .bind(failure_class.as_str())
            .bind(failure_code)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            sqlx::query(
                "UPDATE job_attempt
                 SET outcome = 'retry_scheduled', completed_at = $1, retry_at = $2,
                     duration_ms = GREATEST(0, EXTRACT(EPOCH FROM ($1 - started_at)) * 1000)::BIGINT,
                     failure_class = $3, failure_code = $4
                 WHERE job_id = $5 AND attempt_number = $6 AND outcome = 'running'",
            )
            .bind(now)
            .bind(retry_at)
            .bind(failure_class.as_str())
            .bind(failure_code)
            .bind(job_id)
            .bind(retry_count + 1)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            tx.commit().await.map_err(Error::Database)?;
            return Ok(JobRetryOutcome::Scheduled {
                next_attempt_at: retry_at,
            });
        } else {
            sqlx::query(
                "UPDATE job_queue
                 SET status = 'failed'::job_status, completed_at = $1, error_message = $2,
                     next_attempt_at = NULL, failure_class = $3, failure_code = $4
                 WHERE id = $5",
            )
            .bind(now)
            .bind(error)
            .bind(failure_class.as_str())
            .bind("retry_exhausted")
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            let job_type: String =
                sqlx::query_scalar("SELECT job_type::text FROM job_queue WHERE id = $1")
                    .bind(job_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(Error::Database)?;

            sqlx::query(
                "INSERT INTO job_history (id, job_type, duration_ms, payload_size, success, created_at)
                 VALUES ($1, $2::job_type, 0, NULL, false, $3)",
            )
            .bind(new_v7())
            .bind(&job_type)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            sqlx::query(
                "UPDATE job_attempt
                 SET outcome = 'terminal_failed', completed_at = $1,
                     duration_ms = GREATEST(0, EXTRACT(EPOCH FROM ($1 - started_at)) * 1000)::BIGINT,
                     failure_class = $2, failure_code = 'retry_exhausted'
                 WHERE job_id = $3 AND attempt_number = $4 AND outcome = 'running'",
            )
            .bind(now)
            .bind(failure_class.as_str())
            .bind(job_id)
            .bind(retry_count + 1)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(JobRetryOutcome::Exhausted)
    }

    async fn fail(
        &self,
        job_id: Uuid,
        error: &str,
        failure_class: JobFailureClass,
        failure_code: &str,
    ) -> Result<()> {
        Self::validate_failure_metadata(failure_class, failure_code)?;
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        let (job_type, retry_count): (String, i32) = sqlx::query_as(
            "SELECT job_type::text, retry_count FROM job_queue
             WHERE id = $1 AND status = 'running'::job_status
             FOR UPDATE",
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query(
            "UPDATE job_queue
             SET status = 'failed'::job_status, completed_at = $1, error_message = $2,
                 next_attempt_at = NULL, failure_class = $3, failure_code = $4
             WHERE id = $5",
        )
        .bind(now)
        .bind(error)
        .bind(failure_class.as_str())
        .bind(failure_code)
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query(
            "INSERT INTO job_history (id, job_type, duration_ms, payload_size, success, created_at)
             VALUES ($1, $2::job_type, 0, NULL, false, $3)",
        )
        .bind(new_v7())
        .bind(job_type)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query(
            "UPDATE job_attempt
             SET outcome = 'terminal_failed', completed_at = $1,
                 duration_ms = GREATEST(0, EXTRACT(EPOCH FROM ($1 - started_at)) * 1000)::BIGINT,
                 failure_class = $2, failure_code = $3
             WHERE job_id = $4 AND attempt_number = $5 AND outcome = 'running'",
        )
        .bind(now)
        .bind(failure_class.as_str())
        .bind(failure_code)
        .bind(job_id)
        .bind(retry_count + 1)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<Job>> {
        let row = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at, cost_tier
             FROM job_queue WHERE id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        row.map(Self::parse_job_row).transpose()
    }

    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at, cost_tier
             FROM job_queue WHERE note_id = $1
             ORDER BY created_at DESC",
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        rows.into_iter().map(Self::parse_job_row).collect()
    }

    async fn pending_count(&self) -> Result<i64> {
        let supported_types = Self::supported_job_type_strings();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_queue
             WHERE status = 'pending'::job_status
               AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
               AND job_type::text = ANY($1)",
        )
        .bind(&supported_types)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(count)
    }

    async fn pending_count_for_type(&self, job_type: JobType) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_queue
             WHERE status = 'pending'::job_status
               AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
               AND job_type = $1::job_type",
        )
        .bind(Self::job_type_to_str(job_type))
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(count)
    }

    async fn list_recent(&self, limit: i64) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at, cost_tier
             FROM job_queue
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        rows.into_iter().map(Self::parse_job_row).collect()
    }

    async fn list_filtered(
        &self,
        status: Option<&str>,
        job_type: Option<&str>,
        note_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Job>> {
        let mut conditions = Vec::new();
        let mut param_idx = 1;

        if status.is_some() {
            conditions.push(format!("status::text = ${}", param_idx));
            param_idx += 1;
        }
        if job_type.is_some() {
            conditions.push(format!("job_type::text = ${}", param_idx));
            param_idx += 1;
        }
        if note_id.is_some() {
            conditions.push(format!("note_id = ${}", param_idx));
            param_idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let query = format!(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at, cost_tier
             FROM job_queue
             {}
             ORDER BY created_at DESC
             LIMIT ${} OFFSET ${}",
            where_clause,
            param_idx,
            param_idx + 1
        );

        let mut q = sqlx::query(&query);
        if let Some(s) = status {
            q = q.bind(s);
        }
        if let Some(jt) = job_type {
            q = q.bind(jt);
        }
        if let Some(nid) = note_id {
            q = q.bind(nid);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pool).await.map_err(Error::Database)?;
        rows.into_iter().map(Self::parse_job_row).collect()
    }

    async fn queue_stats(&self) -> Result<QueueStats> {
        let supported_types = Self::supported_job_type_strings();
        let supported_statuses = Self::supported_job_status_strings();
        let row = sqlx::query(
            "SELECT
                COUNT(*) FILTER (
                    WHERE status = 'pending'
                      AND (next_attempt_at IS NULL OR next_attempt_at <= NOW())
                      AND job_type::text = ANY($1)
                ) as pending,
                COUNT(*) FILTER (
                    WHERE status = 'pending'
                      AND next_attempt_at > NOW()
                      AND job_type::text = ANY($1)
                ) as delayed,
                COUNT(*) FILTER (
                    WHERE status = 'running'
                      AND job_type::text = ANY($1)
                ) as processing,
                COUNT(*) FILTER (
                    WHERE status = 'completed'
                      AND job_type::text = ANY($1)
                      AND completed_at > NOW() - INTERVAL '1 hour'
                ) as completed_last_hour,
                COUNT(*) FILTER (
                    WHERE status = 'failed'
                      AND job_type::text = ANY($1)
                      AND completed_at > NOW() - INTERVAL '1 hour'
                ) as failed_last_hour,
                COUNT(*) FILTER (
                    WHERE status = 'failed'
                      AND job_type::text = ANY($1)
                ) as dead,
                COUNT(*) FILTER (
                    WHERE NOT (job_type::text = ANY($1))
                       OR NOT (status::text = ANY($2))
                ) as incompatible,
                COUNT(*) as total
             FROM job_queue",
        )
        .bind(&supported_types)
        .bind(&supported_statuses)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        use sqlx::Row;
        Ok(QueueStats {
            pending: row.get::<i64, _>("pending"),
            delayed: row.get::<i64, _>("delayed"),
            processing: row.get::<i64, _>("processing"),
            completed_last_hour: row.get::<i64, _>("completed_last_hour"),
            failed_last_hour: row.get::<i64, _>("failed_last_hour"),
            dead: row.get::<i64, _>("dead"),
            incompatible: row.get::<i64, _>("incompatible"),
            total: row.get::<i64, _>("total"),
        })
    }

    async fn cleanup(&self, keep_count: i64) -> Result<i64> {
        let result = sqlx::query(
            "DELETE FROM job_queue
             WHERE id NOT IN (
                 SELECT id FROM job_queue
                 ORDER BY
                     CASE WHEN status IN ('pending', 'running') THEN 0 ELSE 1 END,
                     completed_at DESC NULLS LAST
                 LIMIT $1
             )",
        )
        .bind(keep_count)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() as i64)
    }

    async fn reap_stale_running(
        &self,
        timeout_secs: u64,
        retry_policy: &JobRetryPolicy,
    ) -> Result<i64> {
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let result = sqlx::query(
            "WITH stale AS (
                 SELECT id, retry_count, max_retries, started_at,
                        LEAST(
                            $2::BIGINT,
                            $3::BIGINT * (1::BIGINT << LEAST(retry_count, 10))
                        ) AS base_delay_ms
                 FROM job_queue
                 WHERE status = 'running'::job_status
                   AND started_at < $1
                 FOR UPDATE SKIP LOCKED
             ),
             scheduled AS (
                 SELECT stale.*,
                        NOW() + (
                            LEAST(
                                $2::BIGINT,
                                base_delay_ms
                                + (
                                    get_byte(uuid_send(id), 15)
                                    % (
                                        GREATEST(
                                            1,
                                            (base_delay_ms * $4::BIGINT / 100)::INTEGER
                                        ) + 1
                                    )
                                )
                            ) * INTERVAL '1 millisecond'
                        ) AS retry_at
                 FROM stale
             ),
             retried AS (
                 UPDATE job_queue
                 SET status = 'pending'::job_status,
                     retry_count = job_queue.retry_count + 1,
                     error_message = 'Reaped: job orphaned after worker restart',
                     started_at = NULL,
                     progress_percent = 0,
                     progress_message = NULL,
                     next_attempt_at = stale.retry_at,
                     failure_class = 'stale_worker',
                     failure_code = 'worker_lease_expired'
                 FROM scheduled AS stale
                 WHERE job_queue.id = stale.id
                   AND stale.retry_count < stale.max_retries
                 RETURNING job_queue.id, job_queue.retry_count, stale.retry_at
             ),
             retried_attempts AS (
                 UPDATE job_attempt
                 SET outcome = 'stale_reaped',
                     completed_at = NOW(),
                     retry_at = retried.retry_at,
                     duration_ms = GREATEST(
                         0,
                         EXTRACT(EPOCH FROM (NOW() - job_attempt.started_at)) * 1000
                     )::BIGINT,
                     failure_class = 'stale_worker',
                     failure_code = 'worker_lease_expired'
                 FROM retried
                 WHERE job_attempt.job_id = retried.id
                   AND job_attempt.attempt_number = retried.retry_count
                   AND job_attempt.outcome = 'running'
                 RETURNING job_attempt.job_id
             ),
             exhausted AS (
                 UPDATE job_queue
                 SET status = 'failed'::job_status,
                     completed_at = NOW(),
                     error_message = 'Reaped: job orphaned after worker restart (retries exhausted)',
                     next_attempt_at = NULL,
                     failure_class = 'stale_worker',
                     failure_code = 'retry_exhausted'
                 FROM scheduled AS stale
                 WHERE job_queue.id = stale.id
                   AND stale.retry_count >= stale.max_retries
                 RETURNING job_queue.id, job_queue.retry_count
             ),
             exhausted_attempts AS (
                 UPDATE job_attempt
                 SET outcome = 'terminal_failed',
                     completed_at = NOW(),
                     duration_ms = GREATEST(
                         0,
                         EXTRACT(EPOCH FROM (NOW() - job_attempt.started_at)) * 1000
                     )::BIGINT,
                     failure_class = 'stale_worker',
                     failure_code = 'retry_exhausted'
                 FROM exhausted
                 WHERE job_attempt.job_id = exhausted.id
                   AND job_attempt.attempt_number = exhausted.retry_count + 1
                   AND job_attempt.outcome = 'running'
                 RETURNING job_attempt.job_id
             )
             SELECT (SELECT COUNT(*) FROM retried) + (SELECT COUNT(*) FROM exhausted) AS total",
        )
        .bind(cutoff)
        .bind(i64::try_from(retry_policy.max_delay_ms).map_err(|_| {
            Error::InvalidInput("Job retry maximum delay exceeds database bounds".to_string())
        })?)
        .bind(i64::try_from(retry_policy.stale_worker_base_delay_ms).map_err(|_| {
            Error::InvalidInput("Stale-worker retry delay exceeds database bounds".to_string())
        })?)
        .bind(i64::from(retry_policy.jitter_percent))
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.get::<i64, _>("total"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_type_round_trip_covers_every_supported_variant() {
        for job_type in JobType::ALL {
            let encoded = PgJobRepository::job_type_to_str(job_type);
            assert_eq!(
                PgJobRepository::str_to_job_type(encoded),
                Ok(job_type),
                "failed to round-trip {encoded}"
            );
        }
    }

    #[test]
    fn unknown_empty_and_future_job_types_are_rejected() {
        for value in [
            "",
            "unknown_type",
            "future_worker_job",
            "attachment_processing",
        ] {
            let error = PgJobRepository::str_to_job_type(value)
                .expect_err("unknown job type must fail closed");
            assert!(error.contains(&format!("value_len={}", value.chars().count())));
            assert!(!error.contains(value) || value.is_empty());
        }
    }

    #[test]
    fn case_mismatched_job_types_are_rejected() {
        assert!(PgJobRepository::str_to_job_type("AI_REVISION").is_err());
        assert!(PgJobRepository::str_to_job_type("Embedding").is_err());
    }

    #[test]
    fn job_status_round_trip_covers_every_supported_variant() {
        for status in JobStatus::ALL {
            let encoded = PgJobRepository::job_status_to_str(status);
            assert_eq!(
                PgJobRepository::str_to_job_status(encoded),
                Ok(status),
                "failed to round-trip {encoded}"
            );
        }
    }

    #[test]
    fn unknown_empty_future_and_case_mismatched_statuses_are_rejected() {
        for value in ["", "unknown_status", "delayed", "PENDING", "Running"] {
            let error = PgJobRepository::str_to_job_status(value)
                .expect_err("unknown job status must fail closed");
            assert!(error.contains(&format!("value_len={}", value.chars().count())));
            assert!(!error.contains(value) || value.is_empty());
        }
    }

    #[test]
    fn supported_job_type_strings_are_unique_and_complete() {
        let strings: Vec<&str> = JobType::ALL
            .iter()
            .map(|job_type| PgJobRepository::job_type_to_str(*job_type))
            .collect();
        let mut unique_strings = strings.clone();
        unique_strings.sort();
        unique_strings.dedup();

        assert_eq!(
            strings.len(),
            unique_strings.len(),
            "JobType strings must be unique"
        );
        assert_eq!(
            PgJobRepository::supported_job_type_strings().len(),
            JobType::ALL.len()
        );
    }

    #[test]
    fn supported_job_status_strings_are_unique_and_complete() {
        let strings: Vec<&str> = JobStatus::ALL
            .iter()
            .map(|status| PgJobRepository::job_status_to_str(*status))
            .collect();
        let mut unique_strings = strings.clone();
        unique_strings.sort();
        unique_strings.dedup();

        assert_eq!(
            strings.len(),
            unique_strings.len(),
            "JobStatus strings must be unique"
        );
        assert_eq!(
            PgJobRepository::supported_job_status_strings().len(),
            JobStatus::ALL.len()
        );
    }

    #[test]
    fn enum_diagnostic_value_is_bounded_and_identifier_only() {
        assert_eq!(
            PgJobRepository::enum_diagnostic_value("future_job_type"),
            "future_job_type"
        );
        assert_eq!(
            PgJobRepository::enum_diagnostic_value("Bearer secret"),
            "<redacted>"
        );
        assert_eq!(
            PgJobRepository::enum_diagnostic_value(&"a".repeat(65)),
            "<redacted>"
        );
    }
}

/// Get extraction job statistics.
///
/// Returns analytics for all extraction jobs including:
/// - Total job counts by status
/// - Average duration for completed jobs
/// - Breakdown by extraction strategy
pub async fn get_extraction_stats(pool: &Pool<Postgres>) -> Result<matric_core::ExtractionStats> {
    use sqlx::Row;
    use std::collections::HashMap;

    // Get basic counts and average duration
    let stats_row = sqlx::query(
        "SELECT
            COUNT(*) as total_jobs,
            COUNT(*) FILTER (WHERE status = 'completed') as completed_jobs,
            COUNT(*) FILTER (WHERE status = 'failed') as failed_jobs,
            COUNT(*) FILTER (WHERE status = 'pending') as pending_jobs,
            AVG(EXTRACT(EPOCH FROM (completed_at - started_at)))
                FILTER (WHERE status = 'completed' AND started_at IS NOT NULL AND completed_at IS NOT NULL)
                as avg_duration_secs
         FROM job_queue
         WHERE job_type = 'extraction'::job_type"
    )
    .fetch_one(pool)
    .await
    .map_err(Error::Database)?;

    let total_jobs: i64 = stats_row.get("total_jobs");
    let completed_jobs: i64 = stats_row.get("completed_jobs");
    let failed_jobs: i64 = stats_row.get("failed_jobs");
    let pending_jobs: i64 = stats_row.get("pending_jobs");
    let avg_duration_secs: Option<f64> = stats_row.try_get("avg_duration_secs").ok();

    // Get strategy breakdown from payload->>'strategy'
    let strategy_rows = sqlx::query(
        "SELECT
            COALESCE(payload->>'strategy', 'unknown') as strategy,
            COUNT(*) as count
         FROM job_queue
         WHERE job_type = 'extraction'::job_type
         GROUP BY payload->>'strategy'",
    )
    .fetch_all(pool)
    .await
    .map_err(Error::Database)?;

    let mut strategy_breakdown = HashMap::new();
    for row in strategy_rows {
        let strategy: String = row.get("strategy");
        let count: i64 = row.get("count");
        strategy_breakdown.insert(strategy, count);
    }

    Ok(matric_core::ExtractionStats {
        total_jobs,
        completed_jobs,
        failed_jobs,
        pending_jobs,
        avg_duration_secs,
        strategy_breakdown,
    })
}

#[cfg(test)]
mod extraction_tests {
    use std::collections::HashMap;

    #[test]
    fn test_extraction_stats_serialization() {
        let mut strategy_breakdown = HashMap::new();
        strategy_breakdown.insert("pdf_text".to_string(), 5);
        strategy_breakdown.insert("text_native".to_string(), 10);

        let stats = matric_core::ExtractionStats {
            total_jobs: 20,
            completed_jobs: 15,
            failed_jobs: 2,
            pending_jobs: 3,
            avg_duration_secs: Some(2.5),
            strategy_breakdown,
        };

        // Test serialization
        let json = serde_json::to_string(&stats).expect("Should serialize");
        assert!(json.contains("\"total_jobs\":20"));
        assert!(json.contains("\"completed_jobs\":15"));
        assert!(json.contains("\"failed_jobs\":2"));
        assert!(json.contains("\"pending_jobs\":3"));
        assert!(json.contains("\"avg_duration_secs\":2.5"));

        // Test deserialization
        let deserialized: matric_core::ExtractionStats =
            serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized.total_jobs, 20);
        assert_eq!(deserialized.completed_jobs, 15);
        assert_eq!(deserialized.failed_jobs, 2);
        assert_eq!(deserialized.pending_jobs, 3);
        assert_eq!(deserialized.avg_duration_secs, Some(2.5));
        assert_eq!(deserialized.strategy_breakdown.len(), 2);
    }

    #[test]
    fn test_extraction_stats_with_null_avg() {
        let stats = matric_core::ExtractionStats {
            total_jobs: 5,
            completed_jobs: 0,
            failed_jobs: 2,
            pending_jobs: 3,
            avg_duration_secs: None,
            strategy_breakdown: HashMap::new(),
        };

        let json = serde_json::to_string(&stats).expect("Should serialize");
        assert!(json.contains("\"avg_duration_secs\":null"));

        let deserialized: matric_core::ExtractionStats =
            serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized.avg_duration_secs, None);
    }
}
