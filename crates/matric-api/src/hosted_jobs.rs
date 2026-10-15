//! Production handlers for the explicit hosted worker contract.
//! Runtime DML uses fenced claims; index DDL uses a narrow administrative capability.
//! Only explicitly migrated handlers are registered in hosted mode.

use async_trait::async_trait;
use matric_core::JobType;
use matric_db::PgDocumentTypeRepository;
use matric_jobs::handler::hosted_database_failure;
use matric_jobs::{HostedJobContext, HostedJobHandler, JobResult};

pub struct HostedDocumentTypeInferenceHandler;

pub struct HostedBuildVectorIndexHandler {
    administrative: matric_db::administrative::AdministrativeExecutor,
}

impl HostedBuildVectorIndexHandler {
    pub fn new(administrative: matric_db::administrative::AdministrativeExecutor) -> Self {
        Self { administrative }
    }
}

#[async_trait]
impl HostedJobHandler for HostedBuildVectorIndexHandler {
    fn job_type(&self) -> JobType {
        JobType::BuildSetIndex
    }

    async fn execute(&self, ctx: HostedJobContext) -> JobResult {
        let Some(payload) = ctx.payload().cloned().and_then(|value| {
            serde_json::from_value::<matric_db::vector_index::VectorIndexJobPayload>(value).ok()
        }) else {
            return JobResult::Failed("Invalid vector index job payload".into());
        };
        if payload.schema != ctx.archive_schema() {
            return JobResult::Failed("Index job archive differs from its authorized claim".into());
        }
        let shape = match matric_db::vector_index::VectorIndexShape::from_payload(payload.clone()) {
            Ok(shape) => shape,
            Err(error) => return hosted_database_failure(&error),
        };
        let building = shape.clone();
        match ctx
            .with_content(move |scope| {
                Box::pin(async move {
                    matric_db::vector_index::set_shape_status_scoped(scope, &building, "building")
                        .await
                })
            })
            .await
        {
            Ok(Some(())) => {}
            Ok(None) => return JobResult::Failed("Vector index claim unavailable".into()),
            Err(error) => return hosted_database_failure(&error),
        }
        let result = self
            .administrative
            .maintain_vector_indexes(ctx.tenant_id(), payload)
            .await;
        let status = if result.is_ok() { "ready" } else { "stale" };
        match ctx
            .with_content(move |scope| {
                Box::pin(async move {
                    matric_db::vector_index::set_shape_status_scoped(scope, &shape, status).await
                })
            })
            .await
        {
            Ok(Some(())) => {}
            Ok(None) => return JobResult::Failed("Vector index claim unavailable".into()),
            Err(error) => return hosted_database_failure(&error),
        }
        match result {
            Ok(()) => JobResult::Success(Some(serde_json::json!({"schema":ctx.archive_schema()}))),
            Err(error) => hosted_database_failure(&error),
        }
    }
}

#[async_trait]
impl HostedJobHandler for HostedDocumentTypeInferenceHandler {
    fn job_type(&self) -> JobType {
        JobType::DocumentTypeInference
    }

    async fn execute(&self, ctx: HostedJobContext) -> JobResult {
        let Some(note_id) = ctx.note_id() else {
            return JobResult::Failed("No note_id provided".into());
        };
        match ctx.report_progress(10, Some("Fetching note...")).await {
            Ok(true) => {}
            Ok(false) => {
                return JobResult::Failed("Document type inference claim unavailable".into())
            }
            Err(error) => return hosted_database_failure(&error),
        }
        let job_id = ctx.job_id();
        let outcome = ctx
            .with_content(move |scope| {
                Box::pin(async move {
                    PgDocumentTypeRepository::infer_note_scoped(scope, job_id, note_id).await
                })
            })
            .await;
        match outcome {
            Ok(Some(result)) => {
                match ctx
                    .report_progress(100, Some("Document type inference complete"))
                    .await
                {
                    Ok(true) => JobResult::Success(Some(result)),
                    Ok(false) => {
                        JobResult::Failed("Document type inference claim unavailable".into())
                    }
                    // Content is durable; replacement attempts replay its receipt.
                    Err(error) => hosted_database_failure(&error),
                }
            }
            Ok(None) => JobResult::Failed("Document type inference claim unavailable".into()),
            Err(error) => hosted_database_failure(&error),
        }
    }
}
/// Bridge committed hosted lifecycle events without queue lookups or progress
/// writes. Existing wire payloads retain explicit tenant/archive envelope scope.
pub fn emit_worker_event(
    bus: &matric_core::EventBus,
    event: &matric_jobs::worker::HostedWorkerEvent,
) {
    use matric_core::{EventContext, ServerEvent};
    use matric_jobs::worker::HostedWorkerEventKind;
    let payload = match event.kind() {
        HostedWorkerEventKind::Started => ServerEvent::JobStarted {
            job_id: event.job_id(),
            job_type: format!("{:?}", event.job_type()),
            note_id: event.note_id(),
        },
        HostedWorkerEventKind::Progress { percent, message } => ServerEvent::JobProgress {
            job_id: event.job_id(),
            note_id: event.note_id(),
            progress: *percent,
            message: message.clone(),
        },
        HostedWorkerEventKind::Completed { duration_ms } => ServerEvent::JobCompleted {
            job_id: event.job_id(),
            job_type: format!("{:?}", event.job_type()),
            note_id: event.note_id(),
            duration_ms: Some(*duration_ms),
        },
        HostedWorkerEventKind::NoteUpdated { event } => event.clone(),
        HostedWorkerEventKind::Failed { failure_code, .. } => ServerEvent::JobFailed {
            job_id: event.job_id(),
            job_type: format!("{:?}", event.job_type()),
            note_id: event.note_id(),
            error: (*failure_code).into(),
        },
        // No retry event exists in the pinned public catalog. Keep the durable
        // internal receipt without inventing a terminal public failure.
        HostedWorkerEventKind::RetryScheduled { .. } => return,
    };
    bus.emit_with_context(
        payload,
        EventContext {
            tenant_id: Some(event.tenant_id().to_string()),
            memory: Some(event.archive_schema().into()),
            archive_schema: Some(event.archive_schema().into()),
            correlation_id: Some(event.job_id()),
            ..EventContext::default()
        },
    );
}
