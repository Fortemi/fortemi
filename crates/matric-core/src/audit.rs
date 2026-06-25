//! Core audit event contract and first CE sink implementation (#910).
//!
//! Audit events are sanitized before buffering or sink dispatch. This keeps
//! future durable/EE sinks from receiving raw secret-shaped attributes by
//! accident.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::time::Instant;
use uuid::Uuid;

const AUDIT_SCHEMA_VERSION: u16 = 1;
const MAX_ATTR_STRING_BYTES: usize = 512;
const REDACTED: &str = "[REDACTED]";
const TRUNCATED: &str = "[TRUNCATED]";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub schema_version: u16,
    pub id: Uuid,
    pub idempotency_key: Option<String>,
    pub event_ts: DateTime<Utc>,
    pub observed_ts: DateTime<Utc>,
    pub tenant_id: Option<String>,
    pub principal_id: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<String>,
    pub correlation_id: Option<String>,
    pub category: String,
    pub action: String,
    pub outcome: AuditOutcome,
    pub reason: Option<String>,
    pub severity: AuditSeverity,
    pub failure_policy: AuditFailurePolicy,
    pub visibility: AuditVisibilityClass,
    pub retention: AuditRetentionClass,
    pub source: AuditSource,
    pub attrs: HashMap<String, Value>,
}

