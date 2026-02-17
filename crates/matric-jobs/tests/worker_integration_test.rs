//! Integration tests for JobWorker functionality.
//!
//! This test suite validates:
//! - Worker-001: Worker processes jobs from queue
//! - Worker-002: Job claiming uses SKIP LOCKED for concurrency
//! - Worker-003: Retry logic exhausts retries before failing
//! - Worker-004: Event broadcasting works correctly
//! - Worker-005: Worker lifecycle (start/shutdown)
//! - Worker-006: Progress reporting updates job status
//! - Worker-007: Handler registration and execution
//!
//! Related issues:
//! - #466: Add comprehensive test suite for matric-jobs
//!
//! NOTE: These tests use #[tokio::test] with manual pool setup instead of
//! #[sqlx::test] because migrations contain `CREATE INDEX CONCURRENTLY`
//! which cannot run inside a transaction block.
//!
//! ISOLATION: Each test uses a unique JobType so workers from parallel tests
//! never compete for the same jobs (claim_next_for_types filters by registered
//! handler types). See the type assignment table in the source.

use matric_core::{JobRepository, JobStatus, JobType};
use matric_db::{create_pool, Database};
use matric_jobs::{
    JobContext, JobHandler, JobResult, NoOpHandler, WorkerBuilder, WorkerConfig, WorkerEvent,
};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use uuid::Uuid;

/// Create a test database pool from environment or default.
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test job in the database.
async fn create_test_job(
    db: &Database,
    job_type: JobType,
    note_id: Option<Uuid>,
    priority: i32,
) -> Uuid {
    db.jobs
        .queue(note_id, job_type, priority, None, None)
        .await
        .expect("Failed to create test job")
}

/// Wait for a job to reach a specific status.
async fn wait_for_job_status(
    db: &Database,
    job_id: Uuid,
    expected_status: JobStatus,
    timeout_secs: u64,
) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(Some(job)) = db.jobs.get(job_id).await {
            if job.status == expected_status {
                return true;
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Custom test handler that tracks execution.
struct TrackingHandler {
    job_type: JobType,
    executions: Arc<Mutex<Vec<Uuid>>>,
    should_fail: bool,
}

impl TrackingHandler {
    fn new(job_type: JobType, should_fail: bool) -> (Self, Arc<Mutex<Vec<Uuid>>>) {
        let executions = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                job_type,
                executions: executions.clone(),
                should_fail,
            },
            executions,
        )
    }
}

#[async_trait::async_trait]
impl JobHandler for TrackingHandler {
    fn job_type(&self) -> JobType {
        self.job_type
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        self.executions.lock().await.push(ctx.job.id);

        if self.should_fail {
            JobResult::Failed("Intentional test failure".to_string())
        } else {
            ctx.report_progress(50, Some("Halfway"));
            ctx.report_progress(100, Some("Done"));
            JobResult::Success(Some(json!({"result": "ok"})))
        }
    }
}

/// Custom handler that simulates slow execution.
struct SlowHandler {
    job_type: JobType,
    duration_ms: u64,
}

impl SlowHandler {
    fn new(job_type: JobType, duration_ms: u64) -> Self {
        Self {
            job_type,
            duration_ms,
        }
    }
}

#[async_trait::async_trait]
impl JobHandler for SlowHandler {
    fn job_type(&self) -> JobType {
        self.job_type
    }

    async fn execute(&self, _ctx: JobContext) -> JobResult {
        sleep(Duration::from_millis(self.duration_ms)).await;
        JobResult::Success(None)
    }
}

// ============================================================================
// INTEGRATION TESTS - Worker Lifecycle
//
// Job type assignments for parallel isolation (each test gets a unique type):
//   processes_single_job       → ContextUpdate
//   processes_multiple_jobs    → TitleGeneration
//   disabled_does_not_process  → CreateEmbeddingSet
//   broadcasts_events          → RefreshEmbeddingSet
//   broadcasts_progress_events → BuildSetIndex
//   retries_failed_job         → ConceptTagging
//   broadcasts_failed_event    → ReEmbedAll
//   skips_jobs_without_handler → PurgeNote (job) / GenerateGraphEmbedding (handler)
//   multiple_handler_types     → EntityExtraction + GenerateFineTuningData + EmbedForSet
//   concurrent_workers         → Embedding
//   handles_empty_queue        → GenerateGraphEmbedding
//   shutdown_gracefully        → GenerateCoarseEmbedding
//   with_job_payload           → AiRevision
//   updates_job_result         → Linking
// ============================================================================

