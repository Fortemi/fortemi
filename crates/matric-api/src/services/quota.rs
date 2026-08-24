//! Shared request-admission state for hosted multi-instance deployments.

use std::time::Duration;

use redis::aio::ConnectionManager;
use thiserror::Error;

const ADMIT_SCRIPT: &str = r#"
local current = redis.call('INCR', KEYS[1])
if current == 1 then
  redis.call('PEXPIRE', KEYS[1], ARGV[1])
end
local ttl = redis.call('PTTL', KEYS[1])
return {current, ttl}
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestQuotaPolicy {
    pub id: String,
    pub version: u32,
    pub limit: u64,
    pub window: Duration,
}

impl RequestQuotaPolicy {
    pub fn validate(&self) -> Result<(), QuotaStoreError> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || self.version == 0
            || self.limit == 0
            || self.limit > 1_000_000
            || self.window.is_zero()
            || self.window > Duration::from_secs(86_400)
        {
            return Err(QuotaStoreError::InvalidPolicy);
        }
        Ok(())
    }

    fn window_millis(&self) -> Result<u64, QuotaStoreError> {
        u64::try_from(self.window.as_millis()).map_err(|_| QuotaStoreError::InvalidPolicy)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestQuotaIdentity<'a> {
    pub tenant: &'a str,
    pub principal: &'a str,
    pub client: &'a str,
    pub route_class: &'a str,
}

