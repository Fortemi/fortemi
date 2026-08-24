//! Released OIDC authority integration for hosted Fortemi requests.

use std::sync::Arc;

use async_trait::async_trait;
use fortemi_auth_clerk::{ClerkConfig, ClerkProvider};
use fortemi_auth_core::{
    AuthContext, AuthError, OAuthProvider, TenantRecord, TenantStatus, TenantStore,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedAuthConfig {
    pub issuer: String,
    pub audience: String,
    pub tenant_claim_name: String,
    pub clock_skew_seconds: i64,
    pub jwks_cache_capacity: usize,
    pub http_timeout_seconds: u64,
}

impl HostedAuthConfig {
    pub fn from_env<F>(env: F) -> Result<Self, AuthError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let issuer = env("ISSUER_URL")
            .filter(|value| !value.trim().is_empty())
            .ok_or(AuthError::ConfigError)?;
        let audience = env("FORTEMI_AUTH_AUDIENCE")
            .filter(|value| !value.trim().is_empty())
            .ok_or(AuthError::ConfigError)?;
        let tenant_claim_name = env("FORTEMI_AUTH_TENANT_CLAIM")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "fortemi:tenant_id".to_string());
        let clock_skew_seconds =
            parse_bounded::<i64>(env("FORTEMI_AUTH_CLOCK_SKEW_SECONDS"), 60, 0, 60)?;
        let jwks_cache_capacity =
            parse_bounded::<usize>(env("FORTEMI_AUTH_JWKS_CACHE_CAPACITY"), 128, 1, 4096)?;
        let http_timeout_seconds =
            parse_bounded::<u64>(env("FORTEMI_AUTH_HTTP_TIMEOUT_SECONDS"), 5, 1, 30)?;

        let config = Self {
            issuer,
            audience,
            tenant_claim_name,
            clock_skew_seconds,
            jwks_cache_capacity,
            http_timeout_seconds,
        };
        ClerkConfig::from(&config).validate()?;
        Ok(config)
    }
}

fn parse_bounded<T>(
    value: Option<String>,
    default: T,
    minimum: T,
    maximum: T,
) -> Result<T, AuthError>
where
    T: Copy + Ord + std::str::FromStr,
{
    let parsed = match value {
        Some(value) => value.parse().map_err(|_| AuthError::ConfigError)?,
        None => default,
    };
    (minimum..=maximum)
        .contains(&parsed)
        .then_some(parsed)
        .ok_or(AuthError::ConfigError)
}

impl From<&HostedAuthConfig> for ClerkConfig {
    fn from(config: &HostedAuthConfig) -> Self {
        Self {
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            tenant_claim_name: config.tenant_claim_name.clone(),
            clock_skew_seconds: config.clock_skew_seconds,
            jwks_cache_capacity: config.jwks_cache_capacity,
            http_timeout_seconds: config.http_timeout_seconds,
        }
    }
}

#[derive(Clone)]
pub struct PgTenantStore {
    pool: PgPool,
}

impl PgTenantStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl TenantStore for PgTenantStore {
    async fn lookup(&self, tenant_id: Uuid) -> Result<Option<TenantRecord>, AuthError> {
        let row: Option<(Uuid, String)> =
            sqlx::query_as("SELECT id, status FROM tenant_registry WHERE id = $1")
                .bind(tenant_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| AuthError::InternalError)?;

        row.map(|(id, status)| {
            let status = match status.as_str() {
                "active" => TenantStatus::Active,
                "suspended" => TenantStatus::Suspended,
                "soft_deleted" => TenantStatus::SoftDeleted,
                _ => return Err(AuthError::InternalError),
            };
            Ok(TenantRecord { id, status })
        })
        .transpose()
    }
}

#[async_trait]
pub trait HostedAuthenticator: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<AuthContext, AuthError>;
}

#[async_trait]
impl<P> HostedAuthenticator for P
where
    P: OAuthProvider + Send + Sync,
{
    async fn authenticate(&self, token: &str) -> Result<AuthContext, AuthError> {
        OAuthProvider::authenticate(self, token).await
    }
}

pub fn build_clerk_authenticator(
    config: &HostedAuthConfig,
    pool: PgPool,
) -> Result<Arc<dyn HostedAuthenticator>, AuthError> {
    let provider = ClerkProvider::new(ClerkConfig::from(config), PgTenantStore::new(pool))?;
    Ok(Arc::new(provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_config_requires_issuer_and_audience_and_bounds_resources() {
        let config = HostedAuthConfig::from_env(|name| match name {
            "ISSUER_URL" => Some("https://issuer.example".to_string()),
            "FORTEMI_AUTH_AUDIENCE" => Some("fortemi-api".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.tenant_claim_name, "fortemi:tenant_id");
        assert_eq!(config.clock_skew_seconds, 60);

        assert!(HostedAuthConfig::from_env(|_| None).is_err());
        assert!(HostedAuthConfig::from_env(|name| match name {
            "ISSUER_URL" => Some("https://issuer.example".to_string()),
            "FORTEMI_AUTH_AUDIENCE" => Some("fortemi-api".to_string()),
            "FORTEMI_AUTH_CLOCK_SKEW_SECONDS" => Some("61".to_string()),
            _ => None,
        })
        .is_err());
    }
}
