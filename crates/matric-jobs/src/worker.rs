//! Job worker and runner for processing background jobs.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use matric_core::{
    cost_tier, Error, JobFailureClass, JobRepository, JobRetryOutcome, JobRetryPolicy, JobType,
    Result, TierGroup,
};
use matric_db::Database;
use matric_inference::{OllamaBackend, VisionBackend};

use crate::extraction::ExtractionRegistry;
use crate::handler::{JobContext, JobHandler, JobResult};
use crate::pause::PauseState;
use crate::DEFAULT_POLL_INTERVAL_MS;

/// Configuration for the job worker.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Safety-net poll interval in milliseconds (Issue #417).
    ///
    /// The worker is primarily event-driven (wakes instantly on job enqueue).
    /// This interval is a safety net for edge cases: crash recovery, external
    /// SQL inserts, or race conditions between notify and claim.
    pub poll_interval_ms: u64,
    /// Maximum number of concurrent jobs.
    pub max_concurrent_jobs: usize,
    /// Whether to enable job processing.
    pub enabled: bool,
    /// Bounded retry timing shared with stale-job recovery.
    pub retry_policy: JobRetryPolicy,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            max_concurrent_jobs: matric_core::defaults::JOB_MAX_CONCURRENT,
            enabled: true,
            retry_policy: JobRetryPolicy::default(),
        }
    }
}

fn worker_error_reason_code(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("permission") || text.contains("denied") {
        "permission_denied"
    } else if text.contains("not found")
        || text.contains("no such")
        || text.contains("missing")
        || text.contains("unknown")
    {
        "not_found"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timed_out"
    } else if text.contains("connection refused")
        || text.contains("cannot connect")
        || text.contains("connection")
    {
        "connection_failed"
    } else if text.contains("database") || text.contains("sql") || text.contains("postgres") {
        "database_error"
    } else if text.contains("model") || text.contains("ollama") || text.contains("inference") {
        "model_backend_error"
    } else {
        "operation_failed"
    }
}

fn worker_failure_telemetry(error: &str) -> (usize, &'static str) {
    (error.len(), worker_error_reason_code(error))
}

fn retry_failure_class(error: &str) -> JobFailureClass {
    let error = error.to_ascii_lowercase();
    if error.contains("rate limit") || error.contains("too many requests") || error.contains("429")
    {
        JobFailureClass::RateLimited
    } else if error.contains("timeout") || error.contains("timed out") {
        JobFailureClass::Timeout
    } else {
        JobFailureClass::Transient
    }
}

fn retry_delay(
    job_id: Uuid,
    retry_count: i32,
    failure_class: JobFailureClass,
    policy: JobRetryPolicy,
) -> Duration {
    let base_ms = policy.base_delay_ms(failure_class);
    let exponent = retry_count.clamp(0, 10) as u32;
    let delay_ms = base_ms
        .saturating_mul(1_u64 << exponent)
        .min(policy.max_delay_ms);
    let jitter_window = delay_ms.saturating_mul(u64::from(policy.jitter_percent)) / 100;
    let mut seed_bytes = [0_u8; 8];
    seed_bytes.copy_from_slice(&job_id.as_bytes()[..8]);
    let seed = u64::from_be_bytes(seed_bytes) ^ retry_count.max(0) as u64;
    let jitter_secs = seed % (jitter_window + 1);
    Duration::from_millis(
        delay_ms
            .saturating_add(jitter_secs)
            .min(policy.max_delay_ms),
    )
}

fn worker_job_type_len(job_type: &JobType) -> usize {
    format!("{job_type:?}").len()
}

fn worker_tier_class(tier_group: TierGroup) -> &'static str {
    match tier_group {
        TierGroup::CpuAndAgnostic => "cpu_agnostic",
        TierGroup::AudioGpu => "audio_gpu",
        TierGroup::FastGpu => "fast_gpu",
        TierGroup::StandardGpu => "standard_gpu",
        TierGroup::RenderGpu => "render_gpu",
        TierGroup::VisionGpu => "vision_gpu",
    }
}

