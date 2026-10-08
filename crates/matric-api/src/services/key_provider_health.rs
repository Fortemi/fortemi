//! Bounded, cached key-provider health signal used by `/readyz` (#1170).
//!
//! Fed by the outcomes of real seal/unseal/rewrap calls and a low-rate canary.
//! Readiness reads the cached state and never calls the provider. The state is a
//! fixed-size enum plus timestamps: it holds no key reference, tenant, user or
//! context value.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use matric_crypto::KeyFailureClass;
use serde_json::{json, Value};

use super::user_secrets::key_failure_class_label;

/// Default interval for the periodic health canary.
pub const DEFAULT_CANARY_INTERVAL: Duration = Duration::from_secs(60);
/// Smallest accepted non-zero canary interval.
pub const MIN_CANARY_INTERVAL: Duration = Duration::from_secs(10);
/// Degraded signals expire after this long when the canary is disabled.
const DISABLED_CANARY_DEGRADED_TTL: Duration = Duration::from_secs(120);

/// How a call outcome changes provider health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthEffect {
    /// The provider answered; clear any degraded or unavailable state.
    Recover,
    /// Transient provider trouble: keep serving, report degraded.
    Degrade,
    /// The key or credentials cannot be used: readiness fails until recovery.
    Unavailable,
    /// A per-request input problem that says nothing about provider health.
    Ignore,
}

/// Map the provider-neutral failure taxonomy onto readiness effects.
pub fn health_effect(class: KeyFailureClass) -> HealthEffect {
    match class {
        KeyFailureClass::Throttled
        | KeyFailureClass::ProviderUnavailable
        | KeyFailureClass::ProviderFailure => HealthEffect::Degrade,
        KeyFailureClass::KeyDisabled
        | KeyFailureClass::AccessDenied
        | KeyFailureClass::KeyVersionUnavailable
        | KeyFailureClass::InvalidConfiguration => HealthEffect::Unavailable,
        KeyFailureClass::InvalidContext
        | KeyFailureClass::ContextMismatch
        | KeyFailureClass::InvalidCiphertext
        | KeyFailureClass::UnsupportedVersion
        | KeyFailureClass::UnsupportedOperation => HealthEffect::Ignore,
    }
}

/// Point-in-time health as reported by readiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyHealthSnapshot {
    Ready,
    Degraded(KeyFailureClass),
    Unavailable(KeyFailureClass),
}

impl KeyHealthSnapshot {
    /// Degraded providers still serve traffic; only `Unavailable` fails readiness.
    pub fn permits_readiness(self) -> bool {
        !matches!(self, Self::Unavailable(_))
    }

    /// Readiness body fragment: closed vocabulary only.
    pub fn to_json(self) -> Value {
        match self {
            Self::Ready => json!({ "status": "ready" }),
            Self::Degraded(class) => json!({
                "status": "degraded",
                "reason_code": key_failure_class_label(class),
            }),
            Self::Unavailable(class) => json!({
                "status": "unavailable",
                "reason_code": key_failure_class_label(class),
            }),
        }
    }
}

struct HealthState {
    snapshot: KeyHealthSnapshot,
    changed_at: Instant,
    last_success: Option<Instant>,
    last_canary: Option<Instant>,
}

/// Shared, lock-protected key-provider health.
pub struct KeyProviderHealth {
    canary_interval: Option<Duration>,
    degraded_ttl: Duration,
    state: Mutex<HealthState>,
}

impl KeyProviderHealth {
    /// `canary_interval = None` disables the periodic canary. The provider is
    /// assumed ready because hosted startup already passed a fail-closed canary.
    pub fn new(canary_interval: Option<Duration>, now: Instant) -> Self {
        let degraded_ttl = canary_interval
            .map(|interval| interval * 2)
            .unwrap_or(DISABLED_CANARY_DEGRADED_TTL);
        Self {
            canary_interval,
            degraded_ttl,
            state: Mutex::new(HealthState {
                snapshot: KeyHealthSnapshot::Ready,
                changed_at: now,
                last_success: Some(now),
                last_canary: Some(now),
            }),
        }
    }

