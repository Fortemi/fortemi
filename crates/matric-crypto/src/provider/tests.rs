use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::*;

const MOCK_KEK_REF: &str = "deterministic-mock-kek";
const MOCK_WRAP_DOMAIN: &str = "fortemi/test/mock-wrap/aad/v1";

struct DeterministicMockProvider {
    current_version: AtomicU64,
    nonce_counter: AtomicU64,
    dek_counter: AtomicU64,
}

impl DeterministicMockProvider {
    fn new() -> Self {
        Self {
            current_version: AtomicU64::new(1),
            nonce_counter: AtomicU64::new(1),
            dek_counter: AtomicU64::new(1),
        }
    }

    fn provider_kind() -> KeyProviderKind {
        KeyProviderKind::Other("deterministic-mock".to_string())
    }

    fn key(version: u64) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"fortemi/test/mock-kek/v1");
        hasher.update(version.to_be_bytes());
        hasher.finalize().into()
    }

    fn nonce(counter: u64) -> Vec<u8> {
        let mut nonce = vec![0u8; AES_GCM_NONCE_BYTES];
        nonce[4..].copy_from_slice(&counter.to_be_bytes());
        nonce
    }

    fn timestamp() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-24T00:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc)
    }
}

impl KeyProvider for DeterministicMockProvider {
    fn kind(&self) -> KeyProviderKind {
        Self::provider_kind()
    }

    fn wrap_dek<'a>(
        &'a self,
        plaintext_dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            let version = self.current_version.load(Ordering::SeqCst);
            let key = Zeroizing::new(Self::key(version));
            let cipher = Aes256Gcm::new_from_slice(key.as_slice()).expect("valid fixture key");
            let nonce = Self::nonce(self.nonce_counter.fetch_add(1, Ordering::SeqCst));
            let aad =
                context.canonical_bytes(MOCK_WRAP_DOMAIN, &Self::provider_kind(), MOCK_KEK_REF)?;
            let ciphertext = cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext_dek.expose_secret(),
                        aad: &aad,
                    },
                )
                .map_err(|_| {
                    KeyError::new(KeyOperation::WrapDek, KeyFailureClass::ProviderFailure)
                })?;
            let mut metadata = BTreeMap::new();
            metadata.insert("key_version".to_string(), version.to_string());
            WrappedKey::new(
                Self::provider_kind(),
                MOCK_KEK_REF,
                context.purpose().clone(),
                context.version(),
                ciphertext,
                Some(nonce),
                metadata,
                Self::timestamp(),
            )
        })
    }

    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        Box::pin(async move {
            wrapped.validate()?;
            if wrapped.provider_kind() != &Self::provider_kind()
                || wrapped.kek_ref() != MOCK_KEK_REF
                || wrapped.purpose() != context.purpose()
                || wrapped.context_version() != context.version()
            {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::ContextMismatch,
                ));
            }
            let version = metadata_version(wrapped)?;
            if version > self.current_version.load(Ordering::SeqCst) {
                return Err(KeyError::new(
                    KeyOperation::UnwrapDek,
                    KeyFailureClass::KeyVersionUnavailable,
                ));
            }
            let key = Zeroizing::new(Self::key(version));
            let cipher = Aes256Gcm::new_from_slice(key.as_slice()).expect("valid fixture key");
            let nonce = wrapped.wrapping_nonce().ok_or_else(|| {
                KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::InvalidCiphertext)
            })?;
            let aad =
                context.canonical_bytes(MOCK_WRAP_DOMAIN, &Self::provider_kind(), MOCK_KEK_REF)?;
            let plaintext = cipher
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: wrapped.wrapped_dek(),
                        aad: &aad,
                    },
                )
                .map_err(|_| {
                    KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::ContextMismatch)
                })?;
            PlaintextDek::new(plaintext)
        })
    }

    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek> {
        Box::pin(async move {
            validate_dek_len(bytes, KeyOperation::GenerateDek)?;
            let counter = self.dek_counter.fetch_add(1, Ordering::SeqCst);
            let mut plaintext = Vec::with_capacity(bytes);
            let mut block = 0u64;
            while plaintext.len() < bytes {
                let mut hasher = Sha256::new();
                hasher.update(b"fortemi/test/mock-dek/v1");
                hasher.update(counter.to_be_bytes());
                hasher.update(block.to_be_bytes());
                plaintext.extend_from_slice(&hasher.finalize());
                block += 1;
            }
            plaintext.truncate(bytes);
            let plaintext = PlaintextDek::new(plaintext)?;
            let wrapped_key = self.wrap_dek(&plaintext, context).await?;
            GeneratedDek::new(plaintext, wrapped_key)
        })
    }

    fn rotate<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, RotationInfo> {
        Box::pin(async move {
            let previous = self.current_version.fetch_add(1, Ordering::SeqCst);
            RotationInfo::new(
                Self::provider_kind(),
                context.purpose().clone(),
                Some(previous.to_string()),
                (previous + 1).to_string(),
            )
        })
    }

    fn health_check<'a>(&'a self, _context: &'a KeyContext) -> KeyFuture<'a, HealthStatus> {
        Box::pin(async { Ok(HealthStatus::Ready) })
    }
}

