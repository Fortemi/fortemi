//! Explicit live AWS KMS profile. Run through enterprise-kms/scripts/qualify-aws.py.
//! The runner creates isolated keys and short-lived role credentials; this test fails
//! if that infrastructure is missing. Normal CI uses the mock-transport unit tests.
#![cfg(feature = "kms-aws")]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use matric_crypto::{
    decrypt_blob, encrypt_blob, rewrap_between, AwsKmsClient, AwsKmsDecryptOutput, AwsKmsFuture,
    AwsKmsGenerateDataKeyOutput, AwsKmsProvider, AwsKmsWrapOutput, AwsSdkKmsClient, DegradedMode,
    EncryptedBlob, HealthStatus, KeyContext, KeyError, KeyFailureClass, KeyProvider, KeyPurpose,
    PlaintextDek,
};

const SECRET: &[u8] = b"fortemi kms4 live qualification secret";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required; run qualify-aws.py"))
}

fn control(action: &str) {
    let result = Command::new("python3")
        .arg(env("FORTEMI_AWS_LIVE_CONTROL"))
        .arg("--control")
        .arg(action)
        .output()
        .expect("control process launches");
    assert!(result.status.success(), "control operation {action} failed");
}

fn context(tenant: &str, user: &str, resource: &str, schema: &str) -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, schema)
        .and_then(|c| c.with_tenant_id(tenant))
        .and_then(|c| c.with_user_id(user))
        .and_then(|c| c.with_resource_id(resource))
        .expect("valid context")
}

fn canonical() -> KeyContext {
    context("tenant-a", "user-a", "secret-a", "user_secrets")
}

async fn sdk_client(profile: Option<&str>, endpoint: Option<&str>) -> AwsSdkKmsClient {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::v2026_01_12());
    if let Some(profile) = profile {
        loader = loader.profile_name(profile);
    }
    if let Some(endpoint) = endpoint {
        loader = loader.endpoint_url(endpoint);
    }
    AwsSdkKmsClient::new(aws_sdk_kms::Client::new(&loader.load().await))
}

/// Transport decorator that counts requests reaching the SDK client.
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

/// Poll until `check` yields `Some`, tolerating KMS key-state propagation delay.
async fn eventually<T, F, Fut>(what: &str, seconds: u64, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn class_of<T>(result: &Result<T, KeyError>) -> Option<KeyFailureClass> {
    result.as_ref().err().map(KeyError::class)
}

/// Loopback endpoint that always answers with a KMS ThrottlingException.
fn throttling_endpoint() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut request = Vec::new();
            let mut buffer = [0u8; 8192];
            while let Ok(read) = stream.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"__type":"ThrottlingException","message":"Rate exceeded"}"#;
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/x-amz-json-1.1\r\n\
                 x-amzn-RequestId: 00000000-0000-0000-0000-000000000000\r\n\
                 x-amzn-ErrorType: ThrottlingException\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (address, hits)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires explicit live AWS KMS runner"]
