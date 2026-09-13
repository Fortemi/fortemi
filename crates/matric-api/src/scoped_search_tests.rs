//! Real hosted middleware/SQL with fixture identities and synthetic embeddings.
//! This does not replace the authority's JWT signature/JWKS qualification.
use super::*;
use matric_core::{ArchiveRepository, CreateNoteRequest};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

mod binary_http;
mod installed_http;
mod memory_context;
mod resolution_checks;

struct FixtureIdentity(Uuid, Uuid);

struct DenyNoteReads;

#[async_trait::async_trait]
impl AuthorizationPolicy for DenyNoteReads {
    async fn authorize(
        &self,
        principal: &AuthPrincipal,
        action: &matric_core::Action,
        resource: &matric_core::Resource,
        context: &matric_core::AuthzContext,
    ) -> Result<Decision, matric_core::AuthzError> {
        if resource.kind == ResourceKind::Note && resource.id.is_some() {
            return Ok(Decision::Deny {
                reason: DenyReason::InvalidResource,
                policy_id: self.policy_id().into(),
                policy_version: self.policy_version().into(),
            });
        }
        RoleBasedPolicy
            .authorize(principal, action, resource, context)
            .await
    }
    fn policy_id(&self) -> &'static str {
        "fixture-note-denial"
    }
    fn policy_version(&self) -> &'static str {
        "1"
    }
}

#[async_trait::async_trait]
impl HostedAuthenticator for FixtureIdentity {
    async fn authenticate(
        &self,
        token: &str,
    ) -> Result<fortemi_auth_core::AuthContext, fortemi_auth_core::AuthError> {
        let (tenant_id, scopes) = match token {
            "fixture-a" => (self.0, vec!["read".into()]),
            "fixture-b" => (self.1, vec!["read".into()]),
            "fixture-denied" => (self.0, vec!["mcp".into()]),
            _ => return Err(fortemi_auth_core::AuthError::MalformedToken),
        };
        Ok(fortemi_auth_core::AuthContext {
            tenant_id,
            scopes,
            principal_id: "search-fixture".into(),
            credential: fortemi_auth_core::Credential::Bearer(fortemi_auth_core::JwtToken {
                jti: None,
                algorithm: "RS256".into(),
                key_id: "fixture-only".into(),
            }),
            issued_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            session_id: None,
        })
    }
}

