//! Integration tests for archive schema routing in note handlers.
//!
//! Tests that note CRUD operations correctly route to archive-specific schemas
//! and maintain data isolation between archives.

use matric_core::{ArchiveRepository, CreateNoteRequest, NoteRepository};
use matric_db::{Database, PgNoteRepository};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string())
}

/// Test helper to create a test archive.
async fn create_test_archive(db: &Database, name: &str) -> String {
    let archive = db
        .archives
        .create_archive_schema(name, Some("Test archive"))
        .await
        .expect("Failed to create test archive");
    archive.schema_name
}

/// Test helper to cleanup test archives.
async fn cleanup_archive(db: &Database, name: &str) {
    let _ = db.archives.drop_archive_schema(name).await;
}

#[tokio::test]
async fn test_note_create_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_create_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive schema using SchemaContext
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test note in archive".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: Some(vec!["test".to_string()]),
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    // Verify note exists in archive schema
    let notes2 = PgNoteRepository::new(db.pool.clone());
    let note = ctx
        .query(move |tx| Box::pin(async move { notes2.fetch_tx(tx, note_id).await }))
        .await
        .expect("Failed to fetch note");

    assert_eq!(note.note.id, note_id);
    assert_eq!(note.original.content, "Test note in archive");

    // Verify note does NOT exist in public schema
    let public_notes = PgNoteRepository::new(db.pool.clone());
    let public_result = public_notes.fetch(note_id).await;
    assert!(
        public_result.is_err(),
        "Note should not exist in public schema"
    );

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_note_list_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_list_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_list_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create note in archive1
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let notes1 = PgNoteRepository::new(db.pool.clone());
    let req1 = CreateNoteRequest {
        content: "Note in archive 1".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note1_id = ctx1
        .execute(move |tx| Box::pin(async move { notes1.insert_tx(tx, req1).await }))
        .await
        .expect("Failed to create note in archive1");

    // Create note in archive2
    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let notes2 = PgNoteRepository::new(db.pool.clone());
    let req2 = CreateNoteRequest {
        content: "Note in archive 2".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note2_id = ctx2
        .execute(move |tx| Box::pin(async move { notes2.insert_tx(tx, req2).await }))
        .await
        .expect("Failed to create note in archive2");

    // List notes in archive1 - should only see note1
    let notes1_list = PgNoteRepository::new(db.pool.clone());
    let list1 = ctx1
        .query(move |tx| Box::pin(async move { notes1_list.list_all_ids_tx(tx).await }))
        .await
        .expect("Failed to list notes in archive1");

    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0], note1_id);

    // List notes in archive2 - should only see note2
    let notes2_list = PgNoteRepository::new(db.pool.clone());
    let list2 = ctx2
        .query(move |tx| Box::pin(async move { notes2_list.list_all_ids_tx(tx).await }))
        .await
        .expect("Failed to list notes in archive2");

    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0], note2_id);

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_note_update_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_update_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes1 = PgNoteRepository::new(db.pool.clone());
    let req = CreateNoteRequest {
        content: "Original content".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes1.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    // Update note content
    let notes2 = PgNoteRepository::new(db.pool.clone());
    let new_content = "Updated content".to_string();
    ctx.execute(move |tx| {
        Box::pin(async move { notes2.update_original_tx(tx, note_id, &new_content).await })
    })
    .await
    .expect("Failed to update note");

    // Verify update
    let notes3 = PgNoteRepository::new(db.pool.clone());
    let updated = ctx
        .query(move |tx| Box::pin(async move { notes3.fetch_tx(tx, note_id).await }))
        .await
        .expect("Failed to fetch updated note");

    assert_eq!(updated.original.content, "Updated content");

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_note_delete_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_delete_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes1 = PgNoteRepository::new(db.pool.clone());
    let req = CreateNoteRequest {
        content: "To be deleted".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes1.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    // Soft delete note
    let notes2 = PgNoteRepository::new(db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes2.soft_delete_tx(tx, note_id).await }))
        .await
        .expect("Failed to soft delete note");

    // Verify note is soft deleted by checking deleted_at timestamp
    // Note: fetch_tx still returns soft-deleted notes (they exist in DB with deleted_at set)
    let deleted_note = ctx
        .query(move |tx| {
            Box::pin(async move {
                // Query deleted_at directly to verify soft delete
                let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
                    sqlx::query_scalar("SELECT deleted_at FROM note WHERE id = $1")
                        .bind(note_id)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?;
                Ok::<_, matric_core::Error>(deleted_at)
            })
        })
        .await
        .expect("Failed to check deleted_at");

    assert!(
        deleted_note.is_some(),
        "Note should have deleted_at timestamp set"
    );

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_note_restore_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_restore_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create and soft delete note
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes1 = PgNoteRepository::new(db.pool.clone());
    let req = CreateNoteRequest {
        content: "To be restored".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes1.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    let notes2 = PgNoteRepository::new(db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes2.soft_delete_tx(tx, note_id).await }))
        .await
        .expect("Failed to soft delete note");

    // Restore note
    let notes3 = PgNoteRepository::new(db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { notes3.restore_tx(tx, note_id).await }))
        .await
        .expect("Failed to restore note");

    // Verify note is restored (deleted_at should be NULL)
    let restored = ctx
        .query(move |tx| {
            Box::pin(async move {
                // Query deleted_at directly to verify restore
                let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
                    sqlx::query_scalar("SELECT deleted_at FROM note WHERE id = $1")
                        .bind(note_id)
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(matric_core::Error::Database)?;
                Ok::<_, matric_core::Error>(deleted_at)
            })
        })
        .await
        .expect("Failed to check deleted_at");

    assert!(
        restored.is_none(),
        "Restored note should have deleted_at = NULL"
    );

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_public_schema_passthrough() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    // Create note using SchemaContext with "public" schema (should work as no-op)
    let ctx = db
        .for_schema("public")
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test note in public schema".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note in public schema");

    // Verify note exists in public schema (using both ctx and direct access)
    let notes2 = PgNoteRepository::new(db.pool.clone());
    let note_via_ctx = ctx
        .query(move |tx| Box::pin(async move { notes2.fetch_tx(tx, note_id).await }))
        .await
        .expect("Failed to fetch note via ctx");

    let notes3 = PgNoteRepository::new(db.pool.clone());
    let note_direct = notes3
        .fetch(note_id)
        .await
        .expect("Failed to fetch note directly");

    assert_eq!(note_via_ctx.note.id, note_direct.note.id);
    assert_eq!(note_via_ctx.original.content, "Test note in public schema");

    // Cleanup
    let notes4 = PgNoteRepository::new(db.pool.clone());
    let _ = notes4.hard_delete(note_id).await;
}
