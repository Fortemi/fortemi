//! Archive routing middleware for Gitea Issue #107.
//!
//! Provides default archive schema routing with TTL-based caching to minimize
//! database queries while supporting dynamic archive selection.

use axum::extract::State;
use chrono::{DateTime, Utc};

use crate::AppState;
use matric_core::ArchiveRepository;

/// Archive context injected into request extensions.
///
/// Contains the schema name to use for the current request and whether it's
/// the default archive. Handlers can access this via request.extensions().
#[derive(Clone, Debug)]
pub struct ArchiveContext {
    /// PostgreSQL schema name to use for database operations.
    pub schema: String,
    /// Whether this is the default archive (vs explicitly selected).
    /// Used by handlers to determine routing behavior.
    #[allow(dead_code)]
    pub is_default: bool,
}

impl Default for ArchiveContext {
    fn default() -> Self {
        Self {
            schema: "public".to_string(),
            is_default: false,
        }
    }
}

/// Cached default archive with TTL expiration.
///
/// Stores the default archive context and refreshes from the database only
/// when the cache expires. This reduces database load for frequently accessed
/// default archive information.
pub struct DefaultArchiveCache {
    /// Cached archive context (None if no default is set).
    pub archive: Option<ArchiveContext>,
    /// Timestamp of last cache refresh.
    pub last_refresh: DateTime<Utc>,
    /// TTL in seconds (cache expires after this duration).
    pub ttl_seconds: i64,
}

impl DefaultArchiveCache {
    /// Create a new cache with the specified TTL in seconds.
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            archive: None,
            last_refresh: DateTime::<Utc>::UNIX_EPOCH,
            ttl_seconds,
        }
    }

    /// Check if the cache has expired based on TTL.
    pub fn is_expired(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.last_refresh)
            .num_seconds()
            > self.ttl_seconds
    }

    /// Invalidate the cache by resetting to UNIX epoch.
    ///
    /// Called when the default archive is changed to force a refresh on the
    /// next request.
    pub fn invalidate(&mut self) {
        self.archive = None;
        self.last_refresh = DateTime::<Utc>::UNIX_EPOCH;
    }
}

/// Refresh the default archive cache from the database.
///
/// Queries the archive repository for the current default archive and updates
/// the cache with the result.
async fn refresh_and_get(state: &AppState) -> ArchiveContext {
    let mut cache = state.default_archive_cache.write().await;

    // Fetch default archive from database
    let archive_info = match state.db.archives.get_default_archive().await {
        Ok(Some(info)) => info,
        Ok(None) | Err(_) => {
            // No default archive or error - use public schema
            let ctx = ArchiveContext::default();
            cache.archive = Some(ctx.clone());
            cache.last_refresh = Utc::now();
            return ctx;
        }
    };

    // Auto-migrate if schema is outdated
    if let Err(e) = state
        .db
        .archives
        .sync_archive_schema(&archive_info.name)
        .await
    {
        tracing::warn!(
            "Failed to sync default archive '{}': {}",
            archive_info.name,
            e
        );
    }

    // Update cache with fetched archive
    let ctx = ArchiveContext {
        schema: archive_info.schema_name,
        is_default: archive_info.is_default,
    };

    cache.archive = Some(ctx.clone());
    cache.last_refresh = Utc::now();

    ctx
}

/// Resolve the archive context from cache or database.
///
/// This is intentionally a separate function so the RwLockReadGuard is
/// guaranteed to be dropped before the caller proceeds. In async Rust,
/// holding an RwLock read guard across an `.await` in the same function
/// can cause deadlocks when downstream handlers need a write lock on the
/// same RwLock (the async generator may keep the guard alive in its state
/// even after NLL considers it dead).
async fn resolve_archive_context(state: &AppState) -> ArchiveContext {
    {
        let cache = state.default_archive_cache.read().await;
        if let Some(ref cached) = cache.archive {
            if !cache.is_expired() {
                return cached.clone();
            }
        } else if !cache.is_expired() {
            return ArchiveContext::default();
        }
        // Read lock dropped here at end of block
    }

    // Cache expired or missing — refresh with write lock
    refresh_and_get(state).await
}

/// Header name for per-request memory selection.
///
/// Clients can send `X-Fortemi-Memory: <name>` to route the request to a
/// specific memory (archive schema). If absent, the default memory is used.
pub const MEMORY_HEADER: &str = "x-fortemi-memory";

/// Archive routing middleware function.
///
/// Injects an ArchiveContext into request extensions based on:
/// 1. `X-Fortemi-Memory` header (explicit per-request selection)
/// 2. Default archive setting (cached, TTL-based)
/// 3. Fallback to public schema
///
/// If the header specifies a memory that doesn't exist, returns 404.
pub async fn archive_routing_middleware(
    State(state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Check for explicit memory selection via header
    if let Some(memory_name) = req.headers().get(MEMORY_HEADER) {
        let name = match memory_name.to_str() {
            Ok(n) => n.to_string(),
            Err(_) => {
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::BAD_REQUEST)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"error":"Invalid X-Fortemi-Memory header value"}"#,
                    ))
                    .unwrap();
            }
        };

        // Look up the requested memory
        match state.db.archives.get_archive_by_name(&name).await {
            Ok(Some(info)) => {
                // Auto-migrate if schema is outdated (non-blocking best-effort)
                if let Err(e) = state.db.archives.sync_archive_schema(&name).await {
                    tracing::warn!("Failed to sync archive schema '{}': {}", name, e);
                }
                let ctx = ArchiveContext {
                    schema: info.schema_name,
                    is_default: false,
                };
                req.extensions_mut().insert(ctx);
                return next.run(req).await;
            }
            Ok(None) => {
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::NOT_FOUND)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"error":"Memory not found: {}"}}"#,
                        name
                    )))
                    .unwrap();
            }
            Err(_) => {
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"error":"Failed to look up memory"}"#,
                    ))
                    .unwrap();
            }
        }
    }

    // No explicit selection — use default archive (cached)
    let ctx = resolve_archive_context(&state).await;
    req.extensions_mut().insert(ctx);

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_archive_context_default() {
        let ctx = ArchiveContext::default();
        assert_eq!(ctx.schema, "public");
        assert!(!ctx.is_default);
    }

    #[test]
    fn test_default_archive_cache_new() {
        let cache = DefaultArchiveCache::new(300);
        assert!(cache.archive.is_none());
        assert_eq!(cache.ttl_seconds, 300);
        assert!(cache.is_expired()); // UNIX_EPOCH is always expired
    }

    #[test]
    fn test_default_archive_cache_expiration() {
        let mut cache = DefaultArchiveCache::new(60);
        cache.last_refresh = Utc::now();
        assert!(!cache.is_expired()); // Just refreshed, not expired

        // Simulate old cache (61 seconds ago)
        cache.last_refresh = Utc::now() - chrono::Duration::seconds(61);
        assert!(cache.is_expired());
    }

    #[test]
    fn test_default_archive_cache_invalidate() {
        let mut cache = DefaultArchiveCache::new(300);
        cache.archive = Some(ArchiveContext {
            schema: "archive_test".to_string(),
            is_default: true,
        });
        cache.last_refresh = Utc::now();

        cache.invalidate();

        assert!(cache.archive.is_none());
        assert_eq!(cache.last_refresh, DateTime::<Utc>::UNIX_EPOCH);
        assert!(cache.is_expired());
    }

    #[test]
    fn test_memory_header_constant() {
        assert_eq!(MEMORY_HEADER, "x-fortemi-memory");
    }
}
