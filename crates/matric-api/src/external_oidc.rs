//! External OIDC resource-server mode for single-tenant deployments.

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::Context;
use fortemi_auth_core::{AuthContext, PrincipalKind};
use matric_core::AuthPrincipal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::admin_bootstrap::{bootstrap_tenant, BootstrapRequest};
use crate::hosted_claim_policy::{ClaimPolicyLoadOptions, MAPPABLE_SCOPES};

pub const AUTH_MODE_ENV: &str = "FORTEMI_AUTH_MODE";
pub const EXTERNAL_OIDC_MODE: &str = "external-oidc";
pub const DEFAULT_MAX_TOKEN_LIFETIME_SECONDS: i64 = 3600;
pub const MAX_TOKEN_LIFETIME_SECONDS: i64 = 86_400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMode {
    Local,
    HostedMultiTenant,
    ExternalOidc,
}

impl AuthMode {
    pub const fn external_oidc(self) -> bool {
        matches!(self, Self::ExternalOidc)
    }

    pub const fn external_jwt(self) -> bool {
        matches!(self, Self::HostedMultiTenant | Self::ExternalOidc)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalOidcConfig {
    pub audiences: BTreeSet<String>,
    pub default_tenant_id: Uuid,
    pub allow_multi_audience: bool,
    pub allow_token_scopes: bool,
    pub allow_legacy_tokens: bool,
    pub legacy_scope_ceiling: BTreeSet<String>,
    pub max_token_lifetime_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestPrincipal {
    pub iss: String,
    pub sub: String,
    pub azp: Option<String>,
    pub scopes: Vec<String>,
    pub credential_class: CredentialClass,
    pub kind: PrincipalKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialClass {
    Oidc,
    Legacy,
}

impl CredentialClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oidc => "oidc",
            Self::Legacy => "legacy",
        }
    }
}

impl RequestPrincipal {
    pub fn from_auth_context(issuer: &str, context: &AuthContext) -> Self {
        Self {
            iss: issuer.to_string(),
            sub: context.principal_id.clone(),
            azp: None,
            scopes: context.scopes.clone(),
            credential_class: CredentialClass::Oidc,
            kind: context.principal_kind,
        }
    }

    pub fn legacy(principal: &AuthPrincipal) -> Self {
        let (sub, scopes) = match principal {
            AuthPrincipal::OAuthClient {
                client_id,
                user_id,
                scope,
            } => (
                user_id.clone().unwrap_or_else(|| client_id.clone()),
                scope.split_whitespace().map(ToOwned::to_owned).collect(),
            ),
            AuthPrincipal::ApiKey { key_id, scope } => (
                key_id.to_string(),
                scope.split_whitespace().map(ToOwned::to_owned).collect(),
            ),
            AuthPrincipal::Anonymous => ("anonymous".to_string(), Vec::new()),
        };
        Self {
            iss: "fortemi:legacy".to_string(),
            sub,
            azp: None,
            scopes,
            credential_class: CredentialClass::Legacy,
            kind: PrincipalKind::Service,
        }
    }

    pub const fn kind_label(&self) -> &'static str {
        match self.kind {
            PrincipalKind::Human => "user",
            PrincipalKind::Service => "service",
        }
    }
}

/// Hook for #1191: resolve or provision `app_user` from `(iss, sub)` after full
/// token validation and claim-policy scope evaluation.
pub fn resolve_user_principal(issuer: &str, context: &AuthContext) -> RequestPrincipal {
    RequestPrincipal::from_auth_context(issuer, context)
}

pub fn auth_mode_from_env<F>(env: F, multi_tenant: bool) -> anyhow::Result<AuthMode>
where
    F: Fn(&str) -> Option<String>,
{
    match env(AUTH_MODE_ENV).as_deref().unwrap_or("") {
        "" if multi_tenant => Ok(AuthMode::HostedMultiTenant),
        "" => Ok(AuthMode::Local),
        EXTERNAL_OIDC_MODE if multi_tenant => anyhow::bail!(
            "{AUTH_MODE_ENV}=external-oidc is for single-tenant deployments; unset it when FORTEMI_MULTI_TENANT=true"
        ),
        EXTERNAL_OIDC_MODE => Ok(AuthMode::ExternalOidc),
        value => anyhow::bail!(
            "{AUTH_MODE_ENV} has invalid value '{value}'. Expected external-oidc or unset."
        ),
    }
}

