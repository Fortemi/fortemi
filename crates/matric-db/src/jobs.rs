//! Job repository implementation.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Pool, Postgres, Row};
use tokio::sync::Notify;
use uuid::Uuid;

use matric_core::{
    new_v7, Error, Job, JobRepository, JobStatus, JobType, QueueStats, Result, TierGroup,
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

    /// Convert JobType to string for database.
    fn job_type_to_str(job_type: JobType) -> &'static str {
        match job_type {
            JobType::AiRevision => "ai_revision",
            JobType::Embedding => "embedding",
            JobType::Linking => "linking",
            JobType::ContextUpdate => "context_update",
            JobType::TitleGeneration => "title_generation",
            JobType::CreateEmbeddingSet => "create_embedding_set",
            JobType::RefreshEmbeddingSet => "refresh_embedding_set",
            JobType::BuildSetIndex => "build_set_index",
            JobType::PurgeNote => "purge_note",
            JobType::ConceptTagging => "concept_tagging",
            JobType::ReEmbedAll => "re_embed_all",
            JobType::EntityExtraction => "entity_extraction",
            JobType::GenerateFineTuningData => "generate_fine_tuning_data",
            JobType::EmbedForSet => "embed_for_set",
            JobType::GenerateGraphEmbedding => "generate_graph_embedding",
            JobType::GenerateCoarseEmbedding => "generate_coarse_embedding",
            JobType::ExifExtraction => "exif_extraction",
            JobType::Extraction => "extraction",
            JobType::ThreeDAnalysis => "3d_analysis",
            JobType::DocumentTypeInference => "document_type_inference",
            JobType::MetadataExtraction => "metadata_extraction",
            JobType::RelatedConceptInference => "related_concept_inference",
            JobType::ReferenceExtraction => "reference_extraction",
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
            "create_embedding_set" => JobType::CreateEmbeddingSet,
            "refresh_embedding_set" => JobType::RefreshEmbeddingSet,
            "build_set_index" => JobType::BuildSetIndex,
            "purge_note" => JobType::PurgeNote,
            "concept_tagging" => JobType::ConceptTagging,
            "re_embed_all" => JobType::ReEmbedAll,
            "entity_extraction" => JobType::EntityExtraction,
            "generate_fine_tuning_data" => JobType::GenerateFineTuningData,
            "embed_for_set" => JobType::EmbedForSet,
            "generate_graph_embedding" => JobType::GenerateGraphEmbedding,
            "generate_coarse_embedding" => JobType::GenerateCoarseEmbedding,
            "exif_extraction" => JobType::ExifExtraction,
            "extraction" => JobType::Extraction,
            "3d_analysis" => JobType::ThreeDAnalysis,
            "document_type_inference" => JobType::DocumentTypeInference,
            "metadata_extraction" => JobType::MetadataExtraction,
            "related_concept_inference" => JobType::RelatedConceptInference,
            "reference_extraction" => JobType::ReferenceExtraction,
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
            cost_tier: row.get("cost_tier"),
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
            // No note_id — can't deduplicate, just queue normally
            // (notify happens inside queue())
            let job_id = self
                .queue(note_id, job_type, priority, payload, cost_tier)
                .await?;
            Ok(Some(job_id))
        }
    }

    async fn claim_next(&self) -> Result<Option<Job>> {
        self.claim_next_for_types(&[]).await
    }

    async fn claim_next_for_types(&self, job_types: &[JobType]) -> Result<Option<Job>> {
        let now = Utc::now();
        let type_strings: Vec<String> = job_types
            .iter()
            .map(|jt| Self::job_type_to_str(*jt).to_string())
            .collect();

        // Use FOR UPDATE SKIP LOCKED for concurrent processing.
        // Filter by job type BEFORE locking (proven 20x faster than lock-then-filter
        // per graphile-worker benchmarks). Empty array = claim any type.
        let row = sqlx::query(
            "UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                   AND (cardinality($2::text[]) = 0 OR job_type::text = ANY($2))
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at, cost_tier",
        )
        .bind(now)
        .bind(&type_strings)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(Self::parse_job_row))
    }

    async fn claim_next_for_tier(
        &self,
        tier_group: TierGroup,
        job_types: &[JobType],
    ) -> Result<Option<Job>> {
        let now = Utc::now();
        let type_strings: Vec<String> = job_types
            .iter()
            .map(|jt| Self::job_type_to_str(*jt).to_string())
            .collect();

        // Build tier filter clause based on group.
        let tier_clause = match tier_group {
            TierGroup::CpuAndAgnostic => "(cost_tier IS NULL OR cost_tier = 0)",
            TierGroup::FastGpu => "cost_tier = 1",
            TierGroup::StandardGpu => "cost_tier = 2",
        };

        let query = format!(
            "UPDATE job_queue
             SET status = 'running'::job_status, started_at = $1
             WHERE id = (
                 SELECT id FROM job_queue
                 WHERE status = 'pending'::job_status
                   AND {tier_clause}
                   AND (cardinality($2::text[]) = 0 OR job_type::text = ANY($2))
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, note_id, job_type::text, status::text, priority, payload, result,
                       error_message, progress_percent, progress_message, retry_count, max_retries,
                       created_at, started_at, completed_at, cost_tier"
        );

        let row = sqlx::query(&query)
            .bind(now)
            .bind(&type_strings)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(row.map(Self::parse_job_row))
    }

    async fn pending_count_for_tier(&self, tier: i16) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM job_queue WHERE status = 'pending'::job_status AND cost_tier = $1",
        )
        .bind(tier)
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
            .bind(new_v7())
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
                    created_at, started_at, completed_at, cost_tier
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
                    created_at, started_at, completed_at, cost_tier
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
                    created_at, started_at, completed_at, cost_tier
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

