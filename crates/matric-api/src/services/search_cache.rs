//! Redis-based search query cache for matric-memory.
//!
//! Provides caching for hybrid search results to reduce latency
//! and compute load for repeated/similar queries.
//!
//! ## Configuration
//!
//! Environment variables:
//! - `REDIS_ENABLED`: Set to "false" to disable caching (default: true)
//! - `REDIS_URL`: Redis connection URL (default: redis://localhost:6379)
//! - `REDIS_CACHE_TTL`: Cache TTL in seconds (default: 300)

use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Search cache backed by Redis.
#[derive(Clone)]
pub struct SearchCache {
    inner: Arc<SearchCacheInner>,
}

struct SearchCacheInner {
    /// Redis connection manager (None if disabled).
    connection: RwLock<Option<ConnectionManager>>,
    /// Cache TTL in seconds.
    ttl_seconds: u64,
    /// Whether caching is enabled.
    enabled: bool,
    /// Cache key prefix.
    prefix: String,
}

/// Cache statistics for monitoring.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
}

impl SearchCache {
    /// Create a new search cache from environment configuration.
    ///
    /// Reads:
    /// - `REDIS_ENABLED` (default: true)
    /// - `REDIS_URL` (default: redis://localhost:6379)
    /// - `REDIS_CACHE_TTL` (default: 300 seconds)
    pub async fn from_env() -> Self {
        let enabled = std::env::var("REDIS_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let ttl_seconds: u64 = std::env::var("REDIS_CACHE_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let connection = if enabled {
            match redis::Client::open(redis_url.as_str()) {
                Ok(client) => {
                    // Timeout the connection attempt — without Redis the default
                    // connect blocks for minutes, stalling the entire server startup.
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        ConnectionManager::new(client),
                    )
                    .await
                    {
                        Ok(Ok(conn)) => {
                            info!(
                                ttl_seconds,
                                redis_url_class = search_cache_url_class(&redis_url),
                                redis_url_len = search_cache_text_len(&redis_url),
                                "Redis search cache enabled"
                            );
                            Some(conn)
                        }
                        Ok(Err(e)) => {
                            let diagnostic = e.to_string();
                            warn!(
                                reason_code = search_cache_diagnostic_reason(&diagnostic),
                                error_len = search_cache_text_len(&diagnostic),
                                "Redis search cache connect failed; cache disabled"
                            );
                            None
                        }
                        Err(_) => {
                            warn!(
                                redis_url_class = search_cache_url_class(&redis_url),
                                redis_url_len = search_cache_text_len(&redis_url),
                                "Redis search cache connect timed out; cache disabled"
                            );
                            None
                        }
                    }
                }
                Err(e) => {
                    let diagnostic = e.to_string();
                    warn!(
                        reason_code = search_cache_diagnostic_reason(&diagnostic),
                        error_len = search_cache_text_len(&diagnostic),
                        "Redis search cache URL rejected; cache disabled"
                    );
                    None
                }
            }
        } else {
            info!("Redis search cache disabled via REDIS_ENABLED=false");
            None
        };

        Self {
            inner: Arc::new(SearchCacheInner {
                connection: RwLock::new(connection),
                ttl_seconds,
                enabled,
                prefix: "mm:search:".to_string(),
            }),
        }
    }

