//! Database connection pool management.

use std::time::{Duration, Instant};

use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{debug, info, warn};

use matric_core::{Error, Result};

/// Default maximum number of connections in the pool.
pub const DEFAULT_MAX_CONNECTIONS: u32 = 10;

/// Default connection timeout in seconds.
pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 30;

/// Default idle timeout in seconds.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

/// Pool configuration options.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of connections to maintain.
    pub min_connections: u32,
    /// Connection timeout duration.
    pub connect_timeout: Duration,
    /// Idle connection timeout duration.
    pub idle_timeout: Duration,
    /// Maximum connection lifetime.
    pub max_lifetime: Option<Duration>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
            min_connections: 1,
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
            idle_timeout: Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
            max_lifetime: Some(Duration::from_secs(1800)), // 30 minutes
        }
    }
}

impl PoolConfig {
    /// Create a new pool configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of connections.
    pub fn max_connections(mut self, n: u32) -> Self {
        self.max_connections = n;
        self
    }

    /// Set the minimum number of connections.
    pub fn min_connections(mut self, n: u32) -> Self {
        self.min_connections = n;
        self
    }

    /// Set the connection timeout.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the idle connection timeout.
    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the maximum connection lifetime.
    pub fn max_lifetime(mut self, lifetime: Option<Duration>) -> Self {
        self.max_lifetime = lifetime;
        self
    }
}

/// Create a new PostgreSQL connection pool with default configuration.
pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    create_pool_with_config(database_url, PoolConfig::default()).await
}

/// Create a new PostgreSQL connection pool with custom configuration.
pub async fn create_pool_with_config(database_url: &str, config: PoolConfig) -> Result<PgPool> {
    let start = Instant::now();

    info!(
        subsystem = "database",
        component = "pool",
        op = "create",
        max_connections = config.max_connections,
        min_connections = config.min_connections,
        connect_timeout_secs = config.connect_timeout.as_secs(),
        idle_timeout_secs = config.idle_timeout.as_secs(),
        "Creating database connection pool"
    );

    let mut options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout);

    if let Some(max_lifetime) = config.max_lifetime {
        options = options.max_lifetime(max_lifetime);
    }

    let pool = options
        .connect(database_url)
        .await
        .map_err(Error::Database)?;

    info!(
        subsystem = "database",
        component = "pool",
        op = "established",
        pool_size = pool.size(),
        pool_idle = pool.num_idle(),
        duration_ms = start.elapsed().as_millis() as u64,
        "Database connection pool established"
    );
    Ok(pool)
}

/// Log current pool health metrics.
///
/// Emits structured debug-level log with pool size, idle count,
/// and warns if idle connections drop below 1 (potential exhaustion).
pub fn log_pool_metrics(pool: &PgPool) {
    let size = pool.size();
    let idle = pool.num_idle();

    debug!(
        subsystem = "database",
        component = "pool",
        op = "metrics",
        pool_size = size,
        pool_idle = idle,
        "Pool health check"
    );

    if idle == 0 && size > 0 {
        warn!(
            subsystem = "database",
            component = "pool",
            pool_size = size,
            "Connection pool has no idle connections — potential exhaustion"
        );
    }
}

/// Create a new PostgreSQL connection pool with custom options (legacy API).
pub async fn create_pool_with_options(database_url: &str, max_connections: u32) -> Result<PgPool> {
    create_pool_with_config(
        database_url,
        PoolConfig::default().max_connections(max_connections),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_max_connections() {
        assert_eq!(DEFAULT_MAX_CONNECTIONS, 10);
    }

    #[test]
    fn test_pool_config_builder() {
        let config = PoolConfig::new()
            .max_connections(20)
            .min_connections(5)
            .connect_timeout(Duration::from_secs(60));

        assert_eq!(config.max_connections, 20);
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.connect_timeout, Duration::from_secs(60));
    }
}
