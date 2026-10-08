//! Shared helpers for the `/oauth/authorize` handler tests (#943). Needs `DATABASE_URL`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use matric_core::{ClientRegistrationRequest, CreateApiKeyRequest};
use tower::ServiceExt;

use super::config::AuthorizeConfig;
use super::AuthorizeRuntime;
use crate::{oauth_authorize_get, oauth_authorize_post, AppState, Database};

pub(super) const REDIRECT: &str = "https://client.example/callback";
pub(super) const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

pub(super) async fn state_with(config: AuthorizeConfig) -> AppState {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required for OAuth tests");
    let pool = matric_db::create_pool(&url)
        .await
        .expect("connect test database");
    let mut state = crate::tests::build_call_api_test_state(Database::new(pool), &url).await;
    state.oauth_authorize = Arc::new(AuthorizeRuntime::new(config));
    state
}

pub(super) async fn client(state: &AppState, scope: &str) -> String {
    state
        .db
        .oauth
        .register_client(ClientRegistrationRequest {
            client_name: format!("consent-test-{}", uuid::Uuid::new_v4()),
            redirect_uris: vec![REDIRECT.to_string()],
            grant_types: vec!["authorization_code".to_string()],
            response_types: Vec::new(),
            scope: Some(scope.to_string()),
            token_endpoint_auth_method: None,
            client_uri: None,
            logo_uri: None,
            contacts: None,
            policy_uri: None,
            tos_uri: None,
            software_id: None,
            software_version: None,
            software_statement: None,
        })
        .await
        .expect("register test client")
        .client_id
}

pub(super) async fn api_key(state: &AppState, scope: &str) -> String {
    state
        .db
        .oauth
        .create_api_key(CreateApiKeyRequest {
            name: format!("consent-owner-{}", uuid::Uuid::new_v4()),
            description: None,
            scope: scope.to_string(),
            expires_in_days: None,
        })
        .await
        .expect("create api key")
        .api_key
}

pub(super) struct Reply {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) body: String,
}

impl Reply {
    pub(super) fn location(&self) -> Option<&str> {
        self.headers
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
    }

    fn hidden(&self, name: &str) -> String {
        let marker = format!("name=\"{name}\" value=\"");
        let start = self.body.find(&marker).expect("hidden field") + marker.len();
        self.body[start..].split('"').next().unwrap().to_string()
    }

    /// The consent form's transaction id, CSRF token and binding cookie pair.
    pub(super) fn form(&self) -> (String, String, String) {
        let cookie = self.headers[header::SET_COOKIE].to_str().unwrap();
        let pair = cookie.split(';').next().unwrap().to_string();
        (self.hidden("transaction"), self.hidden("csrf_token"), pair)
    }
}

pub(super) async fn send(state: &AppState, request: Request<Body>) -> Reply {
    let router = Router::new()
        .route(
            "/oauth/authorize",
            get(oauth_authorize_get).post(oauth_authorize_post),
        )
        .with_state(state.clone());
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).to_string(),
    }
}

pub(super) fn authorize_uri(client_id: &str, redirect: &str, scope: &str) -> String {
    format!(
        "/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}&scope={}\
&state=st-123&code_challenge={CHALLENGE}&code_challenge_method=S256",
        urlencoding::encode(redirect),
        urlencoding::encode(scope),
    )
}

pub(super) fn get_request(
    uri: &str,
    peer: Option<SocketAddr>,
    owner: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(owner) = owner {
        builder = builder.header("x-forwarded-email", owner);
    }
    let mut request = builder.body(Body::empty()).unwrap();
    if let Some(peer) = peer {
        request.extensions_mut().insert(ConnectInfo(peer));
    }
    request
}

pub(super) struct Post<'a> {
    pub(super) transaction: &'a str,
    pub(super) csrf: &'a str,
    pub(super) cookie: Option<&'a str>,
    pub(super) action: &'a str,
    pub(super) credential: Option<&'a str>,
}

pub(super) fn post_request(
    post: &Post<'_>,
    peer: Option<SocketAddr>,
    owner: Option<&str>,
) -> Request<Body> {
    let mut body = format!(
        "transaction={}&csrf_token={}&action={}",
        post.transaction, post.csrf, post.action
    );
    if let Some(credential) = post.credential {
        body.push_str(&format!("&credential={}", urlencoding::encode(credential)));
    }
    let mut builder = Request::post("/oauth/authorize")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("sec-fetch-site", "same-origin");
    if let Some(cookie) = post.cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(owner) = owner {
        builder = builder.header("x-forwarded-email", owner);
    }
    let mut request = builder.body(Body::from(body)).unwrap();
    if let Some(peer) = peer {
        request.extensions_mut().insert(ConnectInfo(peer));
    }
    request
}

pub(super) async fn code_owner(state: &AppState, location: &str) -> Option<String> {
    let code = location.split("code=").nth(1)?.split('&').next()?;
    let code = urlencoding::decode(code).ok()?.to_string();
    sqlx::query_scalar("SELECT user_id FROM oauth_authorization_code WHERE code = $1")
        .bind(code)
        .fetch_one(&state.db.pool)
        .await
        .ok()
        .flatten()
}

pub(super) async fn codes_for(state: &AppState, client_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM oauth_authorization_code WHERE client_id = $1")
        .bind(client_id)
        .fetch_one(&state.db.pool)
        .await
        .unwrap()
}
