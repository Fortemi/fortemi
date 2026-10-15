//! Queue controls and summaries use durable archive ownership and worker eligibility.

use super::*;
use futures::StreamExt;
use matric_core::{ArchiveRepository, JobRepository, TierGroup};
use sqlx::ConnectOptions;
use std::time::Duration;
use tower::ServiceExt;

const BOUND: Duration = Duration::from_secs(12);

async fn community_db(admin: &sqlx::PgPool) -> Database {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    startup::database(
        admin.connect_options().to_url_lossy().as_str(),
        None,
        startup::Tenancy::Community,
    )
    .await
    .unwrap()
}

async fn add_tenant(admin: &sqlx::PgPool) -> Uuid {
    let tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Queue regression','active')")
        .bind(tenant).bind(format!("queue-{tenant}"))
        .execute(admin).await.unwrap();
    tenant
}

fn request(method: Method, path: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

async fn control(app: &Router, action: &str, key: &str, expected: StatusCode) {
    let response = tokio::time::timeout(
        BOUND,
        app.clone().oneshot(request(
            Method::POST,
            &format!("/api/v1/jobs/{action}/{key}"),
        )),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), expected, "{action}/{key}");
}

#[sqlx::test(migrations = false)]
async fn archive_pause_keys_resume_persisted_state_and_fence_worker_claims(admin: sqlx::PgPool) {
    let db = community_db(&admin).await;
    let archive = db
        .archives
        .create_archive_schema("queue_owned", None)
        .await
        .unwrap();
    let foreign = add_tenant(&admin).await;
    sqlx::query("INSERT INTO archive_registry(name,schema_name,tenant_id) VALUES('queue_foreign','archive_queue_foreign',$1)")
        .bind(foreign).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO system_config(key,value) VALUES('job_pause_state',$1)")
        .bind(serde_json::json!({"global_paused":false,"paused_archives":[archive.schema_name]}))
        .execute(&db.pool)
        .await
        .unwrap();
    let pauses = PauseState::load(db.pool.clone()).await.unwrap();
    let mut state = tests::build_call_api_test_state(db.clone(), "queue-controls").await;
    state.pause_state = Some(pauses.clone());
    let bus = state.event_bus.clone();
    let app = production_router(state, None);

    // Both forms remove schema-key pauses, including state loaded after restart.
    for key in [&archive.schema_name, &archive.name] {
        assert!(pauses.is_archive_paused(&archive.schema_name).await);
        control(&app, "resume", key, StatusCode::OK).await;
        assert!(pauses.paused_archive_names().await.is_empty());
        assert!(PauseState::load(db.pool.clone())
            .await
            .unwrap()
            .paused_archive_names()
            .await
            .is_empty());
        pauses.pause_archive(&archive.schema_name).await.unwrap();
    }
    pauses.resume_archive(&archive.schema_name).await.unwrap();
    for key in [&archive.name, &archive.schema_name] {
        control(&app, "pause", key, StatusCode::OK).await;
        assert_eq!(
            pauses.paused_archive_names().await,
            vec![archive.schema_name.clone()]
        );
        assert_eq!(
            PauseState::load(db.pool.clone())
                .await
                .unwrap()
                .paused_archive_names()
                .await,
            vec![archive.schema_name.clone()]
        );
        control(&app, "resume", key, StatusCode::OK).await;
    }

    // A display-name pause must feed the exact exclusion list used by workers.
    control(&app, "pause", &archive.name, StatusCode::OK).await;
    let job = db
        .jobs
        .queue(
            None,
            JobType::Embedding,
            0,
            Some(serde_json::json!({"schema":archive.schema_name})),
            Some(0),
        )
        .await
        .unwrap();
    assert!(db
        .jobs
        .claim_next_for_tier_excluding(
            TierGroup::CpuAndAgnostic,
            &[JobType::Embedding],
            &pauses.paused_archive_names().await
        )
        .await
        .unwrap()
        .is_none());
    let status: String = sqlx::query_scalar("SELECT status::text FROM job_queue WHERE id=$1")
        .bind(job)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(status, "pending");

    // Even a bypass-capable community pool cannot resolve a foreign archive.
    let before = bus.replay_buffer_len();
    for action in ["pause", "resume"] {
        for key in ["queue_foreign", "archive_queue_foreign"] {
            control(&app, action, key, StatusCode::NOT_FOUND).await;
        }
    }
    assert_eq!(bus.replay_buffer_len(), before);
    assert_eq!(
        pauses.paused_archive_names().await,
        vec![archive.schema_name.clone()]
    );
    control(&app, "resume", &archive.name, StatusCode::OK).await;
    let claimed = db
        .jobs
        .claim_next_for_tier_excluding(
            TierGroup::CpuAndAgnostic,
            &[JobType::Embedding],
            &pauses.paused_archive_names().await,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job);
    drop(app);
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn archive_pause_alias_collision_prefers_schema_key_and_changes_only_one_archive(
    admin: sqlx::PgPool,
) {
    let db = community_db(&admin).await;
    let research = db
        .archives
        .create_archive_schema("research", None)
        .await
        .unwrap();
    let prefixed = db
        .archives
        .create_archive_schema("archive_research", None)
        .await
        .unwrap();
    sqlx::query("INSERT INTO system_config(key,value) VALUES('job_pause_state',$1)")
        .bind(serde_json::json!({"global_paused":false,"paused_archives":[research.schema_name,prefixed.schema_name]}))
        .execute(&db.pool).await.unwrap();
    let pauses = PauseState::load(db.pool.clone()).await.unwrap();
    let mut state = tests::build_call_api_test_state(db.clone(), "queue-alias-collision").await;
    state.pause_state = Some(pauses.clone());
    let app = production_router(state, None);

    // The colliding display name addresses research via its legacy schema key.
    // archive_research itself is unambiguously addressed by archive_archive_research.
    for (key, target, other) in [
        (&research.name, &research.schema_name, &prefixed.schema_name),
        (
            &research.schema_name,
            &research.schema_name,
            &prefixed.schema_name,
        ),
        (
            &prefixed.schema_name,
            &prefixed.schema_name,
            &research.schema_name,
        ),
    ] {
        control(&app, "resume", key, StatusCode::OK).await;
        for observed in [&pauses, &PauseState::load(db.pool.clone()).await.unwrap()] {
            assert!(!observed.is_archive_paused(target).await, "resume/{key}");
            assert!(
                observed.is_archive_paused(other).await,
                "resume/{key} changed the other archive"
            );
            assert_eq!(observed.paused_archive_names().await.len(), 1);
        }
        control(&app, "pause", key, StatusCode::OK).await;
        for observed in [&pauses, &PauseState::load(db.pool.clone()).await.unwrap()] {
            assert!(observed.is_archive_paused(target).await, "pause/{key}");
            assert!(
                observed.is_archive_paused(other).await,
                "pause/{key} changed the other archive"
            );
            assert_eq!(observed.paused_archive_names().await.len(), 2);
        }
    }
    // Pausing through either alias must also leave an unpaused second archive alone.
    control(&app, "resume", &prefixed.schema_name, StatusCode::OK).await;
    for key in [&research.name, &research.schema_name] {
        control(&app, "pause", key, StatusCode::OK).await;
        for observed in [&pauses, &PauseState::load(db.pool.clone()).await.unwrap()] {
            assert_eq!(
                observed.paused_archive_names().await,
                vec![research.schema_name.clone()]
            );
        }
    }
    drop(app);
    db.pool.close().await;
}

struct QueueStream {
    body: axum::body::BodyDataStream,
    buffer: String,
}

impl QueueStream {
    async fn connect(app: &Router, cursor: Option<Uuid>) -> Self {
        let mut request = request(
            Method::GET,
            "/api/v1/events?memory=queue_named&types=queue.status",
        );
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
                    let chunk = self.body.next().await.unwrap().unwrap();
                    self.buffer.push_str(std::str::from_utf8(&chunk).unwrap());
                }
            }
        })
        .await
        .expect("bounded queue summary delivery")
    }
}

