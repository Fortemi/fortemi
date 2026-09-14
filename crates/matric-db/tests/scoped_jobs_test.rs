#![cfg(feature = "migrations")]
use matric_core::{
    ArchiveRepository, Error, JobFailureClass, JobRetryOutcome, JobRetryPolicy, JobType, TierGroup,
};
use matric_db::{
    assert_hosted_runtime_role, Database, ScopedClaimedJob, ScopedJobRepository, TenantScopedConn,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

async fn pending(admin: &PgPool, tenant: Uuid, payload: Option<Value>, priority: i32) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,job_type,status,priority,payload,cost_tier) VALUES($1,$2,'embedding','pending',$3,$4,0)")
        .bind(id).bind(tenant).bind(priority).bind(payload).execute(admin).await.unwrap();
    id
}

async fn running(admin: &PgPool, tenant: Uuid, exhausted: bool, stale: bool) -> Uuid {
    let id = pending(
        admin,
        tenant,
        Some(json!({"schema":"public","tenant_id":tenant})),
        1,
    )
    .await;
    sqlx::query("UPDATE public.job_queue SET status='running',started_at=NOW()-($2 * interval '1 second'),max_retries=$3 WHERE id=$1")
        .bind(id).bind(if stale {7200_i32} else {0}).bind(if exhausted {0_i32} else {3}).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO public.job_attempt(id,tenant_id,job_id,attempt_number,started_at) SELECT $1,tenant_id,id,1,started_at FROM public.job_queue WHERE id=$2")
        .bind(Uuid::new_v4()).bind(id).execute(admin).await.unwrap();
    id
}

async fn state(admin: &PgPool, id: Uuid) -> (String, i32, Option<String>) {
    sqlx::query_as("SELECT q.status::text,q.retry_count,(SELECT outcome FROM public.job_attempt WHERE job_id=q.id ORDER BY attempt_number DESC LIMIT 1) FROM public.job_queue q WHERE q.id=$1")
        .bind(id).fetch_one(admin).await.unwrap()
}

async fn set_status(admin: &PgPool, tenant: Uuid, status: &str) {
    sqlx::query("UPDATE public.tenant_registry SET status=$2 WHERE id=$1")
        .bind(tenant)
        .bind(status)
        .execute(admin)
        .await
        .unwrap();
}

async fn claim_id(runtime: &PgPool, tenant: Uuid, id: Uuid) -> ScopedClaimedJob {
    let mut scope = TenantScopedConn::begin(runtime, tenant).await.unwrap();
    let claim = ScopedJobRepository::new(&mut scope)
        .claim_for_tier(TierGroup::CpuAndAgnostic, &[JobType::Embedding], &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.job().id, id);
    scope.commit().await.unwrap();
    claim
}

async fn assert_lost(runtime: &PgPool, tenant: Uuid, claim: &ScopedClaimedJob) {
    let mut scope = TenantScopedConn::begin(runtime, tenant).await.unwrap();
    let mut jobs = ScopedJobRepository::new(&mut scope);
    assert!(!jobs.update_progress(claim, 99, Some("late")).await.unwrap());
    assert!(!jobs
        .complete(claim, Some(json!({"late":true})))
        .await
        .unwrap());
    assert!(!jobs
        .fail(claim, "late", JobFailureClass::Permanent, "late_worker")
        .await
        .unwrap());
    assert!(jobs
        .retry(
            claim,
            "late",
            JobFailureClass::Transient,
            "late_worker",
            chrono::Utc::now() + chrono::Duration::hours(1)
        )
        .await
        .unwrap()
        .is_none());
    scope.commit().await.unwrap();
}

