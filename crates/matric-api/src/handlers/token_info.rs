//! `GET /api/v1/auth/token-info`: report the verified caller's credential class and scopes.
//!
//! The MCP server calls this with the bearer it received, so externally issued OIDC
//! tokens are verified by exactly the same hosted verifier as REST requests (#1151).
//! The response never contains token material, principal ids or tenant ids.

use axum::http::header;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Serialize;

use crate::{RequireAuth, ValidatedBearerIdentity};
use matric_core::AuthPrincipal;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TokenInfo {
    pub active: bool,
    /// `hosted_oidc`, `oauth_access_token` or `api_key`.
    pub token_class: &'static str,
    pub scope: String,
    pub tenant_bound: bool,
    /// Unix seconds, when the credential carries an expiry known to this request.
    pub exp: Option<i64>,
    /// `human` or `service` for hosted OIDC tokens (claim policy, #1152).
    pub principal_kind: Option<&'static str>,
}

pub fn token_info_for(
    principal: &AuthPrincipal,
    identity: Option<&ValidatedBearerIdentity>,
) -> TokenInfo {
    #[cfg(feature = "hosted-auth")]
    let hosted = identity.and_then(|identity| identity.canonical_context.as_ref());
    #[cfg(not(feature = "hosted-auth"))]
    let hosted: Option<&()> = None;

    let token_class = match (hosted.is_some(), principal) {
        (true, _) => "hosted_oidc",
        (false, AuthPrincipal::ApiKey { .. }) => "api_key",
        (false, _) => "oauth_access_token",
    };
    #[cfg(feature = "hosted-auth")]
    let exp = hosted.map(|context| context.expires_at.timestamp());
    #[cfg(not(feature = "hosted-auth"))]
    let exp = None;
    #[cfg(feature = "hosted-auth")]
    let principal_kind = hosted.map(|context| context.principal_kind.as_str());
    #[cfg(not(feature = "hosted-auth"))]
    let principal_kind = None;

    TokenInfo {
        active: true,
        token_class,
        scope: principal.scope_str().to_string(),
        tenant_bound: identity.is_some_and(|identity| identity.tenant_id.is_some()),
        exp,
        principal_kind,
    }
}

/// GET /api/v1/auth/token-info — verify the presented bearer and describe it.
///
/// Hidden from the public OpenAPI (route policy `Hidden`): it exists for the
/// co-deployed MCP server. 401 for missing/invalid/expired/wrong-issuer or
/// wrong-audience tokens; 403 when a verified token lacks `read` or its
/// tenant/client is not admitted.
pub async fn token_info(
    auth: RequireAuth,
    identity: Option<Extension<ValidatedBearerIdentity>>,
) -> impl IntoResponse {
    let identity = identity.map(|Extension(identity)| identity);
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(token_info_for(&auth.principal, identity.as_ref())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn api_keys_and_oauth_tokens_report_class_and_scope_only() {
        let key = AuthPrincipal::ApiKey {
            key_id: Uuid::new_v4(),
            scope: "read mcp".into(),
        };
        let info = token_info_for(&key, None);
        assert_eq!(info.token_class, "api_key");
        assert_eq!(info.scope, "read mcp");
        assert!(!info.tenant_bound);

        let oauth = AuthPrincipal::OAuthClient {
            client_id: "client-sentinel".into(),
            scope: "mcp read write".into(),
            user_id: Some("user-sentinel".into()),
        };
        let info = token_info_for(&oauth, None);
        assert_eq!(info.token_class, "oauth_access_token");
        let body = serde_json::to_string(&info).unwrap();
        assert!(!body.contains("client-sentinel"));
        assert!(!body.contains("user-sentinel"));
    }

    #[cfg(feature = "hosted-auth")]
    #[test]
    fn hosted_contexts_report_hosted_class_expiry_and_tenant_binding() {
        use fortemi_auth_core::{AuthContext, Credential, JwtToken};

        let tenant = Uuid::new_v4();
        let expires_at = chrono::Utc::now() + chrono::Duration::minutes(5);
        let principal = AuthPrincipal::OAuthClient {
            client_id: "hosted-oidc".into(),
            scope: "mcp read".into(),
            user_id: Some("subject-sentinel".into()),
        };
        let identity = ValidatedBearerIdentity {
            principal: principal.clone(),
            tenant_id: Some(tenant),
            canonical_context: Some(AuthContext {
                tenant_id: tenant,
                principal_id: "subject-sentinel".into(),
                credential: Credential::Bearer(JwtToken {
                    jti: None,
                    algorithm: "RS256".into(),
                    key_id: "kid".into(),
                }),
                issued_at: chrono::Utc::now(),
                expires_at,
                scopes: vec!["mcp".into(), "read".into()],
                session_id: None,
                principal_kind: fortemi_auth_core::PrincipalKind::Human,
                scope_grants: Vec::new(),
            }),
        };
        let info = token_info_for(&principal, Some(&identity));
        assert_eq!(info.token_class, "hosted_oidc");
        assert_eq!(info.scope, "mcp read");
        assert!(info.tenant_bound);
        assert_eq!(info.exp, Some(expires_at.timestamp()));
        assert_eq!(info.principal_kind, Some("human"));
        let body = serde_json::to_string(&info).unwrap();
        assert!(!body.contains("subject-sentinel"));
        assert!(!body.contains(&tenant.to_string()));
    }
}
