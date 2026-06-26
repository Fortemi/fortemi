//! The `InboundEventSource` connector contract + supporting types (#833).
//!
//! A connector pulls events from an upstream technical source (external Redis
//! Stream, SSE, Kafka), hands them to the supervisor one at a time, and commits
//! the upstream offset once the supervisor has durably written the event to the
//! shared `event_outbox`. Connectors use interior mutability (the trait takes
//! `&self`) so a single registered instance can hold its own connection/cursor.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

const INBOUND_TELEMETRY_TEXT_LEN_CAP: usize = 512;

/// Opaque upstream cursor (e.g. a Redis Stream id, Kafka offset, SSE
/// `Last-Event-ID`). Stored as text; interpreted by the owning connector.
pub type Offset = String;

/// A normalized event pulled from an upstream source, ready for the outbox.
#[derive(Debug, Clone)]
pub struct InboundEvent {
    /// Outbox `event_type` (e.g. `external.metric.v1`).
    pub event_type: String,
    /// Event payload (written verbatim into the outbox payload envelope).
    pub payload: Value,
    /// Upstream cursor to commit once the event is durably stored.
    pub offset: Offset,
}

impl InboundEvent {
    pub fn new(event_type: impl Into<String>, payload: Value, offset: impl Into<Offset>) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
            offset: offset.into(),
        }
    }
}

/// Connector-facing error. `Closed` ends the per-connector loop cleanly;
/// `Transient` triggers a backoff+retry of `next_event`.
#[derive(Debug, thiserror::Error)]
pub enum InboundError {
    /// The source is exhausted/closed; the supervisor stops the connector.
    #[error("inbound source closed")]
    Closed,
    /// A transient fetch error; the supervisor backs off and retries.
    #[error("transient inbound source error: {0}")]
    Transient(String),
}

pub type InboundResult<T> = std::result::Result<T, InboundError>;

/// Bounded length helper for user/backend-originated diagnostic strings.
pub(crate) fn telemetry_text_len(value: &str) -> usize {
    value.chars().take(INBOUND_TELEMETRY_TEXT_LEN_CAP).count()
}

/// Coarse destination class for connector endpoints.
pub(crate) fn telemetry_destination_class(raw: &str) -> &'static str {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        "empty"
    } else if value.starts_with("https://") {
        "https"
    } else if value.starts_with("http://") {
        "http"
    } else if value.starts_with("rediss://") {
        "rediss"
    } else if value.starts_with("redis://") {
        "redis"
    } else if value.contains(',') {
        "broker_list"
    } else if value.contains(':') {
        "host_port"
    } else {
        "other"
    }
}

/// Stable reason codes for connector/backend errors before broad telemetry.
pub(crate) fn inbound_error_reason_code(error: &str) -> &'static str {
    let value = error.to_ascii_lowercase();
    if value.contains("invalid")
        || value.contains("malformed")
        || value.contains("requires")
        || value.contains("empty")
        || value.contains("parse")
    {
        "invalid_config_or_payload"
    } else if value.contains("timeout") || value.contains("timed out") {
        "timeout"
    } else if value.contains("connect")
        || value.contains("connection")
        || value.contains("network")
        || value.contains("dns")
    {
        "connection_failed"
    } else if value.contains("auth")
        || value.contains("permission")
        || value.contains("denied")
        || value.contains("forbidden")
        || value.contains("unauthorized")
    {
        "authorization_failed"
    } else if value.contains("subscribe") {
        "subscribe_failed"
    } else if value.contains("commit") || value.contains("xack") {
        "commit_failed"
    } else if value.contains("dlq") || value.contains("dead-letter") {
        "dead_letter_failed"
    } else if value.contains("outbox") || value.contains("database") || value.contains("sql") {
        "storage_failed"
    } else {
        "backend_error"
    }
}

