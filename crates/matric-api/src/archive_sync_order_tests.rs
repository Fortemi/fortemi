//! Observe both successful and failed schema-sync attempts at routing admission.
use super::*;

async fn response_parts(
    app: &Router,
    token: &str,
    method: Method,
    path: &str,
    memory: Option<&str>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        app.clone().oneshot(request(
            token,
            method,
            path,
            serde_json::Value::Null,
            memory,
        )),
    )
    .await
    .expect("bounded production request")
    .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, headers, body.to_vec())
}

#[sqlx::test(migrations = false)]
async fn external_job_action_refusal_is_independent_of_existence(admin: sqlx::PgPool) {
    let state = setup(&admin).await;
    let issuer = external_oidc_test_issuer::issuer();
    let owner = issuer.token(serde_json::json!({"groups":["admin"]}));
    let app = crate::production_router(state.clone(), None);
    create(&app, &owner, "admission").await;
    state
        .db
        .archives
        .set_default_archive("admission")
        .await
        .unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='admission'")
        .execute(&state.db.pool)
        .await
        .unwrap();
    state.default_archive_cache.write().await.invalidate();
    let id = state
        .db
        .jobs
        .queue(None, JobType::GraphMaintenance, 1, None, None)
        .await
        .unwrap();
    let existing = format!("/api/v1/jobs/{id}");
    let missing = format!("/api/v1/jobs/{}", Uuid::new_v4());
    let counter = Arc::new(AtomicUsize::new(0));
    let app = crate::production_router(state.clone(), None).layer(Extension(
        middleware::archive_routing::SchemaSyncCounter(counter.clone()),
    ));
    for group in ["reader", "writer"] {
        let token = issuer.token(serde_json::json!({"groups":[group]}));
        for method in [Method::GET, Method::HEAD] {
            for memory in [Some("admission"), None] {
                let present = response_parts(&app, &token, method.clone(), &existing, memory).await;
                let absent = response_parts(&app, &token, method.clone(), &missing, memory).await;
                assert_eq!(present.0, StatusCode::FORBIDDEN, "{group} {method}");
                assert_eq!(
                    present, absent,
                    "status, all headers and body: {group} {method}"
                );
                assert_eq!(
                    present.1[header::WWW_AUTHENTICATE],
                    insufficient_scope_www_authenticate_header(&state.issuer, &["admin".into()])
                );
                if method == Method::HEAD {
                    assert!(present.2.is_empty());
                } else {
                    let body: serde_json::Value = serde_json::from_slice(&present.2).unwrap();
                    assert_eq!(body["detail"], "Authorization denied.");
                }
                assert_eq!(counter.load(Ordering::Relaxed), 0);
                let version: i32 = sqlx::query_scalar(
                    "SELECT schema_version FROM archive_registry WHERE name='admission'",
                )
                .fetch_one(&state.db.pool)
                .await
                .unwrap();
                assert_eq!(version, 0);
                assert!(state.default_archive_cache.read().await.archive.is_none());
            }
        }
    }
    for method in [Method::GET, Method::HEAD] {
        for (path, expected) in [
            (&existing, StatusCode::OK),
            (&missing, StatusCode::NOT_FOUND),
        ] {
            let (status, headers, body) =
                response_parts(&app, &owner, method.clone(), path, None).await;
            assert_eq!(status, expected, "admin {method} {path}");
            assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
            if method == Method::HEAD {
                assert!(body.is_empty());
            } else {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                if expected == StatusCode::OK {
                    assert_eq!(body["id"], id.to_string());
                } else {
                    assert_eq!(body["detail"], "Job not found");
                }
            }
        }
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn external_named_archive_action_refusal_precedes_target_resolution(admin: sqlx::PgPool) {
    let state = setup(&admin).await;
    let issuer = external_oidc_test_issuer::issuer();
    let owner = issuer.token(serde_json::json!({"groups":["admin"]}));
    let app = crate::production_router(state.clone(), None);
    create(&app, &owner, "admission").await;
    for group in ["reader", "writer"] {
        let token = issuer.token(serde_json::json!({"groups":[group]}));
        for (method, prefix, suffix) in [
            (Method::GET, "backup/memory", ""),
            (Method::HEAD, "backup/memory", ""),
            (Method::PATCH, "archives", ""),
            (Method::DELETE, "archives", ""),
            (Method::POST, "archives", "/clone"),
            (Method::POST, "archives", "/set-default"),
            (Method::PATCH, "memories", ""),
            (Method::DELETE, "memories", ""),
            (Method::POST, "memories", "/clone"),
            (Method::POST, "memories", "/set-default"),
        ] {
            let existing = format!("/api/v1/{prefix}/admission{suffix}");
            let missing = format!("/api/v1/{prefix}/missing{suffix}");
            let present = response_parts(&app, &token, method.clone(), &existing, None).await;
            let absent = response_parts(&app, &token, method.clone(), &missing, None).await;
            assert_eq!(
                present.0,
                StatusCode::FORBIDDEN,
                "{group} {method} {existing}"
            );
            assert_eq!(
                present, absent,
                "status, all headers and body: {group} {method} {prefix}"
            );
            assert_eq!(
                present.1[header::WWW_AUTHENTICATE],
                insufficient_scope_www_authenticate_header(&state.issuer, &["admin".into()])
            );
        }
    }
    for path in ["/api/v1/archives/admission", "/api/v1/memories/admission"] {
        assert_eq!(
            response_parts(&app, &owner, Method::GET, path, None)
                .await
                .0,
            StatusCode::OK
        );
    }
    for path in [
        "/api/v1/archives/missing",
        "/api/v1/memories/missing",
        "/api/v1/backup/memory/missing",
    ] {
        assert_eq!(
            response_parts(&app, &owner, Method::GET, path, None)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn community_complete_catalog_refusal_precedes_explicit_and_default_schema_sync(
    admin: sqlx::PgPool,
) {
    let (mut state, _, _, _, a, _) = restricted(&admin).await;
    state.require_auth = false;
    state.auth_mode = external_oidc::AuthMode::Local;
    state.external_oidc = None;
    state.authorization_policy = Arc::new(SchemaPolicy(a));
    state
        .db
        .archives
        .set_default_archive("archive_a")
        .await
        .unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='archive_a'")
        .execute(&state.db.pool)
        .await
        .unwrap();
    state.default_archive_cache.write().await.invalidate();
    let unscoped = state
        .db
        .jobs
        .queue(None, JobType::GraphMaintenance, 1, None, None)
        .await
        .unwrap();
    let unscoped_path = format!("/api/v1/jobs/{unscoped}");
    let id_text = unscoped.to_string();
    let encoded_unscoped_path = format!(
        "/api/v1/jobs/%{:02X}{}",
        id_text.as_bytes()[0],
        &id_text[1..]
    );
    let counter = Arc::new(AtomicUsize::new(0));
    let app = crate::production_router(state.clone(), None).layer(Extension(
        middleware::archive_routing::SchemaSyncCounter(counter.clone()),
    ));
    for memory in [Some("archive_a"), None] {
        for (method, path, body, expected) in [
            (
                Method::GET,
                "/api/v1/health/streaming",
                serde_json::Value::Null,
                StatusCode::FORBIDDEN,
            ),
            (
                Method::POST,
                "/api/v1/archives",
                serde_json::json!({"name":"new_archive"}),
                StatusCode::FORBIDDEN,
            ),
            (
                Method::POST,
                "/api/v1/memories",
                serde_json::json!({"name":"new_memory"}),
                StatusCode::FORBIDDEN,
            ),
            (
                Method::GET,
                unscoped_path.as_str(),
                serde_json::Value::Null,
                StatusCode::NOT_FOUND,
            ),
            (
                Method::HEAD,
                unscoped_path.as_str(),
                serde_json::Value::Null,
                StatusCode::NOT_FOUND,
            ),
            (
                Method::GET,
                encoded_unscoped_path.as_str(),
                serde_json::Value::Null,
                StatusCode::NOT_FOUND,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(oidc_request(method.clone(), path, body, memory))
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{method} {path}");
            assert_eq!(
                counter.load(Ordering::Relaxed),
                0,
                "refusal must perform no sync attempt, including one that would fail"
            );
            let version: i32 = sqlx::query_scalar(
                "SELECT schema_version FROM archive_registry WHERE name='archive_a'",
            )
            .fetch_one(&state.db.pool)
            .await
            .unwrap();
            assert_eq!(version, 0, "refusal cannot synchronize the archive");
            assert!(
                state.default_archive_cache.read().await.archive.is_none(),
                "refusal must precede even default-cache mutation"
            );
        }
    }
    // The same stale schema is observable when sync is actually called.
    let mut tx = admin.begin().await.unwrap();
    let tenant: Uuid = sqlx::query_scalar("SELECT current_setting('app.current_tenant')::uuid")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    matric_db::PgArchiveRepository::sync_archive_schema_on(&mut tx, "archive_a")
        .await
        .unwrap();
    let version: i32 =
        sqlx::query_scalar("SELECT schema_version FROM archive_registry WHERE name='archive_a'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(
        version > 0,
        "positive control proves schema sync is observable"
    );
    tx.rollback().await.unwrap();
    // Positive controls prove the hook observes both routing branches, including
    // best-effort sync failures under the restricted runtime role.
    for (index, memory) in [Some("archive_a"), None].into_iter().enumerate() {
        state.default_archive_cache.write().await.invalidate();
        let path = format!("/api/v1/notes/{}", Uuid::new_v4());
        let response = app
            .clone()
            .oneshot(oidc_request(
                Method::GET,
                &path,
                serde_json::Value::Null,
                memory,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(counter.load(Ordering::Relaxed), index + 1);
    }
    state.db.pool.close().await;
}

struct PublicAndArchive(String);
#[async_trait::async_trait]
impl AuthorizationPolicy for PublicAndArchive {
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
        Ok(r.attrs["archive_schema"] == "public" || r.attrs["archive_schema"] == self.0)
    }
    fn policy_id(&self) -> &'static str {
        "custom_public_and_archive"
    }
    fn policy_version(&self) -> &'static str {
        "1201-r6"
    }
}

#[sqlx::test(migrations = false)]
async fn community_shared_memory_backup_refusal_precedes_schema_sync(admin: sqlx::PgPool) {
    let (mut state, _, _, _, a, _) = restricted(&admin).await;
    state.require_auth = false;
    state.auth_mode = external_oidc::AuthMode::Local;
    state.external_oidc = None;
    // Both the dump target public and the routing target A are allowed; B is
    // denied. Refusal must therefore come from the complete-catalog decision.
    state.authorization_policy = Arc::new(PublicAndArchive(a.clone()));
    state
        .db
        .archives
        .set_default_archive("archive_a")
        .await
        .unwrap();
    sqlx::query("UPDATE archive_registry SET schema_version=0 WHERE name='archive_a'")
        .execute(&state.db.pool)
        .await
        .unwrap();
    state.default_archive_cache.write().await.invalidate();
    let counter = Arc::new(AtomicUsize::new(0));
    let app = crate::production_router(state.clone(), None).layer(Extension(
        middleware::archive_routing::SchemaSyncCounter(counter.clone()),
    ));
    assert!(!state
        .db
        .archives
        .schema_contains_shared_tables(&a)
        .await
        .unwrap());
    for target in ["public", "archive_a"] {
        // Test public against an ordinary routing schema first, then prove the
        // same gate also covers non-public schemas containing shared tables.
        if target == "archive_a" {
            sqlx::query(&format!("CREATE TABLE {a}.job_queue (private_data text)"))
                .execute(&admin)
                .await
                .unwrap();
        }
        for method in [Method::GET, Method::HEAD] {
            for memory in [Some("archive_a"), None] {
                let path = format!("/api/v1/backup/memory/{target}");
                let response = app
                    .clone()
                    .oneshot(oidc_request(
                        method.clone(),
                        &path,
                        serde_json::Value::Null,
                        memory,
                    ))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {path}");
                assert_eq!(
                    counter.load(Ordering::Relaxed),
                    0,
                    "refused dump cannot sync routing schema"
                );
                let version: i32 = sqlx::query_scalar(
                    "SELECT schema_version FROM archive_registry WHERE name='archive_a'",
                )
                .fetch_one(&state.db.pool)
                .await
                .unwrap();
                assert_eq!(version, 0);
                assert!(state.default_archive_cache.read().await.archive.is_none());
            }
        }
    }
    state.db.pool.close().await;
}
