//! #1201 signed credentials through the production router and runtime role.
use super::*;
use crate::sse_tests::next_frame;
use matric_api::authorization_policy::{select_policy_with, PolicyMode};
use matric_core::{AuthzContext, AuthzError, Resource};

#[path = "archive_access_support.rs"]
mod support;
use support::{call_as, create, request, restricted, setup, OnlyArchive};
#[path = "archive_global_access_tests.rs"]
mod global;
#[path = "archive_job_access_tests.rs"]
mod jobs;
#[path = "archive_management_tests.rs"]
mod management;
#[path = "archive_round3_tests.rs"]
mod round3;
#[path = "archive_round4_tests.rs"]
mod round4;
#[path = "archive_round5_tests.rs"]
mod round5;
#[path = "archive_stream_access_tests.rs"]
mod streams;

#[sqlx::test(migrations = false)]
async fn archive_hook_filters_router_selection_inventory_and_sse_live_replay(admin: sqlx::PgPool) {
    let mut state = setup(&admin).await;
    let redis = std::env::var("REDIS_URL").expect("disposable Redis");
    state.ingest_token_store = matric_api::services::IngestTokenStore::with_ttl(&redis, 60)
        .await
        .unwrap();
    let issuer = external_oidc_test_issuer::issuer();
    let token = issuer.token(serde_json::json!({"groups":["admin"]}));
    let original = crate::production_router(state.clone(), None);
    let a = create(&original, &token, "archive_a").await;
    let b = create(&original, &token, "archive_b").await;
    // A denied implicit default must still allow discovery of A.
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
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    let user = identity.request_principal.unwrap().user_id.unwrap();
    let checked = Arc::new(AtomicUsize::new(0));
    let counter = checked.clone();
    let errors = Arc::new(AtomicBool::new(false));
    let policy_errors = errors.clone();
    let a_schema = a.clone();
    state.authorization_policy = select_policy_with(
        PolicyMode {
            multi_tenant: false,
            external_oidc: true,
        },
        Some(&move |mode, default| {
            assert!(mode.external_oidc && !mode.multi_tenant);
            Arc::new(OnlyArchive {
                default,
                name: "archive_a".into(),
                schema: a_schema.clone(),
                user,
                checked: counter.clone(),
                errors: policy_errors.clone(),
            })
        }),
    );
    let app = crate::production_router(state.clone(), None);
    // Unknown and denied selection must have the same complete problem body.
    for (path, header) in [
        ("/api/v1/memory/context", true),
        ("/api/v1/archives/", false),
        ("/api/v1/memories/", false),
    ] {
        let mut results = Vec::new();
        for name in ["archive_b", "unknown"] {
            let path = if header {
                path.to_owned()
            } else {
                format!("{path}{name}/stats")
            };
            results.push(
                call_as(
                    &app,
                    &token,
                    Method::GET,
                    &path,
                    serde_json::Value::Null,
                    header.then_some(name),
                )
                .await,
            );
        }
        assert_eq!(results[0].0, StatusCode::NOT_FOUND);
        assert_eq!(results[0], results[1]);
    }
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        "/api/v1/memory/context",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["schema_name"], a);
    for path in [
        "/api/v1/memories/available",
        "/api/v1/archives",
        "/api/v1/memories/overview",
    ] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            path,
            serde_json::Value::Null,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let memories = body.get("memories").unwrap_or(&body).as_array().unwrap();
        assert_eq!(memories.len(), 1, "{body}");
        assert_eq!(memories[0]["name"], "archive_a");
        if path.ends_with("overview") {
            assert_eq!(body["database_size_bytes"], body["total_size_bytes"]);
            assert_eq!(body["memory_count"], 1);
        }
    }
    // Explicit body and alternate named selectors cannot bypass the grant.
    let (status, body) = call_as(
        &app,
        &token,
        Method::POST,
        "/api/v1/search/federated",
        serde_json::json!({"q":"synthetic","memories":["all"]}),
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["memories_searched"], serde_json::json!(["archive_a"]));
    for (method, template, body) in [
        (
            Method::PATCH,
            "/api/v1/archives/NAME",
            serde_json::json!({"description":"denied"}),
        ),
        (
            Method::DELETE,
            "/api/v1/archives/NAME",
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/archives/NAME/clone",
            serde_json::json!({"new_name":"denied_clone"}),
        ),
        (
            Method::POST,
            "/api/v1/archives/NAME/set-default",
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/jobs/pause/NAME",
            serde_json::Value::Null,
        ),
        (
            Method::POST,
            "/api/v1/jobs/resume/NAME",
            serde_json::Value::Null,
        ),
        (
            Method::GET,
            "/api/v1/backup/memory/NAME",
            serde_json::Value::Null,
        ),
    ] {
        let denied = call_as(
            &app,
            &token,
            method.clone(),
            &template.replace("NAME", "archive_b"),
            body.clone(),
            Some("archive_a"),
        )
        .await;
        let unknown = call_as(
            &app,
            &token,
            method,
            &template.replace("NAME", "unknown"),
            body,
            Some("archive_a"),
        )
        .await;
        assert_eq!(denied.0, StatusCode::NOT_FOUND, "{denied:?}");
        assert_eq!(denied, unknown);
    }
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "probes do not select the denied default"
    );
    // Query-token reconnects carry the same app_user ID as their signed parent.
    // A denied default B cannot prevent an explicit query selection of A.
    let (status, minted) = call_as(
        &app,
        &token,
        Method::POST,
        "/api/v1/events/tokens",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{minted}");
    for memory in ["archive_a", "archive_b", "unknown"] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!(
                        "/api/v1/events?token={}&memory={memory}",
                        minted["token"].as_str().unwrap()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if memory == "archive_a" {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            }
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            &token,
            Method::GET,
            "/api/v1/events?memory=archive_a",
            serde_json::Value::Null,
            Some("archive_b"),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "query selection precedes the header"
    );
    // SSE query selection also conceals denied archives.
    for memory in ["archive_b", "unknown"] {
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            &format!("/api/v1/events?memory={memory}"),
            serde_json::Value::Null,
            Some("archive_a"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }
    let tenant = identity.tenant_id.unwrap();
    let emit = |name: &str, schema: &str, id: Uuid| {
        state.event_bus.emit_with_context(
            ServerEvent::NoteDeleted { note_id: id },
            EventContext {
                tenant_id: Some(tenant.to_string()),
                memory: Some(name.into()),
                archive_name: Some(name.into()),
                archive_schema: Some(schema.into()),
                ..Default::default()
            },
        )
    };
    let anchor = Uuid::new_v4();
    let mut events = state.event_bus.subscribe();
    emit("archive_a", &a, anchor);
    let cursor = events.recv().await.unwrap().event_id;
    let replay = Uuid::new_v4();
    emit("archive_b", &b, Uuid::new_v4());
    emit("archive_a", &a, replay);
    // The external stream resolves its default archive at admission; the policy
    // also checks each matching live/replay envelope.
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
    let mut req = request(
        &token,
        Method::GET,
        "/api/v1/events?types=note.deleted",
        serde_json::Value::Null,
        None,
    );
    req.headers_mut()
        .insert("last-event-id", cursor.to_string().parse().unwrap());
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    let event = next_frame(&mut stream).await;
    assert_eq!(event["payload"]["note_id"], replay.to_string());
    let live = Uuid::new_v4();
    emit("archive_b", &b, Uuid::new_v4());
    emit("archive_a", &a, live);
    let event = next_frame(&mut stream).await;
    assert_eq!(event["payload"]["note_id"], live.to_string());
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(150),
            futures::StreamExt::next(&mut stream)
        )
        .await
        .is_err(),
        "no extra denied event may remain buffered"
    );
    // Grant lookup errors fail closed for HTTP and already admitted streams.
    errors.store(true, Ordering::Relaxed);
    let (status, body) = call_as(
        &app,
        &token,
        Method::GET,
        "/api/v1/memory/context",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    emit("archive_a", &a, Uuid::new_v4());
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(150),
            futures::StreamExt::next(&mut stream)
        )
        .await
        .is_err(),
        "an admitted stream cannot bypass a failed policy"
    );
    assert!(checked.load(Ordering::Relaxed) > 10);
    drop(stream);
    drop(app);
    drop(original);
    state.db.pool.close().await;
}
