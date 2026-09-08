use super::*;

fn request(extra: serde_json::Value) -> CreateNoteBody {
    let mut body = serde_json::json!({"content": "Hosted note"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(body).unwrap()
}

#[test]
fn hosted_creation_default_jobs_have_verified_tenant_and_archive() {
    let tenant = Uuid::new_v4();
    let body =
        request(serde_json::json!({"model":"custom", "chunk_max_chars":4000, "chunk_overlap":200}));
    let jobs = hosted_create_note_jobs(&body, RevisionMode::Light, false, "archive_test", tenant);
    assert_eq!(
        jobs.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        vec![
            JobType::AiRevision,
            JobType::TitleGeneration,
            JobType::ReferenceExtraction,
            JobType::MetadataExtraction,
            JobType::DocumentTypeInference,
        ]
    );
    for (_, payload) in &jobs {
        assert_eq!(payload["tenant_id"], tenant.to_string());
        assert_eq!(payload["schema"], "archive_test");
        assert_eq!(payload["model"], "custom");
    }
    assert_eq!(jobs[0].1["chunk_max_chars"], 4000);
    assert_eq!(jobs[0].1["chunk_overlap"], 200);
    assert!(jobs[1].1.get("chunk_max_chars").is_none());
}

#[test]
fn hosted_creation_store_only_never_queues_jobs() {
    let body = request(serde_json::json!({"pipeline":[]}));
    assert!(
        hosted_create_note_jobs(&body, RevisionMode::Light, false, "public", Uuid::new_v4())
            .is_empty()
    );
}

#[test]
fn hosted_creation_no_revision_and_explicit_title_preserve_pipeline_selection() {
    let body = request(serde_json::json!({}));
    let jobs = hosted_create_note_jobs(&body, RevisionMode::None, true, "public", Uuid::new_v4());
    assert_eq!(
        jobs.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        vec![
            JobType::ReferenceExtraction,
            JobType::MetadataExtraction,
            JobType::DocumentTypeInference,
            JobType::ConceptTagging,
        ]
    );
    let body = request(serde_json::json!({"pipeline":["concept_tagging"]}));
    let jobs = hosted_create_note_jobs(&body, RevisionMode::Light, false, "public", Uuid::new_v4());
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, JobType::ConceptTagging);
}

#[derive(Clone)]
struct Identity(Uuid);

async fn inject_identity(
    Extension(identity): Extension<Identity>,
    mut request: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let tenant = identity.0.to_string();
    let scopes = if request.headers().contains_key("x-test-mcp-only") {
        "mcp"
    } else {
        "read write mcp"
    };
    request
        .extensions_mut()
        .insert(VerifiedRequestTenant::from_verified(identity.0).unwrap());
    request.extensions_mut().insert(TenantScopeRequired);
    let input = route_policy::authorization_input_for_request(
        request.method(),
        request.uri().path(),
        Some(&tenant),
    )
    .unwrap();
    request.extensions_mut().insert(input);
    request.extensions_mut().insert(Auth {
        principal: AuthPrincipal::OAuthClient {
            client_id: "hosted-create-regression".into(),
            scope: scopes.into(),
            user_id: Some("hosted-create-regression".into()),
        },
    });
    #[cfg(feature = "hosted-auth")]
    {
        let principal = request
            .extensions()
            .get::<Auth>()
            .unwrap()
            .principal
            .clone();
        request.extensions_mut().insert(ValidatedBearerIdentity {
            principal,
            tenant_id: Some(identity.0),
            canonical_context: Some(fortemi_auth_core::AuthContext {
                tenant_id: identity.0,
                principal_id: "hosted-create-regression".into(),
                credential: fortemi_auth_core::Credential::Bearer(fortemi_auth_core::JwtToken {
                    jti: None,
                    algorithm: "RS256".into(),
                    key_id: "test".into(),
                }),
                issued_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::seconds(2),
                scopes: scopes.split_whitespace().map(str::to_string).collect(),
                session_id: None,
            }),
        });
    }
    next.run(request).await
}

// Runs inside tenant middleware: a failure after a successful handler must
// still roll back its rows and discard its queued external effects.
async fn reject_after_handler(
    request: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    let reject = request.headers().contains_key("x-test-rollback");
    let mut response = next.run(request).await;
    if reject {
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    }
    response
}

fn hosted_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route("/api/v1/events", get(sse_events))
        .route("/api/v1/notes", post(create_note))
        .route("/api/v1/notes/{id}", get(get_note).delete(delete_note))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn(reject_after_handler))
        .layer(axum::middleware::from_fn_with_state(
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn(inject_identity))
        .layer(Extension(Identity(tenant)))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state)
}

