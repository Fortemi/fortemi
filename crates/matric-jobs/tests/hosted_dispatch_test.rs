use std::{sync::Arc, time::Duration};

use matric_core::{JobFailureClass, JobRetryOutcome, JobType, TierGroup};
use matric_db::Database;
use matric_jobs::worker::{HostedClaim, HostedDispatchLimits, HostedJobDispatcher};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

async fn pending(
    admin: &PgPool,
    tenant: Uuid,
    tier: i16,
    kind: JobType,
    payload: Option<Value>,
    priority: i32,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,job_type,status,priority,cost_tier,payload) VALUES($1,$2,$3::job_type,'pending',$4,$5,$6)")
        .bind(id).bind(tenant).bind(kind.as_str()).bind(priority).bind(tier).bind(payload).execute(admin).await.unwrap();
    id
}

async fn state(admin: &PgPool, id: Uuid) -> (String, i64) {
    sqlx::query_as("SELECT status::text,(SELECT count(*) FROM public.job_attempt WHERE job_id=q.id) FROM public.job_queue q WHERE id=$1")
        .bind(id).fetch_one(admin).await.unwrap()
}

async fn dispatcher(runtime: &PgPool) -> HostedJobDispatcher {
    HostedJobDispatcher::new(runtime.clone(), HostedDispatchLimits::default())
        .await
        .unwrap()
}

async fn claim(
    dispatcher: &HostedJobDispatcher,
    tier: TierGroup,
    kind: JobType,
) -> Arc<HostedClaim> {
    let r = dispatcher.claim_next(tier, &[kind], &[]).await.unwrap();
    assert!(!r.timed_out);
    assert_eq!(r.failed_tenants, 0);
    r.claim.expect("expected committed claim")
}

