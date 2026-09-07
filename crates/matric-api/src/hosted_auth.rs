//! Released OIDC authority integration for hosted Fortemi requests.

use std::sync::Arc;

use anyhow::Context;
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
    /// Optional PEM trust roots, read once when the verifier is initialized.
    pub ca_bundle_path: Option<String>,
}

impl HostedAuthConfig {
    /// Read process configuration without treating an invalid CA path as unset.
    pub fn from_process_env() -> anyhow::Result<Self> {
        let ca_bundle_path = ca_bundle_env_value(std::env::var("FORTEMI_AUTH_CA_BUNDLE"))?;
        Self::from_env(|name| {
            if name == "FORTEMI_AUTH_CA_BUNDLE" {
                ca_bundle_path.clone()
            } else {
                std::env::var(name).ok()
            }
        })
        .context("hosted OIDC configuration is invalid")
    }

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
            ca_bundle_path: env("FORTEMI_AUTH_CA_BUNDLE"),
        };
        ClerkConfig::from(&config).validate()?;
        Ok(config)
    }
}

fn ca_bundle_env_value(
    value: Result<String, std::env::VarError>,
) -> anyhow::Result<Option<String>> {
    match value {
        Ok(path) => Ok(Some(path)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("FORTEMI_AUTH_CA_BUNDLE must contain a valid Unicode file path")
        }
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
                .map_err(|_| AuthError::TenantStoreUnavailable)?;

        row.map(|(id, status)| tenant_record_from_row(id, &status))
            .transpose()
    }
}

fn tenant_record_from_row(id: Uuid, status: &str) -> Result<TenantRecord, AuthError> {
    let status = match status {
        "active" => TenantStatus::Active,
        "suspended" => TenantStatus::Suspended,
        "soft_deleted" => TenantStatus::SoftDeleted,
        _ => return Err(AuthError::TenantStoreUnavailable),
    };
    Ok(TenantRecord { id, status })
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
) -> anyhow::Result<Arc<dyn HostedAuthenticator>> {
    let ca_bundle = load_ca_bundle(config.ca_bundle_path.as_deref())?;
    let provider = match ca_bundle {
        Some(bytes) => ClerkProvider::new_with_ca_bundle(
            ClerkConfig::from(config),
            PgTenantStore::new(pool),
            &bytes,
        )
        .context("FORTEMI_AUTH_CA_BUNDLE could not initialize OIDC TLS trust; provide a valid PEM certificate bundle")?,
        None => ClerkProvider::new(ClerkConfig::from(config), PgTenantStore::new(pool))
            .context("hosted OIDC verifier initialization failed")?,
    };
    Ok(Arc::new(provider))
}

/// An explicit setting must never silently fall back to the default trust store.
/// Avoid including paths or certificate contents in errors and logs.
fn load_ca_bundle(path: Option<&str>) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(path) = path else {
        return Ok(None);
    };
    anyhow::ensure!(
        !path.trim().is_empty(),
        "FORTEMI_AUTH_CA_BUNDLE must name a nonempty PEM certificate file"
    );
    let bytes = std::fs::read(path).context("FORTEMI_AUTH_CA_BUNDLE could not be read; mount a PEM certificate file readable by the server user")?;
    anyhow::ensure!(!bytes.is_empty(), "FORTEMI_AUTH_CA_BUNDLE file is empty");
    Ok(Some(bytes))
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

    #[test]
    fn ca_bundle_environment_rejects_non_unicode_instead_of_defaulting() {
        assert_eq!(
            ca_bundle_env_value(Err(std::env::VarError::NotPresent)).unwrap(),
            None
        );
        assert_eq!(ca_bundle_env_value(Ok("".into())).unwrap(), Some("".into()));
        assert_eq!(
            ca_bundle_env_value(Ok("ca.pem".into())).unwrap(),
            Some("ca.pem".into())
        );
        let error = ca_bundle_env_value(Err(std::env::VarError::NotUnicode(
            std::ffi::OsString::from("redacted-path"),
        )))
        .unwrap_err();
        assert!(error.to_string().contains("FORTEMI_AUTH_CA_BUNDLE"));
        assert!(!error.to_string().contains("redacted-path"));
    }

    #[test]
    fn ca_bundle_loading_distinguishes_absent_explicit_invalid_and_read_once() {
        assert_eq!(load_ca_bundle(None).unwrap(), None);
        for path in ["", " ", "\t"] {
            assert!(load_ca_bundle(Some(path))
                .unwrap_err()
                .to_string()
                .contains("FORTEMI_AUTH_CA_BUNDLE"));
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ca.pem");
        assert!(load_ca_bundle(Some(path.to_str().unwrap())).is_err());
        assert!(load_ca_bundle(Some(directory.path().to_str().unwrap())).is_err());
        std::fs::write(&path, []).unwrap();
        assert!(load_ca_bundle(Some(path.to_str().unwrap()))
            .unwrap_err()
            .to_string()
            .contains("empty"));
        std::fs::write(&path, b"first bundle bytes").unwrap();
        let loaded = load_ca_bundle(Some(path.to_str().unwrap()))
            .unwrap()
            .unwrap();
        std::fs::write(&path, b"replacement bytes").unwrap();
        assert_eq!(loaded, b"first bundle bytes");
    }

    #[tokio::test]
    async fn configured_invalid_ca_bundle_fails_verifier_initialization() {
        let mut config = HostedAuthConfig::from_env(|name| match name {
            "ISSUER_URL" => Some("https://issuer.example".to_string()),
            "FORTEMI_AUTH_AUDIENCE" => Some("fortemi-api".to_string()),
            _ => None,
        })
        .unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@localhost/unused")
            .unwrap();
        assert!(build_clerk_authenticator(&config, pool.clone()).is_ok());
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"not a certificate").unwrap();
        config.ca_bundle_path = Some(file.path().to_str().unwrap().to_owned());
        let error = build_clerk_authenticator(&config, pool.clone())
            .err()
            .unwrap();
        assert!(error.to_string().contains("FORTEMI_AUTH_CA_BUNDLE"));
        assert!(!error.to_string().contains(file.path().to_str().unwrap()));
        std::fs::write(
            file.path(),
            include_bytes!("../../../ci/trust/integro-labs-root-ca-g2.crt"),
        )
        .unwrap();
        assert!(build_clerk_authenticator(&config, pool).is_ok());
    }

    #[test]
    fn tenant_store_rows_preserve_state_and_fail_closed_on_malformed_status() {
        let id = Uuid::new_v4();
        assert_eq!(
            tenant_record_from_row(id, "active").unwrap(),
            TenantRecord {
                id,
                status: TenantStatus::Active,
            }
        );
        assert_eq!(
            tenant_record_from_row(id, "suspended").unwrap(),
            TenantRecord {
                id,
                status: TenantStatus::Suspended,
            }
        );
        assert_eq!(
            tenant_record_from_row(id, "soft_deleted").unwrap(),
            TenantRecord {
                id,
                status: TenantStatus::SoftDeleted,
            }
        );
        assert_eq!(
            tenant_record_from_row(id, "unexpected").unwrap_err(),
            AuthError::TenantStoreUnavailable
        );
    }
}
