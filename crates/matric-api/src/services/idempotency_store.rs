//! Redis-backed idempotency store for incoming webhook receivers (#822).
//!
//! Clients may send an `Idempotency-Key` header on
//! `POST /api/v1/webhooks/incoming/{slug}`. The first request with a given key
//! is processed normally and its accepted response is cached under
//! a fingerprinted Redis key for 24 hours, alongside a hash of the request body.
//!
//! - Repeat key + **matching** body hash → the cached response is replayed
//!   verbatim (no re-processing, no duplicate outbox row).
//! - Repeat key + **different** body hash → `409 Conflict` (the key was reused
//!   for a different payload).
//! - No header → processed normally (idempotency is opt-in).
//!
//! ## Graceful degradation
//!
//! When Redis is unavailable the store is a no-op: `get` returns `None` and
//! `store` does nothing. Requests are still processed; only replay-dedup is
//! unavailable. (At-least-once delivery remains; the outbox consumer should
//! tolerate occasional duplicates.)
//!
//! ## Configuration
//!
//! - `REDIS_ENABLED` (default: true) — shared with the search cache
//! - `REDIS_URL` (default: redis://localhost:6379)
//! - `FORTEMI_IDEMPOTENCY_TTL` (default: 86400 seconds / 24h)

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Default replay window: 24 hours (roadmap §4.2 Idempotency).
const DEFAULT_TTL_SECONDS: u64 = 86_400;

/// Cached outcome of a processed inbound webhook, keyed by `Idempotency-Key`.
#[derive(Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    /// Hex SHA-256 of the raw request body, used to detect key reuse with a
    /// different payload.
    pub body_hash: String,
    /// HTTP status of the cached response (currently always the accepted 200).
    pub response_status: u16,
    /// The JSON response body to replay.
    pub response_body: serde_json::Value,
}

impl std::fmt::Debug for IdempotencyRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdempotencyRecord")
            .field("body_hash_len", &idempotency_text_len(&self.body_hash))
            .field("response_status", &self.response_status)
            .field(
                "response_body_class",
                &idempotency_json_class(&self.response_body),
            )
            .field(
                "response_body_len",
                &idempotency_text_len(&self.response_body.to_string()),
            )
            .finish()
    }
}

fn idempotency_json_class(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Redis-backed idempotency store for incoming webhooks.
#[derive(Clone)]
pub struct IdempotencyStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// Redis connection manager (None when disabled or unreachable).
    connection: RwLock<Option<ConnectionManager>>,
    /// Replay window in seconds.
    ttl_seconds: u64,
    /// Key prefix (`idem:`); full key is `idem:{sha256(slug + key)}`.
    prefix: String,
}

impl IdempotencyStore {
    /// Construct from environment configuration, sharing `REDIS_ENABLED` /
    /// `REDIS_URL` with the other Redis-backed stores. Never blocks startup
    /// longer than the 5s connect timeout; on any failure the store is disabled.
    pub async fn from_env() -> Self {
        let enabled = std::env::var("REDIS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let ttl_seconds: u64 = std::env::var("FORTEMI_IDEMPOTENCY_TTL")
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
                prefix: "idem:".to_string(),
            }),
        }
    }

    /// A disabled store (no Redis) — for tests or when dedup is off.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(None),
                ttl_seconds: DEFAULT_TTL_SECONDS,
                prefix: "idem:".to_string(),
            }),
        }
    }

    /// Whether the store has a live Redis connection (dedup available).
    pub async fn is_connected(&self) -> bool {
        self.inner.connection.read().await.is_some()
    }

    /// The configured replay window in seconds.
    pub fn ttl_seconds(&self) -> u64 {
        self.inner.ttl_seconds
    }

    fn key(&self, slug: &str, idem_key: &str) -> String {
        format!(
            "{}{}",
            self.inner.prefix,
            idempotency_key_fingerprint(slug, idem_key)
        )
    }

    /// Look up a cached outcome for `(slug, idem_key)`. Returns `None` when the
    /// entry has expired, never existed, or Redis is unavailable.
    pub async fn get(&self, slug: &str, idem_key: &str) -> Option<IdempotencyRecord> {
        let mut guard = self.inner.connection.write().await;
        let conn = guard.as_mut()?;
        let key = self.key(slug, idem_key);
        match conn.get::<_, Option<String>>(&key).await {
            Ok(Some(raw)) => match serde_json::from_str(&raw) {
                Ok(rec) => Some(rec),
                Err(e) => {
                    let diagnostic = e.to_string();
                    warn!(
                        operation = "deserialize_record",
                        reason_code = idempotency_diagnostic_reason(&diagnostic),
                        error_len = idempotency_text_len(&diagnostic),
                        key_len = idempotency_text_len(&key),
                        "Idempotency store cached record could not be decoded"
                    );
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "get",
                    reason_code = idempotency_diagnostic_reason(&diagnostic),
                    error_len = idempotency_text_len(&diagnostic),
                    "Idempotency store lookup failed"
                );
                None
            }
        }
    }

    /// Cache the outcome for `(slug, idem_key)` with the configured TTL.
    /// No-op (silent) when Redis is down — the request already succeeded.
    pub async fn store(&self, slug: &str, idem_key: &str, record: &IdempotencyRecord) {
        let mut guard = self.inner.connection.write().await;
        let Some(conn) = guard.as_mut() else { return };
        let key = self.key(slug, idem_key);
        let payload = match serde_json::to_string(record) {
            Ok(p) => p,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "serialize_record",
                    reason_code = idempotency_diagnostic_reason(&diagnostic),
                    error_len = idempotency_text_len(&diagnostic),
                    "Idempotency store cached record serialization failed"
                );
                return;
            }
        };
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&key, payload, self.inner.ttl_seconds)
            .await
        {
            let diagnostic = e.to_string();
            warn!(
                operation = "set",
                reason_code = idempotency_diagnostic_reason(&diagnostic),
                error_len = idempotency_text_len(&diagnostic),
                key_len = idempotency_text_len(&key),
                "Idempotency store write failed"
            );
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
                reason_code = idempotency_diagnostic_reason(&diagnostic),
                error_len = idempotency_text_len(&diagnostic),
                "Idempotency store Redis URL rejected; dedup disabled"
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
            info!("Idempotency store enabled (TTL: {ttl_seconds}s)");
            Some(conn)
        }
        Ok(Err(e)) => {
            let diagnostic = e.to_string();
            warn!(
                operation = "connect",
                reason_code = idempotency_diagnostic_reason(&diagnostic),
                error_len = idempotency_text_len(&diagnostic),
                "Idempotency store Redis connect failed; dedup disabled"
            );
            None
        }
        Err(_) => {
            warn!("Idempotency store: Redis connect timed out, dedup disabled");
            None
        }
    }
}

