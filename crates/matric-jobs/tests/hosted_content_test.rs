use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use matric_core::{ArchiveRepository, Error, JobRetryPolicy, JobType, TierGroup};
use matric_db::Database;
use matric_jobs::worker::{
    HostedClaim, HostedDispatchLimits, HostedJobDispatcher, HostedJobRecovery, HostedRecoveryLimits,
};
use matric_jobs::{HostedJobContext, HostedJobHandler, JobResult};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::oneshot;
use uuid::Uuid;

// A handler-contract fixture with real native writes, not a migrated production handler.
struct RenameHandler;

#[async_trait]
impl HostedJobHandler for RenameHandler {
    fn job_type(&self) -> JobType {
        JobType::Linking
    }
    async fn execute(&self, ctx: HostedJobContext) -> JobResult {
        let Some(note) = ctx.note_id() else {
            return JobResult::Failed("missing_note".into());
        };
        match ctx
            .with_content(move |scope| {
                Box::pin(async move {
                    let rows = sqlx::query("UPDATE note SET title='handler-committed' WHERE id=$1")
                        .bind(note)
                        .execute(scope.executor())
                        .await
                        .map_err(Error::Database)?
                        .rows_affected();
                    Ok(rows)
                })
            })
            .await
        {
            Ok(Some(1)) => JobResult::Success(Some(json!({"updated":true}))),
            _ => JobResult::Failed("content_not_committed".into()),
        }
    }
}

async fn seed(admin: &PgPool, schema: &str, tenant: Uuid, note: Uuid, title: &str) {
    let mut tx = admin.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema},public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO note(id,tenant_id,format,source,created_at_utc,updated_at_utc,title) VALUES($1,$2,'markdown','owned-content-fixture',NOW(),NOW(),$3)")
        .bind(note).bind(tenant).bind(title).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

async fn title(admin: &PgPool, schema: &str, note: Uuid) -> String {
    sqlx::query_scalar(&format!("SELECT title FROM {schema}.note WHERE id=$1"))
        .bind(note)
        .fetch_one(admin)
        .await
        .unwrap()
}

async fn mint(
    admin: &PgPool,
    runtime: &PgPool,
    tenant: Uuid,
    schema: &str,
    note: Uuid,
    limit: Duration,
) -> Arc<HostedClaim> {
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'linking',0,$4)")
        .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).execute(admin).await.unwrap();
    let dispatcher = HostedJobDispatcher::new(
        runtime.clone(),
        HostedDispatchLimits {
            statement_timeout: limit,
            ..HostedDispatchLimits::default()
        },
    )
    .await
    .unwrap();
    let result = dispatcher
        .claim_next(TierGroup::CpuAndAgnostic, &[JobType::Linking], &[])
        .await
        .unwrap();
    assert_eq!(result.failed_tenants, 0);
    let claim = result.claim.unwrap();
    assert_eq!(claim.job().id, job);
    claim
}

