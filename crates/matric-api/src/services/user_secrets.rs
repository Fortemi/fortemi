//! Hosted user-secret envelope boundary shared by storage and proxy handlers.

use std::fmt;

use matric_crypto::{
    decrypt_blob, encrypt_blob, EncryptedBlob, KeyContext, KeyError, KeyFailureClass, KeyOperation,
    KeyProvider, KeyPurpose,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const STORED_SECRET_PAYLOAD_VERSION: u16 = 1;
const MAX_SECRET_BYTES: usize = 8192;
const MAX_NAME_CHARS: usize = 100;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum UserSecretServiceError {
    #[error("provider is not allowed for stored credentials")]
    InvalidProvider,
    #[error("secret name is invalid")]
    InvalidName,
    #[error("provider credential is invalid")]
    InvalidSecret,
    #[error("stored credential context is invalid")]
    InvalidContext,
    #[error("stored credential envelope is invalid")]
    InvalidEnvelope,
    #[error("stored credential key operation failed")]
    KeyOperation {
        operation: KeyOperation,
        class: KeyFailureClass,
        retryable: bool,
    },
}

impl From<KeyError> for UserSecretServiceError {
    fn from(error: KeyError) -> Self {
        Self::KeyOperation {
            operation: error.operation(),
            class: error.class(),
            retryable: error.is_retryable(),
        }
    }
}

pub struct SealedUserSecret {
    pub encrypted_blob: Value,
    pub masked: String,
}

impl fmt::Debug for SealedUserSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedUserSecret")
            .field("encrypted_blob", &"[REDACTED]")
            .field("masked_len", &self.masked.chars().count())
            .finish()
    }
}

#[derive(Serialize)]
struct StoredSecretPayloadRef<'a> {
    version: u16,
    provider: &'a str,
    key: &'a str,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct StoredSecretPayload {
    version: u16,
    provider: String,
    key: String,
}

pub fn normalize_user_secret_provider(provider: &str) -> Result<String, UserSecretServiceError> {
    let provider = provider.trim().to_ascii_lowercase();
    let profile = matric_inference::lookup_provider_profile(&provider)
        .ok_or(UserSecretServiceError::InvalidProvider)?;
    if profile.env.api_key.is_none() {
        return Err(UserSecretServiceError::InvalidProvider);
    }
    Ok(provider)
}

pub fn normalize_user_secret_name(name: &str) -> Result<String, UserSecretServiceError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(UserSecretServiceError::InvalidName);
    }
    Ok(name.to_string())
}

pub fn validate_user_secret_value(secret: &str) -> Result<(), UserSecretServiceError> {
    if secret.is_empty()
        || secret.len() > MAX_SECRET_BYTES
        || !secret.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(UserSecretServiceError::InvalidSecret);
    }
    Ok(())
}

pub fn user_secret_context(
    tenant_id: Uuid,
    user_id: &str,
    secret_id: Uuid,
) -> Result<KeyContext, UserSecretServiceError> {
    if tenant_id.is_nil() || secret_id.is_nil() {
        return Err(UserSecretServiceError::InvalidContext);
    }
    KeyContext::new(KeyPurpose::USER_SECRET, "user_secrets")
        .and_then(|context| context.with_tenant_id(tenant_id.to_string()))
        .and_then(|context| context.with_user_id(user_id))
        .and_then(|context| context.with_resource_id(secret_id.to_string()))
        .map_err(|_| UserSecretServiceError::InvalidContext)
}

pub async fn seal_user_secret(
    key_provider: &dyn KeyProvider,
    tenant_id: Uuid,
    user_id: &str,
    secret_id: Uuid,
    provider: &str,
    secret: &str,
) -> Result<SealedUserSecret, UserSecretServiceError> {
    let provider = normalize_user_secret_provider(provider)?;
    validate_user_secret_value(secret)?;
    let context = user_secret_context(tenant_id, user_id, secret_id)?;
    let payload = StoredSecretPayloadRef {
        version: STORED_SECRET_PAYLOAD_VERSION,
        provider: &provider,
        key: secret,
    };
    let encoded = serde_json::to_vec(&payload)
        .map(Zeroizing::new)
        .map_err(|_| UserSecretServiceError::InvalidEnvelope)?;
    let encrypted = encrypt_blob(key_provider, encoded.as_slice(), &context).await?;
    let encrypted_blob =
        serde_json::to_value(encrypted).map_err(|_| UserSecretServiceError::InvalidEnvelope)?;

    Ok(SealedUserSecret {
        encrypted_blob,
        masked: user_secret_mask(&provider),
    })
}