async fn live_aws_kms_contract() {
    let key_arn = env("FORTEMI_AWS_KMS_KEY_ID");
    let second_arn = env("FORTEMI_AWS_LIVE_SECOND_KEY");
    let account = key_arn
        .split(':')
        .nth(4)
        .expect("key ARN account")
        .to_owned();
    let ctx = canonical();

    // Startup canary through the exact production constructor and credential chain.
    let provider = Arc::new(
        AwsKmsProvider::from_environment(key_arn.clone())
            .await
            .unwrap(),
    );
    assert_eq!(
        provider.health_check(&ctx).await.unwrap(),
        HealthStatus::Ready
    );
    control("runtime-denials");

    // No DEK cache: every unwrap reaches KMS.
    let counting = Arc::new(Counting {
        inner: sdk_client(None, None).await,
        decrypts: AtomicUsize::new(0),
    });
    let counted = AwsKmsProvider::new(counting.clone(), key_arn.clone()).unwrap();
    let generated = counted.generate_dek(&ctx, 32).await.unwrap();
    for _ in 0..5 {
        let unwrapped = counted
            .unwrap_dek(generated.wrapped_key(), &ctx)
            .await
            .unwrap();
        assert!(unwrapped.expose_secret() == generated.plaintext().expose_secret());
    }
    assert_eq!(counting.decrypts.load(Ordering::SeqCst), 5, "no DEK cache");
    let initial_material = generated
        .wrapped_key()
        .provider_metadata()
        .get("key_material_id")
        .cloned();

    // Wrong canonical context: KMS rejects tenant/user/resource/schema; purpose locally.
    for wrong in [
        context("tenant-b", "user-a", "secret-a", "user_secrets"),
        context("tenant-a", "user-b", "secret-a", "user_secrets"),
        context("tenant-a", "user-a", "secret-b", "user_secrets"),
        context("tenant-a", "user-a", "secret-a", "other_schema"),
    ] {
        let result = provider.unwrap_dek(generated.wrapped_key(), &wrong).await;
        assert_eq!(class_of(&result), Some(KeyFailureClass::ContextMismatch));
    }
    let wrong_purpose = KeyContext::new(KeyPurpose::OAUTH_REFRESH_TOKEN, "user_secrets")
        .and_then(|c| c.with_tenant_id("tenant-a"))
        .unwrap();
    let result = provider
        .unwrap_dek(generated.wrapped_key(), &wrong_purpose)
        .await;
    assert_eq!(class_of(&result), Some(KeyFailureClass::ContextMismatch));

    // IAM encryption-context conditions deny non-canonical contexts at the policy layer.
    let raw = sdk_client(None, None).await;
    let mut missing = BTreeMap::from([
        ("fortemi_context_version".to_owned(), "1".to_owned()),
        ("provider_kind".to_owned(), "aws-kms".to_owned()),
        ("tenant_id".to_owned(), "tenant-a".to_owned()),
    ]);
    let denied = raw.generate_data_key(&key_arn, &missing).await;
    assert_eq!(
        denied.err().map(|e| e.class()),
        Some(KeyFailureClass::AccessDenied)
    );
    missing.insert("kek_ref".to_owned(), second_arn.clone());
    let denied = raw.generate_data_key(&key_arn, &missing).await;
    assert_eq!(
        denied.err().map(|e| e.class()),
        Some(KeyFailureClass::AccessDenied)
    );

    // Insufficient IAM policy: the under-privileged role may only Encrypt.
    let under = AwsKmsProvider::new(
        Arc::new(sdk_client(Some("fortemi-underprivileged"), None).await),
        key_arn.clone(),
    )
    .unwrap();
    assert_eq!(
        class_of(&under.health_check(&ctx).await),
        Some(KeyFailureClass::AccessDenied)
    );
    let result = under.unwrap_dek(generated.wrapped_key(), &ctx).await;
    assert_eq!(class_of(&result), Some(KeyFailureClass::AccessDenied));
    let probe = PlaintextDek::new(vec![0x5A; 32]).unwrap();
    let wrapped_by_under = under.wrap_dek(&probe, &ctx).await.unwrap();
    let probe_back = provider.unwrap_dek(&wrapped_by_under, &ctx).await.unwrap();
    assert!(probe_back.expose_secret() == probe.expose_secret());

    // Envelopes created before rotation.
    let mut blobs = Vec::new();
    for _ in 0..3 {
        blobs.push(encrypt_blob(provider.as_ref(), SECRET, &ctx).await.unwrap());
    }
    let payloads: Vec<(Vec<u8>, Vec<u8>)> = blobs
        .iter()
        .map(|b| (b.ciphertext().to_vec(), b.nonce().to_vec()))
        .collect();

    // On-demand key material rotation; old envelopes still decrypt.
    control("rotate");
    // KeyMaterialId is optional in the KMS response; require a change only when reported.
    let material_reported = initial_material.is_some();
    let rotated_material = if !material_reported {
        None
    } else {
        eventually("new key material", 300, || {
            let provider = provider.clone();
            let ctx = ctx.clone();
            let initial = initial_material.clone();
            async move {
                let fresh = provider.generate_dek(&ctx, 32).await.ok()?;
                let material = fresh
                    .wrapped_key()
                    .provider_metadata()
                    .get("key_material_id")
                    .cloned();
                (material.is_some() && material != initial).then_some(material)
            }
        })
        .await
    };
    assert_eq!(rotated_material.is_some(), material_reported);
    let old = provider
        .unwrap_dek(generated.wrapped_key(), &ctx)
        .await
        .unwrap();
    assert!(old.expose_secret() == generated.plaintext().expose_secret());

    // Resumable same-DEK rewrap with an interruption, rollback and recovery.
    let first = provider
        .rewrap_dek(blobs[0].wrapped_key(), &ctx)
        .await
        .unwrap();
    assert!(first.wrapped_dek() != blobs[0].wrapped_key().wrapped_dek());
    blobs[0].replace_wrapped_key(first).unwrap();
    control("disable");
    let disabled_class = eventually("disabled key", 180, || {
        let provider = provider.clone();
        let ctx = ctx.clone();
        async move { class_of(&provider.health_check(&ctx).await) }
    })
    .await;
    assert_eq!(disabled_class, KeyFailureClass::KeyDisabled);
    // No cached DEK survives disablement, even for an envelope unwrapped earlier.
    let result = provider.unwrap_dek(generated.wrapped_key(), &ctx).await;
    assert_eq!(class_of(&result), Some(KeyFailureClass::KeyDisabled));
    assert_eq!(
        provider
            .unwrap_dek(generated.wrapped_key(), &ctx)
            .await
            .unwrap_err()
            .degraded_mode(),
        DegradedMode::FailClosed
    );
    let before = blobs[1].clone();
    let interrupted = provider.rewrap_dek(blobs[1].wrapped_key(), &ctx).await;
    assert_eq!(class_of(&interrupted), Some(KeyFailureClass::KeyDisabled));
    assert!(
        blobs[1] == before,
        "failed rewrap leaves the prior envelope untouched"
    );
    control("enable");
    eventually("re-enabled key", 180, || {
        let provider = provider.clone();
        let ctx = ctx.clone();
        async move { matches!(provider.health_check(&ctx).await, Ok(HealthStatus::Ready)).then_some(()) }
    })
    .await;
    for blob in blobs.iter_mut().skip(1) {
        let next = provider.rewrap_dek(blob.wrapped_key(), &ctx).await.unwrap();
        assert!(next.rewrapped_at().is_some());
        blob.replace_wrapped_key(next).unwrap();
    }
    for (blob, (ciphertext, nonce)) in blobs.iter().zip(&payloads) {
        assert!(blob.ciphertext() == ciphertext.as_slice() && blob.nonce() == nonce.as_slice());
        let plain = decrypt_blob(provider.as_ref(), blob, &ctx).await.unwrap();
        assert_eq!(plain.as_slice(), SECRET);
    }

    // Throttling classification through the real SDK client and its retry policy.
    let (endpoint, hits) = throttling_endpoint();
    let throttled = AwsKmsProvider::new(
        Arc::new(sdk_client(None, Some(&endpoint)).await),
        key_arn.clone(),
    )
    .unwrap();
    let error = throttled.health_check(&ctx).await.unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::Throttled);
    assert_eq!(error.degraded_mode(), DegradedMode::RetryableFailClosed);
    assert!(error.is_retryable());
    let throttle_attempts = hits.load(Ordering::SeqCst);
    assert!(throttle_attempts >= 2, "SDK retried throttling");

    // Bounded real burst against KMS; records whether any genuine throttling occurred.
    let mut burst = tokio::task::JoinSet::new();
    for _ in 0..200 {
        let provider = provider.clone();
        let ctx = ctx.clone();
        burst.spawn(async move { class_of(&provider.generate_dek(&ctx, 32).await) });
    }
    let (mut burst_ok, mut burst_throttled, mut burst_other) = (0u32, 0u32, 0u32);
    while let Some(outcome) = burst.join_next().await {
        match outcome.unwrap() {
            None => burst_ok += 1,
            Some(KeyFailureClass::Throttled) => burst_throttled += 1,
            Some(_) => burst_other += 1,
        }
    }
    assert_eq!(burst_other, 0, "burst produced only success or throttling");

    // Re-key to a new ARN: the source binding is enforced, migration keeps the payload.
    let second =
        AwsKmsProvider::new(Arc::new(sdk_client(None, None).await), second_arn.clone()).unwrap();
    assert_eq!(
        second.health_check(&ctx).await.unwrap(),
        HealthStatus::Ready
    );
    let result = second.unwrap_dek(blobs[0].wrapped_key(), &ctx).await;
    assert_eq!(class_of(&result), Some(KeyFailureClass::ContextMismatch));
    let mut migrated: EncryptedBlob = blobs[0].clone();
    let rekeyed = rewrap_between(provider.as_ref(), &second, blobs[0].wrapped_key(), &ctx)
        .await
        .unwrap();
    migrated.replace_wrapped_key(rekeyed).unwrap();
    assert!(migrated.ciphertext() == blobs[0].ciphertext());
    assert_eq!(
        decrypt_blob(&second, &migrated, &ctx)
            .await
            .unwrap()
            .as_slice(),
        SECRET
    );
    assert!(decrypt_blob(provider.as_ref(), &migrated, &ctx)
        .await
        .is_err());

    // Pending deletion fails closed; cancellation plus enablement recovers.
    control("schedule-second");
    let pending = eventually("pending deletion", 180, || {
        let second = &second;
        let migrated = &migrated;
        let ctx = &ctx;
        async move { class_of(&decrypt_blob(second, migrated, ctx).await) }
    })
    .await;
    assert_eq!(pending, KeyFailureClass::KeyDisabled);
    control("cancel-second");
    eventually("cancelled deletion", 180, || {
        let second = &second;
        let migrated = &migrated;
        let ctx = &ctx;
        async move { decrypt_blob(second, migrated, ctx).await.ok().map(|_| ()) }
    })
    .await;

    // Redaction: no ARN, account, DEK or payload in Debug/Display output.
    let dek_hex: String = generated
        .plaintext()
        .expose_secret()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let rendered = [
        format!("{provider:?}"),
        format!("{generated:?}"),
        format!("{:?}", blobs[0]),
        format!("{error:?} {error}"),
        format!(
            "{:?}",
            provider
                .unwrap_dek(generated.wrapped_key(), &wrong_purpose)
                .await
        ),
        format!("{:?}", AwsKmsProvider::new(counting.clone(), "x").err()),
    ]
    .join("\n");
    for forbidden in [
        key_arn.as_str(),
        second_arn.as_str(),
        account.as_str(),
        dek_hex.as_str(),
    ] {
        assert!(!rendered.contains(forbidden), "diagnostics are redacted");
    }
    assert!(!rendered.contains(std::str::from_utf8(SECRET).unwrap()));

    let receipt = serde_json::json!({
        "status": "PASS",
        "scope": "actual Rust AWS KMS provider and SDK client against live isolated AWS KMS keys",
        "strategy": "single key per deployment plus canonical encryption-context binding (shared-with-context, context version 1)",
        "startup_canary_generate_decrypt": true,
        "runtime_admin_operations_denied": true,
        "no_dek_cache_decrypt_calls_per_unwrap": counting.decrypts.load(Ordering::SeqCst) == 5,
        "no_dek_cache_after_disable": true,
        "wrong_tenant_user_resource_schema_denied_by_kms": true,
        "wrong_purpose_denied_by_local_binding": true,
        "noncanonical_context_denied_by_iam_condition": true,
        "insufficient_iam_policy_denied": true,
        "underprivileged_role_encrypt_only_verified": true,
        "key_material_id_reported": material_reported,
        "on_demand_rotation_new_key_material_observed": rotated_material.is_some(),
        "old_envelope_decrypt_after_rotation": true,
        "disabled_key_fail_closed": true,
        "interrupted_rewrap_rollback_envelope_unchanged": true,
        "recovery_after_enable": true,
        "same_dek_resumable_rewrap_payload_unchanged": true,
        "pending_deletion_fail_closed": true,
        "pending_deletion_cancel_recovery": true,
        "rekey_to_new_arn_migration": true,
        "source_kek_binding_rejects_cross_key_unwrap": true,
        "sdk_throttling_classified_retryable_fail_closed": true,
        "sdk_throttling_attempts_observed": throttle_attempts,
        "throttling_source": "loopback endpoint returning a KMS ThrottlingException to the real SDK client",
        "real_kms_burst": {"requests": 200, "ok": burst_ok, "throttled": burst_throttled},
        "secret_redaction_debug_display": true,
        "production_deployment_qualified": false
    });
    std::fs::write(
        env("FORTEMI_AWS_LIVE_RECEIPT"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
