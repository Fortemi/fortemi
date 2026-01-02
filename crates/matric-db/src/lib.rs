//! # matric-db
//!
//! PostgreSQL + pgvector database layer for matric-memory.
//!
//! This crate provides:
//! - Connection pool management
//! - CRUD operations for notes, revisions, embeddings
//! - Schema migrations (optional feature)

use sqlx::postgres::PgPool;

pub mod pool;

// Re-export core types
pub use matric_core::*;
pub use pool::*;

/// Database state container.
#[derive(Clone)]
pub struct DbState {
    pool: PgPool,
}

impl DbState {
    /// Create a new database state from a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Connect to the database with the given URL.
pub async fn connect(database_url: &str) -> Result<DbState> {
    let pool = pool::create_pool(database_url).await?;
    Ok(DbState::new(pool))
}
