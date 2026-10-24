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
    let document_type_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM document_type WHERE tenant_id = $1 AND name = 'markdown'",
    )
    .bind(Uuid::nil())
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
        title: None,
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
        title: None,
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

async fn synthetic_tenant(conn: &mut sqlx::PgConnection) -> Uuid {
    let tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Document type test','active')")
        .bind(tenant).bind(format!("document-type-test-{tenant}"))
        .execute(&mut *conn).await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant',$1,true)")
        .bind(tenant.to_string())
        .execute(conn)
        .await
        .unwrap();
    tenant
}

fn synthetic_note(document_type_id: Option<Uuid>) -> CreateNoteRequest {
    CreateNoteRequest {
        content: "Synthetic document type regression".into(),
        format: "markdown".into(),
        source: "test".into(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id,
        title: None,
    }
}

#[tokio::test]
async fn tenant_without_types_can_create_notes_and_rejects_foreign_assignments() {
    use matric_db::PgDocumentTypeRepository;
    let pool = setup_test_pool().await;
    let repo = PgNoteRepository::new(pool.clone());
    let local_type: Uuid =
        sqlx::query_scalar("SELECT id FROM document_type WHERE tenant_id=$1 AND name='markdown'")
            .bind(Uuid::nil())
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let tenant = synthetic_tenant(&mut tx).await;
    // Exercise every detection stage, including the plaintext fallback.
    for (filename, content, mime) in [
        (Some("Dockerfile"), None, None),
        (Some("note.rs"), None, None),
        (None, None, Some("text/markdown")),
        (None, Some("fn main() { println!(\"synthetic\"); }"), None),
        (None, Some("unrecognized synthetic content"), None),
    ] {
        assert!(
            PgDocumentTypeRepository::detect_tx(&mut tx, filename, content, mime)
                .await
                .unwrap()
                .is_none()
        );
    }
    let id = repo.insert_tx(&mut tx, synthetic_note(None)).await.unwrap();
    let ids = repo
        .insert_bulk_tx(&mut tx, vec![synthetic_note(None)])
        .await
        .unwrap();
    assert_eq!(ids.len(), 1);
    for insert_bulk in [false, true] {
        let result = if insert_bulk {
            repo.insert_bulk_tx(&mut tx, vec![synthetic_note(Some(local_type))])
                .await
                .map(|ids| ids[0])
        } else {
            repo.insert_tx(&mut tx, synthetic_note(Some(local_type)))
                .await
        };
        assert!(matches!(result, Err(matric_core::Error::InvalidInput(_))));
    }
    let row: (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT tenant_id, document_type_id FROM note WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(row, (tenant, None));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn detection_and_attachment_assignments_use_only_current_tenant_types() {
    use matric_core::DocumentTypeRepository;
    use matric_db::{FilesystemBackend, PgDocumentTypeRepository, PgFileStorageRepository};
    let pool = setup_test_pool().await;
    let detector = PgDocumentTypeRepository::new(pool.clone());
    let local = detector
        .detect(Some("note.rs"), None, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local.document_type.name, "rust");
    let local_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM document_type d WHERE tenant_id=$1",
    )
    .bind(Uuid::nil())
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let tenant = synthetic_tenant(&mut tx).await;
    let own_type = Uuid::new_v4();
    sqlx::query("INSERT INTO document_type(id,tenant_id,name,display_name,category,file_extensions,mime_types,filename_patterns,magic_patterns) VALUES($1,$2,'plaintext','Synthetic','custom',ARRAY['.rs'],ARRAY['text/markdown'],ARRAY['Dockerfile'],ARRAY['SYNTHETIC'])")
        .bind(own_type).bind(tenant).execute(&mut *tx).await.unwrap();
    for (filename, content, mime) in [
        (Some("Dockerfile"), None, None),
        (Some("note.rs"), None, None),
        (None, None, Some("text/markdown")),
        (None, Some("SYNTHETIC"), None),
        (None, Some("unrecognized"), None),
    ] {
        assert_eq!(
            PgDocumentTypeRepository::detect_tx(&mut tx, filename, content, mime)
                .await
                .unwrap()
                .unwrap()
                .document_type
                .id,
            own_type
        );
    }
    let repo = PgNoteRepository::new(pool.clone());
    let note = repo
        .insert_tx(&mut tx, synthetic_note(Some(own_type)))
        .await
        .unwrap();
    let storage = PgFileStorageRepository::new(pool.clone(), FilesystemBackend::new("/tmp"), 1024);
    let attachment = storage
        .store_file_tx(&mut tx, note, "synthetic.txt", "text/plain", b"synthetic")
        .await
        .unwrap();
    assert!(matches!(
        storage
            .set_document_type_tx(&mut tx, attachment.id, local.document_type.id, None)
            .await,
        Err(matric_core::Error::InvalidInput(_))
    ));
    for promote in [false, true] {
        assert!(matches!(
            storage
                .set_detected_document_type_tx(
                    &mut tx,
                    attachment.id,
                    local.document_type.id,
                    1.0,
                    "test",
                    promote
                )
                .await,
            Err(matric_core::Error::InvalidInput(_))
        ));
    }
    storage
        .set_document_type_tx(&mut tx, attachment.id, own_type, None)
        .await
        .unwrap();
    storage
        .set_detected_document_type_tx(&mut tx, attachment.id, own_type, 1.0, "test", true)
        .await
        .unwrap();
    let assigned: Uuid = sqlx::query_scalar("SELECT document_type_id FROM attachment WHERE id=$1")
        .bind(attachment.id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(assigned, own_type);
    tx.rollback().await.unwrap();
    let local_after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM document_type d WHERE tenant_id=$1",
    )
    .bind(Uuid::nil())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(local_before, local_after);
    assert_eq!(
        detector
            .detect(Some("note.rs"), None, None)
            .await
            .unwrap()
            .unwrap()
            .document_type
            .id,
        local.document_type.id
    );
}
