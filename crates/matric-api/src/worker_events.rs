//! Worker events retain persisted tenant and canonical archive ownership.

use super::*;

/// Bridge WorkerEvent from the job worker to ServerEvent on the unified EventBus.
pub(super) async fn bridge_worker_events(
    mut worker_rx: tokio::sync::broadcast::Receiver<WorkerEvent>,
    event_bus: Arc<EventBus>,
    db: Database,
) {
    use matric_core::JobRepository;
    loop {
        let event = match worker_rx.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "Event bridge lagged, missed events");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        let job_id = match &event {
            WorkerEvent::Hosted(event) => {
                matric_api::hosted_jobs::emit_worker_event(&event_bus, event);
                continue;
            }
            WorkerEvent::JobStarted { job_id, .. }
            | WorkerEvent::JobProgress { job_id, .. }
            | WorkerEvent::JobCompleted { job_id, .. }
            | WorkerEvent::JobFailed { job_id, .. }
            | WorkerEvent::JobQueued { job_id, .. } => *job_id,
            WorkerEvent::JobRetryScheduled { .. }
            | WorkerEvent::WorkerStarted
            | WorkerEvent::WorkerStopped => continue,
        };
        let ownership = match event_scope::job_context(&db, job_id).await {
            Ok(Some(ownership)) => ownership,
            _ => {
                tracing::warn!("Worker event skipped: durable job ownership unavailable");
                continue;
            }
        };
        let Some(job) = db.jobs.get(job_id).await.ok().flatten() else {
            continue;
        };
        let note_id = job.note_id;
        let context = ownership.context;
        let payload = match event {
            WorkerEvent::JobStarted { job_type, .. } => ServerEvent::JobStarted {
                job_id,
                job_type: format!("{job_type:?}"),
                note_id,
            },
            WorkerEvent::JobProgress {
                percent, message, ..
            } => {
                if db
                    .jobs
                    .update_progress(job_id, percent, message.as_deref())
                    .await
                    .is_err()
                {
                    tracing::debug!(
                        detail = API_EVENT_DISPATCH_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "persist_job_progress",
                        "Failed to persist job progress"
                    );
                    continue;
                }
                ServerEvent::JobProgress {
                    job_id,
                    note_id,
                    progress: percent,
                    message,
                }
            }
            WorkerEvent::JobCompleted { job_type, .. } => {
                let duration_ms = job
                    .completed_at
                    .and_then(|c| job.started_at.map(|s| (c - s).num_milliseconds()));
                event_bus.emit_with_context(
                    ServerEvent::JobCompleted {
                        job_id,
                        job_type: format!("{job_type:?}"),
                        note_id,
                        duration_ms,
                    },
                    context.clone(),
                );
                // Every derivative retains the completed job's persisted owner.
                if let Some(nid) = note_id {
                    match job_type {
                        JobType::Embedding => event_bus.emit_with_context(
                            ServerEvent::IndexEmbeddingUpdated {
                                note_id: nid,
                                job_id: Some(job_id),
                            },
                            context.clone(),
                        ),
                        JobType::Linking => event_bus.emit_with_context(
                            ServerEvent::IndexLinkingUpdated {
                                note_id: nid,
                                job_id: Some(job_id),
                            },
                            context.clone(),
                        ),
                        _ => {}
                    }
                    // Read the note in the job's archive under its persisted
                    // tenant, including named archives in external deployments.
                    let note = async {
                        let mut scope =
                            matric_db::TenantScopedConn::begin(&db.pool, ownership.tenant).await?;
                        sqlx::query("SELECT set_config('search_path', $1, true)")
                            .bind(format!("{}, public", ownership.schema))
                            .execute(scope.executor())
                            .await?;
                        let owned: bool = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM note WHERE id=$1 AND tenant_id=$2)",
                        )
                        .bind(nid)
                        .bind(ownership.tenant)
                        .fetch_one(scope.executor())
                        .await?;
                        if !owned {
                            return Err(matric_core::Error::NotFound(
                                "Job note ownership is unavailable".into(),
                            ));
                        }
                        let notes = matric_db::PgNoteRepository::new(db.pool.clone());
                        let note = notes.fetch_tx(scope.executor(), nid).await?;
                        scope.commit().await?;
                        Ok::<_, matric_core::Error>(note)
                    }
                    .await;
                    if let Ok(note) = note {
                        event_bus.emit_with_context(
                            ServerEvent::NoteUpdated {
                                note_id: nid,
                                title: note.note.title.clone(),
                                tags: note.tags.clone(),
                                has_ai_content: note.revised.ai_generated_at.is_some(),
                                has_links: !note.links.is_empty(),
                            },
                            context,
                        );
                    }
                }
                continue;
            }
            WorkerEvent::JobFailed {
                job_type,
                failure_code,
                ..
            } => ServerEvent::JobFailed {
                job_id,
                job_type: format!("{job_type:?}"),
                note_id,
                error: failure_code,
            },
            WorkerEvent::JobQueued { job_type, .. } => ServerEvent::JobQueued {
                job_id,
                job_type: format!("{job_type:?}"),
                note_id,
            },
            _ => unreachable!("non-job events handled before ownership resolution"),
        };
        event_bus.emit_with_context(payload, context);
    }
}