#[sqlx::test(migrations = false)]
async fn hosted_content_scope_fences_atomicity_and_handler_contract(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let provision = PgPoolOptions::new().max_connections(1).after_connect(|c,_| Box::pin(async move {
        sqlx::query("SELECT set_config('app.current_tenant','00000000-0000-0000-0000-000000000000',false)").execute(c).await?;Ok(())
    })).connect_with((*admin.connect_options()).clone()).await.unwrap();
    let [a, b] = [Uuid::new_v4(), Uuid::new_v4()];
    for tenant in [a, b] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("content-{tenant}")).execute(&admin).await.unwrap();
    }
    let db = Database::new(provision.clone());
    let mut schemas = Vec::new();
    for tenant in [a, b] {
        let archive = db
            .archives
            .create_archive_schema(&format!("content_{}", tenant.simple()), None)
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
    let (aa, ba) = (&schemas[0], &schemas[1]);
    let note = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    seed(&admin, "public", a, note, "public-original").await;
    seed(&admin, aa, a, note, "archive-original").await;
    seed(&admin, ba, b, note, "foreign-archive").await;
    seed(&admin, "public", b, foreign, "foreign-public").await;
    let role = format!("content_{}", Uuid::new_v4().simple());
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
    for schema in ["public", aa, ba] {
        sqlx::query(&format!("GRANT USAGE ON SCHEMA {schema} TO {role}"))
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(&format!("GRANT SELECT,UPDATE ON {schema}.note TO {role}"))
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(&format!("GRANT SELECT ON {schema}.embedding_set TO {role}"))
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
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    let original_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&runtime)
        .await
        .unwrap();
    let normal = Duration::from_secs(2);
    let claim = mint(&admin, &runtime, a, aa, note, normal).await;
    let ctx = HostedJobContext::from_claim(claim.clone());
    assert_eq!(ctx.tenant_id(), a);
    assert_eq!(ctx.archive_schema(), aa);
    assert_eq!(ctx.note_id(), Some(note));
    assert_eq!(ctx.payload().unwrap()["schema"], aa.as_str());
    assert!(ctx
        .report_progress(25, Some("bounded handler progress"))
        .await
        .unwrap());
    let handler: Box<dyn HostedJobHandler> = Box::new(RenameHandler);
    assert_eq!(handler.job_type(), JobType::Linking);
    assert!(matches!(handler.execute(ctx).await, JobResult::Success(_)));
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    assert_eq!(title(&admin, "public", note).await, "public-original");
    assert_eq!(title(&admin, ba, note).await, "foreign-archive");
    assert_eq!(title(&admin, "public", foreign).await, "foreign-public");

    // RLS still excludes foreign rows even when explicitly qualifying public.
    assert_eq!(
        claim
            .with_content(move |scope| Box::pin(async move {
                let rows = sqlx::query("UPDATE public.note SET title='forbidden' WHERE id=$1")
                    .bind(foreign)
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?
                    .rows_affected();
                Ok(rows)
            }))
            .await
            .unwrap(),
        Some(0)
    );
    let failed = claim
        .with_content(move |scope| {
            Box::pin(async move {
                sqlx::query("UPDATE note SET title='must-rollback' WHERE id=$1")
                    .bind(note)
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?;
                Err::<(), _>(Error::InvalidInput("owned handler failure".into()))
            })
        })
        .await;
    assert!(failed.is_err());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");

    let drift = claim
        .with_content(move |scope| {
            Box::pin(async move {
                sqlx::query("UPDATE note SET title='scope-drift' WHERE id=$1")
                    .bind(note)
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?;
                sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
                    .bind(b.to_string())
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?;
                Ok(())
            })
        })
        .await;
    assert!(drift.is_err());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    let path_drift = claim
        .with_content(move |scope| {
            Box::pin(async move {
                sqlx::query("UPDATE note SET title='path-drift' WHERE id=$1")
                    .bind(note)
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?;
                sqlx::query("SET LOCAL search_path=public")
                    .execute(scope.executor())
                    .await
                    .map_err(Error::Database)?;
                Ok(())
            })
        })
        .await;
    assert!(path_drift.is_err());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");

    // A temporary shadow must not replace the actual claimed archive table.
    sqlx::query("CREATE TEMP TABLE note(id uuid,title text)")
        .execute(&runtime)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pg_temp.note VALUES($1,'temporary-shadow')")
        .bind(note)
        .execute(&runtime)
        .await
        .unwrap();
    assert_eq!(
        claim
            .with_content(move |scope| Box::pin(async move {
                sqlx::query_scalar::<_, String>("SELECT title FROM note WHERE id=$1")
                    .bind(note)
                    .fetch_one(scope.executor())
                    .await
                    .map_err(Error::Database)
            }))
            .await
            .unwrap(),
        Some("handler-committed".into())
    );
    sqlx::query("DROP TABLE pg_temp.note")
        .execute(&runtime)
        .await
        .unwrap();

    // Missing inventory must fail before closure entry, never fall through to public.
    sqlx::query(&format!("ALTER TABLE {aa}.note RENAME TO note_held"))
        .execute(&admin)
        .await
        .unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let flag = entered.clone();
    assert!(claim
        .with_content(move |_| Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .await
        .is_err());
    assert!(!entered.load(Ordering::SeqCst));
    sqlx::query(&format!("ALTER TABLE {aa}.note_held RENAME TO note"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "ALTER TABLE {aa}.note NO FORCE ROW LEVEL SECURITY"
    ))
    .execute(&admin)
    .await
    .unwrap();
    assert!(claim
        .with_content(|_| Box::pin(async { Ok(()) }))
        .await
        .is_err());
    sqlx::query(&format!("ALTER TABLE {aa}.note FORCE ROW LEVEL SECURITY"))
        .execute(&admin)
        .await
        .unwrap();

    // While bounded content work holds the claim locks, stale recovery skips it.
    sqlx::query("CREATE FUNCTION public.content_reject_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.title='commit-failure' THEN RAISE EXCEPTION 'owned commit rejection'; END IF; RETURN NEW; END $$").execute(&admin).await.unwrap();
    sqlx::query(&format!("CREATE CONSTRAINT TRIGGER content_reject_commit AFTER UPDATE ON {aa}.note DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.content_reject_commit()")).execute(&admin).await.unwrap();
    assert!(claim
        .with_content(move |scope| Box::pin(async move {
            sqlx::query("UPDATE note SET title='commit-failure' WHERE id=$1")
                .bind(note)
                .execute(scope.executor())
                .await
                .map_err(Error::Database)?;
            Ok(())
        }))
        .await
        .is_err());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    sqlx::query(&format!("DROP TRIGGER content_reject_commit ON {aa}.note"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION public.content_reject_commit()")
        .execute(&admin)
        .await
        .unwrap();

    sqlx::query("UPDATE public.job_queue SET started_at=NOW()-interval '2 hours' WHERE id=$1")
        .bind(claim.job().id)
        .execute(&admin)
        .await
        .unwrap();
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let held_claim = claim.clone();
    let task = tokio::spawn(async move {
        held_claim
            .with_content(move |scope| {
                Box::pin(async move {
                    sqlx::query("UPDATE note SET title='held-content' WHERE id=$1")
                        .bind(note)
                        .execute(scope.executor())
                        .await
                        .map_err(Error::Database)?;
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    Ok(())
                })
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    let recovery = HostedJobRecovery::new(other.clone(), HostedRecoveryLimits::default())
        .await
        .unwrap();
    let report = recovery
        .sweep(3600, &JobRetryPolicy::default())
        .await
        .unwrap();
    assert_eq!(report.reaped_count, 0);
    assert_eq!(report.failed_tenants, 0);
    // Revoking archive ownership during the callback makes the final fence roll it back.
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$2 WHERE schema_name=$1")
        .bind(aa)
        .bind(b)
        .execute(&admin)
        .await
        .unwrap();
    release_tx.send(()).unwrap();
    assert!(task.await.unwrap().unwrap().is_none());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$2 WHERE schema_name=$1")
        .bind(aa)
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();

    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let suspended_claim = claim.clone();
    let task = tokio::spawn(async move {
        suspended_claim
            .with_content(move |scope| {
                Box::pin(async move {
                    sqlx::query("UPDATE note SET title='suspended-content' WHERE id=$1")
                        .bind(note)
                        .execute(scope.executor())
                        .await
                        .map_err(Error::Database)?;
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    Ok(())
                })
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE public.tenant_registry SET status='suspended' WHERE id=$1")
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();
    release_tx.send(()).unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    sqlx::query("UPDATE public.tenant_registry SET status='active' WHERE id=$1")
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();

    let (entered_tx, entered_rx) = oneshot::channel();
    let (_release_tx, release_rx) = oneshot::channel::<()>();
    let cancel_claim = claim.clone();
    let task = tokio::spawn(async move {
        cancel_claim
            .with_content(move |scope| {
                Box::pin(async move {
                    sqlx::query("UPDATE note SET title='cancelled-content' WHERE id=$1")
                        .bind(note)
                        .execute(scope.executor())
                        .await
                        .map_err(Error::Database)?;
                    entered_tx.send(()).unwrap();
                    let _ = release_rx.await;
                    Ok(())
                })
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered_rx)
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(claim.update_progress(50, None).await.unwrap());
    assert_eq!(title(&admin, aa, note).await, "handler-committed");
    assert!(claim.complete(None).await.unwrap());
    let entered = Arc::new(AtomicBool::new(false));
    let flag = entered.clone();
    assert!(claim
        .with_content(move |_| Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .await
        .unwrap()
        .is_none());
    assert!(!entered.load(Ordering::SeqCst));

    let bounded = mint(
        &admin,
        &runtime,
        a,
        "public",
        note,
        Duration::from_millis(100),
    )
    .await;
    let entered = Arc::new(AtomicBool::new(false));
    let flag = entered.clone();
    assert!(bounded
        .with_content(move |scope| Box::pin(async move {
            sqlx::query("UPDATE note SET title='timed-out-content' WHERE id=$1")
                .bind(note)
                .execute(scope.executor())
                .await
                .map_err(Error::Database)?;
            flag.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(())
        }))
        .await
        .is_err());
    assert!(entered.load(Ordering::SeqCst));
    assert_eq!(title(&admin, "public", note).await, "public-original");
    assert!(bounded.complete(None).await.unwrap());
    let after: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(backend, after);
    let settings: (String, String) = sqlx::query_as(
        "SELECT current_setting('app.current_tenant'),current_setting('search_path')",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_eq!(settings, ("".into(), original_path));
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
    println!("hosted_content: real native note writes through typed hosted handler fixture; committed claim authority; tenant/archive separation; explicit public foreign-row RLS; full content inventory and FORCE RLS; no public fallback; temporary shadow denied; callback error/GUC/path drift rollback; deferred commit failure; recovery blocked by claim locks; archive revocation and tenant suspension final fence; cancellation and timeout rollback; lost claim skips callback; same connection/scope restoration; owned role removed; production handlers and event routing not wired");
}
