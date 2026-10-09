//! Handler tests for the dynamic client registration policy (#944). Needs `DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use matric_core::CreateApiKeyRequest;
use tower::ServiceExt;

use crate::oauth_registration::OAuthRegistrationMode;
use crate::{
    oauth_authorize_get, oauth_discovery, oauth_register, oauth_token, AppState, Database,
};

async fn state_with(mode: OAuthRegistrationMode) -> AppState {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for OAuth tests");
    let pool = matric_db::create_pool(&url)
        .await
        .expect("connect test database");
    let mut state = crate::tests::build_call_api_test_state(Database::new(pool), &url).await;
    state.oauth_registration = mode;
    state
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/oauth/register", post(oauth_register))
        .route("/oauth/authorize", get(oauth_authorize_get))
        .route("/oauth/token", post(oauth_token))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_discovery),
        )
        .with_state(state)
}

async fn external_disabled_state() -> AppState {
    let mut state = state_with(OAuthRegistrationMode::Disabled).await;
    state.oauth_local_as_enabled = false;
    state.oauth_external_idp_configured = true;
    state.oauth_authorize = std::sync::Arc::new(crate::oauth_consent::AuthorizeRuntime::new(
        crate::oauth_consent::config::AuthorizeConfig::disabled(),
    ));
    state
}

const BODY: &str =
    r#"{"client_name":"policy test","grant_types":["client_credentials"],"scope":"read"}"#;

async fn register(
    state: AppState,
    bearer: Option<&str>,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let mut request =
        Request::post("/oauth/register").header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = router(state)
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

async fn post_token(state: AppState, body: &str) -> (StatusCode, serde_json::Value) {
    let request = Request::post("/oauth/token")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router(state).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

async fn authorize(state: AppState) -> (StatusCode, String) {
    let request = Request::get("/oauth/authorize?response_type=code&client_id=mm_test&redirect_uri=https%3A%2F%2Fclient.example%2Fcb")
        .body(Body::empty())
        .unwrap();
    let response = router(state).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn discovery(state: AppState) -> serde_json::Value {
    let request = Request::get("/.well-known/oauth-authorization-server")
        .body(Body::empty())
        .unwrap();
    let response = router(state).oneshot(request).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn api_key(state: &AppState, scope: &str) -> String {
    state
        .db
        .oauth
        .create_api_key(CreateApiKeyRequest {
            name: format!("dcr-policy-{}", uuid::Uuid::new_v4()),
            description: None,
            scope: scope.to_string(),
            expires_in_days: None,
        })
        .await
        .expect("create api key")
        .api_key
}

#[tokio::test]
async fn enabled_mode_registers_without_management_metadata() {
    let state = state_with(OAuthRegistrationMode::Enabled).await;
    let (status, body) = register(state.clone(), None, BODY).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["client_id"]
        .as_str()
        .is_some_and(|id| id.starts_with("mm_")));
    assert!(body.get("registration_client_uri").is_none());
    assert!(body.get("registration_access_token").is_none());
    let metadata = discovery(state).await;
    assert_eq!(
        metadata["registration_endpoint"],
        "http://localhost:3000/oauth/register"
    );
}

#[tokio::test]
async fn disabled_mode_refuses_with_a_stable_error_and_is_not_advertised() {
    let state = state_with(OAuthRegistrationMode::Disabled).await;
    let name = format!("disabled-{}", uuid::Uuid::new_v4());
    let unique = format!(r#"{{"client_name":"{name}","grant_types":["client_credentials"]}}"#);
    for body in [unique.as_str(), "not json"] {
        let (status, problem) = register(state.clone(), None, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            problem["detail"],
            crate::oauth_registration::DISABLED_DETAIL
        );
    }
    let written: i64 =
        sqlx::query_scalar("SELECT count(*) FROM oauth_client WHERE client_name = $1")
            .bind(&name)
            .fetch_one(&state.db.pool)
            .await
            .unwrap();
    assert_eq!(written, 0, "disabled registration must not write clients");
    let metadata = discovery(state).await;
    assert!(metadata.get("registration_endpoint").is_none());
}

#[tokio::test]
async fn admin_mode_requires_an_admin_bearer_credential() {
    let state = state_with(OAuthRegistrationMode::Admin).await;
    assert_eq!(
        register(state.clone(), None, BODY).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        register(state.clone(), Some("mm_key_not_a_real_key"), BODY)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let reader = api_key(&state, "read write").await;
    assert_eq!(
        register(state.clone(), Some(&reader), BODY).await.0,
        StatusCode::FORBIDDEN
    );
    let admin = api_key(&state, "admin").await;
    let (status, body) = register(state.clone(), Some(&admin), BODY).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body.get("registration_access_token").is_none());
    assert!(discovery(state)
        .await
        .get("registration_endpoint")
        .is_none());
}

#[tokio::test]
async fn unsupported_token_endpoint_auth_method_is_rejected() {
    let state = state_with(OAuthRegistrationMode::Enabled).await;
    let body = r#"{"client_name":"public","grant_types":["client_credentials"],"token_endpoint_auth_method":"none"}"#;
    assert_eq!(register(state, None, body).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_redirect_uri_metadata_is_rejected() {
    let state = state_with(OAuthRegistrationMode::Enabled).await;
    let body = r#"{"client_name":"bad redirect","grant_types":["authorization_code"],"redirect_uris":["http://attacker.example/cb"]}"#;
    let (status, body) = register(state, None, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_redirect_uri");
}

#[tokio::test]
async fn external_idp_defaults_disable_local_as_entrypoints() {
    let state = external_disabled_state().await;

    let (status, body) = register(state.clone(), None, BODY).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "access_denied");

    let (status, body) = post_token(
        state.clone(),
        "grant_type=authorization_code&client_id=mm_x&client_secret=s",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "access_denied");

    let (status, body) = authorize(state).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("access_denied"));
}
