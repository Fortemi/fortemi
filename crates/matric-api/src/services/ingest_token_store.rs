//! Redis-backed stream-scoped bearer tokens for `POST /api/v1/ingest/stream` (#829).
//!
//! A stream token is a short-lived (default 1h) credential, minted via
//! `POST /api/v1/ingest/tokens` (behind normal auth) and presented as
//! `Authorization: Bearer <token>` on `/ingest/stream` when
//! `INGEST_REQUIRE_TOKEN=true` (the default for shared deployments). Each token
//! is bound at mint time to the caller's archive **schema** and carries a
//! per-token **rate limit** (lines/sec; 0 = unlimited). Tokens are ephemeral by
//! design — stored only in Redis with a rolling TTL, never persisted to the DB —
//! which matches the "single-use, stream-session" lifetime in the roadmap and
//! mirrors [`IngestCursorStore`](super::ingest_cursor_store).
//!
//! ## Storage layout
//!
//! Two keys per token so revocation can take a non-secret id rather than the
//! bearer secret:
//! - `mm:ingesttoken:{token}` → JSON [`IngestTokenData`] (the validation lookup)
//! - `mm:ingesttoken-id:{token_id}` → `{token}` (reverse index for revoke-by-id)
//!
//! Both share the same TTL; a refresh is not performed on validate (a stream
//! token's lifetime is fixed from mint, unlike the resumption cursor's rolling
//! window).
//!
//! ## Graceful degradation
//!
//! When Redis is unavailable the store is inert: `mint` returns `None` (the mint
//! endpoint reports `503`), `validate` returns `None`, `revoke` returns `false`.
//! The handler's fail-closed posture (see `ingest_stream`) then rejects token-
//! gated streams with `401` rather than silently allowing them.
//!
//! ## Configuration
//!
//! - `REDIS_ENABLED` (default: true) — shared with the search cache / cursor store
//! - `REDIS_URL` (default: redis://localhost:6379)
//! - `FORTEMI_INGEST_TOKEN_TTL` (default: 3600 seconds — 1h lifetime)
//! - `FORTEMI_INGEST_TOKEN_RATE_LIMIT` (default: 0 — unlimited lines/sec)

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Default token lifetime — the roadmap §3 Phase C5 decision (1h).
const DEFAULT_TTL_SECONDS: u64 = 3600;

/// Default per-token rate limit (lines/sec); 0 means unlimited.
const DEFAULT_RATE_LIMIT: u64 = 0;

/// The persisted record for a stream token (value under `mm:ingesttoken:{token}`).
/// Holds no secret beyond its own key; the `token_id` is a non-secret handle for
/// revocation.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IngestTokenData {
    /// Non-secret revocation handle.
    pub token_id: String,
    /// Archive schema the stream writes to (bound at mint time).
    pub schema: String,
    /// Per-token rate limit in lines/sec; 0 = unlimited.
    pub rate_limit: u64,
}

impl std::fmt::Debug for IngestTokenData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestTokenData")
            .field("token_id_len", &ingest_token_text_len(&self.token_id))
            .field("schema_len", &ingest_token_text_len(&self.schema))
            .field("rate_limit", &self.rate_limit)
            .finish()
    }
}

/// The result of [`IngestTokenStore::mint`]. The secret `token` is returned to
/// the caller exactly once (it is never recoverable afterward — only revocable
/// by `token_id`).
pub struct MintedIngestToken {
    pub token: String,
    pub token_id: String,
    pub rate_limit: u64,
    pub ttl_seconds: u64,
}

/// Redis-backed store of short-lived, archive-bound ingest stream tokens.
#[derive(Clone)]
pub struct IngestTokenStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// Redis connection manager (None when disabled or unreachable).
    connection: RwLock<Option<ConnectionManager>>,
    /// Token lifetime in seconds.
    ttl_seconds: u64,
    /// Default per-token rate limit (lines/sec) when a mint omits one.
    default_rate_limit: u64,
    /// Key prefix for the primary token → data entries.
    prefix: String,
    /// Key prefix for the reverse token_id → token index.
    id_prefix: String,
}