impl AuditEvent {
    pub fn new(
        category: impl Into<String>,
        action: impl Into<String>,
        outcome: AuditOutcome,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: AUDIT_SCHEMA_VERSION,
            id: Uuid::new_v4(),
            idempotency_key: None,
            event_ts: now,
            observed_ts: now,
            tenant_id: None,
            principal_id: None,
            resource_kind: None,
            resource_id: None,
            correlation_id: None,
            category: category.into(),
            action: action.into(),
            outcome,
            reason: None,
            severity: AuditSeverity::Info,
            failure_policy: AuditFailurePolicy::BestEffort,
            visibility: AuditVisibilityClass::SystemAudit,
            retention: AuditRetentionClass::Security,
            source: AuditSource::Core,
            attrs: HashMap::new(),
        }
        .sanitized()
    }

    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.attrs.insert(key.into(), value.into());
        self.sanitized()
    }

    pub fn with_principal(mut self, principal_id: impl Into<String>) -> Self {
        self.principal_id = Some(principal_id.into());
        self.sanitized()
    }

    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self.sanitized()
    }

    pub fn with_resource(
        mut self,
        resource_kind: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource_kind = Some(resource_kind.into());
        self.resource_id = Some(resource_id.into());
        self.sanitized()
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self.sanitized()
    }

    pub fn with_failure_policy(mut self, failure_policy: AuditFailurePolicy) -> Self {
        self.failure_policy = failure_policy;
        self
    }

    pub fn sanitized(mut self) -> Self {
        self.idempotency_key = sanitize_optional_string(self.idempotency_key, false);
        self.tenant_id = sanitize_optional_string(self.tenant_id, false);
        self.principal_id = sanitize_optional_string(self.principal_id, false);
        self.resource_kind = sanitize_optional_string(self.resource_kind, false);
        self.resource_id = sanitize_optional_string(self.resource_id, false);
        self.correlation_id = sanitize_optional_string(self.correlation_id, false);
        self.category = sanitize_string(self.category, false);
        self.action = sanitize_string(self.action, false);
        self.reason = sanitize_optional_string(self.reason, false);
        self.attrs = sanitize_attrs(self.attrs);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Denied,
    Failure,
    Error,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditSeverity {
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditFailurePolicy {
    BestEffort,
    DegradeWithAlert,
    FailClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditVisibilityClass {
    TenantAudit,
    PrivacyAudit,
    SystemAudit,
    SecurityRestricted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditRetentionClass {
    Operational,
    Security,
    Privacy,
    LegalHoldEligible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditSource {
    Core,
    Api,
    Worker,
    Plugin,
    ExternalClient,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditBufferConfig {
    pub max_events: usize,
    pub flush_count: usize,
    pub flush_interval: Duration,
}

impl Default for AuditBufferConfig {
    fn default() -> Self {
        Self {
            max_events: 1024,
            flush_count: 128,
            flush_interval: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuditBufferStats {
    pub pending: usize,
    pub emitted: u64,
    pub dropped_overflow: u64,
    pub sink_failures: u64,
}

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit buffer capacity must be greater than zero")]
    InvalidBufferConfig,
    #[error("audit buffer overflow for fail-closed event")]
    BufferOverflow,
    #[error("audit sink failed: {0}")]
    Sink(String),
}

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn emit(&self, event: AuditEvent) -> Result<(), AuditError>;

    async fn flush(&self) -> Result<(), AuditError>;
}

#[derive(Clone, Debug, Default)]
pub struct TracingSink;

#[async_trait]
impl AuditSink for TracingSink {
    async fn emit(&self, event: AuditEvent) -> Result<(), AuditError> {
        let event = event.sanitized();
        tracing::info!(
            target: "fortemi.audit",
            audit_schema_version = event.schema_version,
            audit_id = %event.id,
            audit_idempotency_key = ?event.idempotency_key,
            audit_event_ts = %event.event_ts,
            audit_observed_ts = %event.observed_ts,
            audit_tenant_id = ?event.tenant_id,
            audit_principal_id = ?event.principal_id,
            audit_resource_kind = ?event.resource_kind,
            audit_resource_id = ?event.resource_id,
            audit_correlation_id = ?event.correlation_id,
            audit_category = %event.category,
            audit_action = %event.action,
            audit_outcome = ?event.outcome,
            audit_reason = ?event.reason,
            audit_severity = ?event.severity,
            audit_failure_policy = ?event.failure_policy,
            audit_visibility = ?event.visibility,
            audit_retention = ?event.retention,
            audit_source = ?event.source,
            audit_attrs = ?event.attrs,
            "audit event"
        );
        Ok(())
    }

    async fn flush(&self) -> Result<(), AuditError> {
        Ok(())
    }
}

pub struct AuditBuffer<S> {
    sink: S,
    config: AuditBufferConfig,
    pending: VecDeque<AuditEvent>,
    last_flush: Instant,
    stats: AuditBufferStats,
}

impl<S> AuditBuffer<S>
where
    S: AuditSink,
{
    pub fn new(sink: S, config: AuditBufferConfig) -> Result<Self, AuditError> {
        if config.max_events == 0 || config.flush_count == 0 {
            return Err(AuditError::InvalidBufferConfig);
        }
        Ok(Self {
            sink,
            config,
            pending: VecDeque::new(),
            last_flush: Instant::now(),
            stats: AuditBufferStats::default(),
        })
    }

    pub fn stats(&self) -> AuditBufferStats {
        let mut stats = self.stats.clone();
        stats.pending = self.pending.len();
        stats
    }

    pub async fn emit(&mut self, event: AuditEvent) -> Result<(), AuditError> {
        let event = event.sanitized();
        if self.pending.len() >= self.config.max_events {
            if event.failure_policy == AuditFailurePolicy::FailClosed {
                return Err(AuditError::BufferOverflow);
            }
            self.pending.pop_front();
            self.stats.dropped_overflow += 1;
        }
        self.pending.push_back(event);

        if self.pending.len() >= self.config.flush_count
            || self.last_flush.elapsed() >= self.config.flush_interval
        {
            self.flush().await?;
        }

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), AuditError> {
        while let Some(event) = self.pending.pop_front() {
            match self.sink.emit(event).await {
                Ok(()) => self.stats.emitted += 1,
                Err(err) => {
                    self.stats.sink_failures += 1;
                    self.pending.clear();
                    return Err(err);
                }
            }
        }
        self.sink.flush().await?;
        self.last_flush = Instant::now();
        Ok(())
    }

    pub async fn shutdown_flush(&mut self) -> Result<(), AuditError> {
        self.flush().await
    }
}

fn sanitize_attrs(attrs: HashMap<String, Value>) -> HashMap<String, Value> {
    attrs
        .into_iter()
        .map(|(key, value)| {
            let safe_key = sanitize_string(key, false);
            let sensitive = is_sensitive_key(&safe_key);
            let safe_value = sanitize_value(value, sensitive);
            (safe_key, safe_value)
        })
        .collect()
}

fn sanitize_value(value: Value, sensitive: bool) -> Value {
    match value {
        Value::String(value) => Value::String(sanitize_string(value, sensitive)),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| sanitize_value(value, sensitive))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let key = sanitize_string(key, false);
                    let child_sensitive = sensitive || is_sensitive_key(&key);
                    (key, sanitize_value(value, child_sensitive))
                })
                .collect(),
        ),
        other => other,
    }
}

fn sanitize_optional_string(value: Option<String>, sensitive: bool) -> Option<String> {
    value.map(|value| sanitize_string(value, sensitive))
}

fn sanitize_string(mut value: String, sensitive: bool) -> String {
    if sensitive || looks_secret_shaped(&value) {
        return REDACTED.to_string();
    }
    value = value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            '|' | ';' => ',',
            _ => ch,
        })
        .collect();
    if value.len() > MAX_ATTR_STRING_BYTES {
        value.truncate(MAX_ATTR_STRING_BYTES);
        value.push_str(TRUNCATED);
    }
    value
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("passphrase")
        || key.contains("private_key")
        || key.contains("api_key")
        || key.contains("authorization")
        || key.contains("dsn")
}

fn looks_secret_shaped(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("mm_key_")
        || lower.starts_with("mm_at_")
        || lower.contains("-----begin private key-----")
        || lower.contains("client_secret=")
        || lower.contains("password=")
        || lower.contains("postgres://")
        || lower.contains("postgresql://")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::time::{advance, pause};

    use super::*;

    #[derive(Clone, Default)]
    struct RecordingSink {
        events: Arc<Mutex<Vec<AuditEvent>>>,
        fail: bool,
    }

    #[async_trait]
    impl AuditSink for RecordingSink {
        async fn emit(&self, event: AuditEvent) -> Result<(), AuditError> {
            if self.fail {
                return Err(AuditError::Sink("test failure".to_string()));
            }
            self.events.lock().unwrap().push(event);
            Ok(())
        }

        async fn flush(&self) -> Result<(), AuditError> {
            Ok(())
        }
    }

    fn event(action: &str) -> AuditEvent {
        AuditEvent::new("test", action, AuditOutcome::Success)
    }

    #[test]
    fn audit_event_sanitizes_secret_and_injection_attrs_before_buffering() {
        let event = event("startup\nready|check")
            .with_attr("authorization", "Bearer abc123")
            .with_attr("api_key", "mm_key_supersecret")
            .with_attr("private_key", "-----BEGIN PRIVATE KEY-----abc")
            .with_attr("filename", "evil\nname|part")
            .with_attr("nested", json!({"client_secret": "secret-value"}))
            .with_attr("long", "x".repeat(MAX_ATTR_STRING_BYTES + 20));

        assert_eq!(event.action, "startup ready,check");
        assert_eq!(event.attrs["authorization"], REDACTED);
        assert_eq!(event.attrs["api_key"], REDACTED);
        assert_eq!(event.attrs["private_key"], REDACTED);
        assert_eq!(event.attrs["filename"], "evil name,part");
        assert_eq!(event.attrs["nested"]["client_secret"], REDACTED);
        assert!(event.attrs["long"].as_str().unwrap().ends_with(TRUNCATED));
    }

    #[tokio::test]
    async fn audit_buffer_flushes_by_count() {
        let sink = RecordingSink::default();
        let events = sink.events.clone();
        let mut buffer = AuditBuffer::new(
            sink,
            AuditBufferConfig {
                max_events: 4,
                flush_count: 2,
                flush_interval: Duration::from_secs(60),
            },
        )
        .unwrap();

        buffer.emit(event("one")).await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 0);
        buffer.emit(event("two")).await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 2);
        assert_eq!(buffer.stats().emitted, 2);
    }

    #[tokio::test]
    async fn audit_buffer_flushes_by_interval() {
        pause();
        let sink = RecordingSink::default();
        let events = sink.events.clone();
        let mut buffer = AuditBuffer::new(
            sink,
            AuditBufferConfig {
                max_events: 4,
                flush_count: 4,
                flush_interval: Duration::from_secs(5),
            },
        )
        .unwrap();

        buffer.emit(event("one")).await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 0);
        advance(Duration::from_secs(5)).await;
        buffer.emit(event("two")).await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn audit_buffer_shutdown_flushes_pending_events() {
        let sink = RecordingSink::default();
        let events = sink.events.clone();
        let mut buffer = AuditBuffer::new(
            sink,
            AuditBufferConfig {
                max_events: 4,
                flush_count: 4,
                flush_interval: Duration::from_secs(60),
            },
        )
        .unwrap();

        buffer.emit(event("pending")).await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 0);
        buffer.shutdown_flush().await.unwrap();
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn audit_buffer_overflow_drops_oldest_for_best_effort() {
        let sink = RecordingSink::default();
        let events = sink.events.clone();
        let mut buffer = AuditBuffer::new(
            sink,
            AuditBufferConfig {
                max_events: 1,
                flush_count: 10,
                flush_interval: Duration::from_secs(60),
            },
        )
        .unwrap();

        buffer.emit(event("old")).await.unwrap();
        buffer.emit(event("new")).await.unwrap();
        assert_eq!(buffer.stats().dropped_overflow, 1);
        buffer.flush().await.unwrap();
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "new");
    }

    #[tokio::test]
    async fn audit_buffer_overflow_fails_closed_for_sensitive_events() {
        let mut buffer = AuditBuffer::new(
            RecordingSink::default(),
            AuditBufferConfig {
                max_events: 1,
                flush_count: 10,
                flush_interval: Duration::from_secs(60),
            },
        )
        .unwrap();

        buffer.emit(event("old")).await.unwrap();
        let result = buffer
            .emit(event("sensitive").with_failure_policy(AuditFailurePolicy::FailClosed))
            .await;
        assert!(matches!(result, Err(AuditError::BufferOverflow)));
    }

    #[tokio::test]
    async fn audit_buffer_reports_sink_failure() {
        let mut buffer = AuditBuffer::new(
            RecordingSink {
                fail: true,
                ..RecordingSink::default()
            },
            AuditBufferConfig {
                max_events: 4,
                flush_count: 1,
                flush_interval: Duration::from_secs(60),
            },
        )
        .unwrap();

        let result = buffer.emit(event("fails")).await;
        assert!(matches!(result, Err(AuditError::Sink(_))));
        assert_eq!(buffer.stats().sink_failures, 1);
    }

    #[test]
    fn audit_failure_policy_represents_low_risk_and_sensitive_classes() {
        let low_risk = event("startup");
        let sensitive = event("key.delete").with_failure_policy(AuditFailurePolicy::FailClosed);

        assert_eq!(low_risk.failure_policy, AuditFailurePolicy::BestEffort);
        assert_eq!(sensitive.failure_policy, AuditFailurePolicy::FailClosed);
    }
}
