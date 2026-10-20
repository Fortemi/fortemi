use super::*;
use fortemi_auth_core::{AuthContext, Credential, JwtToken, TokenAttribution};
use tower::ServiceExt;

pub(super) fn assert_no_email(value: &serde_json::Value) {
    // Inspect the complete event, including nested keys and strings, rather
    // than checking only a known email fixture or the top-level attrs map.
    let serialized = serde_json::to_string(value).unwrap();
    let email =
        regex::Regex::new(r"(?i)[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9.-]+\.[a-z]{2,}").unwrap();
    assert!(
        !email.is_match(&serialized),
        "email-shaped value in audit: {serialized}"
    );
    fn check_keys(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, value) in object {
                    assert!(!key.eq_ignore_ascii_case("email"), "email key in audit");
                    check_keys(value);
                }
            }
            serde_json::Value::Array(array) => array.iter().for_each(check_keys),
            _ => {}
        }
    }
    check_keys(value);
}

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

fn bearer_identity(principal: RequestPrincipal) -> ValidatedBearerIdentity {
    ValidatedBearerIdentity {
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
    }
}

async fn me_body(principal: RequestPrincipal) -> serde_json::Value {
    let identity = bearer_identity(principal);
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
        assert_eq!(event.attrs.get("azp"), Some(&serde_json::Value::Null));
        assert_eq!(event.attrs["credential_class"], "pat");
        let serialized = serde_json::to_string(&event).unwrap();
        assert!(!serialized.contains("synthetic@example.invalid"));
        assert!(!serialized.contains("synthetic-web-client"));
        assert!(!event.attrs.contains_key("email"));
        assert_no_email(&serde_json::to_value(&event).unwrap());
    }
}

#[test]
fn legacy_decision_audits_have_no_authorized_party() {
    for principal in [
        AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read".to_string(),
        },
        AuthPrincipal::OAuthClient {
            client_id: "internal-oauth-client".to_string(),
            user_id: Some("synthetic-subject".to_string()),
            scope: "read".to_string(),
        },
    ] {
        let request_principal = RequestPrincipal::legacy(&principal);
        let auth = Auth { principal };
        for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
            let input =
                route_policy::authorization_input_for_request(&Method::GET, "/api/v1/notes", None)
                    .unwrap();
            let input = apply_claim_policy_audit_context(input, None, Some(&request_principal));
            let event =
                auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
            assert_eq!(event.attrs.get("azp"), Some(&serde_json::Value::Null));
            assert_eq!(event.attrs["azp_present"], false);
            assert_no_email(&serde_json::to_value(&event).unwrap());
        }
    }
}

#[test]
fn decision_and_jit_audits_report_exact_authorized_party_without_email() {
    let auth = Auth {
        principal: AuthPrincipal::OAuthClient {
            client_id: "external-oidc".to_string(),
            user_id: Some("synthetic-subject".to_string()),
            scope: "read".to_string(),
        },
    };
    for client in [
        Some("web-auth"),
        Some("fortemi-cli"),
        Some("fortemi-admin-cli"),
        None,
    ] {
        let mut attribution = attribution(Some("synthetic@example.invalid"), Some(true));
        attribution.azp = client.map(ToOwned::to_owned);
        let context = context(Some(attribution));
        let principal = RequestPrincipal::from_auth_context("https://issuer.example", &context);
        let jit = app_user_jit_audit_event(&principal, context.tenant_id);
        assert_eq!(jit.attrs.get("azp"), Some(&serde_json::json!(client)));
        assert_no_email(&serde_json::to_value(&jit).unwrap());
        for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
            let input =
                route_policy::authorization_input_for_request(&Method::GET, "/api/v1/me", None)
                    .unwrap();
            let input = apply_claim_policy_audit_context(input, Some(&context), Some(&principal));
            let event =
                auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
            assert_eq!(event.attrs.get("azp"), Some(&serde_json::json!(client)));
            assert_eq!(event.attrs["azp_present"], client.is_some());
            assert_no_email(&serde_json::to_value(&event).unwrap());
        }
    }
}

