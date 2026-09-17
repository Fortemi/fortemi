use super::*;
use axum::http::HeaderValue;

fn app(state: AppState, runtime: PgPool) -> Router {
    Router::new()
        .route("/api/v1/memory/context", get(get_memory_context))
        .route("/api/v1/archives", get(list_archives))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authorize_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_routing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            runtime,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

async fn request(
    app: &Router,
    path: &str,
    token: Option<&str>,
    memory: Option<HeaderValue>,
) -> (StatusCode, Value) {
    let mut request = axum::http::Request::builder().uri(path);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(memory) = memory {
        request = request.header("X-Fortemi-Memory", memory);
    }
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        app.clone().oneshot(request.body(Body::empty()).unwrap()),
    )
    .await
    .expect("memory resolution must not acquire a second connection")
    .unwrap();
    let status = response.status();
    if status.is_success() {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    } else {
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
    }
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[sqlx::test(migrations = false)]
async fn hosted_memory_context_is_tenant_bound_and_read_only(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    for tenant in [a, b] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("context-{tenant}")).execute(&admin).await.unwrap();
    }
    let original_defaults: i64 = sqlx::query_scalar("SELECT count(*) FROM public.archive_registry WHERE tenant_id='00000000-0000-0000-0000-000000000000' AND is_default")
        .fetch_one(&admin).await.unwrap();
    assert_eq!(original_defaults, 1);
    let provision = sqlx::postgres::PgPoolOptions::new().max_connections(1)
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SELECT set_config('app.current_tenant','00000000-0000-0000-0000-000000000000',false)").execute(c).await?;
            Ok(())
        })).connect_with((*admin.connect_options()).clone()).await.unwrap();
    let db = Database::new(provision.clone());
    let first = db
        .archives
        .create_archive_schema("context_alpha", None)
        .await
        .unwrap();
    let second = db
        .archives
        .create_archive_schema("context_beta", None)
        .await
        .unwrap();
    for (tenant, archive) in [(a, &first), (b, &second)] {
        sqlx::query("UPDATE public.archive_registry SET tenant_id=$1,is_default=true WHERE id=$2")
            .bind(tenant)
            .bind(archive.id)
            .execute(&admin)
            .await
            .unwrap();
    }
    provision.close().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM public.archive_registry WHERE is_default"
        )
        .fetch_one(&admin)
        .await
        .unwrap(),
        3
    );
    let duplicate = sqlx::query("INSERT INTO public.archive_registry(name,schema_name,tenant_id,is_default) VALUES('duplicate_context','archive_duplicate_context',$1,true)")
        .bind(a).execute(&admin).await.unwrap_err();
    assert_eq!(
        duplicate.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    let forced: bool = sqlx::query_scalar("SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid='public.archive_registry'::regclass")
        .fetch_one(&admin).await.unwrap();
    assert!(forced);
    let role = format!("memory_context_{}", Uuid::new_v4().simple());
    let password = Uuid::new_v4().simple().to_string();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*admin.connect_options())
                .clone()
                .username(&role)
                .password(&password),
        )
        .await
        .unwrap();
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user"
    )
    .fetch_one(&runtime)
    .await
    .unwrap());
    let mut state =
        tests::build_call_api_test_state(Database::new(runtime.clone()), "fixture").await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.hosted_auth = Some(Arc::new(FixtureIdentity(a, b)));
    let router = app(state.clone(), runtime.clone());
    let path = "/api/v1/memory/context";
    let spec: Value = serde_json::to_value(
        serde_yaml::from_str::<serde_yaml::Value>(&openapi_yaml_with_problem_contract()).unwrap(),
    )
    .unwrap();
    let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$ref":"#/components/schemas/MemoryContextResponse", "components":spec["components"]});
    let validator = jsonschema::options().build(&schema).unwrap();
    let problem_schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$ref":"#/components/schemas/ProblemDetails", "components":spec["components"]});
    let problem_validator = jsonschema::options().build(&problem_schema).unwrap();
    let operation = &spec["paths"][path]["get"];
    assert_eq!(operation["security"], json!([{"bearerAuth":[]}]));
    assert_eq!(
        operation["responses"]["200"]["headers"]["Cache-Control"]["description"],
        "no-store"
    );
    for status in ["400", "401", "403", "404", "429", "500", "503"] {
        assert_eq!(
            operation["responses"][status]["content"]["application/problem+json"]["schema"]["$ref"],
            "#/components/schemas/ProblemDetails"
        );
    }
    let mut checks = 0;
    for (token, memory, expected) in [
        ("fixture-a", None, &first),
        ("fixture-b", None, &second),
        ("fixture-a", Some(first.name.as_str()), &first),
        ("fixture-b", Some(second.name.as_str()), &second),
        ("fixture-a", None, &first),
    ] {
        let (status, body) = request(
            &router,
            path,
            Some(token),
            memory.map(|m| m.parse().unwrap()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({"name":expected.name, "schema_name":expected.schema_name})
        );
        assert!(validator.is_valid(&body));
        checks += 1;
    }
    for (token, memory, expected) in [
        (None, None, StatusCode::UNAUTHORIZED),
        (Some("invalid"), None, StatusCode::UNAUTHORIZED),
        (Some("fixture-denied"), None, StatusCode::FORBIDDEN),
        (
            Some("fixture-b"),
            Some(first.name.as_str()),
            StatusCode::NOT_FOUND,
        ),
        (
            Some("fixture-a"),
            Some(second.name.as_str()),
            StatusCode::NOT_FOUND,
        ),
        (
            Some("fixture-a"),
            Some("absent-memory"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, body) =
            request(&router, path, token, memory.map(|m| m.parse().unwrap())).await;
        assert_eq!(status, expected, "{body}");
        assert!(problem_validator.is_valid(&body));
        assert!(!body.to_string().contains(&first.schema_name));
        assert!(!body.to_string().contains(&second.schema_name));
        checks += 1;
    }
    assert_eq!(
        request(
            &router,
            path,
            Some("fixture-a"),
            Some(HeaderValue::from_bytes(b"invalid\xff").unwrap())
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    checks += 1;
    assert_eq!(
        request(&router, "/api/v1/archives", Some("fixture-a"), None)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    checks += 1;
    // Default changes must be reflected without a process-wide cache or tenant crossover.
    sqlx::query("UPDATE public.archive_registry SET is_default=false WHERE id=$1")
        .bind(first.id)
        .execute(&admin)
        .await
        .unwrap();
    let fallback = json!({"name":"public", "schema_name":"public"});
    assert_eq!(
        request(&router, path, Some("fixture-a"), None).await,
        (StatusCode::OK, fallback.clone())
    );
    checks += 1;
    assert_eq!(
        request(
            &router,
            path,
            Some("fixture-b"),
            Some(HeaderValue::from_static("public"))
        )
        .await,
        (StatusCode::OK, fallback)
    );
    checks += 1;
    assert_eq!(
        request(&router, path, Some("fixture-b"), None).await.1["name"],
        second.name
    );
    checks += 1;
    let missing_scope = Router::new()
        .route(path, get(get_memory_context))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state.clone());
    assert_eq!(
        request(&missing_scope, path, None, None).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    checks += 1;
    let missing_context = Router::new()
        .route(path, get(get_memory_context))
        .layer(axum::middleware::from_fn_with_state(
            runtime.clone(),
            tenant_scope_middleware,
        ))
        .layer(Extension(VerifiedRequestTenant::from_verified(a).unwrap()))
        .layer(Extension(TenantScopeRequired))
        .with_state(state);
    assert_eq!(
        request(&missing_context, path, None, None).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    checks += 1;
    sqlx::query(&format!(
        "REVOKE SELECT ON public.archive_registry FROM {role}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    let (status, body) = request(&router, path, Some("fixture-b"), None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.to_string().contains("archive_registry"));
    checks += 1;
    assert!(!validator.is_valid(&json!({"name":"valid", "schema_name":"public", "size_bytes":0})));
    assert!(!validator.is_valid(&json!({"name":"valid", "schema_name":"UpperCase"})));
    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    println!("hosted_memory_context: {checks} request controls; independent defaults, forced RLS, one runtime connection, generated schema and read/admin boundaries; runtime role removed");
}
