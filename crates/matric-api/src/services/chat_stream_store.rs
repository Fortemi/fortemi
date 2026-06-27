//! Redis-backed buffer for SSE chat-stream resumption (#815, epic #811 A4).
//!
//! Each `POST /api/v1/chat/stream` response is assigned a session id. Every
//! emitted frame (`delta`/`done`/`error`) is appended to a Redis list keyed by a
//! session fingerprint, with a rolling 60-second TTL. SSE events carry an id of
//! the form `{session}-{seq}`, so a client reconnecting with a `Last-Event-ID`
//! header lets the server replay the frames *after* that sequence without
//! duplicating already-delivered tokens.
//!
//! The generation task persists frames to Redis independently of the live
//! client connection, so a client that drops mid-stream can reconnect within
//! the TTL window and resume from where it left off.
//!
//! ## Graceful degradation
//!
//! When Redis is unavailable the store is a no-op: `append` does nothing and
//! `read_after` returns empty. The live stream still works end to end —
//! resumption is simply unsupported until Redis is reachable.
//!
//! ## Configuration
//!
//! - `REDIS_ENABLED` (default: true) — shared with the search cache
//! - `REDIS_URL` (default: redis://localhost:6379)
//! - `FORTEMI_CHAT_STREAM_TTL` (default: 60 seconds) — resumption window

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Default resumption window. Matches the roadmap §6 decision (60s cursor TTL).
const DEFAULT_TTL_SECONDS: u64 = 60;

/// One buffered SSE frame, as persisted in the per-session Redis list.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredFrame {
    /// Monotonic per-session sequence number (1-based).
    pub seq: u64,
    /// SSE event name: `"delta"`, `"done"`, or `"error"`.
    pub event: String,
    /// JSON-encoded event payload.
    pub data: String,
}

impl std::fmt::Debug for StoredFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredFrame")
            .field("seq", &self.seq)
            .field("event_len", &chat_store_text_len(&self.event))
            .field("data_len", &chat_store_text_len(&self.data))
            .field("data_class", &chat_store_json_text_class(&self.data))
            .finish()
    }
}

impl StoredFrame {
    /// Whether this frame terminates the stream (`done` or `error`).
    pub fn is_terminal(&self) -> bool {
        self.event == "done" || self.event == "error"
    }
}

fn chat_store_text_len(value: &str) -> usize {
    value.chars().count()
}

fn chat_store_session_fingerprint(session: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session.as_bytes());
    hex::encode(hasher.finalize())
}

fn chat_store_json_text_class(value: &str) -> &'static str {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Null) => "null",
        Ok(serde_json::Value::Bool(_)) => "bool",
        Ok(serde_json::Value::Number(_)) => "number",
        Ok(serde_json::Value::String(_)) => "string",
        Ok(serde_json::Value::Array(_)) => "array",
        Ok(serde_json::Value::Object(_)) => "object",
        Err(_) => "invalid",
    }
}

fn chat_store_diagnostic_reason(value: &str) -> &'static str {
    let value = value.to_ascii_lowercase();
    if value.contains("invalid") || value.contains("parse") {
        "invalid_input"
    } else if value.contains("timeout") || value.contains("timed out") {
        "timeout"
    } else if value.contains("connect") || value.contains("connection") {
        "connection_failed"
    } else if value.contains("auth")
        || value.contains("permission")
        || value.contains("denied")
        || value.contains("unauthorized")
    {
        "authorization_failed"
    } else if value.contains("serialize") || value.contains("json") {
        "serialization_failed"
    } else {
        "operation_failed"
    }
}

/// A resumption cursor parsed from a `Last-Event-ID` of the form
/// `{session}-{seq}`.
#[derive(Clone, PartialEq, Eq)]
pub struct ResumeCursor {
    pub session: String,
    pub after_seq: u64,
}

impl std::fmt::Debug for ResumeCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumeCursor")
            .field("session_len", &chat_store_text_len(&self.session))
            .field("after_seq", &self.after_seq)
            .finish()
    }
}

impl ResumeCursor {
    /// Parse a `Last-Event-ID` value of the form `{session}-{seq}`.
    ///
    /// The session id is a UUID (which itself contains hyphens), so the split
    /// is on the **last** hyphen: everything before it is the session, the
    /// trailing component is the sequence number. Returns `None` for any value
    /// that does not match this shape.
    pub fn parse(last_event_id: &str) -> Option<Self> {
        let idx = last_event_id.rfind('-')?;
        let session = &last_event_id[..idx];
        let seq_str = &last_event_id[idx + 1..];
        if session.is_empty() || seq_str.is_empty() {
            return None;
        }
        let after_seq = seq_str.parse::<u64>().ok()?;
        Some(Self {
            session: session.to_string(),
            after_seq,
        })
    }
}

