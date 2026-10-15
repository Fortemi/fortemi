//! Round-three shared-state and compatibility regressions for #1201.
use super::*;

#[sqlx::test(migrations = false)]
async fn legacy_redis_ingest_record_keeps_schema_or_is_invalid(admin: sqlx::PgPool) {
    use redis::AsyncCommands;
    use sha2::{Digest, Sha256};
    let (mut state, _, _, _, a, b) = restricted(&admin).await;
    state.ingest_token_store = matric_api::services::IngestTokenStore::from_env().await;
    assert!(state.ingest_token_store.is_connected().await);
    let token = format!("mm_ist_{}", Uuid::new_v4().simple());
    let key = format!(
        "mm:ingesttoken:{}",
        hex::encode(Sha256::digest(token.as_bytes()))
    );
    let mut redis = redis::Client::open(std::env::var("REDIS_URL").unwrap())
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    // Exact old JSON shape: no identity, audience, scope or expiry fields.
    let old = serde_json::json!({"token_id":Uuid::new_v4().to_string(),"schema":a,"rate_limit":17});
    let _: () = redis.set_ex(&key, old.to_string(), 60).await.unwrap();
    state.authorization_policy = Arc::new(RoleBasedPolicy);
    let headers = HeaderMap::from_iter([
        (
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        ),
        (
            "X-Fortemi-Memory".parse().unwrap(),
            "archive_b".parse().unwrap(),
        ),
    ]);
    let (selected, rate) =
        handlers::ingest_stream::resolve_stream_token(&headers, &state, &ArchiveContext::default())
            .await
            .unwrap();
    assert_eq!(selected.schema, a);
    assert_eq!(rate, 17);
    let app = crate::production_router(state.clone(), None);
    let mut req = request(
        &token,
        Method::POST,
        "/api/v1/ingest/stream",
        serde_json::Value::Null,
        Some("archive_b"),
    );
    *req.body_mut() =
        Body::from("{\"type\":\"note\",\"data\":{\"content\":\"legacy token stays in A\"}}\n");
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _ = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    for (schema, count) in [(&a, 1_i64), (&b, 0)] {
        let actual: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {schema}.note"))
            .fetch_one(&state.db.pool)
            .await
            .unwrap();
        assert_eq!(actual, count);
    }
    struct DenyPublic;
    #[async_trait::async_trait]
    impl AuthorizationPolicy for DenyPublic {
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
            Ok(r.attrs["archive_schema"] != "public")
        }
        fn policy_id(&self) -> &'static str {
            "deny_public"
        }
        fn policy_version(&self) -> &'static str {
            "1"
        }
    }
    state.authorization_policy = Arc::new(DenyPublic);
    // This suite runs serially, like the existing environment-mutating API tests.
    struct TokenMode(Option<std::ffi::OsString>);
    impl Drop for TokenMode {
        fn drop(&mut self) {
            if let Some(value) = &self.0 {
                std::env::set_var("INGEST_REQUIRE_TOKEN", value);
            } else {
                std::env::remove_var("INGEST_REQUIRE_TOKEN");
            }
        }
    }
    let mode_guard = TokenMode(std::env::var_os("INGEST_REQUIRE_TOKEN"));
    for mode in ["true", "false"] {
        std::env::set_var("INGEST_REQUIRE_TOKEN", mode);
        let response = handlers::ingest_stream::resolve_stream_token(
            &headers,
            &state,
            &ArchiveContext::default(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "legacy token with {mode}"
        );
    }
    drop(mode_guard);
    let _: () = redis.del(key).await.unwrap();
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn shared_configuration_and_webhook_routes_require_complete_catalog(admin: sqlx::PgPool) {
    let (state, app, original, token, _, _) = restricted(&admin).await;
    let (status, created) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/webhooks",
        serde_json::json!({"url":"https://example.test/hook","events":[]}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let webhook = state.db.webhooks.get(id).await.unwrap().unwrap();
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    assert_eq!(webhook.created_by_user_id, sse::stable_user_id(&identity));
    let config: Uuid = sqlx::query_scalar("SELECT id FROM public.embedding_config LIMIT 1")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    // A as default selects global inference configuration rather than an override.
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/archives/archive_a/set-default",
        serde_json::Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    for (method, path, body) in [
        (
            Method::GET,
            "/api/v1/webhooks".into(),
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/webhooks".into(),
            serde_json::json!({"url":"https://example.test/new","events":[]}),
        ),
        (
            Method::GET,
            format!("/api/v1/webhooks/{id}"),
            serde_json::Value::Null,
        ),
        (
            Method::PATCH,
            format!("/api/v1/webhooks/{id}"),
            serde_json::json!({"url":"https://example.test/redirect"}),
        ),
        (
            Method::DELETE,
            format!("/api/v1/webhooks/{id}"),
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            format!("/api/v1/webhooks/{id}/test"),
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            format!("/api/v1/webhooks/{id}/deliveries"),
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            "/api/v1/embedding-configs".into(),
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            "/api/v1/embedding-configs/default".into(),
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            format!("/api/v1/embedding-configs/{config}"),
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/embedding-configs".into(),
            serde_json::json!({"name":"blocked-config","model":"blocked","dimension":768}),
        ),
        (
            Method::PATCH,
            format!("/api/v1/embedding-configs/{config}"),
            serde_json::json!({"description":"blocked"}),
        ),
        (
            Method::DELETE,
            format!("/api/v1/embedding-configs/{config}"),
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/inference/config".into(),
            serde_json::json!({}),
        ),
        (
            Method::DELETE,
            "/api/v1/inference/config".into(),
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            "/api/v1/inference/config/audit".into(),
            serde_json::Value::Null,
        ),
    ] {
        let (status, result) = call_as(&app, &token, method, &path, body, Some("archive_a")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {result}");
    }
    assert_eq!(
        state.db.webhooks.get(id).await.unwrap().unwrap().url,
        webhook.url
    );
    state.db.pool.close().await;
}

struct RevocableCatalog {
    user: Uuid,
    revoked: Arc<AtomicBool>,
    checked: Arc<AtomicUsize>,
}
#[async_trait::async_trait]
impl AuthorizationPolicy for RevocableCatalog {
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
        principal: &AuthPrincipal,
        archive: &Resource,
        context: &AuthzContext,
    ) -> Result<bool, AuthzError> {
        assert!(
            principal.is_authenticated(),
            "dispatch must use its creating user"
        );
        assert_eq!(context.environment["app_user_id"], self.user.to_string());
        self.checked.fetch_add(1, Ordering::Relaxed);
        Ok(!self.revoked.load(Ordering::Relaxed) || archive.id.as_deref() != Some("archive_b"))
    }
    fn policy_id(&self) -> &'static str {
        "revocable_catalog"
    }
    fn policy_version(&self) -> &'static str {
        "1"
    }
}

