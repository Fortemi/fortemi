//! Fails attachment-backed background jobs closed when attachments are disabled (#1160).

use async_trait::async_trait;
use matric_jobs::{JobContext, JobHandler, JobResult, JobType};

use crate::attachments_switch::ATTACHMENTS_DISABLED_CODE;

/// Which jobs a gated handler refuses while attachments are disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentJobScope {
    /// Every job of this type operates on attachment bytes or derivatives.
    AttachmentOnly,
    /// Only jobs whose payload names an `attachment_id` (e.g. extraction).
    AttachmentPayload,
}

/// Wraps a job handler so that attachment work fails closed without touching storage.
pub struct AttachmentGatedHandler<H> {
    inner: H,
    attachments_enabled: bool,
    scope: AttachmentJobScope,
}

impl<H> AttachmentGatedHandler<H> {
    pub fn new(inner: H, attachments_enabled: bool, scope: AttachmentJobScope) -> Self {
        Self {
            inner,
            attachments_enabled,
            scope,
        }
    }

    fn refuses(&self, ctx: &JobContext) -> bool {
        if self.attachments_enabled {
            return false;
        }
        match self.scope {
            AttachmentJobScope::AttachmentOnly => true,
            AttachmentJobScope::AttachmentPayload => ctx
                .payload()
                .and_then(|payload| payload.get("attachment_id"))
                .is_some_and(|value| !value.is_null()),
        }
    }
}

#[async_trait]
impl<H: JobHandler> JobHandler for AttachmentGatedHandler<H> {
    fn job_type(&self) -> JobType {
        self.inner.job_type()
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        if self.refuses(&ctx) {
            return JobResult::Failed(ATTACHMENTS_DISABLED_CODE.to_string());
        }
        self.inner.execute(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_jobs::{Job, JobStatus};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingHandler(Arc<AtomicUsize>);

    #[async_trait]
    impl JobHandler for CountingHandler {
        fn job_type(&self) -> JobType {
            JobType::Extraction
        }

        async fn execute(&self, _ctx: JobContext) -> JobResult {
            self.0.fetch_add(1, Ordering::SeqCst);
            JobResult::Success(None)
        }
    }

    fn ctx(payload: Option<serde_json::Value>) -> JobContext {
        JobContext::new(Job {
            id: uuid::Uuid::new_v4(),
            note_id: None,
            job_type: JobType::Extraction,
            status: JobStatus::Running,
            priority: 0,
            payload,
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
        })
    }

    fn gated(
        enabled: bool,
        scope: AttachmentJobScope,
    ) -> (AttachmentGatedHandler<CountingHandler>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            AttachmentGatedHandler::new(CountingHandler(calls.clone()), enabled, scope),
            calls,
        )
    }

    #[tokio::test]
    async fn disabled_attachment_payload_jobs_fail_closed_without_running() {
        let (handler, calls) = gated(false, AttachmentJobScope::AttachmentPayload);
        let result = handler
            .execute(ctx(Some(
                json!({ "attachment_id": uuid::Uuid::new_v4().to_string() }),
            )))
            .await;
        assert!(matches!(result, JobResult::Failed(ref code) if code == ATTACHMENTS_DISABLED_CODE));
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // Inline (non-attachment) extraction still runs.
        let result = handler
            .execute(ctx(Some(json!({ "strategy": "text_native" }))))
            .await;
        assert!(matches!(result, JobResult::Success(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn disabled_attachment_only_jobs_always_fail_closed() {
        let (handler, calls) = gated(false, AttachmentJobScope::AttachmentOnly);
        let result = handler.execute(ctx(None)).await;
        assert!(matches!(result, JobResult::Failed(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(handler.job_type(), JobType::Extraction);
    }

    #[tokio::test]
    async fn enabled_gate_is_transparent() {
        let (handler, calls) = gated(true, AttachmentJobScope::AttachmentOnly);
        let result = handler
            .execute(ctx(Some(
                json!({ "attachment_id": uuid::Uuid::new_v4().to_string() }),
            )))
            .await;
        assert!(matches!(result, JobResult::Success(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
