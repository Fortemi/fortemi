//! Post-batch vector index maintenance (#1181 R1).
//!
//! After a large import or refresh batch lands in an embedding set, planner
//! statistics for `embedding` go stale and the per-table autovacuum settings
//! alone may lag behind. This module offers one callable entry point,
//! [`schedule_post_batch_maintenance`], for writers such as #1177 E: when more
//! than [`POST_BATCH_MAINTENANCE_THRESHOLD_ROWS`] rows changed in a set, it
//! queues an `analyze_embedding` job that runs `ANALYZE embedding` in the
//! background. It never runs `VACUUM FULL`, which would take an
//! `ACCESS EXCLUSIVE` lock and block reads and writes on million-row tables.

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::schema_validation::validate_schema_name;
use matric_core::{Error, JobType, Result};

/// Rows changed in one set above which a post-batch `ANALYZE` is queued.
pub const POST_BATCH_MAINTENANCE_THRESHOLD_ROWS: i64 = 10_000;

/// Whether `changed_rows` warrants a post-batch `ANALYZE` job.
pub fn needs_post_batch_maintenance(changed_rows: i64) -> bool {
    changed_rows > POST_BATCH_MAINTENANCE_THRESHOLD_ROWS
}

/// Payload for `analyze_embedding` jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeEmbeddingJobPayload {
    /// Schema owning the `embedding` table to analyze.
    pub schema: String,
    /// Set whose batch triggered the maintenance.
    pub set_id: Uuid,
    /// Rows changed in the triggering batch.
    pub changed_rows: i64,
    /// Human-readable trigger, e.g. `bulk_import` or `refresh_batch`.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Queue an `ANALYZE embedding` job after a large import or refresh batch.
///
/// Returns the queued job id, or `None` when `changed_rows` is at or below
/// [`POST_BATCH_MAINTENANCE_THRESHOLD_ROWS`] or an equivalent job is already
/// pending or running for the schema.
pub async fn schedule_post_batch_maintenance(
    pool: &Pool<Postgres>,
    schema: &str,
    set_id: Uuid,
    changed_rows: i64,
) -> Result<Option<Uuid>> {
    schedule_post_batch_maintenance_with_reason(pool, schema, set_id, changed_rows, None).await
}

/// [`schedule_post_batch_maintenance`] with an explicit trigger reason.
pub async fn schedule_post_batch_maintenance_with_reason(
    pool: &Pool<Postgres>,
    schema: &str,
    set_id: Uuid,
    changed_rows: i64,
    reason: Option<&str>,
) -> Result<Option<Uuid>> {
    if !needs_post_batch_maintenance(changed_rows) {
        return Ok(None);
    }
    validate_schema_name(schema)?;
    if pending_analyze_exists(pool, schema).await? {
        return Ok(None);
    }
    let payload = serde_json::to_value(AnalyzeEmbeddingJobPayload {
        schema: schema.to_string(),
        set_id,
        changed_rows,
        reason: reason.map(str::to_string),
    })
    .map_err(|error| Error::Internal(error.to_string()))?;
    let id = matric_core::new_v7();
    sqlx::query_scalar(
        "INSERT INTO public.job_queue (id, job_type, status, priority, payload, created_at)
         VALUES ($1, $2::job_type, 'pending'::job_status, $3, $4, NOW())
         RETURNING id",
    )
    .bind(id)
    .bind(JobType::AnalyzeEmbedding.as_str())
    .bind(JobType::AnalyzeEmbedding.default_priority())
    .bind(payload)
    .fetch_optional(pool)
    .await
    .map_err(Error::Database)
}

async fn pending_analyze_exists(pool: &Pool<Postgres>, schema: &str) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM public.job_queue
            WHERE job_type = $1::job_type
              AND status IN ('pending'::job_status, 'running'::job_status)
              AND payload->>'schema' = $2
        )",
    )
    .bind(JobType::AnalyzeEmbedding.as_str())
    .bind(schema)
    .fetch_one(pool)
    .await
    .map_err(Error::Database)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_boundary() {
        assert!(!needs_post_batch_maintenance(0));
        assert!(!needs_post_batch_maintenance(10_000));
        assert!(needs_post_batch_maintenance(10_001));
    }
}