#[tokio::test]
async fn test_worker_processes_single_job() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job_id = create_test_job(&db, JobType::ContextUpdate, None, 10).await;

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::ContextUpdate))
        .build()
        .await;

    let handle = worker.start();

    let completed = wait_for_job_status(&db, job_id, JobStatus::Completed, 5).await;
    assert!(completed, "Job should complete within timeout");

    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.progress_percent, 100);

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_processes_multiple_jobs() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job1 = create_test_job(&db, JobType::TitleGeneration, None, 10).await;
    let job2 = create_test_job(&db, JobType::TitleGeneration, None, 5).await;
    let job3 = create_test_job(&db, JobType::TitleGeneration, None, 15).await;

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(50))
        .with_handler(NoOpHandler::new(JobType::TitleGeneration))
        .build()
        .await;

    let handle = worker.start();

    let job1_done = wait_for_job_status(&db, job1, JobStatus::Completed, 10).await;
    let job2_done = wait_for_job_status(&db, job2, JobStatus::Completed, 10).await;
    let job3_done = wait_for_job_status(&db, job3, JobStatus::Completed, 10).await;

    assert!(
        job1_done && job2_done && job3_done,
        "All jobs should complete"
    );

    // Verify priority order (job3 has highest priority)
    let _job1_data = db.jobs.get(job1).await.unwrap().unwrap();
    let job2_data = db.jobs.get(job2).await.unwrap().unwrap();
    let job3_data = db.jobs.get(job3).await.unwrap().unwrap();

    // Job 3 (priority 15) should start before job 2 (priority 5)
    assert!(
        job3_data.started_at.unwrap() <= job2_data.started_at.unwrap(),
        "Higher priority job should start first"
    );

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_disabled_does_not_process_jobs() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job_id = create_test_job(&db, JobType::CreateEmbeddingSet, None, 10).await;

    // Create worker with disabled config
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_enabled(false))
        .with_handler(NoOpHandler::new(JobType::CreateEmbeddingSet))
        .build()
        .await;

    let handle = worker.start();

    // Wait a bit
    sleep(Duration::from_millis(500)).await;

    // Verify job is still pending (not processed by this disabled worker)
    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert_eq!(
        job.status,
        JobStatus::Pending,
        "Job should not be processed by disabled worker"
    );

    // Disabled workers may not have a running loop to shutdown - ignore errors
    let _ = handle.shutdown().await;
}

// ============================================================================
// INTEGRATION TESTS - Event Broadcasting
// ============================================================================

#[tokio::test]
async fn test_worker_broadcasts_events() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create and start worker FIRST, then create the job
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::RefreshEmbeddingSet))
        .build()
        .await;

    let handle = worker.start();
    let mut events = handle.events();

    // Wait for worker to start
    sleep(Duration::from_millis(50)).await;

    // Create a job AFTER worker is running to ensure this worker handles it
    let job_id = create_test_job(&db, JobType::RefreshEmbeddingSet, None, 10).await;

    // Collect events until we see our job complete or timeout
    let mut received_events = Vec::new();
    let timeout = Duration::from_secs(10);
    let start = std::time::Instant::now();

    let mut has_job_completed = false;
    while start.elapsed() < timeout && !has_job_completed {
        tokio::select! {
            event = events.recv() => {
                if let Ok(event) = event {
                    if matches!(&event, WorkerEvent::JobCompleted { job_id: id, .. } if *id == job_id) {
                        has_job_completed = true;
                    }
                    received_events.push(event);
                }
            }
            _ = sleep(Duration::from_millis(50)) => {}
        }
    }

    let has_worker_started = received_events
        .iter()
        .any(|e| matches!(e, WorkerEvent::WorkerStarted));
    let has_job_started = received_events
        .iter()
        .any(|e| matches!(e, WorkerEvent::JobStarted { job_id: id, .. } if *id == job_id));
    let has_job_completed = received_events
        .iter()
        .any(|e| matches!(e, WorkerEvent::JobCompleted { job_id: id, .. } if *id == job_id));

    assert!(has_worker_started, "Should receive WorkerStarted event");
    assert!(has_job_started, "Should receive JobStarted event");
    assert!(has_job_completed, "Should receive JobCompleted event");

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_broadcasts_progress_events() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job_id = create_test_job(&db, JobType::BuildSetIndex, None, 10).await;

    // NoOpHandler reports progress
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::BuildSetIndex))
        .build()
        .await;

    let handle = worker.start();
    let mut events = handle.events();

    // Collect progress events
    let mut progress_events = Vec::new();
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        tokio::select! {
            event = events.recv() => {
                if let Ok(WorkerEvent::JobProgress { job_id: id, percent, message }) = event {
                    if id == job_id {
                        progress_events.push((percent, message));
                    }
                    if percent == 100 {
                        break;
                    }
                }
            }
            _ = sleep(Duration::from_millis(50)) => {}
        }
    }

    // NoOpHandler reports 50% and 100% progress
    assert!(
        progress_events.len() >= 2,
        "Should receive at least 2 progress events"
    );
    assert!(
        progress_events.iter().any(|(p, _)| *p == 50),
        "Should receive 50% progress"
    );
    assert!(
        progress_events.iter().any(|(p, _)| *p == 100),
        "Should receive 100% progress"
    );

    handle.shutdown().await.unwrap();
}