pub fn config_from_env<F>(
    env: F,
    mode: AuthMode,
    multi_tenant: bool,
) -> anyhow::Result<Option<ExternalOidcConfig>>
where
    F: Fn(&str) -> Option<String>,
{
    if !mode.external_oidc() {
        if env("FORTEMI_AUTH_DEFAULT_TENANT").is_some() && multi_tenant {
            anyhow::bail!(
                "FORTEMI_AUTH_DEFAULT_TENANT is not allowed with FORTEMI_MULTI_TENANT=true"
            );
        }
        return Ok(None);
    }

    let audiences = parse_audiences(env("FORTEMI_AUTH_AUDIENCES"))?;
    let default_tenant_id = env("FORTEMI_AUTH_DEFAULT_TENANT")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("FORTEMI_AUTH_DEFAULT_TENANT is required in external OIDC mode")
        })
        .and_then(|value| {
            Uuid::parse_str(value.trim()).context("FORTEMI_AUTH_DEFAULT_TENANT must be a UUID")
        })?;
    let max_token_lifetime_seconds = parse_i64_bounded(
        "FORTEMI_AUTH_MAX_TOKEN_LIFETIME_SECONDS",
        env("FORTEMI_AUTH_MAX_TOKEN_LIFETIME_SECONDS"),
        DEFAULT_MAX_TOKEN_LIFETIME_SECONDS,
        1,
        MAX_TOKEN_LIFETIME_SECONDS,
    )?;
    let allow_multi_audience = parse_bool(
        "FORTEMI_AUTH_ALLOW_MULTI_AUDIENCE",
        env("FORTEMI_AUTH_ALLOW_MULTI_AUDIENCE"),
        false,
    )?;
    let allow_token_scopes = parse_bool(
        "FORTEMI_AUTH_ALLOW_TOKEN_SCOPES",
        env("FORTEMI_AUTH_ALLOW_TOKEN_SCOPES"),
        false,
    )?;
    let allow_legacy_tokens = parse_bool(
        "FORTEMI_AUTH_ALLOW_LEGACY_TOKENS",
        env("FORTEMI_AUTH_ALLOW_LEGACY_TOKENS"),
        false,
    )?;
    let legacy_scope_ceiling = parse_scope_set(
        env("FORTEMI_AUTH_LEGACY_SCOPE_CEILING")
            .unwrap_or_else(|| "read mcp".to_string())
            .split_whitespace(),
        "FORTEMI_AUTH_LEGACY_SCOPE_CEILING",
    )?;

    Ok(Some(ExternalOidcConfig {
        audiences,
        default_tenant_id,
        allow_multi_audience,
        allow_token_scopes,
        allow_legacy_tokens,
        legacy_scope_ceiling,
        max_token_lifetime_seconds,
    }))
}

pub fn claim_policy_options(config: Option<&ExternalOidcConfig>) -> ClaimPolicyLoadOptions<'_> {
    match config {
        Some(config) => ClaimPolicyLoadOptions {
            required: true,
            allow_token_scopes: config.allow_token_scopes,
            forbidden_audiences: &config.audiences,
        },
        None => ClaimPolicyLoadOptions::hosted_default(),
    }
}

pub async fn ensure_default_tenant(pool: &PgPool, tenant_id: Uuid) -> anyhow::Result<()> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM tenant_registry WHERE id = $1 AND status = 'active')",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .context("external OIDC default tenant lookup failed")?;
    if exists {
        return Ok(());
    }

    let slug = format!("external-{}", tenant_id.simple());
    let request = BootstrapRequest::new(
        &slug,
        Some(tenant_id),
        Some("External OIDC default tenant"),
        false,
    )
    .map_err(|error| anyhow::anyhow!("external OIDC default tenant bootstrap invalid: {error}"))?;
    bootstrap_tenant(pool, &request, false, "fortemi:tenant_id")
        .await
        .map_err(|error| {
            anyhow::anyhow!("external OIDC default tenant bootstrap failed: {error}")
        })?;
    Ok(())
}

