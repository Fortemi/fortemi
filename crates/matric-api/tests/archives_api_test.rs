//! Integration tests for archive management API endpoints.
//!
//! Tests the REST API handlers for parallel memory archives (Epic #441).

use matric_core::{ArchiveInfo, ArchiveRepository};
use matric_db::Database;
use uuid::Uuid;

/// Test helper to create a test archive.
async fn create_test_archive(db: &Database, name: &str) -> ArchiveInfo {
    db.archives
        .create_archive_schema(name, Some("Test archive"))
        .await
        .expect("Failed to create test archive")
}

/// Test helper to cleanup test archives.
async fn cleanup_archive(db: &Database, name: &str) {
    let _ = db.archives.drop_archive_schema(name).await;
}

#[tokio::test]
async fn test_archive_lifecycle() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_archive_{}", Uuid::new_v4());

    // Create archive
    let archive = create_test_archive(&db, &archive_name).await;
    assert_eq!(archive.name, archive_name);
    assert!(archive.description.is_some());
    assert!(!archive.is_default);

    // List archives - should include our test archive
    let archives = db
        .archives
        .list_archive_schemas()
        .await
        .expect("Failed to list archives");
    assert!(!archives.is_empty());
    assert!(archives.iter().any(|a| a.name == archive_name));

    // Get archive by name
    let retrieved = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive not found");
    assert_eq!(retrieved.name, archive_name);

    // Update archive metadata
    db.archives
        .update_archive_metadata(&archive_name, Some("Updated description"))
        .await
        .expect("Failed to update archive");

    let updated = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get updated archive")
        .expect("Archive not found");
    assert_eq!(updated.description, Some("Updated description".to_string()));

    // Set as default
    db.archives
        .set_default_archive(&archive_name)
        .await
        .expect("Failed to set default");

    let default_archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get default archive")
        .expect("Archive not found");
    assert!(default_archive.is_default);

    // Cleanup
    cleanup_archive(&db, &archive_name).await;

    // Verify deletion
    let deleted = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to check if archive was deleted");
    assert!(deleted.is_none());
}

#[tokio::test]
async fn test_archive_stats_update() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_stats_{}", Uuid::new_v4());
    let _archive = create_test_archive(&db, &archive_name).await;

    // Update stats
    db.archives
        .update_archive_stats(&archive_name)
        .await
        .expect("Failed to update stats");

    let archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive not found");

    // Should have stats populated
    assert!(archive.note_count.is_some());
    assert!(archive.size_bytes.is_some());
    assert_eq!(archive.note_count.unwrap(), 0); // New archive has no notes

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_get_nonexistent_archive() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .expect("Failed to connect to database");

    let result = db
        .archives
        .get_archive_by_name("nonexistent_archive_xyz")
        .await
        .expect("Query should succeed");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_update_nonexistent_archive() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .expect("Failed to connect to database");

    let result = db
        .archives
        .update_archive_metadata("nonexistent_archive_xyz", Some("Description"))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_delete_nonexistent_archive() {
    let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .expect("Failed to connect to database");

    let result = db
        .archives
        .drop_archive_schema("nonexistent_archive_xyz")
        .await;

    assert!(result.is_err());
}
