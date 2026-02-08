//! Schema-scoped database operations for parallel memory archives.
//!
//! Provides a `SchemaContext` abstraction that automatically sets the PostgreSQL
//! search_path for all operations, enabling schema-scoped data isolation.

use matric_core::{Error, Result};
use sqlx::{PgPool, Postgres, Transaction};
use std::future::Future;
use std::pin::Pin;

use crate::schema_validation::validate_schema_name;

/// A database context scoped to a specific PostgreSQL schema.
///
/// All operations executed through this context will automatically have their
/// search_path set to the specified schema, providing data isolation for
/// parallel memory archives.
///
/// # Examples
///
/// ```rust,ignore
/// use matric_db::{Database, SchemaContext};
///
/// let db = Database::connect("postgres://localhost/matric").await?;
/// let ctx = db.for_schema("archive_2026")?;
///
/// // All operations within this closure operate on the archive_2026 schema
/// ctx.execute(|tx| Box::pin(async move {
///     sqlx::query("INSERT INTO note (id, content) VALUES ($1, $2)")
///         .bind(note_id)
///         .bind(&content)
///         .execute(&mut **tx)
///         .await
/// })).await?;
/// ```
#[derive(Clone)]
pub struct SchemaContext {
    pool: PgPool,
    schema: String,
}

impl SchemaContext {
    /// Create a new SchemaContext for the specified schema.
    ///
    /// # Arguments
    ///
    /// * `pool` - The PostgreSQL connection pool
    /// * `schema` - The schema name (will be validated)
    ///
    /// # Returns
    ///
    /// A new `SchemaContext` instance
    ///
    /// # Errors
    ///
    /// Returns an error if the schema name is invalid or unsafe.
    pub fn new(pool: PgPool, schema: impl Into<String>) -> Result<Self> {
        let schema = schema.into();
        validate_schema_name(&schema)?;
        Ok(Self { pool, schema })
    }

