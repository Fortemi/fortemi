use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use matric_core::{JobFailureClass, JobType};
use matric_db::Database;
use matric_jobs::worker::{HostedWorkerEvent, HostedWorkerEventKind};
use matric_jobs::{
    HostedJobContext, HostedJobHandler, JobResult, JobWorker, WorkerConfig, WorkerEvent,
};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

struct Handler {
    contexts: mpsc::UnboundedSender<HostedJobContext>,
}

#[async_trait]
impl HostedJobHandler for Handler {
    fn job_type(&self) -> JobType {
        JobType::TitleGeneration
    }
    async fn execute(&self, ctx: HostedJobContext) -> JobResult {
        assert!(ctx
            .report_progress(40, Some("owned fixture progress"))
            .await
            .unwrap());
        match ctx
            .payload()
            .and_then(|v| v["mode"].as_str())
            .unwrap_or("success")
        {
            "retry" => JobResult::Retry("connection timeout; private fixture text".into()),
            "fail" => JobResult::Failed("invalid input; private fixture text".into()),
            "panic" => panic!("owned fixture panic"),
            "timeout" => {
                std::future::pending::<()>().await;
                unreachable!()
            }
            "hold" => {
                self.contexts
                    .send(ctx)
                    .unwrap_or_else(|_| panic!("context receiver closed"));
                std::future::pending::<()>().await;
                unreachable!()
            }
            _ => JobResult::Success(Some(json!({"fixture":true}))),
        }
    }
}

async fn pending(admin: &PgPool, tenant: Uuid, mode: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,job_type,cost_tier,payload) VALUES($1,$2,'title_generation',0,$3)")
        .bind(id).bind(tenant).bind(json!({"mode":mode})).execute(admin).await.unwrap();
    id
}

async fn state(admin: &PgPool, id: Uuid) -> (String, i32) {
    sqlx::query_as("SELECT status::text,retry_count FROM public.job_queue WHERE id=$1")
        .bind(id)
        .fetch_one(admin)
        .await
        .unwrap()
}

async fn event(rx: &mut broadcast::Receiver<WorkerEvent>) -> HostedWorkerEvent {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match rx.recv().await.unwrap() {
                WorkerEvent::Hosted(event) => return event,
                WorkerEvent::WorkerStarted => {}
                other => panic!("unexpected unscoped event: {other:?}"),
            }
        }
    })
    .await
    .expect("hosted event deadline")
}

