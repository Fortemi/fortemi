//! `/oauth/authorize` tests for trusted-proxy owners and the disabled mode (#943).

use std::net::SocketAddr;

use axum::http::{HeaderName, StatusCode};

use super::config::AuthorizeConfig;
use super::test_support::*;
use crate::trusted_proxy::TrustedProxyConfig;

async fn proxy_state() -> crate::AppState {
    let mut state = state_with(AuthorizeConfig::trusted_header(HeaderName::from_static(
        "x-forwarded-email",
    )))
    .await;
    state.trusted_proxy_config = TrustedProxyConfig::from_value(Some("10.0.0.0/8")).unwrap();
    state
}

fn proxy() -> Option<SocketAddr> {
    Some("10.9.8.7:40000".parse().unwrap())
}

#[tokio::test]
async fn a_proxy_signed_in_owner_approves_without_a_credential() {
    let state = proxy_state().await;
    let client_id = client(&state, "read mcp").await;
    let uri = authorize_uri(&client_id, REDIRECT, "read mcp");
    let page = send(&state, get_request(&uri, proxy(), Some("ops@example.com"))).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("Signed in as"));
    assert!(!page.body.contains("name=\"credential\""));
    let (tx, csrf, cookie) = page.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: None,
    };
    let reply = send(
        &state,
        post_request(&post, proxy(), Some("ops@example.com")),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let location = reply.location().unwrap().to_string();
    assert_eq!(
        code_owner(&state, &location).await.as_deref(),
        Some("proxy:ops@example.com")
    );
}

#[tokio::test]
async fn the_owner_header_is_ignored_from_untrusted_peers() {
    let state = proxy_state().await;
    let client_id = client(&state, "read").await;
    let uri = authorize_uri(&client_id, REDIRECT, "read");
    let direct: Option<SocketAddr> = Some("192.0.2.50:5000".parse().unwrap());
    let reply = send(&state, get_request(&uri, direct, Some("ops@example.com"))).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.location().is_none());

    let page = send(&state, get_request(&uri, proxy(), Some("ops@example.com"))).await;
    let (tx, csrf, cookie) = page.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: None,
    };
    let reply = send(&state, post_request(&post, direct, Some("ops@example.com"))).await;
    assert_eq!(
        reply.status,
        StatusCode::UNAUTHORIZED,
        "the POST re-checks the owner"
    );
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn disabled_mode_denies_valid_requests_and_still_refuses_bad_redirects() {
    let state = state_with(AuthorizeConfig::disabled()).await;
    let client_id = client(&state, "read").await;
    let reply = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read"), None, None),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let location = reply.location().unwrap();
    assert!(location.starts_with(&format!("{REDIRECT}?error=access_denied")));
    assert!(location.contains("state=st-123"));
    let bad = authorize_uri(&client_id, "https://attacker.example/cb", "read");
    let reply = send(&state, get_request(&bad, None, None)).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(reply.location().is_none());
    assert_eq!(codes_for(&state, &client_id).await, 0);
}
