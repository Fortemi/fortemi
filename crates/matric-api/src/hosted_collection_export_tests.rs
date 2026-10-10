//! Collections and collection export on the single-tenant dedicated hosted profile (#1154).

use super::*;

/// Mirrors `auth_middleware`: hosted bearer routes outside the qualified set fail with 503.
async fn hosted_route_gate(
    request: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    if !route_policy::hosted_tenant_transaction_ready(request.method(), request.uri().path()) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    next.run(request).await
}

fn collection_router(state: AppState, pool: sqlx::PgPool, tenant: Uuid) -> Router {
    Router::new()
        .route("/api/v1/notes", post(create_note))
        .route("/api/v1/notes/{id}/move", post(move_note_to_collection))
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/api/v1/collections/{id}",
            get(get_collection)
                .patch(update_collection)
                .delete(delete_collection),
        )
        .route("/api/v1/collections/{id}/notes", get(get_collection_notes))
        .route("/api/v1/collections/{id}/export", get(export_collection))
        .route("/api/v1/graph/{id}", get(|| async { StatusCode::OK }))
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
        .layer(axum::middleware::from_fn(hosted_route_gate))
        .layer(axum::middleware::from_fn(inject_identity))
        .layer(Extension(Identity {
            tenant_id: tenant,
            user_id: hosted_test_user_id(tenant),
        }))
        .layer(Extension(ArchiveContext::default()))
        .with_state(state)
}

async fn send(app: &Router, method: Method, path: &str, body: serde_json::Value) -> Response {
    use tower::ServiceExt;
    app.clone()
        .oneshot(http_request(method, path, body, false))
        .await
        .unwrap()
}