/// Redis-backed per-session frame buffer for chat-stream resumption.
#[derive(Clone)]
pub struct ChatStreamStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// Redis connection manager (None when disabled or unreachable).
    connection: RwLock<Option<ConnectionManager>>,
    /// Resumption window in seconds.
    ttl_seconds: u64,
    /// Key prefix for per-session lists.
    prefix: String,
}

impl ChatStreamStore {
    /// Construct from environment configuration, sharing `REDIS_ENABLED` /
    /// `REDIS_URL` with the search cache. Never blocks server startup longer
    /// than the 5s connect timeout; on any failure the store is disabled.
    pub async fn from_env() -> Self {
        let enabled = std::env::var("REDIS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let ttl_seconds: u64 = std::env::var("FORTEMI_CHAT_STREAM_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_TTL_SECONDS);

        let connection = if enabled {
            match redis::Client::open(redis_url.as_str()) {
                Ok(client) => match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    ConnectionManager::new(client),
                )
                .await
                {
                    Ok(Ok(conn)) => {
                        info!(
                            "Chat-stream resumption store enabled (TTL: {}s)",
                            ttl_seconds
                        );
                        Some(conn)
                    }
                    Ok(Err(e)) => {
                        let diagnostic = e.to_string();
                        warn!(
                            operation = "connect",
                            reason_code = chat_store_diagnostic_reason(&diagnostic),
                            error_len = chat_store_text_len(&diagnostic),
                            "Chat-stream store Redis connect failed; resumption disabled"
                        );
                        None
                    }
                    Err(_) => {
                        warn!("Chat-stream store: Redis connect timed out, resumption disabled");
                        None
                    }
                },
                Err(e) => {
                    let diagnostic = e.to_string();
                    warn!(
                        operation = "open",
                        reason_code = chat_store_diagnostic_reason(&diagnostic),
                        error_len = chat_store_text_len(&diagnostic),
                        "Chat-stream store Redis URL rejected; resumption disabled"
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(connection),
                ttl_seconds,
                prefix: "mm:chatstream:".to_string(),
            }),
        }
    }

