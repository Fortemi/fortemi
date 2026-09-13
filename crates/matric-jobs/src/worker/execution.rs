use super::*;
use crate::handler::{HostedJobContext, HostedJobHandler};
use sqlx::PgPool;
use tokio::task::JoinSet;

/// Internal event identity cannot be supplied by a handler or payload.
#[derive(Clone)]
pub struct HostedWorkerEvent {
    tenant_id: Uuid,
    archive_schema: String,
    job_id: Uuid,
    attempt_id: Uuid,
    note_id: Option<Uuid>,
    job_type: JobType,
    kind: HostedWorkerEventKind,
}

#[derive(Clone)]
pub enum HostedWorkerEventKind {
    Started,
    Progress {
        percent: i32,
        message: Option<String>,
    },
    Completed {
        duration_ms: i64,
    },
    NoteUpdated {
        event: matric_core::ServerEvent,
    },
    Failed {
        failure_class: JobFailureClass,
        failure_code: &'static str,
    },
    RetryScheduled {
        failure_class: JobFailureClass,
        failure_code: &'static str,
        next_attempt_at: chrono::DateTime<chrono::Utc>,
        retry_count: i32,
    },
}

impl HostedWorkerEvent {
    fn new(claim: &HostedClaim, kind: HostedWorkerEventKind) -> Self {
        Self {
            tenant_id: claim.tenant_id(),
            archive_schema: claim.archive_schema().into(),
            job_id: claim.job().id,
            attempt_id: claim.attempt_id(),
            note_id: claim.job().note_id,
            job_type: claim.job().job_type,
            kind,
        }
    }
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    pub fn archive_schema(&self) -> &str {
        &self.archive_schema
    }
    pub fn job_id(&self) -> Uuid {
        self.job_id
    }
    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }
    pub fn note_id(&self) -> Option<Uuid> {
        self.note_id
    }
    pub fn job_type(&self) -> JobType {
        self.job_type
    }
    pub fn kind(&self) -> &HostedWorkerEventKind {
        &self.kind
    }
}

impl fmt::Debug for HostedWorkerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self.kind {
            HostedWorkerEventKind::Started => "started",
            HostedWorkerEventKind::Progress { .. } => "progress",
            HostedWorkerEventKind::Completed { .. } => "completed",
            HostedWorkerEventKind::NoteUpdated { .. } => "note_updated",
            HostedWorkerEventKind::Failed { .. } => "failed",
            HostedWorkerEventKind::RetryScheduled { .. } => "retry_scheduled",
        };
        f.debug_struct("HostedWorkerEvent")
            .field("stage", &stage)
            .field("archive_schema_len", &self.archive_schema.len())
            .field("note_id_set", &self.note_id.is_some())
            .finish_non_exhaustive()
    }
}

pub(super) struct HostedExecution {
    dispatcher: HostedJobDispatcher,
    handlers: HashMap<JobType, Arc<dyn HostedJobHandler>>,
    types: Vec<JobType>,
}

impl HostedExecution {
    pub(super) async fn new(
        pool: PgPool,
        handlers: Vec<Arc<dyn HostedJobHandler>>,
    ) -> Result<Self> {
        if handlers.is_empty() || handlers.len() > JobType::ALL.len() {
            return Err(Error::InvalidInput(
                "Hosted registry must be nonempty and bounded".into(),
            ));
        }
        let mut registry = HashMap::new();
        let mut types = Vec::new();
        for handler in handlers {
            let kind = handler.job_type();
            if registry.insert(kind, handler).is_some() {
                return Err(Error::InvalidInput("Duplicate hosted handler type".into()));
            }
            types.push(kind);
        }
        Ok(Self {
            dispatcher: HostedJobDispatcher::new(pool, HostedDispatchLimits::default()).await?,
            handlers: registry,
            types,
        })
    }

