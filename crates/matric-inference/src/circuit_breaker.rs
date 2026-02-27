//! Circuit breaker pattern for sidecar HTTP clients.
//!
//! Prevents wasting worker slots on requests to downed sidecars. When a
//! sidecar is down, the circuit opens after N consecutive failures and
//! fast-fails subsequent requests until a cooldown probe succeeds.
//!
//! # States
//!
//! - **Closed** (normal): requests pass through
//! - **Open** (after N failures): fail fast without making HTTP call
//! - **HalfOpen** (after cooldown): allow one probe request to check recovery
//!
//! # Thread Safety
//!
//! All state is behind `Arc<Mutex<_>>` so the breaker can be shared across
//! async tasks and cloned into retry closures.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Tripped — fail fast without making HTTP calls.
    Open,
    /// Cooldown expired — allow one probe request.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
            Self::Open => write!(f, "open"),
            Self::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Configuration for circuit breaker behavior.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// Duration to wait before allowing a probe request (half-open).
    pub cooldown: Duration,
    /// Human-readable service name for logging.
    pub service_name: String,
}

impl CircuitBreakerConfig {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            failure_threshold: 3,
            cooldown: Duration::from_secs(30),
            service_name: service_name.into(),
        }
    }
}

/// Internal mutable state of the circuit breaker.
#[derive(Debug)]
struct BreakerState {
    state: CircuitState,
    consecutive_failures: u32,
    last_failure_time: Option<Instant>,
    total_trips: u64,
}

impl Default for BreakerState {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_failure_time: None,
            total_trips: 0,
        }
    }
}