    /// Create a disabled cache (for testing or when Redis unavailable).
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(SearchCacheInner {
                connection: RwLock::new(None),
                ttl_seconds: 300,
                enabled: false,
                prefix: "mm:search:".to_string(),
            }),
        }
    }

    /// Check if caching is enabled and connected.
    pub async fn is_connected(&self) -> bool {
        self.inner.enabled && self.inner.connection.read().await.is_some()
    }

    /// Generate a cache key from the search query parameters.
    pub fn cache_key(
        &self,
        query: &str,
        tags: Option<&[String]>,
        collection_id: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();

        // Normalize query (lowercase, trim whitespace)
        hasher.update(query.to_lowercase().trim().as_bytes());

        // Include tags in hash (sorted for consistency)
        if let Some(tags) = tags {
            let mut sorted_tags: Vec<_> = tags.iter().collect();
            sorted_tags.sort();
            for tag in sorted_tags {
                hasher.update(tag.as_bytes());
            }
        }

        // Include collection filter
        if let Some(cid) = collection_id {
            hasher.update(cid.as_bytes());
        }

        let hash = hex::encode(hasher.finalize());
        format!("{}{}", self.inner.prefix, &hash[..16]) // Use first 16 chars of hash
    }

    /// Get cached search results.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let mut conn_guard = self.inner.connection.write().await;
        let conn = conn_guard.as_mut()?;

        match conn.get::<_, Option<String>>(key).await {
            Ok(Some(data)) => match serde_json::from_str(&data) {
                Ok(result) => {
                    debug!(key_len = search_cache_text_len(key), "Search cache hit");
                    Some(result)
                }
                Err(e) => {
                    let diagnostic = e.to_string();
                    warn!(
                        key_len = search_cache_text_len(key),
                        reason_code = search_cache_diagnostic_reason(&diagnostic),
                        error_len = search_cache_text_len(&diagnostic),
                        "Search cache deserialization failed"
                    );
                    None
                }
            },
            Ok(None) => {
                debug!(key_len = search_cache_text_len(key), "Search cache miss");
                None
            }
            Err(e) => {
                let diagnostic = e.to_string();
                error!(
                    key_len = search_cache_text_len(key),
                    reason_code = search_cache_diagnostic_reason(&diagnostic),
                    error_len = search_cache_text_len(&diagnostic),
                    "Search cache Redis GET failed"
                );
                None
            }
        }
    }

    /// Store search results in cache.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> bool {
        let mut conn_guard = self.inner.connection.write().await;
        let conn = match conn_guard.as_mut() {
            Some(c) => c,
            None => return false,
        };

        let serialized = match serde_json::to_string(value) {
            Ok(s) => s,
            Err(e) => {
                let diagnostic = e.to_string();
                error!(
                    key_len = search_cache_text_len(key),
                    reason_code = search_cache_diagnostic_reason(&diagnostic),
                    error_len = search_cache_text_len(&diagnostic),
                    "Search cache serialization failed"
                );
                return false;
            }
        };

        match conn
            .set_ex::<_, _, ()>(key, serialized, self.inner.ttl_seconds)
            .await
        {
            Ok(_) => {
                debug!(
                    key_len = search_cache_text_len(key),
                    ttl_seconds = self.inner.ttl_seconds,
                    "Search cache set"
                );
                true
            }
            Err(e) => {
                let diagnostic = e.to_string();
                error!(
                    key_len = search_cache_text_len(key),
                    reason_code = search_cache_diagnostic_reason(&diagnostic),
                    error_len = search_cache_text_len(&diagnostic),
                    "Search cache Redis SET failed"
                );
                false
            }
        }
    }

    /// Invalidate a specific cache key.
    pub async fn invalidate(&self, key: &str) -> bool {
        let mut conn_guard = self.inner.connection.write().await;
        let conn = match conn_guard.as_mut() {
            Some(c) => c,
            None => return false,
        };

        match conn.del::<_, ()>(key).await {
            Ok(_) => {
                debug!(
                    key_len = search_cache_text_len(key),
                    "Search cache invalidate"
                );
                true
            }
            Err(e) => {
                let diagnostic = e.to_string();
                error!(
                    key_len = search_cache_text_len(key),
                    reason_code = search_cache_diagnostic_reason(&diagnostic),
                    error_len = search_cache_text_len(&diagnostic),
                    "Search cache Redis DEL failed"
                );
                false
            }
        }
    }

    /// Invalidate all search cache entries (flush with prefix).
    pub async fn invalidate_all(&self) -> bool {
        let mut conn_guard = self.inner.connection.write().await;
        let conn = match conn_guard.as_mut() {
            Some(c) => c,
            None => return false,
        };

        let pattern = format!("{}*", self.inner.prefix);

        // Use SCAN to find keys, then DEL
        // Note: For production with many keys, consider UNLINK for async deletion
        match redis::cmd("KEYS")
            .arg(&pattern)
            .query_async::<Vec<String>>(conn)
            .await
        {
            Ok(keys) if !keys.is_empty() => match conn.del::<_, ()>(&keys[..]).await {
                Ok(_) => {
                    info!(
                        removed_count = keys.len(),
                        "Search cache flush removed keys"
                    );
                    true
                }
                Err(e) => {
                    let diagnostic = e.to_string();
                    error!(
                        key_count = keys.len(),
                        reason_code = search_cache_diagnostic_reason(&diagnostic),
                        error_len = search_cache_text_len(&diagnostic),
                        "Search cache Redis flush failed"
                    );
                    false
                }
            },
            Ok(_) => {
                debug!("Search cache flush found no keys");
                true
            }
            Err(e) => {
                let diagnostic = e.to_string();
                error!(
                    reason_code = search_cache_diagnostic_reason(&diagnostic),
                    error_len = search_cache_text_len(&diagnostic),
                    "Search cache Redis KEYS failed"
                );
                false
            }
        }
    }

    /// Get cache TTL setting.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.inner.ttl_seconds)
    }
}

fn search_cache_text_len(value: &str) -> usize {
    value.chars().count()
}

