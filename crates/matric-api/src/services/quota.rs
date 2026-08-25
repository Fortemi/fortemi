//! Shared request-admission state for hosted multi-instance deployments.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use matric_core::{
    QuotaDecision, QuotaReservation, QuotaReservationRequest, UsageDimension, UsageQuantity,
    UsageSubject,
};
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

const CHECK_SCRIPT: &str = r#"
local current = tonumber(redis.call('HGET', KEYS[1], 'current') or '0')
local reset_at = tonumber(redis.call('HGET', KEYS[1], 'reset_at') or '0')
return {current, reset_at}
"#;

const RESERVE_SCRIPT: &str = r#"
if redis.call('EXISTS', KEYS[2]) == 1 then
  if redis.call('HGET', KEYS[2], 'fingerprint') ~= ARGV[3] then
    return {-2, 0, 0}
  end
  local state = redis.call('HGET', KEYS[2], 'state')
  if state ~= 'reserved' then
    return {-2, 0, 0}
  end
  local current = tonumber(redis.call('HGET', KEYS[1], 'current') or '0')
  local reset_at = tonumber(redis.call('HGET', KEYS[1], 'reset_at') or '0')
  return {2, current, reset_at}
end
local counter_exists = redis.call('EXISTS', KEYS[1])
local current = tonumber(redis.call('HGET', KEYS[1], 'current') or '0')
local reset_at = tonumber(redis.call('HGET', KEYS[1], 'reset_at') or '0')
local requested = tonumber(ARGV[4])
local limit = tonumber(ARGV[5])
if current + requested > limit then
  return {0, current, reset_at}
end
local updated = redis.call('HINCRBY', KEYS[1], 'current', requested)
if counter_exists == 0 then
  local now = redis.call('TIME')
  reset_at = now[1] * 1000 + math.floor(now[2] / 1000) + tonumber(ARGV[1])
  redis.call('HSET', KEYS[1], 'reset_at', reset_at)
  redis.call('PEXPIRE', KEYS[1], ARGV[1])
end
redis.call('HSET', KEYS[2],
  'fingerprint', ARGV[3],
  'state', 'reserved',
  'reserved', ARGV[4])
redis.call('PEXPIRE', KEYS[2], ARGV[2])
return {1, updated, reset_at}
"#;

const FINALIZE_SCRIPT: &str = r#"
if redis.call('EXISTS', KEYS[2]) == 0 then
  return {-1, 0, 0}
end
if redis.call('HGET', KEYS[2], 'fingerprint') ~= ARGV[1] then
  return {-2, 0, 0}
end
local state = redis.call('HGET', KEYS[2], 'state')
if state == 'released' then
  return {-2, 0, 0}
end
if state == 'finalized' then
  if redis.call('HGET', KEYS[2], 'actual') ~= ARGV[2] then
    return {-2, 0, 0}
  end
  local current = tonumber(redis.call('HGET', KEYS[1], 'current') or '0')
  local reset_at = tonumber(redis.call('HGET', KEYS[1], 'reset_at') or '0')
  return {2, current, reset_at}
end
if redis.call('EXISTS', KEYS[1]) == 0 then
  return {-3, 0, 0}
end
local reserved = tonumber(redis.call('HGET', KEYS[2], 'reserved'))
local actual = tonumber(ARGV[2])
local delta = actual - reserved
local updated = tonumber(redis.call('HGET', KEYS[1], 'current'))
if delta ~= 0 then
  updated = redis.call('HINCRBY', KEYS[1], 'current', delta)
end
if updated < 0 then
  redis.call('HSET', KEYS[1], 'current', 0)
  updated = 0
end
redis.call('HSET', KEYS[2], 'state', 'finalized', 'actual', ARGV[2])
local reset_at = tonumber(redis.call('HGET', KEYS[1], 'reset_at') or '0')
return {1, updated, reset_at}
"#;

const RELEASE_SCRIPT: &str = r#"
if redis.call('EXISTS', KEYS[2]) == 0 then
  return -1
