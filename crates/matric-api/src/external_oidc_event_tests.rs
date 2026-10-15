//! Signed external-OIDC live/replay regressions exercise production publishers.

use super::*;
use futures::StreamExt;
use matric_core::JobRepository;
use matric_jobs::{JobContext, JobHandler, JobResult, JobWorker, WorkerConfig};
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(12);

struct SseReader {
    body: axum::body::BodyDataStream,
    buffer: String,
}

impl SseReader {
    async fn connect(app: &Router, memory: &str, cursor: Option<Uuid>) -> Self {
        let mut request = oidc_request(Method::GET,
            &format!("/api/v1/events?memory={memory}&types=note.archived,note.restored,note.updated,note.tags.updated,job.completed,index.embedding.updated"),
            serde_json::Value::Null, None);
        if let Some(cursor) = cursor {
            request
                .headers_mut()
                .insert("last-event-id", cursor.to_string().parse().unwrap());
        }
        let response = tokio::time::timeout(BOUND, app.clone().oneshot(request))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        Self {
            body: response.into_body().into_data_stream(),
            buffer: String::new(),
        }
    }

    async fn next(&mut self) -> serde_json::Value {
        tokio::time::timeout(BOUND, async {
            loop {
                if let Some(end) = self.buffer.find("\n\n") {
                    let frame: String = self.buffer.drain(..end + 2).collect();
                    if let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) {
                        return serde_json::from_str(data).unwrap();
                    }
                } else {
                    let chunk = self.body.next().await.expect("SSE stays open").unwrap();
                    self.buffer.push_str(std::str::from_utf8(&chunk).unwrap());
                }
            }
        })
        .await
        .expect("bounded SSE delivery")
    }
}

async fn bounded_call(
    app: &Router,
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) {
    let (status, body) = tokio::time::timeout(BOUND, call(app, method, path, body, memory))
        .await
        .unwrap();
    assert!(status.is_success(), "{status}: {body}");
}

async fn owned_event(
    reader: &mut SseReader,
    tenant: Uuid,
    memory: &str,
    kind: &str,
    note: Uuid,
) -> serde_json::Value {
    let event = reader.next().await;
    assert_eq!(
        event["tenant_id"],
        tenant.to_string(),
        "foreign tenant must be rejected: {event}"
    );
    assert_eq!(event["memory"], memory);
    assert_eq!(event["event_type"], kind);
    assert_eq!(event["payload"]["note_id"], note.to_string());
    event
}

// The real worker persists lifecycle transitions and emits completion after
// commit. This CPU-only handler keeps the regression independent of inference.
struct CompleteEmbedding;

#[async_trait::async_trait]
impl JobHandler for CompleteEmbedding {
    fn job_type(&self) -> JobType {
        JobType::Embedding
    }
    async fn execute(&self, ctx: JobContext) -> JobResult {
        ctx.report_progress(100, Some("Complete"));
        JobResult::Success(None)
    }
}

async fn worker(
    db: Database,
    bus: Arc<EventBus>,
) -> (matric_jobs::WorkerHandle, tokio::task::JoinHandle<()>) {
    let worker = JobWorker::new(
        db.clone(),
        WorkerConfig::default()
            .with_poll_interval(20)
            .with_max_concurrent(1),
        None,
    );
    worker.register_handler(CompleteEmbedding).await;
    let rx = worker.events();
    let bridge = tokio::spawn(bridge_worker_events(rx, bus, db));
    (worker.start(), bridge)
}

async fn wait_raw(
    bus: &mut tokio::sync::broadcast::Receiver<EventEnvelope>,
    tenant: Uuid,
    kind: &str,
) -> EventEnvelope {
    tokio::time::timeout(BOUND, async {
        loop {
            let event = bus.recv().await.unwrap();
            if event.tenant_id.as_deref() == Some(tenant.to_string().as_str())
                && event.event_type == kind
            {
                return event;
            }
        }
    })
    .await
    .expect("bounded worker publication")
}

