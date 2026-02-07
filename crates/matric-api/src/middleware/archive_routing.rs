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

    // Update cache with fetched archive
    let ctx = ArchiveContext {
        schema: archive_info.schema_name,
        is_default: archive_info.is_default,
    };

    cache.archive = Some(ctx.clone());
    cache.last_refresh = Utc::now();

    ctx
}

/// Archive routing middleware function.
///
/// Injects an ArchiveContext into request extensions based on the default
/// archive setting. Uses a TTL-based cache to minimize database queries.
///
/// Future enhancements (not in Issue #107):
/// - Extract archive selection from request headers or query params
/// - Support per-user archive defaults
/// - Archive-specific request routing
pub async fn archive_routing_middleware(
    State(state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Read cache
    let cache = state.default_archive_cache.read().await;

    let ctx = if let Some(ref cached) = cache.archive {
        if !cache.is_expired() {
            // Cache hit and not expired
            cached.clone()
        } else {
            // Cache expired - drop read lock and refresh
            drop(cache);
            refresh_and_get(&state).await
        }
    } else if cache.is_expired() {
        // No cache and expired - drop read lock and refresh
        drop(cache);
        refresh_and_get(&state).await
    } else {
        // No cache but not expired yet (initial state)
        ArchiveContext::default()
    };

    // Inject archive context into request extensions
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
}
