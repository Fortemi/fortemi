//! Hosted memory export: tenant transaction, read scope, read-only runtime role.
use super::*;

fn app(state: AppState, runtime: PgPool) -> Router {
    Router::new()
        .route(
            "/api/v1/memory/export",
            post(handlers::memory_export::export_memory),
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
            runtime,
            tenant_scope_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

async fn export(app: &Router, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/memory/export")
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
    .expect("export must use the request connection")
    .unwrap();
    let status = response.status();
    if status.is_success() {
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

#[sqlx::test(migrations = false)]
async fn hosted_memory_export_is_tenant_bound_read_scoped_and_read_only(admin: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    Database::new(admin.clone()).migrate().await.unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    for (tenant, title) in [(a, "tenant a note"), (b, "tenant b note")] {
        sqlx::query("INSERT INTO public.tenant_registry(id,slug,display_name,status) VALUES($1,$2,$2,'active')")
            .bind(tenant).bind(format!("export-{tenant}")).execute(&admin).await.unwrap();
        let mut tx = admin.begin().await.unwrap();
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tenant.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO note (id, format, source, title, created_at_utc, updated_at_utc) VALUES ($1, 'markdown', 'export-fixture', $2, now(), now())")
            .bind(Uuid::now_v7()).bind(title).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }

    // The runtime login can only read: the export must not need any write.
    let role = format!("memory_export_{}", Uuid::new_v4().simple());
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
    let mut state =
        tests::build_call_api_test_state(Database::new(runtime.clone()), "fixture").await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.hosted_auth = Some(Arc::new(FixtureIdentity(a, b)));
    let router = app(state, runtime.clone());

    for (token, tenant, title) in [
        ("fixture-a", a, "tenant a note"),
        ("fixture-b", b, "tenant b note"),
    ] {
        let (status, body) = export(&router, Some(token), json!({"entity_types": ["note"]})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["manifest"]["memory"]["tenant_id"], tenant.to_string());
        assert_eq!(body["manifest"]["memory"]["schema"], "public");
        let records = body["records"].as_array().unwrap();
        assert_eq!(records.len(), 1, "{body}");
        assert_eq!(records[0]["record"]["title"], title);

        let mark = body["manifest"]["high_water_mark"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, increment) = export(
            &router,
            Some(token),
            json!({"mode": "incremental", "since": mark}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{increment}");
        assert_eq!(increment["manifest"]["window"]["from"], mark);
    }

    for (token, body, expected) in [
        (None, json!({}), StatusCode::UNAUTHORIZED),
        (Some("fixture-denied"), json!({}), StatusCode::FORBIDDEN),
        (
            Some("fixture-a"),
            json!({"mode": "incremental"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some("fixture-a"),
            json!({"since": "x"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let (status, response) = export(&router, token, body).await;
        assert_eq!(status, expected, "{response}");
    }

    runtime.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
}
