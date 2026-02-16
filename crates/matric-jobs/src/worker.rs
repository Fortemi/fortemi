//! Job worker and runner for processing background jobs.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use matric_core::{JobRepository, JobType, Result};
use matric_db::Database;

use crate::extraction::ExtractionRegistry;
use crate::handler::{JobContext, JobHandler, JobResult};
use crate::DEFAULT_POLL_INTERVAL_MS;

/// Configuration for the job worker.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Polling interval in milliseconds.
    pub poll_interval_ms: u64,
    /// Maximum number of concurrent jobs.
    pub max_concurrent_jobs: usize,
    /// Whether to enable job processing.
    pub enabled: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            max_concurrent_jobs: matric_core::defaults::JOB_MAX_CONCURRENT,
            enabled: true,
        }
    }
}

impl WorkerConfig {
    /// Create config from environment variables (with defaults).
    ///
    /// | Variable | Default | Description |
    /// |----------|---------|-------------|
    /// | `JOB_WORKER_ENABLED` | `true` | Enable/disable job processing |
    /// | `JOB_MAX_CONCURRENT` | `4` | Max concurrent jobs |
    /// | `JOB_POLL_INTERVAL_MS` | `500` | Polling interval when queue is empty |
    pub fn from_env() -> Self {
        let enabled = std::env::var("JOB_WORKER_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let max_concurrent_jobs = std::env::var("JOB_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(matric_core::defaults::JOB_MAX_CONCURRENT)
            .max(1);

        let poll_interval_ms = std::env::var("JOB_POLL_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS);

        Self {
            poll_interval_ms,
            max_concurrent_jobs,
            enabled,
        }
    }

    /// Create a new config with custom poll interval.
    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Set maximum concurrent jobs.
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_jobs = max;
        self
    }

    /// Enable or disable job processing.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Event emitted by the job worker.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// A job was started.
    JobStarted { job_id: Uuid, job_type: JobType },
    /// Job progress was updated.
    JobProgress {
        job_id: Uuid,
        percent: i32,
        message: Option<String>,
    },
    /// A job completed successfully.
    JobCompleted { job_id: Uuid, job_type: JobType },
    /// A job failed.
    JobFailed {
        job_id: Uuid,
        job_type: JobType,
        error: String,
    },
    /// Worker started.
    WorkerStarted,
    /// Worker stopped.
    WorkerStopped,
}

/// Handle for controlling a running worker.
pub struct WorkerHandle {
    shutdown_tx: mpsc::Sender<()>,
    event_rx: broadcast::Receiver<WorkerEvent>,
}

impl WorkerHandle {
    /// Signal the worker to shut down gracefully.
    pub async fn shutdown(&self) -> Result<()> {
        self.shutdown_tx
            .send(())
            .await
            .map_err(|_| matric_core::Error::Internal("Failed to send shutdown signal".into()))?;
        Ok(())
    }

    /// Get a receiver for worker events.
    pub fn events(&self) -> broadcast::Receiver<WorkerEvent> {
        self.event_rx.resubscribe()
    }
}

/// Job worker that processes jobs from the queue.
pub struct JobWorker {
    db: Database,
    config: WorkerConfig,
    handlers: Arc<RwLock<HashMap<JobType, Arc<dyn JobHandler>>>>,
    event_tx: broadcast::Sender<WorkerEvent>,
    extraction_registry: Option<Arc<ExtractionRegistry>>,
}

impl JobWorker {
    /// Create a new job worker.
    pub fn new(
        db: Database,
        config: WorkerConfig,
        extraction_registry: Option<ExtractionRegistry>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(matric_core::defaults::EVENT_BUS_CAPACITY);
        Self {
            db,
            config,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            extraction_registry: extraction_registry.map(Arc::new),
        }
    }

    /// Register a handler for a job type.
    pub async fn register_handler<H: JobHandler + 'static>(&self, handler: H) {
        let job_type = handler.job_type();
        let mut handlers = self.handlers.write().await;
        handlers.insert(job_type, Arc::new(handler));
        debug!(?job_type, "Registered job handler");
    }

    /// Get a reference to the extraction registry (if configured).
    pub fn extraction_registry(&self) -> Option<&Arc<ExtractionRegistry>> {
        self.extraction_registry.as_ref()
    }

    /// Start the worker and return a handle for control.
    pub fn start(self) -> WorkerHandle {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let event_rx = self.event_tx.subscribe();

        let worker = Arc::new(self);
        let worker_clone = worker.clone();

        tokio::spawn(async move {
            worker_clone.run(&mut shutdown_rx).await;
        });

        WorkerHandle {
            shutdown_tx,
            event_rx,
        }
    }

