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
                Ok(client) => match ConnectionManager::new(client).await {
                    Ok(conn) => {
                        info!(
                            "Redis search cache enabled (TTL: {}s, URL: {})",
                            ttl_seconds,
                            redis_url.replace(|c: char| c.is_ascii_alphanumeric(), "*")
                        );
                        Some(conn)
                    }
                    Err(e) => {
                        warn!("Failed to connect to Redis, cache disabled: {}", e);
                        None
                    }
                },
                Err(e) => {
                    warn!("Invalid Redis URL, cache disabled: {}", e);
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
                    debug!("Cache HIT: {}", key);
                    Some(result)
                }
                Err(e) => {
                    warn!("Cache deserialization error: {}", e);
                    None
                }
            },
            Ok(None) => {
                debug!("Cache MISS: {}", key);
                None
            }
            Err(e) => {
                error!("Redis GET error: {}", e);
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
                error!("Cache serialization error: {}", e);
                return false;
            }
        };

        match conn
            .set_ex::<_, _, ()>(key, serialized, self.inner.ttl_seconds)
            .await
        {
            Ok(_) => {
                debug!("Cache SET: {} (TTL: {}s)", key, self.inner.ttl_seconds);
                true
            }
            Err(e) => {
                error!("Redis SET error: {}", e);
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
                debug!("Cache INVALIDATE: {}", key);
                true
            }
            Err(e) => {
                error!("Redis DEL error: {}", e);
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
                    info!("Cache FLUSH: removed {} keys", keys.len());
                    true
                }
                Err(e) => {
                    error!("Redis flush error: {}", e);
                    false
                }
            },
            Ok(_) => {
                debug!("Cache FLUSH: no keys to remove");
                true
            }
            Err(e) => {
                error!("Redis KEYS error: {}", e);
                false
            }
        }
    }

    /// Get cache TTL setting.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.inner.ttl_seconds)
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
}
