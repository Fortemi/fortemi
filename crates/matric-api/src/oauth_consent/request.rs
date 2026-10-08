//! Validates an authorization request before any redirect is produced (#943).
//!
//! Client and redirect URI are checked first. Until both are trusted, every failure is
//! a local error page: Fortemi never redirects to a request-controlled URI (RFC 6749
//! §4.1.2.1). Later failures (response type, grant, scope, PKCE) go back to the
//! registered redirect URI with `state` and `iss`.

use axum::http::StatusCode;

use super::transaction::ValidatedRequest;
use crate::oauth_profile::is_allowed_oauth_scope;
use crate::{validate_redirect_uri, AuthorizationRequest, Database};

const MAX_STATE_LEN: usize = 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rejection {
    /// The client or redirect URI is not trusted: render locally, never redirect.
    Local(StatusCode, &'static str),
    /// The redirect URI is registered: report the error to the client.
    Redirect {
        redirect_uri: String,
        error: &'static str,
        description: &'static str,
        state: Option<String>,
    },
}

pub(crate) async fn validate(
    db: &Database,
    req: &AuthorizationRequest,
) -> Result<ValidatedRequest, Rejection> {
    let client = match db.oauth.get_client(&req.client_id).await {
        Ok(Some(client)) if client.is_active => client,
        Ok(_) => {
            return Err(Rejection::Local(
                StatusCode::BAD_REQUEST,
                "The client is unknown or inactive.",
            ))
        }
        Err(_) => {
            return Err(Rejection::Local(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The authorization server could not load the client.",
            ))
        }
    };
    if !validate_redirect_uri(&req.redirect_uri, &client.redirect_uris) {
        return Err(Rejection::Local(
            StatusCode::BAD_REQUEST,
            "The redirect_uri is not registered for this client.",
        ));
    }
    let reject = |error, description| Rejection::Redirect {
        redirect_uri: req.redirect_uri.clone(),
        error,
        description,
        state: req.state.clone(),
    };
    if req
        .state
        .as_deref()
        .is_some_and(|state| state.len() > MAX_STATE_LEN)
    {
        return Err(reject("invalid_request", "state is too long"));
    }
    if req.response_type != "code" {
        return Err(reject(
            "unsupported_response_type",
            "Only the code response type is supported",
        ));
    }
    if !client
        .grant_types
        .iter()
        .any(|grant| grant == "authorization_code")
    {
        return Err(reject(
            "unauthorized_client",
            "The client is not registered for the authorization_code grant",
        ));
    }
    let scope = grantable_scope(req.scope.as_deref(), &client.scope).ok_or_else(|| {
        reject(
            "invalid_scope",
            "The requested scope is not allowed for this client",
        )
    })?;
    validate_pkce(
        req.code_challenge.as_deref(),
        req.code_challenge_method.as_deref(),
    )
    .map_err(|description| reject("invalid_request", description))?;
    Ok(ValidatedRequest {
        client_id: client.client_id,
        client_name: client.client_name,
        redirect_uri: req.redirect_uri.clone(),
        scope,
        state: req.state.clone(),
        code_challenge: req.code_challenge.clone(),
        code_challenge_method: req.code_challenge_method.clone(),
    })
}

/// The requested scopes, each issued by this server and registered for the client.
/// No request means the client's registered scope.
pub(crate) fn grantable_scope(requested: Option<&str>, registered: &str) -> Option<String> {
    let registered: Vec<&str> = registered.split_whitespace().collect();
    let requested = requested
        .map(str::split_whitespace)
        .map(Iterator::collect::<Vec<_>>)
        .unwrap_or_else(|| registered.clone());
    let mut granted: Vec<&str> = Vec::new();
    for scope in requested {
        if !is_allowed_oauth_scope(scope) || !registered.contains(&scope) {
            return None;
        }
        if !granted.contains(&scope) {
            granted.push(scope);
        }
    }
    (!granted.is_empty()).then(|| granted.join(" "))
}

/// PKCE is optional here (#941 owns mandatory PKCE), but when present it must be S256
/// with an RFC 7636 challenge. `plain` is never accepted.
pub(crate) fn validate_pkce(
    challenge: Option<&str>,
    method: Option<&str>,
) -> Result<(), &'static str> {
    match (challenge, method) {
        (None, None) => Ok(()),
        (Some(challenge), Some("S256")) if is_pkce_challenge(challenge) => Ok(()),
        (Some(_), Some("S256")) => Err("code_challenge is malformed"),
        _ => Err("PKCE requires code_challenge with code_challenge_method=S256"),
    }
}

fn is_pkce_challenge(value: &str) -> bool {
    (43..=128).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}

/// Append query parameters to a registered redirect URI.
pub(crate) fn redirect_with(redirect_uri: &str, params: &[(&str, &str)]) -> String {
    let mut url = redirect_uri.to_string();
    let mut separator = if redirect_uri.contains('?') { '&' } else { '?' };
    for (name, value) in params {
        url.push(separator);
        url.push_str(name);
        url.push('=');
        url.push_str(&urlencoding::encode(value));
        separator = '&';
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_must_be_registered_and_issued() {
        assert_eq!(
            grantable_scope(None, "read write").as_deref(),
            Some("read write")
        );
        assert_eq!(
            grantable_scope(Some("read read"), "read write").as_deref(),
            Some("read")
        );
        assert_eq!(grantable_scope(Some("admin"), "read write"), None);
        assert_eq!(grantable_scope(Some("delete"), "read delete"), None);
        assert_eq!(grantable_scope(Some(""), "read"), None);
    }

    #[test]
    fn pkce_accepts_only_s256() {
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(validate_pkce(None, None).is_ok());
        assert!(validate_pkce(Some(challenge), Some("S256")).is_ok());
        assert!(validate_pkce(Some(challenge), Some("plain")).is_err());
        assert!(validate_pkce(Some(challenge), None).is_err());
        assert!(validate_pkce(None, Some("S256")).is_err());
        assert!(validate_pkce(Some("short"), Some("S256")).is_err());
    }

    #[test]
    fn redirect_parameters_are_encoded() {
        assert_eq!(
            redirect_with("https://c.example/cb", &[("code", "a b"), ("state", "x&y")]),
            "https://c.example/cb?code=a%20b&state=x%26y"
        );
        assert_eq!(
            redirect_with("https://c.example/cb?v=1", &[("error", "access_denied")]),
            "https://c.example/cb?v=1&error=access_denied"
        );
    }
}
