use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use super::*;
use crate::provider::{decrypt_blob, encrypt_blob, KeyPurpose};

const KEY_ID: &str = "arn:aws:kms:us-east-1:123456789012:key/test-key";

#[derive(Default)]
struct MockKmsClient {
    contexts: Mutex<Vec<BTreeMap<String, String>>>,
}

impl MockKmsClient {
    fn mask(context: &BTreeMap<String, String>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for (key, value) in context {
            hasher.update((key.len() as u64).to_be_bytes());
            hasher.update(key.as_bytes());
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        hasher.finalize().into()
    }

    fn encrypt_value(plaintext: &[u8], context: &BTreeMap<String, String>) -> Vec<u8> {
        let mask = Self::mask(context);
        let mut ciphertext = mask.to_vec();
        ciphertext.extend(
            plaintext
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % mask.len()]),
        );
        ciphertext
    }

    fn record(&self, context: &BTreeMap<String, String>) {
        self.contexts.lock().unwrap().push(context.clone());
    }
}

impl AwsKmsClient for MockKmsClient {
    fn generate_data_key<'a>(
        &'a self,
        key_id: &'a str,
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsGenerateDataKeyOutput> {
        Box::pin(async move {
            self.record(encryption_context);
            let plaintext = vec![0x42; 32];
            Ok(AwsKmsGenerateDataKeyOutput {
                ciphertext: Self::encrypt_value(&plaintext, encryption_context),
                plaintext: Zeroizing::new(plaintext),
                key_id: key_id.to_string(),
                key_material_id: Some("material-v1".to_string()),
            })
        })
    }

    fn encrypt<'a>(
        &'a self,
        key_id: &'a str,
        plaintext: &'a [u8],
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsWrapOutput> {
        Box::pin(async move {
            self.record(encryption_context);
            Ok(AwsKmsWrapOutput {
                ciphertext: Self::encrypt_value(plaintext, encryption_context),
                key_id: key_id.to_string(),
            })
        })
    }

    fn decrypt<'a>(
        &'a self,
        key_id: &'a str,
        ciphertext: &'a [u8],
        encryption_context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsDecryptOutput> {
        Box::pin(async move {
            self.record(encryption_context);
            let mask = Self::mask(encryption_context);
            if ciphertext.len() < mask.len() || ciphertext[..mask.len()] != mask {
                return Err(AwsKmsClientError::new(KeyFailureClass::ContextMismatch));
            }
            let plaintext = ciphertext[mask.len()..]
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % mask.len()])
                .collect();
            Ok(AwsKmsDecryptOutput {
                plaintext: Zeroizing::new(plaintext),
                key_id: Some(key_id.to_string()),
                key_material_id: Some("material-v1".to_string()),
            })
        })
    }
}

fn context(tenant: &str, resource: &str) -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, "user_secrets")
        .unwrap()
        .with_tenant_id(tenant)
        .unwrap()
        .with_user_id("user-1")
        .unwrap()
        .with_resource_id(resource)
        .unwrap()
}

fn provider(client: Arc<MockKmsClient>) -> AwsKmsProvider {
    AwsKmsProvider::new(client, KEY_ID).unwrap()
}

#[tokio::test]
async fn generate_encrypt_decrypt_and_health_use_kms_canary() {
    let client = Arc::new(MockKmsClient::default());
    let provider = provider(client.clone());
    let context = context("tenant-a", "secret-1");

    let blob = encrypt_blob(&provider, b"credential", &context)
        .await
        .unwrap();
    assert_eq!(
        blob.wrapped_key().provider_metadata().get("key_id"),
        Some(&KEY_ID.to_string())
    );
    assert_eq!(
        blob.wrapped_key()
            .provider_metadata()
            .get("key_material_id"),
        Some(&"material-v1".to_string())
    );
    assert_eq!(
        decrypt_blob(&provider, &blob, &context)
            .await
            .unwrap()
            .as_slice(),
        b"credential"
    );
    assert_eq!(
        provider.health_check(&context).await.unwrap(),
        HealthStatus::Ready
    );

    let contexts = client.contexts.lock().unwrap();
    assert!(contexts.iter().all(|values| {
        values.get("fortemi_context_version").map(String::as_str) == Some("1")
            && values.get("provider_kind").map(String::as_str) == Some("aws-kms")
            && values.get("kek_ref").map(String::as_str) == Some(KEY_ID)
            && values.get("tenant_id").map(String::as_str) == Some("tenant-a")
            && values.get("resource_id").map(String::as_str) == Some("secret-1")
    }));
}

#[tokio::test]
async fn encryption_context_tampering_fails_closed() {
    let provider = provider(Arc::new(MockKmsClient::default()));
    let trusted = context("tenant-a", "secret-1");
    let wrapped = provider.generate_dek(&trusted, 32).await.unwrap();

    for wrong in [
        context("tenant-b", "secret-1"),
        context("tenant-a", "secret-2"),
    ] {
        let error = provider
            .unwrap_dek(wrapped.wrapped_key(), &wrong)
            .await
            .unwrap_err();
        assert_eq!(error.class(), KeyFailureClass::ContextMismatch);
        assert_eq!(
            error.degraded_mode(),
            super::super::DegradedMode::FailClosed
        );
    }
}

#[tokio::test]
async fn wrap_unwrap_and_rewrap_preserve_the_dek() {
    let provider = provider(Arc::new(MockKmsClient::default()));
    let context = context("tenant-a", "secret-1");
    let plaintext = PlaintextDek::new(vec![0x99; 32]).unwrap();
    let wrapped = provider.wrap_dek(&plaintext, &context).await.unwrap();
    let unwrapped = provider.unwrap_dek(&wrapped, &context).await.unwrap();
    assert_eq!(unwrapped.expose_secret(), plaintext.expose_secret());

    let rewrapped = provider.rewrap_dek(&wrapped, &context).await.unwrap();
    assert!(rewrapped.rewrapped_at().is_some());
    assert_eq!(
        provider
            .unwrap_dek(&rewrapped, &context)
            .await
            .unwrap()
            .expose_secret(),
        plaintext.expose_secret()
    );
}

#[test]
fn provider_and_client_errors_are_stably_redacted() {
    let provider = provider(Arc::new(MockKmsClient::default()));
    let rendered = format!(
        "{provider:?} {}",
        AwsKmsClientError::new(KeyFailureClass::AccessDenied)
    );
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains(KEY_ID));
    assert!(!rendered.contains("credential"));
}
