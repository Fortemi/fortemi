//! Explicit disposable-provider profile. Run through enterprise-kms/scripts/qualify-openbao.py.
//! The profile fails if its required infrastructure is missing; normal CI uses unit fixtures.
#![cfg(feature = "kms-vault")]

use matric_crypto::provider::{
    decrypt_blob, encrypt_blob, HealthStatus, KeyContext, KeyProvider, KeyPurpose,
    VaultTransitConfig, VaultTransitProvider,
};
use std::{collections::BTreeMap, path::PathBuf, process::Command, time::Duration};

fn control(action: &str) {
    let script = std::env::var("FORTEMI_VAULT_LIVE_CONTROL").expect("required control script");
    let result = Command::new("python3")
        .arg(script)
        .arg("--control")
        .arg(action)
        .output()
        .expect("control process launches");
    assert!(
        result.status.success(),
        "disposable control operation failed"
    );
}

fn provider(values: &BTreeMap<String, String>) -> VaultTransitProvider {
    let config = VaultTransitConfig::from_lookup(|name| values.get(name).cloned())
        .expect("valid explicit live configuration");
    VaultTransitProvider::new(config).expect("live HTTPS provider constructs")
}

#[tokio::test]
#[ignore = "requires explicit disposable TLS OpenBao runner"]
async fn live_openbao_contract() {
    let scratch = PathBuf::from(
        std::env::var("FORTEMI_VAULT_LIVE_SCRATCH")
            .expect("run enterprise-kms/scripts/qualify-openbao.py"),
    );
    let address = std::fs::read_to_string(scratch.join("address")).unwrap();
    let mut values: BTreeMap<String, String> = [
        ("FORTEMI_VAULT_ADDR", address.as_str()),
        ("FORTEMI_VAULT_TRANSIT_KEY", "fortemi-test"),
        ("FORTEMI_KEY_STRATEGY", "shared-with-context"),
        ("FORTEMI_KEY_CONTEXT_VERSION", "1"),
        ("FORTEMI_VAULT_AUTH_METHOD", "token-file"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    values.insert(
        "FORTEMI_VAULT_TOKEN_FILE".into(),
        scratch.join("runtime.token").display().to_string(),
    );
    values.insert(
        "FORTEMI_VAULT_CA_BUNDLE".into(),
        scratch.join("ca.pem").display().to_string(),
    );
    let provider = provider(&values);
    control("runtime-denials");
    let context = KeyContext::new(KeyPurpose::USER_SECRET, "user_secrets")
        .unwrap()
        .with_tenant_id("tenant-a")
        .unwrap()
        .with_user_id("user-a")
        .unwrap()
        .with_resource_id("secret-a")
        .unwrap();
    assert!(matches!(
        provider.health_check(&context).await.unwrap(),
        HealthStatus::Ready
    ));
    let mut per_purpose = values.clone();
    per_purpose.insert("FORTEMI_KEY_STRATEGY".into(), "per-purpose".into());
    per_purpose.insert("FORTEMI_VAULT_TRANSIT_KEY".into(), "fortemi-purpose".into());
    assert!(matches!(
        self::provider(&per_purpose)
            .health_check(&context)
            .await
            .unwrap(),
        HealthStatus::Ready
    ));
    let generated = provider.generate_dek(&context, 32).await.unwrap();
    let original = provider
        .unwrap_dek(generated.wrapped_key(), &context)
        .await
        .unwrap();
    assert!(
        original.expose_secret() == generated.plaintext().expose_secret(),
        "round-trip matches"
    );

    for wrong in [
        context.clone().with_tenant_id("tenant-b").unwrap(),
        context.clone().with_user_id("user-b").unwrap(),
        context.clone().with_resource_id("secret-b").unwrap(),
        KeyContext::new(KeyPurpose::OAUTH_REFRESH_TOKEN, "user_secrets").unwrap(),
        KeyContext::new(KeyPurpose::USER_SECRET, "other_schema").unwrap(),
    ] {
        assert!(
            provider
                .unwrap_dek(generated.wrapped_key(), &wrong)
                .await
                .is_err(),
            "wrong context denied"
        );
    }
    let mut blob = encrypt_blob(&provider, b"disposable qualification secret", &context)
        .await
        .unwrap();
    let ciphertext = blob.ciphertext().to_vec();
    let nonce = blob.nonce().to_vec();
    control("rotate");
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_ok(),
        "old version survives rotation"
    );
    let rewrapped = provider
        .rewrap_dek(blob.wrapped_key(), &context)
        .await
        .unwrap();
    assert!(
        rewrapped.wrapped_dek() != blob.wrapped_key().wrapped_dek(),
        "rewrap changes wrapping"
    );
    assert!(
        rewrapped.rewrapped_at().is_some(),
        "rewrap timestamp preserved"
    );
    blob.replace_wrapped_key(rewrapped).unwrap();
    assert!(
        blob.ciphertext() == ciphertext && blob.nonce() == nonce,
        "payload unchanged"
    );
    assert!(
        decrypt_blob(&provider, &blob, &context)
            .await
            .unwrap()
            .as_slice()
            == b"disposable qualification secret",
        "same DEK decrypts payload"
    );

    control("deny");
    assert!(
        provider.health_check(&context).await.is_err(),
        "insufficient policy fails startup probe"
    );
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_err(),
        "insufficient policy fails runtime"
    );
    control("replace");
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_ok(),
        "same provider reloads replacement token"
    );
    control("short");
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_err(),
        "expired token fails closed"
    );
    control("replace");
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_ok(),
        "replacement after expiry recovers"
    );
    control("renewable");
    control("renew");
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_ok(),
        "externally renewed token outlives original TTL"
    );

    let mut untrusted = values.clone();
    untrusted.remove("FORTEMI_VAULT_CA_BUNDLE");
    assert!(
        self::provider(&untrusted)
            .health_check(&context)
            .await
            .is_err(),
        "private CA absent fails TLS"
    );
    let mut wrong_hostname = values.clone();
    wrong_hostname.insert(
        "FORTEMI_VAULT_ADDR".into(),
        address.replace("localhost", "127.0.0.1"),
    );
    assert!(
        self::provider(&wrong_hostname)
            .health_check(&context)
            .await
            .is_err(),
        "hostname mismatch fails TLS"
    );
    let mut unreachable = values.clone();
    unreachable.insert("FORTEMI_VAULT_ADDR".into(), "https://localhost:1".into());
    assert!(
        self::provider(&unreachable)
            .health_check(&context)
            .await
            .is_err(),
        "unreachable provider fails startup"
    );
    control("seal");
    assert!(
        provider.health_check(&context).await.is_err(),
        "sealed service fails startup"
    );
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_err(),
        "sealed service fails runtime"
    );
    control("unseal");
    assert!(
        provider
            .unwrap_dek(generated.wrapped_key(), &context)
            .await
            .is_ok(),
        "same provider recovers after unseal"
    );

    let receipt = serde_json::json!({
        "status":"PASS", "scope":"actual Rust provider against disposable HTTPS OpenBao",
        "datakey_strategy":"local OS CSPRNG32 plus Transit encrypt with derived context and AEAD associated_data",
        "startup_generate_decrypt":true,"per_purpose_and_shared_profiles":true,"wrong_tenant_user_resource_purpose_schema_denied":true,
        "insufficient_policy_denied":true,"expired_token_denied":true,
        "runtime_create_rotate_export_config_datakey_denied":true,
        "same_provider_token_file_replacement":true,"external_token_renewal":true,
        "old_version_decrypt_after_rotation":true,"same_dek_rewrap_payload_unchanged":true,
        "untrusted_ca_denied":true,"wrong_hostname_denied":true,
        "unreachable_startup_denied":true,"sealed_startup_runtime_denied":true,"unseal_recovery":true,
        "production_deployment_qualified":false
    });
    std::fs::write(
        std::env::var("FORTEMI_VAULT_LIVE_RECEIPT").expect("required receipt path"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
