//! Job handlers for each job type.

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use matric_core::{Job, JobType};

/// Progress callback type for job handlers.
pub type ProgressCallback = Box<dyn Fn(i32, Option<&str>) + Send + Sync>;

/// Context provided to job handlers.
pub struct JobContext {
    /// The job being processed.
    pub job: Job,
    /// Progress callback for updating job progress.
    progress_callback: Option<ProgressCallback>,
}

impl JobContext {
    /// Create a new job context.
    pub fn new(job: Job) -> Self {
        Self {
            job,
            progress_callback: None,
        }
    }

    /// Set the progress callback.
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(i32, Option<&str>) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Report progress to the callback.
    pub fn report_progress(&self, percent: i32, message: Option<&str>) {
        if let Some(ref callback) = self.progress_callback {
            callback(percent, message);
        }
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
#[derive(Debug)]
pub enum JobResult {
    /// Job completed successfully with optional result data.
    Success(Option<JsonValue>),
    /// Job failed with an error message.
    Failed(String),
    /// Job should be retried after a delay.
    Retry(String),
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
        };

        let ctx = JobContext::new(job);
        let result = handler.execute(ctx).await;
        assert!(matches!(result, JobResult::Success(_)));
    }
}
