//! Decorator tests: metric records, health feeding and readiness with a fake
//! failing provider.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use axum::http::StatusCode;
use matric_crypto::{decrypt_blob, encrypt_blob, DeploymentMode, EnvKeyProvider, KeyPurpose};
use zeroize::Zeroizing;

use super::*;
use crate::services::key_provider_health::{readiness_response, KeyHealthSnapshot};

/// Local provider whose calls can be forced to fail with a chosen class.
struct FlakyProvider {
    inner: EnvKeyProvider,
    failure: Mutex<Option<KeyFailureClass>>,
    calls: AtomicUsize,
}

impl FlakyProvider {
    fn new() -> Self {
        Self {
            inner: EnvKeyProvider::new(
                Zeroizing::new([3u8; 32]),
                "metered-test-kek",
                1,
                DeploymentMode::Development,
            )
            .unwrap(),
            failure: Mutex::new(None),
            calls: AtomicUsize::new(0),
        }
    }

    fn fail_with(&self, class: Option<KeyFailureClass>) {
        *self.failure.lock().unwrap() = class;
    }

    fn gate(&self, operation: matric_crypto::KeyOperation) -> Result<(), KeyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match *self.failure.lock().unwrap() {
            Some(class) => Err(KeyError::new(operation, class)),
            None => Ok(()),
        }
    }
}

impl KeyProvider for FlakyProvider {
    fn kind(&self) -> KeyProviderKind {
        self.inner.kind()
    }
    fn wrap_dek<'a>(&'a self, d: &'a PlaintextDek, c: &'a KeyContext) -> KeyFuture<'a, WrappedKey> {
        Box::pin(async move {
            self.gate(matric_crypto::KeyOperation::WrapDek)?;
            self.inner.wrap_dek(d, c).await
        })
    }
    fn unwrap_dek<'a>(
        &'a self,
        w: &'a WrappedKey,
        c: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        Box::pin(async move {
            self.gate(matric_crypto::KeyOperation::UnwrapDek)?;
            self.inner.unwrap_dek(w, c).await
        })
    }
    fn generate_dek<'a>(&'a self, c: &'a KeyContext, n: usize) -> KeyFuture<'a, GeneratedDek> {
        Box::pin(async move {
            self.gate(matric_crypto::KeyOperation::GenerateDek)?;
            self.inner.generate_dek(c, n).await
        })
    }
    fn rotate<'a>(&'a self, c: &'a KeyContext) -> KeyFuture<'a, RotationInfo> {
        self.inner.rotate(c)
    }
    fn health_check<'a>(&'a self, c: &'a KeyContext) -> KeyFuture<'a, HealthStatus> {
        Box::pin(async move {
            self.gate(matric_crypto::KeyOperation::HealthCheck)?;
            self.inner.health_check(c).await
        })
    }
}

struct Harness {
    fake: Arc<FlakyProvider>,
    health: Arc<KeyProviderHealth>,
    metered: MeteredKeyProvider,
    calls: Arc<Mutex<Vec<KmsCall>>>,
}

fn harness(started: Instant) -> Harness {
    let fake = Arc::new(FlakyProvider::new());
    let health = Arc::new(KeyProviderHealth::new(
        Some(Duration::from_secs(10)),
        started,
    ));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let sink_calls = calls.clone();
    let metered = MeteredKeyProvider::with_sink(
        fake.clone(),
        health.clone(),
        Arc::new(move |call| sink_calls.lock().unwrap().push(call)),
    );
    Harness {
        fake,
        health,
        metered,
        calls,
    }
}

fn context() -> KeyContext {
    KeyContext::new(KeyPurpose::USER_SECRET, "metered_test")
        .and_then(|c| c.with_tenant_id("00000000-0000-0000-0000-0000000000aa"))
        .and_then(|c| c.with_resource_id("resource-sensitive-id"))
        .unwrap()
}

fn labels(calls: &[KmsCall]) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    calls
        .iter()
        .map(|c| (c.operation, c.outcome, c.failure_class, c.retryability))
        .collect()
}