/// Thread-safe circuit breaker.
///
/// Clone is cheap (Arc internals). Designed to be stored alongside HTTP clients.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<BreakerState>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(BreakerState::default())),
        }
    }

    /// Get the current state of the circuit breaker.
    ///
    /// Automatically transitions Open → HalfOpen if cooldown has elapsed.
    pub fn current_state(&self) -> CircuitState {
        let mut inner = self.state.lock().unwrap();
        self.maybe_transition_to_half_open(&mut inner);
        inner.state
    }

    /// Check if a request is allowed through the circuit breaker.
    ///
    /// Returns `Ok(())` if the request can proceed, or `Err` with a fast-fail
    /// error if the circuit is open.
    pub fn check_request(&self) -> matric_core::Result<()> {
        let mut inner = self.state.lock().unwrap();
        self.maybe_transition_to_half_open(&mut inner);

        match inner.state {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                debug!(
                    service = %self.config.service_name,
                    "Circuit half-open, allowing probe request"
                );
                Ok(())
            }
            CircuitState::Open => {
                let remaining = inner
                    .last_failure_time
                    .map(|t| {
                        self.config
                            .cooldown
                            .checked_sub(t.elapsed())
                            .unwrap_or(Duration::ZERO)
                    })
                    .unwrap_or(Duration::ZERO);

                Err(matric_core::Error::Internal(format!(
                    "{} circuit breaker is open (fast-fail). \
                     {} consecutive failures, cooldown remaining: {:.0}s. \
                     The sidecar may be down — it will be probed after cooldown.",
                    self.config.service_name,
                    inner.consecutive_failures,
                    remaining.as_secs_f64()
                )))
            }
        }
    }

    /// Record a successful request. Resets the failure counter and closes the circuit.
    pub fn record_success(&self) {
        let mut inner = self.state.lock().unwrap();
        if inner.state != CircuitState::Closed {
            info!(
                service = %self.config.service_name,
                previous_state = %inner.state,
                "Circuit breaker closing after successful request"
            );
        }
        inner.consecutive_failures = 0;
        inner.state = CircuitState::Closed;
        inner.last_failure_time = None;
    }

    /// Record a failed request. May trip the circuit to Open.
    pub fn record_failure(&self) {
        let mut inner = self.state.lock().unwrap();
        inner.consecutive_failures += 1;
        inner.last_failure_time = Some(Instant::now());

        if inner.consecutive_failures >= self.config.failure_threshold
            && inner.state != CircuitState::Open
        {
            inner.state = CircuitState::Open;
            inner.total_trips += 1;
            warn!(
                service = %self.config.service_name,
                consecutive_failures = inner.consecutive_failures,
                total_trips = inner.total_trips,
                cooldown_secs = self.config.cooldown.as_secs(),
                "Circuit breaker OPEN — fast-failing requests"
            );
        }
    }

    /// Get the total number of times the circuit has tripped.
    pub fn total_trips(&self) -> u64 {
        self.state.lock().unwrap().total_trips
    }

    /// Get the number of consecutive failures.
    pub fn consecutive_failures(&self) -> u32 {
        self.state.lock().unwrap().consecutive_failures
    }

    /// Reset the circuit breaker to its initial state.
    pub fn reset(&self) {
        let mut inner = self.state.lock().unwrap();
        *inner = BreakerState::default();
        info!(
            service = %self.config.service_name,
            "Circuit breaker reset to closed state"
        );
    }

    /// Transition from Open → HalfOpen if cooldown has elapsed.
    fn maybe_transition_to_half_open(&self, inner: &mut BreakerState) {
        if inner.state == CircuitState::Open {
            if let Some(last_failure) = inner.last_failure_time {
                if last_failure.elapsed() >= self.config.cooldown {
                    debug!(
                        service = %self.config.service_name,
                        "Cooldown elapsed, transitioning to half-open"
                    );
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_breaker(service: &str) -> CircuitBreaker {
        CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_millis(100),
            service_name: service.to_string(),
        })
    }

    #[test]
    fn test_initial_state_is_closed() {
        let cb = test_breaker("test");
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check_request().is_ok());
    }

    #[test]
    fn test_stays_closed_below_threshold() {
        let cb = test_breaker("test");

        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check_request().is_ok());

        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.check_request().is_ok());
    }

    #[test]
    fn test_opens_at_threshold() {
        let cb = test_breaker("test");

        cb.record_failure();
        cb.record_failure();
        cb.record_failure();

        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(cb.check_request().is_err());
    }

    #[test]
    fn test_open_error_message() {
        let cb = test_breaker("whisper");

        for _ in 0..3 {
            cb.record_failure();
        }

        let err = cb.check_request().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("whisper"));
        assert!(msg.contains("circuit breaker is open"));
        assert!(msg.contains("3 consecutive failures"));
    }

    #[test]
    fn test_transitions_to_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_millis(1), // very short for test
            service_name: "test".to_string(),
        });

        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Wait for cooldown
        std::thread::sleep(Duration::from_millis(5));

        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
        assert!(cb.check_request().is_ok()); // probe allowed
    }

    #[test]
    fn test_half_open_success_closes_circuit() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_millis(1),
            service_name: "test".to_string(),
        });

        for _ in 0..3 {
            cb.record_failure();
        }

        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        cb.record_success();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn test_half_open_failure_reopens_circuit() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            cooldown: Duration::from_millis(1),
            service_name: "test".to_string(),
        });

        for _ in 0..3 {
            cb.record_failure();
        }

        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        // Failure during half-open — counter was already at 3, so one more keeps it open
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = test_breaker("test");

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.consecutive_failures(), 2);

        cb.record_success();
        assert_eq!(cb.consecutive_failures(), 0);
        assert_eq!(cb.current_state(), CircuitState::Closed);

        // Need 3 more failures to trip again
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Closed);
    }

    #[test]
    fn test_total_trips_counter() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            cooldown: Duration::from_millis(1),
            service_name: "test".to_string(),
        });

        // First trip
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.total_trips(), 1);

        // Recover
        std::thread::sleep(Duration::from_millis(5));
        cb.record_success();

        // Second trip
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.total_trips(), 2);
    }

    #[test]
    fn test_reset() {
        let cb = test_breaker("test");

        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.current_state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.consecutive_failures(), 0);
        assert_eq!(cb.total_trips(), 0);
    }

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(CircuitState::Closed.to_string(), "closed");
        assert_eq!(CircuitState::Open.to_string(), "open");
        assert_eq!(CircuitState::HalfOpen.to_string(), "half-open");
    }

    #[test]
    fn test_clone_shares_state() {
        let cb1 = test_breaker("test");
        let cb2 = cb1.clone();

        cb1.record_failure();
        cb1.record_failure();
        cb1.record_failure();

        // cb2 sees the same state
        assert_eq!(cb2.current_state(), CircuitState::Open);
        assert!(cb2.check_request().is_err());
    }

    #[test]
    fn test_default_config() {
        let config = CircuitBreakerConfig::new("whisper");
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.cooldown, Duration::from_secs(30));
        assert_eq!(config.service_name, "whisper");
    }
}
