//! Provider-neutral key-provider decorator: times every call, records bounded
//! metrics and feeds the cached readiness signal (#1170).
//!
//! Wraps any `KeyProvider` (AWS KMS, OpenBao/Vault Transit, local env), so seal,
//! unseal and rewrap are metered without backend-specific branching.

use std::sync::Arc;
use std::time::{Duration, Instant};

use matric_core::telemetry::{record_kms_call, KmsCall};
use matric_crypto::{
    GeneratedDek, HealthStatus, KeyContext, KeyError, KeyFailureClass, KeyFuture, KeyProvider,
    KeyProviderKind, PlaintextDek, ProviderSignature, RotationInfo, WrappedKey,
};

use super::key_provider_health::{KeyProviderHealth, KeyUse};
use super::user_secrets::key_failure_class_label;

/// Receives every timed call; production uses OpenTelemetry, tests capture.
pub type KmsCallSink = Arc<dyn Fn(KmsCall) + Send + Sync>;

/// Closed outcome classification for one call result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallOutcome {
    Ok,
    Failed {
        class: KeyFailureClass,
        retryable: bool,
    },
}

impl CallOutcome {
    pub fn from_error(error: &KeyError) -> Self {
        Self::Failed {
            class: error.class(),
            retryable: error.is_retryable(),
        }
    }

    /// Health-check results report trouble as `Ok(Degraded | Unavailable)`.
    pub fn from_health(status: HealthStatus) -> Self {
        match status {
            HealthStatus::Ready => Self::Ok,
            HealthStatus::Degraded { class, mode } | HealthStatus::Unavailable { class, mode } => {
                Self::Failed {
                    class,
                    retryable: mode == matric_crypto::DegradedMode::RetryableFailClosed,
                }
            }
        }
    }

    pub fn to_call(self, operation: &'static str, elapsed: Duration) -> KmsCall {
        let (outcome, failure_class, retryability) = match self {
            Self::Ok => ("ok", "none", "none"),
            Self::Failed { class, retryable } => (
                "error",
                key_failure_class_label(class),
                if retryable { "retryable" } else { "terminal" },
            ),
        };
        KmsCall {
            operation,
            outcome,
            failure_class,
            retryability,
            elapsed,
        }
    }
}

/// Decorator that meters an inner provider and updates shared health.
pub struct MeteredKeyProvider {
    inner: Arc<dyn KeyProvider>,
    health: Arc<KeyProviderHealth>,
    sink: KmsCallSink,
}

impl MeteredKeyProvider {
    pub fn new(inner: Arc<dyn KeyProvider>, health: Arc<KeyProviderHealth>) -> Self {
        Self::with_sink(inner, health, Arc::new(record_kms_call))
    }

    pub fn with_sink(
        inner: Arc<dyn KeyProvider>,
        health: Arc<KeyProviderHealth>,
        sink: KmsCallSink,
    ) -> Self {
        Self {
            inner,
            health,
            sink,
        }
    }

    fn observe(&self, operation: &'static str, outcome: CallOutcome, started: Instant) {
        let now = Instant::now();
        match outcome {
            CallOutcome::Ok => self.health.record_success(now),
            CallOutcome::Failed { class, .. } => {
                let key_use = match operation {
                    "unseal" | "rewrap" => KeyUse::ExistingData,
                    _ => KeyUse::CurrentKey,
                };
                self.health.record_failure_for(class, key_use, now);
            }
        }
        (self.sink)(outcome.to_call(operation, now.saturating_duration_since(started)));
    }

    fn metered<'a, T: Send + 'a>(
        &'a self,
        operation: &'static str,
        call: KeyFuture<'a, T>,
    ) -> KeyFuture<'a, T> {
        Box::pin(async move {
            let started = Instant::now();
            let result = call.await;
            let outcome = match &result {
                Ok(_) => CallOutcome::Ok,
                Err(error) => CallOutcome::from_error(error),
            };
            self.observe(operation, outcome, started);
            result
        })
    }
}

impl KeyProvider for MeteredKeyProvider {
    fn kind(&self) -> KeyProviderKind {
        self.inner.kind()
    }

    fn wrap_dek<'a>(
        &'a self,
        plaintext_dek: &'a PlaintextDek,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        self.metered("seal", self.inner.wrap_dek(plaintext_dek, context))
    }

    fn unwrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, PlaintextDek> {
        self.metered("unseal", self.inner.unwrap_dek(wrapped, context))
    }

    fn generate_dek<'a>(
        &'a self,
        context: &'a KeyContext,
        bytes: usize,
    ) -> KeyFuture<'a, GeneratedDek> {
        self.metered("seal", self.inner.generate_dek(context, bytes))
    }

    fn rewrap_dek<'a>(
        &'a self,
        wrapped: &'a WrappedKey,
        context: &'a KeyContext,
    ) -> KeyFuture<'a, WrappedKey> {
        self.metered("rewrap", self.inner.rewrap_dek(wrapped, context))
    }

    fn sign<'a>(
        &'a self,
        context: &'a KeyContext,
        data: &'a [u8],
    ) -> KeyFuture<'a, ProviderSignature> {
        self.metered("sign", self.inner.sign(context, data))
    }

    fn verify<'a>(
        &'a self,
        context: &'a KeyContext,
        data: &'a [u8],
        signature: &'a ProviderSignature,
    ) -> KeyFuture<'a, bool> {
        self.metered("verify", self.inner.verify(context, data, signature))
    }

    fn rotate<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, RotationInfo> {
        self.metered("rotate", self.inner.rotate(context))
    }

    fn health_check<'a>(&'a self, context: &'a KeyContext) -> KeyFuture<'a, HealthStatus> {
        Box::pin(async move {
            let started = Instant::now();
            let result = self.inner.health_check(context).await;
            let outcome = match &result {
                Ok(status) => CallOutcome::from_health(*status),
                Err(error) => CallOutcome::from_error(error),
            };
            self.observe("health_canary", outcome, started);
            result
        })
    }
}

/// Run the low-rate canary: at most one provider call per interval, skipped
/// while real traffic keeps proving the provider healthy. Returns immediately
/// when the canary is disabled.
pub async fn run_key_provider_canary(
    provider: Arc<dyn KeyProvider>,
    health: Arc<KeyProviderHealth>,
    context: KeyContext,
) {
    let Some(interval) = health.canary_interval() else {
        return;
    };
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        run_canary_once(provider.as_ref(), &health, &context).await;
    }
}

/// One canary step; returns whether the provider was called.
pub async fn run_canary_once(
    provider: &dyn KeyProvider,
    health: &KeyProviderHealth,
    context: &KeyContext,
) -> bool {
    let now = Instant::now();
    if !health.canary_due(now) {
        return false;
    }
    health.mark_canary(now);
    // The metered provider records the outcome; the result itself is not needed.
    let _ = provider.health_check(context).await;
    true
}

#[cfg(test)]
#[path = "metered_key_provider_tests.rs"]
mod tests;
