use std::time::Duration;

use matric_core::JobRetryPolicy;
use matric_db::{Database, ScopedJobRepository, TenantScopedConn};
use matric_jobs::worker::{HostedJobRecovery, HostedRecoveryLimits};
use matric_jobs::{JobWorker, WorkerConfig, WorkerEvent};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

async fn running(admin: &PgPool, tenant: Uuid, age: i32) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO public.job_queue(id,tenant_id,job_type,status,started_at,cost_tier,payload)
        VALUES($1,$2,'embedding','running',NOW()-($3*interval '1 second'),0,
        '{\"tenant_id\":\"not-authority\",\"schema\":\"not-routing-authority\"}'::jsonb)",
    )
    .bind(id)
    .bind(tenant)
    .bind(age)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO public.job_attempt(id,tenant_id,job_id,attempt_number,started_at)
        SELECT $1,tenant_id,id,1,started_at FROM public.job_queue WHERE id=$2",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .execute(admin)
    .await
    .unwrap();
    id
}

async fn state(admin: &PgPool, id: Uuid) -> (String, i32, String) {
    sqlx::query_as(
        "SELECT q.status::text,q.retry_count,a.outcome FROM public.job_queue q
        JOIN public.job_attempt a ON a.job_id=q.id WHERE q.id=$1",
    )
    .bind(id)
    .fetch_one(admin)
    .await
    .unwrap()
}

async fn recovered(admin: &PgPool, id: Uuid) {
    assert_eq!(
        state(admin, id).await,
        ("pending".into(), 1, "stale_reaped".into())
    );
}

