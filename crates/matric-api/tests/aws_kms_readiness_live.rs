//! Explicit live AWS KMS readiness/metrics profile (Fortemi-Enterprise/kms#4, #1170).
//! Run through enterprise-kms/scripts/qualify-aws.py, which supplies short-lived
//! role credentials, the qualification key ARN and the key-state control script.
//!
//! Composes the production pieces exactly as `src/kms.rs` does — the AWS provider
//! from the standard credential chain, wrapped by `MeteredKeyProvider` with the
//! real OpenTelemetry sink and a `KeyProviderHealth` — and evaluates `/readyz`
//! through the same `readiness_response` the handler calls. `kms.rs` is private
//! to the binary, and a full API process would also need PostgreSQL, Redis and
//! the audit sink just to reach `/readyz`; none of those affect this signal.
#![cfg(all(feature = "kms-aws", feature = "otel"))]

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use matric_api::services::key_provider_health::{
    readiness_response, KeyHealthSnapshot, KeyProviderHealth,
};
use matric_api::services::metered_key_provider::{run_canary_once, MeteredKeyProvider};
use matric_crypto::{
    decrypt_blob, encrypt_blob, AwsKmsProvider, AwsSdkKmsClient, KeyContext, KeyFailureClass,
    KeyProvider, KeyPurpose,
};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

const CANARY_TENANT: &str = "00000000-0000-0000-0000-000000000001";
const CANARY_USER: &str = "00000000-0000-0000-0000-000000000002";
const DATA_TENANT: &str = "6d1b3f0e-4a8c-4c55-9a51-kms4readiness";

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

/// Same fixed context as the hosted canary in `src/kms.rs`.
fn canary_context() -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, "hosted_startup_canary")
        .and_then(|c| c.with_tenant_id(CANARY_TENANT))
        .and_then(|c| c.with_user_id(CANARY_USER))
        .and_then(|c| c.with_resource_id("kms_health"))
        .unwrap()
}

fn data_context() -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, "user_secrets")
        .and_then(|c| c.with_tenant_id(DATA_TENANT))
        .and_then(|c| c.with_user_id("readiness-user"))
        .and_then(|c| c.with_resource_id("readiness-secret"))
        .unwrap()
}

/// Health whose canary is already due (as after a canary interval has elapsed).
fn due_health() -> Arc<KeyProviderHealth> {
    let past = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .expect("monotonic clock");
    Arc::new(KeyProviderHealth::new(Some(Duration::from_secs(10)), past))
}

fn readyz(health: &KeyProviderHealth) -> (StatusCode, serde_json::Value) {
    readiness_response(true, Some(health.snapshot(Instant::now())))
}

async fn aws_provider(endpoint: Option<&str>, key_arn: &str) -> Arc<dyn KeyProvider> {
    match endpoint {
        None => Arc::new(AwsKmsProvider::from_environment(key_arn).await.unwrap()),
        Some(endpoint) => {
            let config = aws_config::defaults(aws_config::BehaviorVersion::v2026_01_12())
                .endpoint_url(endpoint)
                .load()
                .await;
            let client = AwsSdkKmsClient::new(aws_sdk_kms::Client::new(&config));
            Arc::new(AwsKmsProvider::new(Arc::new(client), key_arn).unwrap())
        }
    }
}

