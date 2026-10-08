//! How `/oauth/authorize` authenticates the resource owner (#943).
//!
//! `FORTEMI_OAUTH_AUTHORIZE_OWNER_AUTH` is a comma list of methods, `none` or `disabled`:
//! - `none`: approve-only consent with no resource-owner authentication (the pre-#943
//!   behavior). Community default, kept for compatibility and logged as a warning at
//!   startup. Redirect validation, CSRF binding and framing protection still apply.
//! - `api_key`: the owner enters a Fortemi API key or access token on the consent page.
//!   It must hold every requested scope (`admin` holds all of them).
//! - `trusted_header`: a reverse proxy (for example oauth2-proxy) that already signed the
//!   user in passes the user in `FORTEMI_OAUTH_OWNER_HEADER`. The header is honored only
//!   from peers in `FORTEMI_TRUSTED_PROXY_CIDRS`.
//! - `disabled`: no browser authorization; requests get `access_denied`.
//!
//! Community deployments default to `none`; operators opt in to `api_key` or
//! `trusted_header`. Hosted (`FORTEMI_MULTI_TENANT=true`) only accepts `disabled`: Fortemi-issued tokens are not admitted there, so an
//! authorization code would carry no usable tenant context.

use axum::http::HeaderName;

pub(crate) const OWNER_AUTH_ENV: &str = "FORTEMI_OAUTH_AUTHORIZE_OWNER_AUTH";
pub(crate) const OWNER_HEADER_ENV: &str = "FORTEMI_OAUTH_OWNER_HEADER";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerAuthMethod {
    ApiKey,
    TrustedHeader,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthorizeConfig {
    methods: Vec<OwnerAuthMethod>,
    owner_header: Option<HeaderName>,
    /// `none`: approval needs no authenticated owner (community compatibility default).
    unauthenticated: bool,
}

impl AuthorizeConfig {
    /// Community default: approve-only consent, no owner authentication.
    pub(crate) fn approve_only() -> Self {
        Self {
            methods: Vec::new(),
            owner_header: None,
            unauthenticated: true,
        }
    }

    /// Owners sign in with a Fortemi credential.
    #[cfg(test)]
    pub(crate) fn api_key_only() -> Self {
        Self {
            methods: vec![OwnerAuthMethod::ApiKey],
            owner_header: None,
            unauthenticated: false,
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            methods: Vec::new(),
            owner_header: None,
            unauthenticated: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn trusted_header(header: HeaderName) -> Self {
        Self {
            methods: vec![OwnerAuthMethod::TrustedHeader],
            owner_header: Some(header),
            unauthenticated: false,
        }
    }

    pub(crate) fn allows(&self, method: OwnerAuthMethod) -> bool {
        self.methods.contains(&method)
    }

    pub(crate) fn is_disabled(&self) -> bool {
        self.methods.is_empty() && !self.unauthenticated
    }

    /// Whether approval is allowed without an authenticated resource owner.
    pub(crate) fn is_unauthenticated(&self) -> bool {
        self.unauthenticated
    }

    pub(crate) fn owner_header(&self) -> Option<&HeaderName> {
        self.owner_header.as_ref()
    }

    pub(crate) fn describe(&self) -> String {
        if self.is_disabled() {
            return "disabled".to_string();
        }
        if self.unauthenticated {
            return "none".to_string();
        }
        self.methods
            .iter()
            .map(|method| match method {
                OwnerAuthMethod::ApiKey => "api_key",
                OwnerAuthMethod::TrustedHeader => "trusted_header",
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub(crate) fn from_env(multi_tenant: bool, trusted_proxies: usize) -> anyhow::Result<Self> {
        Self::from_values(
            std::env::var(OWNER_AUTH_ENV).ok().as_deref(),
            std::env::var(OWNER_HEADER_ENV).ok().as_deref(),
            multi_tenant,
            trusted_proxies,
        )
    }

    pub(crate) fn from_values(
        methods: Option<&str>,
        header: Option<&str>,
        multi_tenant: bool,
        trusted_proxies: usize,
    ) -> anyhow::Result<Self> {
        let methods = methods.map(str::trim).filter(|value| !value.is_empty());
        let config = match methods {
            None if multi_tenant => Self::disabled(),
            None => Self::approve_only(),
            Some(raw) => parse_methods(raw, header)?,
        };
        if multi_tenant && !config.is_disabled() {
            anyhow::bail!(
                "{OWNER_AUTH_ENV} must be disabled with FORTEMI_MULTI_TENANT=true; hosted \
                 deployments authorize through the external identity provider"
            );
        }
        if config.allows(OwnerAuthMethod::TrustedHeader) && trusted_proxies == 0 {
            anyhow::bail!(
                "{OWNER_AUTH_ENV}=trusted_header requires FORTEMI_TRUSTED_PROXY_CIDRS so the \
                 owner header is only accepted from the signing-in proxy"
            );
        }
        Ok(config)
    }
}

fn parse_methods(raw: &str, header: Option<&str>) -> anyhow::Result<AuthorizeConfig> {
    if raw == "disabled" {
        return Ok(AuthorizeConfig::disabled());
    }
    if raw == "none" {
        return Ok(AuthorizeConfig::approve_only());
    }
    let mut methods = Vec::new();
    for item in raw.split(',').map(str::trim) {
        let method = match item {
            "api_key" => OwnerAuthMethod::ApiKey,
            "trusted_header" => OwnerAuthMethod::TrustedHeader,
            _ => anyhow::bail!(
                "{OWNER_AUTH_ENV} accepts api_key, trusted_header (comma separated), none or disabled"
            ),
        };
        if methods.contains(&method) {
            anyhow::bail!("{OWNER_AUTH_ENV} lists a method twice");
        }
        methods.push(method);
    }
    let owner_header = if methods.contains(&OwnerAuthMethod::TrustedHeader) {
        let raw = header.map(str::trim).filter(|value| !value.is_empty());
        let raw = raw.ok_or_else(|| {
            anyhow::anyhow!("{OWNER_AUTH_ENV}=trusted_header requires {OWNER_HEADER_ENV}")
        })?;
        Some(
            HeaderName::from_bytes(raw.as_bytes())
                .map_err(|_| anyhow::anyhow!("{OWNER_HEADER_ENV} is not a valid header name"))?,
        )
    } else {
        None
    };
    Ok(AuthorizeConfig {
        methods,
        owner_header,
        unauthenticated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_deployment_mode() {
        let community = AuthorizeConfig::from_values(None, None, false, 0).unwrap();
        assert_eq!(community, AuthorizeConfig::approve_only());
        assert!(community.is_unauthenticated() && !community.is_disabled());
        assert_eq!(community.describe(), "none");
        assert_eq!(
            AuthorizeConfig::from_values(Some("api_key"), None, false, 0).unwrap(),
            AuthorizeConfig::api_key_only()
        );
        assert!(AuthorizeConfig::from_values(None, None, true, 0)
            .unwrap()
            .is_disabled());
    }

    #[test]
    fn hosted_mode_only_accepts_disabled() {
        assert!(AuthorizeConfig::from_values(Some("api_key"), None, true, 0).is_err());
        assert!(AuthorizeConfig::from_values(Some("none"), None, true, 0).is_err());
        assert!(AuthorizeConfig::from_values(Some("disabled"), None, true, 0).is_ok());
    }

    #[test]
    fn trusted_header_needs_a_header_and_trusted_proxies() {
        assert!(AuthorizeConfig::from_values(Some("trusted_header"), None, false, 1).is_err());
        assert!(AuthorizeConfig::from_values(
            Some("trusted_header"),
            Some("X-Forwarded-Email"),
            false,
            0
        )
        .is_err());
        let config = AuthorizeConfig::from_values(
            Some("trusted_header,api_key"),
            Some("X-Forwarded-Email"),
            false,
            1,
        )
        .unwrap();
        assert!(config.allows(OwnerAuthMethod::TrustedHeader));
        assert!(config.allows(OwnerAuthMethod::ApiKey));
        assert_eq!(config.owner_header().unwrap().as_str(), "x-forwarded-email");
        assert_eq!(config.describe(), "trusted_header,api_key");
    }

    #[test]
    fn rejects_unknown_or_repeated_methods() {
        assert!(AuthorizeConfig::from_values(Some("password"), None, false, 0).is_err());
        assert!(AuthorizeConfig::from_values(Some("api_key,api_key"), None, false, 0).is_err());
        assert!(
            AuthorizeConfig::from_values(Some("trusted_header"), Some("bad header"), false, 1)
                .is_err()
        );
    }
}
