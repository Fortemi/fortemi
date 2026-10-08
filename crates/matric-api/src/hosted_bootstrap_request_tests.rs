//! A tenant provisioned only by `admin bootstrap` serves hosted note creation.

use super::*;

/// Uses the same opt-in disposable database as the creation regression test.
#[tokio::test]
async fn hosted_creation_succeeds_for_bootstrapped_tenant_without_manual_sql() {
    use tower::ServiceExt;
    let Ok(url) = std::env::var("FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL") else {
        eprintln!("SKIP: FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL not supplied");
        return;
    };
    let admin = matric_db::create_pool(&url)
        .await
        .expect("explicit test database");
    let role = format!("hb_{}", Uuid::new_v4().simple());
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

    let slug = format!("bootstrap-request-{}", Uuid::new_v4().simple());
    let request =
        matric_api::admin_bootstrap::BootstrapRequest::new(&slug, None, None, false).unwrap();
    matric_api::admin_bootstrap::bootstrap_tenant(&admin, &request, false, "fortemi:tenant_id")
        .await
        .unwrap();
    let tenant = request.tenant_id;

    let mut state = super::super::tests::build_call_api_test_state(
        Database::new(runtime.clone()),
        "disposable-hosted-bootstrap-test",
    )
    .await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.audit_sink = Arc::new(PostgresAuditSink::new(runtime.clone()));
    let app = hosted_router(state.clone(), runtime.clone(), tenant);

    // Tags require the tenant's default SKOS scheme, which bootstrap seeded.
    let response = app
        .oneshot(http_request(
            Method::POST,
            "/api/v1/notes",
            serde_json::json!({"content":"First bootstrapped note", "tags":["onboarding"], "pipeline":[]}),
            false,
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(count_notes(&admin, tenant).await, 1);

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