#[cfg(test)]
mod tests {
    use super::*;

    // Test JobType to string conversion
    #[test]
    fn test_job_type_to_str_all_variants() {
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::AiRevision),
            "ai_revision"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::Embedding),
            "embedding"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::Linking),
            "linking"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ContextUpdate),
            "context_update"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::TitleGeneration),
            "title_generation"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::CreateEmbeddingSet),
            "create_embedding_set"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::RefreshEmbeddingSet),
            "refresh_embedding_set"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::BuildSetIndex),
            "build_set_index"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::PurgeNote),
            "purge_note"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ConceptTagging),
            "concept_tagging"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ReEmbedAll),
            "re_embed_all"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ExifExtraction),
            "exif_extraction"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ThreeDAnalysis),
            "3d_analysis"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::Extraction),
            "extraction"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::DocumentTypeInference),
            "document_type_inference"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::MetadataExtraction),
            "metadata_extraction"
        );
        assert_eq!(
            PgJobRepository::job_type_to_str(JobType::ReferenceExtraction),
            "reference_extraction"
        );
    }

    // Test string to JobType conversion
    #[test]
    fn test_str_to_job_type_all_variants() {
        assert_eq!(
            PgJobRepository::str_to_job_type("ai_revision"),
            JobType::AiRevision
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("embedding"),
            JobType::Embedding
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("linking"),
            JobType::Linking
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("context_update"),
            JobType::ContextUpdate
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("title_generation"),
            JobType::TitleGeneration
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("create_embedding_set"),
            JobType::CreateEmbeddingSet
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("refresh_embedding_set"),
            JobType::RefreshEmbeddingSet
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("build_set_index"),
            JobType::BuildSetIndex
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("purge_note"),
            JobType::PurgeNote
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("concept_tagging"),
            JobType::ConceptTagging
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("re_embed_all"),
            JobType::ReEmbedAll
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("exif_extraction"),
            JobType::ExifExtraction
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("3d_analysis"),
            JobType::ThreeDAnalysis
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("extraction"),
            JobType::Extraction
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("document_type_inference"),
            JobType::DocumentTypeInference
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("metadata_extraction"),
            JobType::MetadataExtraction
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("reference_extraction"),
            JobType::ReferenceExtraction
        );
    }

    #[test]
    fn test_str_to_job_type_unknown_fallback() {
        // Unknown strings should fall back to ContextUpdate
        assert_eq!(
            PgJobRepository::str_to_job_type("unknown_type"),
            JobType::ContextUpdate
        );
        assert_eq!(PgJobRepository::str_to_job_type(""), JobType::ContextUpdate);
        assert_eq!(
            PgJobRepository::str_to_job_type("invalid"),
            JobType::ContextUpdate
        );
    }

    #[test]
    fn test_str_to_job_type_case_sensitive() {
        // Test that conversion is case-sensitive
        assert_eq!(
            PgJobRepository::str_to_job_type("AI_REVISION"),
            JobType::ContextUpdate
        );
        assert_eq!(
            PgJobRepository::str_to_job_type("Embedding"),
            JobType::ContextUpdate
        );
    }

    // Test JobStatus to string conversion
    #[test]
    fn test_job_status_to_str_all_variants() {
        assert_eq!(
            PgJobRepository::job_status_to_str(JobStatus::Pending),
            "pending"
        );
        assert_eq!(
            PgJobRepository::job_status_to_str(JobStatus::Running),
            "running"
        );
        assert_eq!(
            PgJobRepository::job_status_to_str(JobStatus::Completed),
            "completed"
        );
        assert_eq!(
            PgJobRepository::job_status_to_str(JobStatus::Failed),
            "failed"
        );
        assert_eq!(
            PgJobRepository::job_status_to_str(JobStatus::Cancelled),
            "cancelled"
        );
    }

    // Test string to JobStatus conversion
    #[test]
    fn test_str_to_job_status_all_variants() {
        assert_eq!(
            PgJobRepository::str_to_job_status("pending"),
            JobStatus::Pending
        );
        assert_eq!(
            PgJobRepository::str_to_job_status("running"),
            JobStatus::Running
        );
        assert_eq!(
            PgJobRepository::str_to_job_status("completed"),
            JobStatus::Completed
        );
        assert_eq!(
            PgJobRepository::str_to_job_status("failed"),
            JobStatus::Failed
        );
        assert_eq!(
            PgJobRepository::str_to_job_status("cancelled"),
            JobStatus::Cancelled
        );
    }

    #[test]
    fn test_str_to_job_status_unknown_fallback() {
        // Unknown strings should fall back to Pending
        assert_eq!(
            PgJobRepository::str_to_job_status("unknown_status"),
            JobStatus::Pending
        );
        assert_eq!(PgJobRepository::str_to_job_status(""), JobStatus::Pending);
        assert_eq!(
            PgJobRepository::str_to_job_status("invalid"),
            JobStatus::Pending
        );
    }

    #[test]
    fn test_str_to_job_status_case_sensitive() {
        // Test that conversion is case-sensitive
        assert_eq!(
            PgJobRepository::str_to_job_status("PENDING"),
            JobStatus::Pending
        );
        assert_eq!(
            PgJobRepository::str_to_job_status("Running"),
            JobStatus::Pending
        );
    }

    // Test round-trip conversion for JobType
    #[test]
    fn test_job_type_round_trip() {
        let types = vec![
            JobType::AiRevision,
            JobType::Embedding,
            JobType::Linking,
            JobType::ContextUpdate,
            JobType::TitleGeneration,
            JobType::CreateEmbeddingSet,
            JobType::RefreshEmbeddingSet,
            JobType::BuildSetIndex,
            JobType::PurgeNote,
            JobType::ConceptTagging,
            JobType::ReEmbedAll,
            JobType::ExifExtraction,
            JobType::Extraction,
            JobType::ThreeDAnalysis,
            JobType::DocumentTypeInference,
            JobType::ReferenceExtraction,
        ];

        for job_type in types {
            let str_repr = PgJobRepository::job_type_to_str(job_type);
            let recovered = PgJobRepository::str_to_job_type(str_repr);
            assert_eq!(job_type, recovered);
        }
    }

    // Test round-trip conversion for JobStatus
    #[test]
    fn test_job_status_round_trip() {
        let statuses = vec![
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ];

        for status in statuses {
            let str_repr = PgJobRepository::job_status_to_str(status);
            let recovered = PgJobRepository::str_to_job_status(str_repr);
            assert_eq!(status, recovered);
        }
    }

    // Test that all JobType strings are unique
    #[test]
    fn test_job_type_strings_are_unique() {
        let types = vec![
            JobType::AiRevision,
            JobType::Embedding,
            JobType::Linking,
            JobType::ContextUpdate,
            JobType::TitleGeneration,
            JobType::CreateEmbeddingSet,
            JobType::RefreshEmbeddingSet,
            JobType::BuildSetIndex,
            JobType::PurgeNote,
            JobType::ConceptTagging,
            JobType::ReEmbedAll,
            JobType::ExifExtraction,
            JobType::Extraction,
            JobType::ThreeDAnalysis,
            JobType::DocumentTypeInference,
            JobType::ReferenceExtraction,
        ];

        let strings: Vec<&str> = types
            .iter()
            .map(|t| PgJobRepository::job_type_to_str(*t))
            .collect();
        let mut unique_strings = strings.clone();
        unique_strings.sort();
        unique_strings.dedup();

        assert_eq!(
            strings.len(),
            unique_strings.len(),
            "JobType strings must be unique"
        );
    }

    // Test that all JobStatus strings are unique
    #[test]
    fn test_job_status_strings_are_unique() {
        let statuses = [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ];

        let strings: Vec<&str> = statuses
            .iter()
            .map(|s| PgJobRepository::job_status_to_str(*s))
            .collect();
        let mut unique_strings = strings.clone();
        unique_strings.sort();
        unique_strings.dedup();

        assert_eq!(
            strings.len(),
            unique_strings.len(),
            "JobStatus strings must be unique"
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
