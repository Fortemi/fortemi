//! Redis-backed idempotency store for incoming webhook receivers (#822).
//!
//! Clients may send an `Idempotency-Key` header on
//! `POST /api/v1/webhooks/incoming/{slug}`. The first request with a given key
//! is processed normally and its accepted response is cached under
//! `idem:{slug}:{key}` for 24 hours, alongside a hash of the request body.
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
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Default replay window: 24 hours (roadmap §4.2 Idempotency).
const DEFAULT_TTL_SECONDS: u64 = 86_400;

/// Cached outcome of a processed inbound webhook, keyed by `Idempotency-Key`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    /// Hex SHA-256 of the raw request body, used to detect key reuse with a
    /// different payload.
    pub body_hash: String,
    /// HTTP status of the cached response (currently always the accepted 200).
    pub response_status: u16,
    /// The JSON response body to replay.
    pub response_body: serde_json::Value,
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
    /// Key prefix (`idem:`); full key is `idem:{slug}:{key}`.
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
        format!("{}{}:{}", self.inner.prefix, slug, idem_key)
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
                    warn!("Idempotency store: corrupt record at {key}: {e}");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!("Idempotency store: GET failed: {e}");
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
                warn!("Idempotency store: serialize failed: {e}");
                return;
            }
        };
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&key, payload, self.inner.ttl_seconds)
            .await
        {
            warn!("Idempotency store: SET failed: {e}");
        }
    }
}

/// Open a Redis connection manager with a bounded connect timeout, logging and
/// disabling (returning `None`) on any failure.
async fn connect(redis_url: &str, ttl_seconds: u64) -> Option<ConnectionManager> {
    let client = match redis::Client::open(redis_url) {
        Ok(c) => c,
        Err(e) => {
            warn!("Idempotency store: invalid Redis URL, dedup disabled: {e}");
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
            warn!("Idempotency store: Redis connect failed, dedup disabled: {e}");
            None
        }
        Err(_) => {
            warn!("Idempotency store: Redis connect timed out, dedup disabled");
            None
        }
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
    fn key_format_matches_spec() {
        let store = IdempotencyStore::disabled();
        assert_eq!(
            store.key("twilio-voice", "abc-123"),
            "idem:twilio-voice:abc-123"
        );
    }
}
