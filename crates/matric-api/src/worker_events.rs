//! Trusted archive attribution for personal worker and enqueue events.

use super::*;

/// Resolve the persisted execution schema against the archive registry. Missing
/// payload schemas mean public, matching personal worker execution. Resolution
/// failure must never turn an archive event into system-wide monitoring data.
async fn context_for_job(db: &Database, job_id: Uuid) -> Option<EventContext> {
    let job = db.jobs.get(job_id).await.ok()??;
    let schema = job
        .payload
        .as_ref()
        .and_then(|payload| payload.get("schema"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("public");
    let archive = sqlx::query_as::<_, (String, String)>(
        "SELECT name, schema_name FROM public.archive_registry WHERE schema_name = $1",
    )
    .bind(schema)
    .fetch_optional(&db.pool)
    .await
    .ok()?;
    let (name, schema) = match archive {
        Some(archive) => archive,
        None if schema == "public" => ("public".into(), "public".into()),
        None => return None,
    };
    Some(EventContext {
        memory: Some(name.clone()),
        archive_name: Some(name),
        archive_schema: Some(schema),
        correlation_id: Some(job_id),
        ..Default::default()
    })
}

pub(super) async fn emit_job_event(
    bus: &EventBus,
    db: &Database,
    job_id: Uuid,
    event: ServerEvent,
) {
    if let Some(context) = context_for_job(db, job_id).await {
        bus.emit_with_context(event, context);
    }
}

/// Bridge WorkerEvent from the job worker to ServerEvent on the unified EventBus.
pub(super) async fn bridge_worker_events(
    mut worker_rx: tokio::sync::broadcast::Receiver<WorkerEvent>,
    event_bus: Arc<EventBus>,
    db: Database,
) {
    use matric_core::JobRepository;
    loop {
        match worker_rx.recv().await {
            Ok(event) => {
                let context = match &event {
                    WorkerEvent::Hosted(event) => {
                        matric_api::hosted_jobs::emit_worker_event(&event_bus, event);
                        continue;
                    }
                    WorkerEvent::JobStarted { job_id, .. }
                    | WorkerEvent::JobProgress { job_id, .. }
                    | WorkerEvent::JobCompleted { job_id, .. }
                    | WorkerEvent::JobFailed { job_id, .. }
                    | WorkerEvent::JobQueued { job_id, .. } => {
                        let Some(context) = context_for_job(&db, *job_id).await else {
                            continue;
                        };
                        context
                    }
                    _ => continue,
                };
                let server_event = match event {
                    WorkerEvent::Hosted(event) => {
                        matric_api::hosted_jobs::emit_worker_event(&event_bus, &event);
                        continue;
                    }
                    WorkerEvent::JobStarted { job_id, job_type } => {
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobStarted {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                        }
                    }
                    WorkerEvent::JobProgress {
                        job_id,
                        percent,
                        message,
                    } => {
                        // Persist progress to database so API queries reflect it
                        if let Err(e) = db
                            .jobs
                            .update_progress(job_id, percent, message.as_deref())
                            .await
                        {
                            tracing::debug!(
                                error_len = telemetry_text_len(&e.to_string()),
                                detail = API_EVENT_DISPATCH_DIAGNOSTIC_FAILURE_DETAIL,
                                percent,
                                operation = "persist_job_progress",
                                "Failed to persist job progress"
                            );
                        }
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobProgress {
                            job_id,
                            note_id,
                            progress: percent,
                            message,
                        }
                    }
                    WorkerEvent::JobCompleted { job_id, job_type } => {
                        let job = db.jobs.get(job_id).await.ok().flatten();
                        let note_id = job.as_ref().and_then(|j| j.note_id);
                        let duration_ms = job.as_ref().and_then(|j| {
                            j.completed_at
                                .and_then(|c| j.started_at.map(|s| (c - s).num_milliseconds()))
                        });

                        let evt = ServerEvent::JobCompleted {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                            duration_ms,
                        };
                        event_bus.emit_with_context(evt, context.clone());

                        // Emit index materialization events (Issue #464)
                        match job_type {
                            JobType::Embedding => {
                                if let Some(nid) = note_id {
                                    event_bus.emit_with_context(
                                        ServerEvent::IndexEmbeddingUpdated {
                                            note_id: nid,
                                            job_id: Some(job_id),
                                        },
                                        context.clone(),
                                    );
                                }
                            }
                            JobType::Linking => {
                                if let Some(nid) = note_id {
                                    event_bus.emit_with_context(
                                        ServerEvent::IndexLinkingUpdated {
                                            note_id: nid,
                                            job_id: Some(job_id),
                                        },
                                        context.clone(),
                                    );
                                }
                            }
                            _ => {}
                        }

                        // Also emit NoteUpdated if this was a note-related job (Issue #41)
                        if let Some(nid) = note_id {
                            let notes = matric_db::PgNoteRepository::new(db.pool.clone());
                            let schema = db
                                .for_schema(context.archive_schema.as_deref().unwrap_or("public"));
                            if let Ok(schema) = schema {
                                if let Ok(note) = schema
                                    .query(move |tx| {
                                        Box::pin(async move { notes.fetch_tx(tx, nid).await })
                                    })
                                    .await
                                {
                                    event_bus.emit_with_context(
                                        ServerEvent::NoteUpdated {
                                            note_id: nid,
                                            title: note.note.title.clone(),
                                            tags: note.tags.clone(),
                                            has_ai_content: note.revised.ai_generated_at.is_some(),
                                            has_links: !note.links.is_empty(),
                                        },
                                        context.clone(),
                                    );
                                }
                            }
                        }
                        continue; // Already emitted above
                    }
                    WorkerEvent::JobFailed {
                        job_id,
                        job_type,
                        failure_code,
                        ..
                    } => {
                        let note_id = db
                            .jobs
                            .get(job_id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|j| j.note_id);
                        ServerEvent::JobFailed {
                            job_id,
                            job_type: format!("{:?}", job_type),
                            note_id,
                            error: failure_code,
                        }
                    }
                    WorkerEvent::JobRetryScheduled { .. } => continue,
                    WorkerEvent::JobQueued {
                        job_id,
                        job_type,
                        note_id,
                    } => ServerEvent::JobQueued {
                        job_id,
                        job_type: format!("{:?}", job_type),
                        note_id,
                    },
                    WorkerEvent::WorkerStarted | WorkerEvent::WorkerStopped => continue,
                };
                event_bus.emit_with_context(server_event, context);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "Event bridge lagged, missed events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("Worker event channel closed, bridge stopping");
                break;
            }
        }
    }
}
