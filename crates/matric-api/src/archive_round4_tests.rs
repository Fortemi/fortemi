//! Structural admission regressions for pass-3 findings #2, #9, #12–14.
use super::*;
use base64::Engine;

struct BackupDest(Option<std::ffi::OsString>);
impl Drop for BackupDest {
    fn drop(&mut self) {
        match &self.0 {
            Some(value) => std::env::set_var("BACKUP_DEST", value),
            None => std::env::remove_var("BACKUP_DEST"),
        }
    }
}

#[sqlx::test(migrations = false)]
async fn complete_catalog_gate_precedes_handlers_and_allows_default_policy(admin: sqlx::PgPool) {
    let (state, app, original, token, _, _) = restricted(&admin).await;
    // Streaming health had no handler-level archive check. This tests the
    // central gate's positive and negative decisions through the real router.
    for (router, expected) in [(&app, StatusCode::FORBIDDEN), (&original, StatusCode::OK)] {
        let (status, body) = call_as(
            router,
            &token,
            Method::GET,
            "/api/v1/health/streaming",
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, expected, "{body}");
    }
    // Malformed UUID and JSON must never reach handler extractors.
    for (method, path) in [
        (Method::GET, "/api/v1/calls/not-a-uuid"),
        (Method::POST, "/api/v1/embedding-sets"),
    ] {
        let (status, body) = call_as(
            &app,
            &token,
            method,
            path,
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    // Also fail closed if a router is extended without inventory metadata or
    // authentication exemptions omit RoutePolicyInput. This sentinel has no
    // handler-level authorization and must never execute for the A-only caller.
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    for (full_catalog, method, path) in [
        (false, Method::GET, "/__unclassified_isolation_probe"),
        (true, Method::GET, "/__unclassified_isolation_probe"),
        (false, Method::OPTIONS, "/api/v1/embedding-sets"),
        (true, Method::OPTIONS, "/api/v1/embedding-sets"),
    ] {
        ran.store(false, Ordering::Relaxed);
        let mut gate_state = state.clone();
        if full_catalog {
            gate_state.authorization_policy = Arc::new(AllowAllPolicy);
        }
        let hit = ran.clone();
        let method_filter = if method == Method::OPTIONS {
            axum::routing::MethodFilter::OPTIONS
        } else {
            axum::routing::MethodFilter::GET
        };
        let probe = Router::new()
            .route(
                path,
                axum::routing::on(method_filter, move || async move {
                    hit.store(true, Ordering::Relaxed);
                    StatusCode::OK
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                gate_state,
                authorize_middleware,
            ))
            .layer(Extension(Auth {
                principal: identity.principal.clone(),
            }))
            .layer(Extension(
                VerifiedRequestTenant::from_verified(identity.tenant_id.unwrap()).unwrap(),
            ))
            .layer(Extension(identity.clone()));
        let response = probe
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .method(method)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if full_catalog {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            }
        );
        assert_eq!(ran.load(Ordering::Relaxed), full_catalog);
    }
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn shared_knowledge_archive_files_cannot_be_read_or_overwritten(admin: sqlx::PgPool) {
    let (state, app, _, token, _, _) = restricted(&admin).await;
    let mut full = state.clone();
    full.authorization_policy = Arc::new(AllowAllPolicy);
    let original = crate::production_router(full, None);
    let dir = tempfile::tempdir().unwrap();
    let _restore = BackupDest(std::env::var_os("BACKUP_DEST"));
    std::env::set_var("BACKUP_DEST", dir.path());
    let filename = "snapshot_denied.sql.gz";
    let file = dir.path().join(filename);
    std::fs::write(&file, b"denied backup bytes").unwrap();
    for name in [filename, "snapshot_missing.sql.gz"] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            &format!("/api/v1/backup/knowledge-archive/{name}"),
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
    // A full-catalog download is still compatible and contains the seeded bytes.
    let response = original
        .oneshot(request(
            &token,
            Method::GET,
            &format!("/api/v1/backup/knowledge-archive/{filename}"),
            serde_json::Value::Null,
            Some("archive_a"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mut tar = tar::Archive::new(std::io::Cursor::new(bytes));
    assert!(tar.entries().unwrap().any(|entry| {
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry.unwrap(), &mut data).unwrap();
        data == b"denied backup bytes"
    }));
    let mut tar = tar::Builder::new(Vec::new());
    let replacement = b"replacement backup bytes";
    let mut header = tar::Header::new_gnu();
    header.set_size(replacement.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    tar.append_data(&mut header, filename, replacement.as_slice())
        .unwrap();
    let tar = tar.into_inner().unwrap();
    let mut multipart = b"--r4\r\nContent-Disposition: form-data; name=\"file\"; filename=\"backup.archive\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    multipart.extend(tar);
    multipart.extend(b"\r\n--r4--\r\n");
    let mut req = request(
        &token,
        Method::POST,
        "/api/v1/backup/knowledge-archive",
        serde_json::Value::Null,
        Some("archive_a"),
    );
    req.headers_mut().insert(
        header::CONTENT_TYPE,
        "multipart/form-data; boundary=r4".parse().unwrap(),
    );
    *req.body_mut() = Body::from(multipart);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(std::fs::read(file).unwrap(), b"denied backup bytes");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn shared_config_binding_and_skip_merge_import_cannot_create_authority(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let config = Uuid::new_v4();
    let ctx = state.db.for_schema(&b).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query("INSERT INTO public.embedding_config(id,name,model,dimension,provider_config) VALUES($1,$2,'b-private',768,$3)")
        .bind(config).bind(format!("private-{config}"))
        .bind(serde_json::json!({"private_config":"B-secret"}))
        .execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let response = original
        .oneshot(request(
            &token,
            Method::GET,
            "/api/v1/backup/knowledge-shard?profile=full-v1",
            serde_json::Value::Null,
            Some("archive_b"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let files = read_shard_archive(&bytes, ShardArchiveLimits::default()).unwrap();
    assert!(String::from_utf8_lossy(&files["embedding_configs.json"]).contains("B-secret"));
    let (status, body) = call_as(&app, &token, Method::POST, "/api/v1/embedding-sets",
        serde_json::json!({"name":"stolen binding","slug":"stolen-binding","set_type":"full","embedding_config_id":config}),
        Some("archive_a")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    for strategy in ["skip", "merge", "replace"] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::POST,
            "/api/v1/backup/knowledge-shard/import",
            serde_json::json!({
                "shard_base64":base64::engine::general_purpose::STANDARD.encode(&bytes),
                "on_conflict":strategy, "skip_embedding_regen":true
            }),
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{strategy}: {body}");
    }
    let mut req = request(
        &token,
        Method::POST,
        "/api/v1/backup/knowledge-shard/upload?on_conflict=merge",
        serde_json::Value::Null,
        Some("archive_a"),
    );
    *req.body_mut() = Body::from(bytes);
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let references: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {a}.embedding_set WHERE embedding_config_id=$1"
    ))
    .bind(config)
    .fetch_one(&state.db.pool)
    .await
    .unwrap();
    assert_eq!(
        references, 0,
        "caller-created references cannot authorize shared configs"
    );
    let declarations: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {a}.shard_embedding_config_declaration WHERE config_id=$1"
    ))
    .bind(config)
    .fetch_one(&state.db.pool)
    .await
    .unwrap();
    assert_eq!(
        declarations, 0,
        "skip/merge cannot establish export authority either"
    );
    let shared_config: serde_json::Value =
        sqlx::query_scalar("SELECT provider_config FROM public.embedding_config WHERE id=$1")
            .bind(config)
            .fetch_one(&state.db.pool)
            .await
            .unwrap();
    assert_eq!(
        shared_config,
        serde_json::json!({"private_config":"B-secret"})
    );
    assert_ne!(a, b);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn shared_call_transcripts_require_complete_catalog(admin: sqlx::PgPool) {
    let (state, app, _, token, _, _) = restricted(&admin).await;
    let mut full = state.clone();
    full.authorization_policy = Arc::new(AllowAllPolicy);
    let original = crate::production_router(full, None);
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.call_sessions(call_id,provider,provider_call_id,started_at) VALUES($1,'twilio',$2,now())")
        .bind(id).bind(format!("r4-{id}")).execute(&state.db.pool).await.unwrap();
    sqlx::query("INSERT INTO public.transcript_segments(id,call_id,start_ts,end_ts,text,sequence) VALUES($1,$2,0,1,'denied transcript',0)")
        .bind(Uuid::new_v4()).bind(id).execute(&state.db.pool).await.unwrap();
    for call in [id, Uuid::new_v4()] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            &format!("/api/v1/calls/{call}"),
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(!body.to_string().contains("denied transcript"));
    }
    let (status, body) = call_as(
        &original,
        &token,
        Method::GET,
        &format!("/api/v1/calls/{id}"),
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.to_string().contains("denied transcript"));
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn shared_defaults_receivers_connectors_and_document_types_are_gated(admin: sqlx::PgPool) {
    let (state, app, original, token, _, _) = restricted(&admin).await;
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/archives/archive_b/set-default",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    for path in [
        "/api/v1/archives/archive_a/set-default",
        "/api/v1/memories/archive_a/set-default",
        "/api/v1/webhooks/incoming",
        "/api/v1/inbound-sources",
        "/api/v1/document-types",
    ] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::POST,
            path,
            serde_json::json!({}),
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    for (method, path) in [
        (Method::PATCH, "/api/v1/webhooks/incoming/receiver"),
        (Method::DELETE, "/api/v1/inbound-sources/source"),
        (Method::PATCH, "/api/v1/document-types/markdown"),
    ] {
        let (status, body) = call_as(
            &app,
            &token,
            method,
            path,
            serde_json::json!({}),
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    assert_eq!(
        state
            .db
            .archives
            .get_default_archive()
            .await
            .unwrap()
            .unwrap()
            .name,
        "archive_b"
    );
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn restricted_health_is_coarse_and_detailed_health_is_gated(admin: sqlx::PgPool) {
    let (state, app, original, token, _, _) = restricted(&admin).await;
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        "/health",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body.as_object().unwrap().len(),
        3,
        "no cross-archive diagnostics"
    );
    assert_eq!(body["status"], "healthy");
    for path in ["/api/v1/health/streaming", "/api/v1/health/knowledge"] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            path,
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    let (status, body) = call_as(
        &original,
        &token,
        Method::GET,
        "/health",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["sse"].is_object());
    assert!(body["capabilities"]["attachment_scanning"]["queue_depth"].is_number());
    state.db.pool.close().await;
}