fn context(purpose: KeyPurpose, tenant: &str, resource: &str) -> KeyContext {
    KeyContext::new(purpose, "user_secrets")
        .unwrap()
        .with_tenant_id(tenant)
        .unwrap()
        .with_resource_id(resource)
        .unwrap()
}

fn env_provider(key: u8, kek_ref: &str, version: u64) -> EnvKeyProvider {
    EnvKeyProvider::new(
        Zeroizing::new([key; 32]),
        kek_ref,
        version,
        DeploymentMode::Development,
    )
    .unwrap()
}

#[test]
fn purpose_and_context_are_explicitly_versioned() {
    let purpose = KeyPurpose::USER_SECRET;
    let context = context(purpose.clone(), "tenant-a", "secret-1");
    assert_eq!(purpose.version(), 1);
    assert_eq!(context.version(), 1);
    assert_eq!(purpose.name(), "user_secret");
    assert!(KeyPurpose::custom("free text is rejected").is_err());
}

#[test]
fn env_provider_refuses_hosted_multi_tenant_mode() {
    let result = EnvKeyProvider::new(
        Zeroizing::new([7u8; 32]),
        "local-kek",
        1,
        DeploymentMode::HostedMultiTenant,
    );
    assert_eq!(
        result.unwrap_err().class(),
        KeyFailureClass::InvalidConfiguration
    );
}

#[tokio::test]
async fn env_provider_binds_wrap_to_purpose_and_context() {
    let provider = env_provider(7, "local-kek-v1", 1);
    let plaintext = PlaintextDek::new(vec![42; 32]).unwrap();
    let user_context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let oauth_context = context(KeyPurpose::OAUTH_REFRESH_TOKEN, "tenant-a", "secret-1");

    let user_wrapped = provider.wrap_dek(&plaintext, &user_context).await.unwrap();
    let oauth_wrapped = provider.wrap_dek(&plaintext, &oauth_context).await.unwrap();
    assert_ne!(user_wrapped.wrapped_dek(), oauth_wrapped.wrapped_dek());

    let error = provider
        .unwrap_dek(&user_wrapped, &oauth_context)
        .await
        .unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::ContextMismatch);
    assert_eq!(error.degraded_mode(), DegradedMode::FailClosed);
}

#[tokio::test]
async fn env_health_is_round_trip_and_signing_is_not_faked() {
    let provider = env_provider(7, "local-kek-v1", 1);
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    assert_eq!(
        provider.health_check(&context).await.unwrap(),
        HealthStatus::Ready
    );
    let error = provider.sign(&context, b"payload").await.unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::UnsupportedOperation);
}

