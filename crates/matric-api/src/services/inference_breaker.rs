//! Bounded shared circuit-breaker state for hosted stored-credential inference.

use std::fmt;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lru::LruCache;
use matric_inference::{CircuitBreaker, CircuitBreakerConfig};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InferenceBreakerConfig {
    pub failure_threshold: NonZeroU32,
    pub cooldown: Duration,
    pub capacity: NonZeroUsize,
}

impl Default for InferenceBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: NonZeroU32::new(3).expect("three is non-zero"),
            cooldown: Duration::from_secs(30),
            capacity: NonZeroUsize::new(4096).expect("4096 is non-zero"),
        }
    }
}

impl InferenceBreakerConfig {
    pub fn from_values(
        failure_threshold: u32,
        cooldown: Duration,
        capacity: usize,
    ) -> Result<Self, &'static str> {
        if cooldown.is_zero() || cooldown > Duration::from_secs(3600) {
            return Err("invalid_cooldown");
        }
        Ok(Self {
            failure_threshold: NonZeroU32::new(failure_threshold)
                .filter(|value| value.get() <= 100)
                .ok_or("invalid_failure_threshold")?,
            cooldown,
            capacity: NonZeroUsize::new(capacity)
                .filter(|value| value.get() <= 65_536)
                .ok_or("invalid_capacity")?,
        })
    }
}

pub struct InferenceBreakerScope<'a> {
    pub tenant_id: Uuid,
    pub user_id: &'a str,
    pub secret_id: Uuid,
    pub provider_id: &'a str,
    pub model: &'a str,
}

impl fmt::Debug for InferenceBreakerScope<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InferenceBreakerScope")
            .field("tenant_present", &!self.tenant_id.is_nil())
            .field("user_present", &!self.user_id.is_empty())
            .field("secret_present", &!self.secret_id.is_nil())
            .field("provider_present", &!self.provider_id.is_empty())
            .field("model_present", &!self.model.is_empty())
            .finish()
    }
}

#[derive(Clone)]
pub struct InferenceCircuitBreakerRegistry {
    config: InferenceBreakerConfig,
    breakers: Arc<Mutex<LruCache<[u8; 32], CircuitBreaker>>>,
}

impl fmt::Debug for InferenceCircuitBreakerRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InferenceCircuitBreakerRegistry")
            .field("config", &self.config)
            .field("scope_count", &self.scope_count())
            .finish()
    }
}

impl InferenceCircuitBreakerRegistry {
    pub fn new(config: InferenceBreakerConfig) -> Self {
        Self {
            config,
            breakers: Arc::new(Mutex::new(LruCache::new(config.capacity))),
        }
    }

    pub fn breaker_for(&self, scope: InferenceBreakerScope<'_>) -> CircuitBreaker {
        let key = scope_key(scope);
        let mut breakers = self
            .breakers
            .lock()
            .expect("inference breaker lock poisoned");
        breakers
            .get_or_insert(key, || {
                CircuitBreaker::new(CircuitBreakerConfig {
                    failure_threshold: self.config.failure_threshold.get(),
                    cooldown: self.config.cooldown,
                    service_name: "hosted-stored-inference".to_string(),
                })
            })
            .clone()
    }

    pub fn scope_count(&self) -> usize {
        self.breakers
            .lock()
            .expect("inference breaker lock poisoned")
            .len()
    }
}

fn scope_key(scope: InferenceBreakerScope<'_>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    append_scope_field(&mut hasher, scope.tenant_id.as_bytes());
    append_scope_field(&mut hasher, scope.user_id.as_bytes());
    append_scope_field(&mut hasher, scope.secret_id.as_bytes());
    append_scope_field(&mut hasher, scope.provider_id.as_bytes());
    append_scope_field(&mut hasher, scope.model.as_bytes());
    *hasher.finalize().as_bytes()
}

fn append_scope_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_inference::CircuitState;

    fn scope<'a>(model: &'a str, secret_id: Uuid) -> InferenceBreakerScope<'a> {
        InferenceBreakerScope {
            tenant_id: Uuid::parse_str("01991f42-cd5f-7000-8000-000000000001").unwrap(),
            user_id: "user-private",
            secret_id,
            provider_id: "openai",
            model,
        }
    }

    #[test]
    fn same_scope_shares_state_and_different_model_isolated() {
        let registry = InferenceCircuitBreakerRegistry::new(
            InferenceBreakerConfig::from_values(1, Duration::from_secs(30), 4).unwrap(),
        );
        let secret_id = Uuid::parse_str("01991f42-cd5f-7000-8000-000000000002").unwrap();
        let first = registry.breaker_for(scope("model-a", secret_id));
        let shared = registry.breaker_for(scope("model-a", secret_id));
        let isolated = registry.breaker_for(scope("model-b", secret_id));

        first.record_failure();
        assert_eq!(shared.current_state(), CircuitState::Open);
        assert_eq!(isolated.current_state(), CircuitState::Closed);
    }

    #[test]
    fn registry_is_bounded_and_debug_redacts_scope_values() {
        let registry = InferenceCircuitBreakerRegistry::new(
            InferenceBreakerConfig::from_values(2, Duration::from_secs(5), 2).unwrap(),
        );
        for suffix in 1..=3 {
            registry.breaker_for(scope(
                "private-model-name",
                Uuid::from_u128(0x01991f42_cd5f_7000_8000_000000000000 + suffix),
            ));
        }
        assert_eq!(registry.scope_count(), 2);
        let rendered = format!("{registry:?}");
        assert!(!rendered.contains("private-model-name"));
        assert!(!rendered.contains("user-private"));
    }
}