    pub(super) async fn run(&self, worker: &JobWorker, shutdown: &mut mpsc::Receiver<()>) {
        let notify = worker.db.jobs.job_notify();
        let poll = Duration::from_millis(worker.config.poll_interval_ms.max(100));
        let concurrency = worker.config.max_concurrent_jobs.clamp(1, 64);
        let tiers = [
            TierGroup::CpuAndAgnostic,
            TierGroup::AudioGpu,
            TierGroup::FastGpu,
            TierGroup::StandardGpu,
            TierGroup::RenderGpu,
            TierGroup::VisionGpu,
        ];
        let executor = HostedExecutor {
            events: worker.event_tx.clone(),
            timeout: worker.config.job_timeout,
            retry_policy: worker.config.retry_policy,
        };
        // Begin immediately; bounded registry pages continue without waiting for
        // the safety-net poll. No global count or legacy empty-means-all claim.
        loop {
            let mut more = false;
            let mut unavailable = false;
            for tier in tiers {
                let mut tier_more = false;
                let mut claimed_any = false;
                let mut tasks = JoinSet::new();
                let mut claims = HashMap::new();
                for _ in 0..concurrency {
                    if worker
                        .pause_state
                        .as_ref()
                        .is_some_and(PauseState::is_globally_paused)
                    {
                        break;
                    }
                    let excluded = match &worker.pause_state {
                        Some(pause) => pause.paused_archive_names().await,
                        None => Vec::new(),
                    };
                    let report = tokio::select! {
                        biased;
                        _ = shutdown.recv() => {
                            executor.cancel(&mut tasks, &mut claims).await;
                            return;
                        }
                        report = self.dispatcher.claim_next(tier, &self.types, &excluded) => report,
                    };
                    match report {
                        Ok(report) => {
                            unavailable |= report.failed_tenants != 0 || report.timed_out;
                            tier_more = !report.wrapped;
                            if let Some(claim) = report.claim {
                                claimed_any = true;
                                let handler = self.handlers[&claim.job().job_type].clone();
                                let task_claim = claim.clone();
                                let task_executor = executor.clone();
                                let handle = tasks.spawn(async move {
                                    task_executor.execute(task_claim, handler).await;
                                });
                                claims.insert(handle.id(), claim);
                            } else if report.wrapped {
                                break;
                            }
                        }
                        Err(_) => {
                            unavailable = true;
                            break;
                        }
                    }
                }
                while !tasks.is_empty() {
                    tokio::select! {
                        biased;
                        _ = shutdown.recv() => {
                            executor.cancel(&mut tasks, &mut claims).await;
                            return;
                        }
                        result = tasks.join_next_with_id() => match result {
                            Some(Ok((id, ()))) => { claims.remove(&id); }
                            Some(Err(error)) => {
                                if let Some(claim) = claims.remove(&error.id()) {
                                    executor.retry(&claim, JobFailureClass::Poison, "worker_task_lost").await;
                                }
                            }
                            None => break,
                        }
                    }
                }
                more |= tier_more || claimed_any;
            }
            if unavailable {
                warn!("Hosted dispatch incomplete; retrying after bounded backoff, not reporting an empty queue");
            }
            // A short yield also bounds polling under continuous inserts. Failures
            // use at least a one-second backoff, including partial-success passes.
            let delay = if unavailable {
                poll.max(Duration::from_secs(1))
            } else if more {
                Duration::from_millis(10)
            } else {
                poll
            };
            tokio::select! {
                _ = shutdown.recv() => return,
                _ = sleep(delay) => {},
                _ = notify.notified(), if !unavailable => {},
            }
        }
    }
}

#[derive(Clone)]
struct HostedExecutor {
    events: broadcast::Sender<WorkerEvent>,
    timeout: Duration,
    retry_policy: JobRetryPolicy,
}

impl HostedExecutor {
    fn emit(&self, claim: &HostedClaim, kind: HostedWorkerEventKind) {
        let _ = self
            .events
            .send(WorkerEvent::Hosted(HostedWorkerEvent::new(claim, kind)));
    }