fn search_cache_url_class(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("redis://localhost")
        || lower.starts_with("rediss://localhost")
        || lower.contains("@localhost")
        || lower.contains("://127.")
        || lower.contains("@127.")
        || lower.contains("://10.")
        || lower.contains("@10.")
        || lower.contains("://192.168.")
        || lower.contains("@192.168.")
        || lower.contains("://172.16.")
        || lower.contains("@172.16.")
        || lower.contains("://172.17.")
        || lower.contains("@172.17.")
        || lower.contains("://172.18.")
        || lower.contains("@172.18.")
        || lower.contains("://172.19.")
        || lower.contains("@172.19.")
        || lower.contains("://172.20.")
        || lower.contains("@172.20.")
        || lower.contains("://172.21.")
        || lower.contains("@172.21.")
        || lower.contains("://172.22.")
        || lower.contains("@172.22.")
        || lower.contains("://172.23.")
        || lower.contains("@172.23.")
        || lower.contains("://172.24.")
        || lower.contains("@172.24.")
        || lower.contains("://172.25.")
        || lower.contains("@172.25.")
        || lower.contains("://172.26.")
        || lower.contains("@172.26.")
        || lower.contains("://172.27.")
        || lower.contains("@172.27.")
        || lower.contains("://172.28.")
        || lower.contains("@172.28.")
        || lower.contains("://172.29.")
        || lower.contains("@172.29.")
        || lower.contains("://172.30.")
        || lower.contains("@172.30.")
        || lower.contains("://172.31.")
        || lower.contains("@172.31.")
        || lower.contains(".internal")
    {
        "local_or_private"
    } else if lower.starts_with("redis://") || lower.starts_with("rediss://") {
        "redis"
    } else {
        "invalid_url"
    }
}

fn search_cache_diagnostic_reason(value: &str) -> &'static str {
    let value = value.to_ascii_lowercase();
    if value.contains("timeout") || value.contains("timed out") {
        "timeout"
    } else if value.contains("invalid") || value.contains("url") {
        "invalid_config"
    } else if value.contains("connect") || value.contains("connection") {
        "connection_failed"
    } else if value.contains("noauth")
        || value.contains("auth")
        || value.contains("permission")
        || value.contains("denied")
    {
        "auth_failed"
    } else if value.contains("json") || value.contains("parse") || value.contains("serde") {
        "serialization_failed"
    } else {
        "operation_failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let cache = SearchCache::disabled();

        // Same query should produce same key
        let key1 = cache.cache_key("hello world", None, None);
        let key2 = cache.cache_key("hello world", None, None);
        assert_eq!(key1, key2);

        // Different queries should produce different keys
        let key3 = cache.cache_key("different query", None, None);
        assert_ne!(key1, key3);

        // Case insensitive
        let key4 = cache.cache_key("HELLO WORLD", None, None);
        assert_eq!(key1, key4);

        // Tags affect key
        let key5 = cache.cache_key("hello world", Some(&["tag1".to_string()]), None);
        assert_ne!(key1, key5);

        // Collection affects key
        let key6 = cache.cache_key("hello world", None, Some("collection-id"));
        assert_ne!(key1, key6);
    }

    #[test]
    fn test_cache_key_prefix() {
        let cache = SearchCache::disabled();
        let key = cache.cache_key("test", None, None);
        assert!(key.starts_with("mm:search:"));
    }

    #[test]
    fn search_cache_url_class_uses_stable_classes() {
        let cases = [
            (
                "redis://user:pass@localhost:6379/0?token=secret",
                "local_or_private",
            ),
            (
                "rediss://user:pass@10.0.0.8:6379/0?api_key=secret",
                "local_or_private",
            ),
            (
                "redis://user:pass@cache.internal:6379/0?token=secret",
                "local_or_private",
            ),
            (
                "rediss://user:pass@redis.example.com:6379/0?token=secret",
                "redis",
            ),
            ("not a redis url with token=secret", "invalid_url"),
        ];

        for (url, expected) in cases {
            assert_eq!(search_cache_url_class(url), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("rediss://"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("redis.example.com"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("api_key=secret"));
        }
    }

    #[test]
    fn search_cache_diagnostic_reason_uses_stable_codes() {
        let cases = [
            (
                "invalid redis url redis://user:pass@cache.internal:6379/0?token=secret",
                "invalid_config",
            ),
            (
                "connection refused at redis://cache.internal:6379 with token=secret",
                "connection_failed",
            ),
            (
                "NOAUTH Authentication required for mm:search:key-secret",
                "auth_failed",
            ),
            (
                "json parser failed at line 1 column 7 with sk-search-secret",
                "serialization_failed",
            ),
            (
                "backend returned provider.example token=secret",
                "operation_failed",
            ),
        ];

        for (diagnostic, expected) in cases {
            assert_eq!(search_cache_diagnostic_reason(diagnostic), expected);
            assert!(!expected.contains("redis://"));
            assert!(!expected.contains("user:pass"));
            assert!(!expected.contains("cache.internal"));
            assert!(!expected.contains("provider.example"));
            assert!(!expected.contains("token=secret"));
            assert!(!expected.contains("key-secret"));
            assert!(!expected.contains("sk-search-secret"));
        }
    }

    #[test]
    fn search_cache_text_len_counts_without_exposing_content() {
        let key = "mm:search:query-derived-secret-key";
        assert_eq!(search_cache_text_len(key), key.chars().count());
    }
}
