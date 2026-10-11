//! Live AWS KMS qualification under workload identity (Fortemi-Enterprise/kms#4).
//! Runs in-cluster as the qualification Job: the only credential is the pod's
//! web-identity token (IRSA or EKS Pod Identity). The full scenario profile in
//! aws_kms_live.rs drives key-admin actions through an operator runner; this one
//! proves the production provider and key policy under workload credentials.
#![cfg(feature = "kms-aws")]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use matric_crypto::{
    decrypt_blob, encrypt_blob, rewrap_between, AwsKmsClient, AwsKmsDecryptOutput, AwsKmsFuture,
    AwsKmsGenerateDataKeyOutput, AwsKmsProvider, AwsKmsWrapOutput, AwsSdkKmsClient, HealthStatus,
    KeyContext, KeyError, KeyFailureClass, KeyProvider, KeyPurpose,
};

const SECRET: &[u8] = b"fortemi kms4 workload identity qualification secret";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required in the qualification Job"))
}

fn context(tenant: &str, user: &str, resource: &str, schema: &str) -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, schema)
        .and_then(|c| c.with_tenant_id(tenant))
        .and_then(|c| c.with_user_id(user))
        .and_then(|c| c.with_resource_id(resource))
        .expect("valid context")
}

fn class_of<T>(result: &Result<T, KeyError>) -> Option<KeyFailureClass> {
    result.as_ref().err().map(KeyError::class)
}

async fn sdk() -> aws_sdk_kms::Client {
    let config = aws_config::defaults(aws_config::BehaviorVersion::v2026_01_12())
        .load()
        .await;
    aws_sdk_kms::Client::new(&config)
}

/// Transport decorator that counts decrypt requests reaching KMS.
struct Counting {
    inner: AwsSdkKmsClient,
    decrypts: AtomicUsize,
}

impl AwsKmsClient for Counting {
    fn generate_data_key<'a>(
        &'a self,
        key_id: &'a str,
        context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsGenerateDataKeyOutput> {
        self.inner.generate_data_key(key_id, context)
    }

    fn encrypt<'a>(
        &'a self,
        key_id: &'a str,
        plaintext: &'a [u8],
        context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsWrapOutput> {
        self.inner.encrypt(key_id, plaintext, context)
    }

    fn decrypt<'a>(
        &'a self,
        key_id: &'a str,
        ciphertext: &'a [u8],
        context: &'a BTreeMap<String, String>,
    ) -> AwsKmsFuture<'a, AwsKmsDecryptOutput> {
        self.decrypts.fetch_add(1, Ordering::SeqCst);
        self.inner.decrypt(key_id, ciphertext, context)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the in-cluster workload-identity qualification Job"]
async fn live_aws_kms_workload_identity() {
    let key_arn = env("FORTEMI_AWS_KMS_KEY_ID");
    let mut passed: Vec<&str> = Vec::new();

    // The only credential source is the injected web-identity token.
    let token_file = env("AWS_WEB_IDENTITY_TOKEN_FILE");
    let role_arn = env("AWS_ROLE_ARN");
    assert!(
        std::path::Path::new(&token_file).is_file(),
        "web identity token is mounted"
    );
    for forbidden in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_PROFILE",
    ] {
        assert!(
            std::env::var_os(forbidden).is_none(),
            "{forbidden} must not be set"
        );
    }
    assert_eq!(
        std::env::var("AWS_EC2_METADATA_DISABLED").as_deref(),
        Ok("true"),
        "instance metadata is disabled"
    );
    passed.push("credentials_web_identity_only");

    // Startup canary through the exact production constructor.
    let ctx = context("tenant-a", "user-a", "secret-a", "user_secrets");
    let provider = Arc::new(
        AwsKmsProvider::from_environment(key_arn.clone())
            .await
            .unwrap(),
    );
    assert_eq!(
        provider.health_check(&ctx).await.unwrap(),
        HealthStatus::Ready
    );
    passed.push("startup_canary_ready");

    // No DEK cache: every unwrap reaches KMS.
    let counting = Arc::new(Counting {
        inner: AwsSdkKmsClient::new(sdk().await),
        decrypts: AtomicUsize::new(0),
    });
    let counted = AwsKmsProvider::new(counting.clone(), key_arn.clone()).unwrap();
    let generated = counted.generate_dek(&ctx, 32).await.unwrap();
    for _ in 0..3 {
        let unwrapped = counted
            .unwrap_dek(generated.wrapped_key(), &ctx)
            .await
            .unwrap();
        assert!(unwrapped.expose_secret() == generated.plaintext().expose_secret());
    }
    assert_eq!(counting.decrypts.load(Ordering::SeqCst), 3, "no DEK cache");
    passed.push("no_dek_cache");

    // Wrong canonical context or tenant is refused.
    for wrong in [
        context("tenant-b", "user-a", "secret-a", "user_secrets"),
        context("tenant-a", "user-b", "secret-a", "user_secrets"),
        context("tenant-a", "user-a", "secret-b", "user_secrets"),
        context("tenant-a", "user-a", "secret-a", "other_schema"),
    ] {
        let result = provider.unwrap_dek(generated.wrapped_key(), &wrong).await;
        assert_eq!(class_of(&result), Some(KeyFailureClass::ContextMismatch));
    }
    passed.push("wrong_context_denied");

    // The key policy denies a non-canonical encryption context to the workload role.
    let raw = AwsSdkKmsClient::new(sdk().await);
    let partial = BTreeMap::from([
        ("fortemi_context_version".to_owned(), "1".to_owned()),
        ("provider_kind".to_owned(), "aws-kms".to_owned()),
        ("tenant_id".to_owned(), "tenant-a".to_owned()),
    ]);
    let denied = raw.generate_data_key(&key_arn, &partial).await;
    assert_eq!(
        denied.err().map(|e| e.class()),
        Some(KeyFailureClass::AccessDenied)
    );
    passed.push("policy_denies_noncanonical_context");

    // Seal, unseal and same-key rewrap round trip.
    let blob = encrypt_blob(provider.as_ref(), SECRET, &ctx).await.unwrap();
    assert_eq!(
        decrypt_blob(provider.as_ref(), &blob, &ctx)
            .await
            .unwrap()
            .as_slice(),
        SECRET
    );
    let rewrapped = rewrap_between(
        provider.as_ref(),
        provider.as_ref(),
        blob.wrapped_key(),
        &ctx,
    )
    .await
    .unwrap();
    let mut migrated = blob.clone();
    migrated.replace_wrapped_key(rewrapped).unwrap();
    assert!(migrated.ciphertext() == blob.ciphertext());
    assert_eq!(
        decrypt_blob(provider.as_ref(), &migrated, &ctx)
            .await
            .unwrap()
            .as_slice(),
        SECRET
    );
    passed.push("seal_unseal_rewrap");

    // The workload role cannot administer the key.
    let admin = sdk().await.disable_key().key_id(&key_arn).send().await;
    let code = admin
        .err()
        .and_then(|e| e.into_service_error().meta().code().map(str::to_owned));
    assert_eq!(code.as_deref(), Some("AccessDeniedException"));
    passed.push("key_admin_denied");

    // Receipt: identifiers only, never credentials or key material.
    let role_name = role_arn.rsplit('/').next().unwrap_or_default();
    println!(
        "{{\"receipt\":\"fortemi-kms4-workload-identity\",\"status\":\"PASS\",\"key_arn\":\"{key_arn}\",\"role\":\"{role_name}\",\"credential_source\":\"web_identity\",\"crate_version\":\"{}\",\"scenarios\":[{}]}}",
        env!("CARGO_PKG_VERSION"),
        passed
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
}