impl WorkerConfig {
    /// Create config from environment variables (with defaults).
    ///
    /// | Variable | Default | Description |
    /// |----------|---------|-------------|
    /// | `JOB_WORKER_ENABLED` | `true` | Enable/disable job processing |
    /// | `JOB_MAX_CONCURRENT` | `1` | Max concurrent jobs |
    /// | `JOB_POLL_INTERVAL_MS` | `60000` | Safety-net poll interval (ms) |
    /// | `JOB_RETRY_BASE_DELAY_MS` | `5000` | Transient retry base delay |
    /// | `JOB_RETRY_RATE_LIMIT_BASE_DELAY_MS` | `30000` | Rate-limit retry base delay |
    /// | `JOB_RETRY_TIMEOUT_BASE_DELAY_MS` | `15000` | Timeout retry base delay |
    /// | `JOB_RETRY_STALE_BASE_DELAY_MS` | `30000` | Stale-worker retry base delay |
    /// | `JOB_RETRY_MAX_DELAY_MS` | `3600000` | Retry delay cap |
    /// | `JOB_RETRY_JITTER_PERCENT` | `20` | Deterministic jitter window |
    pub fn from_env() -> Result<Self> {
        let defaults = Self::default();
        let enabled = parse_bool_env("JOB_WORKER_ENABLED", defaults.enabled)?;
        let max_concurrent_jobs = usize::try_from(parse_u64_env(
            "JOB_MAX_CONCURRENT",
            defaults.max_concurrent_jobs as u64,
            1,
            64,
        )?)
        .map_err(|_| Error::Config("JOB_MAX_CONCURRENT exceeds platform bounds".to_string()))?;
        let poll_interval_ms = parse_u64_env(
            "JOB_POLL_INTERVAL_MS",
            defaults.poll_interval_ms,
            100,
            300_000,
        )?;

        let mut retry_policy = defaults.retry_policy;
        retry_policy.transient_base_delay_ms = parse_u64_env(
            "JOB_RETRY_BASE_DELAY_MS",
            retry_policy.transient_base_delay_ms,
            100,
            600_000,
        )?;
        retry_policy.rate_limit_base_delay_ms = parse_u64_env(
            "JOB_RETRY_RATE_LIMIT_BASE_DELAY_MS",
            retry_policy.rate_limit_base_delay_ms,
            100,
            600_000,
        )?;
        retry_policy.timeout_base_delay_ms = parse_u64_env(
            "JOB_RETRY_TIMEOUT_BASE_DELAY_MS",
            retry_policy.timeout_base_delay_ms,
            100,
            600_000,
        )?;
        retry_policy.stale_worker_base_delay_ms = parse_u64_env(
            "JOB_RETRY_STALE_BASE_DELAY_MS",
            retry_policy.stale_worker_base_delay_ms,
            100,
            600_000,
        )?;
        retry_policy.max_delay_ms = parse_u64_env(
            "JOB_RETRY_MAX_DELAY_MS",
            retry_policy.max_delay_ms,
            100,
            86_400_000,
        )?;
        retry_policy.jitter_percent = u8::try_from(parse_u64_env(
            "JOB_RETRY_JITTER_PERCENT",
            u64::from(retry_policy.jitter_percent),
            0,
            100,
        )?)
        .expect("validated retry jitter must fit u8");
        let largest_base = [
            retry_policy.transient_base_delay_ms,
            retry_policy.rate_limit_base_delay_ms,
            retry_policy.timeout_base_delay_ms,
            retry_policy.stale_worker_base_delay_ms,
        ]
        .into_iter()
        .max()
        .expect("retry policy has fixed base delays");
        if retry_policy.max_delay_ms < largest_base {
            return Err(Error::Config(
                "JOB_RETRY_MAX_DELAY_MS must be at least every retry base delay".to_string(),
            ));
        }

        Ok(Self {
            poll_interval_ms,
            max_concurrent_jobs,
            enabled,
            retry_policy,
        })
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

    /// Override retry timing, primarily for deployments and deterministic tests.
    pub fn with_retry_policy(mut self, retry_policy: JobRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }
}

fn read_env(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::Config(format!("{name} must be valid UTF-8")))
        }
    }
}

fn parse_bool_value(name: &str, value: &str) -> Result<bool> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(Error::Config(format!(
            "{name} must be one of true, false, 1, or 0"
        ))),
    }
}

fn parse_bool_env(name: &str, default: bool) -> Result<bool> {
    read_env(name)?
        .as_deref()
        .map(|value| parse_bool_value(name, value))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_bounded_u64_value(name: &str, value: &str, min: u64, max: u64) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| Error::Config(format!("{name} must be an unsigned integer")))?;
    if !(min..=max).contains(&parsed) {
        return Err(Error::Config(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(parsed)
}

fn parse_u64_env(name: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    read_env(name)?
        .as_deref()
        .map(|value| parse_bounded_u64_value(name, value, min, max))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

/// Event emitted by the job worker.
#[derive(Clone)]
pub enum WorkerEvent {
    /// A downstream job was queued by a handler (not directly by user request).
    JobQueued {
        job_id: Uuid,
        job_type: JobType,
        note_id: Option<Uuid>,
    },
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
    /// A retry was durably scheduled.
    JobRetryScheduled {
        job_id: Uuid,
        job_type: JobType,
        failure_class: JobFailureClass,
        failure_code: String,
        next_attempt_at: chrono::DateTime<chrono::Utc>,
        retry_count: i32,
    },
    /// A job failed terminally.
    JobFailed {
        job_id: Uuid,
        job_type: JobType,
        error: String,
        failure_class: JobFailureClass,
        failure_code: String,
    },
    /// Worker started.
    WorkerStarted,
    /// Worker stopped.
    WorkerStopped,
}

impl fmt::Debug for WorkerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JobQueued {
                job_id,
                job_type,
                note_id,
            } => f
                .debug_struct("JobQueued")
                .field("job_id_set", &job_id_set(job_id))
                .field("job_type_len", &worker_job_type_len(job_type))
                .field("note_id_set", &note_id.is_some())
                .finish(),
            Self::JobStarted { job_id, job_type } => f
                .debug_struct("JobStarted")
                .field("job_id_set", &job_id_set(job_id))
                .field("job_type_len", &worker_job_type_len(job_type))
                .finish(),
            Self::JobProgress {
                job_id,
                percent,
                message,
            } => f
                .debug_struct("JobProgress")
                .field("job_id_set", &job_id_set(job_id))
                .field("percent", percent)
                .field("message_len", &message.as_deref().map(str::len))
                .finish(),
            Self::JobCompleted { job_id, job_type } => f
                .debug_struct("JobCompleted")
                .field("job_id_set", &job_id_set(job_id))
                .field("job_type_len", &worker_job_type_len(job_type))
                .finish(),
            Self::JobRetryScheduled {
                job_id,
                job_type,
                failure_class,
                failure_code,
                next_attempt_at,
                retry_count,
            } => f
                .debug_struct("JobRetryScheduled")
                .field("job_id_set", &job_id_set(job_id))
                .field("job_type_len", &worker_job_type_len(job_type))
                .field("failure_class", failure_class)
                .field("failure_code", failure_code)
                .field("next_attempt_at", next_attempt_at)
                .field("retry_count", retry_count)
                .finish(),
            Self::JobFailed {
                job_id,
                job_type,
                error,
                failure_class,
                failure_code,
            } => {
                let (error_len, _) = worker_failure_telemetry(error);
                f.debug_struct("JobFailed")
                    .field("job_id_set", &job_id_set(job_id))
                    .field("job_type_len", &worker_job_type_len(job_type))
                    .field("error_len", &error_len)
                    .field("failure_class", failure_class)
                    .field("failure_code", failure_code)
                    .finish()
            }
            Self::WorkerStarted => f.write_str("WorkerStarted"),
            Self::WorkerStopped => f.write_str("WorkerStopped"),
        }
    }
}

