//! Delegated ingest identity and query-token authentication ordering.
use super::*;

#[sqlx::test(migrations = false)]
async fn ingest_rechecks_the_minting_user_and_stored_archive(admin: sqlx::PgPool) {
    let (state, app, original, token, a, b) = restricted(&admin).await;
    let mut minted = Vec::new();
    for name in ["archive_a", "archive_b"] {
        let (status, body) = call_as(
            &original,
            &token,
            Method::POST,
            "/api/v1/ingest/tokens",
            serde_json::Value::Null,
            Some(name),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        minted.push(body["token"].as_str().unwrap().to_string());
    }
    let data = state
        .ingest_token_store
        .validate_ingest(&minted[1])
        .await
        .unwrap();
    let identity = validate_bearer_identity(&state, &token, None)
        .await
        .unwrap();
    assert_eq!(data.schema, b);
    assert_eq!(
        data.ingest_identity.unwrap().user_id,
        sse::stable_user_id(&identity)
    );
    let invalid = call_as(
        &app,
        "mm_ist_invalid",
        Method::POST,
        "/api/v1/ingest/stream",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    let denied = call_as(
        &app,
        &minted[1],
        Method::POST,
        "/api/v1/ingest/stream",
        serde_json::Value::Null,
        Some("archive_a"),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNAUTHORIZED);
    assert_eq!(denied, invalid);
    let response = app
        .oneshot(request(
            &minted[0],
            Method::POST,
            "/api/v1/ingest/stream",
            serde_json::Value::Null,
            Some("archive_b"),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "stored allowed A overrides a denied header"
    );
    let _ = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {a}.note"))
        .fetch_one(&state.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "synthetic empty stream stores no notes");
    state.db.pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn invalid_query_token_cannot_distinguish_existing_and_missing_archives(admin: sqlx::PgPool) {
    let (state, app, _, _, _, _) = restricted(&admin).await;
    let mut results = Vec::new();
    for memory in ["archive_b", "missing_archive"] {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/events?token=invalid&memory={memory}"))
                    .header("x-request-id", "00000000-0000-4000-8000-000000000001")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        results.push((status, headers, bytes));
    }
    assert_eq!(results[0].0, StatusCode::UNAUTHORIZED);
    assert_eq!(results[0].0, results[1].0);
    assert_eq!(
        results[0].2, results[1].2,
        "complete error bodies must be indistinguishable"
    );
    for name in ["content-type", "www-authenticate", "cache-control"] {
        assert_eq!(results[0].1.get(name), results[1].1.get(name));
    }
    state.db.pool.close().await;
}