#[test]
fn derived_resource_audits_keep_verified_azp_without_policy_changes() {
    for client in [
        Some("web-auth"),
        Some("fortemi-cli"),
        Some("fortemi-admin-cli"),
        None,
    ] {
        let mut attribution = attribution(Some("synthetic@example.invalid"), Some(true));
        attribution.azp = client.map(ToOwned::to_owned);
        let context = context(Some(attribution));
        let identity = bearer_identity(RequestPrincipal::from_auth_context(
            "https://issuer.example",
            &context,
        ));
        let auth = Auth {
            principal: identity.principal.clone(),
        };
        for path in [
            "/api/v1/archives/synthetic",
            "/api/v1/notes/018fd1a0-0000-7000-8000-000000000001",
        ] {
            let original =
                route_policy::authorization_input_for_request(&Method::GET, path, None).unwrap();
            let input = apply_identity_audit_attribution(original.clone(), Some(&identity));
            assert_eq!(input.context, original.context);
            assert_eq!(input.resource, original.resource);
            assert_eq!(input.action, original.action);
            for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
                let event =
                    auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
                assert_eq!(event.attrs.get("azp"), Some(&serde_json::json!(client)));
                assert_eq!(event.attrs["azp_present"], client.is_some());
                assert_no_email(&serde_json::to_value(&event).unwrap());
            }
            let without = apply_identity_audit_attribution(input, None);
            assert_eq!(without, original);
        }
    }
}

#[test]
fn decision_azp_uses_jit_sanitization_and_length_bound() {
    let auth = Auth {
        principal: AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read".to_string(),
        },
    };
    for (azp, expected) in [
        (
            "web\nclient|name;cli\t".to_string(),
            "web client,name,cli ".to_string(),
        ),
        ("x".repeat(600), format!("{}[TRUNCATED]", "x".repeat(512))),
        (
            "client_secret=redact-me".to_string(),
            "[REDACTED]".to_string(),
        ),
    ] {
        let mut principal = RequestPrincipal::legacy(&auth.principal);
        principal.azp = Some(azp);
        let jit = app_user_jit_audit_event(&principal, Uuid::new_v4());
        assert_eq!(jit.attrs["azp"], expected);
        for outcome in [AuditOutcome::Success, AuditOutcome::Denied] {
            let input =
                route_policy::authorization_input_for_request(&Method::GET, "/api/v1/notes", None)
                    .unwrap();
            let input = apply_claim_policy_audit_context(input, None, Some(&principal));
            let event =
                auth_decision_audit_event(&auth, &input, outcome, None, "role_based", "test");
            assert_eq!(event.attrs["azp"], jit.attrs["azp"]);
        }
    }
}

#[tokio::test]
async fn authorized_party_and_email_do_not_change_policy_decisions() {
    let auth = Auth {
        principal: AuthPrincipal::OAuthClient {
            client_id: "external-oidc".to_string(),
            user_id: Some("synthetic-subject".to_string()),
            scope: "read".to_string(),
        },
    };
    let context = context(None);
    for (method, allowed) in [(Method::GET, true), (Method::POST, false)] {
        let input =
            route_policy::authorization_input_for_request(&method, "/api/v1/notes", None).unwrap();
        let mut principal = RequestPrincipal::from_auth_context("https://issuer.example", &context);
        let baseline =
            apply_claim_policy_audit_context(input.clone(), Some(&context), Some(&principal));
        for client in [
            Some("web-auth"),
            Some("fortemi-cli"),
            Some("fortemi-admin-cli"),
            None,
        ] {
            principal.azp = client.map(ToOwned::to_owned);
            for email in [
                None,
                Some("admin@example.invalid"),
                Some("reader@example.invalid"),
            ] {
                principal.email = email.map(ToOwned::to_owned);
                principal.email_verified = email.is_some();
                let input = apply_claim_policy_audit_context(
                    input.clone(),
                    Some(&context),
                    Some(&principal),
                );
                // All inputs visible to an AuthorizationPolicy are unchanged,
                // apart from the pre-existing azp presence bit.
                assert_eq!(input.action, baseline.action);
                assert_eq!(input.resource, baseline.resource);
                let mut expected_context = baseline.context.clone();
                expected_context
                    .environment
                    .insert("azp_present".into(), serde_json::json!(client.is_some()));
                assert_eq!(input.context, expected_context);
                assert!(!input.context.environment.contains_key("azp"));
                assert!(!input.context.environment.contains_key("email"));
                let decision = RoleBasedPolicy
                    .authorize(
                        &auth.principal,
                        &input.action,
                        &input.resource,
                        &input.context,
                    )
                    .await
                    .unwrap();
                if allowed {
                    assert!(matches!(decision, Decision::Allow { .. }));
                } else {
                    assert!(matches!(
                        decision,
                        Decision::Deny {
                            reason: DenyReason::MissingScope,
                            ..
                        }
                    ));
                }
            }
        }
    }
}