    pub fn canary_interval(&self) -> Option<Duration> {
        self.canary_interval
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HealthState> {
        // The state is always internally consistent; recover from poisoning.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn record_success(&self, now: Instant) {
        let mut state = self.lock();
        if state.snapshot != KeyHealthSnapshot::Ready {
            state.snapshot = KeyHealthSnapshot::Ready;
            state.changed_at = now;
        }
        state.last_success = Some(now);
    }

    pub fn record_failure(&self, class: KeyFailureClass, now: Instant) {
        let next = match health_effect(class) {
            HealthEffect::Ignore | HealthEffect::Recover => return,
            HealthEffect::Degrade => KeyHealthSnapshot::Degraded(class),
            HealthEffect::Unavailable => KeyHealthSnapshot::Unavailable(class),
        };
        let mut state = self.lock();
        // A transient error does not mask a known terminal key problem.
        if matches!(state.snapshot, KeyHealthSnapshot::Unavailable(_))
            && matches!(next, KeyHealthSnapshot::Degraded(_))
        {
            return;
        }
        state.snapshot = next;
        state.changed_at = now;
    }

    /// Cached health. Degraded signals older than the TTL decay to ready.
    pub fn snapshot(&self, now: Instant) -> KeyHealthSnapshot {
        let mut state = self.lock();
        if matches!(state.snapshot, KeyHealthSnapshot::Degraded(_))
            && now.saturating_duration_since(state.changed_at) >= self.degraded_ttl
        {
            state.snapshot = KeyHealthSnapshot::Ready;
            state.changed_at = now;
        }
        state.snapshot
    }

    /// At most one canary per interval, and only when real traffic has not
    /// recently proven the provider healthy.
    pub fn canary_due(&self, now: Instant) -> bool {
        let Some(interval) = self.canary_interval else {
            return false;
        };
        let state = self.lock();
        let elapsed =
            |at: Option<Instant>| at.is_none_or(|at| now.saturating_duration_since(at) >= interval);
        elapsed(state.last_canary)
            && (state.snapshot != KeyHealthSnapshot::Ready || elapsed(state.last_success))
    }

    pub fn mark_canary(&self, now: Instant) {
        self.lock().last_canary = Some(now);
    }
}

/// Parse `FORTEMI_KMS_HEALTH_CANARY_SECS`: unset → default, `0` → disabled.
pub fn parse_canary_interval(value: Option<&str>) -> Result<Option<Duration>, &'static str> {
    let Some(value) = value else {
        return Ok(Some(DEFAULT_CANARY_INTERVAL));
    };
    let seconds: u64 = value
        .trim()
        .parse()
        .map_err(|_| "FORTEMI_KMS_HEALTH_CANARY_SECS must be a whole number of seconds")?;
    match seconds {
        0 => Ok(None),
        s if Duration::from_secs(s) < MIN_CANARY_INTERVAL => {
            Err("FORTEMI_KMS_HEALTH_CANARY_SECS must be 0 or at least 10")
        }
        s => Ok(Some(Duration::from_secs(s))),
    }
}

/// `/readyz` response once lifecycle checks pass. The `key_provider` member is
/// present only when hosted key custody is configured, so CE bodies are unchanged.
pub fn readiness_response(
    dependencies_ready: bool,
    key_health: Option<KeyHealthSnapshot>,
) -> (StatusCode, Value) {
    let key_ready = key_health.is_none_or(KeyHealthSnapshot::permits_readiness);
    let mut body = if dependencies_ready && key_ready {
        json!({ "status": "ready" })
    } else {
        json!({ "status": "not_ready", "reason_code": "required_dependency_unavailable" })
    };
    if let Some(snapshot) = key_health {
        body["key_provider"] = snapshot.to_json();
    }
    let status = if dependencies_ready && key_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, body)
}

#[cfg(test)]
#[path = "key_provider_health_tests.rs"]
mod tests;
