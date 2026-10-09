use matric_core::{Error, Job, JobType};
use matric_db::Database;
use serde_json::json;
use sqlx::Row;

pub async fn attach_child_job_progress(db: &Database, mut job: Job) -> Result<Job, Error> {
    if !matches!(
        job.job_type,
        JobType::ReEmbedAll | JobType::RefreshEmbeddingSet
    ) {
        return Ok(job);
    }

    let batch_id = job
        .result
        .as_ref()
        .and_then(|result| result.get("batch_id"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            job.payload
                .as_ref()
                .and_then(|payload| payload.get("embedding_batch_id"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string);
    let Some(batch_id) = batch_id else {
        return Ok(job);
    };

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::bigint AS total,
            COUNT(*) FILTER (WHERE status = 'completed'::job_status)::bigint AS completed,
            COUNT(*) FILTER (WHERE status = 'failed'::job_status)::bigint AS failed,
            COUNT(*) FILTER (WHERE status = 'pending'::job_status)::bigint AS pending,
            COUNT(*) FILTER (WHERE status = 'running'::job_status)::bigint AS running
        FROM job_queue
        WHERE job_type = 'embedding'::job_type
          AND payload->>'embedding_batch_id' = $1
        "#,
    )
    .bind(&batch_id)
    .fetch_one(&db.pool)
    .await
    .map_err(Error::Database)?;

    if let Some(result) = job
        .result
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        result.insert(
            "child_job_progress".to_string(),
            json!({
                "completed": row.get::<i64, _>("completed"),
                "failed": row.get::<i64, _>("failed"),
                "pending": row.get::<i64, _>("pending"),
                "running": row.get::<i64, _>("running"),
                "total": row.get::<i64, _>("total"),
            }),
        );
    }

    Ok(job)
}
