use super::*;
use matric_core::{Error, JobFailureClass, ServerEvent};
use matric_db::PgNoteRepository;
use matric_jobs::worker::HostedWorkerEventKind;
use matric_jobs::{JobWorker, WorkerConfig, WorkerEvent};

pub async fn verify_completion_failure(
    admin: &PgPool,
    runtime: &PgPool,
    tenant: Uuid,
    schema: &str,
) {
    // A failed COMMIT may discard its SQLx connection. Keep the caller's
    // same-connection restoration probe independent of this destructive case.
    let failure_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*runtime.connect_options()).clone())
        .await
        .unwrap();
    let note = Uuid::new_v4();
    seed(admin, schema, tenant, note, "MATCH SPEC", "", None).await;
    seed_metadata(admin, schema, tenant, note).await;
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'document_type_inference',0,$4)")
        .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).execute(admin).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION public.reject_note_completion() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'owned completion failure' USING ERRCODE='23514'; END$$; CREATE CONSTRAINT TRIGGER reject_note_completion AFTER INSERT ON public.job_history DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.reject_note_completion();")
        .execute(admin).await.unwrap();
    let worker = JobWorker::new(
        Database::new(failure_pool.clone()),
        WorkerConfig::default().with_poll_interval(100),
        None,
    )
    .with_hosted_handlers(vec![Arc::new(HostedDocumentTypeInferenceHandler)])
    .await
    .unwrap();
    let mut events = worker.events();
    let handle = worker.start();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let WorkerEvent::Hosted(event) = events.recv().await.unwrap() {
                assert_eq!(event.job_id(), job);
                if matches!(
                    event.kind(),
                    HostedWorkerEventKind::Progress { percent: 100, .. }
                ) {
                    break;
                }
                assert!(!matches!(
                    event.kind(),
                    HostedWorkerEventKind::Completed { .. }
                        | HostedWorkerEventKind::NoteUpdated { .. }
                ));
            }
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        events.try_recv().is_err(),
        "failed completion must not publish either success event"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status::text FROM public.job_queue WHERE id=$1")
            .bind(job)
            .fetch_one(admin)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(state(admin, schema, note).await["access"], json!(1));
    assert_eq!(state(admin, schema, note).await["activities"], json!(1));
    handle.shutdown().await.unwrap();
    sqlx::raw_sql("DROP TRIGGER reject_note_completion ON public.job_history; DROP FUNCTION public.reject_note_completion()").execute(admin).await.unwrap();
    let mut connection = failure_pool.acquire().await.unwrap();
    let path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let caller_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(runtime)
        .await
        .unwrap();
    assert_eq!(path, caller_path);
    let tenant_setting: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant',true)")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    assert!(tenant_setting.as_deref().unwrap_or("").is_empty());
    drop(connection);
    failure_pool.close().await;
    println!("hosted_note_completion: deferred history commit failure publishes neither completed nor note.updated; durable content retained for recovery");
}