fn idempotency_text_len(value: &str) -> usize {
    value.chars().count()
}

fn idempotency_key_fingerprint(slug: &str, idem_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(slug.as_bytes());
    hasher.update([0]);
    hasher.update(idem_key.as_bytes());
    hex::encode(hasher.finalize())
}

fn idempotency_diagnostic_reason(value: &str) -> &'static str {
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
        let store = IdempotencyStore::disabled();
        assert!(!store.is_connected().await);
        let rec = IdempotencyRecord {
            body_hash: "abc".to_string(),
            response_status: 200,
            response_body: serde_json::json!({"status": "accepted"}),
        };
        // store and get must be inert without panicking.
        store.store("slug", "key-1", &rec).await;
        assert!(store.get("slug", "key-1").await.is_none());
        assert_eq!(store.ttl_seconds(), DEFAULT_TTL_SECONDS);
    }

    #[test]
    fn idempotency_record_json_persistence_preserves_replay_body() {
        let rec = IdempotencyRecord {
            body_hash: "body-hash-secret-value".to_string(),
            response_status: 200,
            response_body: serde_json::json!({
                "status": "accepted",
                "token": "payload-secret-token",
                "callback": "https://provider.example/hook?api_key=payload-secret-token"
            }),
        };

        let json = serde_json::to_string(&rec).unwrap();
        let back: IdempotencyRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(back.body_hash, rec.body_hash);
        assert_eq!(back.response_status, rec.response_status);
        assert_eq!(back.response_body, rec.response_body);
    }

    #[test]
    fn idempotency_record_debug_redacts_cached_payload_and_hash() {
        let rec = IdempotencyRecord {
            body_hash: "body-hash-secret-value".to_string(),
            response_status: 200,
            response_body: serde_json::json!({
                "status": "accepted",
                "token": "payload-secret-token",
                "callback": "https://provider.example/hook?api_key=payload-secret-token"
            }),
        };

        let rendered = format!("{rec:?}");

        assert!(rendered.contains("IdempotencyRecord"));
        assert!(rendered.contains("body_hash_len"));
        assert!(rendered.contains("response_body_class"));
        assert!(rendered.contains("response_body_len"));
        assert!(!rendered.contains("body-hash-secret-value"));
        assert!(!rendered.contains("payload-secret-token"));
        assert!(!rendered.contains("provider.example"));
        assert!(!rendered.contains("api_key"));
        assert!(!rendered.contains("\"status\""));
        assert!(!rendered.contains("accepted"));
    }

    #[test]
    fn key_format_uses_fingerprint_without_raw_slug_or_idempotency_key() {
        let store = IdempotencyStore::disabled();
        let slug = "receiver-secret-slug";
        let idem_key = "tenant-alpha-secret-idempotency-key-token";
        let rendered = store.key(slug, idem_key);

        assert!(rendered.starts_with("idem:"));
        assert_eq!(rendered.len(), "idem:".len() + 64);
        assert_eq!(rendered, store.key(slug, idem_key));
        assert_ne!(rendered, store.key(slug, "different-key"));
        assert_ne!(rendered, store.key("different-slug", idem_key));
        assert!(!rendered.contains(slug));
        assert!(!rendered.contains(idem_key));
        assert!(!rendered.contains("receiver-secret"));
        assert!(!rendered.contains("idempotency-key"));
        assert!(!rendered.contains("tenant-alpha"));
    }

    #[test]
    fn idempotency_diagnostic_reason_uses_stable_codes() {
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
                "NOAUTH Authentication required for idem:tenant-secret:key-secret",
                "authorization_failed",
            ),
            (
                "json parser failed at line 1 column 7 with sk-idem-secret",
                "invalid_input",
            ),
            (
                "backend returned provider.example token=secret",
                "operation_failed",
            ),
        ];

        for (diagnostic, expected) in cases {
            assert_eq!(idempotency_diagnostic_reason(diagnostic), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("provider.example"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("key-secret"));
            assert!(!expected.contains("sk-idem-secret"));
        }
    }
}