    /// Run the worker loop with concurrent job processing (Issue #416).
    ///
    /// Claims up to `max_concurrent_jobs` at a time and processes them concurrently.
    /// Only sleeps when the queue is empty (backpressure-aware polling).
    #[instrument(skip(self, shutdown_rx))]
    async fn run(&self, shutdown_rx: &mut mpsc::Receiver<()>) {
        if !self.config.enabled {
            info!("Job worker is disabled, not starting");
            return;
        }

        info!(
            poll_interval_ms = self.config.poll_interval_ms,
            max_concurrent = self.config.max_concurrent_jobs,
            "Job worker started"
        );

        let _ = self.event_tx.send(WorkerEvent::WorkerStarted);

        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let max_concurrent = self.config.max_concurrent_jobs;

        loop {
            // Check for shutdown before claiming jobs
            if shutdown_rx.try_recv().is_ok() {
                info!("Job worker received shutdown signal");
                break;
            }

            // Claim up to max_concurrent jobs
            let mut claimed = 0;
            let mut tasks = tokio::task::JoinSet::new();

            for _ in 0..max_concurrent {
                match self.claim_job().await {
                    Some(job) => {
                        claimed += 1;
                        let worker = self.clone_refs();
                        tasks.spawn(async move {
                            worker.execute_job(job).await;
                        });
                    }
                    None => break,
                }
            }

            if claimed == 0 {
                // Queue empty — sleep before polling again
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Job worker received shutdown signal");
                        break;
                    }
                    _ = sleep(poll_interval) => {}
                }
            } else {
                debug!(claimed, "Processing concurrent job batch");
                // Wait for all claimed jobs to complete
                while let Some(result) = tasks.join_next().await {
                    if let Err(e) = result {
                        error!(error = ?e, "Job task panicked");
                    }
                }
                // No sleep — immediately try to claim more jobs
            }
        }

        let _ = self.event_tx.send(WorkerEvent::WorkerStopped);
        info!("Job worker stopped");
    }

    /// Claim the next available job without processing it.
    async fn claim_job(&self) -> Option<matric_core::Job> {
        let job_types: Vec<JobType> = {
            let handlers = self.handlers.read().await;
            handlers.keys().copied().collect()
        };

        match self.db.jobs.claim_next_for_types(&job_types).await {
            Ok(Some(job)) => Some(job),
            Ok(None) => None,
            Err(e) => {
                error!(error = ?e, "Failed to claim job");
                None
            }
        }
    }

    /// Clone references needed for spawned job tasks.
    fn clone_refs(&self) -> JobWorkerRef {
        JobWorkerRef {
            db: self.db.clone(),
            handlers: self.handlers.clone(),
            event_tx: self.event_tx.clone(),
        }
    }

    /// Get a receiver for worker events.
    pub fn events(&self) -> broadcast::Receiver<WorkerEvent> {
        self.event_tx.subscribe()
    }

    /// Get the pending job count.
    pub async fn pending_count(&self) -> Result<i64> {
        self.db.jobs.pending_count().await
    }
}

/// Lightweight reference bundle for executing a single job in a spawned task.
///
/// This avoids requiring `Arc<JobWorker>` to be `Send + Sync` for `JoinSet::spawn`.
struct JobWorkerRef {
    db: Database,
    handlers: Arc<RwLock<HashMap<JobType, Arc<dyn JobHandler>>>>,
    event_tx: broadcast::Sender<WorkerEvent>,
}