    /// A disabled store (no Redis) — for tests or when resumption is off.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(None),
                ttl_seconds: DEFAULT_TTL_SECONDS,
                prefix: "mm:chatstream:".to_string(),
            }),
        }
    }

    /// Whether the store has a live Redis connection (i.e. resumption is
    /// actually available).
    pub async fn is_connected(&self) -> bool {
        self.inner.connection.read().await.is_some()
    }

    /// The configured resumption window in seconds.
    pub fn ttl_seconds(&self) -> u64 {
        self.inner.ttl_seconds
    }

    fn key(&self, session: &str) -> String {
        format!(
            "{}{}",
            self.inner.prefix,
            chat_store_session_fingerprint(session)
        )
    }

    /// Append a frame to the session buffer and refresh the TTL. No-op (and
    /// silent) when Redis is unavailable — resumption degrades, the live stream
    /// is unaffected.
    pub async fn append(&self, session: &str, frame: &StoredFrame) {
        let mut guard = self.inner.connection.write().await;
        let conn = match guard.as_mut() {
            Some(c) => c,
            None => return,
        };
        let key = self.key(session);
        let payload = match serde_json::to_string(frame) {
            Ok(s) => s,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "serialize_frame",
                    reason_code = chat_store_diagnostic_reason(&diagnostic),
                    error_len = chat_store_text_len(&diagnostic),
                    "Chat-stream store frame serialization failed"
                );
                return;
            }
        };
        if let Err(e) = conn.rpush::<_, _, ()>(&key, payload).await {
            let diagnostic = e.to_string();
            warn!(
                operation = "rpush",
                reason_code = chat_store_diagnostic_reason(&diagnostic),
                error_len = chat_store_text_len(&diagnostic),
                "Chat-stream store append failed"
            );
            return;
        }
        // Roll the TTL forward on every append so the window measures time
        // since the last activity on the stream.
        let _ = conn
            .expire::<_, ()>(&key, self.inner.ttl_seconds as i64)
            .await;
    }

    /// Return all buffered frames with `seq > after_seq`, in order. Empty when
    /// Redis is unavailable or the session has expired / never existed.
    pub async fn read_after(&self, session: &str, after_seq: u64) -> Vec<StoredFrame> {
        let mut guard = self.inner.connection.write().await;
        let conn = match guard.as_mut() {
            Some(c) => c,
            None => return Vec::new(),
        };
        let key = self.key(session);
        let raw: Vec<String> = match conn.lrange(&key, 0, -1).await {
            Ok(v) => v,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "lrange",
                    reason_code = chat_store_diagnostic_reason(&diagnostic),
                    error_len = chat_store_text_len(&diagnostic),
                    "Chat-stream store replay read failed"
                );
                return Vec::new();
            }
        };
        raw.into_iter()
            .filter_map(|s| serde_json::from_str::<StoredFrame>(&s).ok())
            .filter(|f| f.seq > after_seq)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_cursor_parses_uuid_session_and_seq() {
        let c = ResumeCursor::parse("550e8400-e29b-41d4-a716-446655440000-42").unwrap();
        assert_eq!(c.session, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(c.after_seq, 42);
    }

    #[test]
    fn resume_cursor_parses_simple_session() {
        let c = ResumeCursor::parse("abc-7").unwrap();
        assert_eq!(c.session, "abc");
        assert_eq!(c.after_seq, 7);
    }

    #[test]
    fn resume_cursor_rejects_malformed() {
        assert!(ResumeCursor::parse("").is_none());
        assert!(ResumeCursor::parse("noseq").is_none());
        assert!(ResumeCursor::parse("-5").is_none()); // empty session
        assert!(ResumeCursor::parse("abc-").is_none()); // empty seq
        assert!(ResumeCursor::parse("abc-notanumber").is_none());
    }

    #[test]
    fn resume_cursor_roundtrips_with_event_id_format() {
        let session = "11111111-2222-3333-4444-555555555555";
        for seq in [1u64, 9, 100, 99999] {
            let event_id = format!("{session}-{seq}");
            let c = ResumeCursor::parse(&event_id).unwrap();
            assert_eq!(c.session, session);
            assert_eq!(c.after_seq, seq);
        }
    }

    #[test]
    fn resume_cursor_debug_redacts_session_id() {
        let cursor =
            ResumeCursor::parse("tenant-secret-session-11111111-2222-3333-4444-555555555555-42")
                .unwrap();

        let rendered = format!("{cursor:?}");

        assert!(rendered.contains("session_len"));
        assert!(rendered.contains("after_seq: 42"));
        assert!(!rendered.contains("tenant-secret-session"));
        assert!(!rendered.contains("11111111-2222-3333-4444-555555555555"));
    }

    #[test]
    fn chat_stream_redis_key_uses_session_fingerprint_without_raw_session_id() {
        let store = ChatStreamStore::disabled();
        let session = "tenant-secret-session-11111111-2222-3333-4444-555555555555";
        let key = store.key(session);

        assert!(key.starts_with("mm:chatstream:"));
        assert_eq!(key.len(), "mm:chatstream:".len() + 64);
        assert_eq!(key, store.key(session));
        assert_ne!(key, store.key("different-session"));
        assert!(!key.contains(session));
        assert!(!key.contains("tenant-secret-session"));
        assert!(!key.contains("11111111-2222-3333-4444-555555555555"));
    }

    #[test]
    fn chat_store_diagnostic_reason_uses_stable_codes() {
        let cases = [
            (
                "invalid redis url redis://user:pass@cache.internal:6379/0?token=secret",
                "invalid_input",
            ),
            (
                "connection refused at redis://cache.internal:6379 with token=secret",
                "connection_failed",
            ),
            ("operation timed out connecting to private-cache", "timeout"),
            (
                "NOAUTH Authentication required for tenant secret",
                "authorization_failed",
            ),
            (
                "json parser failed at line 1 column 7 with sk-json-secret",
                "invalid_input",
            ),
            (
                "backend returned provider.example token=secret",
                "operation_failed",
            ),
        ];

        for (diagnostic, expected) in cases {
            assert_eq!(chat_store_diagnostic_reason(diagnostic), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("provider.example"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("sk-json-secret"));
        }
    }

    #[test]
    fn stored_frame_terminal_classification() {
        let delta = StoredFrame {
            seq: 1,
            event: "delta".into(),
            data: "{}".into(),
        };
        let done = StoredFrame {
            seq: 2,
            event: "done".into(),
            data: "{}".into(),
        };
        let err = StoredFrame {
            seq: 3,
            event: "error".into(),
            data: "{}".into(),
        };
        assert!(!delta.is_terminal());
        assert!(done.is_terminal());
        assert!(err.is_terminal());
    }

    #[tokio::test]
    async fn disabled_store_is_noop() {
        let store = ChatStreamStore::disabled();
        assert!(!store.is_connected().await);
        // append and read_after must not panic and must be inert.
        store
            .append(
                "sess",
                &StoredFrame {
                    seq: 1,
                    event: "delta".into(),
                    data: "{\"content\":\"hi\"}".into(),
                },
            )
            .await;
        assert!(store.read_after("sess", 0).await.is_empty());
        assert_eq!(store.ttl_seconds(), DEFAULT_TTL_SECONDS);
    }
}
