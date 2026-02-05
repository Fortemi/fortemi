//! Test that document_type_id is properly stored and retrieved from notes.

use matric_core::{CreateNoteRequest, NoteRepository};
use matric_db::{create_pool, PgNoteRepository};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    create_pool(&database_url)
        .await
        .expect("Failed to create test pool")
}

#[tokio::test]
async fn test_note_with_document_type_id() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());

    // First, get a document type ID from the database
    let document_type_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM document_type WHERE name = 'markdown' LIMIT 1")
            .fetch_optional(&pool)
            .await
            .expect("Failed to query document_type");

    let document_type_id = document_type_id.expect("Markdown document type not found in database");

    // Create a note with document_type_id
    let req = CreateNoteRequest {
        content: "# Test Note\n\nThis is a test note with a document type.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: Some(document_type_id),
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Fetch the note and verify document_type_id is set
    let note = repo.fetch(note_id).await.expect("Failed to fetch note");

    assert_eq!(note.note.document_type_id, Some(document_type_id));

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}

#[tokio::test]
async fn test_note_without_document_type_id() {
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool);

    // Create a note WITHOUT document_type_id
    let req = CreateNoteRequest {
        content: "# Test Note\n\nThis note has no document type.".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = repo.insert(req).await.expect("Failed to insert note");

    // Fetch the note and verify document_type_id is None
    let note = repo.fetch(note_id).await.expect("Failed to fetch note");

    assert_eq!(note.note.document_type_id, None);

    // Clean up
    repo.hard_delete(note_id)
        .await
        .expect("Failed to delete test note");
}
