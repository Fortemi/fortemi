//! Pass-4 audit regressions: SQL exports, queue identity, and admission ordering.
use super::*;
#[path = "archive_sync_order_tests.rs"]
mod sync_order;

struct SchemaPolicy(String);
#[async_trait::async_trait]
impl AuthorizationPolicy for SchemaPolicy {
    async fn authorize(
        &self,
        p: &AuthPrincipal,
        a: &matric_core::Action,
        r: &Resource,
        c: &AuthzContext,
    ) -> Result<Decision, AuthzError> {
        AllowAllPolicy.authorize(p, a, r, c).await
    }
    async fn may_select_archive(
        &self,
        _: &AuthPrincipal,
        r: &Resource,
        _: &AuthzContext,
    ) -> Result<bool, AuthzError> {
        Ok(r.attrs["archive_schema"] == self.0)
    }
    fn policy_id(&self) -> &'static str {
        "custom_schema_grants"
    }
    fn policy_version(&self) -> &'static str {
        "1201-r5"
    }
}

#[sqlx::test(migrations = false)]
async fn memory_sql_backup_requires_complete_catalog_for_shared_schemas(admin: sqlx::PgPool) {
    let (mut state, _, _, token, a, _) = restricted(&admin).await;
    // This custom policy admits ordinary operations and public, while denying B.
    // It deliberately has a different policy_id from role_based.
    state.authorization_policy = Arc::new(SchemaPolicy("public".into()));
    let app = crate::production_router(state.clone(), None);
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        "/api/v1/backup/memory/public",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let caller = archive_access::Caller {
        principal: AuthPrincipal::Anonymous,
        tenant_id: None,
        user_id: None,
    };
    let result = memory_backup_download(State(state.clone()), caller, Path("public".into())).await;
    assert!(
        matches!(result, Err(ApiError::Forbidden(_))),
        "handler must refuse before pg_dump credential lookup/spawn"
    );
    assert!(!state
        .db
        .archives
        .schema_contains_shared_tables(&a)
        .await
        .unwrap());
    state.authorization_policy = Arc::new(SchemaPolicy(a.clone()));
    let caller = archive_access::Caller {
        principal: AuthPrincipal::Anonymous,
        tenant_id: None,
        user_id: None,
    };
    let scoped =
        memory_backup_download(State(state.clone()), caller, Path("archive_a".into())).await;
    assert!(
        matches!(
            scoped,
            Ok(_)
                | Err(ApiError::OperationFailed {
                    operation: "Memory backup",
                    ..
                })
        ),
        "ordinary non-public export reaches pg_dump setup with archive-only authority"
    );
    // Even a non-public schema containing shared control data must be gated.
    sqlx::query(&format!("CREATE TABLE {a}.job_queue (private_data text)"))
        .execute(&admin)
        .await
        .unwrap();
    state.authorization_policy = Arc::new(SchemaPolicy(a.clone()));
    assert!(state
        .db
        .archives
        .schema_contains_shared_tables(&a)
        .await
        .unwrap());
    let caller = archive_access::Caller {
        principal: AuthPrincipal::Anonymous,
        tenant_id: None,
        user_id: None,
    };
    assert!(matches!(
        memory_backup_download(State(state.clone()), caller, Path("archive_a".into())).await,
        Err(ApiError::Forbidden(_))
    ));
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn noteless_job_deduplication_is_symmetric_and_archive_scoped(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let submit = |schema: &str| serde_json::json!({"job_type":"graph_maintenance","payload":{"schema":schema},"deduplicate":true});
    let (status, first_b) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/jobs",
        submit(&b),
        Some("archive_b"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first_b}");
    let (status, first_a) = call_as(
        &app,
        &token,
        Method::POST,
        "/api/v1/jobs",
        submit(&a),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first_a}");
    assert_eq!(first_a["status"], "queued");
    assert_ne!(first_a["id"], first_b["id"]);
    for schema in [&a, &b] {
        let (status, body) = call_as(
            &original,
            &token,
            Method::POST,
            "/api/v1/jobs",
            submit(schema),
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "already_pending");
    }
    sqlx::query("DELETE FROM job_queue")
        .execute(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(
        call_as(
            &app,
            &token,
            Method::POST,
            "/api/v1/jobs",
            submit(&a),
            Some("archive_a")
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call_as(
            &original,
            &token,
            Method::POST,
            "/api/v1/jobs",
            submit(&b),
            Some("archive_b")
        )
        .await
        .0,
        StatusCode::CREATED,
        "A must not suppress B's work"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM job_queue")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn note_patch_does_not_deduplicate_against_another_archive(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let (status, note) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"before", "pipeline":[]}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{note}");
    let id: Uuid = note["id"].as_str().unwrap().parse().unwrap();
    let kind = JobType::TitleGeneration;
    state
        .db
        .jobs
        .queue_deduplicated_with_initiator(
            Some(id),
            kind,
            1,
            Some(serde_json::json!({"schema":b})),
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();
    let (status, body) = call_as(
        &app,
        &token,
        Method::PATCH,
        &format!("/api/v1/notes/{id}"),
        serde_json::json!({"content":"after", "revision_mode":"none"}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let schemas: Vec<String> = sqlx::query_scalar("SELECT payload->>'schema' FROM job_queue WHERE note_id=$1 AND job_type='title_generation' ORDER BY payload->>'schema'")
        .bind(id).fetch_all(&state.db.pool).await.unwrap();
    let mut expected = vec![a.clone(), b];
    expected.sort();
    assert_eq!(schemas, expected);
    assert!(state
        .db
        .jobs
        .queue_deduplicated_with_initiator(
            Some(id),
            kind,
            1,
            Some(serde_json::json!({"schema":a})),
            None,
            None
        )
        .await
        .unwrap()
        .is_none());
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn default_federated_order_and_overview_public_totals(admin: sqlx::PgPool) {
    let (state, _, original, token, _, _) = restricted(&admin).await;
    // Default policy: public first, remaining archives in main's catalog order.
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/search/federated",
        serde_json::json!({"q":"r5_no_matches", "memories":["all"]}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["memories_searched"],
        serde_json::json!(["public", "archive_b", "archive_a"])
    );
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/notes",
        serde_json::json!({"content":"one public note", "pipeline":[]}),
        Some("public"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, overview) = call_as(
        &original,
        &token,
        Method::GET,
        "/api/v1/memories/overview",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert_eq!(
        overview["total_notes"], 1,
        "registered public is counted exactly once"
    );
    let memories = overview["memories"].as_array().unwrap();
    assert_eq!(memories.iter().filter(|m| m["name"] == "public").count(), 1);
    assert_eq!(
        memories
            .iter()
            .map(|m| m["note_count"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
    assert_eq!(
        overview["total_size_bytes"].as_i64().unwrap(),
        memories
            .iter()
            .map(|m| m["size_bytes"].as_i64().unwrap())
            .sum::<i64>()
    );
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn unclassified_static_route_cannot_inherit_dynamic_route_isolation(admin: sqlx::PgPool) {
    let (mut state, _, _, _, a, _) = restricted(&admin).await;
    state.authorization_policy = Arc::new(SchemaPolicy(a));
    let app = Router::new()
        .route("/api/v1/notes/{id}", get(|| async { StatusCode::OK }))
        .route("/api/v1/notes/private", get(|| async { StatusCode::OK }))
        .fallback(|| async { StatusCode::OK })
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            archive_isolation_middleware,
        ));
    for (method, path, expected) in [
        (Method::GET, "/api/v1/notes/known-id", StatusCode::OK),
        (Method::GET, "/api/v1/notes/private", StatusCode::FORBIDDEN),
        (
            Method::POST,
            "/api/v1/notes/known-id",
            StatusCode::FORBIDDEN,
        ),
        (Method::GET, "/unmatched", StatusCode::FORBIDDEN),
    ] {
        let response = app
            .clone()
            .oneshot(oidc_request(method, path, serde_json::Value::Null, None))
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{path}");
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn production_matched_templates_and_methods_have_exact_inventory_entries(
    admin: sqlx::PgPool,
) {
    let state = setup(&admin).await;
    let app = crate::production_router(state.clone(), None).layer(axum::middleware::from_fn(
        |req: axum::http::Request<Body>, _: axum::middleware::Next| async move {
            let template = req
                .extensions()
                .get::<axum::extract::MatchedPath>()
                .map(|p| p.as_str())
                .unwrap_or("");
            (
                [("x-test-matched-path", template.to_owned())],
                StatusCode::NO_CONTENT,
            )
        },
    ));
    let mut seen = std::collections::BTreeSet::new();
    for (template, method) in route_policy::tests::all_registered_operations() {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        let path = route_policy::tests::materialize_route_template(template);
        let response = app
            .clone()
            .oneshot(oidc_request(
                method.clone(),
                &path,
                serde_json::Value::Null,
                None,
            ))
            .await
            .unwrap();
        let matched = response
            .headers()
            .get("x-test-matched-path")
            .unwrap()
            .to_str()
            .unwrap();
        if matched.is_empty() {
            assert!(
                matches!(
                    template,
                    "/recording.wav"
                        | "/api/v1/problem-contract-test"
                        | "/api/v1/problem-contract-status/{status}"
                ),
                "production template missing: {method} {template}"
            );
        } else {
            assert_eq!(
                matched, template,
                "Axum selected a different template for {path}"
            );
            assert!(
                route_policy::route_policy_for_matched_path(&method, matched).is_some(),
                "unclassified production route: {method} {matched}"
            );
            seen.insert(matched.to_owned());
        }
    }
    let expected: std::collections::BTreeSet<_> = route_policy::ROUTE_POLICY_INVENTORY
        .iter()
        .map(|route| route.path)
        .filter(|path| {
            !matches!(
                *path,
                "/recording.wav"
                    | "/api/v1/problem-contract-test"
                    | "/api/v1/problem-contract-status/{status}"
            )
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(seen, expected);
    state.db.pool.close().await;
}
