//! Hosted provider selection. Explicit selections never fall back to another backend.
use std::sync::Arc;
use std::time::Instant;

use matric_api::services::key_provider_health::{parse_canary_interval, KeyProviderHealth};
use matric_api::services::metered_key_provider::{
    run_key_provider_canary, CallOutcome, MeteredKeyProvider,
};
use matric_crypto::KeyProvider;

#[derive(Debug, PartialEq, Eq)]
enum ProviderSelection {
    Aws,
    Vault,
}

impl ProviderSelection {
    fn parse(value: Option<&str>) -> anyhow::Result<Self> {
        match value {
            // Preserve existing configured AWS deployments during this additive rollout.
            None | Some("aws-kms") => Ok(Self::Aws),
            Some("vault-transit") => Ok(Self::Vault),
            Some("env") => anyhow::bail!("local key providers are forbidden in hosted mode"),
            _ => anyhow::bail!("FORTEMI_KEY_PROVIDER is unsupported by this build"),
        }
    }
}

/// Hosted key custody: the metered provider plus its cached health signal.
pub struct HostedKeyCustody {
    pub provider: Arc<dyn KeyProvider>,
    pub health: Arc<KeyProviderHealth>,
}

impl HostedKeyCustody {
    /// Spawn the low-rate health canary (#1170); a no-op when disabled.
    pub fn spawn_canary(&self) -> anyhow::Result<()> {
        if self.health.canary_interval().is_none() {
            return Ok(());
        }
        let context = canary_context()?;
        tokio::spawn(run_key_provider_canary(
            self.provider.clone(),
            self.health.clone(),
            context,
        ));
        Ok(())
    }
}

/// Fixed, production-shaped canary context. The current hosted secret consumer
/// uses this purpose; a new purpose must add its own canary before it can use a
/// separately provisioned Transit key. These synthetic identifiers are never
/// reported in health state or metric labels.
fn canary_context() -> anyhow::Result<matric_crypto::KeyContext> {
    Ok(matric_crypto::KeyContext::new(
        matric_crypto::KeyPurpose::USER_SECRET,
        "hosted_startup_canary",
    )?
    .with_tenant_id("00000000-0000-0000-0000-000000000001")?
    .with_user_id("00000000-0000-0000-0000-000000000002")?
    .with_resource_id("kms_health")?)
}

fn canary_interval_from_env() -> anyhow::Result<Option<std::time::Duration>> {
    let value = match std::env::var("FORTEMI_KMS_HEALTH_CANARY_SECS") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => anyhow::bail!("FORTEMI_KMS_HEALTH_CANARY_SECS is invalid"),
    };
    parse_canary_interval(value.as_deref()).map_err(|message| anyhow::anyhow!(message))
}

pub async fn provider_for_mode(multi_tenant: bool) -> anyhow::Result<Option<HostedKeyCustody>> {
    if !multi_tenant {
        return Ok(None);
    }
    let canary_interval = canary_interval_from_env()?;
    let selection =
        std::env::var("FORTEMI_KEY_PROVIDER")
            .map(Some)
            .or_else(|error| match error {
                std::env::VarError::NotPresent => Ok(None),
                _ => Err(anyhow::anyhow!("FORTEMI_KEY_PROVIDER is invalid")),
            })?;
    let provider = match ProviderSelection::parse(selection.as_deref())? {
        ProviderSelection::Aws => aws_provider().await?,
        ProviderSelection::Vault => vault_provider()?,
    };
    let context = canary_context()?;
    let started = Instant::now();
    let result = provider.health_check(&context).await;
    let outcome = match &result {
        Ok(status) => CallOutcome::from_health(*status),
        Err(error) => CallOutcome::from_error(error),
    };
    matric_core::telemetry::record_kms_call(outcome.to_call("startup_canary", started.elapsed()));
    if outcome != CallOutcome::Ok {
        anyhow::bail!("hosted KMS generate/decrypt startup check failed");
    }
    let health = Arc::new(KeyProviderHealth::new(canary_interval, Instant::now()));
    Ok(Some(HostedKeyCustody {
        provider: Arc::new(MeteredKeyProvider::new(provider, health.clone())),
        health,
    }))
}

#[cfg(feature = "kms-vault")]
fn vault_provider() -> anyhow::Result<Arc<dyn KeyProvider>> {
    let config = matric_crypto::VaultTransitConfig::from_env()
        .map_err(|_| anyhow::anyhow!("OpenBao Transit configuration is invalid"))?;
    let provider = matric_crypto::VaultTransitProvider::new(config)
        .map_err(|_| anyhow::anyhow!("OpenBao Transit client initialization failed"))?;
    Ok(Arc::new(provider))
}

#[cfg(not(feature = "kms-vault"))]
fn vault_provider() -> anyhow::Result<Arc<dyn KeyProvider>> {
    anyhow::bail!("vault-transit requires a server compiled with the kms-vault feature")
}

#[cfg(feature = "kms-aws")]
async fn aws_provider() -> anyhow::Result<Arc<dyn KeyProvider>> {
    let key_id = std::env::var("FORTEMI_AWS_KMS_KEY_ID")
        .map_err(|_| anyhow::anyhow!("aws-kms requires FORTEMI_AWS_KMS_KEY_ID"))?;
    // The existing AWS provider supports one configured key with context binding.
    // Previously these canonical settings were ignored; reject unsupported choices.
    for (name, expected) in [
        ("FORTEMI_KEY_STRATEGY", "shared-with-context"),
        ("FORTEMI_KEY_CONTEXT_VERSION", "1"),
    ] {
        match std::env::var(name) {
            Err(std::env::VarError::NotPresent) => {}
            Ok(value) if value == expected => {}
            _ => anyhow::bail!("AWS KMS supports shared-with-context and context version 1"),
        }
    }
    let provider = matric_crypto::AwsKmsProvider::from_environment(key_id)
        .await
        .map_err(|_| anyhow::anyhow!("AWS KMS provider configuration is invalid"))?;
    Ok(Arc::new(provider))
}

#[cfg(not(feature = "kms-aws"))]
async fn aws_provider() -> anyhow::Result<Arc<dyn KeyProvider>> {
    anyhow::bail!("aws-kms requires a server compiled with the kms-aws feature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_selection_never_falls_back() {
        assert_eq!(
            ProviderSelection::parse(None).unwrap(),
            ProviderSelection::Aws
        );
        assert_eq!(
            ProviderSelection::parse(Some("vault-transit")).unwrap(),
            ProviderSelection::Vault
        );
        assert_eq!(
            ProviderSelection::parse(Some("aws-kms")).unwrap(),
            ProviderSelection::Aws
        );
        for value in ["env", "gcp-kms", "", "vault", "VAULT-TRANSIT"] {
            assert!(ProviderSelection::parse(Some(value)).is_err());
        }
    }

    #[tokio::test]
    async fn personal_mode_does_not_access_external_kms() {
        assert!(provider_for_mode(false).await.unwrap().is_none());
    }

    #[cfg(not(feature = "kms-vault"))]
    #[test]
    fn missing_vault_feature_fails_closed() {
        assert!(vault_provider().is_err());
    }

    #[cfg(not(feature = "kms-aws"))]
    #[tokio::test]
    async fn missing_aws_feature_fails_closed() {
        assert!(aws_provider().await.is_err());
    }
}
