//! Redis-backed cursor store for `POST /api/v1/ingest/stream` resumption (#828).
//!
//! Each ingest stream is assigned a `stream_id`; every per-line `ack` carries a
//! cursor of the form `{stream_id}-{line}`. After each ack the server persists
//! the last-processed (absolute) line number to Redis under
//! `mm:ingestcursor:{stream_id}` with a rolling 60-second TTL. A client that
//! drops mid-stream can reconnect within the window, send
//! `X-Ingest-Cursor: {stream_id}-{N}` and re-send the body; the server reads the
//! persisted line and **skips** the already-processed prefix (skip-ahead dedup),
//! resuming inserts after it. Beyond the TTL the entry is gone → `410 Gone`.
//!
//! Unlike chat-stream resumption (#815), which replays *server-generated* frames
//! it buffered, ingest input comes from the client — so this store holds only
//! the cursor *position* (the line count), not the data. The strong
//! zero-duplicate guarantee via outbox idempotency-key is deferred to #830
//! (blocked on #592); this store provides at-most-once via the line cursor.
//!
//! ## Graceful degradation
//!
//! When Redis is unavailable the store is a no-op: `record` does nothing and
//! `get` returns `None`. Fresh ingests still work end to end; resumption is
//! simply unsupported (a reconnect cursor cannot be validated → `410 Gone`).
//!
//! ## Configuration
//!
//! - `REDIS_ENABLED` (default: true) — shared with the search cache
//! - `REDIS_URL` (default: redis://localhost:6379)
//! - `FORTEMI_INGEST_CURSOR_TTL` (default: 60 seconds) — resumption window

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Default resumption window. Matches the roadmap §4 decision (60s cursor TTL).
const DEFAULT_TTL_SECONDS: u64 = 60;

/// Redis-backed per-stream ingest cursor (last-processed line + rolling TTL).
#[derive(Clone)]
pub struct IngestCursorStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// Redis connection manager (None when disabled or unreachable).
    connection: RwLock<Option<ConnectionManager>>,
    /// Resumption window in seconds.
    ttl_seconds: u64,
    /// Key prefix for per-stream cursor entries.
    prefix: String,
}

impl IngestCursorStore {
    /// Construct from environment configuration, sharing `REDIS_ENABLED` /
    /// `REDIS_URL` with the search cache. Never blocks startup longer than the
    /// 5s connect timeout; on any failure the store is disabled.
    pub async fn from_env() -> Self {
        let enabled = std::env::var("REDIS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let ttl_seconds: u64 = std::env::var("FORTEMI_INGEST_CURSOR_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_TTL_SECONDS);

        let connection = if enabled {
            connect(&redis_url, ttl_seconds).await
        } else {
            None
        };

        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(connection),
                ttl_seconds,
                prefix: "mm:ingestcursor:".to_string(),
            }),
        }
    }

    /// A disabled store (no Redis) — for tests or when resumption is off.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(None),
                ttl_seconds: DEFAULT_TTL_SECONDS,
                prefix: "mm:ingestcursor:".to_string(),
            }),
        }
    }

    /// Whether the store has a live Redis connection (resumption available).
    pub async fn is_connected(&self) -> bool {
        self.inner.connection.read().await.is_some()
    }

    /// The configured resumption window in seconds.
    pub fn ttl_seconds(&self) -> u64 {
        self.inner.ttl_seconds
    }

    fn key(&self, stream_id: &str) -> String {
        format!("{}{}", self.inner.prefix, stream_id)
    }

    /// Persist the last-processed absolute line for a stream and refresh the TTL
    /// in a single `SET key value EX ttl`. No-op (silent) when Redis is down —
    /// resumption degrades, the live stream is unaffected.
    pub async fn record(&self, stream_id: &str, last_line: u64) {
        let mut guard = self.inner.connection.write().await;
        let Some(conn) = guard.as_mut() else { return };
        let key = self.key(stream_id);
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&key, last_line, self.inner.ttl_seconds)
            .await
        {
            let diagnostic = e.to_string();
            warn!(
                operation = "set",
                reason_code = ingest_cursor_diagnostic_reason(&diagnostic),
                error_len = ingest_cursor_text_len(&diagnostic),
                key_len = ingest_cursor_text_len(&key),
                "Ingest cursor store write failed"
            );
        }
    }

    /// Read the last-processed line for a stream, or `None` when the entry has
    /// expired, never existed, or Redis is unavailable.
    pub async fn get(&self, stream_id: &str) -> Option<u64> {
        let mut guard = self.inner.connection.write().await;
        let conn = guard.as_mut()?;
        let key = self.key(stream_id);
        match conn.get::<_, Option<u64>>(&key).await {
            Ok(v) => v,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "get",
                    reason_code = ingest_cursor_diagnostic_reason(&diagnostic),
                    error_len = ingest_cursor_text_len(&diagnostic),
                    key_len = ingest_cursor_text_len(&key),
                    "Ingest cursor store lookup failed"
                );
                None
            }
        }
    }
}

/// Open a Redis connection manager with a bounded connect timeout, logging and
/// disabling (returning `None`) on any failure.
async fn connect(redis_url: &str, ttl_seconds: u64) -> Option<ConnectionManager> {
    let client = match redis::Client::open(redis_url) {
        Ok(c) => c,
        Err(e) => {
            let diagnostic = e.to_string();
            warn!(
                operation = "open",
                reason_code = ingest_cursor_diagnostic_reason(&diagnostic),
                error_len = ingest_cursor_text_len(&diagnostic),
                "Ingest cursor store Redis URL rejected; resumption disabled"
            );
            return None;
        }
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ConnectionManager::new(client),
    )
    .await
    {
        Ok(Ok(conn)) => {
            info!("Ingest cursor store enabled (TTL: {ttl_seconds}s)");
            Some(conn)
        }
        Ok(Err(e)) => {
            let diagnostic = e.to_string();
            warn!(
                operation = "connect",
                reason_code = ingest_cursor_diagnostic_reason(&diagnostic),
                error_len = ingest_cursor_text_len(&diagnostic),
                "Ingest cursor store Redis connect failed; resumption disabled"
            );
            None
        }
        Err(_) => {
            warn!("Ingest cursor store: Redis connect timed out, resumption disabled");
            None
        }
    }
}

fn ingest_cursor_text_len(value: &str) -> usize {
    value.chars().count()
}

fn ingest_cursor_diagnostic_reason(value: &str) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_store_is_noop() {
        let store = IngestCursorStore::disabled();
        assert!(!store.is_connected().await);
        // record and get must be inert without panicking.
        store.record("stream-abc", 42).await;
        assert!(store.get("stream-abc").await.is_none());
        assert_eq!(store.ttl_seconds(), DEFAULT_TTL_SECONDS);
    }

    #[test]
    fn ingest_cursor_diagnostic_reason_uses_stable_codes() {
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
                "NOAUTH Authentication required for mm:ingestcursor:stream-secret",
                "authorization_failed",
            ),
            (
                "json parser failed at line 1 column 7 with sk-cursor-secret",
                "invalid_input",
            ),
            (
                "backend returned provider.example token=secret",
                "operation_failed",
            ),
        ];

        for (diagnostic, expected) in cases {
            assert_eq!(ingest_cursor_diagnostic_reason(diagnostic), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("provider.example"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("stream-secret"));
            assert!(!expected.contains("sk-cursor-secret"));
        }
    }
}