impl RequestQuotaIdentity<'_> {
    fn key(&self, policy: &RequestQuotaPolicy) -> Result<String, QuotaStoreError> {
        for value in [self.tenant, self.principal, self.client, self.route_class] {
            if value.is_empty() || value.len() > 256 || value.contains('\0') {
                return Err(QuotaStoreError::InvalidIdentity);
            }
        }
        let material = format!(
            "v1\0{}\0{}\0{}\0{}\0{}\0{}",
            policy.id, policy.version, self.tenant, self.principal, self.client, self.route_class
        );
        Ok(format!(
            "fortemi:quota:v1:{}",
            blake3::hash(material.as_bytes()).to_hex()
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestQuotaDecision {
    pub allowed: bool,
    pub remaining: u64,
    pub retry_after: Duration,
    pub policy_id: String,
    pub policy_version: u32,
    pub limit: u64,
    pub window: Duration,
}

#[derive(Debug, Error)]
pub enum QuotaStoreError {
    #[error("invalid quota policy")]
    InvalidPolicy,
    #[error("invalid quota identity")]
    InvalidIdentity,
    #[error("quota state unavailable")]
    Unavailable,
}

#[derive(Clone)]
pub struct RedisRequestQuotaGate {
    connection: ConnectionManager,
    policy: RequestQuotaPolicy,
}

impl std::fmt::Debug for RedisRequestQuotaGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisRequestQuotaGate")
            .field("policy_id", &self.policy.id)
            .field("policy_version", &self.policy.version)
            .finish_non_exhaustive()
    }
}

impl RedisRequestQuotaGate {
    pub async fn connect(
        redis_url: &str,
        policy: RequestQuotaPolicy,
    ) -> Result<Self, QuotaStoreError> {
        policy.validate()?;
        let client = redis::Client::open(redis_url).map_err(|_| QuotaStoreError::Unavailable)?;
        let mut connection = ConnectionManager::new(client)
            .await
            .map_err(|_| QuotaStoreError::Unavailable)?;
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map_err(|_| QuotaStoreError::Unavailable)?;
        Ok(Self { connection, policy })
    }

    pub fn policy(&self) -> &RequestQuotaPolicy {
        &self.policy
    }

    pub async fn admit(
        &self,
        identity: &RequestQuotaIdentity<'_>,
    ) -> Result<RequestQuotaDecision, QuotaStoreError> {
        let key = identity.key(&self.policy)?;
        let window_millis = self.policy.window_millis()?;
        let mut connection = self.connection.clone();
        let values: Vec<i64> = redis::Script::new(ADMIT_SCRIPT)
            .key(key)
            .arg(window_millis)
            .invoke_async(&mut connection)
            .await
            .map_err(|_| QuotaStoreError::Unavailable)?;
        let [current, ttl_millis] = values.as_slice() else {
            return Err(QuotaStoreError::Unavailable);
        };
        let current = u64::try_from(*current).map_err(|_| QuotaStoreError::Unavailable)?;
        let ttl_millis = u64::try_from(*ttl_millis).map_err(|_| QuotaStoreError::Unavailable)?;

        Ok(RequestQuotaDecision {
            allowed: current <= self.policy.limit,
            remaining: self.policy.limit.saturating_sub(current),
            retry_after: Duration::from_millis(ttl_millis.max(1)),
            policy_id: self.policy.id.clone(),
            policy_version: self.policy.version,
            limit: self.policy.limit,
            window: self.policy.window,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn policy(limit: u64) -> RequestQuotaPolicy {
        RequestQuotaPolicy {
            id: "hosted-api".to_string(),
            version: 1,
            limit,
            window: Duration::from_secs(60),
        }
    }

    #[test]
    fn identity_keys_are_opaque_and_dimension_sensitive() {
        let policy = policy(10);
        let first = RequestQuotaIdentity {
            tenant: "tenant-secret-a",
            principal: "principal-secret",
            client: "client-secret",
            route_class: "authenticated_read",
        };
        let second = RequestQuotaIdentity {
            route_class: "authenticated_write",
            ..first.clone()
        };
        let first_key = first.key(&policy).unwrap();

        assert_ne!(first_key, second.key(&policy).unwrap());
        assert!(!first_key.contains("tenant-secret"));
        assert!(!first_key.contains("principal-secret"));
        assert!(!first_key.contains("client-secret"));
    }

    #[test]
    fn invalid_policy_and_identity_fail_before_store_access() {
        assert!(matches!(
            policy(0).validate(),
            Err(QuotaStoreError::InvalidPolicy)
        ));
        assert!(matches!(
            policy(1_000_001).validate(),
            Err(QuotaStoreError::InvalidPolicy)
        ));
        let identity = RequestQuotaIdentity {
            tenant: "",
            principal: "principal",
            client: "client",
            route_class: "read",
        };
        assert!(matches!(
            identity.key(&policy(1)),
            Err(QuotaStoreError::InvalidIdentity)
        ));
    }

    #[tokio::test]
    #[ignore = "requires TEST_REDIS_URL"]
    async fn two_gates_share_atomic_limits_and_keep_tenants_separate() {
        let redis_url = std::env::var("TEST_REDIS_URL").expect("TEST_REDIS_URL");
        let first = Arc::new(
            RedisRequestQuotaGate::connect(&redis_url, policy(20))
                .await
                .unwrap(),
        );
        let second = Arc::new(
            RedisRequestQuotaGate::connect(&redis_url, policy(20))
                .await
                .unwrap(),
        );
        let tenant_a = format!("tenant-a-{}", uuid::Uuid::now_v7());
        let tenant_b = format!("tenant-b-{}", uuid::Uuid::now_v7());
        let mut tasks = Vec::new();

        for tenant in [tenant_a, tenant_b] {
            for index in 0..50 {
                let gate = if index % 2 == 0 {
                    first.clone()
                } else {
                    second.clone()
                };
                let tenant = tenant.clone();
                tasks.push(tokio::spawn(async move {
                    let allowed = gate
                        .admit(&RequestQuotaIdentity {
                            tenant: &tenant,
                            principal: "principal",
                            client: "client",
                            route_class: "authenticated_write",
                        })
                        .await
                        .unwrap()
                        .allowed;
                    (tenant, allowed)
                }));
            }
        }

        let mut allowed_by_tenant = std::collections::HashMap::new();
        for task in tasks {
            let (tenant, allowed) = task.await.unwrap();
            *allowed_by_tenant.entry(tenant).or_insert(0) += usize::from(allowed);
        }
        assert_eq!(
            allowed_by_tenant.values().copied().collect::<Vec<_>>(),
            [20, 20]
        );
    }
}