async fn text_body(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Uses the same opt-in disposable database as the creation regression test.
#[tokio::test]
async fn hosted_collections_and_export_are_tenant_bound_for_bootstrapped_tenant() {
    let Ok(url) = std::env::var("FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL") else {
        eprintln!("SKIP: FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL not supplied");
        return;
    };
    let admin = matric_db::create_pool(&url)
        .await
        .expect("explicit test database");
    let role = format!("hx_{}", Uuid::new_v4().simple());
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

    // The dedicated profile provisions its tenant with `admin bootstrap`; a
    // second bootstrapped tenant proves the routes stay tenant-bound.
    let mut tenants = Vec::new();
    for label in ["owner", "other"] {
        let slug = format!("export-{label}-{}", Uuid::new_v4().simple());
        let request =
            matric_api::admin_bootstrap::BootstrapRequest::new(&slug, None, None, false).unwrap();
        matric_api::admin_bootstrap::bootstrap_tenant(&admin, &request, false, "fortemi:tenant_id")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_user (
                id, tenant_id, iss, sub, email, email_verified, display_name, groups,
                current_scopes, kind, azp, status
             ) VALUES ($1, $2, 'https://issuer.example', 'hosted-create-regression',
                       'hosted-create@example.com', true, 'Hosted Create', '{}',
                       ARRAY['read','write','mcp'], 'user', 'fortemi-web', 'active')",
        )
        .bind(hosted_test_user_id(request.tenant_id))
        .bind(request.tenant_id)
        .execute(&admin)
        .await
        .unwrap();
        tenants.push(request.tenant_id);
    }
    let (a, b) = (tenants[0], tenants[1]);

    let mut state = super::super::tests::build_call_api_test_state(
        Database::new(runtime.clone()),
        "disposable-hosted-collection-export-test",
    )
    .await;
    state.require_auth = true;
    state.multi_tenant = true;
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    state.audit_sink = Arc::new(PostgresAuditSink::new(runtime.clone()));
    let app_a = collection_router(state.clone(), runtime.clone(), a);
    let app_b = collection_router(state.clone(), runtime.clone(), b);

    let created = send(
        &app_a,
        Method::POST,
        "/api/v1/collections",
        // Collection names are unique deployment-wide today, so each run uses its own.
        serde_json::json!({"name": format!("Graph research {a}"), "description": "dedicated"}),
    )
    .await;
    let status = created.status();
    let body = json_body(created).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let collection = body["id"].as_str().unwrap().to_string();
    let collection_path = format!("/api/v1/collections/{collection}");

    // Membership is set through tenant-transaction note creation; the legacy
    // note move route is not qualified on this profile.
    let note = send(
        &app_a,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({
            "content": "Tenant A export marker",
            "title": "Export",
            "collection_id": collection,
            "pipeline": []
        }),
    )
    .await;
    let status = note.status();
    let body = json_body(note).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let note_id = body["id"].as_str().unwrap().to_string();

    let moved = send(
        &app_a,
        Method::POST,
        &format!("/api/v1/notes/{note_id}/move"),
        serde_json::json!({"collection_id": null}),
    )
    .await;
    assert_eq!(moved.status(), StatusCode::SERVICE_UNAVAILABLE);
    let renamed = send(
        &app_a,
        Method::PATCH,
        &collection_path,
        serde_json::json!({"name": format!("Graph research renamed {a}")}),
    )
    .await;
    assert_eq!(renamed.status(), StatusCode::NO_CONTENT);

    let listed = json_body(
        send(
            &app_a,
            Method::GET,
            "/api/v1/collections",
            serde_json::Value::Null,
        )
        .await,
    )
    .await;
    assert!(listed.to_string().contains(&collection), "{listed}");
    let detail = send(
        &app_a,
        Method::GET,
        &collection_path,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    assert!(json_body(detail)
        .await
        .to_string()
        .contains(&format!("Graph research renamed {a}")));
    let members = json_body(
        send(
            &app_a,
            Method::GET,
            &format!("{collection_path}/notes"),
            serde_json::Value::Null,
        )
        .await,
    )
    .await;
    assert!(members.to_string().contains(&note_id), "{members}");

    let export = send(
        &app_a,
        Method::GET,
        &format!("{collection_path}/export"),
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(export.status(), StatusCode::OK);
    assert!(export.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/markdown"));
    let markdown = text_body(export).await;
    assert!(markdown.contains("Tenant A export marker"), "{markdown}");
    assert!(markdown.contains(&format!("id: {note_id}")), "{markdown}");

    // Another tenant can neither see nor export the collection.
    let foreign_list = json_body(
        send(
            &app_b,
            Method::GET,
            "/api/v1/collections",
            serde_json::Value::Null,
        )
        .await,
    )
    .await;
    assert!(
        !foreign_list.to_string().contains(&collection),
        "{foreign_list}"
    );
    for suffix in ["", "/notes", "/export"] {
        let denied = send(
            &app_b,
            Method::GET,
            &format!("{collection_path}{suffix}"),
            serde_json::Value::Null,
        )
        .await;
        assert!(
            denied.status().is_client_error(),
            "cross-tenant {suffix} returned {}",
            denied.status()
        );
        assert!(!text_body(denied).await.contains("Tenant A export marker"));
    }

    // Unqualified hosted routes stay closed on this profile.
    let graph = send(
        &app_a,
        Method::GET,
        &format!("/api/v1/graph/{note_id}"),
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(graph.status(), StatusCode::SERVICE_UNAVAILABLE);

    let non_empty = send(
        &app_a,
        Method::DELETE,
        &collection_path,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(non_empty.status(), StatusCode::CONFLICT);
    let deleted = send(
        &app_a,
        Method::DELETE,
        &format!("{collection_path}?force=true"),
        serde_json::Value::Null,
    )
    .await;
    assert!(
        deleted.status().is_success(),
        "delete returned {}",
        deleted.status()
    );
    let gone = send(
        &app_a,
        Method::GET,
        &collection_path,
        serde_json::Value::Null,
    )
    .await;
    // A deleted collection is indistinguishable from a foreign one.
    assert_eq!(gone.status(), StatusCode::FORBIDDEN);

    drop(app_a);
    drop(app_b);
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
