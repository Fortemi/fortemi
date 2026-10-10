//! Dynamic client registration policy for `POST /oauth/register` (#944).
//!
//! `FORTEMI_OAUTH_DYNAMIC_REGISTRATION` selects one of three modes:
//! - `enabled`: open RFC 7591 registration (the community default, unchanged).
//! - `admin`: registration needs a bearer credential with the `admin` scope
//!   (the RFC 7591 initial-access-token pattern). Not advertised in discovery.
//! - `disabled`: registration returns a stable 403 and is not advertised.
//!
//! Hosted (`FORTEMI_MULTI_TENANT=true`) defaults to `disabled` and refuses `enabled`,
//! because open registration there has no abuse controls. First-party clients such as
//! the bundle's MCP introspection client are provisioned with
//! `matric-api admin oauth-client register` instead of the public endpoint.

use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use matric_core::AuthPrincipal;

use crate::{problem_response, validate_bearer_identity, AppState, ProblemType};

pub(crate) const REGISTRATION_ENV: &str = "FORTEMI_OAUTH_DYNAMIC_REGISTRATION";

/// Client authentication methods a registered (always confidential) client may declare.
pub(crate) const SUPPORTED_TOKEN_ENDPOINT_AUTH_METHODS: [&str; 2] =
    ["client_secret_basic", "client_secret_post"];

pub(crate) const DISABLED_DETAIL: &str = "Dynamic client registration is disabled on this server.";
const ADMIN_REQUIRED_DETAIL: &str =
    "Dynamic client registration requires a bearer credential with the admin scope.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OAuthRegistrationMode {
    Enabled,
    Admin,
    Disabled,
}

impl OAuthRegistrationMode {
    /// Parse the configured mode. Unset keeps the community default (`enabled`) and
    /// the stricter hosted default (`disabled`).
    pub(crate) fn from_value(raw: Option<&str>, multi_tenant: bool) -> anyhow::Result<Self> {
        let mode = match raw.map(str::trim).filter(|value| !value.is_empty()) {
            None if multi_tenant => Self::Disabled,
            None => Self::Enabled,
            Some("enabled") => Self::Enabled,
            Some("admin") => Self::Admin,
            Some("disabled") => Self::Disabled,
            Some(_) => anyhow::bail!(
                "{REGISTRATION_ENV} has an invalid value. Expected one of: enabled, admin, disabled."
            ),
        };
        if multi_tenant && mode == Self::Enabled {
            anyhow::bail!(
                "{REGISTRATION_ENV}=enabled is not allowed with FORTEMI_MULTI_TENANT=true; \
                 use admin or disabled"
            );
        }
        Ok(mode)
    }

    #[allow(dead_code)]
    pub(crate) fn from_env(multi_tenant: bool) -> anyhow::Result<Self> {
        Self::from_value(
            std::env::var(REGISTRATION_ENV).ok().as_deref(),
            multi_tenant,
        )
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Admin => "admin",
            Self::Disabled => "disabled",
        }
    }

    /// Only open registration is advertised; admin registration is an operator path.
    pub(crate) fn advertises_endpoint(self) -> bool {
        self == Self::Enabled
    }
}

/// Reject a declared `token_endpoint_auth_method` the server cannot honor.
pub(crate) fn validate_token_endpoint_auth_method(method: Option<&str>) -> Result<(), String> {
    match method {
        None => Ok(()),
        Some(value) if SUPPORTED_TOKEN_ENDPOINT_AUTH_METHODS.contains(&value) => Ok(()),
        Some(_) => Err(format!(
            "token_endpoint_auth_method must be one of: {}",
            SUPPORTED_TOKEN_ENDPOINT_AUTH_METHODS.join(", ")
        )),
    }
}

fn principal_has_admin_scope(principal: &AuthPrincipal) -> bool {
    let scope = match principal {
        AuthPrincipal::OAuthClient { scope, .. } | AuthPrincipal::ApiKey { scope, .. } => scope,
        AuthPrincipal::Anonymous => return false,
    };
    scope.split_whitespace().any(|value| value == "admin")
}