async fn fenced_transitions(
    admin: &PgPool,
    runtime: &PgPool,
    other: &PgPool,
    a: Uuid,
    b: Uuid,
    archive: &str,
    role: &str,
) {
    // Keep earlier recovery fixtures delayed while testing new explicit claims.
    sqlx::query("UPDATE public.job_queue SET next_attempt_at=NOW()+interval '1 hour' WHERE status='pending' AND retry_count>0")
        .execute(admin).await.unwrap();
    let kinds = [JobType::Embedding];
    let tier = TierGroup::CpuAndAgnostic;
    let original_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(runtime)
        .await
        .unwrap();
    let mut shadow = TenantScopedConn::begin(runtime, a).await.unwrap();
    sqlx::query("CREATE TEMP TABLE job_queue(decoy boolean) ON COMMIT DROP")
        .execute(shadow.executor())
        .await
        .unwrap();
    assert!(ScopedJobRepository::new(&mut shadow)
        .claim_for_tier(tier, &kinds, &[])
        .await
        .unwrap()
        .is_none());
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(b.to_string())
        .execute(shadow.executor())
        .await
        .unwrap();
    assert!(matches!(
        ScopedJobRepository::new(&mut shadow)
            .claim_for_tier(tier, &kinds, &[])
            .await,
        Err(Error::InvalidInput(_))
    ));
    shadow.rollback().await.unwrap();
    let restored_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(runtime)
        .await
        .unwrap();
    assert_eq!(
        restored_path, original_path,
        "queue search path is transaction-local"
    );
    for payload in [None, Some(json!({})), Some(json!({"schema":"public"}))] {
        let id = pending(admin, a, payload, 2000).await;
        let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
        assert!(ScopedJobRepository::new(&mut scope)
            .claim_for_tier(tier, &kinds, &["public".into()])
            .await
            .unwrap()
            .is_none());
        scope.commit().await.unwrap();
        assert_eq!(state(admin, id).await, ("pending".into(), 0, None));
        claim_id(runtime, a, id).await;
    }

    let id1 = pending(admin, a, None, 2001).await;
    let id2 = pending(admin, a, None, 2000).await;
    let mut first = TenantScopedConn::begin(runtime, a).await.unwrap();
    let c1 = ScopedJobRepository::new(&mut first)
        .claim_for_tier(tier, &kinds, &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(c1.job().id, id1);
    let mut second = TenantScopedConn::begin(other, a).await.unwrap();
    let c2 = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ScopedJobRepository::new(&mut second).claim_for_tier(tier, &kinds, &[]),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(c2.job().id, id2);
    assert_ne!(c1.attempt_id(), c2.attempt_id());
    second.commit().await.unwrap();
    first.commit().await.unwrap();

    let id = pending(admin, a, None, 2000).await;
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    let abandoned = ScopedJobRepository::new(&mut scope)
        .claim_for_tier(tier, &kinds, &[])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(abandoned.job().id, id);
    scope.rollback().await.unwrap();
    let current = claim_id(runtime, a, id).await;
    assert_eq!(abandoned.attempt_number(), current.attempt_number());
    assert_ne!(abandoned.attempt_id(), current.attempt_id());
    assert_lost(runtime, a, &abandoned).await;
    assert_eq!(
        state(admin, id).await,
        ("running".into(), 0, Some("running".into()))
    );
    let mut foreign = TenantScopedConn::begin(runtime, b).await.unwrap();
    assert!(matches!(
        ScopedJobRepository::new(&mut foreign)
            .complete(&current, None)
            .await,
        Err(Error::InvalidInput(_))
    ));
    foreign.commit().await.unwrap();
    for status in ["suspended", "soft_deleted"] {
        set_status(admin, a, status).await;
        let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
        assert!(matches!(
            ScopedJobRepository::new(&mut scope)
                .complete(&current, None)
                .await,
            Err(Error::InvalidInput(_))
        ));
        scope.rollback().await.unwrap();
    }
    set_status(admin, a, "active").await;
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    let mut jobs = ScopedJobRepository::new(&mut scope);
    for percent in [-1, 101] {
        assert!(jobs.update_progress(&current, percent, None).await.is_err());
    }
    assert!(jobs
        .update_progress(&current, 47, Some("working"))
        .await
        .unwrap());
    assert!(jobs
        .complete(&current, Some(json!({"ok":true})))
        .await
        .unwrap());
    scope.rollback().await.unwrap();
    assert_eq!(
        state(admin, id).await,
        ("running".into(), 0, Some("running".into()))
    );
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.job_history WHERE tenant_id=$1")
            .bind(a)
            .fetch_one(admin)
            .await
            .unwrap();
    assert_eq!(history, 0, "rollback includes history");
    // An actual statement failure must not persist queue/attempt writes either.
    sqlx::query(&format!("REVOKE INSERT ON public.job_history FROM {role}"))
        .execute(admin)
        .await
        .unwrap();
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert!(matches!(
        ScopedJobRepository::new(&mut scope)
            .complete(&current, None)
            .await,
        Err(Error::Database(_))
    ));
    scope.rollback().await.unwrap();
    sqlx::query(&format!("GRANT INSERT ON public.job_history TO {role}"))
        .execute(admin)
        .await
        .unwrap();
    assert_eq!(
        state(admin, id).await,
        ("running".into(), 0, Some("running".into()))
    );
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert!(ScopedJobRepository::new(&mut scope)
        .complete(&current, Some(json!({"ok":true})))
        .await
        .unwrap());
    scope.commit().await.unwrap();
    assert_eq!(
        state(admin, id).await,
        ("completed".into(), 0, Some("completed".into()))
    );
    assert_lost(runtime, a, &current).await;
    let history: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.job_history WHERE tenant_id=$1 AND success",
    )
    .bind(a)
    .fetch_one(admin)
    .await
    .unwrap();
    assert_eq!(history, 1, "completion is exactly once");
    let result: Value = sqlx::query_scalar("SELECT result FROM public.job_queue WHERE id=$1")
        .bind(id)
        .fetch_one(admin)
        .await
        .unwrap();
    assert_eq!(result, json!({"ok":true}));

    let id = pending(admin, a, None, 2000).await;
    let before_reap = claim_id(runtime, a, id).await;
    sqlx::query("UPDATE public.job_queue SET started_at=NOW()-interval '2 hours' WHERE id=$1")
        .bind(id)
        .execute(admin)
        .await
        .unwrap();
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert_eq!(
        ScopedJobRepository::new(&mut scope)
            .reap_stale_running(3600, &JobRetryPolicy::default())
            .await
            .unwrap(),
        1
    );
    scope.commit().await.unwrap();
    assert_lost(runtime, a, &before_reap).await;
    sqlx::query(
        "UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(admin)
    .await
    .unwrap();
    let recovered = claim_id(runtime, a, id).await;
    assert_lost(runtime, a, &before_reap).await;
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert!(ScopedJobRepository::new(&mut scope)
        .complete(&recovered, None)
        .await
        .unwrap());
    let mut contender = TenantScopedConn::begin(other, a).await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(contender.executor())
        .await
        .unwrap();
    let err = ScopedJobRepository::new(&mut contender)
        .update_progress(&recovered, 99, None)
        .await
        .unwrap_err();
    let Error::Database(err) = err else {
        panic!("expected lock timeout")
    };
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
    contender.rollback().await.unwrap();
    scope.commit().await.unwrap();
    assert_lost(other, a, &recovered).await;
    let attempts: Vec<(i32,String)> = sqlx::query_as("SELECT attempt_number,outcome FROM public.job_attempt WHERE job_id=$1 ORDER BY attempt_number")
        .bind(id).fetch_all(admin).await.unwrap();
    assert_eq!(
        attempts,
        vec![(1, "stale_reaped".into()), (2, "completed".into())]
    );

    let id = pending(admin, a, None, 2000).await;
    let old = claim_id(runtime, a, id).await;
    let due = chrono::Utc::now() + chrono::Duration::hours(1);
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    let mut jobs = ScopedJobRepository::new(&mut scope);
    assert!(jobs
        .retry(
            &old,
            "test",
            JobFailureClass::Permanent,
            "test_failure",
            due
        )
        .await
        .is_err());
    assert!(jobs
        .retry(
            &old,
            "test",
            JobFailureClass::Transient,
            "test_failure",
            chrono::Utc::now()
        )
        .await
        .is_err());
    assert!(jobs
        .fail(&old, "test", JobFailureClass::Permanent, "INVALID CODE")
        .await
        .is_err());
    assert!(matches!(
        jobs.retry(
            &old,
            "test",
            JobFailureClass::Transient,
            "test_failure",
            due
        )
        .await
        .unwrap(),
        Some(JobRetryOutcome::Scheduled { .. })
    ));
    scope.commit().await.unwrap();
    assert_lost(runtime, a, &old).await;
    assert_eq!(
        state(admin, id).await,
        ("pending".into(), 1, Some("retry_scheduled".into()))
    );
    sqlx::query(
        "UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(admin)
    .await
    .unwrap();
    let replacement = claim_id(runtime, a, id).await;
    assert_eq!(replacement.attempt_number(), 2);
    assert_lost(runtime, a, &old).await;
    sqlx::query("UPDATE public.job_queue SET max_retries=retry_count WHERE id=$1")
        .bind(id)
        .execute(admin)
        .await
        .unwrap();
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert!(matches!(
        ScopedJobRepository::new(&mut scope)
            .retry(
                &replacement,
                "test",
                JobFailureClass::Transient,
                "test_failure",
                due
            )
            .await
            .unwrap(),
        Some(JobRetryOutcome::Exhausted)
    ));
    scope.commit().await.unwrap();
    assert_eq!(
        state(admin, id).await,
        ("failed".into(), 1, Some("terminal_failed".into()))
    );
    let code: String =
        sqlx::query_scalar("SELECT failure_code FROM public.job_attempt WHERE id=$1")
            .bind(replacement.attempt_id())
            .fetch_one(admin)
            .await
            .unwrap();
    assert_eq!(code, "retry_exhausted");

    let id = pending(admin, a, Some(json!({"schema":archive})), 2000).await;
    let claim = claim_id(runtime, a, id).await;
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$2 WHERE schema_name=$1")
        .bind(archive)
        .bind(b)
        .execute(admin)
        .await
        .unwrap();
    assert_lost(runtime, a, &claim).await;
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$2 WHERE schema_name=$1")
        .bind(archive)
        .bind(a)
        .execute(admin)
        .await
        .unwrap();
    let mut scope = TenantScopedConn::begin(runtime, a).await.unwrap();
    assert!(ScopedJobRepository::new(&mut scope)
        .fail(&claim, "test", JobFailureClass::Permanent, "test_failure")
        .await
        .unwrap());
    scope.commit().await.unwrap();
    assert_eq!(
        state(admin, id).await,
        ("failed".into(), 0, Some("terminal_failed".into()))
    );
    assert_lost(runtime, a, &claim).await;
    println!("scoped_jobs_fences: public pause; concurrent claims; rollback UUID replacement; stale callbacks; cross-tenant; inactive tenant; archive revocation; progress bounds; atomic history; completion; retry; exhaustion; terminal failure; reap/reclaim fencing; competing settlement lock; temporary shadow denial; scope drift denial; local search path restoration");
}

#[sqlx::test(migrations = false)]
async fn scoped_jobs_claim_recovery_admission_and_transaction_isolation(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let provision = PgPoolOptions::new().max_connections(1).after_connect(|c,_| Box::pin(async move {
        sqlx::query("SELECT set_config('app.current_tenant','00000000-0000-0000-0000-000000000000',false)").execute(c).await?;
        Ok(())
    })).connect_with((*admin.connect_options()).clone()).await.unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let inactive = Uuid::new_v4();
    for tenant in [a, b, inactive] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("jobs-{tenant}")).execute(&admin).await.unwrap();
    }
    let db = Database::new(provision.clone());
    let mut schemas = Vec::new();
    for tenant in [a, b] {
        let archive = db
            .archives
            .create_archive_schema(&format!("jobs_{}", tenant.simple()), None)
            .await
            .unwrap();
        sqlx::query("UPDATE public.archive_registry SET tenant_id=$2 WHERE id=$1")
            .bind(archive.id)
            .bind(tenant)
            .execute(&admin)
            .await
            .unwrap();
        schemas.push(archive.schema_name);
    }
    let role = format!("jobs_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for grant in [
        "USAGE ON SCHEMA public",
        "SELECT ON public.tenant_registry,public.archive_registry",
        "SELECT,INSERT,UPDATE ON public.job_queue,public.job_attempt",
        "SELECT,INSERT ON public.job_history",
    ] {
        sqlx::query(&format!("GRANT {grant} TO {role}"))
            .execute(&admin)
            .await
            .unwrap();
    }
    let options = (*admin.connect_options())
        .clone()
        .username(&role)
        .password(&password);
    let runtime = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    let other = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    assert_hosted_runtime_role(&runtime).await.unwrap();
    let backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    let a_public = pending(&admin, a, None, 5).await;
    let a_archive = pending(
        &admin,
        a,
        Some(json!({"schema":schemas[0],"tenant_id":a})),
        4,
    )
    .await;
    let b_public = pending(
        &admin,
        b,
        Some(json!({"schema":"public","tenant_id":b})),
        100,
    )
    .await;
    let mut invalid = Vec::new();
    for payload in [
        json!({"tenant_id":b}),
        json!({"schema":schemas[1]}),
        json!({"schema":"not_registered"}),
        json!({"schema":"public; DROP TABLE job_queue"}),
        json!({"schema":null}),
        json!({"tenant_id":null}),
        json!([]),
    ] {
        invalid.push(pending(&admin, a, Some(payload), 1000).await);
    }
    let kinds = [JobType::Embedding];
    let tier = TierGroup::CpuAndAgnostic;
    {
        let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
        let claim = ScopedJobRepository::new(&mut scope)
            .claim_for_tier(tier, &kinds, &[])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.job().id, a_public);
        assert_eq!(claim.tenant_id(), a);
        assert_eq!(claim.archive_schema(), "public");
        assert_eq!(claim.attempt_number(), 1);
        scope.rollback().await.unwrap();
    }
    assert_eq!(state(&admin, a_public).await, ("pending".into(), 0, None));
    {
        let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
        assert_eq!(
            ScopedJobRepository::new(&mut scope)
                .claim_for_tier(tier, &kinds, &[])
                .await
                .unwrap()
                .unwrap()
                .job()
                .id,
            a_public
        );
        scope.commit().await.unwrap();
    }
    assert_eq!(
        state(&admin, a_public).await,
        ("running".into(), 0, Some("running".into()))
    );
    assert_eq!(state(&admin, b_public).await, ("pending".into(), 0, None));
    {
        let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
        assert!(ScopedJobRepository::new(&mut scope)
            .claim_for_tier(tier, &kinds, &[schemas[0].clone()])
            .await
            .unwrap()
            .is_none());
        let claim = ScopedJobRepository::new(&mut scope)
            .claim_for_tier(tier, &kinds, &[])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claim.job().id, a_archive);
        assert_eq!(claim.archive_schema(), schemas[0]);
        scope.commit().await.unwrap();
    }
    {
        let mut scope = TenantScopedConn::begin(&runtime, b).await.unwrap();
        let foreign: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_queue WHERE id=$1")
            .bind(a_public)
            .fetch_one(scope.executor())
            .await
            .unwrap();
        assert_eq!(foreign, 0);
        assert_eq!(
            ScopedJobRepository::new(&mut scope)
                .claim_for_tier(tier, &kinds, &[])
                .await
                .unwrap()
                .unwrap()
                .job()
                .id,
            b_public
        );
        scope.commit().await.unwrap();
    }
    for id in &invalid {
        assert_eq!(state(&admin, *id).await, ("pending".into(), 0, None));
    }
    let after_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(
        backend_pid, after_pid,
        "same connection reused across tenants"
    );
    let scope_setting: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant',true)")
            .fetch_one(&runtime)
            .await
            .unwrap();
    assert_eq!(scope_setting.as_deref(), Some(""));
    let outside = sqlx::query("SELECT id FROM public.job_queue LIMIT 1")
        .fetch_optional(&runtime)
        .await
        .unwrap_err();
    assert_eq!(
        outside
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("22P02")
    );

    let policy = JobRetryPolicy::default();
    let retry = running(&admin, a, false, true).await;
    let exhausted = running(&admin, a, true, true).await;
    let recent = running(&admin, a, false, false).await;
    let foreign = running(&admin, b, false, true).await;
    {
        let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
        assert_eq!(
            ScopedJobRepository::new(&mut scope)
                .reap_stale_running(3600, &policy)
                .await
                .unwrap(),
            2
        );
        scope.commit().await.unwrap();
    }
    assert_eq!(
        state(&admin, retry).await,
        ("pending".into(), 1, Some("stale_reaped".into()))
    );
    assert_eq!(
        state(&admin, exhausted).await,
        ("failed".into(), 0, Some("terminal_failed".into()))
    );
    assert_eq!(
        state(&admin, recent).await,
        ("running".into(), 0, Some("running".into()))
    );
    assert_eq!(
        state(&admin, foreign).await,
        ("running".into(), 0, Some("running".into()))
    );
    let delayed: bool =
        sqlx::query_scalar("SELECT next_attempt_at>NOW() FROM public.job_queue WHERE id=$1")
            .bind(retry)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert!(delayed);
    {
        let mut scope = TenantScopedConn::begin(&runtime, b).await.unwrap();
        assert_eq!(
            ScopedJobRepository::new(&mut scope)
                .reap_stale_running(3600, &policy)
                .await
                .unwrap(),
            1
        );
        // Dropping the transaction must roll back both queue and attempt writes.
    }
    let mut flush = TenantScopedConn::begin(&runtime, b).await.unwrap();
    assert_eq!(
        ScopedJobRepository::new(&mut flush)
            .reap_stale_running(3600, &policy)
            .await
            .unwrap(),
        1
    );
    flush.commit().await.unwrap();

    let concurrent = running(&admin, a, false, true).await;
    let mut first = TenantScopedConn::begin(&runtime, a).await.unwrap();
    assert_eq!(
        ScopedJobRepository::new(&mut first)
            .reap_stale_running(3600, &policy)
            .await
            .unwrap(),
        1
    );
    let mut second = TenantScopedConn::begin(&other, a).await.unwrap();
    let count = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ScopedJobRepository::new(&mut second).reap_stale_running(3600, &policy),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(count, 0, "concurrent reaper skips locked job");
    second.commit().await.unwrap();
    first.commit().await.unwrap();
    assert_eq!(
        state(&admin, concurrent).await,
        ("pending".into(), 1, Some("stale_reaped".into()))
    );

    let blocked = running(&admin, inactive, false, true).await;
    for status in ["suspended", "soft_deleted"] {
        // Scope construction alone is not admission: recheck after status changes.
        let mut scope = TenantScopedConn::begin(&runtime, inactive).await.unwrap();
        set_status(&admin, inactive, status).await;
        assert!(matches!(
            ScopedJobRepository::new(&mut scope)
                .claim_for_tier(tier, &kinds, &[])
                .await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            ScopedJobRepository::new(&mut scope)
                .reap_stale_running(3600, &policy)
                .await,
            Err(Error::InvalidInput(_))
        ));
        scope.rollback().await.unwrap();
        assert_eq!(
            state(&admin, blocked).await,
            ("running".into(), 0, Some("running".into()))
        );
    }
    let mut missing = TenantScopedConn::begin(&runtime, Uuid::new_v4())
        .await
        .unwrap();
    assert!(ScopedJobRepository::new(&mut missing)
        .reap_stale_running(3600, &policy)
        .await
        .is_err());
    missing.rollback().await.unwrap();
    set_status(&admin, inactive, "active").await;
    let mut active = TenantScopedConn::begin(&runtime, inactive).await.unwrap();
    for invalid in [0, u64::MAX] {
        assert!(ScopedJobRepository::new(&mut active)
            .reap_stale_running(invalid, &policy)
            .await
            .is_err());
    }
    assert_eq!(
        ScopedJobRepository::new(&mut active)
            .reap_stale_running(3600, &policy)
            .await
            .unwrap(),
        1
    );
    active.commit().await.unwrap();

    fenced_transitions(&admin, &runtime, &other, a, b, &schemas[0], &role).await;

    runtime.close().await;
    other.close().await;
    provision.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("scoped_jobs: non-bypass RLS; tenant/archive claims; invalid routing; attempt rollback; stale/exhausted recovery; reuse; concurrent skip-locked; active admission; owned role removed");
}