pub fn ceil_legacy_scope(scope: &str, ceiling: &BTreeSet<String>) -> String {
    scope
        .split_whitespace()
        .filter(|scope| ceiling.contains(*scope))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn legacy_token_extension_lifetime(
    created_at: chrono::DateTime<chrono::Utc>,
    requested: chrono::Duration,
    max_lifetime_seconds: i64,
) -> Option<chrono::Duration> {
    let now = chrono::Utc::now();
    let absolute_expiry = created_at + chrono::Duration::seconds(max_lifetime_seconds);
    let remaining = absolute_expiry - now;
    if remaining <= chrono::Duration::zero() {
        None
    } else {
        Some(std::cmp::min(requested, remaining))
    }
}

pub fn jwks_grace_duration(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn parse_audiences(value: Option<String>) -> anyhow::Result<BTreeSet<String>> {
    let raw = value.ok_or_else(|| {
        anyhow::anyhow!("FORTEMI_AUTH_AUDIENCES is required in external OIDC mode")
    })?;
    let audiences: BTreeSet<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(validate_audience)
        .collect::<anyhow::Result<_>>()?;
    anyhow::ensure!(
        !audiences.is_empty(),
        "FORTEMI_AUTH_AUDIENCES must contain at least one https resource URI"
    );
    Ok(audiences)
}

fn validate_audience(value: &str) -> anyhow::Result<String> {
    let parsed = reqwest::Url::parse(value)
        .with_context(|| "FORTEMI_AUTH_AUDIENCES entries must be absolute https resource URIs")?;
    anyhow::ensure!(
        parsed.scheme() == "https"
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.fragment().is_none(),
        "FORTEMI_AUTH_AUDIENCES entries must be exact https resource URIs without userinfo or fragments"
    );
    Ok(value.trim_end_matches('/').to_string())
}

fn parse_scope_set<'a>(
    scopes: impl Iterator<Item = &'a str>,
    name: &str,
) -> anyhow::Result<BTreeSet<String>> {
    let scopes: BTreeSet<String> = scopes.map(ToOwned::to_owned).collect();
    anyhow::ensure!(!scopes.is_empty(), "{name} must contain at least one scope");
    for scope in &scopes {
        anyhow::ensure!(
            MAPPABLE_SCOPES.contains(&scope.as_str()),
            "{name} may only contain read, write, admin, and mcp"
        );
    }
    Ok(scopes)
}

fn parse_bool(name: &str, value: Option<String>, default: bool) -> anyhow::Result<bool> {
    match value.as_deref() {
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(value) => anyhow::bail!(
            "{name} has invalid boolean value '{value}'. Expected one of: true, false, 1, 0."
        ),
        None => Ok(default),
    }
}

fn parse_i64_bounded(
    name: &str,
    value: Option<String>,
    default: i64,
    min: i64,
    max: i64,
) -> anyhow::Result<i64> {
    let parsed = match value {
        Some(value) => value
            .parse::<i64>()
            .with_context(|| format!("{name} must be an integer"))?,
        None => default,
    };
    anyhow::ensure!(
        (min..=max).contains(&parsed),
        "{name} must be between {min} and {max}"
    );
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_mode_requires_https_audience_and_default_tenant() {
        let tenant = Uuid::new_v4();
        let config = config_from_env(
            |name| match name {
                "FORTEMI_AUTH_AUDIENCES" => {
                    Some("https://fortemi.example,https://fortemi.example/mcp".to_string())
                }
                "FORTEMI_AUTH_DEFAULT_TENANT" => Some(tenant.to_string()),
                _ => None,
            },
            AuthMode::ExternalOidc,
            false,
        )
        .unwrap()
        .unwrap();

        assert_eq!(config.default_tenant_id, tenant);
        assert!(config.audiences.contains("https://fortemi.example"));
        assert!(config.audiences.contains("https://fortemi.example/mcp"));
        assert_eq!(
            config.max_token_lifetime_seconds,
            DEFAULT_MAX_TOKEN_LIFETIME_SECONDS
        );
        assert_eq!(
            config.legacy_scope_ceiling,
            BTreeSet::from(["mcp".to_string(), "read".to_string()])
        );

        assert!(config_from_env(|_| None, AuthMode::ExternalOidc, false).is_err());
        assert!(config_from_env(
            |name| match name {
                "FORTEMI_AUTH_AUDIENCES" => Some("http://fortemi.example".to_string()),
                "FORTEMI_AUTH_DEFAULT_TENANT" => Some(tenant.to_string()),
                _ => None,
            },
            AuthMode::ExternalOidc,
            false,
        )
        .is_err());
    }

    #[test]
    fn legacy_scope_ceiling_intersects_scopes() {
        let ceiling = BTreeSet::from(["read".to_string(), "mcp".to_string()]);
        assert_eq!(
            ceil_legacy_scope("admin write read mcp", &ceiling),
            "read mcp"
        );
    }

    #[test]
    fn auth_mode_refuses_external_oidc_with_multi_tenant() {
        assert_eq!(
            auth_mode_from_env(|_| None, true).unwrap(),
            AuthMode::HostedMultiTenant
        );
        assert!(auth_mode_from_env(
            |name| (name == AUTH_MODE_ENV).then(|| EXTERNAL_OIDC_MODE.to_string()),
            true,
        )
        .is_err());
    }
}
