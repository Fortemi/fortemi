use matric_core::{Error, JobRepository, TierGroup};
use matric_db::PgJobRepository;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

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
    pool
}

#[tokio::test]
async fn incompatible_rows_are_visible_but_never_claimed_or_rewritten() {
    let pool = isolated_job_pool().await;
    let repository = PgJobRepository::new(pool.clone());
    let incompatible_id = Uuid::now_v7();
    let supported_id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO job_queue (id, job_type, status, priority, cost_tier, created_at)
         VALUES
            ($1, 'attachment_processing'::job_type, 'pending'::job_status, 1000, 0, NOW()),
            ($2, 'context_update'::job_type, 'pending'::job_status, 1, 0, NOW())",
    )
    .bind(incompatible_id)
    .bind(supported_id)
    .execute(&pool)
    .await
    .expect("seed incompatible and supported jobs");

    let claimed = repository
        .claim_next()
        .await
        .expect("claim supported job")
        .expect("supported job should be available");
    assert_eq!(claimed.id, supported_id);

    let incompatible_status: String =
        sqlx::query_scalar("SELECT status::text FROM job_queue WHERE id = $1")
            .bind(incompatible_id)
            .fetch_one(&pool)
            .await
            .expect("read incompatible job status");
    assert_eq!(incompatible_status, "pending");
    assert!(
        repository
            .claim_next()
            .await
            .expect("second claim should not error")
            .is_none(),
        "claim-any must not execute a database enum unknown to this binary"
    );
    assert!(repository
        .claim_next_for_types(&[])
        .await
        .expect("filtered claim should not error")
        .is_none());
    assert!(repository
        .claim_next_for_tier(TierGroup::CpuAndAgnostic, &[])
        .await
        .expect("tier claim should not error")
        .is_none());
    assert!(repository
        .claim_next_for_tier_excluding(TierGroup::CpuAndAgnostic, &[], &[])
        .await
        .expect("archive-excluding claim should not error")
        .is_none());

    let error = repository
        .get(incompatible_id)
        .await
        .expect_err("direct reads must reject incompatible job types");
    assert!(matches!(
        error,
        Error::IncompatibleJobRow {
            job_id,
            field: "job_type",
            value_len: 21,
        } if job_id == incompatible_id
    ));

    let list_error = repository
        .list_recent(10)
        .await
        .expect_err("list reads must reject incompatible rows");
    assert!(matches!(
        list_error,
        Error::IncompatibleJobRow {
            field: "job_type",
            ..
        }
    ));

    let stats = repository.queue_stats().await.expect("read queue stats");
    assert_eq!(stats.total, 2);
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.processing, 1);
    assert_eq!(stats.incompatible, 1);
    assert_eq!(
        repository
            .pending_count()
            .await
            .expect("count supported pending jobs"),
        0
    );
    assert_eq!(
        repository
            .pending_count_for_tier(0)
            .await
            .expect("count supported pending tier jobs"),
        0
    );
    pool.close().await;
}
