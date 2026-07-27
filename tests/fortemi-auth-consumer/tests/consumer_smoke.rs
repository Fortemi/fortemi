//! Downstream smoke for the public `fortemi-auth` v0.1 contract.
//!
//! Full hosted router and tenant-transaction integration remains owned by
//! #728 after its RLS and hardened-role prerequisites are complete.

use std::convert::Infallible;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{header::AUTHORIZATION, Request, StatusCode};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{Duration, Utc};
use fortemi_auth_axum::{auth_layer, AuthState, NoApiKeys};
use fortemi_auth_clerk::{ClerkConfig, ClerkProvider};
use fortemi_auth_core::{
    AuthContext, AuthError, Credential, JwtToken, OAuthProvider, VerifiedClaims,
};
use fortemi_auth_mock::MemoryTenantStore;
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;
use xjp_oidc::{HttpClient, HttpClientError, JwtVerifier, MemoryCache};

const TENANT_ID: Uuid = Uuid::from_u128(0x00000000000040008000000000000001);

struct FixtureClaims(AuthContext);

impl VerifiedClaims for FixtureClaims {
    fn issuer(&self) -> &str {
        "https://issuer.fixture.invalid"
    }

    fn audience(&self) -> &str {
        "fortemi-fixture"
    }

    fn tenant_claim(&self) -> Option<&str> {
        Some("00000000-0000-4000-8000-000000000001")
    }

    fn into_context(self, _tenant_id: Uuid) -> AuthContext {
        self.0
    }
}

struct FixtureProvider;

impl OAuthProvider for FixtureProvider {
    type Claims = FixtureClaims;

    async fn verify_token(&self, token: &str) -> Result<Self::Claims, AuthError> {
        if token != "synthetic-valid-token" {
            return Err(AuthError::InvalidSignature);
        }

        let now = Utc::now();
        Ok(FixtureClaims(AuthContext {
            tenant_id: TENANT_ID,
            principal_id: "fixture-user-001".into(),
            credential: Credential::Bearer(JwtToken {
                jti: Some("fixture-jti-001".into()),
                algorithm: "RS256".into(),
                key_id: "fixture-key-1".into(),
            }),
            issued_at: now,
            expires_at: now + Duration::hours(1),
            scopes: vec!["read:note".into(), "write:note".into()],
            session_id: Some("fixture-session-001".into()),
        }))
    }

    async fn extract_tenant_id(&self, claims: &Self::Claims) -> Result<Uuid, AuthError> {
        Ok(claims.0.tenant_id)
    }
}

async fn protected(Extension(context): Extension<AuthContext>) -> Result<Json<Value>, Infallible> {
    Ok(Json(json!({
        "tenant_id": context.tenant_id,
        "principal_id": context.principal_id,
        "scopes": context.scopes,
    })))
}

fn app() -> Router {
    Router::new()
        .route("/protected", get(protected))
        .layer(auth_layer(AuthState::new(FixtureProvider, NoApiKeys)))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("JSON response")
}

#[tokio::test]
async fn public_axum_contract_injects_the_verified_context() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(AUTHORIZATION, "Bearer synthetic-valid-token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible router");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await,
        json!({
            "tenant_id": "00000000-0000-4000-8000-000000000001",
            "principal_id": "fixture-user-001",
            "scopes": ["read:note", "write:note"],
        })
    );
}

#[tokio::test]
async fn public_axum_contract_fails_closed_with_redacted_codes() {
    let missing = app()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible router");
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(missing).await,
        json!({"error": "malformed_token"})
    );

    let invalid = app()
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(AUTHORIZATION, "Bearer synthetic-invalid-token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("infallible router");
    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(invalid).await,
        json!({"error": "invalid_signature"})
    );
}

#[derive(Deserialize)]
struct CorpusManifest {
    config: CorpusConfig,
    jwks: Value,
    cases: Vec<CorpusCase>,
}

#[derive(Deserialize)]
struct CorpusConfig {
    issuer: String,
    audience: String,
    tenant_claim_name: String,
    clock_skew_seconds: i64,
}

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    token: String,
    required_scope: Option<String>,
    expected: CorpusExpected,
}