fn job_id_set(_: &Uuid) -> bool {
    true
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
    /// Fast GPU model backend for tier-1 warmup. None if fast model is disabled.
    fast_backend: Option<Arc<OllamaBackend>>,
    /// Standard GPU model backend for tier-2 warmup.
    standard_backend: Option<Arc<OllamaBackend>>,
    /// Vision GPU model backend for VRAM lifecycle management.
    /// Used to unload the vision model after the vision tier drains.
    vision_backend: Option<Arc<dyn VisionBackend>>,
    /// Pause state for global and per-archive job processing control (Issue #466).
    pause_state: Option<PauseState>,
    /// Sidecar lifecycle controller for GPU-exclusive mode (#576).
    /// Manages whisper/pyannote container start/stop at audio tier boundaries.
    sidecar_controller: Arc<dyn crate::sidecar::SidecarController>,
}

impl JobWorker {
    /// Create a new job worker.
    pub fn new(
        db: Database,
        config: WorkerConfig,
        extraction_registry: Option<ExtractionRegistry>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(matric_core::defaults::EVENT_BUS_CAPACITY);
        // Select sidecar controller based on GPU_EXCLUSIVE_MODE (#576)
        let sidecar_controller: Arc<dyn crate::sidecar::SidecarController> =
            if matric_core::defaults::gpu_exclusive_mode() {
                Arc::new(crate::sidecar::DockerSidecarController::new())
            } else {
                Arc::new(crate::sidecar::NoOpSidecarController)
            };

        Self {
            db,
            config,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            extraction_registry: extraction_registry.map(Arc::new),
            fast_backend: None,
            standard_backend: None,
            vision_backend: None,
            pause_state: None,
            sidecar_controller,
        }
    }

    /// Set the fast GPU model backend for tier-1 warmup.
    pub fn with_fast_backend(mut self, backend: Option<OllamaBackend>) -> Self {
        self.fast_backend = backend.map(Arc::new);
        self
    }

    /// Set the standard GPU model backend for tier-2 warmup.
    pub fn with_standard_backend(mut self, backend: Option<OllamaBackend>) -> Self {
        self.standard_backend = backend.map(Arc::new);
        self
    }

    /// Set the vision GPU model backend for VRAM lifecycle management.
    pub fn with_vision_backend(mut self, backend: Option<Arc<dyn VisionBackend>>) -> Self {
        self.vision_backend = backend;
        self
    }

    /// Set the pause state manager for global/per-archive pause control (Issue #466).
    pub fn with_pause_state(mut self, pause_state: PauseState) -> Self {
        self.pause_state = Some(pause_state);
        self
    }

    /// Get a reference to the pause state (if configured).
    pub fn pause_state(&self) -> Option<&PauseState> {
        self.pause_state.as_ref()
    }

    /// Register a handler for a job type.
    pub async fn register_handler<H: JobHandler + 'static>(&self, handler: H) {
        let job_type = handler.job_type();
        let mut handlers = self.handlers.write().await;
        handlers.insert(job_type, Arc::new(handler));
        debug!(
            job_type_len = worker_job_type_len(&job_type),
            "Registered job handler"
        );
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

    /// Run the event-driven worker loop (Issue #417, builds on #416).
    ///
    /// The worker sleeps until one of:
    /// - A job is enqueued (instant wake via `Notify`)
    /// - The safety-net poll interval expires (catches edge cases)
    /// - A shutdown signal is received
    ///
    /// On wake, it drains all available jobs in concurrent batches before
    /// going back to sleep. This eliminates idle polling and reduces latency
    /// from 0-500ms to <1ms for new jobs.
    #[instrument(skip(self, shutdown_rx))]
    async fn run(&self, shutdown_rx: &mut mpsc::Receiver<()>) {
        if !self.config.enabled {
            info!("Job worker is disabled, not starting");
            return;
        }

        // Reap orphaned running jobs from a previous process (crash/restart).
        // Use 2x the job timeout as the staleness threshold to avoid reaping
        // jobs that are legitimately still running during normal operation.
        let stale_threshold = matric_core::defaults::JOB_TIMEOUT_SECS * 2;
        match self
            .db
            .jobs
            .reap_stale_running(stale_threshold, &self.config.retry_policy)
            .await
        {
            Ok(0) => debug!("No stale running jobs to reap"),
            Ok(n) => warn!(count = n, "Reaped stale running jobs from previous worker"),
            Err(e) => {
                let error_text = e.to_string();
                error!(
                    error_len = error_text.len(),
                    error_reason = worker_error_reason_code(&error_text),
                    "Failed to reap stale running jobs"
                );
            }
        }

        let job_notify = self.db.jobs.job_notify();
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let max_concurrent = self.config.max_concurrent_jobs;
        let gpu_concurrent = matric_core::defaults::gpu_max_concurrent();

        info!(
            safety_net_interval_ms = self.config.poll_interval_ms,
            max_concurrent, gpu_concurrent, "Job worker started (event-driven)"
        );

        let _ = self.event_tx.send(WorkerEvent::WorkerStarted);

        loop {
            // Wait for a wake signal: job enqueue, safety-net timeout, or shutdown
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Job worker received shutdown signal");
                    break;
                }
                _ = job_notify.notified() => {
                    debug!("Worker woke: job enqueue notification");
                }
                _ = sleep(poll_interval) => {
                    debug!("Worker woke: safety-net poll");
                }
            }