async fn seed_queue(pool: &sqlx::PgPool, tenant: Uuid, schema: &str) {
    // Two ready jobs (NULL/past retry time), one delayed, one supported running,
    // and unsupported pending/running rows retained in the total only.
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,payload,job_type,status,next_attempt_at)
        VALUES (uuidv7(),$1,jsonb_build_object('schema',$2::text),'embedding','pending',NULL),
               (uuidv7(),$1,jsonb_build_object('schema',$2::text),'embedding','pending',NOW()-interval '1 hour'),
               (uuidv7(),$1,jsonb_build_object('schema',$2::text),'embedding','pending',NOW()+interval '1 hour'),
               (uuidv7(),$1,jsonb_build_object('schema',$2::text),'embedding','running',NULL),
               (uuidv7(),$1,jsonb_build_object('schema',$2::text),'attachment_processing','pending',NULL),
               (uuidv7(),$1,jsonb_build_object('schema',$2::text),'attachment_processing','running',NULL)")
        .bind(tenant).bind(schema).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn community_named_queue_summary_live_and_replay_preserve_eligibility(admin: sqlx::PgPool) {
    let db = community_db(&admin).await;
    let archive = db
        .archives
        .create_archive_schema("queue_named", None)
        .await
        .unwrap();
    // Archive provisioning queues index maintenance; isolate the eligibility
    // fixture in this disposable database before publishing the baseline.
    sqlx::query("DELETE FROM public.job_queue")
        .execute(&admin)
        .await
        .unwrap();
    let mut state = tests::build_call_api_test_state(db.clone(), "community-queue").await;
    let bus = Arc::new(EventBus::new(128));
    state.event_bus = bus.clone();
    let app = production_router(state, None);
    let mut raw = bus.subscribe();
    event_scope::publish_queue_status(&db, &bus).await;
    let cursor = tokio::time::timeout(BOUND, async {
        loop {
            let event = raw.recv().await.unwrap();
            if event.memory.as_deref() == Some("queue_named") {
                break event.event_id;
            }
        }
    })
    .await
    .expect("production publisher includes named memories");
    let mut live = QueueStream::connect(&app, None).await;
    seed_queue(&admin, Uuid::nil(), &archive.schema_name).await;
    // Community statistics remain global across this owner's archives.
    seed_queue(&admin, Uuid::nil(), "public").await;
    let stats = db.jobs.queue_stats().await.unwrap();
    assert_eq!(
        (
            stats.total,
            stats.processing,
            stats.pending,
            stats.delayed,
            stats.incompatible
        ),
        (12, 2, 4, 2, 4)
    );
    let foreign = add_tenant(&admin).await;
    seed_queue(&admin, foreign, &archive.schema_name).await;
    event_scope::publish_queue_status(&db, &bus).await;
    let delivered = live.next().await;
    assert_eq!(delivered["event_type"], "queue.status");
    assert_eq!(delivered["tenant_id"], Uuid::nil().to_string());
    assert_eq!(delivered["memory"], "queue_named");
    assert_eq!(delivered["payload"]["total_jobs"], stats.total);
    assert_eq!(delivered["payload"]["running"], stats.processing);
    assert_eq!(delivered["payload"]["pending"], stats.pending);
    let mut replay = QueueStream::connect(&app, Some(cursor)).await;
    assert_eq!(
        replay.next().await,
        delivered,
        "replay retains live identity and counts"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(150), replay.next())
            .await
            .is_err(),
        "other memory contexts must stay filtered"
    );
    drop(live);
    drop(replay);
    drop(app);
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn tenant_queue_summary_eligibility_stays_inside_tenant_and_archive_fences(
    admin: sqlx::PgPool,
) {
    let community = community_db(&admin).await;
    let tenant = add_tenant(&admin).await;
    let foreign = add_tenant(&admin).await;
    let pool = matric_db::create_pool_with_config(
        admin.connect_options().to_url_lossy().as_str(),
        PoolConfig::new().single_tenant(tenant),
    )
    .await
    .unwrap();
    let db = Database::new(pool);
    // Summaries read ownership and queue rows without opening archive content.
    sqlx::query("INSERT INTO archive_registry(name,schema_name,tenant_id) VALUES('queue_tenant','archive_queue_tenant',$1)")
        .bind(tenant).execute(&admin).await.unwrap();
    let archive = db
        .archives
        .get_archive_by_name("queue_tenant")
        .await
        .unwrap()
        .unwrap();
    seed_queue(&admin, tenant, &archive.schema_name).await;
    seed_queue(&admin, tenant, "public").await;
    seed_queue(&admin, foreign, &archive.schema_name).await;
    let bus = EventBus::new(32);
    let mut raw = bus.subscribe();
    event_scope::publish_queue_status(&db, &bus).await;
    let mut memories = Vec::new();
    while let Ok(event) = raw.try_recv() {
        assert_eq!(
            event.tenant_id.as_deref(),
            Some(tenant.to_string().as_str())
        );
        assert!(matches!(
            event.payload,
            ServerEvent::QueueStatus {
                total_jobs: 6,
                running: 1,
                pending: 2
            }
        ));
        memories.push(event.memory.unwrap());
    }
    memories.sort();
    assert_eq!(memories, vec!["public", "queue_tenant"]);
    db.pool.close().await;
    community.pool.close().await;
}
