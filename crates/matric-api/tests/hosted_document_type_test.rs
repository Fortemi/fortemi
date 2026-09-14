use std::{sync::Arc, time::Duration};

use matric_api::hosted_jobs::HostedDocumentTypeInferenceHandler;
use matric_core::{ArchiveRepository, DocumentTypeRepository, JobType, TierGroup};
use matric_db::{Database, PgDocumentTypeRepository, TenantScopedConn};
use matric_jobs::worker::{HostedClaim, HostedDispatchLimits, HostedJobDispatcher};
use matric_jobs::{HostedJobContext, HostedJobHandler, JobResult};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

#[path = "hosted_document_type_test/completion.rs"]
mod completion;

async fn seed(
    admin: &PgPool,
    schema: &str,
    tenant: Uuid,
    note: Uuid,
    original: &str,
    revised: &str,
    filename: Option<&str>,
) {
    let mut tx = admin.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant',$1,true),set_config('search_path',$2,true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema},public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO note(id,tenant_id,format,source,created_at_utc,updated_at_utc,metadata) VALUES($1,$2,'markdown','owned-handler-fixture',NOW(),NOW(),$3)")
        .bind(note).bind(tenant).bind(json!({"source_file":filename})).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO note_original(id,note_id,content,hash) VALUES($1,$2,$3,'fixture')")
        .bind(Uuid::new_v4())
        .bind(note)
        .bind(original)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO note_revised_current(note_id,content) VALUES($1,$2)")
        .bind(note)
        .bind(revised)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn mint(
    admin: &PgPool,
    runtime: &PgPool,
    tenant: Uuid,
    schema: &str,
    note: Uuid,
) -> Arc<HostedClaim> {
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'document_type_inference',0,$4)")
        .bind(job).bind(tenant).bind(note).bind(json!({"schema":schema})).execute(admin).await.unwrap();
    let dispatcher = HostedJobDispatcher::new(runtime.clone(), HostedDispatchLimits::default())
        .await
        .unwrap();
    let pass = dispatcher
        .claim_next(
            TierGroup::CpuAndAgnostic,
            &[JobType::DocumentTypeInference],
            &[],
        )
        .await
        .unwrap();
    assert_eq!(pass.failed_tenants, 0);
    let claim = pass.claim.unwrap();
    assert_eq!(claim.job().id, job);
    claim
}

async fn execute(claim: &Arc<HostedClaim>) -> Value {
    let handler: Box<dyn HostedJobHandler> = Box::new(HostedDocumentTypeInferenceHandler);
    assert_eq!(handler.job_type(), JobType::DocumentTypeInference);
    match handler
        .execute(HostedJobContext::from_claim(claim.clone()))
        .await
    {
        JobResult::Success(Some(value)) => value,
        result => {
            let job = claim.job().id;
            let note = claim.job().note_id.unwrap();
            let diagnostic: matric_core::Result<Option<()>> = claim
                .with_content(move |scope| {
                    Box::pin(async move {
                        PgDocumentTypeRepository::infer_note_scoped(scope, job, note).await?;
                        Err(matric_core::Error::InvalidInput(
                            "diagnostic operation succeeded; rolled back".into(),
                        ))
                    })
                })
                .await;
            if let Err(matric_core::Error::Database(error)) = &diagnostic {
                if let Some(db_error) = error.as_database_error() {
                    panic!(
                        "owned fixture SQL failure: code={:?}; message={}",
                        db_error.code(),
                        db_error.message().chars().take(512).collect::<String>()
                    );
                }
            }
            panic!("hosted document type result: {result:?}; owned diagnostic: {diagnostic:?}");
        }
    }
}

