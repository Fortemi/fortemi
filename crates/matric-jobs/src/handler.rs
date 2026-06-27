//! Job handlers for each job type.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::broadcast;
use uuid::Uuid;

use matric_core::{Job, JobType};

use std::{fmt, sync::Arc};

use crate::worker::WorkerEvent;

/// Progress callback type for job handlers.
pub type ProgressCallback = Arc<dyn Fn(i32, Option<&str>) + Send + Sync>;

/// Context provided to job handlers.
pub struct JobContext {
    /// The job being processed.
    pub job: Job,
    /// Progress callback for updating job progress.
    progress_callback: Option<ProgressCallback>,
    /// Event broadcast sender for emitting worker events (e.g. downstream job.queued).
    event_tx: Option<broadcast::Sender<WorkerEvent>>,
}

impl JobContext {
    /// Create a new job context.
    pub fn new(job: Job) -> Self {
        Self {
            job,
            progress_callback: None,
            event_tx: None,
        }
    }

    /// Set the progress callback.
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(i32, Option<&str>) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Arc::new(callback));
        self
    }

    /// Set the worker event broadcast sender.
    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<WorkerEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    /// Report progress to the callback.
    pub fn report_progress(&self, percent: i32, message: Option<&str>) {
        if let Some(ref callback) = self.progress_callback {
            callback(percent, message);
        }
    }

    /// Emit a `JobQueued` event for a downstream job queued by this handler.
    ///
    /// This bridges the gap where handler-initiated downstream jobs (e.g.
    /// extraction → embedding) were missing `job.queued` SSE events.
    pub fn emit_job_queued(&self, job_id: Uuid, job_type: JobType, note_id: Option<Uuid>) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkerEvent::JobQueued {
                job_id,
                job_type,
                note_id,
            });
        }
    }

    /// Get a clone of the progress callback Arc for passing to sub-components.
    pub fn progress_callback_arc(&self) -> Option<ProgressCallback> {
        self.progress_callback.clone()
    }

    /// Get the note ID for this job, if any.
    pub fn note_id(&self) -> Option<Uuid> {
        self.job.note_id
    }

    /// Get the job payload.
    pub fn payload(&self) -> Option<&JsonValue> {
        self.job.payload.as_ref()
    }
}

/// Result of job execution.
pub enum JobResult {
    /// Job completed successfully with optional result data.
    Success(Option<JsonValue>),
    /// Job failed with an error message.
    Failed(String),
    /// Job should be retried after a delay.
    Retry(String),
}

impl fmt::Debug for JobResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobResult::Success(result) => f
                .debug_struct("Success")
                .field("result_present", &result.is_some())
                .field("result_class", &result.as_ref().map(json_value_class))
                .field(
                    "result_serialized_len",
                    &result
                        .as_ref()
                        .and_then(|value| serde_json::to_string(value).ok())
                        .map(|serialized| serialized.len()),
                )
                .finish(),
            JobResult::Failed(error) => f
                .debug_struct("Failed")
                .field("error_len", &error.len())
                .finish_non_exhaustive(),
            JobResult::Retry(error) => f
                .debug_struct("Retry")
                .field("error_len", &error.len())
                .finish_non_exhaustive(),
        }
    }
}

fn json_value_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

/// Trait for job handlers.
#[async_trait]
pub trait JobHandler: Send + Sync {
    /// The job type this handler processes.
    fn job_type(&self) -> JobType;

    /// Execute the job.
    async fn execute(&self, ctx: JobContext) -> JobResult;

    /// Check if this handler can process the given job type.
    fn can_handle(&self, job_type: JobType) -> bool {
        self.job_type() == job_type
    }
}

/// No-op handler for testing.
pub struct NoOpHandler {
    job_type: JobType,
}

impl NoOpHandler {
    /// Create a new no-op handler for the given job type.
    pub fn new(job_type: JobType) -> Self {
        Self { job_type }
    }
}

#[async_trait]
impl JobHandler for NoOpHandler {
    fn job_type(&self) -> JobType {
        self.job_type
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        ctx.report_progress(50, Some("Processing..."));
        ctx.report_progress(100, Some("Done"));
        JobResult::Success(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_context_note_id() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(Uuid::new_v4()),
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job.clone());
        assert_eq!(ctx.note_id(), job.note_id);
    }

    #[tokio::test]
    async fn test_noop_handler() {
        let handler = NoOpHandler::new(JobType::Embedding);
        assert_eq!(handler.job_type(), JobType::Embedding);
        assert!(handler.can_handle(JobType::Embedding));
        assert!(!handler.can_handle(JobType::Linking));

        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        let result = handler.execute(ctx).await;
        assert!(matches!(result, JobResult::Success(_)));
    }