// ============================================================================
// INTEGRATION TESTS - Retry Logic
// ============================================================================

#[tokio::test]
async fn test_worker_retries_failed_job() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job_id = create_test_job(&db, JobType::ConceptTagging, None, 10).await;

    // Create handler that tracks executions and always fails
    let (handler, executions) = TrackingHandler::new(JobType::ConceptTagging, true);

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(handler)
        .build()
        .await;

    let handle = worker.start();

    // Wait for job to fail (after max retries)
    let failed = wait_for_job_status(&db, job_id, JobStatus::Failed, 10).await;
    assert!(failed, "Job should fail after max retries");

    // Verify job was executed multiple times (initial + retries)
    let exec_count = executions.lock().await.len();
    assert!(
        exec_count > 1,
        "Job should be retried at least once, got {} executions",
        exec_count
    );

    // Verify retry count
    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Failed);
    assert!(
        job.retry_count >= job.max_retries,
        "Retry count should reach max_retries"
    );
    assert!(job.error_message.is_some(), "Error message should be set");

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_broadcasts_failed_event() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create a job with max_retries = 0 for faster test
    let job_id = db
        .jobs
        .queue(None, JobType::ReEmbedAll, 10, None, None)
        .await
        .unwrap();

    // Set max_retries to 0 so it fails immediately
    sqlx::query("UPDATE job_queue SET max_retries = 0 WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();

    // Create handler that always fails
    let (handler, _) = TrackingHandler::new(JobType::ReEmbedAll, true);

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(handler)
        .build()
        .await;

    let handle = worker.start();
    let mut events = handle.events();

    // Wait for JobFailed event
    let mut received_failed = false;
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout && !received_failed {
        tokio::select! {
            event = events.recv() => {
                if let Ok(WorkerEvent::JobFailed { job_id: id, error, .. }) = event {
                    if id == job_id {
                        assert!(error.contains("Intentional test failure"));
                        received_failed = true;
                    }
                }
            }
            _ = sleep(Duration::from_millis(50)) => {}
        }
    }

    assert!(received_failed, "Should receive JobFailed event");

    handle.shutdown().await.unwrap();
}

// ============================================================================
// INTEGRATION TESTS - Handler Management
// ============================================================================

#[tokio::test]
async fn test_worker_skips_jobs_without_handler() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create a job for a type the worker does NOT handle
    let job_id = create_test_job(&db, JobType::PurgeNote, None, 10).await;

    // Create worker with a DIFFERENT handler type.
    // The worker should never claim this job because it filters by registered types.
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::GenerateGraphEmbedding))
        .build()
        .await;

    let handle = worker.start();

    // Let the worker poll a few cycles
    sleep(Duration::from_millis(500)).await;

    // Job should remain pending — the worker should not claim a type it can't handle
    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert_eq!(
        job.status,
        JobStatus::Pending,
        "Job should remain pending when no worker can handle its type"
    );

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_with_multiple_handler_types() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create jobs of different types
    let job1 = create_test_job(&db, JobType::EntityExtraction, None, 10).await;
    let job2 = create_test_job(&db, JobType::GenerateFineTuningData, None, 10).await;
    let job3 = create_test_job(&db, JobType::EmbedForSet, None, 10).await;

    // Create worker with handlers for all three types
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(50))
        .with_handler(NoOpHandler::new(JobType::EntityExtraction))
        .with_handler(NoOpHandler::new(JobType::GenerateFineTuningData))
        .with_handler(NoOpHandler::new(JobType::EmbedForSet))
        .build()
        .await;

    let handle = worker.start();

    let job1_done = wait_for_job_status(&db, job1, JobStatus::Completed, 10).await;
    let job2_done = wait_for_job_status(&db, job2, JobStatus::Completed, 10).await;
    let job3_done = wait_for_job_status(&db, job3, JobStatus::Completed, 10).await;

    assert!(
        job1_done && job2_done && job3_done,
        "All jobs should complete"
    );

    handle.shutdown().await.unwrap();
}

