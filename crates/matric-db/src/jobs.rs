//! Job repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Error, Job, JobRepository, JobStatus, JobType, QueueStats, Result};

/// PostgreSQL implementation of JobRepository.
pub struct PgJobRepository {
    pool: Pool<Postgres>,
}

impl PgJobRepository {
    /// Create a new PgJobRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Convert JobType to string for database.
    fn job_type_to_str(job_type: JobType) -> &'static str {
        match job_type {
            JobType::AiRevision => "ai_revision",
            JobType::Embedding => "embedding",
            JobType::Linking => "linking",
            JobType::ContextUpdate => "context_update",
            JobType::TitleGeneration => "title_generation",
        }
    }

    /// Convert string from database to JobType.
    fn str_to_job_type(s: &str) -> JobType {
        match s {
            "ai_revision" => JobType::AiRevision,
            "embedding" => JobType::Embedding,
            "linking" => JobType::Linking,
            "context_update" => JobType::ContextUpdate,
            "title_generation" => JobType::TitleGeneration,
            _ => JobType::ContextUpdate, // fallback
        }
    }

    /// Convert JobStatus to string for database.
    #[allow(dead_code)]
    fn job_status_to_str(status: JobStatus) -> &'static str {
        match status {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    /// Convert string from database to JobStatus.
    fn str_to_job_status(s: &str) -> JobStatus {
        match s {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending, // fallback
        }
    }

    /// Parse a job row into a Job struct.
    fn parse_job_row(row: sqlx::postgres::PgRow) -> Job {
        Job {
            id: row.get("id"),
            note_id: row.get("note_id"),
            job_type: Self::str_to_job_type(row.get("job_type")),
            status: Self::str_to_job_status(row.get("status")),
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
        }
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
    ) -> Result<Uuid> {
        let job_id = Uuid::new_v4();
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
            "INSERT INTO job_queue (id, note_id, job_type, status, priority, payload, estimated_duration_ms, created_at)
             VALUES ($1, $2, $3::job_type, 'pending'::job_status, $4, $5, $6, $7)",
        )
        .bind(job_id)
        .bind(note_id)
        .bind(job_type_str)
        .bind(priority)
        .bind(&payload)
        .bind(estimated_duration)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(job_id)
    }

    async fn queue_deduplicated(
        &self,
        note_id: Option<Uuid>,
        job_type: JobType,
        priority: i32,
        payload: Option<JsonValue>,
    ) -> Result<Option<Uuid>> {
        let job_type_str = Self::job_type_to_str(job_type);

        // Check for existing pending job
        let existing: Option<Uuid> = if let Some(nid) = note_id {
            sqlx::query_scalar(
                "SELECT id FROM job_queue
                 WHERE note_id = $1 AND job_type = $2::job_type AND status = 'pending'::job_status
                 LIMIT 1",
            )
            .bind(nid)
            .bind(job_type_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            None
        };

        if existing.is_some() {
            return Ok(None); // Job already exists
        }

        let job_id = self.queue(note_id, job_type, priority, payload).await?;
        Ok(Some(job_id))
    }

    async fn claim_next(&self) -> Result<Option<Job>> {
        let now = Utc::now();

        // Use FOR UPDATE SKIP LOCKED for concurrent processing
        let row = sqlx::query(
            "UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at",
        )
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(Self::parse_job_row))
    }

    async fn update_progress(
        &self,
        job_id: Uuid,
        percent: i32,
        message: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE job_queue SET progress_percent = $1, progress_message = $2 WHERE id = $3",
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

        // Get job info for history
        let job_row =
            sqlx::query("SELECT job_type::text, payload, started_at FROM job_queue WHERE id = $1")
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
             WHERE id = $4",
        )
        .bind(now)
        .bind(&result)
        .bind(duration_ms)
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
            .bind(Uuid::new_v4())
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

    async fn fail(&self, job_id: Uuid, error: &str) -> Result<()> {
        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Get current retry count
        let (retry_count, max_retries): (i32, i32) =
            sqlx::query_as("SELECT retry_count, max_retries FROM job_queue WHERE id = $1")
                .bind(job_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(Error::Database)?;

        if retry_count < max_retries {
            // Retry: reset to pending with incremented retry count
            sqlx::query(
                "UPDATE job_queue
                 SET status = 'pending'::job_status, retry_count = $1, error_message = $2,
                     started_at = NULL, progress_percent = 0, progress_message = NULL
                 WHERE id = $3",
            )
            .bind(retry_count + 1)
            .bind(error)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        } else {
            // Max retries exceeded: mark as failed
            sqlx::query(
                "UPDATE job_queue
                 SET status = 'failed'::job_status, completed_at = $1, error_message = $2
                 WHERE id = $3",
            )
            .bind(now)
            .bind(error)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

            // Record failure in history
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
            .bind(Uuid::new_v4())
            .bind(&job_type)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<Job>> {
        let row = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at
             FROM job_queue WHERE id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(Self::parse_job_row))
    }

    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at
             FROM job_queue WHERE note_id = $1
             ORDER BY created_at DESC",
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(Self::parse_job_row).collect())
    }

    async fn pending_count(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_queue WHERE status = 'pending'::job_status",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;
        Ok(count)
    }

    async fn list_recent(&self, limit: i64) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, note_id, job_type::text, status::text, priority, payload, result,
                    error_message, progress_percent, progress_message, retry_count, max_retries,
                    created_at, started_at, completed_at
             FROM job_queue
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(Self::parse_job_row).collect())
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
                    created_at, started_at, completed_at
             FROM job_queue
             {}
             ORDER BY created_at DESC
             LIMIT ${} OFFSET ${}",
            where_clause, param_idx, param_idx + 1
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
        Ok(rows.into_iter().map(Self::parse_job_row).collect())
    }

    async fn queue_stats(&self) -> Result<QueueStats> {
        let row = sqlx::query(
            "SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending,
                COUNT(*) FILTER (WHERE status = 'running') as processing,
                COUNT(*) FILTER (WHERE status = 'completed' AND completed_at > NOW() - INTERVAL '1 hour') as completed_last_hour,
                COUNT(*) FILTER (WHERE status = 'failed' AND completed_at > NOW() - INTERVAL '1 hour') as failed_last_hour,
                COUNT(*) as total
             FROM job_queue"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        use sqlx::Row;
        Ok(QueueStats {
            pending: row.get::<i64, _>("pending"),
            processing: row.get::<i64, _>("processing"),
            completed_last_hour: row.get::<i64, _>("completed_last_hour"),
            failed_last_hour: row.get::<i64, _>("failed_last_hour"),
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
}