    // ========== NEW COMPREHENSIVE TESTS ==========

    #[test]
    fn test_job_context_new() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job.clone());
        assert_eq!(ctx.job.id, job.id);
        assert_eq!(ctx.job.job_type, job.job_type);
        assert!(ctx.progress_callback.is_none());
    }

    #[test]
    fn test_job_context_note_id_none() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert!(ctx.note_id().is_none());
    }

    #[test]
    fn test_job_context_note_id_some() {
        let note_id = Uuid::new_v4();
        let job = Job {
            id: Uuid::new_v4(),
            note_id: Some(note_id),
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert_eq!(ctx.note_id(), Some(note_id));
    }

    #[test]
    fn test_job_context_payload_none() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert!(ctx.payload().is_none());
    }

    #[test]
    fn test_job_context_payload_some() {
        use serde_json::json;

        let payload = json!({"key": "value", "count": 42});
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: Some(payload.clone()),
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        assert!(ctx.payload().is_some());
        assert_eq!(ctx.payload().unwrap()["key"], "value");
        assert_eq!(ctx.payload().unwrap()["count"], 42);
    }

    #[test]
    fn test_job_context_report_progress_no_callback() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        // Should not panic
        ctx.report_progress(50, Some("test"));
        ctx.report_progress(100, None);
    }

    #[test]
    fn test_job_context_with_progress_callback() {
        use std::sync::{Arc, Mutex};

        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let progress_log = Arc::new(Mutex::new(Vec::new()));
        let progress_log_clone = progress_log.clone();

        let ctx = JobContext::new(job).with_progress_callback(move |percent, message| {
            progress_log_clone
                .lock()
                .unwrap()
                .push((percent, message.map(String::from)));
        });

        ctx.report_progress(25, Some("Starting"));
        ctx.report_progress(50, Some("Halfway"));
        ctx.report_progress(100, None);

        let log = progress_log.lock().unwrap();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0], (25, Some("Starting".to_string())));
        assert_eq!(log[1], (50, Some("Halfway".to_string())));
        assert_eq!(log[2], (100, None));
    }

    #[test]
    fn test_noop_handler_job_type() {
        let handler = NoOpHandler::new(JobType::AiRevision);
        assert_eq!(handler.job_type(), JobType::AiRevision);
    }

    #[test]
    fn test_noop_handler_can_handle_same_type() {
        let handler = NoOpHandler::new(JobType::Linking);
        assert!(handler.can_handle(JobType::Linking));
    }

    #[test]
    fn test_noop_handler_can_handle_different_type() {
        let handler = NoOpHandler::new(JobType::Embedding);
        assert!(!handler.can_handle(JobType::AiRevision));
        assert!(!handler.can_handle(JobType::Linking));
        assert!(!handler.can_handle(JobType::ContextUpdate));
        assert!(!handler.can_handle(JobType::TitleGeneration));
    }

    #[tokio::test]
    async fn test_noop_handler_execute_with_progress() {
        use std::sync::{Arc, Mutex};

        let handler = NoOpHandler::new(JobType::Embedding);

        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let progress_log = Arc::new(Mutex::new(Vec::new()));
        let progress_log_clone = progress_log.clone();

        let ctx = JobContext::new(job).with_progress_callback(move |percent, message| {
            progress_log_clone
                .lock()
                .unwrap()
                .push((percent, message.map(String::from)));
        });

        let result = handler.execute(ctx).await;

        // Verify result is Success
        assert!(matches!(result, JobResult::Success(None)));

        // Verify progress was reported
        let log = progress_log.lock().unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0], (50, Some("Processing...".to_string())));
        assert_eq!(log[1], (100, Some("Done".to_string())));
    }

    #[tokio::test]
    async fn test_noop_handler_execute_different_job_types() {
        for job_type in [
            JobType::Embedding,
            JobType::AiRevision,
            JobType::Linking,
            JobType::ContextUpdate,
            JobType::TitleGeneration,
        ] {
            let handler = NoOpHandler::new(job_type);

            let job = Job {
                id: Uuid::new_v4(),
                note_id: None,
                job_type,
                status: matric_core::JobStatus::Pending,
                priority: 0,
                payload: None,
                result: None,
                error_message: None,
                progress_percent: 0,
                progress_message: None,
                retry_count: 0,
                max_retries: 3,
                created_at: chrono::Utc::now(),
                started_at: None,
                completed_at: None,
                cost_tier: None,
            };

            let ctx = JobContext::new(job);
            let result = handler.execute(ctx).await;
            assert!(matches!(result, JobResult::Success(None)));
        }
    }

    #[test]
    fn test_job_result_variants() {
        use serde_json::json;

        // Test Success with None
        let result1 = JobResult::Success(None);
        assert!(matches!(result1, JobResult::Success(None)));

        // Test Success with Some
        let result2 = JobResult::Success(Some(json!({"status": "ok"})));
        assert!(matches!(result2, JobResult::Success(Some(_))));

        // Test Failed
        let result3 = JobResult::Failed("error message".to_string());
        assert!(matches!(result3, JobResult::Failed(_)));

        // Test Retry
        let result4 = JobResult::Retry("retry reason".to_string());
        assert!(matches!(result4, JobResult::Retry(_)));
    }

    #[test]
    fn test_job_result_debug_redacts_success_payload() {
        use serde_json::json;

        let result = JobResult::Success(Some(json!({
            "transcript": "patient@example.com has token sk_live_sensitive",
            "path": "/home/user/private/research.pdf"
        })));

        let debug = format!("{result:?}");

        assert!(debug.contains("Success"));
        assert!(debug.contains("result_present: true"));
        assert!(debug.contains("result_class: Some(\"object\")"));
        assert!(debug.contains("result_serialized_len: Some("));
        assert!(!debug.contains("patient@example.com"));
        assert!(!debug.contains("sk_live_sensitive"));
        assert!(!debug.contains("/home/user/private/research.pdf"));
    }

    #[test]
    fn test_job_result_debug_redacts_error_text() {
        let failure = JobResult::Failed(
            "failed for postgres://user:pass@example.com/db and /tmp/private.wav".to_string(),
        );
        let retry =
            JobResult::Retry("retry after backend returned bearer token abc.def.ghi".to_string());

        let debug = format!("{failure:?} {retry:?}");

        assert!(debug.contains("Failed"));
        assert!(debug.contains("Retry"));
        assert!(debug.contains("error_len"));
        assert!(!debug.contains("postgres://"));
        assert!(!debug.contains("/tmp/private.wav"));
        assert!(!debug.contains("abc.def.ghi"));
    }

    #[test]
    fn test_job_context_preserves_all_job_fields() {
        use chrono::Utc;
        use serde_json::json;

        let job_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let created_at = Utc::now();
        let started_at = Utc::now();

        let job = Job {
            id: job_id,
            note_id: Some(note_id),
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Running,
            priority: 5,
            payload: Some(json!({"key": "value"})),
            result: Some(json!({"output": "data"})),
            error_message: Some("test error".to_string()),
            progress_percent: 50,
            progress_message: Some("halfway".to_string()),
            retry_count: 2,
            max_retries: 3,
            created_at,
            started_at: Some(started_at),
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job.clone());

        // Verify all fields are preserved
        assert_eq!(ctx.job.id, job_id);
        assert_eq!(ctx.job.note_id, Some(note_id));
        assert_eq!(ctx.job.job_type, JobType::Embedding);
        assert_eq!(ctx.job.priority, 5);
        assert_eq!(ctx.job.progress_percent, 50);
        assert_eq!(ctx.job.retry_count, 2);
        assert_eq!(ctx.job.max_retries, 3);
        assert!(ctx.job.payload.is_some());
        assert!(ctx.job.result.is_some());
        assert!(ctx.job.error_message.is_some());
        assert!(ctx.job.progress_message.is_some());
        assert_eq!(ctx.job.created_at, created_at);
        assert_eq!(ctx.job.started_at, Some(started_at));
    }

    #[test]
    fn test_job_context_emit_job_queued_no_event_tx() {
        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let ctx = JobContext::new(job);
        // Should not panic when event_tx is None
        ctx.emit_job_queued(Uuid::new_v4(), JobType::Embedding, Some(Uuid::new_v4()));
    }

    #[test]
    fn test_job_context_emit_job_queued_with_event_tx() {
        let (tx, mut rx) = broadcast::channel(16);

        let job = Job {
            id: Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Embedding,
            status: matric_core::JobStatus::Pending,
            priority: 0,
            payload: None,
            result: None,
            error_message: None,
            progress_percent: 0,
            progress_message: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            cost_tier: None,
        };

        let downstream_job_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();

        let ctx = JobContext::new(job).with_event_tx(tx);
        ctx.emit_job_queued(downstream_job_id, JobType::Linking, Some(note_id));

        let event = rx.try_recv().expect("Should receive JobQueued event");
        match event {
            WorkerEvent::JobQueued {
                job_id,
                job_type,
                note_id: nid,
            } => {
                assert_eq!(job_id, downstream_job_id);
                assert_eq!(job_type, JobType::Linking);
                assert_eq!(nid, Some(note_id));
            }
            _ => panic!("Expected JobQueued event"),
        }
    }
}
