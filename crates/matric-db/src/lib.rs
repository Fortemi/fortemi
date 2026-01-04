//! # matric-db
//!
//! PostgreSQL database layer for matric-memory.
//!
//! This crate provides:
//! - Connection pool management
//! - Repository implementations for all core entities
//! - Full-text search with PostgreSQL tsvector
//! - Vector search with pgvector
//!
//! ## Example
//!
//! ```rust,ignore
//! use matric_db::{Database, NoteRepository, CreateNoteRequest};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = Database::connect("postgres://localhost/matric").await?;
//!
//!     let note_id = db.notes.insert(CreateNoteRequest {
//!         content: "Hello, world!".to_string(),
//!         format: "markdown".to_string(),
//!         source: "user".to_string(),
//!         collection_id: None,
//!         tags: Some(vec!["greeting".to_string()]),
//!     }).await?;
//!
//!     println!("Created note: {}", note_id);
//!     Ok(())
//! }
//! ```

pub mod embeddings;
pub mod jobs;
pub mod links;
pub mod notes;
pub mod oauth;
pub mod pool;
pub mod search;
pub mod tags;

// Re-export core types
pub use matric_core::*;

// Re-export repository implementations
pub use embeddings::{utils as embedding_utils, PgEmbeddingRepository};
pub use jobs::PgJobRepository;
pub use links::PgLinkRepository;
pub use notes::PgNoteRepository;
pub use oauth::PgOAuthRepository;
pub use pool::{create_pool, create_pool_with_config, PoolConfig};
pub use search::PgFtsSearch;
pub use tags::PgTagRepository;

/// Combined database context with all repositories.
pub struct Database {
    /// The underlying connection pool.
    pub pool: sqlx::Pool<sqlx::Postgres>,
    /// Note repository for CRUD operations.
    pub notes: PgNoteRepository,
    /// Embedding repository for vector storage.
    pub embeddings: PgEmbeddingRepository,
    /// Link repository for note relationships.
    pub links: PgLinkRepository,
    /// Tag repository for tag management.
    pub tags: PgTagRepository,
    /// Job repository for background processing.
    pub jobs: PgJobRepository,
    /// Full-text search provider.
    pub search: PgFtsSearch,
    /// OAuth2 and API key repository.
    pub oauth: PgOAuthRepository,
}

impl Database {
    /// Create a new Database instance from a connection pool.
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self {
            notes: PgNoteRepository::new(pool.clone()),
            embeddings: PgEmbeddingRepository::new(pool.clone()),
            links: PgLinkRepository::new(pool.clone()),
            tags: PgTagRepository::new(pool.clone()),
            jobs: PgJobRepository::new(pool.clone()),
            search: PgFtsSearch::new(pool.clone()),
            oauth: PgOAuthRepository::new(pool.clone()),
            pool,
        }
    }

    /// Create a new Database instance by connecting to the given URL.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = create_pool(url).await?;
        Ok(Self::new(pool))
    }

    /// Create with custom pool configuration.
    pub async fn connect_with_config(url: &str, config: PoolConfig) -> Result<Self> {
        let pool = create_pool_with_config(url, config).await?;
        Ok(Self::new(pool))
    }

    /// Run pending migrations.
    #[cfg(feature = "migrations")]
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;
        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.pool
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self::new(self.pool.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_clone() {
        // Verify Database struct is Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<Database>();
    }
}