impl IngestTokenStore {
    /// Construct from environment, sharing `REDIS_ENABLED` / `REDIS_URL` with the
    /// search cache and cursor store. Never blocks startup longer than the 5s
    /// connect timeout; on any failure the store is disabled.
    pub async fn from_env() -> Self {
        let enabled = std::env::var("REDIS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let ttl_seconds: u64 = std::env::var("FORTEMI_INGEST_TOKEN_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_TTL_SECONDS);

        let default_rate_limit: u64 = std::env::var("FORTEMI_INGEST_TOKEN_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RATE_LIMIT);

        let connection = if enabled {
            connect(&redis_url, ttl_seconds).await
        } else {
            None
        };

        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(connection),
                ttl_seconds,
                default_rate_limit,
                prefix: "mm:ingesttoken:".to_string(),
                id_prefix: "mm:ingesttoken-id:".to_string(),
            }),
        }
    }

    /// A disabled store (no Redis) — for tests or single-user deployments.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(Inner {
                connection: RwLock::new(None),
                ttl_seconds: DEFAULT_TTL_SECONDS,
                default_rate_limit: DEFAULT_RATE_LIMIT,
                prefix: "mm:ingesttoken:".to_string(),
                id_prefix: "mm:ingesttoken-id:".to_string(),
            }),
        }
    }

    /// Whether the store has a live Redis connection (token minting available).
    pub async fn is_connected(&self) -> bool {
        self.inner.connection.read().await.is_some()
    }

    /// The configured token lifetime in seconds.
    pub fn ttl_seconds(&self) -> u64 {
        self.inner.ttl_seconds
    }

    /// The default per-token rate limit (lines/sec) applied when a mint omits one.
    pub fn default_rate_limit(&self) -> u64 {
        self.inner.default_rate_limit
    }

    fn key(&self, token: &str) -> String {
        format!("{}{}", self.inner.prefix, token)
    }

    fn id_key(&self, token_id: &str) -> String {
        format!("{}{}", self.inner.id_prefix, token_id)
    }

    /// Mint a new archive-bound stream token with the given rate limit
    /// (lines/sec; 0 = unlimited). Returns `None` when Redis is unavailable (the
    /// caller surfaces `503`). The secret token is returned exactly once.
    pub async fn mint(&self, schema: &str, rate_limit: u64) -> Option<MintedIngestToken> {
        let mut guard = self.inner.connection.write().await;
        let conn = guard.as_mut()?;

        let token = format!("mm_ist_{}", Uuid::new_v4().simple());
        let token_id = Uuid::new_v4().to_string();
        let data = IngestTokenData {
            token_id: token_id.clone(),
            schema: schema.to_string(),
            rate_limit,
        };
        let json = serde_json::to_string(&data).ok()?;

        if let Err(e) = conn
            .set_ex::<_, _, ()>(&self.key(&token), &json, self.inner.ttl_seconds)
            .await
        {
            let diagnostic = e.to_string();
            warn!(
                operation = "set_token",
                reason_code = ingest_token_diagnostic_reason(&diagnostic),
                error_len = ingest_token_text_len(&diagnostic),
                "Ingest token store primary write failed"
            );
            return None;
        }
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&self.id_key(&token_id), &token, self.inner.ttl_seconds)
            .await
        {
            let diagnostic = e.to_string();
            warn!(
                operation = "set_reverse_index",
                reason_code = ingest_token_diagnostic_reason(&diagnostic),
                error_len = ingest_token_text_len(&diagnostic),
                "Ingest token store reverse-index write failed"
            );
            // Best-effort rollback so a half-minted token cannot validate.
            let _ = conn.del::<_, u64>(&self.key(&token)).await;
            return None;
        }

        Some(MintedIngestToken {
            token,
            token_id,
            rate_limit,
            ttl_seconds: self.inner.ttl_seconds,
        })
    }

    /// Validate a presented bearer token, returning its bound data or `None` when
    /// the token is unknown, expired, or Redis is unavailable.
    pub async fn validate(&self, token: &str) -> Option<IngestTokenData> {
        let mut guard = self.inner.connection.write().await;
        let conn = guard.as_mut()?;
        let raw: Option<String> = match conn.get(self.key(token)).await {
            Ok(v) => v,
            Err(e) => {
                let diagnostic = e.to_string();
                warn!(
                    operation = "get",
                    reason_code = ingest_token_diagnostic_reason(&diagnostic),
                    error_len = ingest_token_text_len(&diagnostic),
                    "Ingest token store lookup failed"
                );
                None
            }
        };
        serde_json::from_str(&raw?).ok()
    }

    /// Revoke a token by its non-secret `token_id`, deleting both the primary and
    /// reverse-index keys. Returns `true` when something was deleted, `false`
    /// when the id was unknown/expired or Redis is unavailable.
    pub async fn revoke(&self, token_id: &str) -> bool {
        let mut guard = self.inner.connection.write().await;
        let Some(conn) = guard.as_mut() else {
            return false;
        };
        let id_key = self.id_key(token_id);
        let token: Option<String> = conn.get(&id_key).await.unwrap_or(None);
        let mut deleted: u64 = 0;
        if let Some(tok) = token {
            deleted += conn.del::<_, u64>(&self.key(&tok)).await.unwrap_or(0);
        }
        deleted += conn.del::<_, u64>(&id_key).await.unwrap_or(0);
        deleted > 0
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
                reason_code = ingest_token_diagnostic_reason(&diagnostic),
                error_len = ingest_token_text_len(&diagnostic),
                "Ingest token store Redis URL rejected; token auth disabled"
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
            info!("Ingest token store enabled (TTL: {ttl_seconds}s)");
            Some(conn)
        }
        Ok(Err(e)) => {
            let diagnostic = e.to_string();
            warn!(
                operation = "connect",
                reason_code = ingest_token_diagnostic_reason(&diagnostic),
                error_len = ingest_token_text_len(&diagnostic),
                "Ingest token store Redis connect failed; token auth disabled"
            );
            None
        }
        Err(_) => {
            warn!("Ingest token store: Redis connect timed out, token auth disabled");
            None
        }
    }
}