/// A pluggable inbound event source. Concrete connectors (#834 Redis Stream,
/// #835 SSE, #836 Kafka) implement this; the supervisor drives it.
#[async_trait]
pub trait InboundEventSource: Send + Sync {
    /// Block until the next upstream event is available (or the source closes).
    async fn next_event(&self) -> InboundResult<InboundEvent>;
    /// Commit the upstream offset after the event is durably stored.
    async fn commit(&self, offset: Offset) -> InboundResult<()>;
    /// Stable connector name (matches the `inbound_source.name` registration).
    fn name(&self) -> &str;
}

/// An in-memory source used to exercise the supervisor end to end without a
/// live upstream. Yields a fixed queue of events, then reports `Closed`;
/// records committed offsets for assertions.
pub struct InMemorySource {
    name: String,
    queue: Mutex<VecDeque<InboundEvent>>,
    committed: Mutex<Vec<Offset>>,
}

impl InMemorySource {
    pub fn new(name: impl Into<String>, events: Vec<InboundEvent>) -> Self {
        Self {
            name: name.into(),
            queue: Mutex::new(events.into()),
            committed: Mutex::new(Vec::new()),
        }
    }

    /// Offsets committed so far (test assertion helper).
    pub fn committed(&self) -> Vec<Offset> {
        self.committed.lock().unwrap().clone()
    }
}

#[async_trait]
impl InboundEventSource for InMemorySource {
    async fn next_event(&self) -> InboundResult<InboundEvent> {
        match self.queue.lock().unwrap().pop_front() {
            Some(ev) => Ok(ev),
            None => Err(InboundError::Closed),
        }
    }

    async fn commit(&self, offset: Offset) -> InboundResult<()> {
        self.committed.lock().unwrap().push(offset);
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn in_memory_source_drains_then_closes() {
        let src = InMemorySource::new(
            "mem",
            vec![
                InboundEvent::new("e.v1", json!({"n": 1}), "1-0"),
                InboundEvent::new("e.v1", json!({"n": 2}), "1-1"),
            ],
        );
        assert_eq!(src.name(), "mem");
        let a = src.next_event().await.unwrap();
        assert_eq!(a.offset, "1-0");
        src.commit(a.offset).await.unwrap();
        let b = src.next_event().await.unwrap();
        src.commit(b.offset).await.unwrap();
        assert!(matches!(src.next_event().await, Err(InboundError::Closed)));
        assert_eq!(src.committed(), vec!["1-0".to_string(), "1-1".to_string()]);
    }

    #[test]
    fn telemetry_destination_class_omits_raw_endpoint_parts() {
        assert_eq!(
            telemetry_destination_class("https://user:secret@example.internal/events?token=x"),
            "https"
        );
        assert_eq!(
            telemetry_destination_class("redis://user:secret@redis.internal:6379/0"),
            "redis"
        );
        assert_eq!(
            telemetry_destination_class("broker-a.internal:9092,broker-b.internal:9092"),
            "broker_list"
        );
    }

    #[test]
    fn inbound_error_reason_code_avoids_raw_error_text() {
        let raw = "connect failed for https://user:secret@example.internal/path";
        let code = inbound_error_reason_code(raw);
        assert_eq!(code, "connection_failed");
        assert!(!code.contains("secret"));
        assert!(!code.contains("example"));
        assert!(!code.contains("https://"));
    }

    #[test]
    fn telemetry_text_len_is_bounded_and_metadata_only() {
        let raw = format!(
            "mm_key_inbound\r\npostgres://user:pass@db.internal/app{}",
            "x".repeat(INBOUND_TELEMETRY_TEXT_LEN_CAP + 128)
        );

        let rendered = format!("source_name_len={}", telemetry_text_len(&raw));

        assert_eq!(telemetry_text_len(&raw), INBOUND_TELEMETRY_TEXT_LEN_CAP);
        assert!(rendered.contains("source_name_len=512"));

        for raw_fragment in [
            "mm_key_inbound",
            "postgres://user:pass",
            "db.internal",
            "\r",
            "\n",
        ] {
            assert!(
                !rendered.contains(raw_fragment),
                "raw value leaked: {raw_fragment:?}"
            );
        }
    }
}