#[tokio::test]
async fn seal_unseal_and_rewrap_record_outcome_and_latency() {
    let h = harness(Instant::now());
    let ctx = context();
    let blob = encrypt_blob(&h.metered, b"payload", &ctx).await.unwrap();
    decrypt_blob(&h.metered, &blob, &ctx).await.unwrap();
    h.metered
        .rewrap_dek(blob.wrapped_key(), &ctx)
        .await
        .unwrap();

    let calls = h.calls.lock().unwrap().clone();
    assert_eq!(
        labels(&calls),
        vec![
            ("seal", "ok", "none", "none"),
            ("unseal", "ok", "none", "none"),
            ("rewrap", "ok", "none", "none"),
        ]
    );
    // Labels are closed vocabularies: no tenant, resource or key reference.
    for call in &calls {
        let rendered = format!("{call:?}");
        assert!(!rendered.contains("0000000000aa"));
        assert!(!rendered.contains("resource-sensitive-id"));
        assert!(!rendered.contains("metered-test-kek"));
    }
}

#[tokio::test]
async fn failures_are_classified_retryable_or_terminal_and_feed_health() {
    let h = harness(Instant::now());
    let ctx = context();
    let blob = encrypt_blob(&h.metered, b"payload", &ctx).await.unwrap();

    h.fake.fail_with(Some(KeyFailureClass::Throttled));
    assert!(encrypt_blob(&h.metered, b"payload", &ctx).await.is_err());
    let snapshot = h.health.snapshot(Instant::now());
    assert_eq!(
        snapshot,
        KeyHealthSnapshot::Degraded(KeyFailureClass::Throttled)
    );
    assert_eq!(readiness_response(true, Some(snapshot)).0, StatusCode::OK);

    h.fake.fail_with(Some(KeyFailureClass::KeyDisabled));
    assert!(decrypt_blob(&h.metered, &blob, &ctx).await.is_err());
    let snapshot = h.health.snapshot(Instant::now());
    let (status, body) = readiness_response(true, Some(snapshot));
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["key_provider"]["reason_code"], "key_disabled");

    let calls = h.calls.lock().unwrap().clone();
    assert_eq!(
        labels(&calls)[1..],
        [
            ("seal", "error", "throttled", "retryable"),
            ("unseal", "error", "key_disabled", "terminal"),
        ]
    );
}

#[tokio::test]
async fn readiness_reads_the_cache_and_canary_recovers_a_failed_provider() {
    let started = Instant::now().checked_sub(Duration::from_secs(30)).unwrap();
    let h = harness(started);
    let ctx = context();

    h.fake.fail_with(Some(KeyFailureClass::AccessDenied));
    assert!(run_canary_once(&h.metered, &h.health, &ctx).await);
    let after_canary = h.fake.calls.load(Ordering::SeqCst);
    for _ in 0..100 {
        let snapshot = h.health.snapshot(Instant::now());
        assert_eq!(
            readiness_response(true, Some(snapshot)).0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    assert_eq!(
        h.fake.calls.load(Ordering::SeqCst),
        after_canary,
        "probes must not call the provider"
    );
    // Within the same interval the canary does not run again.
    assert!(!run_canary_once(&h.metered, &h.health, &ctx).await);

    // A later successful real operation restores readiness automatically.
    h.fake.fail_with(None);
    encrypt_blob(&h.metered, b"payload", &ctx).await.unwrap();
    let snapshot = h.health.snapshot(Instant::now());
    assert_eq!(snapshot, KeyHealthSnapshot::Ready);
    assert_eq!(readiness_response(true, Some(snapshot)).0, StatusCode::OK);

    let calls = h.calls.lock().unwrap().clone();
    assert_eq!(
        labels(&calls)[0],
        ("health_canary", "error", "access_denied", "terminal")
    );
}

#[test]
fn health_status_results_map_to_outcomes() {
    assert_eq!(
        CallOutcome::from_health(HealthStatus::Ready),
        CallOutcome::Ok
    );
    let degraded = HealthStatus::Degraded {
        class: KeyFailureClass::Throttled,
        mode: matric_crypto::DegradedMode::RetryableFailClosed,
    };
    assert_eq!(
        CallOutcome::from_health(degraded),
        CallOutcome::Failed {
            class: KeyFailureClass::Throttled,
            retryable: true
        }
    );
    let call = CallOutcome::from_health(degraded).to_call("health_canary", Duration::ZERO);
    assert_eq!(call.retryability, "retryable");
}