#[derive(Deserialize)]
struct CorpusExpected {
    outcome: String,
    error: Option<String>,
    tenant_id: Option<Uuid>,
    principal_id: Option<String>,
    scopes: Option<Vec<String>>,
    key_id: Option<String>,
}

struct CorpusHttp {
    issuer: String,
    jwks: Value,
}

#[async_trait]
impl HttpClient for CorpusHttp {
    async fn get_value(&self, url: &str) -> Result<Value, HttpClientError> {
        if url.ends_with("/jwks.json") {
            return Ok(self.jwks.clone());
        }

        Ok(json!({
            "issuer": self.issuer,
            "authorization_endpoint": format!("{}/authorize", self.issuer),
            "token_endpoint": format!("{}/token", self.issuer),
            "jwks_uri": format!("{}/jwks.json", self.issuer),
        }))
    }

    async fn post_form_value(
        &self,
        _url: &str,
        _form: &[(String, String)],
        _auth_header: Option<(&str, &str)>,
    ) -> Result<Value, HttpClientError> {
        Err(HttpClientError::NotSupported("fixture is GET-only".into()))
    }

    async fn post_json_value(
        &self,
        _url: &str,
        _body: &Value,
        _auth_header: Option<(&str, &str)>,
    ) -> Result<Value, HttpClientError> {
        Err(HttpClientError::NotSupported("fixture is GET-only".into()))
    }
}

#[tokio::test]
async fn fortemi_executes_the_canonical_v1_corpus() {
    let manifest: CorpusManifest =
        serde_json::from_str(include_str!("../fixtures/fortemi-auth-v1.json"))
            .expect("canonical auth manifest must parse");
    let config = ClerkConfig {
        issuer: manifest.config.issuer.clone(),
        audience: manifest.config.audience.clone(),
        tenant_claim_name: manifest.config.tenant_claim_name,
        clock_skew_seconds: manifest.config.clock_skew_seconds,
        jwks_cache_capacity: 4,
        http_timeout_seconds: 5,
    };
    let verifier: JwtVerifier<MemoryCache, CorpusHttp> = JwtVerifier::builder()
        .default_issuer(config.issuer.clone())
        .audience(config.audience.clone())
        .http(Arc::new(CorpusHttp {
            issuer: config.issuer.clone(),
            jwks: manifest.jwks,
        }))
        .cache(Arc::new(MemoryCache))
        .clock_skew(config.clock_skew_seconds)
        .build()
        .expect("fixture verifier must build");
    let provider =
        ClerkProvider::with_verifier(config, MemoryTenantStore::with_active(TENANT_ID), verifier)
            .expect("fixture provider must build");

    for case in manifest.cases {
        let result = provider
            .authenticate(&case.token)
            .await
            .and_then(|context| {
                if let Some(scope) = &case.required_scope {
                    context.require_scope(scope)?;
                }
                Ok(context)
            });

        if case.expected.outcome == "accepted" {
            let context = result.unwrap_or_else(|error| {
                panic!("case {} unexpectedly rejected as {}", case.id, error.code())
            });
            assert_eq!(
                Some(context.tenant_id),
                case.expected.tenant_id,
                "{}",
                case.id
            );
            assert_eq!(
                Some(context.principal_id.as_str()),
                case.expected.principal_id.as_deref(),
                "{}",
                case.id
            );
            assert_eq!(
                Some(context.scopes.as_slice()),
                case.expected.scopes.as_deref(),
                "{}",
                case.id
            );
            let Credential::Bearer(jwt) = context.credential else {
                panic!("case {} did not return bearer context", case.id);
            };
            assert_eq!(
                Some(jwt.key_id.as_str()),
                case.expected.key_id.as_deref(),
                "{}",
                case.id
            );
        } else {
            let error = result
                .err()
                .unwrap_or_else(|| panic!("case {} unexpectedly accepted", case.id));
            assert_eq!(
                Some(error.code()),
                case.expected.error.as_deref(),
                "{}",
                case.id
            );
        }
    }
}
