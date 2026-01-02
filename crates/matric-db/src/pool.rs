//! Database connection pool management.

use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::info;

use matric_core::{Error, Result};

/// Default maximum number of connections in the pool.
pub const DEFAULT_MAX_CONNECTIONS: u32 = 10;

/// Create a new PostgreSQL connection pool.
pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    create_pool_with_options(database_url, DEFAULT_MAX_CONNECTIONS).await
}

/// Create a new PostgreSQL connection pool with custom options.
pub async fn create_pool_with_options(database_url: &str, max_connections: u32) -> Result<PgPool> {
    info!(
        "Connecting to database with max {} connections",
        max_connections
    );

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

    info!("Database connection pool established");
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_max_connections() {
        assert_eq!(DEFAULT_MAX_CONNECTIONS, 10);
    }
}
