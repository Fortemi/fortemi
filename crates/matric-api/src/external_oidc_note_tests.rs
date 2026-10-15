//! External-OIDC startup repairs the default archive's document-type registry.

use super::external_oidc_test_issuer;
use super::*;
use matric_core::ArchiveRepository;
use tower::ServiceExt;

#[path = "me_tenant_tests.rs"]
mod me_tenant_tests;

async fn create_after_startup(admin: sqlx::PgPool, existing_tenant: bool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    if existing_tenant {
        Database::new(admin.clone()).migrate().await.unwrap();
    }
    let tenant = Uuid::new_v4();
    if existing_tenant {
        sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Existing external tenant','active')")
            .bind(tenant).bind(format!("upgrade-{tenant}"))
            .execute(&admin).await.unwrap();
    }
    let local_before: Vec<Uuid> = if existing_tenant {
        sqlx::query_scalar("SELECT id FROM document_type WHERE tenant_id=$1 ORDER BY id")
            .bind(Uuid::nil())
            .fetch_all(&admin)
            .await
            .unwrap()
    } else {
        Vec::new()
    };
    let (app, runtime) = production_router(&admin, tenant).await;
    let adopted: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM document_type WHERE tenant_id=$1 ORDER BY id")
            .bind(tenant)
            .fetch_all(&admin)
            .await
            .unwrap();
    if existing_tenant {
        assert_eq!(adopted, local_before, "startup adopts existing UUIDs");
    }
    assert!(!adopted.is_empty());
    let url = runtime.connect_options().to_url_lossy();
    let repeated = startup::database(
        url.as_str(),
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    let repeated_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM document_type ORDER BY id")
        .fetch_all(&repeated.pool)
        .await
        .unwrap();
    assert_eq!(adopted, repeated_ids);
    repeated.pool.close().await;
    for explicit_type in [false, true] {
        let mut body = serde_json::json!({"content":"Synthetic external note", "pipeline":[]});
        if explicit_type {
            body["document_type"] = serde_json::json!("markdown");
        }
        let response = app
            .clone()
            .oneshot(oidc_request(Method::POST, "/api/v1/notes", body, None))
            .await
            .unwrap();
        let status = response.status();
        let body = json_body(response).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let note = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        let (owner, author, updated): (Uuid, Uuid, Uuid) = sqlx::query_as(
            "SELECT tenant_id,created_by_user_id,updated_by_user_id FROM note WHERE id=$1",
        )
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(owner, tenant);
        assert_eq!(author, updated);
        let user_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM app_user WHERE id=$1")
            .bind(author)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(user_tenant, tenant);
        if !explicit_type {
            let mut scope = matric_db::TenantScopedConn::begin(&admin, tenant)
                .await
                .unwrap();
            let result = matric_db::PgDocumentTypeRepository::infer_note_scoped(
                &mut scope,
                Uuid::new_v4(),
                note,
            )
            .await
            .unwrap();
            assert_eq!(result["detected"], true);
            scope.commit().await.unwrap();
        }
        let assigned: Uuid = sqlx::query_scalar(
            "SELECT d.tenant_id FROM note n JOIN document_type d ON d.id=n.document_type_id WHERE n.id=$1",
        ).bind(note).fetch_one(&admin).await.unwrap();
        assert_eq!(assigned, tenant);
    }
    let local_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM document_type WHERE tenant_id=$1")
            .bind(Uuid::nil())
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(local_after, 0, "local registry was adopted, not copied");
    drop(app);
    runtime.close().await;
    admin.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_fresh_default_archive_note_creation(admin: sqlx::PgPool) {
    create_after_startup(admin, false).await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_upgrade_empty_registry_default_archive_note_creation(admin: sqlx::PgPool) {
    create_after_startup(admin, true).await;
}

/// Provision the supported production role, including privileges for objects
/// created by migration/adoption. Only setup and assertions use the admin pool.
async fn runtime_url(admin: &sqlx::PgPool, tenant: Uuid) -> String {
    use sqlx::ConnectOptions;
    let role = format!("external_accept_{}", tenant.simple());
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD 'synthetic-test-only' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE"))
        .execute(admin).await.unwrap();
    let schemas: Vec<String> = sqlx::query_scalar(
        "SELECT nspname::text FROM pg_namespace WHERE nspname='public' OR nspname LIKE 'archive_%'",
    )
    .fetch_all(admin)
    .await
    .unwrap();
    for schema in schemas {
        sqlx::raw_sql(&format!("GRANT USAGE ON SCHEMA {schema} TO {role};
            GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO {role};
            GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO {role};
            ALTER DEFAULT PRIVILEGES IN SCHEMA {schema} GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO {role};
            ALTER DEFAULT PRIVILEGES IN SCHEMA {schema} GRANT USAGE,SELECT ON SEQUENCES TO {role}"))
            .execute(admin).await.unwrap();
    }
    admin
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password("synthetic-test-only")
        .to_url_lossy()
        .to_string()
}

async fn production_router(admin: &sqlx::PgPool, tenant: Uuid) -> (Router, sqlx::PgPool) {
    use sqlx::ConnectOptions;
    let url = runtime_url(admin, tenant).await;
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap_or_else(|error| panic!("production startup: {error:#}"));
    let owner: String = sqlx::query_scalar("SELECT current_setting('app.current_tenant')")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        owner,
        tenant.to_string(),
        "production runtime must reconnect under E"
    );
    let receipt: Uuid = sqlx::query_scalar("SELECT owner FROM external_oidc_ownership")
        .fetch_one(admin)
        .await
        .unwrap();
    assert_eq!(
        receipt, tenant,
        "production startup must adopt automatically"
    );
    production_router_with_database(db, tenant).await
}

async fn production_router_with_pool(
    runtime: sqlx::PgPool,
    tenant: Uuid,
) -> (Router, sqlx::PgPool) {
    production_router_with_database(Database::new(runtime), tenant).await
}

async fn production_router_with_database(db: Database, tenant: Uuid) -> (Router, sqlx::PgPool) {
    production_router_with_event_bus(db, tenant, Arc::new(EventBus::new(256))).await
}

async fn production_router_with_event_bus(
    db: Database,
    tenant: Uuid,
    event_bus: Arc<EventBus>,
) -> (Router, sqlx::PgPool) {
    let state = production_state_with_event_bus(db, tenant, event_bus).await;
    let runtime = state.db.pool.clone();
    (super::production_router(state, None), runtime)
}

async fn production_state_with_event_bus(
    db: Database,
    tenant: Uuid,
    event_bus: Arc<EventBus>,
) -> AppState {
    let runtime = db.pool.clone();
    let storage = std::env::temp_dir().join(format!("external-storage-{tenant}"));
    let db = db.with_filesystem_storage(storage.to_str().unwrap(), 0);
    let mut state =
        super::super::tests::build_call_api_test_state(db, "external-oidc-document-types").await;
    state.tus_staging_path = storage.join("tus").to_string_lossy().into_owned();
    tokio::fs::create_dir_all(&state.tus_staging_path)
        .await
        .unwrap();
    state.event_bus = event_bus;
    state.require_auth = true;
    state.multi_tenant = false;
    state.auth_mode = AuthMode::ExternalOidc;
    state.external_oidc = Some(ExternalOidcConfig {
        audiences: ["https://api.example".into()].into_iter().collect(),
        default_tenant_id: tenant,
        allow_multi_audience: false,
        allow_token_scopes: false,
        allow_legacy_tokens: false,
        legacy_scope_ceiling: Default::default(),
        max_token_lifetime_seconds: 3600,
    });
    let synthetic = external_oidc_test_issuer::issuer();
    state.oidc_issuer = Some(synthetic.url.clone());
    state.hosted_auth = Some(external_oidc_test_issuer::authenticator(
        runtime.clone(),
        tenant,
    ));
    let provider_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_addr = provider_listener.local_addr().unwrap();
    let provider_app = Router::new().route(
        "/api/embed",
        post(|Json(body): Json<serde_json::Value>| async move {
            let count = body["input"].as_array().unwrap().len();
            Json(serde_json::json!({"embeddings": vec![vec![0.25f32; 768]; count]}))
        }),
    );
    let provider_app = provider_app.route(
        "/api/chat",
        post(|| async {
            (
                [(header::CONTENT_TYPE, "application/x-ndjson")],
                Body::from_stream(futures::stream::iter([
                    Ok::<_, std::io::Error>(
                        "{\"message\":{\"content\":\"streamed-first\"},\"done\":false}\n",
                    ),
                    Ok("{\"message\":{\"content\":\"streamed-second\"},\"done\":false}\n"),
                    Ok("{\"message\":{\"content\":\"\"},\"done\":true}\n"),
                ])),
            )
        }),
    );
    tokio::spawn(async move {
        axum::serve(provider_listener, provider_app).await.unwrap();
    });
    let mut registry = matric_inference::ProviderRegistry::new("ollama".into());
    registry.register(matric_inference::ProviderConfig {
        id: "ollama".into(),
        base_url: format!("http://{provider_addr}"),
        api_key: None,
        capabilities: vec![
            matric_inference::ProviderCapability::Embedding,
            matric_inference::ProviderCapability::Generation,
        ],
        timeout: std::time::Duration::from_secs(5),
        is_default: true,
        health: matric_inference::ProviderHealth::Healthy,
        http_referer: None,
        x_title: None,
    });
    state.inference_runtime.write().unwrap().provider_registry = Arc::new(registry);
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.audit_sink = Arc::new(PostgresAuditSink::new(runtime.clone()));
    state
}

fn oidc_request(
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) -> axum::http::Request<Body> {
    let mut builder = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, external_oidc_test_issuer::bearer())
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(memory) = memory {
        builder = builder.header("x-fortemi-memory", memory);
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

async fn call(
    app: &Router,
    method: Method,
    path: &str,
    body: serde_json::Value,
    memory: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(oidc_request(method, path, body, memory))
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
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
async fn external_oidc_production_archives_catalog_rollback_and_attachments(admin: sqlx::PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    use sqlx::ConnectOptions;
    let scoped_admin = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    admin.close().await;
    let admin = scoped_admin;
    let tenant = Uuid::new_v4();
    let other = Uuid::new_v4();
    for id in [tenant, other] {
        external_oidc::ensure_default_tenant(&admin, id)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM document_type WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&admin)
        .await
        .unwrap();
    let local_types: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM document_type d WHERE tenant_id=$1",
    )
    .bind(Uuid::nil())
    .fetch_one(&admin)
    .await
    .unwrap();
    let owner_pool = matric_db::create_pool_with_config(
        admin.connect_options().to_url_lossy().as_str(),
        matric_db::PoolConfig::new().single_tenant(tenant),
    )
    .await
    .unwrap();
    let db = Database::new(owner_pool);
    let named = db
        .archives
        .create_archive_schema("external_named", None)
        .await
        .unwrap();
    let default = db
        .archives
        .create_archive_schema("external_default", None)
        .await
        .unwrap();
    // Avoid two intentionally conflicting deployment defaults in this fixture.
    sqlx::query("UPDATE archive_registry SET is_default=false WHERE tenant_id=$1")
        .bind(Uuid::nil())
        .execute(&admin)
        .await
        .unwrap();
    db.archives
        .set_default_archive("external_default")
        .await
        .unwrap();
    let (app, runtime) = production_router(&admin, tenant).await;
    let other_url = runtime_url(&admin, other).await;
    let other_runtime = matric_db::create_pool_with_config(
        &other_url,
        matric_db::PoolConfig::new().single_tenant(other),
    )
    .await
    .unwrap();
    let (other_app, other_runtime) = production_router_with_pool(other_runtime, other).await;
    let name = "same-tenant-type";
    let path = format!("/api/v1/document-types/{name}");
    let mut ids = Vec::new();
    for (router, size) in [(&app, 111), (&other_app, 222)] {
        let (status, body) = call(router, Method::POST, "/api/v1/document-types", serde_json::json!({"name":name,"category":"custom","chunk_size_default":size,"chunk_overlap_default":10}), None).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        ids.push(Uuid::parse_str(body["id"].as_str().unwrap()).unwrap());
        let (status, body) = call(router, Method::GET, &path, serde_json::Value::Null, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["chunk_size_default"], size);
        let (status, body) = call(
            router,
            Method::GET,
            "/api/v1/document-types?category=custom",
            serde_json::Value::Null,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body
            .as_array()
            .unwrap()
            .iter()
            .any(|dt| dt["id"] == ids.last().unwrap().to_string()));
    }
    assert_ne!(ids[0], ids[1]);
    // The local namesake is a system type; normalization must use the external
    // tenant's custom row so PATCH/DELETE do not reject it as a system type.
    sqlx::query("INSERT INTO document_type(id,name,display_name,category,is_system,tenant_id) VALUES($1,$2,'Local','custom',true,$3)")
        .bind(Uuid::new_v4()).bind(name).bind(Uuid::nil()).execute(&admin).await.unwrap();
    let (status, body) = call(
        &app,
        Method::PATCH,
        &path,
        serde_json::json!({"chunk_size_default":333}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (_, body) = call(
        &other_app,
        Method::GET,
        &path,
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(body["chunk_size_default"], 222);
    let (status, body) = call(&app, Method::DELETE, &path, serde_json::Value::Null, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, _) = call(&app, Method::GET, &path, serde_json::Value::Null, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    sqlx::query("DELETE FROM document_type WHERE name=$1 AND tenant_id=$2")
        .bind(name)
        .bind(Uuid::nil())
        .execute(&admin)
        .await
        .unwrap();
    let external_type: Uuid =
        sqlx::query_scalar("SELECT id FROM document_type WHERE name='markdown' AND tenant_id=$1")
            .bind(tenant)
            .fetch_one(&admin)
            .await
            .unwrap();
    let foreign_type: Uuid =
        sqlx::query_scalar("SELECT id FROM document_type WHERE name='markdown' AND tenant_id=$1")
            .bind(other)
            .fetch_one(&admin)
            .await
            .unwrap();
    let mut created = Vec::new();
    for (memory, schema) in [
        (None, default.schema_name.as_str()),
        (Some("external_named"), named.schema_name.as_str()),
    ] {
        let (status, body) = call(&app, Method::POST, "/api/v1/notes", serde_json::json!({"content":"Synthetic note","document_type":"markdown","pipeline":[]}), memory).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let note = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        let (owner, dt, author): (Uuid, Uuid, Uuid) = sqlx::query_as(&format!(
            "SELECT tenant_id,document_type_id,created_by_user_id FROM {schema}.note WHERE id=$1"
        ))
        .bind(note)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!((owner, dt), (tenant, external_type));
        let user_owner: Uuid = sqlx::query_scalar("SELECT tenant_id FROM app_user WHERE id=$1")
            .bind(author)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(user_owner, tenant);
        created.push(note);
    }
    let note = created[1];
    let (status, body) = call(&app, Method::POST, &format!("/api/v1/notes/{note}/attachments"), serde_json::json!({"filename":"synthetic.txt","content_type":"text/plain","data":"c3ludGhldGlj","document_type_id":external_type}), Some("external_named")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let attached: Uuid = sqlx::query_scalar(&format!(
        "SELECT document_type_id FROM {}.attachment WHERE note_id=$1",
        named.schema_name
    ))
    .bind(note)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(attached, external_type);
    let (status, _) = call(&app, Method::POST, &format!("/api/v1/notes/{note}/attachments"), serde_json::json!({"filename":"foreign.txt","content_type":"text/plain","data":"Zm9yZWlnbg==","document_type_id":foreign_type}), Some("external_named")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let boundary = "synthetic-boundary";
    let multipart = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"multipart.txt\"\r\nContent-Type: text/plain\r\n\r\nMultipart synthetic\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"document_type_id\"\r\n\r\n{external_type}\r\n--{boundary}--\r\n");
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/notes/{note}/attachments/upload"))
                .header(header::AUTHORIZATION, external_oidc_test_issuer::bearer())
                .header("x-fortemi-memory", "external_named")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let typed: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {}.attachment WHERE note_id=$1 AND document_type_id=$2 AND tenant_id=$3", named.schema_name))
        .bind(note).bind(external_type).bind(tenant).fetch_one(&admin).await.unwrap();
    assert_eq!(typed, 2);
    for (doc_type, expected) in [
        (external_type, StatusCode::OK),
        (foreign_type, StatusCode::BAD_REQUEST),
    ] {
        let bytes = b"TUS synthetic";
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::POST)
                    .uri(format!(
                        "/api/v1/notes/{note}/attachments/tus?document_type_id={doc_type}"
                    ))
                    .header(header::AUTHORIZATION, external_oidc_test_issuer::bearer())
                    .header("x-fortemi-memory", "external_named")
                    .header("Tus-Resumable", "1.0.0")
                    .header("Upload-Length", bytes.len())
                    .header(
                        "Upload-Metadata",
                        "filename dHVzLnR4dA==,filetype dGV4dC9wbGFpbg==",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let location = response.headers()["Location"].to_str().unwrap().to_owned();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::PATCH)
                    .uri(location)
                    .header(header::AUTHORIZATION, external_oidc_test_issuer::bearer())
                    .header("x-fortemi-memory", "external_named")
                    .header("Tus-Resumable", "1.0.0")
                    .header("Upload-Offset", "0")
                    .header(header::CONTENT_TYPE, "application/offset+octet-stream")
                    .body(Body::from(bytes.as_slice()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = json_body(response).await;
        assert_eq!(status, expected, "{body}");
    }
    let typed: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {}.attachment WHERE note_id=$1 AND document_type_id=$2 AND tenant_id=$3", named.schema_name))
        .bind(note).bind(external_type).bind(tenant).fetch_one(&admin).await.unwrap();
    assert_eq!(typed, 3);
    // Fail the author write after insertion. Production finalization must roll
    // back the note, its child rows and its jobs on the same transaction.
    sqlx::query(&format!("CREATE FUNCTION {}.reject_author() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Synthetic author failure'; END $$", default.schema_name)).execute(&admin).await.unwrap();
    sqlx::query(&format!("CREATE TRIGGER reject_author BEFORE UPDATE OF created_by_user_id ON {}.note FOR EACH ROW EXECUTE FUNCTION {}.reject_author()", default.schema_name, default.schema_name)).execute(&admin).await.unwrap();
    let before: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {}.note",
        default.schema_name
    ))
    .fetch_one(&admin)
    .await
    .unwrap();
    for (body, expected) in [
        (
            serde_json::json!({"content":"Rejected author","pipeline":[]}),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            serde_json::json!({"content":"Foreign type","document_type_id":foreign_type,"pipeline":[]}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, _) = call(&app, Method::POST, "/api/v1/notes", body, None).await;
        assert_eq!(status, expected);
    }
    let after: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {}.note",
        default.schema_name
    ))
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(before, after);
    let local_notes: i64 = sqlx::query_scalar("SELECT count(*) FROM note WHERE tenant_id=$1")
        .bind(Uuid::nil())
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(local_notes, 0);
    let after_types: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM document_type d WHERE tenant_id=$1",
    )
    .bind(Uuid::nil())
    .fetch_one(&admin)
    .await
    .unwrap();
    assert!(after_types.is_none(), "L types must be adopted");
    let adopted_types: serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(d)||jsonb_build_object('tenant_id',$2::uuid) ORDER BY id) FROM document_type d WHERE tenant_id=$1")
        .bind(tenant).bind(Uuid::nil()).fetch_one(&admin).await.unwrap();
    assert_eq!(local_types, adopted_types);
    runtime.close().await;
    other_runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_oidc_production_named_archive_with_forced_rls(admin: sqlx::PgPool) {
    use sqlx::ConnectOptions;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let local = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let archive = Database::new(local.clone())
        .archives
        .create_archive_schema("legacy_local_catalog", None)
        .await
        .unwrap();
    Database::new(local.clone())
        .archives
        .set_default_archive("legacy_local_catalog")
        .await
        .unwrap();
    let tenant = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Unrelated','active')").bind(foreign).bind(foreign.to_string()).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO document_type(id,tenant_id,name,display_name,category) VALUES(uuidv7(),$1,'foreign-sentinel','Foreign','custom')").bind(foreign).execute(&admin).await.unwrap();
    let url = runtime_url(&admin, tenant).await;
    let role = format!("external_accept_{}", tenant.simple());
    let db = startup::database(
        &url,
        Some(admin.connect_options().to_url_lossy().as_str()),
        startup::Tenancy::External(tenant),
    )
    .await
    .unwrap();
    db.pool.close().await;
    let runtime = matric_db::create_pool_with_config(
        &url,
        matric_db::PoolConfig::new()
            .single_tenant(tenant)
            .max_connections(1),
    )
    .await
    .unwrap();
    let (app, runtime) = production_router_with_pool(runtime, tenant).await;
    let mut embedding_jobs = Vec::new();
    for memory in [None, Some("legacy_local_catalog")] {
        let (status, body) = call(
            &app,
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"RLS external note","document_type":"markdown"}),
            memory,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        let owner: Uuid = sqlx::query_scalar(&format!(
            "SELECT tenant_id FROM {}.note WHERE id=$1",
            archive.schema_name
        ))
        .bind(id)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(owner, tenant);
        // Embedding normally chains after revision/tagging; queue that downstream
        // stage without requiring an external inference service in this RLS test.
        let author: Uuid = sqlx::query_scalar(&format!(
            "SELECT created_by_user_id FROM {}.note WHERE id=$1",
            archive.schema_name
        ))
        .bind(id)
        .fetch_one(&admin)
        .await
        .unwrap();
        let job = Database::new(runtime.clone())
            .jobs
            .queue_with_initiator(
                Some(id),
                matric_core::JobType::Embedding,
                1,
                Some(serde_json::json!({"schema":archive.schema_name,"tenant_id":tenant})),
                None,
                Some(author),
            )
            .await
            .unwrap();
        embedding_jobs.push((id, job));
    }
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE tenant_id=$1 AND initiated_by_user_id IS NOT NULL AND payload->>'tenant_id'=$2 AND payload->>'schema'=$3")
        .bind(tenant).bind(tenant.to_string()).bind(&archive.schema_name).fetch_one(&admin).await.unwrap();
    assert!(queued > 0);
    // A one-connection pool proves catalog handlers and normalization reuse
    // the request transaction instead of acquiring a second pool connection.
    let (status, body) = call(
        &app,
        Method::GET,
        "/api/v1/document-types/markdown",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = call(
        &app,
        Method::GET,
        "/api/v1/document-types",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.as_array().unwrap().is_empty());
    let catalog_path = "/api/v1/document-types/rls-custom";
    let (status, body) = call(
        &app,
        Method::POST,
        "/api/v1/document-types",
        serde_json::json!({"name":"rls-custom","category":"custom"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = call(
        &app,
        Method::PATCH,
        catalog_path,
        serde_json::json!({"chunk_size_default":777}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, body) = call(
        &app,
        Method::DELETE,
        catalog_path,
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    use matric_core::JobRepository;
    use matric_jobs::{JobContext, JobHandler, JobResult};
    let handler = handlers::jobs::EmbeddingHandler::new(
        Database::new(runtime.clone()),
        Arc::new(matric_inference::ProviderRegistry::from_env()),
        Arc::new(matric_core::InMemoryMeter::default()),
    )
    .with_backend_override(Arc::new(upgrade::SyntheticEmbedding));
    let mut completed = 0;
    while let Some(job) = Database::new(runtime.clone())
        .jobs
        .claim_next_for_types(&[matric_core::JobType::Embedding])
        .await
        .unwrap()
    {
        let id = job.id;
        let result = handler.execute(JobContext::new(job)).await;
        let JobResult::Success(result) = result else {
            panic!("{result:?}");
        };
        Database::new(runtime.clone())
            .jobs
            .complete(id, result)
            .await
            .unwrap();
        completed += 1;
    }
    assert_eq!(completed, embedding_jobs.len());
    for (note, job) in embedding_jobs {
        let job = Database::new(runtime.clone())
            .jobs
            .get(job)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.status, matric_core::JobStatus::Completed);
        let embeddings: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {}.embedding WHERE note_id=$1 AND tenant_id=$2",
            archive.schema_name
        ))
        .bind(note)
        .bind(tenant)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert!(
            embeddings > 0,
            "new E-owned note must be embedded under RLS"
        );
    }
    let ambient: String = sqlx::query_scalar("SELECT current_setting('app.current_tenant')")
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(ambient, tenant.to_string());
    let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM document_type WHERE tenant_id<>$1")
        .bind(tenant)
        .fetch_one(&runtime)
        .await
        .unwrap();
    assert_eq!(leaked, 0);
    let sentinel: Uuid =
        sqlx::query_scalar("SELECT tenant_id FROM document_type WHERE name='foreign-sentinel'")
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(sentinel, foreign);
    drop(app);
    runtime.close().await;
    local.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
}

#[path = "external_oidc_upgrade_tests.rs"]
mod upgrade;

#[path = "external_oidc_pool_tests.rs"]
mod pool_tests;

#[path = "external_oidc_event_tests.rs"]
mod events;
