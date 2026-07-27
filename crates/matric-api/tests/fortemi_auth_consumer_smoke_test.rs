//! Downstream smoke for the public `fortemi-auth` v0.1 contract.
//!
//! Full hosted router and tenant-transaction integration remains owned by
//! #728 after its RLS and hardened-role prerequisites are complete.

use std::convert::Infallible;

use axum::body::{to_bytes, Body};
use axum::http::{header::AUTHORIZATION, Request, StatusCode};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{Duration, Utc};
use fortemi_auth_axum::{auth_layer, AuthState, NoApiKeys};
use fortemi_auth_core::{
    AuthContext, AuthError, Credential, JwtToken, OAuthProvider, VerifiedClaims,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

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