fn router(state: AppState, pool: PgPool) -> Router {
    Router::new()
        .route("/api/v1/notes/{id}", get(get_note))
        .route("/api/v1/search", get(search_notes))
        .route(
            "/api/v1/search/evidence/resolve",
            evidence_resolution::route(),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            pool,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

async fn response(
    app: &Router,
    token: Option<&str>,
    memory: &str,
    params: &[(&str, String)],
) -> (StatusCode, Value) {
    let mut request = axum::http::Request::builder()
        .uri(format!(
            "/api/v1/search?{}",
            serde_urlencoded::to_string(params).unwrap()
        ))
        .header("X-Fortemi-Memory", memory);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    // A one-connection runtime pool exposes any accidental unbound pool checkout.
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.clone().oneshot(request.body(Body::empty()).unwrap()),
    )
    .await
    .expect("request must not acquire a second runtime connection")
    .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1048576)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn params(mode: &str) -> Vec<(&'static str, String)> {
    vec![
        ("q", "needle".into()),
        ("mode", mode.into()),
        ("limit", "1".into()),
    ]
}

async fn resolve_response(
    app: &Router,
    token: Option<&str>,
    memory: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/search/evidence/resolve")
        .header("X-Fortemi-Memory", memory)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        app.clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap()),
    )
    .await
    .expect("resolver must use the request connection")
    .unwrap();
    let status = response.status();
    if status.is_success() || status == StatusCode::BAD_REQUEST {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let bytes = axum::body::to_bytes(response.into_body(), 1048576)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn seed(db: &Database, schema: &str, tenant: Uuid, label: &str) -> Uuid {
    matric_db::validate_schema_name(schema).unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT set_config('app.current_tenant', $1, true), set_config('search_path', $2, true)",
    )
    .bind(tenant.to_string())
    .bind(format!("{schema}, public"))
    .execute(&mut *tx)
    .await
    .unwrap();
    // Configuration profiles are tenant-local global objects; sets are archive-local.
    let config: Uuid = sqlx::query_scalar("INSERT INTO public.embedding_config (name, model, dimension, provider, tenant_id) VALUES ($1, 'synthetic-only', 768, 'openai', $2) RETURNING id")
        .bind(format!("profile-{schema}")).bind(tenant).fetch_one(&mut *tx).await.unwrap();
    let set: Uuid = sqlx::query_scalar("INSERT INTO embedding_set (name, slug, embedding_config_id, tenant_id) VALUES ('scoped', 'scoped', $1, $2) RETURNING id")
        .bind(config).bind(tenant).fetch_one(&mut *tx).await.unwrap();
    let mut wanted = Uuid::nil();
    for kind in [
        "wanted",
        "wrong-metadata",
        "wrong-tag",
        "wrong-set",
        "deleted",
        "archived",
        "null-vector",
    ] {
        for i in 0..if kind == "wanted" { 1 } else { 4 } {
            let id = db.notes.insert_tx(&mut tx, CreateNoteRequest {
                content: if kind == "null-vector" { "unrelated pending content".into() } else { format!("needle {label} {kind}") }, format: "markdown".into(), source: "search-fixture".into(),
                collection_id: None, document_type_id: None,
                title: Some(if kind == "wanted" { label } else if kind == "null-vector" { "unrelated" } else { "needle" }.into()),
                tags: Some(vec![if kind == "wrong-tag" { "other" } else { "allowed" }.into()]),
                metadata: Some(if kind == "wrong-metadata" { json!({"provider":"other","model":"42"}) }
                    else { json!({"provider":"fixture","model":42,"role":"assistant","event_kind":"result","sensitivity":"internal"}) }),
            }).await.unwrap();
            if kind == "wanted" {
                wanted = id;
            }
            let mut values = vec![1.0_f32; 768];
            if kind == "wanted" {
                values[0] = 10.0;
            }
            sqlx::query("INSERT INTO embedding (note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, 0, 'needle', $2, 'synthetic-only', $3)")
                .bind(id).bind((kind != "null-vector").then(|| pgvector::Vector::from(values))).bind(set).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO source_identity (note_id, source_namespace, external_id, source_schema_version, content_digest, import_run_id) VALUES ($1, 'synthetic', $2, '1', $3, $4)")
                .bind(id).bind(format!("{kind}-{i}")).bind(format!("sha256:{}", "0".repeat(64)))
                .bind(if kind == "wrong-metadata" { "other" } else { "run-42" }).execute(&mut *tx).await.unwrap();
            match kind {
                "wrong-set" => {
                    sqlx::query("DELETE FROM embedding_set_member WHERE note_id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "deleted" => {
                    sqlx::query("UPDATE note SET deleted_at=now() WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "archived" => {
                    sqlx::query("UPDATE note SET archived=true WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    }
    // Sort before the usable embedding by UUID to expose NULL-sensitive MMR.
    let pending_id = Uuid::from_u128(match label {
        "tenant-a-public" => 1,
        "tenant-b-public" => 2,
        "tenant-a-archive" => 3,
        _ => panic!("unexpected fixture scope"),
    });
    sqlx::query("INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, $2, 1, 'pending', NULL, 'synthetic-only', $3)")
        .bind(pending_id).bind(wanted).bind(set).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    wanted
}

#[sqlx::test(migrations = false)]
async fn hosted_search_preserves_authorization_archive_and_candidate_scope(admin: PgPool) {
    run_scoped_fixture(admin, false).await;
}

#[sqlx::test(migrations = false)]
#[ignore = "requires clean-installed Core package inside the bounded private fixture"]
async fn hosted_installed_core_search_resolution_http(admin: PgPool) {
    run_scoped_fixture(admin, true).await;
}

async fn run_scoped_fixture(admin: PgPool, installed_http: bool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let db = Database::new(admin.clone());
    db.migrate().await.expect("full search fixture migrations");
    let provision = sqlx::postgres::PgPoolOptions::new().max_connections(2)
        .after_connect(|connection, _| Box::pin(async move {
            sqlx::query("SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', false)")
                .execute(connection).await?;
            Ok(())
        })).connect_with((*admin.connect_options()).clone()).await.unwrap();
    let db = Database::new(provision.clone());
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    for tenant in [a, b] {
        sqlx::query(
            "INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')",
        )
        .bind(tenant)
        .bind(format!("search-{tenant}"))
        .execute(&admin)
        .await
        .unwrap();
    }
    let archive = db
        .archives
        .create_archive_schema("search_fixture", None)
        .await
        .unwrap_or_else(|error| match error {
            matric_core::Error::Database(source) => panic!("isolated archive fixture: {source}"),
            other => panic!("isolated archive fixture: {other}"),
        });
    sqlx::query("UPDATE public.archive_registry SET tenant_id=$1 WHERE id=$2")
        .bind(a)
        .bind(archive.id)
        .execute(&admin)
        .await
        .unwrap();
    let a_public = seed(&db, "public", a, "tenant-a-public").await;
    let b_public = seed(&db, "public", b, "tenant-b-public").await;
    let a_archive = seed(&db, &archive.schema_name, a, "tenant-a-archive").await;

    let role = format!("search_{}", Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for schema in ["public", archive.schema_name.as_str()] {
        for grant in [
            format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
            format!("GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO {role}"),
        ] {
            sqlx::query(&grant).execute(&admin).await.unwrap();
        }
    }
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*admin.connect_options()).clone().username(&role))
        .await
        .unwrap();
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert!(!privileged);

    let calls = Arc::new(AtomicUsize::new(0));
    let embedding_calls = calls.clone();
    let embedding_router = Router::new()
        .route("/models", get(|| async { Json(json!({"object":"list","data":[{"id":"synthetic-only","object":"model"}]})) }))
        .route("/embeddings", post(move |Json(body): Json<Value>| {
        let calls = embedding_calls.clone();
        async move {
            assert_eq!(body["model"], "synthetic-only");
            calls.fetch_add(1, Ordering::SeqCst);
            Json(json!({"object":"list","model":"synthetic-only","data":[{"object":"embedding","index":0,"embedding":vec![1.0_f32;768]}],"usage":{"prompt_tokens":1,"total_tokens":1}}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let embedding_server = tokio::spawn(async move {
        axum::serve(listener, embedding_router).await.unwrap();
    });
    let mut registry = matric_inference::ProviderRegistry::new("openai".into());
    registry.register(matric_inference::ProviderConfig {
        id: "openai".into(),
        base_url: url.clone(),
        api_key: None,
        capabilities: vec![matric_inference::ProviderCapability::Embedding],
        timeout: std::time::Duration::from_secs(2),
        is_default: true,
        health: matric_inference::ProviderHealth::Healthy,
        http_referer: None,
        x_title: None,
    });
    let mut state =
        tests::build_call_api_test_state(Database::new(runtime.clone()), "fixture").await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.hosted_auth = Some(Arc::new(FixtureIdentity(a, b)));
    state.inference_runtime.write().unwrap().provider_registry = Arc::new(registry);
    let unscoped_app = Router::new()
        .route(
            "/api/v1/search/evidence/resolve",
            evidence_resolution::route(),
        )
        .layer(Extension(ArchiveContext::default()))
        .with_state(state.clone());
    let mut denied_state = state.clone();
    denied_state.authorization_policy = Arc::new(DenyNoteReads);
    let denied_app = router(denied_state, runtime.clone());
    let app = router(state.clone(), runtime.clone());

    for (token, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("invalid"), StatusCode::UNAUTHORIZED),
        (Some("fixture-denied"), StatusCode::FORBIDDEN),
    ] {
        let (status, _) = response(&app, token, "public", &params("semantic")).await;
        assert_eq!(status, expected);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let openapi: serde_yaml::Value =
        serde_yaml::from_str(&openapi_yaml_with_problem_contract()).unwrap();
    let openapi = serde_json::to_value(openapi).unwrap();
    let response_schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
        "$ref":"#/components/schemas/SearchRestResponse","components":openapi["components"]});
    let response_validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&response_schema)
        .unwrap();
    let mut checks = 0;
    for (token, memory, wanted) in [
        ("fixture-a", "public", a_public),
        ("fixture-b", "public", b_public),
        ("fixture-a", "search_fixture", a_archive),
        ("fixture-a", "public", a_public),
    ] {
        for mode in ["fts", "semantic", "hybrid"] {
            for predicate in [
                json!({"path":"provider","op":"eq","value":"fixture"}),
                json!({"path":"model","op":"eq","value":42}),
                json!({"path":"role","op":"eq","value":"assistant"}),
                json!({"path":"event_kind","op":"eq","value":"result"}),
                json!({"path":"sensitivity","op":"eq","value":"internal"}),
                json!({"path":"import_run_id","op":"eq","value":"run-42"}),
            ] {
                let mut query = params(mode);
                query.extend([
                    ("metadata_predicates", json!([predicate]).to_string()),
                    (
                        "strict_filter",
                        json!({"required_tags":["allowed"]}).to_string(),
                    ),
                    ("set", "scoped".into()),
                    ("diversity", "0.5".into()),
                ]);
                let (status, body) = response(&app, Some(token), memory, &query).await;
                assert_eq!(status, StatusCode::OK, "{mode} {memory}: {body}");
                assert!(
                    response_validator.is_valid(&body),
                    "hosted response violates generated schema: {mode} {memory}"
                );
                assert_eq!(
                    body["degraded"], false,
                    "synthetic embedding must run: {body}"
                );
                assert_eq!(body["total"], 1, "{mode} {memory}: {body}");
                let decoded: EnhancedSearchHit =
                    serde_json::from_value(body["results"][0].clone()).unwrap();
                let evidence = decoded
                    .hit
                    .evidence
                    .as_ref()
                    .expect("hosted response must retain matched evidence");
                assert!(!evidence.locators().is_empty());
                evidence
                    .validate_note(&decoded.hit.note_id.to_string())
                    .unwrap();
                assert_eq!(
                    body["results"][0]["note_id"],
                    wanted.to_string(),
                    "{mode} {memory}: {body}"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 48);
    let mut resolution_checks = 0;
    for (token, memory) in [
        ("fixture-a", "public"),
        ("fixture-b", "public"),
        ("fixture-a", "search_fixture"),
    ] {
        let (status, body) = response(&app, Some(token), memory, &params("fts")).await;
        assert_eq!(status, StatusCode::OK);
        let locator = body["results"][0]["evidence"]["locators"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["unit"]["kind"] == "current")
            .unwrap()
            .clone();
        let (status, resolved) =
            resolve_response(&app, Some(token), memory, json!({"locator":locator})).await;
        assert_eq!(status, StatusCode::OK, "stored evidence must resolve");
        assert!(resolved["text"].as_str().unwrap().contains("needle"));
        resolution_checks += 1;
        let (status, body) =
            resolve_response(&denied_app, Some(token), memory, json!({"locator":locator})).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "specific note policy must run after route admission"
        );
        assert_eq!(body["detail"], "SEARCH_EVIDENCE_UNAVAILABLE");
        resolution_checks += 1;
        let (status, _) = resolve_response(
            &unscoped_app,
            Some(token),
            memory,
            json!({"locator":locator}),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "hosted resolution cannot fall back to a personal pool"
        );
        resolution_checks += 1;
    }
    let before = calls.load(Ordering::SeqCst);
    let mut malformed = params("semantic");
    malformed.push((
        "metadata_predicates",
        json!([{"path":"private-value","op":"eq","value":"not-indexed"}]).to_string(),
    ));
    let (status, body) = response(&app, Some("fixture-a"), "public", &malformed).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains("private-value"));
    let (status, body) = response(
        &app,
        Some("fixture-b"),
        "search_fixture",
        &params("semantic"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.to_string().contains(&a_archive.to_string()));
    for (name, value) in [
        ("limit", "-1"),
        ("limit", "1001"),
        ("diversity", "NaN"),
        ("diversity", "inf"),
        ("diversity", "-inf"),
    ] {
        let mut invalid = params("semantic");
        invalid.retain(|(key, _)| *key != name);
        invalid.push((name, value.into()));
        let (status, _) = response(&app, Some("fixture-a"), "public", &invalid).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(calls.load(Ordering::SeqCst), before);
    resolution_checks += resolution_checks::verify(
        &app,
        &admin,
        a,
        b,
        &archive.schema_name,
        a_public,
        b_public,
        a_archive,
    )
    .await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "resolution must not invoke inference"
    );
    if installed_http {
        for schema in ["public", archive.schema_name.as_str()] {
            // Real note-detail enrichment records access, but must not acquire
            // content-update authority or bypass tenant RLS in this fixture.
            sqlx::query(&format!(
                "GRANT UPDATE(last_accessed_at, access_count) ON {schema}.note TO {role}"
            ))
            .execute(&admin)
            .await
            .unwrap();
            sqlx::query(&format!(
                "GRANT INSERT ON {schema}.note_access_log TO {role}"
            ))
            .execute(&admin)
            .await
            .unwrap();
        }
        let mut http_app = app.clone();
        let mut http_denied = denied_app.clone();
        let mut untrusted = None;
        let mut cold = None;
        if let Ok(issuer) = std::env::var("FORTEMI_TEST_ISSUER") {
            let config = HostedAuthConfig {
                issuer,
                audience: "fortemi-http-fixture".into(),
                tenant_claim_name: "fortemi:tenant_id".into(),
                clock_skew_seconds: 0,
                jwks_cache_capacity: 2,
                http_timeout_seconds: 2,
                ca_bundle_path: Some(std::env::var("FORTEMI_TEST_CA").unwrap()),
            };
            state.hosted_auth = Some(build_clerk_authenticator(&config, runtime.clone()).unwrap());
            http_app = router(state.clone(), runtime.clone());
            let mut denied = state.clone();
            denied.authorization_policy = Arc::new(DenyNoteReads);
            http_denied = router(denied, runtime.clone());
            let mut cold_state = state.clone();
            cold_state.hosted_auth =
                Some(build_clerk_authenticator(&config, runtime.clone()).unwrap());
            cold = Some(router(cold_state, runtime.clone()));
            let mut no_trust = config;
            no_trust.ca_bundle_path = None;
            state.hosted_auth =
                Some(build_clerk_authenticator(&no_trust, runtime.clone()).unwrap());
            untrusted = Some(router(state.clone(), runtime.clone()));
        }
        // Schema provisioning is complete. Release owner connections before
        // concurrent HTTP/SSE acceptance in the bounded private cluster.
        provision.close().await;
        assert!(admin.options().get_max_connections() <= 5);
        assert_eq!(runtime.options().get_max_connections(), 1);
        installed_http::verify(
            &http_app,
            &http_denied,
            untrusted.as_ref(),
            cold.as_ref(),
            &admin,
            &runtime,
            a,
            b,
            &archive.schema_name,
            &url,
        )
        .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            before + 6,
            "HTTP matrix uses six synthetic query embeddings"
        );
    }
    let context: (Option<String>, String) = sqlx::query_as("SELECT nullif(current_setting('app.current_tenant', true), ''), current_setting('search_path')")
        .fetch_one(&runtime).await.unwrap();
    assert_eq!(context.0, None);
    assert!(!context.1.contains(&archive.schema_name));
    embedding_server.abort();
    assert!(embedding_server.await.unwrap_err().is_cancelled());
    drop(app);
    drop(denied_app);
    drop(unscoped_app);
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    provision.close().await;
    println!("hosted_search: {checks} scoped requests; auth negatives=3; invalid-before-inference=7; synthetic embeddings=48; runtime role removed");
    println!(
        "hosted_search_schema: {checks} actual router responses match offline generated OpenAPI"
    );
    println!("hosted_evidence_resolution: {resolution_checks} current-storage checks");
}
