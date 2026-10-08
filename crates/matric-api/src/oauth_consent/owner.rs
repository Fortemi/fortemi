//! Resource-owner authentication for the consent ceremony (#943).
//!
//! An owner is either a user a trusted proxy already signed in, or the holder of a
//! Fortemi credential (API key or access token) entered on the consent page. The
//! owner's subject is recorded on the authorization code and the tokens issued from it.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use matric_core::AuthPrincipal;

use super::config::{AuthorizeConfig, OwnerAuthMethod};
use crate::trusted_proxy::TrustedProxyConfig;
use crate::{auth_principal_audit_id, validate_bearer_identity, AppState};

const MAX_SUBJECT_LEN: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    pub(crate) subject: String,
    /// `None`: the proxy's sign-in is the authorization decision; no scope ceiling.
    scopes: Option<Vec<String>>,
}

impl Owner {
    /// Whether the owner may grant every scope in `scope`.
    pub(crate) fn covers(&self, scope: &str) -> bool {
        let Some(held) = &self.scopes else {
            return true;
        };
        held.iter().any(|value| value == "admin")
            || scope
                .split_whitespace()
                .all(|wanted| held.iter().any(|value| value == wanted))
    }
}

/// The proxy-asserted owner, only when the immediate peer is a trusted proxy.
pub(crate) fn from_trusted_header(
    config: &AuthorizeConfig,
    proxies: &TrustedProxyConfig,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> Option<Owner> {
    if !config.allows(OwnerAuthMethod::TrustedHeader) || !proxies.trusts_peer(peer) {
        return None;
    }
    let value = headers.get(config.owner_header()?)?.to_str().ok()?.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_SUBJECT_LEN
        && !value.chars().any(char::is_control);
    valid.then(|| Owner {
        subject: format!("proxy:{value}"),
        scopes: None,
    })
}

/// The owner proven by a Fortemi credential typed into the consent page.
pub(crate) async fn from_credential(state: &AppState, credential: &str) -> Option<Owner> {
    let credential = credential.trim();
    if credential.is_empty() || !state.oauth_authorize.config.allows(OwnerAuthMethod::ApiKey) {
        return None;
    }
    let identity = validate_bearer_identity(state, credential).await.ok()?;
    owner_for_principal(&identity.principal)
}

fn owner_for_principal(principal: &AuthPrincipal) -> Option<Owner> {
    let scope = match principal {
        AuthPrincipal::OAuthClient { scope, .. } | AuthPrincipal::ApiKey { scope, .. } => scope,
        AuthPrincipal::Anonymous => return None,
    };
    Some(Owner {
        subject: auth_principal_audit_id(principal),
        scopes: Some(scope.split_whitespace().map(str::to_string).collect()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    fn headers(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert("x-forwarded-email", HeaderValue::from_str(value).unwrap());
        map
    }

    fn config() -> AuthorizeConfig {
        AuthorizeConfig::trusted_header(HeaderName::from_static("x-forwarded-email"))
    }

    #[test]
    fn header_is_only_honored_from_a_trusted_peer() {
        let proxies = TrustedProxyConfig::from_value(Some("10.0.0.0/8")).unwrap();
        let trusted: SocketAddr = "10.1.2.3:4000".parse().unwrap();
        let untrusted: SocketAddr = "192.0.2.10:4000".parse().unwrap();
        let owner =
            from_trusted_header(&config(), &proxies, Some(trusted), &headers("a@b.example"));
        assert_eq!(owner.unwrap().subject, "proxy:a@b.example");
        assert!(from_trusted_header(
            &config(),
            &proxies,
            Some(untrusted),
            &headers("a@b.example")
        )
        .is_none());
        assert!(from_trusted_header(&config(), &proxies, None, &headers("a@b.example")).is_none());
        assert!(
            from_trusted_header(&config(), &proxies, Some(trusted), &HeaderMap::new()).is_none()
        );
        let api_key_only = AuthorizeConfig::api_key_only();
        assert!(from_trusted_header(
            &api_key_only,
            &proxies,
            Some(trusted),
            &headers("a@b.example")
        )
        .is_none());
    }

    #[test]
    fn credential_owners_cannot_grant_more_than_they_hold() {
        let reader = owner_for_principal(&AuthPrincipal::ApiKey {
            key_id: uuid::Uuid::nil(),
            scope: "read write".to_string(),
        })
        .unwrap();
        assert!(reader.subject.starts_with("api_key:"));
        assert!(reader.covers("read"));
        assert!(!reader.covers("read mcp"));
        let admin = owner_for_principal(&AuthPrincipal::OAuthClient {
            client_id: "ops".to_string(),
            scope: "admin".to_string(),
            user_id: None,
        })
        .unwrap();
        assert!(admin.covers("read write mcp admin"));
        assert!(owner_for_principal(&AuthPrincipal::Anonymous).is_none());
    }
}