#[sqlx::test(migrations = false)]
async fn hosted_dispatch_commits_fences_bounds_and_authority(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let [a, b, s, d, e] = [10, 20, 30, 40, 50].map(Uuid::from_u128);
    for (tenant, status) in [
        (a, "active"),
        (b, "active"),
        (s, "suspended"),
        (d, "soft_deleted"),
    ] {
        sqlx::query(
            "INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,$3)",
        )
        .bind(tenant)
        .bind(format!("dispatch-{tenant}"))
        .bind(status)
        .execute(&admin)
        .await
        .unwrap();
    }
    let role = format!("dispatch_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for grant in ["USAGE ON SCHEMA public","SELECT ON public.tenant_registry,public.archive_registry,public.job_queue,public.job_attempt,public.job_history","UPDATE ON public.job_queue,public.job_attempt","INSERT ON public.job_attempt,public.job_history"] {
        sqlx::query(&format!("GRANT {grant} TO {role}")).execute(&admin).await.unwrap();
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
    let limits = HostedDispatchLimits {
        tenants_per_pass: 1,
        ..HostedDispatchLimits::default()
    };
    assert!(HostedJobDispatcher::new(admin.clone(), limits)
        .await
        .is_err());
    let dispatch = HostedJobDispatcher::new(runtime.clone(), limits)
        .await
        .unwrap();
    let cpu = TierGroup::CpuAndAgnostic;
    let kind = JobType::Embedding;
    let kinds = [kind];
    for (types, excluded) in [
        (vec![], vec![]),
        (vec![kind; JobType::ALL.len() + 1], vec![]),
        (vec![kind], vec!["public".into(); 257]),
        (vec![kind], vec!["x".repeat(64)]),
    ] {
        assert!(dispatch.claim_next(cpu, &types, &excluded).await.is_err());
    }
    let a1 = pending(&admin, a, 0, kind, None, 100).await;
    let a2 = pending(&admin, a, 0, kind, Some(json!({})), 90).await;
    let b1 = pending(&admin, b, 0, kind, None, 1).await;
    let fast = pending(&admin, a, 1, kind, None, 1).await;
    let excluded = [
        pending(&admin, s, 0, kind, None, 1000).await,
        pending(&admin, d, 0, kind, None, 1000).await,
        pending(&admin, Uuid::nil(), 0, kind, None, 1000).await,
        pending(&admin, a, 0, kind, Some(json!({"tenant_id":b})), 1000).await,
        pending(
            &admin,
            a,
            0,
            kind,
            Some(json!({"schema":"archive_not_owned"})),
            1000,
        )
        .await,
        pending(&admin, a, 0, JobType::TitleGeneration, None, 1000).await,
    ];
    let paused = dispatcher(&runtime)
        .await
        .claim_next(cpu, &kinds, &["public".into()])
        .await
        .unwrap();
    assert!(paused.claim.is_none());
    assert_eq!(state(&admin, a1).await, ("pending".into(), 0));
    let ca = claim(&dispatch, cpu, kind).await;
    assert_eq!(ca.job().id, a1);
    assert_eq!(ca.tenant_id(), a);
    assert_eq!(ca.archive_schema(), "public");
    assert_eq!(ca.attempt_number(), 1);
    assert_eq!(
        state(&admin, a1).await,
        ("running".into(), 1),
        "claim committed before returning capability"
    );
    let cf = claim(&dispatch, TierGroup::FastGpu, kind).await;
    assert_eq!(cf.job().id, fast, "cost-tier cursors must be independent");
    assert!(cf.complete(None).await.unwrap());
    sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,'dispatch-late','dispatch-late','active')").bind(e).execute(&admin).await.unwrap();
    let e1 = pending(&admin, e, 0, kind, None, 1000).await;
    let cb = claim(&dispatch, cpu, kind).await;
    assert_eq!(
        cb.job().id,
        b1,
        "backlogged tenant A must not starve tenant B"
    );
    assert!(dispatch.claim_next(cpu, &kinds, &[]).await.unwrap().wrapped);
    assert_eq!(state(&admin, e1).await, ("pending".into(), 0));
    let ca2 = claim(&dispatch, cpu, kind).await;
    assert_eq!(ca2.job().id, a2);
    assert!(dispatch
        .claim_next(cpu, &kinds, &[])
        .await
        .unwrap()
        .claim
        .is_none());
    let ce = claim(&dispatch, cpu, kind).await;
    assert_eq!(ce.job().id, e1);
    for id in excluded {
        assert_eq!(state(&admin, id).await, ("pending".into(), 0));
    }

    let late = ca.clone();
    assert!(ca
        .update_progress(50, Some("bounded progress"))
        .await
        .unwrap());
    assert!(ca.update_progress(101, None).await.is_err());
    assert!(ca.complete(Some(json!({"done":true}))).await.unwrap());
    assert!(!late.update_progress(99, Some("late")).await.unwrap());
    assert!(!late.complete(None).await.unwrap());
    assert!(!late
        .fail("late", JobFailureClass::Permanent, "late")
        .await
        .unwrap());
    assert!(ca2
        .fail("permanent", JobFailureClass::Permanent, "test_permanent")
        .await
        .unwrap());
    let scheduled = cb
        .retry(
            "transient",
            JobFailureClass::Transient,
            "test_transient",
            chrono::Utc::now() + chrono::Duration::hours(1),
        )
        .await
        .unwrap();
    assert!(matches!(scheduled, Some(JobRetryOutcome::Scheduled { .. })));
    assert!(cb
        .retry(
            "late",
            JobFailureClass::Transient,
            "late",
            chrono::Utc::now() + chrono::Duration::hours(1)
        )
        .await
        .unwrap()
        .is_none());
    sqlx::query(
        "UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second' WHERE id=$1",
    )
    .bind(b1)
    .execute(&admin)
    .await
    .unwrap();
    let replacement = claim(&dispatcher(&runtime).await, cpu, kind).await;
    assert_eq!(replacement.job().id, b1);
    assert_ne!(replacement.attempt_id(), cb.attempt_id());
    assert_eq!(replacement.attempt_number(), 2);
    assert!(!cb.complete(None).await.unwrap());
    assert!(replacement.complete(None).await.unwrap());
    sqlx::query("UPDATE public.job_queue SET max_retries=0 WHERE id=$1")
        .bind(e1)
        .execute(&admin)
        .await
        .unwrap();
    assert!(matches!(
        ce.retry(
            "exhaust",
            JobFailureClass::Transient,
            "test_exhaust",
            chrono::Utc::now() + chrono::Duration::hours(1)
        )
        .await
        .unwrap(),
        Some(JobRetryOutcome::Exhausted)
    ));

    // A commit-time failure must not publish a capability or hide another tenant.
    let failed_a = pending(&admin, a, 0, kind, None, 2000).await;
    let valid_b = pending(&admin, b, 0, kind, None, 2000).await;
    sqlx::query(&format!("CREATE FUNCTION public.dispatch_reject_attempt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{a}'::uuid THEN RAISE EXCEPTION 'owned attempt rejection'; END IF; RETURN NEW; END $$")).execute(&admin).await.unwrap();
    sqlx::query("CREATE CONSTRAINT TRIGGER dispatch_reject_attempt AFTER INSERT ON public.job_attempt DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.dispatch_reject_attempt()").execute(&admin).await.unwrap();
    let partial = dispatcher(&runtime)
        .await
        .claim_next(cpu, &kinds, &[])
        .await
        .unwrap();
    assert_eq!(
        (
            partial.tenants_visited,
            partial.failed_tenants,
            partial.timed_out
        ),
        (2, 1, false)
    );
    let valid = partial.claim.unwrap();
    assert_eq!(valid.job().id, valid_b);
    assert_eq!(state(&admin, failed_a).await, ("pending".into(), 0));
    assert!(valid.complete(None).await.unwrap());
    sqlx::query("DROP TRIGGER dispatch_reject_attempt ON public.job_attempt")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION public.dispatch_reject_attempt()")
        .execute(&admin)
        .await
        .unwrap();
    let locked = claim(&dispatcher(&runtime).await, cpu, kind).await;
    assert_eq!(locked.job().id, failed_a);
    let mut lock = admin.begin().await.unwrap();
    sqlx::query("SELECT id FROM public.job_queue WHERE id=$1 FOR UPDATE")
        .bind(failed_a)
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    assert!(locked.complete(None).await.is_err());
    assert_eq!(state(&admin, failed_a).await, ("running".into(), 1));
    lock.rollback().await.unwrap();
    sqlx::query("UPDATE public.tenant_registry SET status='suspended' WHERE id=$1")
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();
    assert!(locked.complete(None).await.is_err());
    sqlx::query("UPDATE public.tenant_registry SET status='active' WHERE id=$1")
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();
    assert!(locked.complete(None).await.unwrap());

    // Two independent dispatchers share database claim locks, not process cursors.
    let one = pending(&admin, a, 0, JobType::Linking, None, 1).await;
    let two = pending(&admin, a, 0, JobType::Linking, None, 1).await;
    let other = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*admin.connect_options())
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let first = dispatcher(&runtime).await;
    let second = dispatcher(&other).await;
    let (first, second) = tokio::join!(
        claim(&first, cpu, JobType::Linking),
        claim(&second, cpu, JobType::Linking)
    );
    let mut ids = [first.job().id, second.job().id];
    ids.sort();
    let mut expected = [one, two];
    expected.sort();
    assert_eq!(ids, expected);
    assert_ne!(first.attempt_id(), second.attempt_id());
    assert!(first.complete(None).await.unwrap());
    assert!(second.complete(None).await.unwrap());

    let short = HostedJobDispatcher::new(
        runtime.clone(),
        HostedDispatchLimits {
            statement_timeout: Duration::from_millis(100),
            pass_timeout: Duration::from_millis(200),
            ..limits
        },
    )
    .await
    .unwrap();
    let bounded_id = pending(&admin, a, 0, kind, None, 3000).await;
    let bounded = claim(&short, cpu, kind).await;
    assert_eq!(bounded.job().id, bounded_id);
    let held = runtime.acquire().await.unwrap();
    let pass = short.claim_next(cpu, &kinds, &[]);
    tokio::pin!(pass);
    tokio::select! { biased; _ = &mut pass => panic!("pool acquisition should wait"), _ = tokio::time::sleep(Duration::from_millis(10)) => {} }
    assert!(
        short.claim_next(cpu, &kinds, &[]).await.is_err(),
        "same-tier overlap must not queue unbounded work"
    );
    let timeout = tokio::time::timeout(Duration::from_secs(2), &mut pass)
        .await
        .unwrap()
        .unwrap();
    assert!(timeout.timed_out);
    assert!(timeout.claim.is_none());
    assert_eq!(timeout.tenants_visited, 0);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), bounded.complete(None))
            .await
            .unwrap()
            .is_err(),
        "settlement acquisition deadline is an error, not lost-claim success"
    );
    assert_eq!(state(&admin, bounded_id).await, ("running".into(), 1));
    drop(held);
    assert!(bounded.complete(None).await.unwrap());
    let registry_error = dispatcher(&runtime).await;
    sqlx::query(&format!(
        "REVOKE SELECT ON public.tenant_registry FROM {role}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    assert!(
        registry_error.claim_next(cpu, &kinds, &[]).await.is_err(),
        "registry failure must not become an empty successful page"
    );
    sqlx::query(&format!("GRANT SELECT ON public.tenant_registry TO {role}"))
        .execute(&admin)
        .await
        .unwrap();
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
    // Dispatcher/capability clones contain no live transaction and do not prevent close.
    other.close().await;
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("hosted_dispatch: non-bypass role; explicit nonempty handler allowlist; active registry authority; nil/inactive/foreign routing excluded; public pause; committed capability; independent tier cursors; fairness/high-water/wrap; bounded timeout and overlap; deferred commit failure rollback; registry failure explicit; settlement acquisition deadline; cross-dispatcher unique claims; UUID-fenced progress/complete/fail/retry/exhaustion; late and suspended callbacks rejected; same connection reused; owned role removed; production handlers not wired");
}
