//! `/oauth/authorize` tests for the API-key owner mode (#943). Needs `DATABASE_URL`.

use axum::http::{header, StatusCode};

use super::config::AuthorizeConfig;
use super::test_support::*;

fn assert_framing_protected(reply: &Reply) {
    assert_eq!(reply.headers[header::X_FRAME_OPTIONS], "DENY");
    let csp = reply.headers[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap();
    assert!(csp.contains("frame-ancestors 'none'"));
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn unauthenticated_clients_never_receive_a_code() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read write").await;
    let page = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read"), None, None),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    assert_framing_protected(&page);
    assert!(page.body.contains("name=\"credential\""));
    assert!(
        !page.body.contains(REDIRECT),
        "the redirect URI stays server-side"
    );
    let (tx, csrf, cookie) = page.form();
    for credential in [None, Some(""), Some("mm_key_not_a_real_key")] {
        let post = Post {
            transaction: &tx,
            csrf: &csrf,
            cookie: Some(&cookie),
            action: "approve",
            credential,
        };
        let reply = send(&state, post_request(&post, None, None)).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert!(reply.location().is_none());
        assert_framing_protected(&reply);
    }
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn an_authenticated_owner_approves_and_the_code_records_the_subject() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read write").await;
    let key = api_key(&state, "admin").await;
    let page = send(
        &state,
        get_request(
            &authorize_uri(&client_id, REDIRECT, "read write"),
            None,
            None,
        ),
    )
    .await;
    let (tx, csrf, cookie) = page.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&key),
    };
    let reply = send(&state, post_request(&post, None, None)).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_framing_protected(&reply);
    let location = reply.location().unwrap().to_string();
    assert!(location.starts_with(&format!("{REDIRECT}?code=")));
    assert!(location.contains("&state=st-123"));
    assert!(location.contains("&iss=http%3A%2F%2Flocalhost%3A3000"));
    let subject = code_owner(&state, &location)
        .await
        .expect("code carries a subject");
    assert!(subject.starts_with("api_key:"), "subject was {subject}");

    let replay = send(&state, post_request(&post, None, None)).await;
    assert_eq!(
        replay.status,
        StatusCode::BAD_REQUEST,
        "transactions are single use"
    );
    assert!(replay.location().is_none());
}

#[tokio::test]
async fn a_credential_cannot_grant_scopes_it_does_not_hold() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read write mcp").await;
    let reader = api_key(&state, "read").await;
    let page = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read mcp"), None, None),
    )
    .await;
    let (tx, csrf, cookie) = page.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&reader),
    };
    let reply = send(&state, post_request(&post, None, None)).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.body.contains("does not hold every requested scope"));
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn external_idp_mode_refuses_api_key_owner_proof() {
    let mut state = state_with(AuthorizeConfig::api_key_only()).await;
    state.oauth_external_idp_configured = true;
    let client_id = client(&state, "read").await;
    let key = api_key(&state, "read").await;
    let page = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read"), None, None),
    )
    .await;
    let (tx, csrf, cookie) = page.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&key),
    };
    let reply = send(&state, post_request(&post, None, None)).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.body.contains("A valid credential is required"));
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn untrusted_clients_and_redirects_get_local_errors_without_redirects() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read").await;
    for uri in [
        authorize_uri(&client_id, "https://attacker.example/steal", "read"),
        authorize_uri("mm_unknown_client", REDIRECT, "read"),
    ] {
        let reply = send(&state, get_request(&uri, None, None)).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert!(reply.location().is_none());
        assert!(!reply.body.contains("attacker.example"));
        assert_framing_protected(&reply);
    }
}

#[tokio::test]
async fn crafted_posts_cannot_redirect_anywhere() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let crafted = Post {
        transaction: "forged",
        csrf: "forged",
        cookie: None,
        action: "deny",
        credential: None,
    };
    let reply = send(&state, post_request(&crafted, None, None)).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(reply.location().is_none());
}

#[tokio::test]
async fn tampered_csrf_missing_binding_and_cross_site_posts_fail() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read").await;
    let key = api_key(&state, "admin").await;
    let uri = authorize_uri(&client_id, REDIRECT, "read");

    let (tx, _, cookie) = send(&state, get_request(&uri, None, None)).await.form();
    let bad_csrf = Post {
        transaction: &tx,
        csrf: "tampered",
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&key),
    };
    assert_eq!(
        send(&state, post_request(&bad_csrf, None, None))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let (tx, csrf, _) = send(&state, get_request(&uri, None, None)).await.form();
    let no_cookie = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: None,
        action: "approve",
        credential: Some(&key),
    };
    assert_eq!(
        send(&state, post_request(&no_cookie, None, None))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let (tx, csrf, cookie) = send(&state, get_request(&uri, None, None)).await.form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&key),
    };
    let mut request = post_request(&post, None, None);
    request
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert_eq!(send(&state, request).await.status, StatusCode::FORBIDDEN);
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn denial_returns_to_the_registered_redirect_with_state() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read").await;
    let (tx, csrf, cookie) = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read"), None, None),
    )
    .await
    .form();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "deny",
        credential: None,
    };
    let reply = send(&state, post_request(&post, None, None)).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let location = reply.location().unwrap();
    assert!(location.starts_with(&format!("{REDIRECT}?error=access_denied")));
    assert!(location.contains("&state=st-123") && location.contains("&iss="));
}

#[tokio::test]
async fn expired_transactions_fail_closed() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read").await;
    let key = api_key(&state, "admin").await;
    let (tx, csrf, cookie) = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "read"), None, None),
    )
    .await
    .form();
    state.oauth_authorize.store.expire_all();
    let post = Post {
        transaction: &tx,
        csrf: &csrf,
        cookie: Some(&cookie),
        action: "approve",
        credential: Some(&key),
    };
    let reply = send(&state, post_request(&post, None, None)).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(codes_for(&state, &client_id).await, 0);
}

#[tokio::test]
async fn scope_escalation_and_plain_pkce_are_rejected_to_the_registered_redirect() {
    let state = state_with(AuthorizeConfig::api_key_only()).await;
    let client_id = client(&state, "read").await;
    let reply = send(
        &state,
        get_request(&authorize_uri(&client_id, REDIRECT, "admin"), None, None),
    )
    .await;
    assert!(reply.location().unwrap().contains("error=invalid_scope"));
    let plain = authorize_uri(&client_id, REDIRECT, "read").replace("method=S256", "method=plain");
    let reply = send(&state, get_request(&plain, None, None)).await;
    assert!(reply.location().unwrap().contains("error=invalid_request"));
}
