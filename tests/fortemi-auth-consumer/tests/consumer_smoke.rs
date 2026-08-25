//! Downstream smoke for the public `fortemi-auth` v1.1 contract.
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
    extract_tenant_id_strategy_a, AuthContext, AuthError, Credential, JwtToken, OAuthProvider,
    TenantRecord, TenantStatus, TenantStore, VerifiedClaims,
};
use fortemi_auth_mock::MemoryTenantStore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;
use xjp_oidc::{HttpClient, HttpClientError, JwtVerifier, MemoryCache};

const TENANT_ID: Uuid = Uuid::from_u128(0x00000000000040008000000000000001);
const AUTHORITY_COMMIT: &str = "1b6ddb1b58a12efc5b631386ad783cb12edec518";
const MANIFEST_SHA256: &str = "2df0a35edad67cc3e8869286183a4d098b1eb8fc2161432ed0b54ba69b17e242";
const RELEASE_POLICY_SHA256: &str =
    "d70491c336a62508ef3c7937af709dd121a6ec4f421ceab66486af3f371de8db";

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
    contract_id: String,
    contract_version: String,
    profile: String,
    config: CorpusConfig,
    jwks: Value,
    tenant_store_cases: Vec<TenantStoreCase>,
    cases: Vec<CorpusCase>,
}

#[derive(Deserialize)]
struct TenantStoreCase {
    id: String,
    store_result: String,
    expected: TenantStoreExpected,
}

#[derive(Deserialize)]
struct TenantStoreExpected {
    error: String,
    http_status: u16,
}

#[derive(Deserialize)]
struct ReleasePolicy {
    policy_id: String,
    policy_version: String,
    release_scheme: String,
    current_release: ReleaseIdentity,
    compatibility_cases: Vec<ReleaseCase>,
}

#[derive(Deserialize)]
struct ReleaseIdentity {
    version: String,
    tag: Option<String>,
    contract_version: String,
    profile: String,
    manifest_sha256: String,
}

#[derive(Deserialize)]
struct ReleaseCase {
    id: String,
    candidate: ReleaseIdentity,
    expected: ReleaseExpected,
}

#[derive(Deserialize)]
struct ReleaseExpected {
    outcome: String,
    error: Option<String>,
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
    let manifest_bytes = include_bytes!("../fixtures/fortemi-auth-v1.json");
    assert_eq!(sha256(manifest_bytes), MANIFEST_SHA256);
    let manifest: CorpusManifest =
        serde_json::from_slice(manifest_bytes).expect("canonical auth manifest must parse");
    assert_eq!(manifest.contract_id, "fortemi-auth-conformance");
    assert_eq!(manifest.contract_version, "1.1.0");
    assert_eq!(manifest.profile, "rust-node-jwt-v1");
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

struct FixtureTenantStore<'a>(&'a str);

impl TenantStore for FixtureTenantStore<'_> {
    async fn lookup(&self, tenant_id: Uuid) -> Result<Option<TenantRecord>, AuthError> {
        match self.0 {
            "unavailable" | "timeout" | "malformed_response" => {
                Err(AuthError::TenantStoreUnavailable)
            }
            "inactive" => Ok(Some(TenantRecord {
                id: tenant_id,
                status: TenantStatus::Suspended,
            })),
            "not_found" => Ok(None),
            _ => Err(AuthError::InternalError),
        }
    }
}

#[tokio::test]
async fn fortemi_executes_the_canonical_tenant_store_cases() {
    let manifest_bytes = include_bytes!("../fixtures/fortemi-auth-v1.json");
    assert_eq!(sha256(manifest_bytes), MANIFEST_SHA256);
    let manifest: CorpusManifest =
        serde_json::from_slice(manifest_bytes).expect("canonical auth manifest must parse");

    for case in manifest.tenant_store_cases {
        let claims = FixtureClaims(AuthContext {
            tenant_id: TENANT_ID,
            principal_id: "fixture-user-001".into(),
            credential: Credential::Bearer(JwtToken {
                jti: Some("fixture-jti-001".into()),
                algorithm: "RS256".into(),
                key_id: "fixture-key-1".into(),
            }),
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            scopes: vec!["read:note".into()],
            session_id: None,
        });
        let error =
            extract_tenant_id_strategy_a(&claims, &FixtureTenantStore(case.store_result.as_str()))
                .await
                .expect_err("tenant store fixture must reject");
        assert_eq!(error.code(), case.expected.error, "{}", case.id);
        assert_eq!(
            error.http_status(),
            case.expected.http_status,
            "{}",
            case.id
        );
    }
}

#[test]
fn fortemi_enforces_the_calver_release_policy() {
    let policy_bytes = include_bytes!("../fixtures/fortemi-auth-release-policy-v1.json");
    assert_eq!(sha256(policy_bytes), RELEASE_POLICY_SHA256);
    let policy: ReleasePolicy =
        serde_json::from_slice(policy_bytes).expect("release policy must parse");

    assert_eq!(policy.policy_id, "fortemi-auth-release-compatibility");
    assert_eq!(AUTHORITY_COMMIT.len(), 40);
    assert_eq!(policy.policy_version, "1.1.0");
    assert_eq!(policy.release_scheme, "calver-yyyy-m-patch");
    assert_eq!(policy.current_release.version, "2026.8.1");
    assert_eq!(policy.current_release.tag.as_deref(), Some("v2026.8.1"));
    assert_eq!(policy.current_release.contract_version, "1.1.0");
    assert_eq!(policy.current_release.profile, "rust-node-jwt-v1");
    assert_eq!(policy.current_release.manifest_sha256, MANIFEST_SHA256);
    assert_calver(&policy.current_release.version);

    for case in &policy.compatibility_cases {
        let result = evaluate_release(&policy.current_release, &case.candidate);
        if case.expected.outcome == "accepted" {
            assert_eq!(result, Ok(()), "{}", case.id);
        } else {
            assert_eq!(
                result,
                Err(case.expected.error.as_deref().expect("rejection error")),
                "{}",
                case.id
            );
        }
    }
}

#[test]
fn fortemi_pins_the_signed_authority_release_commit() {
    let lock = include_str!("../Cargo.lock");
    let source = format!(
        "git+https://git.integrolabs.net/Fortemi/fortemi-auth.git?tag=v2026.8.1#{AUTHORITY_COMMIT}"
    );
    assert!(
        lock.contains(&source),
        "consumer lock must pin the signed authority release commit"
    );
}

fn evaluate_release<'a>(
    current: &ReleaseIdentity,
    candidate: &'a ReleaseIdentity,
) -> Result<(), &'a str> {
    if candidate.version != current.version {
        return Err("unsupported_release");
    }
    if candidate.contract_version != current.contract_version
        || candidate.profile != current.profile
    {
        return Err("contract_mismatch");
    }
    if candidate.manifest_sha256 != current.manifest_sha256 {
        return Err("artifact_mismatch");
    }
    Ok(())
}

fn assert_calver(version: &str) {
    let parts: Vec<_> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "CalVer must have YYYY.M.PATCH components");
    assert_eq!(parts[0].len(), 4, "CalVer year must have four digits");
    for part in parts {
        assert!(
            !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()),
            "CalVer components must be numeric"
        );
        assert!(
            part == "0" || !part.starts_with('0'),
            "CalVer components must not have leading zeroes"
        );
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