pub async fn unseal_user_secret(
    key_provider: &dyn KeyProvider,
    tenant_id: Uuid,
    user_id: &str,
    secret_id: Uuid,
    expected_provider: &str,
    encrypted_blob: Value,
) -> Result<Zeroizing<String>, UserSecretServiceError> {
    let expected_provider = normalize_user_secret_provider(expected_provider)?;
    let context = user_secret_context(tenant_id, user_id, secret_id)?;
    let encrypted: EncryptedBlob = serde_json::from_value(encrypted_blob)
        .map_err(|_| UserSecretServiceError::InvalidEnvelope)?;
    let plaintext = decrypt_blob(key_provider, &encrypted, &context).await?;
    let mut payload: StoredSecretPayload = serde_json::from_slice(plaintext.as_slice())
        .map_err(|_| UserSecretServiceError::InvalidEnvelope)?;
    if payload.version != STORED_SECRET_PAYLOAD_VERSION || payload.provider != expected_provider {
        return Err(UserSecretServiceError::InvalidContext);
    }
    validate_user_secret_value(&payload.key)?;
    Ok(Zeroizing::new(std::mem::take(&mut payload.key)))
}

pub fn user_secret_mask(provider: &str) -> String {
    format!("{provider}:configured")
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_crypto::{DeploymentMode, EnvKeyProvider};

    fn provider() -> EnvKeyProvider {
        EnvKeyProvider::new(
            Zeroizing::new([7u8; 32]),
            "user-secret-test-kek",
            1,
            DeploymentMode::Development,
        )
        .unwrap()
    }

    #[test]
    fn provider_name_and_secret_validation_is_strict() {
        assert_eq!(
            normalize_user_secret_provider(" OpenAI ").unwrap(),
            "openai"
        );
        assert!(normalize_user_secret_provider("ollama").is_err());
        assert!(normalize_user_secret_provider("unknown").is_err());
        assert_eq!(
            normalize_user_secret_name(" personal ").unwrap(),
            "personal"
        );
        assert!(normalize_user_secret_name("line\nbreak").is_err());
        assert!(validate_user_secret_value("sk-valid_value-123").is_ok());
        assert!(validate_user_secret_value("secret with spaces").is_err());
        assert!(validate_user_secret_value("secret\nheader").is_err());
    }

    #[tokio::test]
    async fn envelope_round_trip_binds_tenant_user_resource_and_provider() {
        let key_provider = provider();
        let tenant = Uuid::now_v7();
        let secret_id = Uuid::now_v7();
        let sealed = seal_user_secret(
            &key_provider,
            tenant,
            "user_test",
            secret_id,
            "openai",
            "sk-never-persist-plaintext",
        )
        .await
        .unwrap();
        let serialized = sealed.encrypted_blob.to_string();
        assert!(!serialized.contains("sk-never-persist-plaintext"));

        let plaintext = unseal_user_secret(
            &key_provider,
            tenant,
            "user_test",
            secret_id,
            "openai",
            sealed.encrypted_blob.clone(),
        )
        .await
        .unwrap();
        assert_eq!(plaintext.as_str(), "sk-never-persist-plaintext");

        let wrong_provider = unseal_user_secret(
            &key_provider,
            tenant,
            "user_test",
            secret_id,
            "openrouter",
            sealed.encrypted_blob.clone(),
        )
        .await
        .unwrap_err();
        assert_eq!(wrong_provider, UserSecretServiceError::InvalidContext);

        let wrong_tenant = unseal_user_secret(
            &key_provider,
            Uuid::now_v7(),
            "user_test",
            secret_id,
            "openai",
            sealed.encrypted_blob,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            wrong_tenant,
            UserSecretServiceError::KeyOperation {
                class: KeyFailureClass::ContextMismatch,
                ..
            }
        ));
    }

    #[test]
    fn service_debug_and_errors_do_not_expose_secret_material() {
        let sealed = SealedUserSecret {
            encrypted_blob: serde_json::json!({"ciphertext": "sensitive-ciphertext"}),
            masked: user_secret_mask("openai"),
        };
        let debug = format!("{sealed:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("sensitive-ciphertext"));

        let error = UserSecretServiceError::KeyOperation {
            operation: KeyOperation::DecryptBlob,
            class: KeyFailureClass::AccessDenied,
            retryable: false,
        };
        assert_eq!(error.to_string(), "stored credential key operation failed");
    }
}