fn http_request(
    method: Method,
    path: &str,
    body: serde_json::Value,
    rollback: bool,
) -> axum::http::Request<Body> {
    let mut request = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if rollback {
        request = request.header("x-test-rollback", "true");
    }
    request.body(Body::from(body.to_string())).unwrap()
}

async fn json_body(response: Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn count_notes(admin: &sqlx::PgPool, tenant: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM note WHERE tenant_id=$1")
        .bind(tenant)
        .fetch_one(admin)
        .await
        .unwrap()
}

/// This test NEVER consults DATABASE_URL. Its opt-in URL must target a disposable,
/// already migrated database; it creates a fresh, powerless runtime login.
#[tokio::test]
async fn hosted_creation_postgres_atomicity_and_tenant_isolation() {
    use tower::ServiceExt;
    let Ok(url) = std::env::var("FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL") else {
        eprintln!("SKIP: FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL not supplied");
        return;
    };
    let admin = matric_db::create_pool(&url)
        .await
        .expect("explicit test database");
    let role = format!("hc_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"))
        .execute(&admin).await.unwrap();
    for grant in [
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
        format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&grant).execute(&admin).await.unwrap();
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            url.parse::<sqlx::postgres::PgConnectOptions>()
                .unwrap()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    let safe_role: bool = sqlx::query_scalar(
        "SELECT NOT rolsuper AND NOT rolbypassrls AND NOT EXISTS
         (SELECT 1 FROM pg_class WHERE relowner = pg_roles.oid AND relnamespace = 'public'::regnamespace)
         FROM pg_roles WHERE rolname=current_user",
    ).fetch_one(&runtime).await.unwrap();
    assert!(
        safe_role,
        "runtime must not bypass row security or own data tables"
    );
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    for tenant in [a, b, c] {
        sqlx::query(
            "INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')",
        )
        .bind(tenant)
        .bind(format!("create-test-{tenant}"))
        .execute(&admin)
        .await
        .unwrap();
    }
    // A and B share the default notation; C deliberately has no scheme.
    for tenant in [a, b] {
        sqlx::query("INSERT INTO skos_concept_scheme(id,notation,title,is_system,tenant_id) VALUES($1,'default','Test',true,$2)")
        .bind(Uuid::new_v4()).bind(tenant).execute(&admin).await.unwrap();
    }
    let collection_b = Uuid::new_v4();
    let type_b = Uuid::new_v4();
    sqlx::query("INSERT INTO collection(id,name,created_at_utc,tenant_id) VALUES($1,$2,now(),$3)")
        .bind(collection_b)
        .bind(format!("collection-{b}"))
        .bind(b)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO document_type(id,name,display_name,category,tenant_id) VALUES($1,$2,'Test','custom',$3)")
        .bind(type_b).bind(format!("doctype-{b}")).bind(b).execute(&admin).await.unwrap();
    let mut state = super::tests::build_call_api_test_state(
        Database::new(runtime.clone()),
        "disposable-hosted-create-test",
    )
    .await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.audit_sink = Arc::new(PostgresAuditSink::new(runtime.clone()));
    let mut events = state.event_bus.subscribe();
    let app_a = hosted_router(state.clone(), runtime.clone(), a);
    let app_b = hosted_router(state.clone(), runtime.clone(), b);
    let app_c = hosted_router(state.clone(), runtime.clone(), c);

    let success = app_a.clone().oneshot(http_request(Method::POST,"/api/v1/notes",
        serde_json::json!({"content":"Tenant A private content", "title":"Explicit title", "tags":["qualification/rust"], "pipeline":[]}),false)).await.unwrap();
    let status = success.status();
    let body = json_body(success).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let event = events.try_recv().expect("creation event after commit");
    assert_eq!(event.event_type, "note.created");
    assert_eq!(event.tenant_id.as_deref(), Some(a.to_string().as_str()));
    assert_eq!(count_notes(&admin, a).await, 1);
    let skos_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM note_skos_concept WHERE note_id=$1 AND tenant_id=$2",
    )
    .bind(note_id)
    .bind(a)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(skos_count, 1);
    let path = format!("/api/v1/notes/{note_id}");
    let own = app_a
        .clone()
        .oneshot(http_request(
            Method::GET,
            &path,
            serde_json::Value::Null,
            false,
        ))
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);
    assert!(json_body(own)
        .await
        .to_string()
        .contains("Tenant A private content"));
    for method in [Method::GET, Method::DELETE] {
        let denied = app_b
            .clone()
            .oneshot(http_request(method, &path, serde_json::Value::Null, false))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert!(!json_body(denied)
            .await
            .to_string()
            .contains("Tenant A private content"));
    }
    assert!(
        events.try_recv().is_err(),
        "cross-tenant denial must not emit events"
    );
    for reference in [
        serde_json::json!({"collection_id":collection_b}),
        serde_json::json!({"document_type_id":type_b}),
    ] {
        let mut body = serde_json::json!({"content":"Invisible reference", "pipeline":[]});
        body.as_object_mut()
            .unwrap()
            .extend(reference.as_object().unwrap().clone());
        let rejected = app_a
            .clone()
            .oneshot(http_request(Method::POST, "/api/v1/notes", body, false))
            .await
            .unwrap();
        assert!(rejected.status().is_client_error());
        assert_eq!(count_notes(&admin, a).await, 1);
        assert!(events.try_recv().is_err());
    }
    let denial_tenants: Vec<Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM audit_event WHERE resource_id=$1 AND outcome='Denied'",
    )
    .bind(note_id.to_string())
    .fetch_all(&admin)
    .await
    .unwrap();
    assert_eq!(
        denial_tenants,
        vec![b, b],
        "denials are attributed to the caller tenant"
    );
    sqlx::query(&format!("REVOKE INSERT ON audit_event FROM {role}"))
        .execute(&admin)
        .await
        .unwrap();
    let unauditable = app_a
        .clone()
        .oneshot(http_request(
            Method::GET,
            &path,
            serde_json::Value::Null,
            false,
        ))
        .await
        .unwrap();
    assert_eq!(unauditable.status(), StatusCode::SERVICE_UNAVAILABLE);
    sqlx::query(&format!("GRANT INSERT ON audit_event TO {role}"))
        .execute(&admin)
        .await
        .unwrap();

    // Failure after the note and flat tags were inserted rolls all of them back.
    let missing_scheme = app_c
        .clone()
        .oneshot(http_request(
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Missing scheme", "tags":["rollback-tag"], "pipeline":[]}),
            false,
        ))
        .await
        .unwrap();
    assert!(missing_scheme.status().is_client_error() || missing_scheme.status().is_server_error());
    assert_eq!(count_notes(&admin, c).await, 0);
    let tags_c: i64 = sqlx::query_scalar("SELECT count(*) FROM tag WHERE tenant_id=$1")
        .bind(c)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!(tags_c, 0);
    assert!(events.try_recv().is_err());

    let shared_tag = app_b.clone().oneshot(http_request(Method::POST, "/api/v1/notes",
        serde_json::json!({"content":"Tenant B same tag", "tags":["qualification/rust"], "pipeline":[]}), false)).await.unwrap();
    let status = shared_tag.status();
    let body = json_body(shared_tag).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        events.try_recv().unwrap().tenant_id.as_deref(),
        Some(b.to_string().as_str())
    );
    let tag_tenants: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT tenant_id FROM tag WHERE name='qualification/rust' AND tenant_id=ANY($1)",
    )
    .bind([a, b])
    .fetch_all(&admin)
    .await
    .unwrap();
    assert_eq!(
        tag_tenants.len(),
        2,
        "same tag names must remain tenant-local"
    );

    // A successful handler followed by middleware rejection never publishes.
    let rollback = app_a
        .clone()
        .oneshot(http_request(
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Late rollback", "pipeline":[]}),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(rollback.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(count_notes(&admin, a).await, 1);
    assert!(
        events.try_recv().is_err(),
        "postcommit callback ran after rollback"
    );

    // Queue failure must fail the entire creation instead of leaving a note
    // whose processing was silently dropped.
    sqlx::query(&format!("REVOKE INSERT ON job_queue FROM {role}"))
        .execute(&admin)
        .await
        .unwrap();
    let queue_failure = app_a
        .clone()
        .oneshot(http_request(
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Queue failure"}),
            false,
        ))
        .await
        .unwrap();
    assert!(queue_failure.status().is_server_error());
    assert_eq!(count_notes(&admin, a).await, 1);
    assert!(events.try_recv().is_err());
    sqlx::query(&format!("GRANT INSERT ON job_queue TO {role}"))
        .execute(&admin)
        .await
        .unwrap();
    let jobs_created = app_a
        .clone()
        .oneshot(http_request(
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"Queue success"}),
            false,
        ))
        .await
        .unwrap();
    let status = jobs_created.status();
    let body = json_body(jobs_created).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let job_note = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let jobs: Vec<(Uuid, serde_json::Value)> =
        sqlx::query_as("SELECT tenant_id,payload FROM job_queue WHERE note_id=$1")
            .bind(job_note)
            .fetch_all(&admin)
            .await
            .unwrap();
    assert_eq!(jobs.len(), 5);
    for (tenant, payload) in jobs {
        assert_eq!(tenant, a);
        assert_eq!(payload["tenant_id"], a.to_string());
        assert_eq!(payload["schema"], "public");
    }
    let mut count = 0;
    while let Ok(event) = events.try_recv() {
        assert_eq!(event.tenant_id.as_deref(), Some(a.to_string().as_str()));
        count += 1;
    }
    assert_eq!(count, 6, "five queue events and one creation event");
    let deleted = app_a
        .oneshot(http_request(
            Method::DELETE,
            &path,
            serde_json::Value::Null,
            false,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let event = events.try_recv().expect("committed delete event");
    assert_eq!(event.event_type, "note.deleted");
    assert_eq!(event.tenant_id.as_deref(), Some(a.to_string().as_str()));
    #[cfg(feature = "hosted-auth")]
    {
        let stream_app = hosted_router(state.clone(), runtime.clone(), a);
        let mut mcp_only = http_request(
            Method::GET,
            "/api/v1/events",
            serde_json::Value::Null,
            false,
        );
        mcp_only
            .headers_mut()
            .insert("x-test-mcp-only", "true".parse().unwrap());
        let denied = stream_app.clone().oneshot(mcp_only).await.unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let emit = |tenant: Uuid, title: &str| {
            state.event_bus.emit_with_context(
                ServerEvent::NoteCreated {
                    note_id: Uuid::new_v4(),
                    title: Some(title.into()),
                    tags: vec![],
                },
                EventContext {
                    tenant_id: Some(tenant.to_string()),
                    memory: Some("public".into()),
                    ..Default::default()
                },
            )
        };
        emit(a, "anchor");
        let anchor = events.try_recv().unwrap().event_id;
        emit(b, "foreign-replay-marker");
        emit(a, "own-replay-marker");
        let mut request = http_request(
            Method::GET,
            "/api/v1/events",
            serde_json::Value::Null,
            false,
        );
        request
            .headers_mut()
            .insert("last-event-id", anchor.to_string().parse().unwrap());
        let response = stream_app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream"));
        let held: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE usename=$1 AND state='idle in transaction'")
            .bind(&role).fetch_one(&admin).await.unwrap();
        assert_eq!(
            held, 0,
            "SSE must release the tenant transaction before streaming"
        );
        emit(b, "foreign-live-marker");
        emit(a, "own-live-marker");
        let bytes = tokio::time::timeout(
            std::time::Duration::from_secs(6),
            axum::body::to_bytes(response.into_body(), 1024 * 1024),
        )
        .await
        .expect("stream closes at canonical token expiry")
        .unwrap();
        let stream = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(stream.contains("own-replay-marker"), "{stream}");
        assert!(stream.contains("own-live-marker"), "{stream}");
        assert!(!stream.contains("foreign-replay-marker"), "{stream}");
        assert!(!stream.contains("foreign-live-marker"), "{stream}");
    }
    // The database is disposable. Remove this test's role and all grants;
    // rows remain available for failure diagnosis until the fixture is removed.
    drop(app_b);
    drop(app_c);
    drop(state);
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