// ============================================================================
// INTEGRATION TESTS - Job Claiming (SKIP LOCKED)
// ============================================================================

#[tokio::test]
async fn test_concurrent_workers_claim_different_jobs() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create multiple jobs and track their IDs
    let mut job_ids = Vec::new();
    for _ in 0..5 {
        job_ids.push(create_test_job(&db, JobType::Embedding, None, 10).await);
    }

    // Use slow handler to ensure jobs overlap in time
    let worker1 = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(50))
        .with_handler(SlowHandler::new(JobType::Embedding, 500))
        .build()
        .await;

    let worker2 = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(50))
        .with_handler(SlowHandler::new(JobType::Embedding, 500))
        .build()
        .await;

    let handle1 = worker1.start();
    let handle2 = worker2.start();

    // Wait for all our specific jobs to complete
    let mut all_completed = false;
    let timeout = Duration::from_secs(15);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        let mut completed = 0;
        for job_id in &job_ids {
            if let Ok(Some(job)) = db.jobs.get(*job_id).await {
                if job.status == JobStatus::Completed {
                    completed += 1;
                }
            }
        }
        if completed == job_ids.len() {
            all_completed = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    assert!(
        all_completed,
        "All jobs should complete with concurrent workers"
    );

    // Verify all our specific jobs are completed (SKIP LOCKED should prevent duplicates)
    let mut completed_count = 0;
    for job_id in &job_ids {
        if let Ok(Some(job)) = db.jobs.get(*job_id).await {
            if job.status == JobStatus::Completed {
                completed_count += 1;
            }
        }
    }

    assert_eq!(completed_count, 5, "Exactly 5 jobs should be completed");

    handle1.shutdown().await.unwrap();
    handle2.shutdown().await.unwrap();
}

// ============================================================================
// INTEGRATION TESTS - Edge Cases
// ============================================================================

#[tokio::test]
async fn test_worker_handles_empty_queue() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Start worker — GenerateGraphEmbedding is unique to this test so the
    // queue is effectively empty for this worker even if other tests have
    // pending jobs of their own types.
    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::GenerateGraphEmbedding))
        .build()
        .await;

    let handle = worker.start();

    // Let it run for a bit — should not panic or error
    sleep(Duration::from_millis(500)).await;

    // Worker should still be alive (no panic). Shutdown cleanly.
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_shutdown_gracefully() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    // Create a slow job
    create_test_job(&db, JobType::GenerateCoarseEmbedding, None, 10).await;

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(50))
        .with_handler(SlowHandler::new(JobType::GenerateCoarseEmbedding, 2000))
        .build()
        .await;

    let handle = worker.start();
    let mut events = handle.events();

    // Wait for job to start
    let mut job_started = false;
    let timeout = Duration::from_secs(3);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout && !job_started {
        tokio::select! {
            event = events.recv() => {
                if let Ok(WorkerEvent::JobStarted { .. }) = event {
                    job_started = true;
                }
            }
            _ = sleep(Duration::from_millis(50)) => {}
        }
    }

    assert!(job_started, "Job should start");

    // Shutdown immediately (while job is running)
    handle.shutdown().await.unwrap();

    // Worker should shutdown gracefully (no panic)
}

#[tokio::test]
async fn test_worker_with_job_payload() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let payload = json!({
        "model": "test-model",
        "embedding_set_id": "test-set-123"
    });

    let job_id = db
        .jobs
        .queue(None, JobType::AiRevision, 10, Some(payload.clone()), None)
        .await
        .unwrap();

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(NoOpHandler::new(JobType::AiRevision))
        .build()
        .await;

    let handle = worker.start();

    let completed = wait_for_job_status(&db, job_id, JobStatus::Completed, 5).await;
    assert!(completed, "Job with payload should complete");

    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert_eq!(job.payload, Some(payload));

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_worker_updates_job_result() {
    let pool = setup_test_pool().await;
    let db = Database::new(pool);

    let job_id = create_test_job(&db, JobType::Linking, None, 10).await;

    // Create handler that returns result
    let (handler, _) = TrackingHandler::new(JobType::Linking, false);

    let worker = WorkerBuilder::new(db.clone())
        .with_config(WorkerConfig::default().with_poll_interval(100))
        .with_handler(handler)
        .build()
        .await;

    let handle = worker.start();

    let completed = wait_for_job_status(&db, job_id, JobStatus::Completed, 5).await;
    assert!(completed, "Job should complete");

    let job = db.jobs.get(job_id).await.unwrap().unwrap();
    assert!(job.result.is_some(), "Job result should be stored");
    assert_eq!(job.result.unwrap()["result"], "ok");

    handle.shutdown().await.unwrap();
}