#[tokio::test]
async fn secret_bearing_debug_output_is_redacted() {
    let provider = env_provider(7, "sensitive-local-kek-ref", 1);
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let generated = provider.generate_dek(&context, 32).await.unwrap();
    let debug = format!("{provider:?} {generated:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("len"));
    assert!(!debug.contains("sensitive-local-kek-ref"));
    assert!(!debug.contains(&base64::engine::general_purpose::STANDARD.encode([7u8; 32])));
}

#[test]
fn provider_binding_rejects_a_wrapped_key_from_another_provider_kind() {
    let provider = env_provider(7, "local-kek-v1", 1);
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let wrapped = WrappedKey::new(
        KeyProviderKind::AwsKms,
        "arn:aws:kms:us-east-1:123456789012:key/example",
        context.purpose().clone(),
        context.version(),
        vec![1, 2, 3],
        None,
        BTreeMap::new(),
        DeterministicMockProvider::timestamp(),
    )
    .unwrap();

    let error = validate_provider_binding(&provider, &wrapped, &context, KeyOperation::DecryptBlob)
        .unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::ContextMismatch);
}

#[tokio::test]
async fn deterministic_mock_repeats_fixture_output() {
    let first = DeterministicMockProvider::new();
    let second = DeterministicMockProvider::new();
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let first = first.generate_dek(&context, 32).await.unwrap();
    let second = second.generate_dek(&context, 32).await.unwrap();
    assert_eq!(
        first.plaintext().expose_secret(),
        second.plaintext().expose_secret()
    );
    assert_eq!(first.wrapped_key(), second.wrapped_key());
}

#[tokio::test]
async fn envelope_round_trip_and_context_mismatch_fail_closed() {
    let provider = DeterministicMockProvider::new();
    let expected_context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let wrong_context = context(KeyPurpose::USER_SECRET, "tenant-b", "secret-1");
    let blob = encrypt_blob(&provider, b"provider credential", &expected_context)
        .await
        .unwrap();

    let plaintext = decrypt_blob(&provider, &blob, &expected_context)
        .await
        .unwrap();
    assert_eq!(plaintext.as_slice(), b"provider credential");
    let error = decrypt_blob(&provider, &blob, &wrong_context)
        .await
        .unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::ContextMismatch);
}

#[tokio::test]
async fn old_wrapped_deks_survive_rotation_and_rewrap_preserves_dek() {
    let provider = DeterministicMockProvider::new();
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let generated = provider.generate_dek(&context, 32).await.unwrap();
    let original_plaintext = generated.plaintext().expose_secret().to_vec();
    let original_wrapped = generated.wrapped_key().clone();

    let receipt = provider.rotate(&context).await.unwrap();
    assert_eq!(receipt.previous_version(), Some("1"));
    assert_eq!(receipt.current_version(), "2");
    let old_plaintext = provider
        .unwrap_dek(&original_wrapped, &context)
        .await
        .unwrap();
    assert_eq!(old_plaintext.expose_secret(), original_plaintext);

    let rewrapped = provider
        .rewrap_dek(&original_wrapped, &context)
        .await
        .unwrap();
    assert_eq!(metadata_version(&rewrapped).unwrap(), 2);
    assert!(rewrapped.rewrapped_at().is_some());
    let rewrapped_plaintext = provider.unwrap_dek(&rewrapped, &context).await.unwrap();
    assert_eq!(rewrapped_plaintext.expose_secret(), original_plaintext);
}

#[tokio::test]
async fn cross_provider_rewrap_does_not_require_payload_reencryption() {
    let old = env_provider(7, "local-kek-v1", 1);
    let new = env_provider(9, "local-kek-v2", 2);
    let context = context(KeyPurpose::USER_SECRET, "tenant-a", "secret-1");
    let mut blob = encrypt_blob(&old, b"provider credential", &context)
        .await
        .unwrap();

    let next = rewrap_between(&old, &new, blob.wrapped_key(), &context)
        .await
        .unwrap();
    blob.replace_wrapped_key(next).unwrap();
    let plaintext = decrypt_blob(&new, &blob, &context).await.unwrap();
    assert_eq!(plaintext.as_slice(), b"provider credential");
}

#[test]
fn provider_failures_expose_only_stable_metadata() {
    let error = KeyError::new(KeyOperation::UnwrapDek, KeyFailureClass::AccessDenied);
    assert_eq!(error.degraded_mode(), DegradedMode::FailClosed);
    assert!(!error.is_retryable());
    assert!(!error.to_string().contains("credential"));

    let unavailable = KeyError::new(
        KeyOperation::HealthCheck,
        KeyFailureClass::ProviderUnavailable,
    );
    assert!(unavailable.is_retryable());
}
