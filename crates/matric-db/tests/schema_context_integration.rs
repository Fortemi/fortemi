//! Integration tests for SchemaContext and schema validation.
//!
//! These tests verify that the SchemaContext abstraction correctly sets
//! the search_path and provides data isolation for parallel memory archives.

use matric_core::Error;
use matric_db::{validate_schema_name, Database, SchemaContext};

#[test]
fn test_validate_schema_name_valid() {
    assert!(validate_schema_name("my_schema").is_ok());
    assert!(validate_schema_name("schema123").is_ok());
    assert!(validate_schema_name("_private").is_ok());
    assert!(validate_schema_name("archive_2026").is_ok());
}

#[test]
fn test_validate_schema_name_empty() {
    let result = validate_schema_name("");
    assert!(result.is_err());
}

#[test]
fn test_validate_schema_name_too_long() {
    let long_name = "a".repeat(64);
    let result = validate_schema_name(&long_name);
    assert!(result.is_err());
}

#[test]
fn test_validate_schema_name_invalid_chars() {
    assert!(validate_schema_name("schema-name").is_err());
    assert!(validate_schema_name("schema.name").is_err());
    assert!(validate_schema_name("schema name").is_err());
    assert!(validate_schema_name("schema;DROP").is_err());
}

#[test]
fn test_validate_schema_name_reserved_keywords() {
    // "public" is now allowed - it's the default PostgreSQL schema
    assert!(validate_schema_name("public").is_ok());
    assert!(validate_schema_name("select").is_err());
    assert!(validate_schema_name("DROP").is_err());
    // System schemas should still be blocked
    assert!(validate_schema_name("pg_catalog").is_err());
    assert!(validate_schema_name("information_schema").is_err());
}

#[tokio::test]
async fn test_schema_context_creation() {
    let pool = sqlx::Pool::<sqlx::Postgres>::connect_lazy("postgres://test:test@localhost/test")
        .expect("Failed to create lazy pool");

    // Valid schema name
    let ctx = SchemaContext::new(pool.clone(), "test_schema");
    assert!(ctx.is_ok());
    assert_eq!(ctx.unwrap().schema(), "test_schema");

    // Invalid schema name
    let result = SchemaContext::new(pool.clone(), "");
    assert!(result.is_err());

    // "public" is now valid (the default PostgreSQL schema)
    let result = SchemaContext::new(pool.clone(), "public");
    assert!(result.is_ok());

    // Reserved system schemas should still fail
    let result = SchemaContext::new(pool.clone(), "pg_catalog");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_database_for_schema() {
    let pool = sqlx::Pool::<sqlx::Postgres>::connect_lazy("postgres://test:test@localhost/test")
        .expect("Failed to create lazy pool");

    let db = Database::new(pool);

    // Valid schema
    let result = db.for_schema("archive_2026");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().schema(), "archive_2026");

    // Invalid schema
    let result = db.for_schema("");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_database_default_schema() {
    let pool = sqlx::Pool::<sqlx::Postgres>::connect_lazy("postgres://test:test@localhost/test")
        .expect("Failed to create lazy pool");

    let db = Database::new(pool);
    let ctx = db.default_schema();
    assert_eq!(ctx.schema(), "public");
}

// Integration test requiring a real database connection
#[tokio::test]
async fn test_schema_context_sets_search_path() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:15432/matric_test".to_string());

    let pool = sqlx::Pool::<sqlx::Postgres>::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Create a test schema
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_schema_integration")
        .execute(&pool)
        .await
        .expect("Failed to create test schema");

    let ctx = SchemaContext::new(pool.clone(), "test_schema_integration")
        .expect("Failed to create SchemaContext");

    // Execute a query and verify search_path
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
        search_path.contains("test_schema_integration"),
        "Expected search_path to contain 'test_schema_integration', got: {}",
        search_path
    );

    // Cleanup
    sqlx::query("DROP SCHEMA IF EXISTS test_schema_integration CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to drop test schema");
}

#[tokio::test]
async fn test_schema_context_transaction_commit() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:15432/matric_test".to_string());

    let pool = sqlx::Pool::<sqlx::Postgres>::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Create a test schema with a table
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_commit")
        .execute(&pool)
        .await
        .expect("Failed to create test schema");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS test_commit.test_table (id INT PRIMARY KEY, value TEXT)",
    )
    .execute(&pool)
    .await
    .expect("Failed to create test table");

    let ctx =
        SchemaContext::new(pool.clone(), "test_commit").expect("Failed to create SchemaContext");

    // Insert a row
    let result = ctx
        .execute(|tx| {
            Box::pin(async move {
                sqlx::query("INSERT INTO test_table (id, value) VALUES (1, 'test')")
                    .execute(&mut **tx)
                    .await
                    .map_err(Error::Database)?;

                Ok(())
            })
        })
        .await;

    assert!(result.is_ok());

    // Verify the row was committed
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_commit.test_table")
        .fetch_one(&pool)
        .await
        .expect("Failed to count rows");

    assert_eq!(count, 1);

    // Cleanup
    sqlx::query("DROP SCHEMA IF EXISTS test_commit CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to drop test schema");
}

#[tokio::test]
async fn test_schema_context_transaction_rollback() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:15432/matric_test".to_string());

    let pool = sqlx::Pool::<sqlx::Postgres>::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Create a test schema with a table
    sqlx::query("CREATE SCHEMA IF NOT EXISTS test_rollback_int")
        .execute(&pool)
        .await
        .expect("Failed to create test schema");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS test_rollback_int.test_table (id INT PRIMARY KEY, value TEXT)",
    )
    .execute(&pool)
    .await
    .expect("Failed to create test table");

    let ctx = SchemaContext::new(pool.clone(), "test_rollback_int")
        .expect("Failed to create SchemaContext");

    // Attempt to insert a row, but error out
    let result: Result<(), Error> = ctx
        .execute(|tx| {
            Box::pin(async move {
                sqlx::query("INSERT INTO test_table (id, value) VALUES (1, 'test')")
                    .execute(&mut **tx)
                    .await
                    .map_err(Error::Database)?;

                // Intentionally return an error to trigger rollback
                Err(Error::Internal("intentional error".to_string()))
            })
        })
        .await;

    assert!(result.is_err());

    // Verify the row was NOT committed (rolled back)
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_rollback_int.test_table")
        .fetch_one(&pool)
        .await
        .expect("Failed to count rows");

    assert_eq!(count, 0);

    // Cleanup
    sqlx::query("DROP SCHEMA IF EXISTS test_rollback_int CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to drop test schema");
}
