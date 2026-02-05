//! Test that has_revision field correctly reflects whether AI actually enhanced content.
//!
//! This test suite validates:
//! - Issue #231: Notes with revision_mode="none" should show has_revision=false
//! - has_revision should be false when revised content equals original content
//! - has_revision should be true when revised content differs from original content
//!
//! Related issues:
//! - #231: Note with revision_mode="none" shows has_revision=true

use matric_core::{CreateNoteRequest, ListNotesRequest, NoteRepository};
use matric_db::{create_pool, PgNoteRepository};
use sqlx::PgPool;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

#[tokio::test]
async fn test_has_revision_false_when_content_identical() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    // Create a note with original content
    let original_content = "# Test Note\n\nThis is the original content.".to_string();
    let req = CreateNoteRequest {
        content: original_content.clone(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // At this point, the note has been created with original=revised (same content)
    // Verify via list endpoint that has_revision is false
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Find our note in the response
    let note_summary = response
        .notes
        .iter()
        .find(|n| n.id == note_id)
        .expect("Note not found in list response");

    // Issue #231: has_revision should be false when content is identical
    assert!(
        !note_summary.has_revision,
        "has_revision should be false when original and revised content are identical"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_has_revision_true_when_content_differs() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    // Create a note
    let original_content = "# Test Note\n\nOriginal content.".to_string();
    let req = CreateNoteRequest {
        content: original_content.clone(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Manually update the revised content to simulate AI enhancement
    let revised_content =
        "# Test Note\n\nThis content has been enhanced by AI with additional context and clarity."
            .to_string();

    repo.update_revised(note_id, &revised_content, Some("AI enhanced"))
        .await
        .expect("Failed to update revised content");

    // Verify via list endpoint that has_revision is true
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Find our note in the response
    let note_summary = response
        .notes
        .iter()
        .find(|n| n.id == note_id)
        .expect("Note not found in list response");

    // has_revision should be true when content differs
    assert!(
        note_summary.has_revision,
        "has_revision should be true when original and revised content differ"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_has_revision_false_after_revision_mode_none() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    // Create a note
    let original_content = "# Test Note\n\nContent that won't be revised.".to_string();
    let req = CreateNoteRequest {
        content: original_content.clone(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Simulate revision_mode="none" by copying original to revised
    // (This is what the API does when revision_mode="none")
    repo.update_revised(
        note_id,
        &original_content,
        Some("Original preserved (no AI revision)"),
    )
    .await
    .expect("Failed to update revised content");

    // Verify via list endpoint that has_revision is false
    let list_req = ListNotesRequest {
        limit: Some(10),
        offset: None,
        filter: None,
        sort_by: Some("created_at".to_string()),
        sort_order: Some("desc".to_string()),
        collection_id: None,
        tags: None,
        created_after: None,
        created_before: None,
        updated_after: None,
        updated_before: None,
    };

    let response = repo.list(list_req).await.expect("Failed to list notes");

    // Find our note in the response
    let note_summary = response
        .notes
        .iter()
        .find(|n| n.id == note_id)
        .expect("Note not found in list response");

    // Issue #231: has_revision should be false for revision_mode="none"
    assert!(
        !note_summary.has_revision,
        "has_revision should be false when revision_mode=none (content identical)"
    );

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}