impl JobWorkerRef {
    /// Execute a single claimed job.
    async fn execute_job(self, job: matric_core::Job) {
        let start = Instant::now();
        let job_id = job.id;
        let job_type = job.job_type;

        info!(?job_id, ?job_type, "Processing job");

        let _ = self
            .event_tx
            .send(WorkerEvent::JobStarted { job_id, job_type });

        // Find a handler for this job type
        let handler = {
            let handlers = self.handlers.read().await;
            handlers.get(&job_type).cloned()
        };

        let result = match handler {
            Some(handler) => {
                let event_tx = self.event_tx.clone();
                let ctx = JobContext::new(job).with_progress_callback(move |percent, message| {
                    let _ = event_tx.send(WorkerEvent::JobProgress {
                        job_id,
                        percent,
                        message: message.map(String::from),
                    });
                });

                let job_timeout = Duration::from_secs(matric_core::defaults::JOB_TIMEOUT_SECS);
                match tokio::time::timeout(job_timeout, handler.execute(ctx)).await {
                    Ok(result) => result,
                    Err(_) => {
                        warn!(
                            ?job_id,
                            ?job_type,
                            "Job exceeded timeout of {}s",
                            matric_core::defaults::JOB_TIMEOUT_SECS
                        );
                        JobResult::Failed(format!(
                            "Job exceeded timeout of {}s",
                            matric_core::defaults::JOB_TIMEOUT_SECS
                        ))
                    }
                }
            }
            None => {
                warn!(?job_type, "No handler registered for job type");
                JobResult::Failed(format!("No handler for job type: {:?}", job_type))
            }
        };

        match result {
            JobResult::Success(result_data) => {
                if let Err(e) = self.db.jobs.complete(job_id, result_data).await {
                    error!(error = ?e, ?job_id, "Failed to mark job as completed");
                } else {
                    info!(
                        ?job_id,
                        ?job_type,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "Job completed successfully"
                    );
                    let _ = self
                        .event_tx
                        .send(WorkerEvent::JobCompleted { job_id, job_type });
                }
            }
            JobResult::Failed(error) | JobResult::Retry(error) => {
                if let Err(e) = self.db.jobs.fail(job_id, &error).await {
                    error!(error = ?e, ?job_id, "Failed to mark job as failed");
                } else {
                    warn!(
                        ?job_id,
                        ?job_type,
                        %error,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "Job failed"
                    );
                    let _ = self.event_tx.send(WorkerEvent::JobFailed {
                        job_id,
                        job_type,
                        error,
                    });
                }
            }
        }
    }
}

/// Builder for creating a job worker with handlers.
pub struct WorkerBuilder {
    db: Database,
    config: WorkerConfig,
    handlers: Vec<Box<dyn JobHandler>>,
    extraction_registry: Option<ExtractionRegistry>,
}

impl WorkerBuilder {
    /// Create a new worker builder.
    pub fn new(db: Database) -> Self {
        Self {
            db,
            config: WorkerConfig::default(),
            handlers: Vec::new(),
            extraction_registry: None,
        }
    }

    /// Set the worker configuration.
    pub fn with_config(mut self, config: WorkerConfig) -> Self {
        self.config = config;
        self
    }