/// Loopback endpoint answering every request with one KMS error type.
fn fault_endpoint(error_type: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
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
            let body = format!(r#"{{"__type":"{error_type}","message":"injected"}}"#);
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/x-amz-json-1.1\r\n\
                 x-amzn-RequestId: 00000000-0000-0000-0000-000000000000\r\n\
                 x-amzn-ErrorType: {error_type}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    address
}

/// Poll until `check` is true, tolerating KMS key-state propagation delay.
async fn eventually<F, Fut>(what: &str, seconds: u64, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while !check().await {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// `fortemi.kms.operations` data points as attribute maps.
fn kms_points(metrics: &[ResourceMetrics]) -> Vec<BTreeMap<String, String>> {
    let mut points = Vec::new();
    for resource in metrics {
        for scope in resource.scope_metrics() {
            for metric in scope.metrics() {
                if metric.name() != "fortemi.kms.operations" {
                    continue;
                }
                if let AggregatedMetrics::U64(MetricData::Sum(sum)) = metric.data() {
                    for point in sum.data_points() {
                        points.push(
                            point
                                .attributes()
                                .map(|kv| (kv.key.to_string(), kv.value.to_string()))
                                .collect(),
                        );
                    }
                }
            }
        }
    }
    points
}

fn has_point(points: &[BTreeMap<String, String>], expected: [&str; 4]) -> bool {
    points.iter().any(|point| {
        point.get("fortemi.kms.operation").map(String::as_str) == Some(expected[0])
            && point.get("outcome").map(String::as_str) == Some(expected[1])
            && point.get("fortemi.kms.failure_class").map(String::as_str) == Some(expected[2])
            && point.get("fortemi.kms.retryability").map(String::as_str) == Some(expected[3])
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires explicit live AWS KMS runner"]
async fn live_aws_kms_readiness_and_metrics() {
    let key_arn = env("FORTEMI_AWS_KMS_KEY_ID");
    let account = key_arn
        .split(':')
        .nth(4)
        .expect("key ARN account")
        .to_owned();
    // Real OTel pipeline: the production sink records into the global meter.
    let exporter = InMemoryMetricExporter::default();
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let inner = aws_provider(None, &key_arn).await;
    let canary = canary_context();
    let ctx = data_context();

    // Healthy: canary, seal, unseal and rewrap succeed; /readyz is 200 + ready.
    let health = due_health();
    let metered = MeteredKeyProvider::new(inner.clone(), health.clone());
    assert!(
        run_canary_once(&metered, &health, &canary).await,
        "canary ran"
    );
    assert_eq!(health.snapshot(Instant::now()), KeyHealthSnapshot::Ready);
    let blob = encrypt_blob(&metered, b"readiness qualification", &ctx)
        .await
        .unwrap();
    assert_eq!(
        decrypt_blob(&metered, &blob, &ctx)
            .await
            .unwrap()
            .as_slice(),
        b"readiness qualification"
    );
    metered.rewrap_dek(blob.wrapped_key(), &ctx).await.unwrap();
    let (status, body) = readyz(&health);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["key_provider"]["status"], "ready");

    // Disabled key: real seal traffic and the canary each make readiness 503.
    control("disable");
    eventually("seal fails with key_disabled", 180, || {
        let metered = &metered;
        let ctx = &ctx;
        async move { metered.generate_dek(ctx, 32).await.is_err() }
    })
    .await;
    assert_eq!(
        health.snapshot(Instant::now()),
        KeyHealthSnapshot::Unavailable(KeyFailureClass::KeyDisabled)
    );
    let (status, body) = readyz(&health);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["reason_code"], "required_dependency_unavailable");
    assert_eq!(body["key_provider"]["status"], "unavailable");
    assert_eq!(body["key_provider"]["reason_code"], "key_disabled");
    let canary_health = due_health();
    let canary_metered = MeteredKeyProvider::new(inner.clone(), canary_health.clone());
    assert!(run_canary_once(&canary_metered, &canary_health, &canary).await);
    assert_eq!(
        canary_health.snapshot(Instant::now()),
        KeyHealthSnapshot::Unavailable(KeyFailureClass::KeyDisabled)
    );
    assert_eq!(readyz(&canary_health).0, StatusCode::SERVICE_UNAVAILABLE);

    // Re-enable: the canary alone returns readiness to Ready; so does real traffic.
    control("enable");
    eventually("canary recovery", 240, || {
        let canary_metered = &canary_metered;
        let canary_health = &canary_health;
        let canary = &canary;
        async move {
            run_canary_once(canary_metered, canary_health, canary).await;
            canary_health.snapshot(Instant::now()) == KeyHealthSnapshot::Ready
        }
    })
    .await;
    assert_eq!(readyz(&canary_health).0, StatusCode::OK);
    eventually("seal recovery", 120, || {
        let metered = &metered;
        let ctx = &ctx;
        async move { metered.generate_dek(ctx, 32).await.is_ok() }
    })
    .await;
    let (status, body) = readyz(&health);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["key_provider"]["status"], "ready");

    // Throttling (loopback fault through the real SDK client): Degraded, still 200.
    let throttle_health = Arc::new(KeyProviderHealth::new(
        Some(Duration::from_secs(10)),
        Instant::now(),
    ));
    let throttled = MeteredKeyProvider::new(
        aws_provider(Some(&fault_endpoint("ThrottlingException")), &key_arn).await,
        throttle_health.clone(),
    );
    let error = throttled.generate_dek(&ctx, 32).await.unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::Throttled);
    assert_eq!(
        throttle_health.snapshot(Instant::now()),
        KeyHealthSnapshot::Degraded(KeyFailureClass::Throttled)
    );
    let (status, body) = readyz(&throttle_health);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["key_provider"]["status"], "degraded");
    assert_eq!(body["key_provider"]["reason_code"], "throttled");

    // KeyVersionUnavailable (NotFoundException) on unseal of existing data is
    // per-request and leaves readiness Ready; on seal it is Unavailable.
    let missing_health = Arc::new(KeyProviderHealth::new(
        Some(Duration::from_secs(10)),
        Instant::now(),
    ));
    let missing = MeteredKeyProvider::new(
        aws_provider(Some(&fault_endpoint("NotFoundException")), &key_arn).await,
        missing_health.clone(),
    );
    let error = decrypt_blob(&missing, &blob, &ctx).await.unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::KeyVersionUnavailable);
    assert_eq!(
        missing_health.snapshot(Instant::now()),
        KeyHealthSnapshot::Ready
    );
    assert_eq!(readyz(&missing_health).0, StatusCode::OK);
    let error = missing.generate_dek(&ctx, 32).await.unwrap_err();
    assert_eq!(error.class(), KeyFailureClass::KeyVersionUnavailable);
    let (status, body) = readyz(&missing_health);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["key_provider"]["reason_code"],
        "key_version_unavailable"
    );

    // Metrics: every operation recorded with only the closed four-label set.
    meter_provider.force_flush().unwrap();
    let exported = exporter.get_finished_metrics().unwrap();
    let latest = exported.last().expect("metrics exported");
    let points = kms_points(std::slice::from_ref(latest));
    for expected in [
        ["health_canary", "ok", "none", "none"],
        ["seal", "ok", "none", "none"],
        ["unseal", "ok", "none", "none"],
        ["rewrap", "ok", "none", "none"],
        ["seal", "error", "key_disabled", "terminal"],
        ["health_canary", "error", "key_disabled", "terminal"],
        ["seal", "error", "throttled", "retryable"],
        ["unseal", "error", "key_version_unavailable", "terminal"],
        ["seal", "error", "key_version_unavailable", "terminal"],
    ] {
        assert!(has_point(&points, expected), "metric point {expected:?}");
    }
    let label_keys: BTreeSet<&str> = points
        .iter()
        .flat_map(|point| point.keys().map(String::as_str))
        .collect();
    assert_eq!(
        label_keys,
        BTreeSet::from([
            "fortemi.kms.failure_class",
            "fortemi.kms.operation",
            "fortemi.kms.retryability",
            "outcome",
        ])
    );
    let dump = format!("{latest:?}");
    assert!(dump.contains("fortemi.kms.operation.duration"));
    for forbidden in [
        key_arn.as_str(),
        account.as_str(),
        CANARY_TENANT,
        CANARY_USER,
        DATA_TENANT,
        "readiness-user",
        "readiness-secret",
        "user_secrets",
        "hosted_startup_canary",
        "kms_health",
    ] {
        assert!(!dump.contains(forbidden), "metric labels are bounded");
    }

    let operations: BTreeSet<&str> = points
        .iter()
        .filter_map(|point| point.get("fortemi.kms.operation").map(String::as_str))
        .collect();
    let receipt = serde_json::json!({
        "status": "PASS",
        "scope": "production AWS provider wrapped by MeteredKeyProvider + KeyProviderHealth, readiness_response, real OpenTelemetry SDK export",
        "healthy_ready_200": true,
        "disabled_key_seal_unavailable_503": true,
        "disabled_key_canary_unavailable_503": true,
        "readyz_reason_code": "key_disabled",
        "reenable_canary_recovers_ready_200": true,
        "reenable_seal_recovers_ready_200": true,
        "throttling_degraded_still_200": true,
        "throttling_source": "loopback ThrottlingException through the real SDK client",
        "key_version_unavailable_unseal_keeps_ready": true,
        "key_version_unavailable_seal_unavailable_503": true,
        "key_version_unavailable_source": "loopback NotFoundException through the real SDK client",
        "metric_names": ["fortemi.kms.operations", "fortemi.kms.operation.duration"],
        "metric_operations_observed": operations,
        "metric_label_keys": label_keys,
        "metric_labels_exclude_arn_account_tenant_context": true
    });
    std::fs::write(
        env("FORTEMI_AWS_LIVE_READINESS_RECEIPT"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
