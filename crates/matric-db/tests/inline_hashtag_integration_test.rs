//! Integration test for inline hashtag extraction (Issue #248)

use matric_core::{CreateNoteRequest, NoteRepository};
use matric_db::Database;

#[tokio::test]
async fn test_inline_hashtag_extraction_on_note_creation() {
    // Set up test database
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:15432/matric_test".to_string());

    let db = Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let content = r#"
# Testing Inline Hashtags

This note tests whether inline #hashtags are extracted.

Topics:
- #inline-extraction behavior
- #tag-parsing rules

Use explicit #test-tag too.
"#;

    let req = CreateNoteRequest {
        content: content.to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["explicit-tag".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    // Create note
    let note_id = db.notes.insert(req).await.expect("Failed to create note");

    // Fetch note
    let note = db.notes.fetch(note_id).await.expect("Failed to fetch note");

    // Verify tags contain both explicit and inline hashtags
    assert!(
        note.tags.contains(&"explicit-tag".to_string()),
        "Should contain explicit tag"
    );
    assert!(
        note.tags.contains(&"hashtags".to_string()),
        "Should contain 'hashtags' from inline #hashtags"
    );
    assert!(
        note.tags.contains(&"inline-extraction".to_string()),
        "Should contain 'inline-extraction'"
    );
    assert!(
        note.tags.contains(&"tag-parsing".to_string()),
        "Should contain 'tag-parsing'"
    );
    assert!(
        note.tags.contains(&"test-tag".to_string()),
        "Should contain 'test-tag'"
    );

    // Clean up
    let _ = db.notes.hard_delete(note_id).await;
}

#[tokio::test]
async fn test_no_inline_hashtags() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:15432/matric_test".to_string());

    let db = Database::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let content = "This note has no hashtags at all";

    let req = CreateNoteRequest {
        content: content.to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["explicit-only".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = db.notes.insert(req).await.expect("Failed to create note");
    let note = db.notes.fetch(note_id).await.expect("Failed to fetch note");

    // Should only have explicit tag
    assert_eq!(note.tags.len(), 1);
    assert!(note.tags.contains(&"explicit-only".to_string()));

    // Clean up
    let _ = db.notes.hard_delete(note_id).await;
}