    async fn execute(&self, claim: Arc<HostedClaim>, handler: Arc<dyn HostedJobHandler>) {
        let started = Instant::now();
        self.emit(&claim, HostedWorkerEventKind::Started);
        let progress_claim = claim.clone();
        let progress_executor = self.clone();
        let ctx = HostedJobContext::from_claim(claim.clone()).with_progress_callback(
            move |percent, message| {
                progress_executor.emit(
                    &progress_claim,
                    HostedWorkerEventKind::Progress {
                        percent,
                        message: message.map(str::to_owned),
                    },
                );
            },
        );
        let result = tokio::time::timeout(
            self.timeout,
            AssertUnwindSafe(handler.execute(ctx)).catch_unwind(),
        )
        .await;
        match result {
            Ok(Ok(JobResult::Success(value))) => {
                match claim.complete_with_note_event(value).await {
                    Ok(Some(event)) => {
                        self.emit(
                            &claim,
                            HostedWorkerEventKind::Completed {
                                duration_ms: started.elapsed().as_millis().min(i64::MAX as u128)
                                    as i64,
                            },
                        );
                        if let Some(event) = event {
                            self.emit(&claim, HostedWorkerEventKind::NoteUpdated { event });
                        }
                    }
                    Ok(None) => warn!("Hosted completion lost its attempt fence; no event emitted"),
                    Err(_) => warn!(
                        "Hosted completion unavailable; recovery retains settlement responsibility"
                    ),
                }
            }
            Ok(Ok(JobResult::Failed(error))) => {
                let code = worker_error_reason_code(&error);
                // Persist stable diagnostics only, never provider content.
                match claim.fail(code, JobFailureClass::Permanent, code).await {
                    Ok(true) => self.emit(
                        &claim,
                        HostedWorkerEventKind::Failed {
                            failure_class: JobFailureClass::Permanent,
                            failure_code: code,
                        },
                    ),
                    Ok(false) => warn!("Hosted failure lost its attempt fence; no event emitted"),
                    Err(_) => warn!(
                        "Hosted failure unavailable; recovery retains settlement responsibility"
                    ),
                }
            }
            Ok(Ok(JobResult::Retry(error))) => {
                self.retry(
                    &claim,
                    retry_failure_class(&error),
                    worker_error_reason_code(&error),
                )
                .await
            }
            Ok(Err(_)) => {
                self.retry(&claim, JobFailureClass::Poison, "handler_panicked")
                    .await
            }
            Err(_) => {
                self.retry(&claim, JobFailureClass::Timeout, "job_timeout")
                    .await
            }
        }
    }

    async fn retry(&self, claim: &HostedClaim, class: JobFailureClass, code: &'static str) {
        let delay = retry_delay(
            claim.job().id,
            claim.job().retry_count,
            class,
            self.retry_policy,
        );
        let at =
            chrono::Utc::now() + chrono::Duration::from_std(delay).expect("bounded retry delay");
        match claim.retry(code, class, code, at).await {
            Ok(Some(JobRetryOutcome::Scheduled { next_attempt_at })) => self.emit(
                claim,
                HostedWorkerEventKind::RetryScheduled {
                    failure_class: class,
                    failure_code: code,
                    next_attempt_at,
                    retry_count: claim.job().retry_count + 1,
                },
            ),
            Ok(Some(JobRetryOutcome::Exhausted)) => self.emit(
                claim,
                HostedWorkerEventKind::Failed {
                    failure_class: class,
                    failure_code: "retry_exhausted",
                },
            ),
            Ok(None) => warn!("Hosted retry lost its attempt fence; no event emitted"),
            Err(_) => warn!("Hosted retry unavailable; periodic recovery remains responsible"),
        }
    }

    async fn cancel(
        &self,
        tasks: &mut JoinSet<()>,
        claims: &mut HashMap<tokio::task::Id, Arc<HostedClaim>>,
    ) {
        tasks.abort_all();
        // Observe task exit/transaction drop before retrying. Keep only abnormal
        // exits: a task that completed before shutdown already settled itself.
        while let Some(result) = tasks.join_next_with_id().await {
            if let Ok((id, ())) = result {
                claims.remove(&id);
            }
        }
        let retries = futures::future::join_all(
            claims
                .values()
                .map(|claim| self.retry(claim, JobFailureClass::Transient, "worker_shutdown")),
        );
        if tokio::time::timeout(Duration::from_secs(5), retries)
            .await
            .is_err()
        {
            warn!("Hosted shutdown settlement deadline reached; stale recovery owns remaining attempts");
        }
        claims.clear();
    }
}
