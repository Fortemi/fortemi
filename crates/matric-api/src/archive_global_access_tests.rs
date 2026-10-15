//! Complete-catalog gates for shared namespaces, global data, and events.
use super::*;
use futures::StreamExt;

#[sqlx::test(migrations = false)]
async fn global_operations_require_complete_catalog_before_observing_names(admin: sqlx::PgPool) {
    let (state, app, _, token, _, _) = restricted(&admin).await;
    let initial_count: i64 = sqlx::query_scalar("SELECT count(*) FROM archive_registry")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    let mut creations = Vec::new();
    for name in ["archive_b", "missing_archive"] {
        creations.push(
            call_as(
                &app,
                &token,
                Method::POST,
                "/api/v1/archives",
                serde_json::json!({"name":name}),
                Some("archive_a"),
            )
            .await,
        );
    }
    assert_eq!(creations[0].0, StatusCode::FORBIDDEN);
    assert_eq!(creations[0], creations[1]);
    for (method, path, body) in [
        (Method::GET, "/api/v1/memory/info", serde_json::Value::Null),
        (
            Method::GET,
            "/api/v1/backup/database",
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/backup/database/snapshot",
            serde_json::json!({}),
        ),
        (
            Method::POST,
            "/api/v1/backup/database/restore",
            serde_json::json!({"filename":"missing.sql","skip_snapshot":true}),
        ),
        (
            Method::POST,
            "/api/v1/backup/database/restore",
            serde_json::json!({"memory":"archive_a","filename":"existing-dump.sql.gz","skip_snapshot":true}),
        ),
        (
            Method::POST,
            "/api/v1/archives/archive_a/clone",
            serde_json::json!({"new_name":"archive_b"}),
        ),
        (Method::GET, "/api/v1/jobs/stats", serde_json::Value::Null),
        (Method::GET, "/api/v1/jobs/pending", serde_json::Value::Null),
        (Method::GET, "/api/v1/jobs/status", serde_json::Value::Null),
        (Method::POST, "/api/v1/jobs/pause", serde_json::Value::Null),
        (Method::POST, "/api/v1/jobs/resume", serde_json::Value::Null),
    ] {
        let (status, body) = call_as(&app, &token, method, path, body, Some("archive_a")).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}: {body}");
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM archive_registry")
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(count, initial_count, "no namespace mutation");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn attributed_global_events_are_hidden_in_live_and_replay(admin: sqlx::PgPool) {
    let (state, app, _, token, a, _) = restricted(&admin).await;
    let context = EventContext {
        tenant_id: Some(
            state
                .external_oidc
                .as_ref()
                .unwrap()
                .default_tenant_id
                .to_string(),
        ),
        memory: Some("archive_a".into()),
        archive_name: Some("archive_a".into()),
        archive_schema: Some(a),
        ..Default::default()
    };
    let emit = |payload| state.event_bus.emit_with_context(payload, context.clone());
    let mut receiver = state.event_bus.subscribe();
    emit(ServerEvent::NoteDeleted {
        note_id: Uuid::new_v4(),
    });
    let cursor = receiver.recv().await.unwrap().event_id;
    emit(ServerEvent::QueueStatus {
        total_jobs: 999,
        running: 998,
        pending: 997,
    });
    let replay = Uuid::new_v4();
    emit(ServerEvent::NoteDeleted { note_id: replay });
    let mut req = request(
        &token,
        Method::GET,
        "/api/v1/events?memory=archive_a",
        serde_json::Value::Null,
        Some("archive_a"),
    );
    req.headers_mut()
        .insert("last-event-id", cursor.to_string().parse().unwrap());
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    assert_eq!(
        next_frame(&mut stream).await["payload"]["note_id"],
        replay.to_string()
    );
    emit(ServerEvent::QueueStatus {
        total_jobs: 999,
        running: 998,
        pending: 997,
    });
    emit(ServerEvent::InferenceConfigChanged {
        default_backend: "synthetic".into(),
        embedding_backend: None,
        changed_fields: vec!["synthetic".into()],
    });
    let live = Uuid::new_v4();
    emit(ServerEvent::NoteDeleted { note_id: live });
    assert_eq!(
        next_frame(&mut stream).await["payload"]["note_id"],
        live.to_string()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), stream.next())
            .await
            .is_err()
    );
    drop(stream);
    state.db.pool.close().await;
}

struct DenyB(String);
#[async_trait::async_trait]
impl AuthorizationPolicy for DenyB {
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
        Ok(r.attrs["archive_schema"] != self.0)
    }
    fn policy_id(&self) -> &'static str {
        "deny_b"
    }
    fn policy_version(&self) -> &'static str {
        "1"
    }
}

#[sqlx::test(migrations = false)]
async fn anonymous_legacy_websocket_refuses_restrictive_policy_at_upgrade(admin: sqlx::PgPool) {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let (mut state, _, _, _, _, b) = restricted(&admin).await;
    state.require_auth = false;
    state.auth_mode = external_oidc::AuthMode::Local;
    state.external_oidc = None;
    state.authorization_policy = Arc::new(DenyB(b));
    let app = crate::production_router(state.clone(), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut req = format!("ws://{}/api/v1/ws", listener.local_addr().unwrap())
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("X-Fortemi-Memory", "archive_a".parse().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let result = tokio_tungstenite::connect_async(req).await;
    server.abort();
    match result {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN)
        }
        _ => panic!("restrictive policy must refuse the upgrade"),
    }
    assert_eq!(state.ws_connections.load(Ordering::Relaxed), 0);
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn legacy_websocket_closes_before_event_after_catalog_grant_loss(admin: sqlx::PgPool) {
    use futures::SinkExt;
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
    let mut state = setup(&admin).await;
    let token = external_oidc_test_issuer::issuer().token(serde_json::json!({"groups":["admin"]}));
    let original = crate::production_router(state.clone(), None);
    create(&original, &token, "archive_a").await;
    state.require_auth = false;
    state.auth_mode = external_oidc::AuthMode::Local;
    state.external_oidc = None;
    // Policy denies a name that does not yet exist, so admission succeeds.
    struct DenyFuture;
    #[async_trait::async_trait]
    impl AuthorizationPolicy for DenyFuture {
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
            Ok(r.id.as_deref() != Some("archive_b"))
        }
        fn policy_id(&self) -> &'static str {
            "deny_future"
        }
        fn policy_version(&self) -> &'static str {
            "1"
        }
    }
    state.authorization_policy = Arc::new(DenyFuture);
    let app = crate::production_router(state.clone(), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut req = format!("ws://{}/api/v1/ws", listener.local_addr().unwrap())
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("X-Fortemi-Memory", "archive_a".parse().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let (mut socket, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while state.event_bus.subscriber_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let first = Uuid::new_v4();
    state
        .event_bus
        .emit(ServerEvent::NoteDeleted { note_id: first });
    loop {
        match socket.next().await.unwrap().unwrap() {
            Message::Text(text) => {
                assert!(text.contains(&first.to_string()));
                break;
            }
            Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    let b = create(&original, &token, "archive_b").await;
    state.event_bus.emit_with_context(
        ServerEvent::NoteDeleted {
            note_id: Uuid::new_v4(),
        },
        EventContext {
            memory: Some("archive_b".into()),
            archive_name: Some("archive_b".into()),
            archive_schema: Some(b),
            ..Default::default()
        },
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Ping(_) => {} // Transport heartbeat can already be queued.
                Message::Close(_) => break,
                other => panic!("denied event must never be delivered: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    server.abort();
    state.db.pool.close().await;
}
