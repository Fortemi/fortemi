//! External-OIDC mutation scope compatibility.
use super::*;
#[sqlx::test(migrations = false)]
async fn external_archive_management_and_overview_require_admin(admin: sqlx::PgPool) {
    let state = setup(&admin).await;
    let runtime = state.db.pool.clone();
    let app = crate::production_router(state, None);
    let issuer = external_oidc_test_issuer::issuer();
    let owner = issuer.token(serde_json::json!({"groups":["admin"]}));
    create(&app, &owner, "management").await;
    for group in ["reader", "writer"] {
        let token = issuer.token(serde_json::json!({"groups":[group]}));
        for prefix in ["archives", "memories"] {
            for (method, suffix, body) in [
                (
                    Method::PATCH,
                    "",
                    serde_json::json!({"description":"denied"}),
                ),
                (Method::DELETE, "", serde_json::Value::Null),
                (
                    Method::POST,
                    "/clone",
                    serde_json::json!({"new_name":"denied"}),
                ),
                (Method::POST, "/set-default", serde_json::Value::Null),
            ] {
                let (status, body) = call_as(
                    &app,
                    &token,
                    method,
                    &format!("/api/v1/{prefix}/management{suffix}"),
                    body,
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{group}: {body}");
            }
        }
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            "/api/v1/memories/overview",
            serde_json::Value::Null,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let (status, body) = call_as(
            &app,
            &token,
            Method::GET,
            "/api/v1/memories/available",
            serde_json::Value::Null,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body
            .as_array()
            .unwrap()
            .iter()
            .any(|memory| memory["name"] == "management"));
        assert!(body
            .as_array()
            .unwrap()
            .iter()
            .all(|memory| memory.as_object().unwrap().len() == 2));
    }
    for (method, path, body, expected) in [
        (
            Method::PATCH,
            "/api/v1/archives/management",
            serde_json::json!({"description":"allowed"}),
            StatusCode::NO_CONTENT,
        ),
        (
            Method::POST,
            "/api/v1/archives/management/clone",
            serde_json::json!({"new_name":"allowed_clone"}),
            StatusCode::CREATED,
        ),
        (
            Method::POST,
            "/api/v1/archives/management/set-default",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            Method::GET,
            "/api/v1/memories/overview",
            serde_json::Value::Null,
            StatusCode::OK,
        ),
        (
            Method::DELETE,
            "/api/v1/archives/allowed_clone",
            serde_json::Value::Null,
            StatusCode::NO_CONTENT,
        ),
    ] {
        let (status, body) = call_as(&app, &owner, method, path, body, None).await;
        assert_eq!(status, expected, "{body}");
    }
    drop(app);
    runtime.close().await;
}
