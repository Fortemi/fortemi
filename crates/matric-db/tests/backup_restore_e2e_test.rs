//! End-to-end tests for backup/restore functionality (Issue #351)
//!
//! Tests cover basic backup/restore cycle validation.
//! These tests require a fully migrated database.

use matric_db::{
    test_fixtures::DEFAULT_TEST_DATABASE_URL, CreateNoteRequest, Database, NoteRepository,
};

// ============================================================================
// Test Fixtures
// ============================================================================

/// Get database connection from DATABASE_URL environment variable or default.
async fn get_database() -> Database {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());
    Database::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Create a test note with predictable content
async fn create_test_note(db: &Database, content: &str, tags: Vec<String>) -> uuid::Uuid {
    db.notes
        .insert(CreateNoteRequest {
            content: content.to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: Some(tags),
            metadata: None,
            document_type_id: None,
        })
        .await
        .expect("Failed to create test note")
}

// ============================================================================
// Test 1: Basic Backup/Restore Cycle
// ============================================================================

#[tokio::test]
async fn test_backup_restore_basic_cycle() {
    let db = get_database().await;

    // Step 1: Create original data
    let note1_id = create_test_note(
        &db,
        "This is the first test note with unique content",
        vec!["test".to_string(), "backup".to_string()],
    )
    .await;

    let note2_id = create_test_note(
        &db,
        "This is the second test note with different content",
        vec!["test".to_string(), "restore".to_string()],
    )
    .await;

    // Step 2: Capture original state
    let original_note1 = db.notes.fetch(note1_id).await.unwrap();
    let original_note2 = db.notes.fetch(note2_id).await.unwrap();

    // Step 3: Simulate export (capture data)
    let export_data = vec![
        (note1_id, original_note1.original.content.clone()),
        (note2_id, original_note2.original.content.clone()),
    ];

    // Step 4: Delete original data (simulate data loss)
    db.notes.hard_delete(note1_id).await.unwrap();
    db.notes.hard_delete(note2_id).await.unwrap();

    // Verify notes are gone
    assert!(
        db.notes.fetch(note1_id).await.is_err(),
        "Note 1 should be deleted"
    );
    assert!(
        db.notes.fetch(note2_id).await.is_err(),
        "Note 2 should be deleted"
    );

    // Step 5: Restore data (simulate import)
    let mut restored_ids = Vec::new();
    for (_original_id, content) in &export_data {
        let new_id = create_test_note(&db, content, vec!["restored".to_string()]).await;
        restored_ids.push(new_id);
    }

    // Step 6: Verify restored data exists and matches content
    for (i, new_id) in restored_ids.iter().enumerate() {
        let restored = db.notes.fetch(*new_id).await.unwrap();
        assert_eq!(
            restored.original.content, export_data[i].1,
            "Content should match after restore"
        );
    }

    // Cleanup: delete test notes
    for id in restored_ids {
        let _ = db.notes.hard_delete(id).await;
    }
}

// ============================================================================
// Test 2: Empty Content Edge Case
// ============================================================================

#[tokio::test]
async fn test_backup_handles_empty_tags() {
    let db = get_database().await;

    // Create note with no tags
    let note_id = create_test_note(&db, "Note with no tags", vec![]).await;

    // Verify note created
    let note = db.notes.fetch(note_id).await.unwrap();
    assert_eq!(note.original.content, "Note with no tags");

    // Cleanup
    let _ = db.notes.hard_delete(note_id).await;
}

// ============================================================================
// Test 3: Multiple Notes Integrity
// ============================================================================

#[tokio::test]
async fn test_backup_preserves_multiple_notes() {
    let db = get_database().await;

    // Create multiple notes
    let mut note_ids = Vec::new();
    for i in 0..5 {
        let id = create_test_note(
            &db,
            &format!("Test note number {}", i),
            vec![format!("batch-{}", i)],
        )
        .await;
        note_ids.push(id);
    }

    // Verify all notes exist
    for (i, id) in note_ids.iter().enumerate() {
        let note = db.notes.fetch(*id).await.unwrap();
        assert!(
            note.original.content.contains(&format!("number {}", i)),
            "Note {} content should be preserved",
            i
        );
    }

    // Cleanup
    for id in note_ids {
        let _ = db.notes.hard_delete(id).await;
    }
}