end
if redis.call('HGET', KEYS[2], 'fingerprint') ~= ARGV[1] then
  return -2
end
local state = redis.call('HGET', KEYS[2], 'state')
if state == 'released' then
  return 2
end
if state == 'finalized' then
  return -2
end
if redis.call('EXISTS', KEYS[1]) == 0 then
  return -3
end
local reserved = tonumber(redis.call('HGET', KEYS[2], 'reserved'))
local updated = redis.call('HINCRBY', KEYS[1], 'current', -reserved)
if updated < 0 then
  redis.call('HSET', KEYS[1], 'current', 0)
end
redis.call('HSET', KEYS[2], 'state', 'released')
return 1
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
    #[error("invalid quota quantity")]
    InvalidQuantity,
    #[error("quota reservation not found")]
    ReservationNotFound,
    #[error("quota reservation conflicts with existing state")]
    ReservationConflict,
    #[error("quota reservation expired")]
    ReservationExpired,
    #[error("quota state unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaStoreHealthStatus {
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaStoreHealth {
    pub status: QuotaStoreHealthStatus,
    pub checked_at: DateTime<Utc>,
    pub consecutive_failures: u64,
}

#[derive(Clone)]
pub struct RedisRequestQuotaGate {
    connection: ConnectionManager,
    policy: RequestQuotaPolicy,
    health: Arc<RwLock<QuotaStoreHealth>>,
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
        Ok(Self {
            connection,
            policy,
            health: new_health_state(),
        })
    }

    pub fn policy(&self) -> &RequestQuotaPolicy {
        &self.policy
    }

    pub fn health(&self) -> QuotaStoreHealth {
        self.health
            .read()
            .expect("quota health lock poisoned")
            .clone()
    }

    pub async fn check_health(&self) -> Result<QuotaStoreHealth, QuotaStoreError> {
        let mut connection = self.connection.clone();
        let result = redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map_err(|_| QuotaStoreError::Unavailable);
        match result {
            Ok(_) => {
                record_health_success(&self.health);
                Ok(self.health())
            }
            Err(error) => {
                record_health_failure(&self.health);
                Err(error)
            }
        }
    }

    pub async fn admit(
        &self,
        identity: &RequestQuotaIdentity<'_>,
    ) -> Result<RequestQuotaDecision, QuotaStoreError> {
        let result = self.admit_inner(identity).await;
        match result {
            Ok(decision) => {
                record_health_success(&self.health);
                Ok(decision)
            }
            Err(error) => {
                if matches!(error, QuotaStoreError::Unavailable) {
                    record_health_failure(&self.health);
                }
                Err(error)
            }
        }
    }

    async fn admit_inner(
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

fn new_health_state() -> Arc<RwLock<QuotaStoreHealth>> {
    Arc::new(RwLock::new(QuotaStoreHealth {
        status: QuotaStoreHealthStatus::Ready,
        checked_at: Utc::now(),
        consecutive_failures: 0,
    }))
}

fn record_health_success(health: &RwLock<QuotaStoreHealth>) {
    let mut health = health.write().expect("quota health lock poisoned");
    health.status = QuotaStoreHealthStatus::Ready;
    health.checked_at = Utc::now();
    health.consecutive_failures = 0;
}

fn record_health_failure(health: &RwLock<QuotaStoreHealth>) {
    let mut health = health.write().expect("quota health lock poisoned");
    health.status = QuotaStoreHealthStatus::Unavailable;
    health.checked_at = Utc::now();
    health.consecutive_failures = health.consecutive_failures.saturating_add(1);
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum QuotaIdentityDimension {
    Tenant,
    Principal,
    Client,
    Archive,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaLimitPolicy {
    pub id: String,
    pub version: u32,
    pub dimension: UsageDimension,
    pub identity_dimensions: BTreeSet<QuotaIdentityDimension>,
    pub limit: u64,
    pub window: Duration,
}

impl QuotaLimitPolicy {
    pub fn validate(&self) -> Result<(), QuotaStoreError> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || self.version == 0
            || self.limit == 0
            || self.limit > i64::MAX as u64
            || self.window.is_zero()
            || self.window > Duration::from_secs(2_592_000)
            || !self
                .identity_dimensions
                .contains(&QuotaIdentityDimension::Tenant)
        {
            return Err(QuotaStoreError::InvalidPolicy);
        }
        dimension_label(&self.dimension)?;
        Ok(())
    }

    fn window_millis(&self) -> Result<u64, QuotaStoreError> {
        u64::try_from(self.window.as_millis()).map_err(|_| QuotaStoreError::InvalidPolicy)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuotaReservationOutcome {
    Reserved(Box<QuotaReservation>),
    HardLimit(QuotaDecision),
}

#[derive(Clone)]
pub struct RedisQuotaCoordinator {
    connection: ConnectionManager,
    policies: HashMap<String, QuotaLimitPolicy>,
    health: Arc<RwLock<QuotaStoreHealth>>,
}

impl std::fmt::Debug for RedisQuotaCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisQuotaCoordinator")
            .field("policy_count", &self.policies.len())
            .field("health", &self.health())
            .finish_non_exhaustive()
    }
}

impl RedisQuotaCoordinator {
    pub async fn connect(
        redis_url: &str,
        policies: Vec<QuotaLimitPolicy>,
    ) -> Result<Self, QuotaStoreError> {
        if policies.is_empty() {
            return Err(QuotaStoreError::InvalidPolicy);
        }
        let mut by_dimension = HashMap::new();
        for policy in policies {
            policy.validate()?;
            let dimension = dimension_label(&policy.dimension)?;
            if by_dimension.insert(dimension, policy).is_some() {
                return Err(QuotaStoreError::InvalidPolicy);
            }
        }

        let client = redis::Client::open(redis_url).map_err(|_| QuotaStoreError::Unavailable)?;
        let mut connection = ConnectionManager::new(client)
            .await
            .map_err(|_| QuotaStoreError::Unavailable)?;
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map_err(|_| QuotaStoreError::Unavailable)?;
        Ok(Self {
            connection,
            policies: by_dimension,
            health: new_health_state(),
        })
    }

    pub fn policy_count(&self) -> usize {
        self.policies.len()
    }

    pub fn health(&self) -> QuotaStoreHealth {
        self.health
            .read()
            .expect("quota health lock poisoned")
            .clone()
    }

    pub async fn check(
        &self,
        subject: &UsageSubject,
        dimension: &UsageDimension,
        requested: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaStoreError> {
        let policy = self.policy_for(dimension)?;
        let requested = whole_quantity(requested, dimension)?;
        let counter_key = quota_counter_key(policy, subject)?;
        let mut connection = self.connection.clone();
        let result: Result<Vec<i64>, _> = redis::Script::new(CHECK_SCRIPT)
            .key(counter_key)
            .invoke_async(&mut connection)
            .await;
        let values = self.store_result(result)?;
        let [current, reset_at_millis] = values.as_slice() else {
            return Err(self.unavailable());
        };
        let projected = u64::try_from(*current)
            .map_err(|_| self.unavailable())?
            .saturating_add(requested);
        Ok(quota_decision(policy, projected, *reset_at_millis))
    }

    pub async fn reserve(
        &self,
        request: &QuotaReservationRequest,
    ) -> Result<QuotaReservationOutcome, QuotaStoreError> {
        let policy = self.policy_for(&request.dimension)?;
        let now = Utc::now();
        if request.expires_at <= now {
            return Err(QuotaStoreError::ReservationExpired);
        }
        let max_expiry = now
            + chrono::Duration::from_std(policy.window)
                .map_err(|_| QuotaStoreError::InvalidPolicy)?;
        if request.expires_at > max_expiry {
            return Err(QuotaStoreError::InvalidPolicy);
        }
        let reserved = whole_quantity(&request.estimated, &request.dimension)?;
        let counter_key = quota_counter_key(policy, &request.subject)?;
        let reservation_key = quota_reservation_key(request.reservation_id);
        let fingerprint = reservation_fingerprint(
            request.reservation_id,
            &request.idempotency_key,
            &request.subject,
            &request.dimension,
            &request.estimated,
            request.expires_at,
        )?;
        let reservation_ttl = u64::try_from((request.expires_at - now).num_milliseconds())
            .map_err(|_| QuotaStoreError::ReservationExpired)?
            .max(1);
        let mut connection = self.connection.clone();
        let result: Result<Vec<i64>, _> = redis::Script::new(RESERVE_SCRIPT)
            .key(counter_key)
            .key(reservation_key)
            .arg(policy.window_millis()?)
            .arg(reservation_ttl)
            .arg(fingerprint)
            .arg(reserved)
            .arg(policy.limit)
            .invoke_async(&mut connection)
            .await;
        let values = self.store_result(result)?;
        let [code, current, reset_at_millis] = values.as_slice() else {
            return Err(self.unavailable());
        };
        match *code {
            0 => Ok(QuotaReservationOutcome::HardLimit(quota_decision(
                policy,
                u64::try_from(*current)
                    .map_err(|_| self.unavailable())?
                    .saturating_add(reserved),
                *reset_at_millis,
            ))),
            1 | 2 => Ok(QuotaReservationOutcome::Reserved(Box::new(
                QuotaReservation {
                    reservation_id: request.reservation_id,
                    idempotency_key: request.idempotency_key.clone(),
                    subject: request.subject.clone(),
                    dimension: request.dimension.clone(),
                    policy_id: policy.id.clone(),
                    reserved: request.estimated.clone(),
                    expires_at: request.expires_at,
                },
            ))),
            -2 => Err(QuotaStoreError::ReservationConflict),
            _ => Err(self.unavailable()),
        }
    }

    pub async fn finalize(
        &self,
        reservation: &QuotaReservation,
        actual: &UsageQuantity,
    ) -> Result<QuotaDecision, QuotaStoreError> {
        if reservation.expires_at <= Utc::now() {
            return Err(QuotaStoreError::ReservationExpired);
        }
        let policy = self.policy_for(&reservation.dimension)?;
        if reservation.policy_id != policy.id {
            return Err(QuotaStoreError::ReservationConflict);
        }
        let actual = whole_quantity(actual, &reservation.dimension)?;
        let counter_key = quota_counter_key(policy, &reservation.subject)?;
        let reservation_key = quota_reservation_key(reservation.reservation_id);
        let fingerprint = reservation_fingerprint(
            reservation.reservation_id,
            &reservation.idempotency_key,
            &reservation.subject,
            &reservation.dimension,
            &reservation.reserved,
            reservation.expires_at,
        )?;
        let mut connection = self.connection.clone();
        let result: Result<Vec<i64>, _> = redis::Script::new(FINALIZE_SCRIPT)
            .key(counter_key)
            .key(reservation_key)
            .arg(fingerprint)
            .arg(actual)
            .invoke_async(&mut connection)
            .await;
        let values = self.store_result(result)?;
        let [code, current, reset_at_millis] = values.as_slice() else {
            return Err(self.unavailable());
        };
        match *code {
            1 | 2 => Ok(quota_decision(
                policy,
                u64::try_from(*current).map_err(|_| self.unavailable())?,
                *reset_at_millis,
            )),
            -1 => Err(QuotaStoreError::ReservationNotFound),
            -2 => Err(QuotaStoreError::ReservationConflict),
            -3 => Err(self.unavailable()),
            _ => Err(self.unavailable()),
        }
    }

    pub async fn release(&self, reservation: &QuotaReservation) -> Result<(), QuotaStoreError> {
        if reservation.expires_at <= Utc::now() {
            return Err(QuotaStoreError::ReservationExpired);
        }
        let policy = self.policy_for(&reservation.dimension)?;
        if reservation.policy_id != policy.id {
            return Err(QuotaStoreError::ReservationConflict);
        }
        let counter_key = quota_counter_key(policy, &reservation.subject)?;
        let reservation_key = quota_reservation_key(reservation.reservation_id);
        let fingerprint = reservation_fingerprint(
            reservation.reservation_id,
            &reservation.idempotency_key,
            &reservation.subject,
            &reservation.dimension,
            &reservation.reserved,
            reservation.expires_at,
        )?;
        let mut connection = self.connection.clone();
        let result: Result<i64, _> = redis::Script::new(RELEASE_SCRIPT)
            .key(counter_key)
            .key(reservation_key)
            .arg(fingerprint)
            .invoke_async(&mut connection)
            .await;
        let code = self.store_result(result)?;
        match code {
            1 | 2 => Ok(()),
            -1 => Err(QuotaStoreError::ReservationNotFound),
            -2 => Err(QuotaStoreError::ReservationConflict),
            -3 => Err(self.unavailable()),
            _ => Err(self.unavailable()),
        }
    }

    fn policy_for(&self, dimension: &UsageDimension) -> Result<&QuotaLimitPolicy, QuotaStoreError> {
        self.policies
            .get(&dimension_label(dimension)?)
            .ok_or(QuotaStoreError::InvalidPolicy)
    }

    fn store_result<T, E>(&self, result: Result<T, E>) -> Result<T, QuotaStoreError> {
        match result {
            Ok(value) => {
                record_health_success(&self.health);
                Ok(value)
            }
            Err(_) => Err(self.unavailable()),
        }
    }

    fn unavailable(&self) -> QuotaStoreError {
        record_health_failure(&self.health);
        QuotaStoreError::Unavailable
    }
}

fn dimension_label(dimension: &UsageDimension) -> Result<String, QuotaStoreError> {
    serde_json::to_string(dimension).map_err(|_| QuotaStoreError::InvalidPolicy)
}

fn whole_quantity(
    quantity: &UsageQuantity,
    dimension: &UsageDimension,
) -> Result<u64, QuotaStoreError> {
    if quantity.unit() != &dimension.unit() {
        return Err(QuotaStoreError::InvalidQuantity);
    }
    quantity
        .value()
        .to_string()
        .parse::<u64>()
        .map_err(|_| QuotaStoreError::InvalidQuantity)
}

fn quota_counter_key(
    policy: &QuotaLimitPolicy,
    subject: &UsageSubject,
) -> Result<String, QuotaStoreError> {
    let mut selected = Vec::new();
    for dimension in &policy.identity_dimensions {
        let value = match dimension {
            QuotaIdentityDimension::Tenant => subject.tenant_id(),
            QuotaIdentityDimension::Principal => subject.principal_id(),
            QuotaIdentityDimension::Client => subject.client_id(),
            QuotaIdentityDimension::Archive => subject.archive_id(),
        }
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or(QuotaStoreError::InvalidIdentity)?;
        selected.push((format!("{dimension:?}"), value));
    }
    let material = serde_json::to_vec(&(
        "v2",
        &policy.id,
        policy.version,
        dimension_label(&policy.dimension)?,
        selected,
    ))
    .map_err(|_| QuotaStoreError::InvalidIdentity)?;
    Ok(format!(
        "fortemi:quota:v2:counter:{}",
        blake3::hash(&material).to_hex()
    ))
}

fn quota_reservation_key(reservation_id: uuid::Uuid) -> String {
    let digest = blake3::hash(reservation_id.as_bytes());
    format!("fortemi:quota:v2:reservation:{}", digest.to_hex())
}

fn reservation_fingerprint(
    reservation_id: uuid::Uuid,
    idempotency_key: &str,
    subject: &UsageSubject,
    dimension: &UsageDimension,
    reserved: &UsageQuantity,
    expires_at: DateTime<Utc>,
) -> Result<String, QuotaStoreError> {
    let material = serde_json::to_vec(&(
        reservation_id,
        idempotency_key,
        subject,
        dimension,
        reserved,
        expires_at,
    ))
    .map_err(|_| QuotaStoreError::InvalidQuantity)?;
    Ok(blake3::hash(&material).to_hex().to_string())
}

fn quota_decision(policy: &QuotaLimitPolicy, current: u64, reset_at_millis: i64) -> QuotaDecision {
    let remaining = policy.limit.saturating_sub(current);
    let now = Utc::now();
    let reset_at = DateTime::from_timestamp_millis(reset_at_millis).or_else(|| {
        chrono::Duration::from_std(policy.window)
            .ok()
            .map(|window| now + window)
    });
    let reset_after = reset_at
        .and_then(|reset_at| (reset_at - now).to_std().ok())
        .unwrap_or(policy.window);
    if current > policy.limit {
        QuotaDecision::HardLimit {
            policy_id: policy.id.clone(),
            retry_after: Some(reset_after),
            reset_at,
        }
    } else {
        let remaining = UsageQuantity::whole(remaining, policy.dimension.unit()).ok();
        if current.saturating_mul(100) >= policy.limit.saturating_mul(80) {
            QuotaDecision::SoftLimit {
                remaining,
                policy_id: policy.id.clone(),
                reset_at,
            }
        } else {
            QuotaDecision::Allow {
                remaining,
                policy_id: policy.id.clone(),
                reset_at,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::UsageUnit;
    use std::sync::Arc;

    fn policy(limit: u64) -> RequestQuotaPolicy {
        RequestQuotaPolicy {
            id: "hosted-api".to_string(),
            version: 1,
            limit,
            window: Duration::from_secs(60),
        }
    }

    fn storage_policy(limit: u64) -> QuotaLimitPolicy {
        QuotaLimitPolicy {
            id: "hosted-storage".to_string(),
            version: 1,
            dimension: UsageDimension::StorageBytes,
            identity_dimensions: BTreeSet::from([QuotaIdentityDimension::Tenant]),
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

    #[test]
    fn multidimensional_policy_requires_tenant_and_selects_subject_dimensions() {
        let policy = storage_policy(1_000);
        policy.validate().unwrap();
        let first = UsageSubject::unknown()
            .with_tenant("tenant-private")
            .unwrap()
            .with_principal("principal-a")
            .unwrap();
        let second = UsageSubject::unknown()
            .with_tenant("tenant-private")
            .unwrap()
            .with_principal("principal-b")
            .unwrap();
        let first_key = quota_counter_key(&policy, &first).unwrap();

        assert_eq!(first_key, quota_counter_key(&policy, &second).unwrap());
        assert!(!first_key.contains("tenant-private"));
        assert!(matches!(
            quota_counter_key(&policy, &UsageSubject::unknown()),
            Err(QuotaStoreError::InvalidIdentity)
        ));

        let mut principal_policy = policy;
        principal_policy
            .identity_dimensions
            .insert(QuotaIdentityDimension::Principal);
        assert_ne!(
            quota_counter_key(&principal_policy, &first).unwrap(),
            quota_counter_key(&principal_policy, &second).unwrap()
        );
    }

    #[test]
    fn reservation_identity_and_whole_quantity_validation_are_deterministic() {
        let subject = UsageSubject::unknown().with_tenant("tenant-a").unwrap();
        let quantity = UsageQuantity::whole(25, UsageUnit::Byte).unwrap();
        let expires_at = Utc::now() + chrono::Duration::seconds(30);
        let reservation_id = uuid::Uuid::now_v7();
        let first = reservation_fingerprint(
            reservation_id,
            "reservation:one",
            &subject,
            &UsageDimension::StorageBytes,
            &quantity,
            expires_at,
        )
        .unwrap();
        let replay = reservation_fingerprint(
            reservation_id,
            "reservation:one",
            &subject,
            &UsageDimension::StorageBytes,
            &quantity,
            expires_at,
        )
        .unwrap();
        let conflict = reservation_fingerprint(
            reservation_id,
            "reservation:two",
            &subject,
            &UsageDimension::StorageBytes,
            &quantity,
            expires_at,
        )
        .unwrap();

        assert_eq!(first, replay);
        assert_ne!(first, conflict);
        assert_eq!(
            whole_quantity(&quantity, &UsageDimension::StorageBytes).unwrap(),
            25
        );
        assert!(matches!(
            whole_quantity(&quantity, &UsageDimension::ApiRequest),
            Err(QuotaStoreError::InvalidQuantity)
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

        let coordinator = RedisQuotaCoordinator::connect(&redis_url, vec![storage_policy(1_000)])
            .await
            .unwrap();
        let subject = UsageSubject::unknown()
            .with_tenant(format!("reservation-tenant-{}", uuid::Uuid::now_v7()))
            .unwrap();
        let request = QuotaReservationRequest {
            reservation_id: uuid::Uuid::now_v7(),
            idempotency_key: "reservation:storage:one".to_string(),
            subject: subject.clone(),
            dimension: UsageDimension::StorageBytes,
            estimated: UsageQuantity::whole(700, UsageUnit::Byte).unwrap(),
            expires_at: Utc::now() + chrono::Duration::seconds(30),
        };
        let reservation = match coordinator.reserve(&request).await.unwrap() {
            QuotaReservationOutcome::Reserved(reservation) => reservation,
            QuotaReservationOutcome::HardLimit(_) => panic!("first reservation must fit"),
        };
        assert_eq!(
            coordinator.reserve(&request).await.unwrap(),
            QuotaReservationOutcome::Reserved(reservation.clone())
        );

        let conflicting = QuotaReservationRequest {
            estimated: UsageQuantity::whole(701, UsageUnit::Byte).unwrap(),
            ..request.clone()
        };
        assert!(matches!(
            coordinator.reserve(&conflicting).await,
            Err(QuotaStoreError::ReservationConflict)
        ));

        let denied = QuotaReservationRequest {
            reservation_id: uuid::Uuid::now_v7(),
            idempotency_key: "reservation:storage:denied".to_string(),
            estimated: UsageQuantity::whole(400, UsageUnit::Byte).unwrap(),
            ..request.clone()
        };
        assert!(matches!(
            coordinator.reserve(&denied).await.unwrap(),
            QuotaReservationOutcome::HardLimit(QuotaDecision::HardLimit { .. })
        ));

        let finalized = coordinator
            .finalize(
                &reservation,
                &UsageQuantity::whole(600, UsageUnit::Byte).unwrap(),
            )
            .await
            .unwrap();
        assert!(finalized.is_allowed());
        assert_eq!(
            coordinator
                .finalize(
                    &reservation,
                    &UsageQuantity::whole(600, UsageUnit::Byte).unwrap(),
                )
                .await
                .unwrap(),
            finalized
        );
        assert!(matches!(
            coordinator.release(&reservation).await,
            Err(QuotaStoreError::ReservationConflict)
        ));

        let releasable = QuotaReservationRequest {
            reservation_id: uuid::Uuid::now_v7(),
            idempotency_key: "reservation:storage:release".to_string(),
            estimated: UsageQuantity::whole(300, UsageUnit::Byte).unwrap(),
            ..request
        };
        let releasable = match coordinator.reserve(&releasable).await.unwrap() {
            QuotaReservationOutcome::Reserved(reservation) => reservation,
            QuotaReservationOutcome::HardLimit(_) => panic!("releasable reservation must fit"),
        };
        coordinator.release(&releasable).await.unwrap();
        coordinator.release(&releasable).await.unwrap();
        assert_eq!(coordinator.policy_count(), 1);
        assert_eq!(coordinator.health().status, QuotaStoreHealthStatus::Ready);
    }
}