fn ingest_token_text_len(value: &str) -> usize {
    value.chars().count()
}

fn ingest_token_diagnostic_reason(value: &str) -> &'static str {
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
    async fn disabled_store_is_inert() {
        let store = IngestTokenStore::disabled();
        assert!(!store.is_connected().await);
        assert!(
            store.mint("public", 10).await.is_none(),
            "mint must fail closed without Redis"
        );
        assert!(store.validate("mm_ist_anything").await.is_none());
        assert!(!store.revoke("some-id").await);
        assert_eq!(store.ttl_seconds(), DEFAULT_TTL_SECONDS);
        assert_eq!(store.default_rate_limit(), DEFAULT_RATE_LIMIT);
    }

    #[test]
    fn token_data_round_trips_through_json() {
        let data = IngestTokenData {
            token_id: "id-1".to_string(),
            schema: "archive_x".to_string(),
            rate_limit: 250,
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: IngestTokenData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, back);
    }

    #[test]
    fn token_data_debug_redacts_identifiers_and_schema() {
        let data = IngestTokenData {
            token_id: "token-id-secret-tenant-alpha".to_string(),
            schema: "private_schema_with_token_secret".to_string(),
            rate_limit: 250,
        };

        let rendered = format!("{data:?}");

        assert!(rendered.contains("IngestTokenData"));
        assert!(rendered.contains("token_id_len"));
        assert!(rendered.contains("schema_len"));
        assert!(rendered.contains("rate_limit"));
        assert!(!rendered.contains("token-id-secret"));
        assert!(!rendered.contains("tenant-alpha"));
        assert!(!rendered.contains("private_schema"));
        assert!(!rendered.contains("token_secret"));
    }

    #[test]
    fn ingest_token_diagnostic_reason_uses_stable_codes() {
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
                "NOAUTH Authentication required for mm:ingesttoken:mm_ist_secret",
                "authorization_failed",
            ),
            (
                "json parser failed at line 1 column 7 with sk-token-secret",
                "invalid_input",
            ),
            (
                "backend returned provider.example token=secret",
                "operation_failed",
            ),
        ];

        for (diagnostic, expected) in cases {
            assert_eq!(ingest_token_diagnostic_reason(diagnostic), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("provider.example"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("mm_ist_secret"));
            assert!(!expected.contains("sk-token-secret"));
        }
    }
}