fn unauthorized() -> Response {
    let mut response = problem_response(
        StatusCode::UNAUTHORIZED,
        ProblemType::Unauthorized,
        ADMIN_REQUIRED_DETAIL.to_string(),
        None,
    );
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer realm=\"oauth-register\""),
    );
    response
}

/// Admission check for a registration request. `None` admits it; `Some` is the refusal.
pub(crate) async fn admit_registration(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    match state.oauth_registration {
        OAuthRegistrationMode::Enabled => None,
        OAuthRegistrationMode::Disabled => Some(problem_response(
            StatusCode::FORBIDDEN,
            ProblemType::Forbidden,
            DISABLED_DETAIL.to_string(),
            None,
        )),
        OAuthRegistrationMode::Admin => {
            let token = headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(token) = token else {
                return Some(unauthorized());
            };
            match validate_bearer_identity(state, token, None).await {
                Ok(identity) if principal_has_admin_scope(&identity.principal) => None,
                Ok(_) => Some(
                    problem_response(
                        StatusCode::FORBIDDEN,
                        ProblemType::Forbidden,
                        ADMIN_REQUIRED_DETAIL.to_string(),
                        None,
                    )
                    .into_response(),
                ),
                Err(_) => Some(unauthorized()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_preserve_community_and_tighten_hosted() {
        assert_eq!(
            OAuthRegistrationMode::from_value(None, false).unwrap(),
            OAuthRegistrationMode::Enabled
        );
        assert_eq!(
            OAuthRegistrationMode::from_value(Some(" "), true).unwrap(),
            OAuthRegistrationMode::Disabled
        );
    }

    #[test]
    fn parses_modes_and_rejects_unknown_values() {
        for (raw, mode) in [
            ("enabled", OAuthRegistrationMode::Enabled),
            ("admin", OAuthRegistrationMode::Admin),
            ("disabled", OAuthRegistrationMode::Disabled),
        ] {
            assert_eq!(
                OAuthRegistrationMode::from_value(Some(raw), false).unwrap(),
                mode
            );
            assert_eq!(mode.as_str(), raw);
        }
        assert!(OAuthRegistrationMode::from_value(Some("open"), false).is_err());
    }

    #[test]
    fn hosted_refuses_open_registration() {
        assert!(OAuthRegistrationMode::from_value(Some("enabled"), true).is_err());
        assert!(OAuthRegistrationMode::from_value(Some("admin"), true).is_ok());
    }

    #[test]
    fn only_open_registration_is_advertised() {
        assert!(OAuthRegistrationMode::Enabled.advertises_endpoint());
        assert!(!OAuthRegistrationMode::Admin.advertises_endpoint());
        assert!(!OAuthRegistrationMode::Disabled.advertises_endpoint());
    }

    #[test]
    fn token_endpoint_auth_method_must_be_supported() {
        assert!(validate_token_endpoint_auth_method(None).is_ok());
        assert!(validate_token_endpoint_auth_method(Some("client_secret_post")).is_ok());
        assert!(validate_token_endpoint_auth_method(Some("none")).is_err());
        assert!(validate_token_endpoint_auth_method(Some("private_key_jwt")).is_err());
    }

    #[test]
    fn admin_scope_is_required_for_admin_registration() {
        let admin = AuthPrincipal::ApiKey {
            key_id: uuid::Uuid::nil(),
            scope: "read admin".to_string(),
        };
        let reader = AuthPrincipal::OAuthClient {
            client_id: "client".to_string(),
            scope: "read write administrator".to_string(),
            user_id: None,
        };
        assert!(principal_has_admin_scope(&admin));
        assert!(!principal_has_admin_scope(&reader));
        assert!(!principal_has_admin_scope(&AuthPrincipal::Anonymous));
    }
}
