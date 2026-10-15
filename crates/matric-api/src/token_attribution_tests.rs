use super::*;
use fortemi_auth_core::{AuthContext, Credential, JwtToken, TokenAttribution};
use tower::ServiceExt;

fn context(attribution: Option<TokenAttribution>) -> AuthContext {
    let now = chrono::Utc::now();
    let mut context = AuthContext::new(
        Uuid::new_v4(),
        "synthetic-subject".to_string(),
        Credential::Bearer(JwtToken {
            jti: Some("synthetic-jti".to_string()),
            algorithm: "RS256".to_string(),
            key_id: "synthetic-kid".to_string(),
        }),
        now,
        now + chrono::Duration::minutes(5),
        vec!["read".to_string()],
    );
    context.token_attribution = attribution;
    context
}

fn attribution(email: Option<&str>, email_verified: Option<bool>) -> TokenAttribution {
    let mut attribution = TokenAttribution::default();
    attribution.azp = Some("synthetic-web-client".to_string());
    attribution.email = email.map(ToOwned::to_owned);
    attribution.email_verified = email_verified;
    attribution
}

fn app_user_with_attribution() -> matric_db::AppUser {
    let now = chrono::Utc::now();
    matric_db::AppUser {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        iss: "https://issuer.example".to_string(),
        sub: "synthetic-subject".to_string(),
        email: Some("synthetic@example.invalid".to_string()),
        email_verified: true,
        display_name: None,
        groups: Vec::new(),
        current_scopes: vec!["read".to_string(), "write".to_string()],
        kind: "user".to_string(),
        azp: Some("synthetic-web-client".to_string()),
        status: "active".to_string(),
        first_seen_at: now,
        last_seen_at: now,
        last_oidc_at: now,
    }
}

async fn me_body(principal: RequestPrincipal) -> serde_json::Value {
    let identity = ValidatedBearerIdentity {
        principal: AuthPrincipal::OAuthClient {
            client_id: "attribution-test".into(),
            user_id: Some(principal.sub.clone()),
            scope: principal.scopes.join(" "),
        },
        tenant_id: principal.tenant_id,
        expires_at: None,
        parent_credential: None,
        stream_identity: None,
        request_principal: Some(principal),
        canonical_context: None,
    };
    let response = Router::new()
        .route("/api/v1/me", get(handlers::user_principal::me))
        .layer(Extension(identity))
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn me_returns_verified_token_attribution() {
    let context = context(Some(attribution(
        Some("synthetic@example.invalid"),
        Some(true),
    )));
    let principal = RequestPrincipal::from_auth_context("https://issuer.example", &context);
    let body = me_body(principal).await;
    assert_eq!(body["azp"], "synthetic-web-client");
    assert_eq!(body["email"], "synthetic@example.invalid");
    assert_eq!(body["email_verified"], true);
    assert_eq!(body["email_unverified"], false);
    assert_eq!(body["sub"], context.principal_id);
    assert_eq!(body["iss"], "https://issuer.example");
    assert_eq!(body["scopes"], serde_json::json!(["read"]));
    assert_eq!(body["credential_class"], "oidc");
}

#[tokio::test]
async fn me_returns_null_email_for_azp_only_token() {
    let context = context(Some(attribution(None, None)));
    let body = me_body(RequestPrincipal::from_auth_context(
        "https://issuer.example",
        &context,
    ))
    .await;
    assert_eq!(body["azp"], "synthetic-web-client");
    assert!(body["email"].is_null());
    assert_eq!(body["email_verified"], false);
}

#[tokio::test]
async fn me_defaults_missing_verification_to_false_and_preserves_false() {
    for verification in [None, Some(false)] {
        let context = context(Some(attribution(
            Some("synthetic@example.invalid"),
            verification,
        )));
        let body = me_body(RequestPrincipal::from_auth_context(
            "https://issuer.example",
            &context,
        ))
        .await;
        assert_eq!(body["email"], "synthetic@example.invalid");
        assert_eq!(body["email_verified"], false);
        assert_eq!(body["scopes"], serde_json::json!(["read"]));
    }
}

#[tokio::test]
async fn me_returns_no_token_attribution_for_api_key() {
    let body = me_body(RequestPrincipal::legacy(&AuthPrincipal::ApiKey {
        key_id: Uuid::new_v4(),
        scope: "read".to_string(),
    }))
    .await;
    assert!(body["azp"].is_null());
    assert!(body["email"].is_null());
    assert_eq!(body["email_verified"], false);
    assert_eq!(body["credential_class"], "legacy");
}

#[tokio::test]
async fn me_returns_no_token_attribution_for_pat_with_stored_oidc_attribution() {
    let user = app_user_with_attribution();
    let pat_id = Uuid::new_v4();
    let principal = RequestPrincipal::pat(user.tenant_id, &user, pat_id, vec!["read".to_string()]);
    let body = me_body(principal).await;
    assert!(body["azp"].is_null());
    assert!(body["email"].is_null());
    assert_eq!(body["email_verified"], false);
    assert_eq!(body["email_unverified"], true);
    assert_eq!(body["user_id"], user.id.to_string());
    assert_eq!(body["iss"], user.iss);
    assert_eq!(body["sub"], user.sub);
    assert_eq!(body["credential_class"], "pat");
    assert_eq!(body["pat_id"], pat_id.to_string());
    assert_eq!(body["scopes"], serde_json::json!(["read"]));
}

#[test]
fn pat_decision_audit_has_no_token_attribution_with_stored_oidc_attribution() {
    let user = app_user_with_attribution();
    let principal = RequestPrincipal::pat(
        user.tenant_id,
        &user,
        Uuid::new_v4(),
        vec!["read".to_string()],
    );
    let auth = Auth {
        principal: AuthPrincipal::OAuthClient {
            client_id: "fortemi-pat".to_string(),
            user_id: Some(user.sub.clone()),
            scope: "read".to_string(),
        },
    };
    for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
        let input = route_policy::authorization_input_for_request(&Method::GET, "/api/v1/me", None)
            .unwrap();
        let input = apply_claim_policy_audit_context(input, None, Some(&principal));
        let event = auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
        assert_eq!(event.attrs["azp_present"], false);
        assert_eq!(event.attrs["credential_class"], "pat");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("synthetic@example.invalid"));
        assert!(!serialized.contains("synthetic-web-client"));
        assert!(!event.attrs.contains_key("email"));
    }
}

#[test]
fn decision_audit_reports_authorized_party_presence_without_email() {
    let auth = Auth {
        principal: AuthPrincipal::OAuthClient {
            client_id: "external-oidc".to_string(),
            user_id: Some("synthetic-subject".to_string()),
            scope: "read".to_string(),
        },
    };
    for (attribution, expected_azp_present) in [
        (
            Some(attribution(Some("synthetic@example.invalid"), Some(true))),
            true,
        ),
        (None, false),
    ] {
        let context = context(attribution);
        let principal = RequestPrincipal::from_auth_context("https://issuer.example", &context);
        for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
            let input =
                route_policy::authorization_input_for_request(&Method::GET, "/api/v1/me", None)
                    .unwrap();
            let input = apply_claim_policy_audit_context(input, Some(&context), Some(&principal));
            let event =
                auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
            assert_eq!(event.attrs["azp_present"], expected_azp_present);
            let serialized = serde_json::to_string(&event).unwrap();
            assert!(!serialized.contains("synthetic@example.invalid"));
            assert!(!serialized.contains("synthetic-web-client"));
            assert!(!event.attrs.contains_key("email"));
        }
    }
}
