//! Signed production authentication and populated archive upgrade qualification.
use super::*;
use matric_core::{CreateNoteRequest, JobRepository};
use matric_jobs::{JobContext, JobHandler, JobResult};

use super::super::legacy_archive;

async fn credential_call(
    app: &Router,
    credential: &str,
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut request = oidc_request(method, path, body, memory);
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {credential}").parse().unwrap(),
    );
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        app.clone().oneshot(request),
    )
    .await
    .expect("bounded legacy archive request")
    .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[sqlx::test(migrations = false)]
async fn external_oidc_legacy_archive_management_uses_configured_owner_and_preserves_gates(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let owner = Uuid::new_v4();
    let url = runtime_url(&admin, owner).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .unwrap();
    let runtime = db.pool.clone();
    let mut state =
        production_state_with_event_bus(db.clone(), owner, Arc::new(EventBus::new(256))).await;
    let disabled = super::super::production_router(state.clone(), None);
    let config = state.external_oidc.as_mut().unwrap();
    config.allow_legacy_tokens = true;
    config.legacy_scope_ceiling = ["read".into(), "admin".into()].into_iter().collect();
    let app = super::super::production_router(state.clone(), None);
    state.external_oidc.as_mut().unwrap().legacy_scope_ceiling =
        ["read".into()].into_iter().collect();
    let ceiling = super::super::production_router(state, None);

    // Mint real persisted credentials through the existing repository paths.
    // HTTP requests use only the non-bypass production runtime and router.
    let oauth = &db.oauth;
    let client = oauth
        .register_client(
            serde_json::from_value(serde_json::json!({
                "client_name":"Legacy archive regression", "scope":"read admin"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let mut keys = Vec::new();
    let mut access_tokens = Vec::new();
    for scope in ["read admin", "read"] {
        keys.push(
            oauth
                .create_api_key(matric_core::CreateApiKeyRequest {
                    name: format!("Legacy {scope}"),
                    description: None,
                    scope: scope.into(),
                    expires_in_days: None,
                })
                .await
                .unwrap()
                .api_key,
        );
        access_tokens.push(
            oauth
                .create_token(&client.client_id, scope, None, false)
                .await
                .unwrap()
                .0,
        );
    }
    let handler = BuildVectorIndexHandler::new(db.clone());
    for (label, permitted, read_only) in [
        ("key", &keys[0], &keys[1]),
        ("access", &access_tokens[0], &access_tokens[1]),
    ] {
        let source = format!("legacy_{label}");
        let clone = format!("legacy_{label}_clone");
        let (status, body) = credential_call(
            &app,
            permitted,
            Method::POST,
            "/api/v1/archives",
            serde_json::json!({"name":source}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{label}: {body}");
        let schema = body["schema_name"].as_str().unwrap().to_owned();
        let (status, body) = credential_call(
            &app,
            permitted,
            Method::POST,
            &format!("/api/v1/archives/{source}/clone"),
            serde_json::json!({"new_name":clone}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{label}: {body}");
        let owners: Vec<Uuid> =
            sqlx::query_scalar("SELECT tenant_id FROM archive_registry WHERE name IN ($1,$2)")
                .bind(&source)
                .bind(&clone)
                .fetch_all(&admin)
                .await
                .unwrap();
        assert_eq!(owners, vec![owner, owner]);

        for (router, credential, expected) in [
            (&app, read_only, StatusCode::FORBIDDEN),
            (&ceiling, permitted, StatusCode::FORBIDDEN),
            (&disabled, permitted, StatusCode::UNAUTHORIZED),
        ] {
            for (method, path, body) in [
                (
                    Method::POST,
                    "/api/v1/archives".to_owned(),
                    serde_json::json!({"name":"legacy_denied"}),
                ),
                (
                    Method::POST,
                    format!("/api/v1/archives/{source}/clone"),
                    serde_json::json!({"new_name":"legacy_denied_clone"}),
                ),
                (
                    Method::DELETE,
                    format!("/api/v1/archives/{clone}"),
                    serde_json::Value::Null,
                ),
            ] {
                let (status, body) =
                    credential_call(router, credential, method, &path, body, None).await;
                assert_eq!(status, expected, "{label}, {path}: {body}");
            }
        }
        let unchanged: bool = sqlx::query_scalar("SELECT
            (SELECT count(*) FROM archive_registry WHERE name IN ($1,$2) AND tenant_id=$3)=2
            AND NOT EXISTS(SELECT 1 FROM archive_registry WHERE name IN ('legacy_denied','legacy_denied_clone'))
            AND to_regnamespace('archive_legacy_denied') IS NULL
            AND to_regnamespace('archive_legacy_denied_clone') IS NULL")
            .bind(&source).bind(&clone).bind(owner).fetch_one(&admin).await.unwrap();
        assert!(
            unchanged,
            "denied credentials must not provision or delete archives"
        );

        // A legacy bearer must supply the verified owner for authorized sync.
        sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name=$1")
            .bind(&source)
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(&format!("DROP TABLE {schema}.note_template"))
            .execute(&admin)
            .await
            .unwrap();
        let (status, body) = credential_call(
            &app,
            permitted,
            Method::GET,
            "/api/v1/notes",
            serde_json::Value::Null,
            Some(&source),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{label}: {body}");
        let restored: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.note_template"))
            .fetch_one(&admin)
            .await
            .unwrap();
        assert!(
            restored,
            "legacy archive use must synchronize through the admin executor"
        );

        // Legacy HTTP provisioning queues index work under the deployment owner.
        // Isolate this archive's pending job, claim with runtime credentials and
        // execute the existing fenced administrative index handler.
        sqlx::query("DELETE FROM job_queue WHERE tenant_id=$1 AND job_type='build_set_index' AND payload->>'schema'<>$2")
            .bind(owner).bind(&schema)
            .execute(&admin)
            .await
            .unwrap();
        let job = db
            .jobs
            .claim_next_for_types(&[matric_core::JobType::BuildSetIndex])
            .await
            .unwrap()
            .unwrap();
        let job_id = job.id;
        let job_owner: Uuid = sqlx::query_scalar("SELECT tenant_id FROM job_queue WHERE id=$1")
            .bind(job_id)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(job_owner, owner);
        assert_eq!(job.payload.as_ref().unwrap()["schema"], schema);
        let JobResult::Success(result) = handler.execute(JobContext::new(job)).await else {
            panic!("legacy index work failed");
        };
        db.jobs.complete(job_id, result).await.unwrap();

        for name in [&clone, &source] {
            let (status, body) = credential_call(
                &app,
                permitted,
                Method::DELETE,
                &format!("/api/v1/archives/{name}"),
                serde_json::Value::Null,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT, "{label}: {body}");
            let absent: bool = sqlx::query_scalar("SELECT to_regnamespace($1) IS NULL AND NOT EXISTS(SELECT 1 FROM archive_registry WHERE name=$2)")
                .bind(format!("archive_{name}")).bind(name).fetch_one(&admin).await.unwrap();
            assert!(absent);
        }
    }
    // Legacy opt-in and its scope ceiling do not change external OIDC admission.
    for router in [&disabled, &ceiling] {
        let (status, body) = call(
            router,
            Method::POST,
            "/api/v1/archives",
            serde_json::json!({"name":"oidc_unchanged"}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let actual: Uuid = sqlx::query_scalar(
            "SELECT tenant_id FROM archive_registry WHERE name='oidc_unchanged'",
        )
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(actual, owner);
        let (status, body) = call(
            router,
            Method::DELETE,
            "/api/v1/archives/oidc_unchanged",
            serde_json::Value::Null,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    }
    drop((app, disabled, ceiling));
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_clone_retirement_failure_rolls_back_and_catalog_names_relations(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    release_schema(&admin).await;
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let archive = legacy_archive::PgArchiveRepository::new(local.clone())
        .create_archive_schema("retirement_failure", None)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE VIEW {}.unexpected_identity_dependency AS SELECT * FROM {}.app_user",
        archive.schema_name, archive.schema_name
    ))
    .execute(&admin)
    .await
    .unwrap();
    let owner = Uuid::new_v4();
    let url = runtime_url(&admin, owner).await;
    let error = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .err()
    .unwrap();
    assert!(format!("{error:#}").contains("app_user"), "{error:#}");
    let unchanged:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM external_oidc_ownership) AND EXISTS(SELECT 1 FROM archive_registry WHERE tenant_id=$1 AND name='retirement_failure')")
        .bind(Uuid::nil()).fetch_one(&admin).await.unwrap();
    assert!(
        unchanged,
        "retirement failure must roll back adoption and receipt"
    );
    let runtime = matric_db::create_pool_with_config(&url, PoolConfig::new().hosted_unscoped())
        .await
        .unwrap();
    let error = matric_db::assert_hosted_runtime_role(&runtime)
        .await
        .unwrap_err();
    let matric_core::Error::Config(diagnostic) = error else {
        panic!("Expected catalog configuration failure");
    };
    assert!(
        diagnostic.contains(&format!("{}.app_user", archive.schema_name)),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains(&format!("{}.personal_access_token", archive.schema_name)),
        "{diagnostic}"
    );
    runtime.close().await;
    local.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_admin_archive_creation_first_use_and_queued_indexes(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let owner = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, owner).await;
    let synthetic = external_oidc_test_issuer::issuer();
    for group in ["reader", "writer"] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/archives")
                    .header(
                        header::AUTHORIZATION,
                        format!(
                            "Bearer {}",
                            synthetic.token(serde_json::json!({"groups":[group],"sub":group}))
                        ),
                    )
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"name":"denied_memory"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let absent: bool =
        sqlx::query_scalar("SELECT to_regnamespace('archive_denied_memory') IS NULL")
            .fetch_one(&admin)
            .await
            .unwrap();
    assert!(
        absent,
        "authorization denial must perform no administrative DDL"
    );
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/v1/archives",
        serde_json::json!({"name":"online_memory"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let schema = body["schema_name"].as_str().unwrap().to_owned();
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"First online memory write","pipeline":[]}),
        Some("online_memory"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let actual: Uuid =
        sqlx::query_scalar(&format!("SELECT tenant_id FROM {schema}.note WHERE id=$1"))
            .bind(note)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(actual, owner);
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/v1/archives/online_memory/clone",
        serde_json::json!({"new_name":"online_clone"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = call(
        &app,
        Method::GET,
        &format!("/api/v1/notes/{note}"),
        serde_json::Value::Null,
        Some("online_clone"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Force real schema drift, then request an existing note. Its authorization
    // lookup must finish before synchronization rebuilds indexes/table DDL.
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='online_memory'")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP TABLE {schema}.note_template"))
        .execute(&admin)
        .await
        .unwrap();
    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        call(
            &app,
            Method::GET,
            &format!("/api/v1/notes/{note}"),
            serde_json::Value::Null,
            Some("online_memory"),
        ),
    )
    .await
    .expect("archive synchronization must not deadlock behind authorization locks");
    assert_eq!(status, StatusCode::OK, "{body}");
    let present: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(format!("{schema}.note_template"))
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert!(present);
    // A resource can disappear during maintenance. Admission before DDL must
    // not let the handler run without checking visibility again afterward.
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION public.remove_note_during_sync() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN DELETE FROM {schema}.note WHERE id='{note}'; RETURN NEW; END $$;
         CREATE TRIGGER remove_note_during_sync AFTER UPDATE OF schema_version ON public.archive_registry
         FOR EACH ROW WHEN (NEW.name='online_memory' AND NEW.schema_version>OLD.schema_version)
         EXECUTE FUNCTION public.remove_note_during_sync();"
    ))
    .execute(&admin).await.unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='online_memory'")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP TABLE {schema}.note_template"))
        .execute(&admin)
        .await
        .unwrap();
    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        call(
            &app,
            Method::GET,
            &format!("/api/v1/notes/{note}"),
            serde_json::Value::Null,
            Some("online_memory"),
        ),
    )
    .await
    .expect("resource revalidation after synchronization must be bounded");
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    sqlx::raw_sql("DROP TRIGGER remove_note_during_sync ON public.archive_registry; DROP FUNCTION public.remove_note_during_sync()")
        .execute(&admin).await.unwrap();
    let db = startup::database(
        runtime.connect_options().to_url_lossy().as_str(),
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .unwrap();
    sqlx::query("DELETE FROM job_queue WHERE tenant_id=$1 AND job_type='build_set_index'")
        .bind(owner)
        .execute(&admin)
        .await
        .unwrap();
    let handler = BuildVectorIndexHandler::new(db.clone());
    for schema in ["public", schema.as_str()] {
        for table in ["embedding", "attachment_embedding"] {
            let index = if table == "embedding" {
                "idx_embedding_hnsw_vector_768"
            } else {
                "idx_attach_emb_hnsw_vector_768"
            };
            sqlx::query(&format!("DROP INDEX IF EXISTS {schema}.{index}"))
                .execute(&admin)
                .await
                .unwrap();
        }
        for m in [16, 24] {
            let payload = serde_json::json!({"schema":schema,"dimension":768,"vector_type":"vector","hnsw_m":m,"hnsw_ef_construction":96,"force":true});
            let job = db
                .jobs
                .queue(
                    None,
                    matric_core::JobType::BuildSetIndex,
                    1,
                    Some(payload),
                    None,
                )
                .await
                .unwrap();
            let queued = db
                .jobs
                .claim_next_for_types(&[matric_core::JobType::BuildSetIndex])
                .await
                .unwrap()
                .unwrap();
            assert_eq!(queued.id, job);
            let result = handler.execute(JobContext::new(queued)).await;
            let JobResult::Success(result) = result else {
                panic!("{schema}, m={m}: {result:?}");
            };
            db.jobs.complete(job, result).await.unwrap();
            for index in [
                "idx_embedding_hnsw_vector_768",
                "idx_attach_emb_hnsw_vector_768",
            ] {
                let (definition,valid):(String,bool) = sqlx::query_as("SELECT pg_get_indexdef(c.oid),i.indisvalid FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_index i ON i.indexrelid=c.oid WHERE n.nspname=$1 AND c.relname=$2")
                    .bind(schema).bind(index).fetch_one(&admin).await.unwrap();
                assert!(
                    valid
                        && definition.contains(&format!("m='{m}'"))
                        && definition.contains("ef_construction='96'"),
                    "{definition}"
                );
            }
            let completed: bool =
                sqlx::query_scalar("SELECT status='completed' FROM job_queue WHERE id=$1")
                    .bind(job)
                    .fetch_one(&runtime)
                    .await
                    .unwrap();
            assert!(completed);
        }
    }
    matric_db::assert_hosted_runtime_role(&runtime)
        .await
        .unwrap();
    let ddl = sqlx::query(&format!(
        "CREATE INDEX forbidden_runtime_index ON {schema}.note(id)"
    ))
    .execute(&runtime)
    .await
    .unwrap_err();
    assert_eq!(
        ddl.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    let (status, body) = call(
        &app,
        Method::DELETE,
        "/api/v1/archives/online_clone",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    drop(app);
    db.pool.close().await;
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn hosted_archive_note_reads_do_not_synchronize_or_grant_runtime_privileges(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let owner = Uuid::new_v4();
    let request = matric_api::admin_bootstrap::BootstrapRequest::new(
        "hosted-read-boundary",
        Some(owner),
        None,
        false,
    )
    .unwrap();
    matric_api::admin_bootstrap::bootstrap_tenant(&admin, &request, false, "fortemi:tenant_id")
        .await
        .unwrap();
    let url = runtime_url(&admin, owner).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::Hosted,
    )
    .await
    .unwrap();
    let archive = db
        .administrative()
        .unwrap()
        .create_archive(owner, "hosted_read_memory", None)
        .await
        .unwrap();
    let mut state =
        super::super::super::tests::build_call_api_test_state(db.clone(), "hosted-read-boundary")
            .await;
    state.auth_mode = AuthMode::HostedMultiTenant;
    state.multi_tenant = true;
    state.require_auth = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    super::super::seed_hosted_test_user(&admin, owner).await;
    let app = super::super::hosted_router(state, db.pool.clone(), owner);
    let mut request = super::super::http_request(
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"Hosted archive read boundary", "pipeline":[]}),
        false,
    );
    request
        .headers_mut()
        .insert("X-Fortemi-Memory", "hosted_read_memory".parse().unwrap());
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = super::super::json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note = body["id"].as_str().unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='hosted_read_memory'")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP TABLE {}.note_template", archive.schema_name))
        .execute(&admin)
        .await
        .unwrap();
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "REVOKE INSERT ON {}.note FROM {role}",
        archive.schema_name
    ))
    .execute(&admin)
    .await
    .unwrap();
    let mut request = super::super::http_request(
        Method::GET,
        &format!("/api/v1/notes/{note}"),
        serde_json::Value::Null,
        false,
    );
    request
        .headers_mut()
        .insert("X-Fortemi-Memory", "hosted_read_memory".parse().unwrap());
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        app.clone().oneshot(request),
    )
    .await
    .expect("hosted read must not enter administrative maintenance")
    .unwrap();
    let status = response.status();
    let body = super::super::json_body(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let untouched: bool = sqlx::query_scalar("SELECT schema_version=0 AND to_regclass(schema_name||'.note_template') IS NULL AND NOT has_table_privilege($1,schema_name||'.note','INSERT') FROM archive_registry WHERE name='hosted_read_memory'")
        .bind(role).fetch_one(&admin).await.unwrap();
    assert!(
        untouched,
        "hosted reads must not repair schemas or change runtime grants"
    );
    drop(app);
    db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn hosted_startup_preserves_real_release_archive_identities_and_builds_indexes(
    admin: sqlx::PgPool,
) {
    use matric_jobs::worker::{HostedDispatchLimits, HostedJobDispatcher};
    use matric_jobs::{HostedJobContext, HostedJobHandler};
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    release_schema(&admin).await;
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let archive = legacy_archive::PgArchiveRepository::new(local.clone())
        .create_archive_schema("hosted_legacy", None)
        .await
        .unwrap();
    let user = Uuid::new_v4();
    sqlx::query(&format!("INSERT INTO {}.app_user(id,iss,sub,kind,status) VALUES($1,'https://legacy.example','hosted-clone','user','disabled')",archive.schema_name)).bind(user).execute(&local).await.unwrap();
    let url = runtime_url(&admin, Uuid::new_v4()).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::Hosted,
    )
    .await
    .unwrap();
    let identity: (Uuid, String) =
        sqlx::query_as("SELECT tenant_id,status FROM public.app_user WHERE id=$1")
            .bind(user)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(identity, (Uuid::nil(), "disabled".into()));
    assert!(matric_db::inspect_tenant_catalog(&db.pool)
        .await
        .unwrap()
        .is_clean());
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM external_oidc_ownership")
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(receipts, 0, "hosted must not adopt local tenant content");
    let owner = Uuid::new_v4();
    let request = matric_api::admin_bootstrap::BootstrapRequest::new(
        "hosted-index-acceptance",
        Some(owner),
        None,
        false,
    )
    .unwrap();
    matric_api::admin_bootstrap::bootstrap_tenant(&admin, &request, false, "fortemi:tenant_id")
        .await
        .unwrap();
    let admin_executor = db.administrative().unwrap();
    let online = admin_executor
        .create_archive(owner, "hosted_online", None)
        .await
        .unwrap();
    sqlx::query("DELETE FROM job_queue WHERE job_type='build_set_index'")
        .execute(&admin)
        .await
        .unwrap();
    let dispatcher = HostedJobDispatcher::new(db.pool.clone(), HostedDispatchLimits::default())
        .await
        .unwrap();
    let handler =
        matric_api::hosted_jobs::HostedBuildVectorIndexHandler::new(admin_executor.clone());
    for m in [16, 24] {
        sqlx::query("INSERT INTO public.job_queue(id,tenant_id,job_type,cost_tier,payload) VALUES(uuidv7(),$1,'build_set_index',0,$2)")
            .bind(owner).bind(serde_json::json!({"schema":online.schema_name,"dimension":768,"vector_type":"vector","hnsw_m":m,"hnsw_ef_construction":96})).execute(&admin).await.unwrap();
        // A dispatcher page can be empty when its registry cursor wraps. The
        // worker continues with a fresh traversal on the next pass.
        let mut claimed = None;
        for _ in 0..3 {
            let pass = dispatcher
                .claim_next(
                    matric_core::TierGroup::CpuAndAgnostic,
                    &[matric_core::JobType::BuildSetIndex],
                    &[],
                )
                .await
                .unwrap();
            assert_eq!(pass.failed_tenants, 0);
            if let Some(claim) = pass.claim {
                claimed = Some(claim);
                break;
            }
        }
        let claim = claimed.expect("dispatcher must claim the queued index job after wrapping");
        assert_eq!(claim.tenant_id(), owner);
        let outcome = handler
            .execute(HostedJobContext::from_claim(claim.clone()))
            .await;
        let JobResult::Success(result) = outcome else {
            let payload = serde_json::from_value::<matric_db::vector_index::VectorIndexJobPayload>(
                claim.job().payload.clone().unwrap(),
            )
            .unwrap();
            let shape = matric_db::vector_index::VectorIndexShape::from_payload(payload).unwrap();
            let diagnostic = claim
                .with_content(move |scope| {
                    Box::pin(async move {
                        matric_db::vector_index::set_shape_status_scoped(scope, &shape, "building")
                            .await
                    })
                })
                .await;
            panic!("hosted index job: {outcome:?}; diagnostic: {diagnostic:?}");
        };
        assert!(claim.complete(result).await.unwrap());
        let definition:String=sqlx::query_scalar("SELECT indexdef FROM pg_indexes WHERE schemaname=$1 AND indexname='idx_embedding_hnsw_vector_768'").bind(&online.schema_name).fetch_one(&admin).await.unwrap();
        assert!(definition.contains(&format!("m='{m}'")), "{definition}");
    }
    db.pool.close().await;
    local.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_signed_authentication_admission_and_scopes(admin: sqlx::PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let tenant = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, tenant).await;
    let synthetic = external_oidc_test_issuer::issuer();
    for overrides in [
        serde_json::json!({"iss":"https://wrong.example","sub":"rejected"}),
        serde_json::json!({"aud":"https://wrong.example","sub":"rejected"}),
        serde_json::json!({"iat":Utc::now().timestamp()-400,"nbf":Utc::now().timestamp()-400,"exp":Utc::now().timestamp()-1,"sub":"rejected"}),
        serde_json::json!({"token_use":"id","nonce":"synthetic-nonce","sub":"rejected"}),
        serde_json::json!({"fortemi:tenant_id":Uuid::new_v4(),"sub":"rejected"}),
    ] {
        let request = axum::http::Request::builder()
            .uri("/api/v1/me")
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", synthetic.token(overrides.clone())),
            )
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
            "{overrides}: {status} {}",
            String::from_utf8_lossy(&bytes)
        );
    }
    let rejected: i64 = sqlx::query_scalar("SELECT count(*) FROM app_user WHERE sub='rejected'")
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(rejected, 0, "failed credentials must never JIT provision");
    for (group, method, path, body, expected) in [
        (
            "reader",
            Method::GET,
            "/api/v1/notes",
            serde_json::Value::Null,
            StatusCode::OK,
        ),
        (
            "reader",
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Reader denied","pipeline":[]}),
            StatusCode::FORBIDDEN,
        ),
        (
            "writer",
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Writer accepted","pipeline":[]}),
            StatusCode::CREATED,
        ),
        (
            "writer",
            Method::POST,
            "/api/v1/document-types",
            serde_json::json!({"name":"denied-admin","category":"custom"}),
            StatusCode::FORBIDDEN,
        ),
        (
            "admin",
            Method::GET,
            "/api/v1/me",
            serde_json::Value::Null,
            StatusCode::OK,
        ),
    ] {
        let request = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header(
                header::AUTHORIZATION,
                format!(
                    "Bearer {}",
                    synthetic.token(serde_json::json!({"groups":[group],"sub":group}))
                ),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&bytes));
    }
    drop(app);
    runtime.close().await;
}

async fn release_schema(admin: &sqlx::PgPool) {
    let all = sqlx::migrate!("../../migrations");
    let release = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < 20261014000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    release.run(admin).await.unwrap();
}

pub(super) struct SyntheticEmbedding;
#[async_trait::async_trait]
impl matric_core::EmbeddingBackend for SyntheticEmbedding {
    async fn embed_texts(&self, texts: &[String]) -> matric_core::Result<Vec<matric_core::Vector>> {
        Ok(texts
            .iter()
            .map(|_| matric_core::Vector::from(vec![0.25; 768]))
            .collect())
    }
    fn dimension(&self) -> usize {
        768
    }
    fn model_name(&self) -> &str {
        "synthetic-upgrade"
    }
}

#[sqlx::test(migrations = false)]
async fn external_oidc_populated_upgrade_reads_edits_catalog_search_downloads_and_workers(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    release_schema(&admin).await;
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let db = Database::new(local.clone());
    let named = legacy_archive::PgArchiveRepository::new(local.clone())
        .create_archive_schema("old_memory", Some("Old archive"))
        .await
        .unwrap();
    for table in ["app_user", "personal_access_token"] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{}.{}", named.schema_name, table))
            .fetch_one(&admin)
            .await
            .unwrap();
        assert!(exists, "release constructor must clone {table}");
    }
    let legacy_user = Uuid::new_v4();
    let legacy_pat = Uuid::new_v4();
    sqlx::query(&format!("INSERT INTO {}.app_user(id,iss,sub,kind,status) VALUES($1,'https://legacy.example','archive-only','user','disabled')",named.schema_name))
        .bind(legacy_user).execute(&local).await.unwrap();
    sqlx::query(&format!("INSERT INTO {}.personal_access_token(id,user_id,token_hash,token_prefix,token_last4,name,status,expires_at) VALUES($1,$2,repeat('b',64),'mm_pat_fixture','1234','Archive only','suspended','2030-01-01')",named.schema_name))
        .bind(legacy_pat).bind(legacy_user).execute(&local).await.unwrap();
    let mut ids = Vec::new();
    let mut attachments = Vec::new();
    for schema in ["public", named.schema_name.as_str()] {
        let notes = matric_db::PgNoteRepository::new(local.clone());
        let id = db
            .for_schema(schema)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    notes
                        .insert_tx(
                            tx,
                            CreateNoteRequest {
                                content: "Synthetic legacy ownership quartz migration".repeat(40),
                                format: "markdown".into(),
                                source: "upgrade".into(),
                                collection_id: None,
                                tags: Some(vec!["legacy".into()]),
                                metadata: None,
                                document_type_id: None,
                                title: Some("Legacy Quartz".into()),
                            },
                        )
                        .await
                })
            })
            .await
            .unwrap();
        // Queue an actual pending legacy embedding job, retaining its UUID.
        let job = db
            .jobs
            .queue(
                Some(id),
                matric_core::JobType::Embedding,
                1,
                Some(serde_json::json!({"schema":schema,"tenant_id":Uuid::nil()})),
                None,
            )
            .await
            .unwrap_or_else(|error| panic!("pending job: {error}"));
        let blob = Uuid::new_v4();
        let attachment = Uuid::new_v4();
        let bytes = b"Synthetic legacy attachment bytes";
        let content_hash = matric_db::file_storage::compute_content_hash(bytes);
        sqlx::query(&format!("INSERT INTO {schema}.attachment_blob(id,content_hash,content_type,size_bytes,storage_type,data) VALUES($1,$2,'text/plain',$3,'database',$4)"))
            .bind(blob).bind(&content_hash).bind(bytes.len() as i64).bind(bytes.as_slice()).execute(&local).await.unwrap();
        // Model an already scanned legacy file, with its verdict bound to these bytes.
        sqlx::query(&format!("INSERT INTO {schema}.attachment(id,note_id,blob_id,filename,extraction_strategy,virus_scan_status,virus_scan_blob_hash,virus_scan_at,virus_scan_backend) VALUES($1,$2,$3,'legacy.txt','text_native','clean',$4,now(),'synthetic')"))
            .bind(attachment).bind(id).bind(blob).bind(&content_hash).execute(&local).await.unwrap();
        db.jobs.queue(Some(id),matric_core::JobType::Extraction,1,Some(serde_json::json!({"schema":schema,"tenant_id":Uuid::nil(),"strategy":"text_native","attachment_id":attachment})),None).await.unwrap();
        attachments.push(attachment);
        ids.push((id, job));
    }
    sqlx::query(&format!(
        "UPDATE {}.note SET created_by_user_id=$1",
        named.schema_name
    ))
    .bind(legacy_user)
    .execute(&local)
    .await
    .unwrap();
    sqlx::raw_sql(&format!("ALTER TABLE {}.note DROP COLUMN updated_by_user_id CASCADE; UPDATE archive_registry SET schema_version=0 WHERE name='old_memory'",named.schema_name)).execute(&admin).await.unwrap();
    let tenant = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, tenant).await;
    let preserved: (Uuid, String, String, String) = sqlx::query_as("SELECT u.tenant_id,u.status,p.status,p.token_hash FROM public.app_user u JOIN public.personal_access_token p ON p.user_id=u.id WHERE p.id=$1")
        .bind(legacy_pat).fetch_one(&admin).await.unwrap();
    assert_eq!(
        preserved,
        (
            tenant,
            "disabled".into(),
            "suspended".into(),
            "b".repeat(64)
        )
    );
    let author: Uuid = sqlx::query_scalar(&format!(
        "SELECT created_by_user_id FROM {}.note LIMIT 1",
        named.schema_name
    ))
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(author, legacy_user);
    for table in ["app_user", "personal_access_token"] {
        let retired: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NULL")
            .bind(format!("{}.{}", named.schema_name, table))
            .fetch_one(&admin)
            .await
            .unwrap();
        assert!(retired);
    }
    let runtime_db = Database::new(runtime.clone()).with_filesystem_storage(
        std::env::temp_dir()
            .join(format!("external-storage-{tenant}"))
            .to_str()
            .unwrap(),
        0,
    );
    let mut registry = matric_jobs::ExtractionRegistry::new();
    registry.register(Arc::new(matric_jobs::TextNativeAdapter));
    let extraction = matric_jobs::ExtractionHandler::new(runtime_db.clone(), Arc::new(registry));
    while let Some(queued) = runtime_db
        .jobs
        .claim_next_for_types(&[matric_core::JobType::Extraction])
        .await
        .unwrap()
    {
        let job_id = queued.id;
        let result = extraction.execute(JobContext::new(queued)).await;
        let JobResult::Success(result) = result else {
            panic!("{result:?}");
        };
        runtime_db.jobs.complete(job_id, result).await.unwrap();
    }
    let handler = handlers::jobs::EmbeddingHandler::new(
        runtime_db.clone(),
        Arc::new(matric_inference::ProviderRegistry::from_env()),
        Arc::new(matric_core::InMemoryMeter::default()),
    )
    .with_backend_override(Arc::new(SyntheticEmbedding));
    // Exercise the real claim/execute/complete sequence for pending legacy jobs.
    while let Some(queued) = runtime_db
        .jobs
        .claim_next_for_types(&[matric_core::JobType::Embedding])
        .await
        .unwrap()
    {
        let job_id = queued.id;
        assert_eq!(
            queued.payload.as_ref().unwrap()["tenant_id"],
            tenant.to_string()
        );
        let result = handler.execute(JobContext::new(queued)).await;
        let JobResult::Success(result) = result else {
            panic!("{result:?}");
        };
        runtime_db
            .jobs
            .complete(job_id, result)
            .await
            .unwrap_or_else(|error| panic!("completion: {error}"));
    }
    for ((id, job), memory) in ids.iter().zip([None, Some("old_memory")]) {
        let (status, body) = call(
            &app,
            Method::GET,
            &format!("/api/v1/notes/{id}"),
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status,body)=call(&app,Method::PATCH,&format!("/api/v1/notes/{id}"),serde_json::json!({"content":"Synthetic edited quartz migration".repeat(40),"revision_mode":"none"}),memory).await;
        assert!(status.is_success(), "{status}: {body}");
        let (status, body) = call(
            &app,
            Method::GET,
            "/api/v1/search?q=quartz&mode=fts",
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.to_string().contains(&id.to_string()), "{body}");
        let (status, vectors) = call(
            &app,
            Method::GET,
            "/api/v1/search?q=nonlexicalvectorprobe&mode=semantic&embedding_set=default",
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{vectors}");
        assert_eq!(
            vectors["degraded"], false,
            "vector search must not fall back to FTS: {vectors}"
        );
        assert!(
            vectors["results"].to_string().contains(&id.to_string()),
            "{vectors}"
        );
        let completed = runtime_db.jobs.get(*job).await.unwrap().unwrap();
        assert_eq!(completed.status, matric_core::JobStatus::Completed);
        let schema = memory
            .map(|_| named.schema_name.as_str())
            .unwrap_or("public");
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {schema}.embedding WHERE tenant_id=$1 AND note_id=$2"
        ))
        .bind(tenant)
        .bind(id)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert!(count > 0);
        let (status,body)=call(&app,Method::POST,"/api/v1/notes",serde_json::json!({"content":"Fresh tagged quartz","tags":["upgrade/new"],"document_type":"markdown","pipeline":[]}),memory).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    for (attachment, memory) in attachments.iter().zip([None, Some("old_memory")]) {
        let response = app
            .clone()
            .oneshot(oidc_request(
                Method::GET,
                &format!("/api/v1/attachments/{attachment}/download"),
                serde_json::Value::Null,
                memory,
            ))
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(bytes.as_ref(), b"Synthetic legacy attachment bytes");
        let schema = memory
            .map(|_| named.schema_name.as_str())
            .unwrap_or("public");
        let text: String = sqlx::query_scalar(&format!(
            "SELECT extracted_text FROM {schema}.attachment WHERE id=$1 AND tenant_id=$2"
        ))
        .bind(attachment)
        .bind(tenant)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(text, "Synthetic legacy attachment bytes");
    }
    let (status, archives) = call(
        &app,
        Method::GET,
        "/api/v1/archives",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{archives}");
    let (status, memories) = call(
        &app,
        Method::GET,
        "/api/v1/memories",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{memories}");
    assert_eq!(archives, memories);
    for prefix in ["archives", "memories"] {
        let path = format!("/api/v1/{prefix}/old_memory");
        let (status, body) = call(&app, Method::GET, &path, serde_json::Value::Null, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body) = call(
            &app,
            Method::PATCH,
            &path,
            serde_json::json!({"description":"Edited archive"}),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    }
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/v1/memories/old_memory/set-default",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, body) = call(
        &app,
        Method::GET,
        &format!("/api/v1/notes/{}", ids[1].0),
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    drop(app);
    runtime.close().await;
    local.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_atomic_queue_failure_and_unknown_types_in_both_archives(
    admin: sqlx::PgPool,
) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let owner = Uuid::new_v4();
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let named = Database::new(local.clone())
        .archives
        .create_archive_schema("atomic_memory", None)
        .await
        .unwrap();
    let (app, runtime) = production_router(&admin, owner).await;
    sqlx::raw_sql("CREATE FUNCTION public.synthetic_reject_queue() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Synthetic queue failure'; END $$; CREATE TRIGGER synthetic_reject_queue BEFORE INSERT ON public.job_queue FOR EACH ROW EXECUTE FUNCTION public.synthetic_reject_queue()")
        .execute(&admin).await.unwrap();
    for (memory, schema) in [
        (None, "public"),
        (Some("atomic_memory"), named.schema_name.as_str()),
    ] {
        let counts = |schema: &str| {
            format!("SELECT ARRAY[(SELECT count(*) FROM {schema}.note),(SELECT count(*) FROM {schema}.note_original),(SELECT count(*) FROM {schema}.note_revised_current),(SELECT count(*) FROM {schema}.note_tag),(SELECT count(*) FROM {schema}.embedding_set_member),(SELECT count(*) FROM public.job_queue)]")
        };
        let before: Vec<i64> = sqlx::query_scalar(&counts(schema))
            .fetch_one(&admin)
            .await
            .unwrap();
        for (body, status) in [
            (
                serde_json::json!({"content":"Unknown type","document_type":"missing-type","pipeline":[]}),
                StatusCode::BAD_REQUEST,
            ),
            (
                serde_json::json!({"content":"Queue failure","tags":["queue/failure"],"pipeline":["document_type_inference"]}),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            let (actual, body) = call(&app, Method::POST, "/api/v1/notes", body, memory).await;
            assert_eq!(actual, status, "{body}");
            let after: Vec<i64> = sqlx::query_scalar(&counts(schema))
                .fetch_one(&admin)
                .await
                .unwrap();
            assert_eq!(before, after, "{schema}: partial creation survived");
        }
    }
    drop(app);
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_startup_role_requirements(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let migration = admin.connect_options().to_url_lossy();
    let owner = Uuid::new_v4();
    assert!(
        startup::database(migration.as_str(), None, startup::Tenancy::External(owner))
            .await
            .is_err()
    );
    assert!(startup::database(
        migration.as_str(),
        Some(migration.as_str()),
        startup::Tenancy::External(owner)
    )
    .await
    .is_err());
    // Different URLs with the same privileged role must also fail startup.
    let mut privileged = reqwest::Url::parse(migration.as_str()).unwrap();
    privileged
        .query_pairs_mut()
        .append_pair("application_name", "runtime-role-rejection");
    let err = startup::database(
        privileged.as_str(),
        Some(migration.as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(err.downcast_ref::<matric_core::Error>(), Some(matric_core::Error::Config(reason)) if reason.contains("NOSUPERUSER")),
        "{err:#}"
    );
    let runtime = runtime_url(&admin, owner).await;
    sqlx::query(&format!(
        "ALTER ROLE external_accept_{} BYPASSRLS",
        owner.simple()
    ))
    .execute(&admin)
    .await
    .unwrap();
    let err = startup::database(
        &runtime,
        Some(migration.as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(err.downcast_ref::<matric_core::Error>(), Some(matric_core::Error::Config(reason)) if reason.contains("NOBYPASSRLS")),
        "{err:#}"
    );
}

#[sqlx::test(migrations = false)]
async fn external_oidc_adoption_is_absent_in_community_and_hosted_startup(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let migration = admin.connect_options().to_url_lossy();
    // Real startup on an empty database in community mode.
    let community = startup::database(migration.as_str(), None, startup::Tenancy::Community)
        .await
        .unwrap();
    let legacy = community
        .notes
        .insert(CreateNoteRequest {
            content: "Community ownership must survive both startup modes".into(),
            format: "markdown".into(),
            source: "test".into(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
            title: None,
        })
        .await
        .unwrap();
    community.pool.close().await;
    let runtime = runtime_url(&admin, Uuid::new_v4()).await;
    for mode in [startup::Tenancy::Community, startup::Tenancy::Hosted] {
        let url = if matches!(mode, startup::Tenancy::Community) {
            migration.as_str()
        } else {
            &runtime
        };
        let db = startup::database(url, Some(migration.as_str()), mode)
            .await
            .unwrap();
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM external_oidc_ownership")
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(receipts, 0);
        let owner: Uuid = sqlx::query_scalar("SELECT tenant_id FROM note WHERE id=$1")
            .bind(legacy)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(owner, Uuid::nil());
        let ambient: Option<String> =
            sqlx::query_scalar("SELECT current_setting('app.current_tenant',true)")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        if matches!(mode, startup::Tenancy::Community) {
            assert_eq!(
                ambient.as_deref(),
                Some("00000000-0000-0000-0000-000000000000")
            );
        } else {
            assert!(ambient.is_none() || ambient.as_deref() == Some(""));
        }
        db.pool.close().await;
    }
}

/// Use the real old bootstrap defaults (config/set/scheme). The pre-#1205
/// bootstrap did not seed document types; discard the current backfill here.
async fn old_bootstrap_fixture(admin: &sqlx::PgPool) -> (sqlx::PgPool, Uuid, Uuid) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let owner = Uuid::new_v4();
    external_oidc::ensure_default_tenant(&local, owner)
        .await
        .unwrap();
    sqlx::query("DELETE FROM document_type WHERE tenant_id=$1")
        .bind(owner)
        .execute(admin)
        .await
        .unwrap();
    let note = Database::new(local.clone())
        .notes
        .insert(CreateNoteRequest {
            content: "Legacy indexed quartz".into(),
            format: "markdown".into(),
            source: "test".into(),
            collection_id: None,
            tags: None,
            metadata: None,
            document_type_id: None,
            title: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE embedding_set SET last_indexed_at='2026-10-01',index_size_bytes=8192,embeddings_current=false,index_status='ready' WHERE tenant_id=$1 AND slug='default'")
        .bind(Uuid::nil()).execute(admin).await.unwrap();
    (local, owner, note)
}

#[sqlx::test(migrations = false)]
async fn external_oidc_old_bootstrap_operational_state_does_not_conflict(admin: sqlx::PgPool) {
    let (local, owner, note) = old_bootstrap_fixture(&admin).await;
    let original: (Uuid, Option<chrono::DateTime<Utc>>, Option<i64>) = sqlx::query_as("SELECT id,last_indexed_at,index_size_bytes FROM embedding_set WHERE tenant_id=$1 AND slug='default'")
        .bind(Uuid::nil()).fetch_one(&admin).await.unwrap();
    let (app, runtime) = production_router(&admin, owner).await;
    let adopted: (Uuid, Option<chrono::DateTime<Utc>>, Option<i64>) = sqlx::query_as("SELECT id,last_indexed_at,index_size_bytes FROM embedding_set WHERE tenant_id=$1 AND slug='default'")
        .bind(owner).fetch_one(&admin).await.unwrap();
    assert_eq!(
        adopted, original,
        "index metadata and UUID must be preserved"
    );
    let (status, body) = call(
        &app,
        Method::GET,
        &format!("/api/v1/notes/{note}"),
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    drop(app);
    runtime.close().await;
    local.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_referenced_configuration_customization_aborts(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    let (local, owner, _) = old_bootstrap_fixture(&admin).await;
    // Distinct names/UUIDs are allowed only when configuration values agree.
    let custom: Uuid = sqlx::query_scalar("INSERT INTO embedding_config SELECT (jsonb_populate_record(NULL::embedding_config,to_jsonb(c)||jsonb_build_object('id',uuidv7(),'tenant_id',$1::uuid,'name','custom-reference','is_default',false,'chunk_size',777))).* FROM embedding_config c WHERE tenant_id=$1 AND is_default RETURNING id")
        .bind(owner).fetch_one(&admin).await.unwrap();
    let url = runtime_url(&admin, owner).await;
    let migration = admin.connect_options().to_url_lossy();
    for reference in ["embedding_set", "document_type"] {
        if reference == "embedding_set" {
            sqlx::query("UPDATE embedding_set SET embedding_config_id=$2 WHERE tenant_id=$1 AND slug='default'").bind(owner).bind(custom).execute(&admin).await.unwrap();
        } else {
            sqlx::query("UPDATE document_type SET recommended_config_id=(SELECT id FROM embedding_config WHERE tenant_id=$1 AND is_default) WHERE tenant_id=$1 AND name='markdown'")
                .bind(Uuid::nil()).execute(&admin).await.unwrap();
            sqlx::query("UPDATE embedding_set SET embedding_config_id=(SELECT id FROM embedding_config WHERE tenant_id=$1 AND is_default) WHERE tenant_id=$1 AND slug='default'").bind(owner).execute(&admin).await.unwrap();
            sqlx::query("INSERT INTO document_type SELECT (jsonb_populate_record(NULL::document_type,to_jsonb(d)||jsonb_build_object('id',uuidv7(),'tenant_id',$1::uuid,'recommended_config_id',$2::uuid))).* FROM document_type d WHERE tenant_id=$3 AND name='markdown'")
                .bind(owner).bind(custom).bind(Uuid::nil()).execute(&admin).await.unwrap();
        }
        let before: serde_json::Value = baseline_snapshot(&admin).await;
        let err = startup::database(
            &url,
            Some(migration.as_str()),
            startup::Tenancy::External(owner),
        )
        .await
        .err()
        .unwrap();
        assert!(
            format!("{err:#}").contains("Conflicting customized baseline"),
            "{err:#}"
        );
        assert_eq!(
            baseline_snapshot(&admin).await,
            before,
            "{reference} conflict must roll back"
        );
    }
    // Once referenced values match, different configuration UUIDs and names
    // are equivalent and both references survive the automatic adoption.
    sqlx::query("UPDATE embedding_config SET chunk_size=(SELECT chunk_size FROM embedding_config WHERE tenant_id=$2 AND is_default) WHERE id=$1")
        .bind(custom).bind(Uuid::nil()).execute(&admin).await.unwrap();
    let db = startup::database(
        &url,
        Some(migration.as_str()),
        startup::Tenancy::External(owner),
    )
    .await
    .unwrap();
    let preserved:Uuid=sqlx::query_scalar("SELECT recommended_config_id FROM document_type WHERE tenant_id=$1 AND name LIKE 'external-previous-%'")
        .bind(owner).fetch_one(&admin).await.unwrap();
    assert_eq!(preserved, custom);
    db.pool.close().await;
    local.close().await;
}

async fn baseline_snapshot(admin: &sqlx::PgPool) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('configs',(SELECT jsonb_agg(to_jsonb(c) ORDER BY id) FROM embedding_config c),'sets',(SELECT jsonb_agg(to_jsonb(s) ORDER BY id) FROM embedding_set s),'types',(SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM document_type d),'owners',(SELECT jsonb_agg(to_jsonb(o)) FROM external_oidc_ownership o),'notes',(SELECT jsonb_agg(to_jsonb(n) ORDER BY id) FROM note n))")
        .fetch_one(admin).await.unwrap()
}

fn source_batch(content: &str, batch: &str) -> serde_json::Value {
    serde_json::json!({"source_namespace":"synthetic-external", "source_schema_version":"1", "import_run_id":"round4", "batch_id":batch,
        "items":[{"external_id":"first","content":content},{"external_id":"second","content":content}]})
}

async fn write_snapshot(admin: &sqlx::PgPool, schema: &str) -> serde_json::Value {
    // Exact rows, including SKOS and import receipts, expose partial commits.
    let mut result = serde_json::Map::new();
    for table in [
        "note",
        "note_original",
        "note_revised_current",
        "note_tag",
        "note_skos_concept",
        "skos_concept",
        "skos_concept_label",
        "embedding_set_member",
        "source_identity",
        "source_import_run",
        "source_import_batch",
    ] {
        let rows: serde_json::Value = sqlx::query_scalar(&format!("SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]') FROM {schema}.{table} t"))
            .fetch_one(admin).await.unwrap();
        result.insert(table.into(), rows);
    }
    let jobs: serde_json::Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(j) ORDER BY id),'[]') FROM public.job_queue j",
    )
    .fetch_one(admin)
    .await
    .unwrap();
    result.insert("jobs".into(), jobs);
    serde_json::Value::Object(result)
}

#[sqlx::test(migrations = false)]
async fn external_oidc_bulk_and_source_upsert_are_atomic_in_both_archives(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let named = Database::new(local.clone())
        .archives
        .create_archive_schema("bulk_memory", None)
        .await
        .unwrap();
    let owner = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, owner).await;
    assert!(!route_policy::hosted_tenant_transaction_ready(
        &Method::POST,
        "/api/v1/notes/bulk"
    ));
    for (memory, schema) in [
        (None, "public"),
        (Some("bulk_memory"), named.schema_name.as_str()),
    ] {
        let bulk = serde_json::json!({"notes":[
            {"content":"Atomic bulk first","tags":["round4/first"],"revision_mode":"none","document_type":"markdown"},
            {"content":"Atomic bulk second","tags":["round4/second"],"revision_mode":"none","document_type":"markdown"}]});
        let (status, body) = call(
            &app,
            Method::POST,
            "/api/v1/notes/bulk",
            bulk.clone(),
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["count"], 2);
        for id in body["ids"].as_array().unwrap() {
            let id = Uuid::parse_str(id.as_str().unwrap()).unwrap();
            let attributed: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {schema}.note n JOIN public.app_user u ON n.created_by_user_id=u.id AND n.updated_by_user_id=u.id WHERE n.id=$1 AND n.tenant_id=$2 AND u.tenant_id=$2"))
                .bind(id).bind(owner).fetch_one(&admin).await.unwrap();
            assert_eq!(attributed, 1);
            let tags: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {schema}.note_skos_concept WHERE note_id=$1 AND tenant_id=$2"
            ))
            .bind(id)
            .bind(owner)
            .fetch_one(&admin)
            .await
            .unwrap();
            assert!(tags > 0);
            let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM public.job_queue WHERE note_id=$1 AND tenant_id=$2 AND initiated_by_user_id IS NOT NULL AND payload->>'tenant_id'=$3 AND payload->>'schema'=$4")
                .bind(id).bind(owner).bind(owner.to_string()).bind(schema).fetch_one(&admin).await.unwrap();
            assert!(
                jobs > 0,
                "bulk must atomically queue attributed work for {schema}"
            );
        }
        // Queue and tag failures must roll back all dependent writes.
        sqlx::raw_sql(&format!("CREATE FUNCTION {schema}.reject_round4() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic failure'; END $$"))
            .execute(&admin).await.unwrap();
        for table in ["job_queue", "note_skos_concept"] {
            let target = if table == "job_queue" {
                "public"
            } else {
                schema
            };
            sqlx::query(&format!("CREATE TRIGGER reject_round4 BEFORE INSERT ON {target}.{table} FOR EACH ROW EXECUTE FUNCTION {schema}.reject_round4()"))
                .execute(&admin).await.unwrap();
            let before = write_snapshot(&admin, schema).await;
            let (status, body) = call(
                &app,
                Method::POST,
                "/api/v1/notes/bulk",
                bulk.clone(),
                memory,
            )
            .await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
            assert_eq!(
                before,
                write_snapshot(&admin, schema).await,
                "{schema}: {table} failure leaked bulk writes"
            );
            sqlx::query(&format!("DROP TRIGGER reject_round4 ON {target}.{table}"))
                .execute(&admin)
                .await
                .unwrap();
        }
        // A later validation error proves an earlier successful item is rolled back.
        let mut invalid = bulk.clone();
        invalid["notes"][1]["document_type"] = serde_json::json!("unknown-round4");
        let before = write_snapshot(&admin, schema).await;
        let (status, body) = call(&app, Method::POST, "/api/v1/notes/bulk", invalid, memory).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(before, write_snapshot(&admin, schema).await);

        let (status, body) = call(
            &app,
            Method::POST,
            "/api/v1/notes/source-upsert",
            source_batch("Original source text", "initial"),
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["counts"]["inserted"], 2, "{body}");
        for item in body["items"].as_array().unwrap() {
            let id = Uuid::parse_str(item["note_id"].as_str().unwrap()).unwrap();
            let owner_and_author: (Uuid,Uuid,Uuid)=sqlx::query_as(&format!("SELECT tenant_id,created_by_user_id,updated_by_user_id FROM {schema}.note WHERE id=$1"))
                .bind(id).fetch_one(&admin).await.unwrap();
            assert_eq!(owner_and_author.0, owner);
            assert_eq!(owner_and_author.1, owner_and_author.2);
        }
        let mut replacement = source_batch("Replacement source text", "replacement");
        replacement["policy"] = serde_json::json!("replace");
        let (status, body) = call(
            &app,
            Method::POST,
            "/api/v1/notes/source-upsert",
            replacement,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["counts"]["replaced"], 2, "{body}");
        // Source-upsert has no tags/jobs in its contract. Its receipt and author
        // writes must share the same request transaction as all inserted notes.
        for target in ["source_import_batch", "note"] {
            let event = if target == "note" {
                "UPDATE OF created_by_user_id"
            } else {
                "INSERT"
            };
            sqlx::query(&format!("CREATE TRIGGER reject_round4 BEFORE {event} ON {schema}.{target} FOR EACH ROW EXECUTE FUNCTION {schema}.reject_round4()"))
                .execute(&admin).await.unwrap();
            let mut rejected = source_batch("Rejected source text", target);
            rejected["items"][0]["external_id"] = serde_json::json!("failure-first");
            rejected["items"][1]["external_id"] = serde_json::json!("failure-second");
            let before = write_snapshot(&admin, schema).await;
            let (status, body) = call(
                &app,
                Method::POST,
                "/api/v1/notes/source-upsert",
                rejected,
                memory,
            )
            .await;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
            assert_eq!(
                before,
                write_snapshot(&admin, schema).await,
                "{schema}: {target} failure leaked source writes"
            );
            sqlx::query(&format!("DROP TRIGGER reject_round4 ON {schema}.{target}"))
                .execute(&admin)
                .await
                .unwrap();
        }
    }
    drop(app);
    runtime.close().await;
    local.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_production_runtime_hides_foreign_content_over_http(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let named = Database::new(local.clone())
        .archives
        .create_archive_schema("isolation_memory", None)
        .await
        .unwrap();
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Foreign isolation sentinel','active')")
        .bind(foreign).bind(foreign.to_string()).execute(&admin).await.unwrap();
    let foreign_pool = matric_db::create_pool_with_config(
        admin.connect_options().to_url_lossy().as_str(),
        PoolConfig::new().single_tenant(foreign),
    )
    .await
    .unwrap();
    let foreign_db = Database::new(foreign_pool.clone());
    let mut foreign_ids = Vec::new();
    for schema in ["public", named.schema_name.as_str()] {
        let notes = matric_db::PgNoteRepository::new(foreign_pool.clone());
        let id = foreign_db
            .for_schema(schema)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    notes
                        .insert_tx(
                            tx,
                            CreateNoteRequest {
                                content: "Foreignsecretquartz must remain invisible".into(),
                                format: "markdown".into(),
                                source: "sentinel".into(),
                                collection_id: None,
                                tags: None,
                                metadata: None,
                                document_type_id: None,
                                title: Some("Foreignsecretquartz".into()),
                            },
                        )
                        .await
                })
            })
            .await
            .unwrap();
        foreign_ids.push(id);
    }
    let foreign_job = foreign_db
        .jobs
        .queue(
            Some(foreign_ids[0]),
            matric_core::JobType::Embedding,
            1,
            None,
            None,
        )
        .await
        .unwrap();
    let owner = Uuid::new_v4();
    let (app, runtime) = production_router(&admin, owner).await;
    for (id, memory) in foreign_ids.iter().zip([None, Some("isolation_memory")]) {
        let (status, body) = call(
            &app,
            Method::GET,
            "/api/v1/notes",
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.to_string().contains(&id.to_string()),
            "foreign note leaked: {body}"
        );
        let (status, body) = call(
            &app,
            Method::GET,
            &format!("/api/v1/notes/{id}"),
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let (status, body) = call(
            &app,
            Method::GET,
            "/api/v1/search?q=foreignsecretquartz&mode=fts",
            serde_json::Value::Null,
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["total"], 0, "{body}");
    }
    let jobs = Database::new(runtime.clone())
        .jobs
        .claim_next_for_types(&[matric_core::JobType::Embedding])
        .await
        .unwrap();
    assert!(jobs.is_none(), "worker claimed foreign work");
    let job_owner: Uuid = sqlx::query_scalar("SELECT tenant_id FROM job_queue WHERE id=$1")
        .bind(foreign_job)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(job_owner, foreign);
    drop(app);
    runtime.close().await;
    foreign_pool.close().await;
    local.close().await;
}
