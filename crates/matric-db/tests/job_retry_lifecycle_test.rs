use chrono::{Duration, Utc};
use matric_core::{
    JobFailureClass, JobRepository, JobRetryOutcome, JobRetryPolicy, JobStatus, JobType,
};
use matric_db::PgJobRepository;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

async fn isolated_job_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("connect to migrated test database");

    sqlx::query("CREATE TEMP TABLE job_queue (LIKE public.job_queue INCLUDING ALL)")
        .execute(&pool)
        .await
        .expect("create session-local job queue");
    sqlx::query("CREATE TEMP TABLE job_attempt (LIKE public.job_attempt INCLUDING ALL)")
        .execute(&pool)
        .await
        .expect("create session-local job attempt table");
    pool
}

#[tokio::test]
async fn delayed_retry_is_not_claimed_early_and_terminal_failure_is_recorded() {
    let pool = isolated_job_pool().await;
    let repository = PgJobRepository::new(pool.clone());
    let job_id = repository
        .queue(
            None,
            JobType::ContextUpdate,
            5,
            Some(json!({"schema": "public", "operation": "retry-test"})),
            None,
        )
        .await
        .expect("queue job");

    let first = repository
        .claim_next_for_types(&[JobType::ContextUpdate])
        .await
        .expect("claim first attempt")
        .expect("first attempt should be ready");
    assert_eq!(first.id, job_id);

    let retry_at = Utc::now() + Duration::minutes(5);
    let outcome = repository
        .retry(
            job_id,
            "provider temporarily unavailable",
            JobFailureClass::Transient,
            "provider_unavailable",
            retry_at,
        )
        .await
        .expect("schedule retry");
    assert!(matches!(outcome, JobRetryOutcome::Scheduled { .. }));
    assert!(
        repository
            .claim_next_for_types(&[JobType::ContextUpdate])
            .await
            .expect("early claim should not error")
            .is_none(),
        "delayed retry must not be claimed before its due time"
    );

    let stats = repository.queue_stats().await.expect("read retry stats");
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.delayed, 1);
    assert_eq!(stats.processing, 0);

    sqlx::query("UPDATE job_queue SET next_attempt_at = NOW() - INTERVAL '1 second'")
        .execute(&pool)
        .await
        .expect("make retry due");
    let second = repository
        .claim_next_for_types(&[JobType::ContextUpdate])
        .await
        .expect("claim due retry")
        .expect("retry should become claimable");
    assert_eq!(second.id, job_id);
    assert_eq!(second.retry_count, 1);

    repository
        .fail(
            job_id,
            "payload validation failed",
            JobFailureClass::Permanent,
            "payload_invalid",
        )
        .await
        .expect("record terminal failure");
    let terminal = repository
        .get(job_id)
        .await
        .expect("read terminal job")
        .expect("job should remain inspectable");
    assert_eq!(terminal.status, JobStatus::Failed);

    let attempts: Vec<(i32, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT attempt_number, outcome, failure_class, failure_code
         FROM job_attempt WHERE job_id = $1 ORDER BY attempt_number",
    )
    .bind(job_id)
    .fetch_all(&pool)
    .await
    .expect("read attempt history");
    assert_eq!(
        attempts,
        vec![
            (
                1,
                "retry_scheduled".to_string(),
                Some("transient".to_string()),
                Some("provider_unavailable".to_string()),
            ),
            (
                2,
                "terminal_failed".to_string(),
                Some("permanent".to_string()),
                Some("payload_invalid".to_string()),
            ),
        ]
    );
    let attempt_evidence: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT payload_size, payload_fingerprint, archive_schema
         FROM job_attempt WHERE job_id = $1 ORDER BY attempt_number",
    )
    .bind(job_id)
    .fetch_all(&pool)
    .await
    .expect("read redacted attempt evidence");
    assert_eq!(attempt_evidence.len(), 2);
    assert!(attempt_evidence.iter().all(|(size, digest, schema)| {
        *size > 0
            && digest.len() == 64
            && digest
                .chars()
                .all(|character| character.is_ascii_hexdigit())
            && schema.as_deref() == Some("public")
    }));
    assert_eq!(attempt_evidence[0], attempt_evidence[1]);

    let stats = repository.queue_stats().await.expect("read terminal stats");
    assert_eq!(stats.delayed, 0);
    assert_eq!(stats.dead, 1);
    pool.close().await;
}

#[tokio::test]
async fn stale_reaper_schedules_backoff_instead_of_immediate_reclaim() {
    let pool = isolated_job_pool().await;
    let repository = PgJobRepository::new(pool.clone());
    let job_id = repository
        .queue(None, JobType::ContextUpdate, 5, None, None)
        .await
        .expect("queue job");
    repository
        .claim_next_for_types(&[JobType::ContextUpdate])
        .await
        .expect("claim first attempt")
        .expect("first attempt should be ready");
    sqlx::query("UPDATE job_queue SET started_at = NOW() - INTERVAL '1 hour' WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .expect("make running job stale");

    assert_eq!(
        repository
            .reap_stale_running(60, &JobRetryPolicy::default())
            .await
            .expect("reap stale job"),
        1
    );
    assert!(
        repository
            .claim_next_for_types(&[JobType::ContextUpdate])
            .await
            .expect("post-reap claim should not error")
            .is_none(),
        "stale job must observe backoff before another claim"
    );

    let (status, retry_count, failure_class, retry_delayed): (String, i32, String, bool) =
        sqlx::query_as(
            "SELECT status::text, retry_count, failure_class,
                    next_attempt_at > NOW()
             FROM job_queue WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("read reaped job");
    assert_eq!(status, "pending");
    assert_eq!(retry_count, 1);
    assert_eq!(failure_class, "stale_worker");
    assert!(retry_delayed);

    let (outcome, failure_code): (String, String) = sqlx::query_as(
        "SELECT outcome, failure_code FROM job_attempt
         WHERE job_id = $1 AND attempt_number = 1",
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("read reaped attempt");
    assert_eq!(outcome, "stale_reaped");
    assert_eq!(failure_code, "worker_lease_expired");

    repository
        .update_progress(job_id, 95, Some("late worker update"))
        .await
        .expect("late progress update should be ignored");
    assert!(
        repository
            .complete(job_id, Some(json!({"late": true})))
            .await
            .is_err(),
        "late completion must not overwrite a stale-reaper retry"
    );
    let (status, progress_percent, result_is_null): (String, i32, bool) = sqlx::query_as(
        "SELECT status::text, progress_percent, result IS NULL
         FROM job_queue WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("read job after late worker updates");
    assert_eq!(status, "pending");
    assert_eq!(progress_percent, 0);
    assert!(result_is_null);
    pool.close().await;
}
