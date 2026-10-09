//! Redirect URI validation for Fortemi's local OAuth authorization server.

use reqwest::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RedirectUriError {
    Parse,
    Unsupported,
    Wildcard,
    Userinfo,
    Fragment,
}

/// Validate a runtime authorization redirect against stored client metadata.
pub(crate) fn validate_redirect_uri(redirect_uri: &str, registered_uris: &[String]) -> bool {
    if registered_uris.iter().any(|value| value == redirect_uri) {
        return true;
    }
    let Ok(requested) = Url::parse(redirect_uri) else {
        return false;
    };
    if !is_loopback_redirect(&requested) {
        return false;
    }
    registered_uris.iter().any(|registered| {
        Url::parse(registered)
            .ok()
            .filter(is_loopback_redirect)
            .is_some_and(|registered| same_loopback_redirect_target(&requested, &registered))
    })
}

/// Validate redirect URI metadata accepted by dynamic client registration.
pub(crate) fn validate_registration_redirect_uri(raw: &str) -> Result<(), RedirectUriError> {
    if raw.contains('*') {
        return Err(RedirectUriError::Wildcard);
    }
    let url = Url::parse(raw).map_err(|_| RedirectUriError::Parse)?;
    if has_userinfo(&url) {
        return Err(RedirectUriError::Userinfo);
    }
    if url.fragment().is_some() || raw.contains('#') {
        return Err(RedirectUriError::Fragment);
    }
    if is_loopback_redirect(&url) || is_https_redirect(&url) || is_private_use_scheme(&url) {
        return Ok(());
    }
    Err(RedirectUriError::Unsupported)
}

fn same_loopback_redirect_target(left: &Url, right: &Url) -> bool {
    left.path() == right.path() && left.query() == right.query()
}

fn is_https_redirect(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some()
}

fn is_loopback_redirect(url: &Url) -> bool {
    url.scheme() == "http"
        && !has_userinfo(url)
        && url.fragment().is_none()
        && is_loopback_host(url.host_str())
}

fn has_userinfo(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "::1" | "[::1]"))
}

fn is_private_use_scheme(url: &Url) -> bool {
    let scheme = url.scheme();
    if matches!(scheme, "http" | "https") || !scheme.contains('.') {
        return false;
    }
    let labels: Vec<&str> = scheme.split('.').collect();
    labels.len() >= 3
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && label
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric())
                && label
                    .bytes()
                    .last()
                    .is_some_and(|b| b.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_redirect_exception_rejects_url_confusion() {
        let registered = vec!["http://localhost:3000/cb".to_string()];
        for candidate in [
            "http://localhost:1234@attacker.example/cb",
            "http://localhost.attacker.example:1/cb",
            "http://127.0.0.1:1/cb#x",
        ] {
            assert!(
                !validate_redirect_uri(candidate, &registered),
                "{candidate} must not match the registered loopback redirect"
            );
        }
    }

    #[test]
    fn loopback_redirect_exception_allows_only_port_variance() {
        let registered = vec![
            "http://localhost:3000/cb".to_string(),
            "http://[::1]:9000/callback?client=desktop".to_string(),
        ];
        assert!(validate_redirect_uri(
            "http://localhost:49152/cb",
            &registered
        ));
        assert!(validate_redirect_uri(
            "http://[::1]:1/callback?client=desktop",
            &registered
        ));
        assert!(!validate_redirect_uri(
            "http://localhost:49152/cb?x=1",
            &registered
        ));
        assert!(!validate_redirect_uri(
            "http://localhost:49152/other",
            &registered
        ));
        assert!(!validate_redirect_uri(
            "https://localhost:49152/cb",
            &registered
        ));
    }

    #[test]
    fn registration_redirects_allow_https_loopback_and_private_scheme_only() {
        for valid in [
            "https://client.example/cb",
            "https://client.example/cb?x=1",
            "http://localhost:3000/cb",
            "http://127.0.0.1:1/cb",
            "http://[::1]:49152/cb?x=1",
            "com.example.app:/cb",
        ] {
            assert!(
                validate_registration_redirect_uri(valid).is_ok(),
                "{valid} should be accepted"
            );
        }
        for invalid in [
            "http://attacker.example/cb",
            "http://localhost:1234@attacker.example/cb",
            "http://localhost.attacker.example:1/cb",
            "http://127.0.0.1:1/cb#x",
            "https://user@client.example/cb",
            "https://client.example/cb#x",
            "https://*.example/cb",
            "example:/cb",
        ] {
            assert!(
                validate_registration_redirect_uri(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }
}
