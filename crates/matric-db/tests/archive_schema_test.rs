//! Test suite for archive schema management (Epic #441: Parallel Memory Archives).
//!
//! Tests the creation, listing, and management of isolated PostgreSQL schemas
//! for parallel memory archives.

use chrono::Utc;
use matric_db::{ArchiveRepository, Database};
use sqlx::PgPool;

/// Helper to create a test database pool.
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_create_archive_schema() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a unique archive name
    let archive_name = format!("test-archive-{}", Utc::now().timestamp_millis());
    let schema_name = format!("archive_{}", archive_name.replace('-', "_"));

    // Test: Create a new archive schema
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("Test archive for parallel memory"))
        .await
        .expect("Failed to create archive schema");

    // Verify archive info
    assert_eq!(archive.name, archive_name);
    assert_eq!(archive.schema_name, schema_name);
    assert_eq!(
        archive.description,
        Some("Test archive for parallel memory".to_string())
    );
    assert_eq!(archive.note_count, Some(0));
    assert_eq!(archive.size_bytes, Some(0));
    assert!(!archive.is_default);

    // Cleanup: Drop the schema
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_list_archive_schemas() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create two test archives
    let archive1_name = format!("test-list-archive1-{}", Utc::now().timestamp_millis());
    let archive2_name = format!("test-list-archive2-{}", Utc::now().timestamp_millis());

    let archive1 = db
        .archives
        .create_archive_schema(&archive1_name, Some("First test archive"))
        .await
        .expect("Failed to create first archive");

    let archive2 = db
        .archives
        .create_archive_schema(&archive2_name, Some("Second test archive"))
        .await
        .expect("Failed to create second archive");

    // Test: List all archives
    let archives = db
        .archives
        .list_archive_schemas()
        .await
        .expect("Failed to list archives");

    // Verify both archives are in the list
    let archive1_found = archives.iter().any(|a| a.id == archive1.id);
    let archive2_found = archives.iter().any(|a| a.id == archive2.id);

    assert!(archive1_found, "First archive should be in the list");
    assert!(archive2_found, "Second archive should be in the list");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive1_name)
        .await
        .expect("Failed to drop first test archive");
    db.archives
        .drop_archive_schema(&archive2_name)
        .await
        .expect("Failed to drop second test archive");
}

#[tokio::test]
async fn test_get_archive_by_name() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-get-archive-{}", Utc::now().timestamp_millis());
    let description = "Archive for get test";

    // Create archive
    let created_archive = db
        .archives
        .create_archive_schema(&archive_name, Some(description))
        .await
        .expect("Failed to create archive");

    // Test: Get archive by name
    let retrieved_archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");

    // Verify retrieved data matches created data
    assert_eq!(retrieved_archive.id, created_archive.id);
    assert_eq!(retrieved_archive.name, archive_name);
    assert_eq!(retrieved_archive.description, Some(description.to_string()));

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_default_archive_uniqueness() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive1_name = format!("test-default1-{}", Utc::now().timestamp_millis());
    let archive2_name = format!("test-default2-{}", Utc::now().timestamp_millis());

    // Create first archive and mark as default
    db.archives
        .create_archive_schema(&archive1_name, Some("First default test"))
        .await
        .expect("Failed to create first archive");

    db.archives
        .set_default_archive(&archive1_name)
        .await
        .expect("Failed to set first archive as default");

    // Create second archive
    db.archives
        .create_archive_schema(&archive2_name, Some("Second default test"))
        .await
        .expect("Failed to create second archive");

    // Test: Setting second archive as default should unset the first
    db.archives
        .set_default_archive(&archive2_name)
        .await
        .expect("Failed to set second archive as default");

    // Verify only one is default
    let archive1 = db
        .archives
        .get_archive_by_name(&archive1_name)
        .await
        .expect("Failed to get first archive")
        .expect("First archive should exist");

    let archive2 = db
        .archives
        .get_archive_by_name(&archive2_name)
        .await
        .expect("Failed to get second archive")
        .expect("Second archive should exist");

    assert!(!archive1.is_default, "First archive should not be default");
    assert!(archive2.is_default, "Second archive should be default");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive1_name)
        .await
        .expect("Failed to drop first archive");
    db.archives
        .drop_archive_schema(&archive2_name)
        .await
        .expect("Failed to drop second archive");
}

#[tokio::test]
async fn test_archive_schema_isolation() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-isolation-{}", Utc::now().timestamp_millis());
    let schema_name = format!("archive_{}", archive_name.replace('-', "_"));

    // Create archive
    db.archives
        .create_archive_schema(&archive_name, Some("Isolation test"))
        .await
        .expect("Failed to create archive");

    // Test: Verify schema exists in PostgreSQL
    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check schema existence");

    assert!(schema_exists, "Archive schema should exist in PostgreSQL");

    // Test: Verify schema has a note table
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = $1 AND table_name = 'note'
        )",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check table existence");

    assert!(table_exists, "Archive schema should have a note table");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");

    // Verify schema was dropped
    let schema_exists_after: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check schema existence after drop");

    assert!(
        !schema_exists_after,
        "Archive schema should be dropped from PostgreSQL"
    );
}

#[tokio::test]
async fn test_duplicate_archive_name_fails() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-duplicate-{}", Utc::now().timestamp_millis());

    // Create first archive
    db.archives
        .create_archive_schema(&archive_name, Some("First archive"))
        .await
        .expect("Failed to create first archive");

    // Test: Attempt to create duplicate should fail
    let result = db
        .archives
        .create_archive_schema(&archive_name, Some("Duplicate archive"))
        .await;

    assert!(result.is_err(), "Creating duplicate archive should fail");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_update_archive_metadata() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-update-{}", Utc::now().timestamp_millis());

    // Create archive
    db.archives
        .create_archive_schema(&archive_name, Some("Original description"))
        .await
        .expect("Failed to create archive");

    // Test: Update archive metadata
    db.archives
        .update_archive_metadata(&archive_name, Some("Updated description"))
        .await
        .expect("Failed to update archive metadata");

    // Verify update
    let archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");

    assert_eq!(
        archive.description,
        Some("Updated description".to_string()),
        "Description should be updated"
    );

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}