/// Run against a disposable, migrated database with DATABASE_URL. Missing DB
/// configuration skips this test; a configured but broken database fails it.
#[tokio::test]
async fn persisted_decision_and_jit_audits_keep_exact_azp_without_email() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping persisted azp audit test: DATABASE_URL unavailable");
        return;
    };
    let pool = matric_db::create_pool(&database_url)
        .await
        .expect("connect audit test DB");
    let provisioned: bool =
        sqlx::query_scalar("SELECT to_regclass('public.tenant_registry') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    if !provisioned {
        matric_db::Database::new(pool.clone())
            .migrate()
            .await
            .unwrap();
    }
    let sink = matric_db::PostgresAuditSink::new(pool.clone());
    for client in [
        Some("web-auth"),
        Some("fortemi-cli"),
        Some("fortemi-admin-cli"),
        None,
    ] {
        let mut attribution = attribution(Some("synthetic@example.invalid"), Some(true));
        attribution.azp = client.map(ToOwned::to_owned);
        let context = context(Some(attribution));
        sqlx::query("INSERT INTO public.tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')")
            .bind(context.tenant_id)
            .bind(format!("audit-azp-test-{}", context.tenant_id))
            .execute(&pool).await.unwrap();
        let principal = RequestPrincipal::from_auth_context("https://issuer.example", &context);
        let auth = Auth {
            principal: AuthPrincipal::OAuthClient {
                client_id: "external-oidc".to_string(),
                user_id: Some(context.principal_id.clone()),
                scope: "read".to_string(),
            },
        };
        sink.emit(app_user_jit_audit_event(&principal, context.tenant_id))
            .await
            .unwrap();
        for (method, allowed) in [(Method::GET, true), (Method::POST, false)] {
            let input = route_policy::authorization_input_for_request(
                &method,
                "/api/v1/notes",
                Some(&context.tenant_id.to_string()),
            )
            .unwrap();
            let input = apply_claim_policy_audit_context(input, Some(&context), Some(&principal));
            let result = authorize_policy_input(&RoleBasedPolicy, &sink, &auth, &input).await;
            if allowed {
                result.expect("read request allowed");
            } else {
                assert_eq!(result.unwrap_err().status(), StatusCode::FORBIDDEN);
            }
        }
        let rows: Vec<(String, String, String, Option<String>, serde_json::Value)> = sqlx::query_as(
            "SELECT category, action, outcome, attrs->>'azp', to_jsonb(event) FROM public.audit_event AS event WHERE tenant_id = $1 ORDER BY action",
        ).bind(context.tenant_id).fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 3, "one allow, one deny, and one JIT event");
        for (
            (category, action, outcome, azp, row),
            (expected_category, expected_action, expected_outcome),
        ) in rows.into_iter().zip([
            ("auth", "decision", "Success"),
            ("auth", "decision_deny", "Denied"),
            ("identity", "user.jit_provisioned", "Success"),
        ]) {
            assert_eq!(category, expected_category);
            assert_eq!(action, expected_action);
            assert_eq!(outcome, expected_outcome);
            assert_eq!(azp.as_deref(), client);
            assert_eq!(row["attrs"].get("azp"), Some(&serde_json::json!(client)));
            if category == "auth" {
                assert_eq!(row["attrs"]["azp_present"], client.is_some());
            }
            assert_no_email(&row);
        }
    }
    pool.close().await;
}