#[sqlx::test(migrations = false)]
async fn outgoing_dispatch_rechecks_creator_and_preserves_legacy_default(admin: sqlx::PgPool) {
    let (state, _, original, token, _, _) = restricted(&admin).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sink = Router::new().route(
        "/",
        axum::routing::post(move |Json(body): Json<serde_json::Value>| async move {
            tx.send(body).unwrap();
            StatusCode::OK
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, sink).await.unwrap();
    });
    let (status, body) = call_as(
        &original,
        &token,
        Method::POST,
        "/api/v1/webhooks",
        serde_json::json!({"url":url,"events":["NoteDeleted"]}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    let revoked = Arc::new(AtomicBool::new(false));
    let checked = Arc::new(AtomicUsize::new(0));
    let policy = Arc::new(RevocableCatalog {
        user: state
            .db
            .webhooks
            .get(id)
            .await
            .unwrap()
            .unwrap()
            .created_by_user_id
            .unwrap(),
        revoked: revoked.clone(),
        checked: checked.clone(),
    });
    let dispatcher = tokio::spawn(webhook_dispatcher(
        state.event_bus.clone(),
        state.db.clone(),
        policy,
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.event_bus.subscriber_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    state.event_bus.emit(ServerEvent::NoteDeleted {
        note_id: Uuid::new_v4(),
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    revoked.store(true, Ordering::Relaxed);
    let before = checked.load(Ordering::Relaxed);
    state.event_bus.emit(ServerEvent::NoteDeleted {
        note_id: Uuid::new_v4(),
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while checked.load(Ordering::Relaxed) == before {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .is_err()
    );
    dispatcher.abort();
    dispatcher.await.unwrap_err();
    state.db.webhooks.delete(id).await.unwrap();
    let legacy = state
        .db
        .webhooks
        .create(matric_core::CreateWebhookRequest {
            url,
            secret: None,
            events: vec!["NoteDeleted".into()],
            max_retries: 3,
        })
        .await
        .unwrap();
    assert!(state
        .db
        .webhooks
        .get(legacy)
        .await
        .unwrap()
        .unwrap()
        .created_by_user_id
        .is_none());
    let dispatcher = tokio::spawn(webhook_dispatcher(
        state.event_bus.clone(),
        state.db.clone(),
        Arc::new(RoleBasedPolicy),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.event_bus.subscriber_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    state.event_bus.emit(ServerEvent::NoteDeleted {
        note_id: Uuid::new_v4(),
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    dispatcher.abort();
    server.abort();
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn default_shard_excludes_b_only_configs_and_inventory_keeps_creation_order(
    admin: sqlx::PgPool,
) {
    let (state, _, original, token, a, b) = restricted(&admin).await;
    let config = Uuid::new_v4();
    let ctx = state.db.for_schema(&b).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query("INSERT INTO public.embedding_config(id,name,model,dimension) VALUES($1,$2,'b-only-model',768)")
        .bind(config).bind(format!("b-only-{config}")).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    for (schema, included) in [(&a, false), (&b, true)] {
        let response = knowledge_shard(
            State(state.clone()),
            Extension(ArchiveContext {
                schema: schema.clone(),
                is_default: false,
                name: None,
            }),
            Query(ShardExportQuery {
                schema_version: None,
                profile: Some("full-v1".into()),
                include: None,
                include_blobs: true,
            }),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let files = read_shard_archive(&bytes, ShardArchiveLimits::default()).unwrap();
        let rows =
            parse_shard_component_records("embedding_configs", &files["embedding_configs.json"])
                .unwrap();
        assert_eq!(
            rows.iter().any(|row| row["id"] == config.to_string()),
            included
        );
    }
    for path in [
        "/api/v1/archives",
        "/api/v1/memories",
        "/api/v1/memories/available",
    ] {
        let (status, body) = call_as(
            &original,
            &token,
            Method::GET,
            path,
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let names: Vec<_> = body
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["name"].as_str())
            .filter(|name| name.starts_with("archive_"))
            .collect();
        assert_eq!(names, ["archive_b", "archive_a"], "{path}");
    }
    state.db.pool.close().await;
}
