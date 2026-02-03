//! Test to verify that tag note_count is properly updated after soft delete.
//!
//! This test verifies issue #363: When a note is soft-deleted, the note_count
//! for its tags should be decremented to exclude the deleted note.

use chrono::Utc;
use matric_core::{CreateNoteRequest, NoteRepository, TagRepository};
use matric_db::{PgNoteRepository, PgTagRepository};
use sqlx::PgPool;

/// Helper to create a test database pool
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_tag_count_after_soft_delete() {
    let pool = setup_test_db().await;
    let note_repo = PgNoteRepository::new(pool.clone());
    let tag_repo = PgTagRepository::new(pool.clone());

    // Create a unique tag name for this test
    let tag_name = format!("test-tag-{}", Utc::now().timestamp_millis());

    // Step 1: Create a note with the tag
    let note_id = note_repo
        .insert(CreateNoteRequest {
            content: "Test note for tag count verification".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: Some(vec![tag_name.clone()]),
            metadata: None,
            document_type_id: None,
        })
        .await
        .expect("Failed to create note");

    // Step 2: Verify tag note_count is 1
    let tags_before = tag_repo.list().await.expect("Failed to list tags");
    let tag_before = tags_before
        .iter()
        .find(|t| t.name == tag_name)
        .expect("Tag should exist");

    assert_eq!(
        tag_before.note_count, 1,
        "Tag should have note_count = 1 before soft delete"
    );

    // Step 3: Soft-delete the note
    note_repo
        .soft_delete(note_id)
        .await
        .expect("Failed to soft-delete note");

    // Step 4: Verify tag note_count is 0 after soft delete
    let tags_after = tag_repo.list().await.expect("Failed to list tags");
    let tag_after = tags_after
        .iter()
        .find(|t| t.name == tag_name)
        .expect("Tag should still exist");

    assert_eq!(
        tag_after.note_count, 0,
        "Tag should have note_count = 0 after soft delete (issue #363)"
    );

    // Cleanup: Hard delete the note (this also removes tag associations)
    note_repo
        .hard_delete(note_id)
        .await
        .expect("Failed to hard-delete note");

    sqlx::query("DELETE FROM tag WHERE name = $1")
        .bind(&tag_name)
        .execute(&pool)
        .await
        .expect("Failed to cleanup test tag");
}

#[tokio::test]
async fn test_tag_count_with_multiple_notes() {
    let pool = setup_test_db().await;
    let note_repo = PgNoteRepository::new(pool.clone());
    let tag_repo = PgTagRepository::new(pool.clone());

    // Create a unique tag name for this test
    let tag_name = format!("test-multi-tag-{}", Utc::now().timestamp_millis());

    // Create two notes with the same tag
    let note1_id = note_repo
        .insert(CreateNoteRequest {
            content: "First note".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: Some(vec![tag_name.clone()]),
            metadata: None,
            document_type_id: None,
        })
        .await
        .expect("Failed to create note 1");

    let note2_id = note_repo
        .insert(CreateNoteRequest {
            content: "Second note".to_string(),
            format: "markdown".to_string(),
            source: "test".to_string(),
            collection_id: None,
            tags: Some(vec![tag_name.clone()]),
            metadata: None,
            document_type_id: None,
        })
        .await
        .expect("Failed to create note 2");

    // Verify tag note_count is 2
    let tags = tag_repo.list().await.expect("Failed to list tags");
    let tag = tags
        .iter()
        .find(|t| t.name == tag_name)
        .expect("Tag should exist");
    assert_eq!(tag.note_count, 2, "Tag should have note_count = 2");

    // Soft-delete first note
    note_repo
        .soft_delete(note1_id)
        .await
        .expect("Failed to soft-delete note 1");

    // Verify tag note_count is 1
    let tags = tag_repo.list().await.expect("Failed to list tags");
    let tag = tags
        .iter()
        .find(|t| t.name == tag_name)
        .expect("Tag should exist");
    assert_eq!(
        tag.note_count, 1,
        "Tag should have note_count = 1 after soft-deleting one note"
    );

    // Soft-delete second note
    note_repo
        .soft_delete(note2_id)
        .await
        .expect("Failed to soft-delete note 2");

    // Verify tag note_count is 0
    let tags = tag_repo.list().await.expect("Failed to list tags");
    let tag = tags
        .iter()
        .find(|t| t.name == tag_name)
        .expect("Tag should exist");
    assert_eq!(
        tag.note_count, 0,
        "Tag should have note_count = 0 after soft-deleting all notes"
    );

    // Cleanup
    note_repo
        .hard_delete(note1_id)
        .await
        .expect("Failed to hard-delete note 1");
    note_repo
        .hard_delete(note2_id)
        .await
        .expect("Failed to hard-delete note 2");

    sqlx::query("DELETE FROM tag WHERE name = $1")
        .bind(&tag_name)
        .execute(&pool)
        .await
        .expect("Failed to cleanup test tag");
}