#[sqlx::test(migrations = false)]
async fn external_oidc_scoped_mutations_and_worker_completion_live_and_replay(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let tenant = Uuid::new_v4();
    let url = runtime_url(&admin, tenant).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    db.pool.close().await;
    // One runtime connection proves stream bodies and mutation publication do
    // not retain a connection or perform nested precommit pool checkouts.
    let runtime = matric_db::create_pool_with_config(
        &url,
        PoolConfig::new().single_tenant(tenant).max_connections(1),
    )
    .await
    .unwrap();
    let db = Database::new(runtime.clone())
        .with_administrative_pool(admin.clone())
        .await
        .unwrap();
    let bus = Arc::new(EventBus::new(256));
    let (app, _) = production_router_with_event_bus(db.clone(), tenant, bus.clone()).await;
    let foreign = Uuid::new_v4();
    external_oidc::ensure_default_tenant(&admin, foreign)
        .await
        .unwrap();
    let foreign_url = runtime_url(&admin, foreign).await;
    let foreign_runtime =
        matric_db::create_pool_with_config(&foreign_url, PoolConfig::new().single_tenant(foreign))
            .await
            .unwrap();
    let foreign_db = Database::new(foreign_runtime.clone());
    let (foreign_app, _) =
        production_router_with_event_bus(foreign_db.clone(), foreign, bus.clone()).await;
    bounded_call(
        &app,
        Method::POST,
        "/api/v1/archives",
        serde_json::json!({"name":"round8_named"}),
        None,
    )
    .await;
    let mut raw = bus.subscribe();

    for memory in ["public", "round8_named"] {
        let (status, note) = call(
            &app,
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Owned note","pipeline":[]}),
            Some(memory),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{note}");
        let note = Uuid::parse_str(note["id"].as_str().unwrap()).unwrap();
        let cursor = wait_raw(&mut raw, tenant, "note.created").await.event_id;
        let (status, other_note) = call(
            &foreign_app,
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Foreign note","pipeline":[]}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{other_note}");
        let other_note = Uuid::parse_str(other_note["id"].as_str().unwrap()).unwrap();
        let mut live = SseReader::connect(&app, memory, None).await;
        let mut delivered = Vec::new();
        for (archived, kind) in [(true, "note.archived"), (false, "note.restored")] {
            bounded_call(
                &foreign_app,
                Method::PATCH,
                &format!("/api/v1/notes/{other_note}/status"),
                serde_json::json!({"archived":archived}),
                None,
            )
            .await;
            bounded_call(
                &app,
                Method::PATCH,
                &format!("/api/v1/notes/{note}/status"),
                serde_json::json!({"archived":archived}),
                Some(memory),
            )
            .await;
            delivered.push(owned_event(&mut live, tenant, memory, kind, note).await);
            let schema = if memory == "public" {
                "public".into()
            } else {
                db.archives
                    .get_archive_by_name(memory)
                    .await
                    .unwrap()
                    .unwrap()
                    .schema_name
            };
            let durable: bool = sqlx::query_scalar(&format!(
                "SELECT archived FROM {schema}.note WHERE id=$1 AND tenant_id=$2"
            ))
            .bind(note)
            .bind(tenant)
            .fetch_one(&admin)
            .await
            .unwrap();
            assert_eq!(durable, archived, "event must follow committed status");
        }
        bounded_call(
            &foreign_app,
            Method::PATCH,
            &format!("/api/v1/notes/{other_note}"),
            serde_json::json!({"content":"Foreign update","revision_mode":"none"}),
            None,
        )
        .await;
        bounded_call(
            &app,
            Method::PATCH,
            &format!("/api/v1/notes/{note}"),
            serde_json::json!({"content":"Owned update","revision_mode":"none"}),
            Some(memory),
        )
        .await;
        delivered.push(owned_event(&mut live, tenant, memory, "note.updated", note).await);
        bounded_call(
            &foreign_app,
            Method::PUT,
            &format!("/api/v1/notes/{other_note}/tags"),
            serde_json::json!({"tags":["foreign-tag"]}),
            None,
        )
        .await;
        bounded_call(
            &app,
            Method::PUT,
            &format!("/api/v1/notes/{note}/tags"),
            serde_json::json!({"tags":["owned-tag"]}),
            Some(memory),
        )
        .await;
        let tagged = owned_event(&mut live, tenant, memory, "note.tags.updated", note).await;
        assert_eq!(tagged["payload"]["tags"], serde_json::json!(["owned-tag"]));
        delivered.push(tagged);
        // A foreign worker publishes on the same bus; neither its completion
        // nor its derivative note/index events may reach this stream or replay.
        let other_job = foreign_db
            .jobs
            .queue(Some(other_note), JobType::Embedding, 0, None, None)
            .await
            .unwrap();
        let (foreign_worker, foreign_bridge) = worker(foreign_db.clone(), bus.clone()).await;
        let foreign_completed = wait_raw(&mut raw, foreign, "job.completed").await;
        assert_eq!(foreign_completed.payload.entity_id(), Some(other_job));
        wait_raw(&mut raw, foreign, "note.updated").await;
        tokio::time::timeout(BOUND, foreign_worker.shutdown())
            .await
            .unwrap()
            .unwrap();
        foreign_bridge.abort();
        let schema = db
            .archives
            .get_archive_by_name(memory)
            .await
            .unwrap()
            .unwrap()
            .schema_name;
        let job = db
            .jobs
            .queue(
                Some(note),
                JobType::Embedding,
                0,
                Some(serde_json::json!({"schema":schema, "tenant_id":foreign})),
                None,
            )
            .await
            .unwrap();
        let (own_worker, own_bridge) = worker(db.clone(), bus.clone()).await;
        let completed = owned_event(&mut live, tenant, memory, "job.completed", note).await;
        assert_eq!(completed["payload"]["job_id"], job.to_string());
        let status: String = sqlx::query_scalar("SELECT status::text FROM job_queue WHERE id=$1")
            .bind(job)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(status, "completed");
        delivered.push(completed);
        delivered
            .push(owned_event(&mut live, tenant, memory, "index.embedding.updated", note).await);
        delivered.push(owned_event(&mut live, tenant, memory, "note.updated", note).await);
        tokio::time::timeout(BOUND, own_worker.shutdown())
            .await
            .unwrap()
            .unwrap();
        own_bridge.abort();
        drop(live);
        let mut replay = SseReader::connect(&app, memory, Some(cursor)).await;
        for expected in delivered {
            assert_eq!(
                replay.next().await,
                expected,
                "replay retains live event identity and ownership"
            );
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(150), replay.next())
                .await
                .is_err(),
            "foreign replay must be rejected"
        );
    }
    // Bare worker hints cannot mint ownership for a missing job or borrow an
    // archive belonging to another tenant, even with an existing queue row.
    let schema = db
        .archives
        .get_archive_by_name("round8_named")
        .await
        .unwrap()
        .unwrap()
        .schema_name;
    let bad_job = foreign_db
        .jobs
        .queue(
            None,
            JobType::Embedding,
            0,
            Some(serde_json::json!({"schema":schema})),
            None,
        )
        .await
        .unwrap();
    let before = bus.replay_buffer_len();
    let (tx, rx) = tokio::sync::broadcast::channel(8);
    let rejected_bridge = tokio::spawn(bridge_worker_events(rx, bus.clone(), foreign_db));
    for job_id in [Uuid::new_v4(), bad_job] {
        tx.send(WorkerEvent::JobCompleted {
            job_id,
            job_type: JobType::Embedding,
        })
        .unwrap();
    }
    drop(tx);
    tokio::time::timeout(BOUND, rejected_bridge)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        bus.replay_buffer_len(),
        before,
        "unverified worker ownership must publish nothing"
    );
    drop(app);
    drop(foreign_app);
    runtime.close().await;
    foreign_runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_commit_failure_publishes_neither_live_nor_replay(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let tenant = Uuid::new_v4();
    let url = runtime_url(&admin, tenant).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    let bus = Arc::new(EventBus::new(256));
    let (app, runtime) = production_router_with_event_bus(db, tenant, bus.clone()).await;
    let mut raw = bus.subscribe();
    let (status, note) = call(
        &app,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"Rollback note","pipeline":[]}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let note = Uuid::parse_str(note["id"].as_str().unwrap()).unwrap();
    let cursor = wait_raw(&mut raw, tenant, "note.created").await.event_id;
    sqlx::raw_sql(&format!("CREATE FUNCTION reject_event_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{note}'::uuid THEN RAISE EXCEPTION 'synthetic commit failure'; END IF; RETURN NEW; END $$;
        CREATE CONSTRAINT TRIGGER reject_event_commit AFTER UPDATE ON public.note DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_event_commit();"))
        .execute(&admin).await.unwrap();
    let mut live = SseReader::connect(&app, "public", None).await;
    for (path, body) in [
        (
            format!("/api/v1/notes/{note}/status"),
            serde_json::json!({"archived":true}),
        ),
        (
            format!("/api/v1/notes/{note}"),
            serde_json::json!({"content":"Must roll back","revision_mode":"none"}),
        ),
    ] {
        let (status, body) =
            tokio::time::timeout(BOUND, call(&app, Method::PATCH, &path, body, None))
                .await
                .unwrap();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    }
    assert!(
        bus.replay_since(cursor).unwrap().is_empty(),
        "failed commits must not enter replay"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(150), live.next())
            .await
            .is_err(),
        "failed commits must not reach live SSE"
    );
    let archived: bool = sqlx::query_scalar("SELECT archived FROM note WHERE id=$1")
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert!(!archived);
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE note_id=$1")
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(jobs, 0, "rollback discards pipeline wakeups");
    drop(live);
    drop(app);
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_archive_queue_controls_preserve_operator_authorization(admin: sqlx::PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let tenant = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, tenant).await;
    bounded_call(
        &app,
        Method::POST,
        "/api/v1/archives",
        serde_json::json!({"name":"queue_owned"}),
        None,
    )
    .await;
    let db = Database::new(runtime.clone());
    let schema = db
        .archives
        .get_archive_by_name("queue_owned")
        .await
        .unwrap()
        .unwrap()
        .schema_name;
    PauseState::load(runtime.clone())
        .await
        .unwrap()
        .pause_archive(&schema)
        .await
        .unwrap();
    drop(app);
    let bus = Arc::new(EventBus::new(32));
    let (app, _) = production_router_with_event_bus(db, tenant, bus.clone()).await;
    let foreign = Uuid::new_v4();
    external_oidc::ensure_default_tenant(&admin, foreign)
        .await
        .unwrap();
    sqlx::query("INSERT INTO archive_registry(name,schema_name,tenant_id) VALUES('queue_foreign','archive_queue_foreign',$1)")
        .bind(foreign).execute(&admin).await.unwrap();
    let before = bus.replay_buffer_len();
    // External tenant admins retain their existing denial on operator-only
    // controls, regardless of whether the path is a name or a schema alias.
    for action in ["pause", "resume"] {
        for key in [
            schema.as_str(),
            "queue_owned",
            "queue_foreign",
            "archive_queue_foreign",
        ] {
            let (status, body) = tokio::time::timeout(
                BOUND,
                call(
                    &app,
                    Method::POST,
                    &format!("/api/v1/jobs/{action}/{key}"),
                    serde_json::Value::Null,
                    None,
                ),
            )
            .await
            .unwrap();
            assert_eq!(status, StatusCode::FORBIDDEN, "{action}/{key}: {body}");
        }
    }
    assert_eq!(
        bus.replay_buffer_len(),
        before,
        "denied controls publish nothing"
    );
    assert_eq!(
        PauseState::load(runtime.clone())
            .await
            .unwrap()
            .paused_archive_names()
            .await,
        vec![schema]
    );
    drop(app);
    runtime.close().await;
}