async fn worker(
    runtime: &PgPool,
    contexts: mpsc::UnboundedSender<HostedJobContext>,
    timeout: Duration,
) -> JobWorker {
    let mut config = WorkerConfig::default().with_poll_interval(60000);
    config.max_concurrent_jobs = 1;
    config.job_timeout = timeout;
    JobWorker::new(Database::new(runtime.clone()), config, None)
        .with_hosted_handlers(vec![Arc::new(Handler { contexts })])
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn hosted_execution_fairness_callbacks_errors_and_shutdown(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    // Empty first page must not delay tenant 100 behind the 60-second poll.
    for i in 1..=40 {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(Uuid::from_u128(i)).bind(format!("worker-empty-{i}")).execute(&admin).await.unwrap();
    }
    let [a, b] = [100, 200].map(Uuid::from_u128);
    for tenant in [a, b] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("worker-{tenant}")).execute(&admin).await.unwrap();
    }
    let role = format!("execution_{}", Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for grant in ["USAGE ON SCHEMA public", "SELECT ON public.tenant_registry,public.archive_registry,public.job_queue,public.job_attempt,public.job_history", "UPDATE ON public.job_queue,public.job_attempt", "INSERT ON public.job_attempt,public.job_history"] {
        sqlx::query(&format!("GRANT {grant} TO {role}")).execute(&admin).await.unwrap();
    }
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*admin.connect_options()).clone().username(&role))
        .await
        .unwrap();
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    let (tx, mut contexts) = mpsc::unbounded_channel();
    assert!(JobWorker::new(
        Database::new(runtime.clone()),
        WorkerConfig::default(),
        None
    )
    .with_hosted_handlers(vec![])
    .await
    .is_err());
    assert!(JobWorker::new(
        Database::new(runtime.clone()),
        WorkerConfig::default(),
        None
    )
    .with_hosted_handlers(vec![
        Arc::new(Handler {
            contexts: tx.clone()
        }),
        Arc::new(Handler {
            contexts: tx.clone()
        })
    ])
    .await
    .is_err());
    assert!(
        JobWorker::new(Database::new(admin.clone()), WorkerConfig::default(), None)
            .with_hosted_handlers(vec![Arc::new(Handler {
                contexts: tx.clone()
            })])
            .await
            .is_err()
    );

    let a1 = pending(&admin, a, "success").await;
    let a2 = pending(&admin, a, "success").await;
    let b1 = pending(&admin, b, "success").await;
    let mut modes = HashMap::new();
    for mode in ["retry", "fail", "panic", "timeout"] {
        modes.insert(pending(&admin, b, mode).await, mode);
    }
    // Deterministic tenant-local ordering so the fairness assertion isn't UUID-dependent.
    sqlx::query("UPDATE public.job_queue SET priority=CASE WHEN id=$1 THEN 100 WHEN id=$2 THEN 90 WHEN id=$3 THEN 100 ELSE 0 END")
        .bind(a1).bind(a2).bind(b1).execute(&admin).await.unwrap();
    let w = worker(&runtime, tx.clone(), Duration::from_millis(120)).await;
    let mut rx = w.events();
    let handle = w.start();
    let mut starts = Vec::new();
    let mut terminals = HashMap::new();
    while terminals.len() < 7 {
        let e = event(&mut rx).await;
        assert!(e.tenant_id() == a || e.tenant_id() == b);
        assert_eq!(e.archive_schema(), "public");
        match e.kind() {
            HostedWorkerEventKind::NoteUpdated { .. } => {
                panic!("no note associated with fixture job")
            }
            HostedWorkerEventKind::Started => starts.push(e.job_id()),
            HostedWorkerEventKind::Progress { percent, .. } => {
                let persisted: i32 =
                    sqlx::query_scalar("SELECT progress_percent FROM public.job_queue WHERE id=$1")
                        .bind(e.job_id())
                        .fetch_one(&admin)
                        .await
                        .unwrap();
                assert!(
                    persisted >= *percent || state(&admin, e.job_id()).await.0 != "running",
                    "a later retry may reset already committed progress"
                );
            }
            HostedWorkerEventKind::Completed { .. } => {
                assert_eq!(state(&admin, e.job_id()).await.0, "completed");
                terminals.insert(e.job_id(), "completed");
            }
            HostedWorkerEventKind::Failed { failure_class, .. } => {
                assert_eq!(*failure_class, JobFailureClass::Permanent);
                assert_eq!(state(&admin, e.job_id()).await.0, "failed");
                terminals.insert(e.job_id(), "failed");
            }
            HostedWorkerEventKind::RetryScheduled {
                failure_class,
                failure_code,
                retry_count,
                ..
            } => {
                let mode = modes[&e.job_id()];
                if mode == "panic" {
                    assert_eq!(*failure_class, JobFailureClass::Poison);
                    assert_eq!(*failure_code, "handler_panicked");
                }
                if mode == "timeout" {
                    assert_eq!(*failure_class, JobFailureClass::Timeout);
                    assert_eq!(*failure_code, "job_timeout");
                }
                assert_eq!(*retry_count, 1);
                assert_eq!(state(&admin, e.job_id()).await, ("pending".into(), 1));
                // Hold scheduled retries outside this fixture's remaining phases.
                sqlx::query("UPDATE public.job_queue SET next_attempt_at=NOW()+interval '1 hour' WHERE id=$1").bind(e.job_id()).execute(&admin).await.unwrap();
                terminals.insert(e.job_id(), "retry");
            }
        }
    }
    assert_eq!(&starts[..3], &[a1, b1, a2]);
    tokio::time::timeout(Duration::from_secs(6), handle.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.recv().await.unwrap(),
        WorkerEvent::WorkerStopped
    ));
    let raw_errors: Vec<String> = sqlx::query_scalar(
        "SELECT error_message FROM public.job_queue WHERE error_message IS NOT NULL",
    )
    .fetch_all(&admin)
    .await
    .unwrap();
    assert!(raw_errors
        .iter()
        .all(|s| !s.contains("private fixture text")));

    // A deferred settlement failure must emit neither completion nor a false retry.
    let failed_commit = pending(&admin, a, "success").await;
    sqlx::raw_sql("CREATE FUNCTION public.fail_worker_settle() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned settlement fixture' USING ERRCODE='23514'; END $$; CREATE CONSTRAINT TRIGGER fail_worker_settle AFTER INSERT ON public.job_history DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.fail_worker_settle();").execute(&admin).await.unwrap();
    let w = worker(&runtime, tx.clone(), Duration::from_secs(60)).await;
    let mut rx = w.events();
    let handle = w.start();
    assert!(matches!(
        event(&mut rx).await.kind(),
        HostedWorkerEventKind::Started
    ));
    assert!(matches!(
        event(&mut rx).await.kind(),
        HostedWorkerEventKind::Progress { .. }
    ));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(state(&admin, failed_commit).await.0, "running");
    handle.shutdown().await.unwrap();
    sqlx::raw_sql("DROP TRIGGER fail_worker_settle ON public.job_history; DROP FUNCTION public.fail_worker_settle()").execute(&admin).await.unwrap();

    // Shutdown drops a live handler future, waits for it, and retries only that
    // attempt. An escaped immutable context cannot mutate the replacement.
    let held = pending(&admin, a, "hold").await;
    let w = worker(&runtime, tx.clone(), Duration::from_secs(60)).await;
    let mut rx = w.events();
    let handle = w.start();
    let ctx = tokio::time::timeout(Duration::from_secs(3), contexts.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ctx.job_id(), held);
    let old_attempt = event(&mut rx).await.attempt_id();
    handle.shutdown().await.unwrap();
    assert_eq!(state(&admin, held).await, ("pending".into(), 1));
    assert!(!ctx.report_progress(99, Some("late")).await.unwrap());
    sqlx::query("UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second',payload=$2 WHERE id=$1")
        .bind(held).bind(json!({"mode":"success"})).execute(&admin).await.unwrap();
    let w = worker(&runtime, tx.clone(), Duration::from_secs(60)).await;
    let mut rx = w.events();
    let handle = w.start();
    loop {
        let e = event(&mut rx).await;
        assert_eq!(e.job_id(), held);
        assert_ne!(e.attempt_id(), old_attempt);
        if matches!(e.kind(), HostedWorkerEventKind::Completed { .. }) {
            break;
        }
    }
    assert!(!ctx.report_progress(99, None).await.unwrap());
    handle.shutdown().await.unwrap();

    let suspended = pending(&admin, b, "hold").await;
    let w = worker(&runtime, tx, Duration::from_secs(60)).await;
    let handle = w.start();
    let ctx = tokio::time::timeout(Duration::from_secs(3), contexts.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ctx.job_id(), suspended);
    sqlx::query("UPDATE public.tenant_registry SET status='suspended' WHERE id=$1")
        .bind(b)
        .execute(&admin)
        .await
        .unwrap();
    assert!(ctx.report_progress(99, None).await.is_err());
    handle.shutdown().await.unwrap();
    assert_eq!(state(&admin, suspended).await.0, "running");

    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&runtime)
            .await
            .unwrap(),
        backend
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT current_setting('app.current_tenant')")
            .fetch_one(&runtime)
            .await
            .unwrap(),
        ""
    );
    runtime.close().await;
    sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("hosted_execution: restrictive role; explicit registry; paged continuation and tenant fairness; committed progress; success/failure/retry/panic/timeout; deferred settlement rollback; bounded shutdown and replacement fencing; suspension; no legacy claims; owned role removed");
}