#[sqlx::test(migrations = false)]
async fn hosted_recovery_bounds_fairness_failure_and_worker_wiring(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let suspended = Uuid::from_u128(3);
    let deleted = Uuid::from_u128(4);
    for (tenant, status) in [
        (a, "active"),
        (b, "active"),
        (suspended, "suspended"),
        (deleted, "soft_deleted"),
    ] {
        sqlx::query(
            "INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,$3)",
        )
        .bind(tenant)
        .bind(format!("recovery-{tenant}"))
        .bind(status)
        .execute(&admin)
        .await
        .unwrap();
    }
    let role = format!("recovery_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for grant in [
        "USAGE ON SCHEMA public",
        "SELECT ON public.tenant_registry,public.job_queue,public.job_attempt",
        "UPDATE ON public.job_queue,public.job_attempt",
    ] {
        sqlx::query(&format!("GRANT {grant} TO {role}"))
            .execute(&admin)
            .await
            .unwrap();
    }
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*admin.connect_options())
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    let limits = HostedRecoveryLimits {
        tenants_per_pass: 1,
        jobs_per_tenant: 2,
        ..HostedRecoveryLimits::default()
    };
    assert!(
        HostedJobRecovery::new(admin.clone(), limits).await.is_err(),
        "migration role must be rejected"
    );
    let recovery = HostedJobRecovery::new(runtime.clone(), limits)
        .await
        .unwrap();
    let policy = JobRetryPolicy::default();
    let a1 = running(&admin, a, 7203).await;
    let a2 = running(&admin, a, 7202).await;
    let a3 = running(&admin, a, 7201).await;
    let b1 = running(&admin, b, 7200).await;
    let s = running(&admin, suspended, 7200).await;
    let d = running(&admin, deleted, 7200).await;
    let personal = running(&admin, Uuid::nil(), 7200).await;
    let recent = running(&admin, a, 0).await;
    let first = recovery.sweep(3600, &policy).await.unwrap();
    assert_eq!(
        (
            first.tenants_visited,
            first.reaped_count,
            first.failed_tenants,
            first.timed_out
        ),
        (1, 2, 0, false)
    );
    recovered(&admin, a1).await;
    recovered(&admin, a2).await;
    assert_eq!(state(&admin, a3).await.0, "running");
    let e = Uuid::from_u128(5);
    sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,'recovery-late','recovery-late','active')").bind(e).execute(&admin).await.unwrap();
    let e1 = running(&admin, e, 7200).await;
    let second = recovery.sweep(3600, &policy).await.unwrap();
    assert_eq!(second.reaped_count, 1);
    recovered(&admin, b1).await;
    assert!(recovery.sweep(3600, &policy).await.unwrap().wrapped);
    assert_eq!(
        state(&admin, e1).await.0,
        "running",
        "new tenants cannot extend the traversal high-water mark"
    );
    for id in [s, d, personal, recent] {
        assert_eq!(
            state(&admin, id).await,
            ("running".into(), 0, "running".into())
        );
    }
    assert_eq!(recovery.sweep(3600, &policy).await.unwrap().reaped_count, 1);
    recovered(&admin, a3).await;
    assert_eq!(recovery.sweep(3600, &policy).await.unwrap().reaped_count, 0);
    assert_eq!(recovery.sweep(3600, &policy).await.unwrap().reaped_count, 1);
    recovered(&admin, e1).await;
    assert!(recovery.sweep(3600, &policy).await.unwrap().wrapped);
    let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
    for limit in [0, 257] {
        assert!(ScopedJobRepository::new(&mut scope)
            .reap_stale_batch(3600, &policy, limit)
            .await
            .is_err());
    }
    scope.rollback().await.unwrap();
    for threshold in [0, u64::MAX] {
        assert!(recovery.sweep(threshold, &policy).await.is_err());
    }

    let locked = running(&admin, a, 7200).await;
    let independent = running(&admin, b, 7200).await;
    let mut lock = admin.begin().await.unwrap();
    sqlx::query("SELECT id FROM public.job_attempt WHERE job_id=$1 FOR UPDATE")
        .bind(locked)
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let page = HostedJobRecovery::new(runtime.clone(), HostedRecoveryLimits::default())
        .await
        .unwrap();
    let result = page.sweep(3600, &policy).await.unwrap();
    assert_eq!(
        (
            result.tenants_visited,
            result.reaped_count,
            result.failed_tenants,
            result.timed_out
        ),
        (3, 1, 1, false)
    );
    recovered(&admin, independent).await;
    assert_eq!(
        state(&admin, locked).await,
        ("running".into(), 0, "running".into()),
        "failed attempt update rolls back queue mutation"
    );
    lock.rollback().await.unwrap();
    assert!(page.sweep(3600, &policy).await.unwrap().wrapped);
    assert_eq!(page.sweep(3600, &policy).await.unwrap().reaped_count, 1);
    recovered(&admin, locked).await;

    let short = HostedJobRecovery::new(
        runtime.clone(),
        HostedRecoveryLimits {
            statement_timeout: Duration::from_millis(1),
            pass_timeout: Duration::from_millis(100),
            ..limits
        },
    )
    .await
    .unwrap();
    let held = runtime.acquire().await.unwrap();
    let timeout = tokio::time::timeout(Duration::from_secs(2), short.sweep(3600, &policy))
        .await
        .unwrap()
        .unwrap();
    assert!(timeout.timed_out);
    assert_eq!(timeout.tenants_visited, 0);
    drop(held);
    let after: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(backend, after);
    let setting: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant',true)")
            .fetch_one(&runtime)
            .await
            .unwrap();
    assert_eq!(setting.as_deref(), Some(""));

    let startup_a = running(&admin, a, 7200).await;
    let startup_b = running(&admin, b, 7200).await;
    let worker = JobWorker::new(
        Database::new(runtime.clone()),
        WorkerConfig::default()
            .with_poll_interval(60000)
            .with_stale_reap_threshold(Duration::from_secs(3600))
            .with_stale_reap_interval(Duration::from_millis(25)),
        None,
    )
    .with_hosted_recovery()
    .await
    .unwrap();
    let mut events = worker.events();
    let handle = worker.start();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), events.recv())
            .await
            .unwrap()
            .unwrap(),
        WorkerEvent::WorkerStarted
    ));
    recovered(&admin, startup_a).await;
    recovered(&admin, startup_b).await;
    let periodic = running(&admin, b, 7200).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while state(&admin, periodic).await.0 == "running" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    recovered(&admin, periodic).await;
    handle.shutdown().await.unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), events.recv())
            .await
            .unwrap()
            .unwrap(),
        WorkerEvent::WorkerStopped
    ));
    for id in [s, d, personal, recent] {
        assert_eq!(state(&admin, id).await.0, "running");
    }
    drop(handle);
    drop(recovery);
    drop(page);
    drop(short);
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("hosted_recovery: non-bypass role; active registry authority; nil/inactive exclusion; oldest-first bounded batches; cursor high-water/wrap/new tenant; failure isolation and rollback; pass deadline; connection reuse; actual worker startup/periodic recovery; stopped event; owned role removed; handler execution not qualified");
}