async fn state(admin: &PgPool, schema: &str, note: Uuid) -> Value {
    sqlx::query_scalar(&format!(
        "SELECT jsonb_build_object('type',document_type_id,'access',access_count,
        'activities',(SELECT count(*) FROM {schema}.provenance_activity WHERE note_id=$1),
        'access_logs',(SELECT count(*) FROM {schema}.note_access_log WHERE note_id=$1),
        'members',(SELECT count(*) FROM {schema}.embedding_set_member WHERE note_id=$1))
        FROM {schema}.note WHERE id=$1"
    ))
    .bind(note)
    .fetch_one(admin)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn hosted_document_type_native_handler_atomic_replay_and_followups(admin: PgPool) {
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
            .bind(tenant).bind(format!("doctype-{tenant}")).execute(&admin).await.unwrap();
    }
    let db = Database::new(provision.clone());
    let mut schemas = Vec::new();
    for tenant in [a, b] {
        let archive = db
            .archives
            .create_archive_schema(&format!("doctype_{}", tenant.simple()), None)
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

    // Use the native registry and controlled detection patterns, not a mock detector.
    sqlx::query(
        "UPDATE public.document_type SET tenant_id=$1,recommended_config_id=NULL,is_active=false,
        file_extensions='{}',filename_patterns='{}',mime_types='{}',magic_patterns='{}'",
    )
    .bind(a)
    .execute(&admin)
    .await
    .unwrap();
    for (name, ext) in [("rust", ".rs"), ("yaml", ".yaml"), ("plaintext", ".txt")] {
        assert_eq!(sqlx::query("UPDATE public.document_type SET is_active=true,file_extensions=ARRAY[$2] WHERE name=$1")
            .bind(name).bind(ext).execute(&admin).await.unwrap().rows_affected(),1);
    }
    let [specific, exact, mime, foreign_type] = [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    for (id, tenant, name, patterns, filenames, mimes) in [
        (
            specific,
            a,
            "hosted_specific",
            vec!["MATCH", "SPEC"],
            vec![],
            vec![],
        ),
        (
            exact,
            a,
            "hosted_exact",
            vec!["MATCH"],
            vec!["ExactFile"],
            vec![],
        ),
        (
            mime,
            a,
            "hosted_mime",
            vec![],
            vec![],
            vec!["application/x-hosted"],
        ),
        (
            foreign_type,
            b,
            "hosted_foreign",
            vec!["FOREIGN"],
            vec!["ForeignFile"],
            vec![],
        ),
    ] {
        sqlx::query("INSERT INTO public.document_type(id,tenant_id,name,display_name,category,magic_patterns,filename_patterns,mime_types) VALUES($1,$2,$3,$3,'custom',$4,$5,$6)")
            .bind(id).bind(tenant).bind(name).bind(patterns).bind(filenames).bind(mimes).execute(&admin).await.unwrap();
    }
    let rust: Uuid = sqlx::query_scalar("SELECT id FROM public.document_type WHERE name='rust'")
        .fetch_one(&admin)
        .await
        .unwrap();
    let plain: Uuid =
        sqlx::query_scalar("SELECT id FROM public.document_type WHERE name='plaintext'")
            .fetch_one(&admin)
            .await
            .unwrap();
    let note = Uuid::new_v4();
    for (schema, tenant) in [("public", a), (aa.as_str(), a), (ba.as_str(), b)] {
        seed(
            &admin,
            schema,
            tenant,
            note,
            "original unclassified",
            "MATCH SPEC",
            Some("data.yaml"),
        )
        .await;
    }
    let foreign = Uuid::new_v4();
    seed(
        &admin,
        "public",
        b,
        foreign,
        "FOREIGN",
        "",
        Some("ForeignFile"),
    )
    .await;

    // Activate native full auto-refresh sets after note insertion, so assignment
    // must create membership and correctly routed follow-up jobs itself.
    let set = Uuid::new_v4();
    for (schema, tenant) in [("public", a), (aa.as_str(), a), (ba.as_str(), b)] {
        sqlx::query(&format!("INSERT INTO {schema}.embedding_set(id,tenant_id,name,slug,set_type,mode,criteria) VALUES($1,$2,'handler-full','handler-full','full','auto',$3)"))
            .bind(set).bind(tenant).bind(json!({"include_all":true})).execute(&admin).await.unwrap();
    }
    let role = format!("doctype_{}", Uuid::new_v4().simple());
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
        "SELECT,UPDATE ON public.document_type",
    ] {
        sqlx::query(&format!("GRANT {grant} TO {role}"))
            .execute(&admin)
            .await
            .unwrap();
    }
    for schema in ["public", aa, ba] {
        for grant in [format!("USAGE ON SCHEMA {schema}"),
            format!("SELECT,UPDATE ON {schema}.note,{schema}.note_original,{schema}.note_revised_current,{schema}.embedding_set"),
            format!("SELECT,INSERT ON {schema}.provenance_activity,{schema}.note_access_log"),
            format!("SELECT,INSERT,UPDATE,DELETE ON {schema}.embedding_set_member"),
            format!("SELECT ON {schema}.embedding,{schema}.note_tag,{schema}.note_revision,{schema}.link"),
            format!("SELECT,DELETE ON {schema}.shard_embedding_set_bootstrap")] {
            sqlx::query(&format!("GRANT {grant} TO {role}")).execute(&admin).await.unwrap();
        }
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
    let original_path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&runtime)
        .await
        .unwrap();

    // The community detector and scoped detector share all precedence/scoring.
    let legacy = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |c, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant',$1,false)")
                    .bind(a.to_string())
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect_with(
            (*admin.connect_options())
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let detector = PgDocumentTypeRepository::new(legacy.clone());
    let mut scope = TenantScopedConn::begin(&runtime, a).await.unwrap();
    for (filename, content, mime_type, expected, method) in [
        (
            Some("ExactFile"),
            Some("MATCH SPEC"),
            Some("application/x-hosted"),
            exact,
            "filename_pattern",
        ),
        (
            Some("code.rs"),
            Some("MATCH SPEC"),
            Some("application/x-hosted"),
            mime,
            "mime_type",
        ),
        (
            Some("code.RS"),
            Some("MATCH SPEC"),
            None,
            rust,
            "file_extension",
        ),
        (
            Some("data.yaml"),
            Some("MATCH SPEC"),
            None,
            specific,
            "content_pattern+file_extension",
        ),
        (None, Some("MATCH SPEC"), None, specific, "content_pattern"),
        (None, Some("MATCH"), None, exact, "content_pattern"),
        (Some("ForeignFile"), Some("FOREIGN"), None, plain, "default"),
    ] {
        let scoped =
            PgDocumentTypeRepository::detect_scoped(&mut scope, filename, content, mime_type)
                .await
                .unwrap()
                .unwrap();
        let old = detector
            .detect(filename, content, mime_type)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(scoped.document_type.id, expected);
        assert_eq!(scoped.detection_method, method);
        assert_eq!(
            serde_json::to_value(scoped).unwrap(),
            serde_json::to_value(old).unwrap()
        );
    }
    scope.rollback().await.unwrap();
    legacy.close().await;

    let claim = mint(&admin, &runtime, a, aa, note).await;
    let result = execute(&claim).await;
    assert_eq!(result["detected"], true);
    assert_eq!(
        result["detection_method_len"],
        "content_pattern+file_extension".len()
    );
    assert!(!result.to_string().contains(&specific.to_string()));
    let assigned = state(&admin, aa, note).await;
    assert_eq!(assigned["type"], specific.to_string());
    for field in ["activities", "access", "access_logs", "members"] {
        assert_eq!(assigned[field], 1, "{field}");
    }
    assert_eq!(execute(&claim).await, result);
    assert_eq!(state(&admin, aa, note).await, assigned);
    assert!(matches!(
        claim
            .retry(
                "owned transient",
                matric_core::JobFailureClass::Transient,
                "owned_retry",
                chrono::Utc::now() + chrono::Duration::hours(1)
            )
            .await
            .unwrap(),
        Some(matric_core::JobRetryOutcome::Scheduled { .. })
    ));
    sqlx::query(
        "UPDATE public.job_queue SET next_attempt_at=NOW()-interval '1 second' WHERE id=$1",
    )
    .bind(claim.job().id)
    .execute(&admin)
    .await
    .unwrap();
    let dispatcher = HostedJobDispatcher::new(runtime.clone(), HostedDispatchLimits::default())
        .await
        .unwrap();
    let next = dispatcher
        .claim_next(
            TierGroup::CpuAndAgnostic,
            &[JobType::DocumentTypeInference],
            &[],
        )
        .await
        .unwrap()
        .claim
        .unwrap();
    assert_eq!(next.job().id, claim.job().id);
    assert_ne!(next.attempt_id(), claim.attempt_id());
    assert!(matches!(
        HostedDocumentTypeInferenceHandler
            .execute(HostedJobContext::from_claim(claim.clone()))
            .await,
        JobResult::Failed(_)
    ));
    assert_eq!(execute(&next).await, result);
    assert_eq!(state(&admin, aa, note).await, assigned);
    let claim = next;
    assert!(claim.complete(Some(result.clone())).await.unwrap());
    assert!(matches!(
        HostedDocumentTypeInferenceHandler
            .execute(HostedJobContext::from_claim(claim.clone()))
            .await,
        JobResult::Failed(_)
    ));
    assert_eq!(state(&admin, aa, note).await, assigned);
    for schema in ["public", ba] {
        let untouched = state(&admin, schema, note).await;
        assert_eq!(untouched["type"], Value::Null);
        assert_eq!(untouched["activities"], 0);
        assert_eq!(untouched["members"], 0);
    }
    assert_eq!(state(&admin, "public", foreign).await["activities"], 0);
    let queued: Vec<Value> = sqlx::query_scalar("SELECT payload FROM public.job_queue WHERE tenant_id=$1 AND note_id=$2 AND job_type='embedding'")
        .bind(a).bind(note).fetch_all(&admin).await.unwrap();
    assert_eq!(queued, vec![json!({"schema":aa,"embedding_set_id":set})]);
    let public_claim = mint(&admin, &runtime, a, "public", note).await;
    execute(&public_claim).await;
    assert!(public_claim.complete(None).await.unwrap());
    let queued: Vec<String> = sqlx::query_scalar("SELECT payload->>'schema' FROM public.job_queue WHERE tenant_id=$1 AND note_id=$2 AND job_type='embedding' ORDER BY payload->>'schema'")
        .bind(a).bind(note).fetch_all(&admin).await.unwrap();
    let mut expected = vec![aa.clone(), "public".to_string()];
    expected.sort();
    assert_eq!(queued, expected);

    sqlx::query("UPDATE public.document_type SET magic_patterns=ARRAY['MATCH','SPEC'] WHERE id=$1")
        .bind(foreign_type)
        .execute(&admin)
        .await
        .unwrap();
    let b_claim = mint(&admin, &runtime, b, ba, note).await;
    execute(&b_claim).await;
    assert_eq!(
        state(&admin, ba, note).await["type"],
        foreign_type.to_string()
    );
    assert!(b_claim.complete(None).await.unwrap());
    assert_eq!(state(&admin, aa, note).await, assigned);
    let b_jobs: Vec<Value> = sqlx::query_scalar("SELECT payload FROM public.job_queue WHERE tenant_id=$1 AND note_id=$2 AND job_type='embedding'")
        .bind(b).bind(note).fetch_all(&admin).await.unwrap();
    assert_eq!(b_jobs, vec![json!({"schema":ba,"embedding_set_id":set})]);

    let preview = Uuid::new_v4();
    seed(
        &admin,
        aa,
        a,
        preview,
        &format!("{}MATCH SPEC", "\u{1f642}".repeat(1000)),
        "",
        None,
    )
    .await;
    let c = mint(&admin, &runtime, a, aa, preview).await;
    execute(&c).await;
    assert_eq!(state(&admin, aa, preview).await["type"], plain.to_string());
    assert!(c.complete(None).await.unwrap());

    // Empty, original fallback, already-assigned, missing/tombstone/foreign note.
    for (content, revised, filename, reason) in [
        (" \n\t", "", None, "empty_content"),
        ("MATCH SPEC", "", None, "detected"),
        ("MATCH SPEC", " \n", None, "empty_content"),
    ] {
        let id = Uuid::new_v4();
        seed(&admin, aa, a, id, content, revised, filename).await;
        let c = mint(&admin, &runtime, a, aa, id).await;
        let value = execute(&c).await;
        if reason == "detected" {
            assert_eq!(value["detected"], true);
        } else {
            assert_eq!(value["reason"], reason);
        }
        assert_eq!(execute(&c).await, value);
        assert_eq!(state(&admin, aa, id).await["activities"], 1);
        assert!(c.complete(None).await.unwrap());
    }
    let c = mint(&admin, &runtime, a, aa, note).await;
    assert_eq!(
        execute(&c).await["reason"],
        "document_type_already_assigned"
    );
    assert!(c.complete(None).await.unwrap());
    let tombstone = Uuid::new_v4();
    seed(&admin, aa, a, tombstone, "MATCH SPEC", "", None).await;
    sqlx::query(&format!(
        "UPDATE {aa}.note SET deleted_at=NOW() WHERE id=$1"
    ))
    .bind(tombstone)
    .execute(&admin)
    .await
    .unwrap();
    for (schema, id) in [
        ("public", foreign),
        (aa.as_str(), Uuid::new_v4()),
        (aa.as_str(), tombstone),
    ] {
        let c = mint(&admin, &runtime, a, schema, id).await;
        assert!(matches!(
            HostedDocumentTypeInferenceHandler
                .execute(HostedJobContext::from_claim(c.clone()))
                .await,
            JobResult::Failed(_)
        ));
        assert!(c.complete(None).await.unwrap());
    }

    // Inactive plaintext is not a hosted fallback, even though the legacy API
    // historically permits it. Both explicit/content inactive matches are hidden.
    sqlx::query("UPDATE public.document_type SET is_active=false WHERE tenant_id=$1")
        .bind(a)
        .execute(&admin)
        .await
        .unwrap();
    let no_match = Uuid::new_v4();
    seed(&admin, aa, a, no_match, "MATCH SPEC", "", Some("ExactFile")).await;
    let c = mint(&admin, &runtime, a, aa, no_match).await;
    assert_eq!(execute(&c).await["reason"], "no_match");
    assert_eq!(execute(&c).await["reason"], "no_match");
    assert_eq!(state(&admin, aa, no_match).await["activities"], 1);
    assert!(c.complete(None).await.unwrap());
    sqlx::query("UPDATE public.document_type SET is_active=true WHERE id=$1")
        .bind(specific)
        .execute(&admin)
        .await
        .unwrap();

    // A deferred failure after assignment must roll back note, access, provenance
    // and trigger-created membership/jobs together. It is not a callback-only test.
    let rollback = Uuid::new_v4();
    seed(&admin, aa, a, rollback, "MATCH SPEC", "", None).await;
    sqlx::query(&format!(
        "DELETE FROM {aa}.embedding_set_member WHERE note_id=$1"
    ))
    .bind(rollback)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query("DELETE FROM public.job_queue WHERE note_id=$1")
        .bind(rollback)
        .execute(&admin)
        .await
        .unwrap();
    let c = mint(&admin, &runtime, a, aa, rollback).await;
    sqlx::raw_sql(&format!("CREATE FUNCTION {aa}.reject_receipt() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'owned deferred failure'; END$$;
        CREATE CONSTRAINT TRIGGER reject_receipt AFTER INSERT ON {aa}.provenance_activity DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION {aa}.reject_receipt();"))
        .execute(&admin).await.unwrap();
    let before = state(&admin, aa, rollback).await;
    assert!(matches!(
        HostedDocumentTypeInferenceHandler
            .execute(HostedJobContext::from_claim(c.clone()))
            .await,
        JobResult::Failed(_)
    ));
    assert_eq!(state(&admin, aa, rollback).await, before);
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.job_queue WHERE note_id=$1 AND job_type='embedding'",
    )
    .bind(rollback)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(jobs, 0);
    sqlx::raw_sql(&format!("DROP TRIGGER reject_receipt ON {aa}.provenance_activity; DROP FUNCTION {aa}.reject_receipt()"))
        .execute(&admin).await.unwrap();
    execute(&c).await;
    assert!(c.complete(None).await.unwrap());

    // Hold the real operation before commit and prove direct content editors
    // cannot race the classified snapshot. Cancellation rolls back its receipt.
    let locked = Uuid::new_v4();
    seed(&admin, aa, a, locked, "MATCH SPEC", "", None).await;
    let c = mint(&admin, &runtime, a, aa, locked).await;
    let job = c.job().id;
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let held = c.clone();
    let task = tokio::spawn(async move {
        held.with_content(move |scope| {
            Box::pin(async move {
                let value = PgDocumentTypeRepository::infer_note_scoped(scope, job, locked).await?;
                ready_tx.send(()).unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok(value)
            })
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), ready_rx)
        .await
        .unwrap()
        .unwrap();
    for table in ["note", "note_original", "note_revised_current"] {
        let mut tx = admin.begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout='100ms'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let key = if table == "note" { "id" } else { "note_id" };
        let error = sqlx::query(&format!(
            "SELECT 1 FROM {aa}.{table} WHERE {key}=$1 FOR UPDATE"
        ))
        .bind(locked)
        .fetch_one(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("55P03")
        );
        tx.rollback().await.unwrap();
    }
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(c.update_progress(50, None).await.unwrap());
    assert_eq!(state(&admin, aa, locked).await["activities"], 0);
    execute(&c).await;
    assert!(c.complete(None).await.unwrap());

    // Member statistics use the triggering schema and update both parents on
    // reparent, including inactive/filter/no-create sets which queue no jobs.
    let [x, y, z] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (id, kind, active, rules) in [
        (x, "full", false, json!({})),
        (y, "filter", true, json!({})),
        (z, "full", true, json!({"on_create":false})),
    ] {
        sqlx::query(&format!("INSERT INTO {aa}.embedding_set(id,tenant_id,name,slug,set_type,is_active,auto_refresh,auto_embed_rules)
            VALUES($1,$2,$3,$3,$4::public.embedding_set_type,$5,false,$6)"))
            .bind(id).bind(a).bind(format!("stats-{id}")).bind(kind).bind(active).bind(rules).execute(&admin).await.unwrap();
    }
    let c = mint(&admin, &runtime, a, aa, note).await;
    let jobs_before: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_queue")
        .fetch_one(&admin)
        .await
        .unwrap();
    c.with_content(move |scope| Box::pin(async move {
        sqlx::query("INSERT INTO embedding_set_member(embedding_set_id,note_id) VALUES($1,$2)")
            .bind(x).bind(note).execute(scope.executor()).await?;
        let count:i32=sqlx::query_scalar("SELECT document_count FROM embedding_set WHERE id=$1").bind(x).fetch_one(scope.executor()).await?;
        assert_eq!(count,1);
        sqlx::query("UPDATE embedding_set_member SET embedding_set_id=$1 WHERE embedding_set_id=$2 AND note_id=$3")
            .bind(y).bind(x).bind(note).execute(scope.executor()).await?;
        for (id,expected) in [(x,0),(y,1)] {
            let count:i32=sqlx::query_scalar("SELECT document_count FROM embedding_set WHERE id=$1").bind(id).fetch_one(scope.executor()).await?;
            assert_eq!(count,expected);
        }
        sqlx::query("DELETE FROM embedding_set_member WHERE embedding_set_id=$1 AND note_id=$2")
            .bind(y).bind(note).execute(scope.executor()).await?;
        let count:i32=sqlx::query_scalar("SELECT document_count FROM embedding_set WHERE id=$1").bind(y).fetch_one(scope.executor()).await?;
        assert_eq!(count,0);
        sqlx::query("INSERT INTO embedding_set_member(embedding_set_id,note_id) VALUES($1,$2)")
            .bind(z).bind(note).execute(scope.executor()).await?;
        Ok(())
    })).await.unwrap().unwrap();
    let jobs_after: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_queue")
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(jobs_after, jobs_before);
    assert!(c.complete(None).await.unwrap());

    let revision_note = Uuid::new_v4();
    let revision = Uuid::new_v4();
    seed(
        &admin,
        aa,
        a,
        revision_note,
        "unclassified",
        "MATCH SPEC",
        None,
    )
    .await;
    let mut tx = admin.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(a.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {aa}.note_revision(id,note_id,revision_number,content,created_at_utc) VALUES($1,$2,1,'MATCH SPEC',NOW())"))
        .bind(revision).bind(revision_note).execute(&mut *tx).await.unwrap();
    sqlx::query(&format!(
        "UPDATE {aa}.note_revised_current SET last_revision_id=$1 WHERE note_id=$2"
    ))
    .bind(revision)
    .bind(revision_note)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let c = mint(&admin, &runtime, a, aa, revision_note).await;
    execute(&c).await;
    let linked: Uuid = sqlx::query_scalar(&format!(
        "SELECT revision_id FROM {aa}.provenance_activity WHERE id=$1"
    ))
    .bind(c.job().id)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(linked, revision);
    assert!(c.complete(None).await.unwrap());

    let conflict = Uuid::new_v4();
    seed(&admin, aa, a, conflict, "MATCH SPEC", "", None).await;
    let c = mint(&admin, &runtime, a, aa, conflict).await;
    sqlx::query(&format!("INSERT INTO {aa}.provenance_activity(id,tenant_id,note_id,activity_type) VALUES($1,$2,$3,'document_type_inference')"))
        .bind(c.job().id).bind(a).bind(conflict).execute(&admin).await.unwrap();
    let before = state(&admin, aa, conflict).await;
    assert!(matches!(
        HostedDocumentTypeInferenceHandler
            .execute(HostedJobContext::from_claim(c.clone()))
            .await,
        JobResult::Failed(_)
    ));
    assert_eq!(state(&admin, aa, conflict).await, before);
    assert!(c.complete(None).await.unwrap());

    // Exercise the production worker, not direct invocation, with the same
    // native archive fixtures and restrictive runtime connection as above.
    use matric_jobs::worker::HostedWorkerEventKind;
    use matric_jobs::{JobWorker, NoOpHandler, PauseState, WorkerConfig, WorkerEvent};
    completion::verify_snapshot(&admin, &runtime, a, aa, foreign).await;
    completion::verify_retries(&admin, &runtime, a, aa).await;
    completion::verify_completion_failure(&admin, &runtime, a, aa).await;
    let pause = PauseState::load(admin.clone()).await.unwrap();
    pause.pause_global().await.unwrap();
    pause.pause_archive(aa).await.unwrap();
    let worker_note = Uuid::new_v4();
    let mut worker_jobs = Vec::new();
    for (schema, tenant, content) in [
        (aa.as_str(), a, "MATCH SPEC"),
        (ba.as_str(), b, "FOREIGN"),
        ("public", a, "MATCH SPEC"),
    ] {
        seed(&admin, schema, tenant, worker_note, content, "", None).await;
        completion::seed_metadata(&admin, schema, tenant, worker_note).await;
        let job = Uuid::new_v4();
        sqlx::query("INSERT INTO public.job_queue(id,tenant_id,note_id,job_type,cost_tier,payload) VALUES($1,$2,$3,'document_type_inference',0,$4)")
            .bind(job).bind(tenant).bind(worker_note).bind(json!({"schema":schema})).execute(&admin).await.unwrap();
        worker_jobs.push((job, tenant, schema.to_owned()));
    }
    let worker = JobWorker::new(
        Database::new(runtime.clone()),
        WorkerConfig::default().with_poll_interval(100),
        None,
    )
    .with_hosted_handlers(vec![Arc::new(HostedDocumentTypeInferenceHandler)])
    .await
    .unwrap()
    .with_pause_state(pause.clone());
    assert!(worker.pending_count().await.is_err());
    // A legacy handler must not authorize the pending native follow-up jobs.
    worker
        .register_handler(NoOpHandler::new(JobType::Embedding))
        .await;
    let mut events = worker.events();
    let bus = matric_core::EventBus::new(128);
    let mut wire = bus.subscribe();
    let handle = worker.start();
    assert!(matches!(
        events.recv().await.unwrap(),
        WorkerEvent::WorkerStarted
    ));
    tokio::time::sleep(Duration::from_millis(130)).await;
    assert!(
        events.try_recv().is_err(),
        "globally paused worker must not claim"
    );
    pause.resume_global().await.unwrap();
    let mut completed = std::collections::HashSet::new();
    let mut updated = std::collections::HashSet::new();
    let mut event_counts = std::collections::HashMap::<Uuid, usize>::new();
    for expected in [2, 3] {
        if expected == 3 {
            pause.resume_archive(aa).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while updated.len() < expected {
                let event = events.recv().await.unwrap();
                let WorkerEvent::Hosted(event) = event else {
                    panic!("unscoped worker event");
                };
                let (_, tenant, schema) = worker_jobs
                    .iter()
                    .find(|(id, _, _)| *id == event.job_id())
                    .unwrap();
                assert_eq!(event.tenant_id(), *tenant);
                assert_eq!(event.archive_schema(), schema);
                assert_eq!(event.note_id(), Some(worker_note));
                assert!(!event.attempt_id().is_nil());
                let debug = format!("{event:?}");
                assert!(!debug.contains(&tenant.to_string()));
                assert!(!debug.contains(&event.job_id().to_string()));
                if expected == 2 {
                    assert_ne!(schema, aa);
                }
                *event_counts.entry(event.job_id()).or_default() += 1;
                if matches!(event.kind(), HostedWorkerEventKind::Completed { .. }) {
                    let settled: String =
                        sqlx::query_scalar("SELECT status::text FROM public.job_queue WHERE id=$1")
                            .bind(event.job_id())
                            .fetch_one(&admin)
                            .await
                            .unwrap();
                    assert_eq!(settled, "completed", "terminal event follows commit");
                    completed.insert(event.job_id());
                }
                let is_note = matches!(event.kind(), HostedWorkerEventKind::NoteUpdated { .. });
                if let HostedWorkerEventKind::NoteUpdated { event: snapshot } = event.kind() {
                    assert!(
                        completed.contains(&event.job_id()),
                        "completion precedes note update"
                    );
                    completion::assert_metadata(snapshot, schema, worker_note);
                    assert!(!debug.contains("owned-title"));
                    assert!(!debug.contains("owned-tag"));
                    assert!(updated.insert(event.job_id()));
                }
                matric_api::hosted_jobs::emit_worker_event(&bus, &event);
                let envelope = wire.try_recv().unwrap();
                assert_eq!(envelope.tenant_id, Some(tenant.to_string()));
                assert_eq!(envelope.memory.as_deref(), Some(schema.as_str()));
                assert_eq!(envelope.correlation_id, Some(event.job_id()));
                assert_eq!(
                    envelope.entity_id,
                    Some(if is_note { worker_note } else { event.job_id() }.to_string())
                );
            }
        })
        .await
        .unwrap();
    }
    for (job, _, schema) in &worker_jobs {
        assert_eq!(
            event_counts[job], 5,
            "started, two persisted progress, completed, note updated"
        );
        assert_eq!(state(&admin, schema, worker_note).await["access"], json!(1));
        assert_eq!(
            state(&admin, schema, worker_note).await["activities"],
            json!(1)
        );
    }
    let followups: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.job_queue WHERE job_type='embedding' AND status='pending'",
    )
    .fetch_one(&admin)
    .await
    .unwrap();
    assert!(followups > 0);
    let followup_attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_attempt a JOIN public.job_queue q ON q.id=a.job_id WHERE q.job_type='embedding'")
        .fetch_one(&admin).await.unwrap();
    assert_eq!(followup_attempts, 0);
    tokio::time::timeout(Duration::from_secs(6), handle.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        events.recv().await.unwrap(),
        WorkerEvent::WorkerStopped
    ));
    handle.shutdown().await.unwrap();
    println!("hosted_worker_native: production registry and claim drain; global/archive pause; public and two-tenant archive events; committed progress and settlement; legacy followups remain pending; shutdown acknowledged");

    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&runtime)
            .await
            .unwrap(),
        backend
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SHOW search_path")
            .fetch_one(&runtime)
            .await
            .unwrap(),
        original_path
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT current_setting('app.current_tenant')")
            .fetch_one(&runtime)
            .await
            .unwrap(),
        ""
    );
    runtime.close().await;
    provision.close().await;
    sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("hosted_document_type: production pool-free handler; native detector precedence; active tenant configuration; public/archive isolation; atomic assignment/access/provenance; job-bound replay; native active-set follow-ups; deferred commit rollback; source locks and cancellation; membership reparent/delete statistics and inactive/filter/no-create guards; revision provenance; conflicting receipt denied; same connection restored; owned role removed; production claim drain and scoped event bridge");
}