    /// Add a handler.
    pub fn with_handler<H: JobHandler + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Box::new(handler));
        self
    }

    /// Set the extraction registry.
    pub fn with_extraction_registry(mut self, registry: ExtractionRegistry) -> Self {
        self.extraction_registry = Some(registry);
        self
    }

    /// Build and return the worker.
    pub async fn build(self) -> JobWorker {
        let worker = JobWorker::new(self.db, self.config, self.extraction_registry);

        for handler in self.handlers {
            let job_type = handler.job_type();
            let mut handlers = worker.handlers.write().await;
            handlers.insert(job_type, Arc::from(handler));
        }

        worker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config_default() {
        let config = WorkerConfig::default();
        assert_eq!(config.poll_interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert_eq!(config.max_concurrent_jobs, 4);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_builder() {
        let config = WorkerConfig::default()
            .with_poll_interval(1000)
            .with_max_concurrent(8)
            .with_enabled(false);

        assert_eq!(config.poll_interval_ms, 1000);
        assert_eq!(config.max_concurrent_jobs, 8);
        assert!(!config.enabled);
    }

    // ========== NEW COMPREHENSIVE TESTS ==========

    #[test]
    fn test_worker_config_default_values() {
        let config = WorkerConfig::default();
        assert_eq!(config.poll_interval_ms, 500); // DEFAULT_POLL_INTERVAL_MS
        assert_eq!(config.max_concurrent_jobs, 4);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_with_poll_interval() {
        let config = WorkerConfig::default().with_poll_interval(100);
        assert_eq!(config.poll_interval_ms, 100);
        // Ensure other defaults preserved
        assert_eq!(config.max_concurrent_jobs, 4);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_with_poll_interval_zero() {
        let config = WorkerConfig::default().with_poll_interval(0);
        assert_eq!(config.poll_interval_ms, 0);
    }

    #[test]
    fn test_worker_config_with_poll_interval_large() {
        let config = WorkerConfig::default().with_poll_interval(60000);
        assert_eq!(config.poll_interval_ms, 60000);
    }

    #[test]
    fn test_worker_config_with_max_concurrent() {
        let config = WorkerConfig::default().with_max_concurrent(16);
        assert_eq!(config.max_concurrent_jobs, 16);
        // Ensure other defaults preserved
        assert_eq!(config.poll_interval_ms, 500);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_with_max_concurrent_one() {
        let config = WorkerConfig::default().with_max_concurrent(1);
        assert_eq!(config.max_concurrent_jobs, 1);
    }

    #[test]
    fn test_worker_config_with_max_concurrent_large() {
        let config = WorkerConfig::default().with_max_concurrent(1000);
        assert_eq!(config.max_concurrent_jobs, 1000);
    }

    #[test]
    fn test_worker_config_with_enabled_true() {
        let config = WorkerConfig::default().with_enabled(true);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_with_enabled_false() {
        let config = WorkerConfig::default().with_enabled(false);
        assert!(!config.enabled);
    }

    #[test]
    fn test_worker_config_chaining() {
        let config = WorkerConfig::default()
            .with_poll_interval(2000)
            .with_max_concurrent(12)
            .with_enabled(false);

        assert_eq!(config.poll_interval_ms, 2000);
        assert_eq!(config.max_concurrent_jobs, 12);
        assert!(!config.enabled);
    }

    #[test]
    fn test_worker_config_chaining_order_independence() {
        let config1 = WorkerConfig::default()
            .with_enabled(false)
            .with_max_concurrent(10)
            .with_poll_interval(3000);

        let config2 = WorkerConfig::default()
            .with_poll_interval(3000)
            .with_enabled(false)
            .with_max_concurrent(10);

        assert_eq!(config1.poll_interval_ms, config2.poll_interval_ms);
        assert_eq!(config1.max_concurrent_jobs, config2.max_concurrent_jobs);
        assert_eq!(config1.enabled, config2.enabled);
    }

    #[test]
    fn test_worker_event_job_started() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobStarted {
            job_id,
            job_type: JobType::Embedding,
        };

        match event {
            WorkerEvent::JobStarted {
                job_id: id,
                job_type,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(job_type, JobType::Embedding);
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_job_progress() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobProgress {
            job_id,
            percent: 50,
            message: Some("halfway".to_string()),
        };

        match event {
            WorkerEvent::JobProgress {
                job_id: id,
                percent,
                message,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(percent, 50);
                assert_eq!(message, Some("halfway".to_string()));
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_job_progress_no_message() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobProgress {
            job_id,
            percent: 75,
            message: None,
        };

        match event {
            WorkerEvent::JobProgress {
                percent, message, ..
            } => {
                assert_eq!(percent, 75);
                assert!(message.is_none());
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_job_completed() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobCompleted {
            job_id,
            job_type: JobType::Linking,
        };

        match event {
            WorkerEvent::JobCompleted {
                job_id: id,
                job_type,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(job_type, JobType::Linking);
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_job_failed() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobFailed {
            job_id,
            job_type: JobType::AiRevision,
            error: "test error".to_string(),
        };

        match event {
            WorkerEvent::JobFailed {
                job_id: id,
                job_type,
                error,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(job_type, JobType::AiRevision);
                assert_eq!(error, "test error");
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_worker_started() {
        let event = WorkerEvent::WorkerStarted;
        assert!(matches!(event, WorkerEvent::WorkerStarted));
    }

    #[test]
    fn test_worker_event_worker_stopped() {
        let event = WorkerEvent::WorkerStopped;
        assert!(matches!(event, WorkerEvent::WorkerStopped));
    }

    #[test]
    fn test_worker_event_clone() {
        let job_id = Uuid::new_v4();
        let event1 = WorkerEvent::JobStarted {
            job_id,
            job_type: JobType::Embedding,
        };

        let event2 = event1.clone();

        match (event1, event2) {
            (
                WorkerEvent::JobStarted {
                    job_id: id1,
                    job_type: jt1,
                },
                WorkerEvent::JobStarted {
                    job_id: id2,
                    job_type: jt2,
                },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(jt1, jt2);
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_worker_event_debug() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobStarted {
            job_id,
            job_type: JobType::Embedding,
        };

        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("JobStarted"));
        assert!(debug_str.contains("Embedding"));
    }

    #[test]
    fn test_worker_config_debug() {
        let config = WorkerConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("WorkerConfig"));
        assert!(debug_str.contains("poll_interval_ms"));
        assert!(debug_str.contains("max_concurrent_jobs"));
        assert!(debug_str.contains("enabled"));
    }

    #[test]
    fn test_worker_config_clone() {
        let config1 = WorkerConfig::default()
            .with_poll_interval(1500)
            .with_max_concurrent(6);

        let config2 = config1.clone();

        assert_eq!(config1.poll_interval_ms, config2.poll_interval_ms);
        assert_eq!(config1.max_concurrent_jobs, config2.max_concurrent_jobs);
        assert_eq!(config1.enabled, config2.enabled);
    }
}