    /// Get the schema name for this context.
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// Execute a write operation within a transaction with the schema search_path set.
    ///
    /// This method:
    /// 1. Begins a new transaction
    /// 2. Executes `SET LOCAL search_path TO {schema}, public`
    /// 3. Executes the provided closure with a mutable transaction reference
    /// 4. Commits the transaction if successful, rolls back on error
    ///
    /// # Arguments
    ///
    /// * `f` - An async closure that receives a mutable transaction reference
    ///
    /// # Returns
    ///
    /// The result returned by the closure
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// ctx.execute(|tx| Box::pin(async move {
    ///     sqlx::query("INSERT INTO note (id, content) VALUES ($1, $2)")
    ///         .bind(id)
    ///         .bind(content)
    ///         .execute(&mut **tx)
    ///         .await?;
    ///     Ok(id)
    /// })).await?;
    /// ```
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(
            &'a mut Transaction<'_, Postgres>,
        ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>,
    {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Set search_path for this transaction
        // Using parameterized query is not possible for SET commands, but we've
        // validated the schema name to prevent SQL injection
        let set_search_path = format!("SET LOCAL search_path TO {}, public", self.schema);
        sqlx::query(&set_search_path)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Execute the user's operation
        let result = f(&mut tx).await?;

        // Commit the transaction
        tx.commit().await.map_err(Error::Database)?;

        Ok(result)
    }

    /// Execute a read-only query with the schema search_path set.
    ///
    /// This method is similar to `execute` but optimized for read operations.
    /// It still uses a transaction to ensure the search_path is set correctly.
    ///
    /// # Arguments
    ///
    /// * `f` - An async closure that receives a mutable transaction reference
    ///
    /// # Returns
    ///
    /// The result returned by the closure
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let notes = ctx.query(|tx| Box::pin(async move {
    ///     sqlx::query_as::<_, Note>("SELECT * FROM note WHERE soft_deleted = false")
    ///         .fetch_all(&mut **tx)
    ///         .await
    /// })).await?;
    /// ```
    pub async fn query<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(
            &'a mut Transaction<'_, Postgres>,
        ) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>,
    {
        // For now, query() is identical to execute()
        // In the future, this could use a read-only transaction or connection
        self.execute(f).await
    }

    /// Begin a transaction with the schema search_path already set.
    ///
    /// Returns a transaction that the caller can use directly with `_tx` methods
    /// on repository references that can't be moved into closures (e.g., file_storage).
    ///
    /// The caller is responsible for committing or rolling back the transaction.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let ctx = db.for_schema(&archive_ctx.schema)?;
    /// let mut tx = ctx.begin_tx().await?;
    /// let result = file_storage.list_by_note_tx(&mut tx, note_id).await?;
    /// tx.commit().await.map_err(Error::Database)?;
    /// ```
    pub async fn begin_tx(&self) -> Result<Transaction<'_, Postgres>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let set_search_path = format!("SET LOCAL search_path TO {}, public", self.schema);
        sqlx::query(&set_search_path)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        Ok(tx)
    }

    /// Get a reference to the underlying connection pool.
    ///
    /// Use this when you need direct pool access for operations that don't
    /// require schema scoping.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests use #[tokio::test] because sqlx 0.8's connect_lazy
    // requires a Tokio runtime to spawn maintenance tasks
    #[tokio::test]
    async fn test_schema_context_new_valid() {
        // Create a test pool (won't actually connect)
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test")
            .expect("Failed to create lazy pool");

        let ctx = SchemaContext::new(pool.clone(), "test_schema");
        assert!(ctx.is_ok());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.schema(), "test_schema");
    }

    #[tokio::test]
    async fn test_schema_context_new_invalid() {
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test")
            .expect("Failed to create lazy pool");

        // Empty schema name
        let result = SchemaContext::new(pool.clone(), "");
        assert!(result.is_err());

        // Invalid characters
        let result = SchemaContext::new(pool.clone(), "schema'; DROP TABLE notes;--");
        assert!(result.is_err());

        // Reserved keyword (pg_catalog is reserved, public is intentionally allowed)
        let result = SchemaContext::new(pool.clone(), "pg_catalog");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schema_context_schema_getter() {
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test")
            .expect("Failed to create lazy pool");

        let ctx = SchemaContext::new(pool, "archive_2026").unwrap();
        assert_eq!(ctx.schema(), "archive_2026");
    }

    #[tokio::test]
    async fn test_schema_context_clone() {
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test")
            .expect("Failed to create lazy pool");

        let ctx = SchemaContext::new(pool, "test_schema").unwrap();
        let ctx_clone = ctx.clone();

        assert_eq!(ctx.schema(), ctx_clone.schema());
    }

    // Integration tests that require a real database connection
    #[tokio::test]
    async fn test_schema_context_sets_search_path() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| crate::test_fixtures::DEFAULT_TEST_DATABASE_URL.to_string());

        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Create a test schema
        sqlx::query("CREATE SCHEMA IF NOT EXISTS test_schema_ctx")
            .execute(&pool)
            .await
            .expect("Failed to create test schema");

        // Create the SchemaContext
        let ctx = SchemaContext::new(pool.clone(), "test_schema_ctx")
            .expect("Failed to create SchemaContext");

        // Test that search_path is set correctly
        let result = ctx
            .execute(|tx| {
                Box::pin(async move {
                    let search_path: String = sqlx::query_scalar("SHOW search_path")
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(Error::Database)?;

                    Ok(search_path)
                })
            })
            .await;

        assert!(result.is_ok());
        let search_path = result.unwrap();
        assert!(
            search_path.contains("test_schema_ctx"),
            "search_path: {}",
            search_path
        );

        // Cleanup
        sqlx::query("DROP SCHEMA IF EXISTS test_schema_ctx CASCADE")
            .execute(&pool)
            .await
            .expect("Failed to drop test schema");
    }

    #[tokio::test]
    async fn test_schema_context_transaction_rollback_on_error() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| crate::test_fixtures::DEFAULT_TEST_DATABASE_URL.to_string());

        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Create a test schema with a table
        sqlx::query("CREATE SCHEMA IF NOT EXISTS test_rollback")
            .execute(&pool)
            .await
            .expect("Failed to create test schema");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS test_rollback.test_table (id INT PRIMARY KEY, value TEXT)",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test table");

        let ctx = SchemaContext::new(pool.clone(), "test_rollback")
            .expect("Failed to create SchemaContext");

        // Test that transaction rolls back on error
        let result = ctx
            .execute(|tx| {
                Box::pin(async move {
                    sqlx::query("INSERT INTO test_table (id, value) VALUES (1, 'test')")
                        .execute(&mut **tx)
                        .await
                        .map_err(Error::Database)?;

                    // Intentionally cause an error
                    Err::<(), Error>(Error::Internal("intentional error".to_string()))
                })
            })
            .await;

        assert!(result.is_err());

        // Verify the insert was rolled back
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_rollback.test_table")
            .fetch_one(&pool)
            .await
            .expect("Failed to count rows");

        assert_eq!(count, 0, "Transaction should have been rolled back");

        // Cleanup
        sqlx::query("DROP SCHEMA IF EXISTS test_rollback CASCADE")
            .execute(&pool)
            .await
            .expect("Failed to drop test schema");
    }

    #[tokio::test]
    async fn test_schema_context_query_method() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| crate::test_fixtures::DEFAULT_TEST_DATABASE_URL.to_string());

        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let ctx =
            SchemaContext::new(pool.clone(), "public").expect("Failed to create SchemaContext");

        // Test the query method with a read operation
        let result = ctx
            .query(|tx| {
                Box::pin(async move {
                    let value: i32 = sqlx::query_scalar("SELECT 1")
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(Error::Database)?;

                    Ok(value)
                })
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }
}