            // Issue #466: Skip drain loop if globally paused.
            if let Some(ref ps) = self.pause_state {
                if ps.is_globally_paused() {
                    debug!("Job processing globally paused, skipping drain");
                    continue;
                }
            }

            // Collect paused archives for per-archive filtering (Issue #466).
            let excluded_archives: Vec<String> = if let Some(ref ps) = self.pause_state {
                ps.paused_archive_names().await
            } else {
                Vec::new()
            };

            // Tiered drain loop: process jobs by cost tier to avoid VRAM contention.
            // Each tier is fully drained before moving to the next. Model warmup
            // happens between tier switches so only one generation model is loaded.
            loop {
                // Check for shutdown between drain iterations
                if shutdown_rx.try_recv().is_ok() {
                    info!("Job worker received shutdown signal during drain");
                    let _ = self.event_tx.send(WorkerEvent::WorkerStopped);
                    info!("Job worker stopped");
                    return;
                }

                let mut any_processed = false;

                // Phase 1: Drain tier NULL + tier 0 (CPU/agnostic jobs — no GPU needed)
                let drained = self
                    .drain_tier(
                        TierGroup::CpuAndAgnostic,
                        max_concurrent,
                        &excluded_archives,
                    )
                    .await;
                if drained > 0 {
                    any_processed = true;
                }

                // Phase 1b: Drain audio GPU tier with sidecar lifecycle (#576).
                // When GPU_EXCLUSIVE_MODE is enabled, start whisper+pyannote sidecars
                // before audio jobs and stop them after, freeing ~6.6 GB VRAM for
                // subsequent Ollama tiers.
                let audio_pending = self
                    .db
                    .jobs
                    .pending_count_for_tier(cost_tier::AUDIO_GPU)
                    .await
                    .unwrap_or(0);
                if audio_pending > 0 {
                    // Start sidecars (no-op if GPU_EXCLUSIVE_MODE=false)
                    crate::sidecar::start_all_sidecars(self.sidecar_controller.as_ref()).await;

                    let drained = self
                        .drain_tier(TierGroup::AudioGpu, gpu_concurrent, &excluded_archives)
                        .await;
                    if drained > 0 {
                        any_processed = true;
                    }

                    // Stop sidecars to free VRAM before Ollama tiers
                    crate::sidecar::stop_all_sidecars(self.sidecar_controller.as_ref()).await;
                }

                // Phase 2: Drain tier 1 (fast GPU) with warmup.
                // GPU tiers use gpu_concurrent (default 1 = serial) to avoid
                // VRAM contention. Set GPU_MAX_CONCURRENT=N for parallel.
                let tier1_pending = self
                    .db
                    .jobs
                    .pending_count_for_tier(cost_tier::FAST_GPU)
                    .await
                    .unwrap_or(0);
                if tier1_pending > 0 {
                    if let Some(ref fast) = self.fast_backend {
                        if let Err(e) = fast.warmup().await {
                            let error_text = e.to_string();
                            warn!(
                                error_len = error_text.len(),
                                error_reason = worker_error_reason_code(&error_text),
                                "Fast model warmup failed, proceeding anyway"
                            );
                        }
                    }
                    let drained = self
                        .drain_tier(TierGroup::FastGpu, gpu_concurrent, &excluded_archives)
                        .await;
                    if drained > 0 {
                        any_processed = true;
                        // Unload fast model — free VRAM before the next tier loads its model
                        if let Some(ref fast) = self.fast_backend {
                            if let Err(e) = fast.unload().await {
                                let error_text = e.to_string();
                                warn!(
                                    error_len = error_text.len(),
                                    error_reason = worker_error_reason_code(&error_text),
                                    "Fast model unload failed"
                                );
                            }
                        }
                    }
                }

                // Phase 3: Drain tier 2 (standard GPU) with warmup
                let tier2_pending = self
                    .db
                    .jobs
                    .pending_count_for_tier(cost_tier::STANDARD_GPU)
                    .await
                    .unwrap_or(0);
                if tier2_pending > 0 {
                    if let Some(ref standard) = self.standard_backend {
                        if let Err(e) = standard.warmup().await {
                            let error_text = e.to_string();
                            warn!(
                                error_len = error_text.len(),
                                error_reason = worker_error_reason_code(&error_text),
                                "Standard model warmup failed, proceeding anyway"
                            );
                        }
                    }
                    let drained = self
                        .drain_tier(TierGroup::StandardGpu, gpu_concurrent, &excluded_archives)
                        .await;
                    if drained > 0 {
                        any_processed = true;
                        // Unload standard model — free VRAM before vision/render tiers
                        if let Some(ref standard) = self.standard_backend {
                            if let Err(e) = standard.unload().await {
                                let error_text = e.to_string();
                                warn!(
                                    error_len = error_text.len(),
                                    error_reason = worker_error_reason_code(&error_text),
                                    "Standard model unload failed"
                                );
                            }
                        }
                    }
                }

                // Phase 4: Drain tier 4 (render GPU) — Open3D multi-view rendering.
                // No model warmup needed: uses the bundled HTTP renderer, not Ollama.
                // Must drain before vision GPU so rendered views are available
                // when ViewVision jobs start.
                let tier4_pending = self
                    .db
                    .jobs
                    .pending_count_for_tier(cost_tier::RENDER_GPU)
                    .await
                    .unwrap_or(0);
                if tier4_pending > 0 {
                    debug!(
                        pending = tier4_pending,
                        gpu_concurrent, "Render GPU tier: draining"
                    );
                    let drained = self
                        .drain_tier(TierGroup::RenderGpu, gpu_concurrent, &excluded_archives)
                        .await;
                    if drained > 0 {
                        any_processed = true;
                    }
                }

                // Phase 5: Drain tier 3 (vision GPU) — per-frame/per-view
                // vision description jobs, also serialized by gpu_concurrent.
                let tier3_pending = self
                    .db
                    .jobs
                    .pending_count_for_tier(cost_tier::VISION_GPU)
                    .await
                    .unwrap_or(0);
                if tier3_pending > 0 {
                    // Proactively unload fast/standard models BEFORE loading the
                    // vision model. These may have been loaded by concurrent API
                    // calls (AI revision, concept tagging) outside the worker
                    // loop, so the after-drain unloads above may not have fired.
                    if let Some(ref fast) = self.fast_backend {
                        if let Err(e) = fast.unload().await {
                            let error_text = e.to_string();
                            warn!(
                                error_len = error_text.len(),
                                error_reason = worker_error_reason_code(&error_text),
                                "Pre-vision fast model unload failed"
                            );
                        }
                    }
                    if let Some(ref standard) = self.standard_backend {
                        if let Err(e) = standard.unload().await {
                            let error_text = e.to_string();
                            warn!(
                                error_len = error_text.len(),
                                error_reason = worker_error_reason_code(&error_text),
                                "Pre-vision standard model unload failed"
                            );
                        }
                    }

                    debug!(
                        pending = tier3_pending,
                        gpu_concurrent, "Vision GPU tier: draining"
                    );
                    let drained = self
                        .drain_tier(TierGroup::VisionGpu, gpu_concurrent, &excluded_archives)
                        .await;
                    if drained > 0 {
                        any_processed = true;
                        // Unload vision model — free VRAM for next cycle's models
                        if let Some(ref vision) = self.vision_backend {
                            if let Err(e) = vision.unload().await {
                                let error_text = e.to_string();
                                warn!(
                                    error_len = error_text.len(),
                                    error_reason = worker_error_reason_code(&error_text),
                                    "Vision model unload failed"
                                );
                            }
                        }
                    }
                }

                if !any_processed {
                    // All tiers drained — go back to sleep
                    break;
                }
                // Loop back: chained tier escalations may have queued new jobs
            }
        }

        let _ = self.event_tx.send(WorkerEvent::WorkerStopped);
        info!("Job worker stopped");
    }

    /// Drain all jobs for a specific tier group, returning the count processed.
    ///
    /// Jobs belonging to `excluded_archives` are skipped at the SQL level (Issue #466).
    async fn drain_tier(
        &self,
        tier_group: TierGroup,
        max_concurrent: usize,
        excluded_archives: &[String],
    ) -> usize {
        let mut total_drained = 0;

        loop {
            let mut claimed = 0;
            let mut tasks = tokio::task::JoinSet::new();

            for _ in 0..max_concurrent {
                match self.claim_job_for_tier(tier_group, excluded_archives).await {
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
                break;
            }

            debug!(
                tier_class = worker_tier_class(tier_group),
                claimed, "Processing tiered job batch"
            );
            while let Some(result) = tasks.join_next().await {
                if let Err(e) = result {
                    let error_text = e.to_string();
                    let (error_len, error_reason) = worker_failure_telemetry(&error_text);
                    error!(error_len, error_reason, "Job task panicked");
                }
            }
            total_drained += claimed;
        }

        total_drained
    }

    /// Claim the next available job for a specific tier group.
    ///
    /// Jobs belonging to `excluded_archives` are filtered out at the SQL level (Issue #466).
    async fn claim_job_for_tier(
        &self,
        tier_group: TierGroup,
        excluded_archives: &[String],
    ) -> Option<matric_core::Job> {
        let job_types: Vec<JobType> = {
            let handlers = self.handlers.read().await;
            handlers.keys().copied().collect()
        };

        let result = if excluded_archives.is_empty() {
            self.db
                .jobs
                .claim_next_for_tier(tier_group, &job_types)
                .await
        } else {
            self.db
                .jobs
                .claim_next_for_tier_excluding(tier_group, &job_types, excluded_archives)
                .await
        };

        match result {
            Ok(Some(job)) => Some(job),
            Ok(None) => None,
            Err(e) => {
                let error_text = e.to_string();
                let (error_len, error_reason) = worker_failure_telemetry(&error_text);
                error!(
                    error_len,
                    error_reason,
                    tier_class = worker_tier_class(tier_group),
                    "Failed to claim job for tier"
                );
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
            retry_policy: self.config.retry_policy,
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
    retry_policy: JobRetryPolicy,
}

impl JobWorkerRef {
    /// Execute a single claimed job.
    async fn execute_job(self, job: matric_core::Job) {
        let start = Instant::now();
        let job_id = job.id;
        let job_type = job.job_type;
        let retry_count = job.retry_count;
        let job_type_len = worker_job_type_len(&job_type);

        info!(job_id_present = true, job_type_len, "Processing job");

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
                let event_tx_for_ctx = self.event_tx.clone();
                let ctx = JobContext::new(job)
                    .with_progress_callback(move |percent, message| {
                        let _ = event_tx.send(WorkerEvent::JobProgress {
                            job_id,
                            percent,
                            message: message.map(String::from),
                        });
                    })
                    .with_event_tx(event_tx_for_ctx);

                let timeout_secs = matric_core::defaults::job_timeout_secs();
                let job_timeout = Duration::from_secs(timeout_secs);
                match tokio::time::timeout(job_timeout, handler.execute(ctx)).await {
                    Ok(result) => result,
                    Err(_) => {
                        warn!(
                            job_id_present = true,
                            job_type_len, timeout_secs, "Job exceeded timeout of {}s", timeout_secs
                        );
                        JobResult::Retry("job_timeout".to_string())
                    }
                }
            }
            None => {
                warn!(job_type_len, "No handler registered for job type");
                JobResult::Failed(format!("No handler for job type: {:?}", job_type))
            }
        };

        match result {
            JobResult::Success(result_data) => {
                if let Err(e) = self.db.jobs.complete(job_id, result_data).await {
                    let error_text = e.to_string();
                    let (error_len, error_reason) = worker_failure_telemetry(&error_text);
                    error!(
                        error_len,
                        error_reason,
                        job_id_present = true,
                        "Failed to mark job as completed"
                    );
                } else {
                    info!(
                        job_id_present = true,
                        job_type_len,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "Job completed successfully"
                    );
                    let _ = self
                        .event_tx
                        .send(WorkerEvent::JobCompleted { job_id, job_type });
                }
            }
            JobResult::Failed(error) => {
                let failure_code = worker_error_reason_code(&error);
                if let Err(e) = self
                    .db
                    .jobs
                    .fail(job_id, &error, JobFailureClass::Permanent, failure_code)
                    .await
                {
                    let error_text = e.to_string();
                    let (error_len, error_reason) = worker_failure_telemetry(&error_text);
                    error!(
                        error_len,
                        error_reason,
                        job_id_present = true,
                        "Failed to mark job as failed"
                    );
                } else {
                    let (error_len, error_reason) = worker_failure_telemetry(&error);
                    warn!(
                        job_id_present = true,
                        job_type_len,
                        error_len,
                        error_reason,
                        retry_scheduled = false,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "Job failed"
                    );
                    let _ = self.event_tx.send(WorkerEvent::JobFailed {
                        job_id,
                        job_type,
                        error,
                        failure_class: JobFailureClass::Permanent,
                        failure_code: failure_code.to_string(),
                    });
                }
            }
            JobResult::Retry(error) => {
                let failure_class = retry_failure_class(&error);
                let failure_code = worker_error_reason_code(&error);
                let delay = retry_delay(job_id, retry_count, failure_class, self.retry_policy);
                let retry_at = chrono::Utc::now()
                    + chrono::Duration::from_std(delay)
                        .expect("bounded job retry delay must fit chrono");
                match self
                    .db
                    .jobs
                    .retry(job_id, &error, failure_class, failure_code, retry_at)
                    .await
                {
                    Ok(JobRetryOutcome::Scheduled { next_attempt_at }) => {
                        warn!(
                            job_id_present = true,
                            job_type_len,
                            failure_class = failure_class.as_str(),
                            failure_code,
                            retry_delay_secs = delay.as_secs(),
                            next_attempt_at = %next_attempt_at,
                            duration_ms = start.elapsed().as_millis() as u64,
                            "Job retry scheduled"
                        );
                        let _ = self.event_tx.send(WorkerEvent::JobRetryScheduled {
                            job_id,
                            job_type,
                            failure_class,
                            failure_code: failure_code.to_string(),
                            next_attempt_at,
                            retry_count: retry_count + 1,
                        });
                    }
                    Ok(JobRetryOutcome::Exhausted) => {
                        warn!(
                            job_id_present = true,
                            job_type_len,
                            failure_class = failure_class.as_str(),
                            failure_code = "retry_exhausted",
                            duration_ms = start.elapsed().as_millis() as u64,
                            "Job retries exhausted"
                        );
                        let _ = self.event_tx.send(WorkerEvent::JobFailed {
                            job_id,
                            job_type,
                            error,
                            failure_class,
                            failure_code: "retry_exhausted".to_string(),
                        });
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        let (error_len, error_reason) = worker_failure_telemetry(&error_text);
                        error!(
                            error_len,
                            error_reason,
                            job_id_present = true,
                            "Failed to schedule job retry"
                        );
                    }
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
    pause_state: Option<PauseState>,
}

impl WorkerBuilder {
    /// Create a new worker builder.
    pub fn new(db: Database) -> Self {
        Self {
            db,
            config: WorkerConfig::default(),
            handlers: Vec::new(),
            extraction_registry: None,
            pause_state: None,
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

    /// Set the pause state manager (Issue #466).
    pub fn with_pause_state(mut self, pause_state: PauseState) -> Self {
        self.pause_state = Some(pause_state);
        self
    }

    /// Build and return the worker.
    pub async fn build(self) -> JobWorker {
        let mut worker = JobWorker::new(self.db, self.config, self.extraction_registry);

        if let Some(ps) = self.pause_state {
            worker.pause_state = Some(ps);
        }

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
        assert_eq!(config.max_concurrent_jobs, 1);
        assert!(config.enabled);
        assert_eq!(config.retry_policy, JobRetryPolicy::default());
    }

    #[test]
    fn test_worker_config_builder() {
        let config = WorkerConfig::default()
            .with_poll_interval(1000)
            .with_max_concurrent(8)
            .with_enabled(false)
            .with_retry_policy(JobRetryPolicy {
                transient_base_delay_ms: 10,
                rate_limit_base_delay_ms: 20,
                timeout_base_delay_ms: 30,
                stale_worker_base_delay_ms: 40,
                max_delay_ms: 100,
                jitter_percent: 0,
            });

        assert_eq!(config.poll_interval_ms, 1000);
        assert_eq!(config.max_concurrent_jobs, 8);
        assert!(!config.enabled);
        assert_eq!(config.retry_policy.max_delay_ms, 100);
    }

    #[test]
    fn retry_failure_classes_are_stable() {
        assert_eq!(
            retry_failure_class("provider returned 429 rate limit"),
            JobFailureClass::RateLimited
        );
        assert_eq!(retry_failure_class("job_timeout"), JobFailureClass::Timeout);
        assert_eq!(
            retry_failure_class("backend temporarily unavailable"),
            JobFailureClass::Transient
        );
    }

    #[test]
    fn retry_delay_is_bounded_exponential_and_deterministic() {
        let job_id = Uuid::parse_str("018f0000-0000-7000-8000-000000000001").unwrap();
        let policy = JobRetryPolicy::default();
        let first = retry_delay(job_id, 0, JobFailureClass::Transient, policy);
        assert!((5_000..=6_000).contains(&(first.as_millis() as u64)));
        assert_eq!(
            first,
            retry_delay(job_id, 0, JobFailureClass::Transient, policy)
        );

        for retry_count in 0..=5 {
            let delay = retry_delay(job_id, retry_count, JobFailureClass::Transient, policy);
            let base = 5_000_u64 * (1_u64 << retry_count);
            assert!((base..=base + (base / 5)).contains(&(delay.as_millis() as u64)));
        }

        assert_eq!(
            retry_delay(job_id, 30, JobFailureClass::RateLimited, policy).as_secs(),
            3_600,
        );
    }

    #[test]
    fn worker_config_values_fail_closed() {
        assert!(parse_bool_value("JOB_WORKER_ENABLED", "true").unwrap());
        assert!(!parse_bool_value("JOB_WORKER_ENABLED", "0").unwrap());
        assert!(parse_bool_value("JOB_WORKER_ENABLED", "flase").is_err());
        assert!(parse_bounded_u64_value("JOB_MAX_CONCURRENT", "0", 1, 64).is_err());
        assert!(parse_bounded_u64_value("JOB_MAX_CONCURRENT", "65", 1, 64).is_err());
        assert!(parse_bounded_u64_value("JOB_POLL_INTERVAL_MS", "99", 100, 300_000).is_err());
        assert!(parse_bounded_u64_value("JOB_POLL_INTERVAL_MS", "300001", 100, 300_000).is_err());
        assert!(parse_bounded_u64_value("JOB_RETRY_BASE_DELAY_MS", "99", 100, 600_000).is_err());
        assert!(parse_bounded_u64_value("JOB_RETRY_JITTER_PERCENT", "101", 0, 100).is_err());
    }

    #[test]
    fn worker_error_reason_code_uses_stable_classes() {
        assert_eq!(
            worker_error_reason_code(
                "Ollama model backend failed for /home/operator/mm_key_secret"
            ),
            "model_backend_error"
        );
        assert_eq!(
            worker_error_reason_code("postgres://user:secret@db/app sql failed"),
            "database_error"
        );
        assert_eq!(
            worker_error_reason_code("Cannot connect to backend"),
            "connection_failed"
        );
        assert_eq!(
            worker_error_reason_code("opaque backend text with token mm_key_secret"),
            "operation_failed"
        );
    }

    #[test]
    fn worker_failure_telemetry_reports_metadata_without_raw_error() {
        let raw_error =
            "postgres://user:pass@db.internal/app failed for /srv/private/mm_key_worker";

        let (error_len, error_reason) = worker_failure_telemetry(raw_error);
        let rendered = format!("error_len={error_len}; error_reason={error_reason}");

        assert_eq!(error_len, raw_error.len());
        assert_eq!(error_reason, "database_error");
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason=database_error"));

        for raw in [
            "postgres://user:pass",
            "db.internal",
            "/srv/private",
            "mm_key_worker",
        ] {
            assert!(!rendered.contains(raw), "raw value leaked: {raw}");
        }
    }

    #[test]
    fn worker_operational_telemetry_uses_presence_and_reason_fields() {
        let raw_job_id = Uuid::new_v4().to_string();
        let job_type = JobType::Embedding;
        let job_type_len = worker_job_type_len(&job_type);
        let raw_error =
            "panic for job /srv/private/mm_key_worker postgres://user:pass@db.internal/app";
        let (error_len, error_reason) = worker_failure_telemetry(raw_error);
        let rendered = format!(
            "job_id_present=true; job_type_len={job_type_len}; error_len={error_len}; error_reason={error_reason}"
        );

        assert!(rendered.contains("job_id_present=true"));
        assert!(rendered.contains("job_type_len="));
        assert!(rendered.contains("error_len="));
        assert!(rendered.contains("error_reason=database_error"));
        assert!(!rendered.contains(&raw_job_id));
        assert!(!rendered.contains("Embedding"));
        assert!(!rendered.contains("postgres://user:pass"));
        assert!(!rendered.contains("/srv/private"));
        assert!(!rendered.contains("mm_key_worker"));
    }

    #[test]
    fn worker_tier_telemetry_uses_stable_classes() {
        let rendered = format!(
            "tier_class={}; tier_class={}",
            worker_tier_class(TierGroup::CpuAndAgnostic),
            worker_tier_class(TierGroup::RenderGpu)
        );

        assert_eq!(worker_tier_class(TierGroup::CpuAndAgnostic), "cpu_agnostic");
        assert_eq!(worker_tier_class(TierGroup::AudioGpu), "audio_gpu");
        assert_eq!(worker_tier_class(TierGroup::FastGpu), "fast_gpu");
        assert_eq!(worker_tier_class(TierGroup::StandardGpu), "standard_gpu");
        assert_eq!(worker_tier_class(TierGroup::RenderGpu), "render_gpu");
        assert_eq!(worker_tier_class(TierGroup::VisionGpu), "vision_gpu");
        assert!(rendered.contains("tier_class=cpu_agnostic"));
        assert!(rendered.contains("tier_class=render_gpu"));
        assert!(!rendered.contains("CpuAndAgnostic"));
        assert!(!rendered.contains("RenderGpu"));
    }

    // ========== NEW COMPREHENSIVE TESTS ==========

    #[test]
    fn test_worker_config_default_values() {
        let config = WorkerConfig::default();
        assert_eq!(config.poll_interval_ms, 60_000); // DEFAULT_POLL_INTERVAL_MS (safety-net)
        assert_eq!(config.max_concurrent_jobs, 1);
        assert!(config.enabled);
    }

    #[test]
    fn test_worker_config_with_poll_interval() {
        let config = WorkerConfig::default().with_poll_interval(100);
        assert_eq!(config.poll_interval_ms, 100);
        // Ensure other defaults preserved
        assert_eq!(config.max_concurrent_jobs, 1);
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
        assert_eq!(config.poll_interval_ms, 60_000);
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
            failure_class: JobFailureClass::Permanent,
            failure_code: "operation_failed".to_string(),
        };

        match event {
            WorkerEvent::JobFailed {
                job_id: id,
                job_type,
                error,
                failure_class,
                failure_code,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(job_type, JobType::AiRevision);
                assert_eq!(error, "test error");
                assert_eq!(failure_class, JobFailureClass::Permanent);
                assert_eq!(failure_code, "operation_failed");
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
        assert!(debug_str.contains("job_type_len"));
        assert!(!debug_str.contains("Embedding"));
        assert!(debug_str.contains("job_id_set"));
        assert!(!debug_str.contains(&job_id.to_string()));
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

    #[test]
    fn test_worker_event_job_queued() {
        let job_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let event = WorkerEvent::JobQueued {
            job_id,
            job_type: JobType::Embedding,
            note_id: Some(note_id),
        };

        match event {
            WorkerEvent::JobQueued {
                job_id: id,
                job_type,
                note_id: nid,
            } => {
                assert_eq!(id, job_id);
                assert_eq!(job_type, JobType::Embedding);
                assert_eq!(nid, Some(note_id));
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn test_worker_event_job_queued_no_note() {
        let job_id = Uuid::new_v4();
        let event = WorkerEvent::JobQueued {
            job_id,
            job_type: JobType::GraphMaintenance,
            note_id: None,
        };

        match event {
            WorkerEvent::JobQueued {
                note_id: nid,
                job_type,
                ..
            } => {
                assert!(nid.is_none());
                assert_eq!(job_type, JobType::GraphMaintenance);
            }
            _ => panic!("Wrong event variant"),
        }
    }

    #[test]
    fn worker_event_debug_redacts_ids_progress_and_errors() {
        let job_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let progress_message =
            "processing customer@example.internal with token sk-live-secret in /srv/private";
        let failure_error =
            "postgres://user:secret@db.internal/app failed for /srv/private/mm_key_worker";

        let rendered = format!(
            "{:?}\n{:?}\n{:?}",
            WorkerEvent::JobQueued {
                job_id,
                job_type: JobType::Embedding,
                note_id: Some(note_id),
            },
            WorkerEvent::JobProgress {
                job_id,
                percent: 42,
                message: Some(progress_message.to_string()),
            },
            WorkerEvent::JobFailed {
                job_id,
                job_type: JobType::AiRevision,
                error: failure_error.to_string(),
                failure_class: JobFailureClass::Permanent,
                failure_code: "operation_failed".to_string(),
            }
        );

        for expected in [
            "JobQueued",
            "JobProgress",
            "JobFailed",
            "job_id_set",
            "note_id_set",
            "job_type_len",
            "message_len",
            "error_len",
            "failure_class",
            "failure_code",
            "operation_failed",
        ] {
            assert!(rendered.contains(expected), "missing field: {expected}");
        }

        for raw in [
            job_id.to_string(),
            note_id.to_string(),
            "Embedding".to_string(),
            "AiRevision".to_string(),
            "customer@example.internal".to_string(),
            "sk-live-secret".to_string(),
            "postgres://user:secret".to_string(),
            "db.internal".to_string(),
            "/srv/private".to_string(),
            "mm_key_worker".to_string(),
        ] {
            assert!(!rendered.contains(&raw), "raw value leaked: {raw}");
        }
    }
}