pub async fn seed_metadata(admin: &PgPool, schema: &str, tenant: Uuid, note: Uuid) {
    let mut tx = admin.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema},public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE note SET title=$2 WHERE id=$1")
        .bind(note)
        .bind(format!("owned-title-{schema}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let tag = format!("owned-tag-{schema}");
    sqlx::query("INSERT INTO tag(name,created_at_utc) VALUES($1,NOW()) ON CONFLICT DO NOTHING")
        .bind(&tag)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO note_tag(note_id,tag_name) VALUES($1,$2)")
        .bind(note)
        .bind(tag)
        .execute(&mut *tx)
        .await
        .unwrap();
    if schema != "public" {
        let revision = Uuid::new_v4();
        sqlx::query("INSERT INTO note_revision(id,note_id,revision_number,content,created_at_utc,ai_generated_at) VALUES($1,$2,1,'owned revised',NOW(),NOW())")
            .bind(revision).bind(note).execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE note_revised_current SET last_revision_id=$2 WHERE note_id=$1")
            .bind(note)
            .bind(revision)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO link(id,from_note_id,to_url,kind,score,created_at_utc) VALUES($1,$2,'https://example.invalid/owned','manual',1,NOW())")
            .bind(Uuid::new_v4()).bind(note).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}

pub fn assert_metadata(event: &ServerEvent, schema: &str, note: Uuid) {
    let ServerEvent::NoteUpdated {
        note_id,
        title,
        tags,
        has_ai_content,
        has_links,
    } = event
    else {
        panic!("wrong snapshot kind")
    };
    assert_eq!(*note_id, note);
    assert_eq!(
        title.as_deref(),
        Some(format!("owned-title-{schema}").as_str())
    );
    assert_eq!(*tags, vec![format!("owned-tag-{schema}")]);
    assert_eq!(*has_ai_content, schema != "public");
    assert_eq!(*has_links, schema != "public");
}

async fn snapshot(
    claim: &Arc<HostedClaim>,
    note: Uuid,
) -> matric_core::Result<Option<Option<ServerEvent>>> {
    claim
        .with_content(move |scope| Box::pin(PgNoteRepository::updated_event_scoped(scope, note)))
        .await
}

pub async fn verify_snapshot(
    admin: &PgPool,
    runtime: &PgPool,
    tenant: Uuid,
    schema: &str,
    foreign: Uuid,
) {
    let note = Uuid::new_v4();
    seed(admin, schema, tenant, note, "MATCH SPEC", "", None).await;
    seed_metadata(admin, schema, tenant, note).await;
    let claim = mint(admin, runtime, tenant, schema, note).await;
    let before = state(admin, schema, note).await;
    assert_metadata(
        &snapshot(&claim, note).await.unwrap().unwrap().unwrap(),
        schema,
        note,
    );
    assert_eq!(
        state(admin, schema, note).await,
        before,
        "snapshot is read-only"
    );
    assert!(snapshot(&claim, foreign).await.unwrap().unwrap().is_none());
    assert!(snapshot(&claim, Uuid::new_v4())
        .await
        .unwrap()
        .unwrap()
        .is_none());
    sqlx::query(&format!(
        "UPDATE {schema}.note SET deleted_at=NOW() WHERE id=$1"
    ))
    .bind(note)
    .execute(admin)
    .await
    .unwrap();
    assert!(snapshot(&claim, note).await.unwrap().unwrap().is_none());
    sqlx::query(&format!(
        "UPDATE {schema}.note SET deleted_at=NULL,title=repeat('x',8193) WHERE id=$1"
    ))
    .bind(note)
    .execute(admin)
    .await
    .unwrap();
    assert!(matches!(
        snapshot(&claim, note).await,
        Err(Error::InvalidInput(_))
    ));
    sqlx::query(&format!(
        "UPDATE {schema}.note SET title=repeat('x',8192) WHERE id=$1"
    ))
    .bind(note)
    .execute(admin)
    .await
    .unwrap();
    assert!(snapshot(&claim, note).await.unwrap().unwrap().is_some());
    // Count, per-tag byte and aggregate byte limits reject without event truncation.
    for (count, width) in [(1025, 8), (1, 1025), (65, 1024)] {
        sqlx::query(&format!("DELETE FROM {schema}.note_tag WHERE note_id=$1"))
            .bind(note)
            .execute(admin)
            .await
            .unwrap();
        let mut tx = admin.begin().await.unwrap();
        sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
            .bind(tenant.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(&format!("INSERT INTO {schema}.tag(name,created_at_utc) SELECT lpad(i::text,$2,'x'),NOW() FROM generate_series(1,$1) i ON CONFLICT DO NOTHING"))
            .bind(count).bind(width).execute(&mut *tx).await.unwrap();
        sqlx::query(&format!("INSERT INTO {schema}.note_tag(note_id,tag_name) SELECT $3,lpad(i::text,$2,'x') FROM generate_series(1,$1) i"))
            .bind(count).bind(width).bind(note).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        assert!(matches!(
            snapshot(&claim, note).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(claim.complete(None).await.unwrap());
    assert!(
        snapshot(&claim, note).await.unwrap().is_none(),
        "stale attempt cannot read metadata"
    );
    println!("hosted_note_snapshot: tenant/archive; metadata and AI/link flags; read-only; missing/deleted/stale; title/tag count/byte bounds");
}

pub async fn verify_retries(admin: &PgPool, runtime: &PgPool, tenant: Uuid, schema: &str) {
    // SQL errors come from the actual progress transaction, not an error-text mock.
    for (percent, sqlstate, max_retries) in [
        (10, "40001", 3),
        (100, "57014", 3),
        (10, "23514", 3),
        (10, "40001", 0),
    ] {
        let note = Uuid::new_v4();
        seed(admin, schema, tenant, note, "MATCH SPEC", "", None).await;
        let job = Uuid::new_v4();
        sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload,max_retries) VALUES($1,$2,$3,'document_type_inference',0,$4,$5)")
            .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).bind(max_retries).execute(admin).await.unwrap();
        sqlx::raw_sql(&format!("CREATE FUNCTION public.reject_owned_progress() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.id='{job}' AND NEW.progress_percent={percent} THEN RAISE EXCEPTION 'owned secret-looking fixture text' USING ERRCODE='{sqlstate}'; END IF; RETURN NEW; END$$; CREATE TRIGGER reject_owned_progress BEFORE UPDATE OF progress_percent ON public.job_queue FOR EACH ROW EXECUTE FUNCTION public.reject_owned_progress();"))
            .execute(admin).await.unwrap();
        let worker = JobWorker::new(
            Database::new(runtime.clone()),
            WorkerConfig::default().with_poll_interval(100),
            None,
        )
        .with_hosted_handlers(vec![Arc::new(HostedDocumentTypeInferenceHandler)])
        .await
        .unwrap();
        let mut events = worker.events();
        let handle = worker.start();
        let retryable = sqlstate != "23514" && max_retries > 0;
        let mut first_attempt = None;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = events.recv().await.unwrap();
                let WorkerEvent::Hosted(event) = event else {
                    continue;
                };
                assert_eq!(event.job_id(), job);
                first_attempt = Some(event.attempt_id());
                match event.kind() {
                    HostedWorkerEventKind::RetryScheduled {
                        failure_class,
                        retry_count,
                        failure_code,
                        ..
                    } => {
                        assert!(retryable);
                        assert_eq!(*retry_count, 1);
                        assert_eq!(
                            *failure_class,
                            if percent == 100 {
                                JobFailureClass::Timeout
                            } else {
                                JobFailureClass::Transient
                            }
                        );
                        assert_eq!(
                            *failure_code,
                            if percent == 100 {
                                "timed_out"
                            } else {
                                "database_error"
                            }
                        );
                        break;
                    }
                    HostedWorkerEventKind::Failed {
                        failure_class,
                        failure_code,
                    } => {
                        assert!(!retryable);
                        assert_eq!(
                            *failure_class,
                            if max_retries == 0 {
                                JobFailureClass::Transient
                            } else {
                                JobFailureClass::Permanent
                            }
                        );
                        assert_eq!(
                            *failure_code,
                            if max_retries == 0 {
                                "retry_exhausted"
                            } else {
                                "database_error"
                            }
                        );
                        break;
                    }
                    HostedWorkerEventKind::Completed { .. }
                    | HostedWorkerEventKind::NoteUpdated { .. } => {
                        panic!("failed transaction emitted success")
                    }
                    HostedWorkerEventKind::Progress {
                        percent: emitted, ..
                    } => assert_ne!(*emitted, percent),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        sqlx::query("UPDATE public.job_queue SET next_attempt_at=NOW()+interval '1 hour' WHERE id=$1 AND status='pending'").bind(job).execute(admin).await.unwrap();
        handle.shutdown().await.unwrap();
        sqlx::raw_sql("DROP TRIGGER reject_owned_progress ON public.job_queue; DROP FUNCTION public.reject_owned_progress()").execute(admin).await.unwrap();
        let committed = state(admin, schema, note).await;
        assert_eq!(
            committed["access"],
            if percent == 100 { json!(1) } else { json!(0) }
        );
        let (status, error): (String, Option<String>) =
            sqlx::query_as("SELECT status::text,error_message FROM public.job_queue WHERE id=$1")
                .bind(job)
                .fetch_one(admin)
                .await
                .unwrap();
        assert_eq!(status, if retryable { "pending" } else { "failed" });
        assert!(!error.unwrap_or_default().contains("secret-looking"));
        if retryable {
            sqlx::query(
                "UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second' WHERE id=$1",
            )
            .bind(job)
            .execute(admin)
            .await
            .unwrap();
            let dispatcher =
                HostedJobDispatcher::new(runtime.clone(), HostedDispatchLimits::default())
                    .await
                    .unwrap();
            let claim = dispatcher
                .claim_next(
                    TierGroup::CpuAndAgnostic,
                    &[JobType::DocumentTypeInference],
                    &[],
                )
                .await
                .unwrap()
                .claim
                .unwrap();
            assert_eq!(claim.job().id, job);
            assert_ne!(Some(claim.attempt_id()), first_attempt);
            execute(&claim).await;
            assert_eq!(state(admin, schema, note).await["access"], json!(1));
            assert_eq!(state(admin, schema, note).await["activities"], json!(1));
            if percent == 100 {
                assert_eq!(state(admin, schema, note).await, committed);
            }
            assert!(claim.complete(None).await.unwrap());
        }
    }
    // Real row-lock timeout and outer deadline retain typed retry classification.
    let note = Uuid::new_v4();
    seed(admin, schema, tenant, note, "MATCH SPEC", "", None).await;
    let claim = mint(admin, runtime, tenant, schema, note).await;
    let mut lock = admin.begin().await.unwrap();
    sqlx::query(&format!(
        "SELECT id FROM {schema}.note WHERE id=$1 FOR UPDATE"
    ))
    .bind(note)
    .fetch_one(&mut *lock)
    .await
    .unwrap();
    assert!(matches!(
        HostedDocumentTypeInferenceHandler
            .execute(HostedJobContext::from_claim(claim.clone()))
            .await,
        JobResult::Retry(_)
    ));
    lock.rollback().await.unwrap();
    let deadline: matric_core::Result<Option<()>> = claim
        .with_content(|_| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(3)).await;
                Ok(())
            })
        })
        .await;
    assert!(matches!(deadline, Err(Error::DeadlineExceeded)));
    assert_eq!(state(admin, schema, note).await["access"], json!(0));
    execute(&claim).await;
    assert!(claim.complete(None).await.unwrap());
    println!("hosted_database_retry: real worker SQLSTATE/progress rollback and durable replay; permanent errors; retry exhaustion; row lock and typed deadline");
}
