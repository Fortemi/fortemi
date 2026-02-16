//! Integration tests for multi-memory job pipeline (Issue #413) and
//! default embedding set seeding on archive creation (Issue #414).
//!
//! Issue #413: Non-default archives couldn't queue AI pipeline jobs due to an FK
//! constraint on `job_queue.note_id` → `public.note(id)`. Fixed by dropping the FK,
//! making callers pass schema in payload, and making job handlers use SchemaContext.
//!
//! Issue #414: New archives didn't get a default embedding set seeded. Fixed by adding
//! seed logic to `create_archive_tables()` in archives.rs.

use matric_core::{ArchiveRepository, CreateNoteRequest, JobRepository, JobType};
use matric_db::{Database, PgLinkRepository, PgNoteRepository, Vector};
use sqlx::Row;
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

/// Test 1: Verify that jobs can be queued for notes in non-default archives
/// without FK constraint violations (Issue #413).
#[tokio::test]
async fn test_archive_job_queue_no_fk_violation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_job_queue_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive schema using SchemaContext
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test note for job queueing".to_string(),
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

    // Queue a job for this note with schema in payload
    let payload = serde_json::json!({
        "schema": schema
    });

    let job_id = db
        .jobs
        .queue(Some(note_id), JobType::Embedding, 5, Some(payload))
        .await
        .expect("Failed to queue job - FK constraint may still exist");

    // Verify the job was queued successfully
    let job =
        sqlx::query("SELECT id, note_id, job_type, payload FROM public.job_queue WHERE id = $1")
            .bind(job_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to fetch queued job");

    let queued_note_id: Option<Uuid> = job.try_get("note_id").expect("Missing note_id");
    let job_payload: Option<serde_json::Value> = job.try_get("payload").expect("Missing payload");

    assert_eq!(
        queued_note_id,
        Some(note_id),
        "Job should reference the note"
    );
    assert!(
        job_payload.is_some(),
        "Job payload should contain schema information"
    );

    let payload_obj = job_payload.unwrap();
    assert_eq!(
        payload_obj["schema"].as_str().unwrap(),
        schema,
        "Payload should contain correct schema"
    );

    cleanup_archive(&db, &archive_name).await;
}

/// Test 2: End-to-end test that a note created in a non-default archive can have
/// embeddings stored (Issue #413 + #414 combined).
#[tokio::test]
async fn test_archive_note_embedding_pipeline_e2e() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_embedding_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test content for embedding generation".to_string(),
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
        .expect("Failed to create note");

    // Create test embeddings within schema context
    let test_chunks: Vec<(String, Vector)> = vec![
        ("First chunk".to_string(), Vector::from(vec![0.1f32; 768])),
        ("Second chunk".to_string(), Vector::from(vec![0.2f32; 768])),
    ];

    let pool_for_embeddings = db.pool.clone();
    let store_result = ctx
        .execute(move |tx| {
            Box::pin(async move {
                let embeddings = matric_db::PgEmbeddingRepository::new(pool_for_embeddings);
                embeddings
                    .store_tx(tx, note_id, test_chunks, "nomic-embed-text")
                    .await
            })
        })
        .await;

    assert!(
        store_result.is_ok(),
        "Storing embeddings should succeed: {:?}",
        store_result
    );

    // Verify embeddings exist in the archive schema (not public)
    let embedding_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.embedding WHERE note_id = $1",
        schema
    ))
    .bind(note_id)
    .fetch_one(&db.pool)
    .await
    .expect("Failed to count embeddings");

    assert_eq!(
        embedding_count, 2,
        "Should have 2 embeddings in archive schema"
    );

    // Verify embeddings do NOT exist in public schema
    let public_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.embedding WHERE note_id = $1")
            .bind(note_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count public embeddings");

    assert_eq!(public_count, 0, "Public schema should have no embeddings");

    // Use get_for_note_tx within schema context to verify retrieval
    let pool_for_retrieval = db.pool.clone();
    let retrieved: Vec<matric_core::Embedding> = ctx
        .query(move |tx| {
            Box::pin(async move {
                let embeddings = matric_db::PgEmbeddingRepository::new(pool_for_retrieval);
                embeddings.get_for_note_tx(tx, note_id).await
            })
        })
        .await
        .expect("Failed to retrieve embeddings");

    assert_eq!(
        retrieved.len(),
        2,
        "Should retrieve 2 embeddings via get_for_note_tx"
    );

    cleanup_archive(&db, &archive_name).await;
}

/// Test 3: Verify that newly created archives have a default embedding set (Issue #414).
#[tokio::test]
async fn test_archive_has_default_embedding_set() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_default_set_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Query the archive schema for the default embedding set
    let default_set: Option<(Uuid, String, String, bool, bool)> = sqlx::query_as(&format!(
        "SELECT id, slug, name, is_system, is_active FROM {}.embedding_set WHERE slug = 'default'",
        schema
    ))
    .fetch_optional(&db.pool)
    .await
    .expect("Failed to query default embedding set");

    assert!(
        default_set.is_some(),
        "Archive should have a default embedding set"
    );

    let (set_id, slug, name, is_system, is_active) = default_set.unwrap();
    assert_eq!(slug, "default", "Default set should have slug 'default'");
    assert_eq!(name, "Default", "Default set should have name 'Default'");
    assert!(is_system, "Default set should be marked as system");
    assert!(is_active, "Default set should be active");

    // Verify the get_default_embedding_set_id() function exists
    let function_result: Option<Uuid> =
        sqlx::query_scalar(&format!("SELECT {}.get_default_embedding_set_id()", schema))
            .fetch_one(&db.pool)
            .await
            .expect("Failed to call get_default_embedding_set_id()");

    assert!(function_result.is_some(), "Function should return a UUID");
    assert_eq!(
        function_result.unwrap(),
        set_id,
        "Function should return the default set ID"
    );

    cleanup_archive(&db, &archive_name).await;
}

/// Test 4: Verify that the schema field in job payloads is correctly set (Issue #413).
#[tokio::test]
async fn test_archive_job_payload_contains_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_job_payload_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test note for payload verification".to_string(),
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
        .expect("Failed to create note");

    // Queue a job with schema payload
    let payload = serde_json::json!({
        "schema": schema,
        "model": "test-model"
    });

    let job_id = db
        .jobs
        .queue(Some(note_id), JobType::Linking, 5, Some(payload.clone()))
        .await
        .expect("Failed to queue job");

    // Verify the job exists in the database with correct payload
    let job_row: (serde_json::Value, Uuid) =
        sqlx::query_as("SELECT payload, note_id FROM public.job_queue WHERE id = $1")
            .bind(job_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to fetch queued job");

    let (job_payload, queued_note_id) = job_row;
    assert_eq!(queued_note_id, note_id, "Job should reference correct note");
    assert_eq!(
        job_payload["schema"].as_str().unwrap(),
        schema,
        "Payload should contain correct schema"
    );
    assert_eq!(
        job_payload["model"].as_str().unwrap(),
        "test-model",
        "Payload should preserve other fields"
    );

    cleanup_archive(&db, &archive_name).await;
}

/// Test 5: Verify that link creation works within archive schemas (Issue #413 context).
#[tokio::test]
async fn test_archive_linking_in_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_linking_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create two notes in the archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");

    let notes1 = PgNoteRepository::new(db.pool.clone());
    let req1 = CreateNoteRequest {
        content: "First note for linking".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note1_id = ctx
        .execute(move |tx| Box::pin(async move { notes1.insert_tx(tx, req1).await }))
        .await
        .expect("Failed to create first note");

    let notes2 = PgNoteRepository::new(db.pool.clone());
    let req2 = CreateNoteRequest {
        content: "Second note for linking".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note2_id = ctx
        .execute(move |tx| Box::pin(async move { notes2.insert_tx(tx, req2).await }))
        .await
        .expect("Failed to create second note");

    // Use SchemaContext to create a link between them
    let links1 = PgLinkRepository::new(db.pool.clone());
    let link_id = ctx
        .execute(move |tx| {
            Box::pin(async move {
                links1
                    .create_tx(tx, note1_id, note2_id, "semantic", 0.85, None)
                    .await
            })
        })
        .await
        .expect("Failed to create link");

    // Use SchemaContext to verify outgoing links
    let links2 = PgLinkRepository::new(db.pool.clone());
    let outgoing = ctx
        .query(move |tx| Box::pin(async move { links2.get_outgoing_tx(tx, note1_id).await }))
        .await
        .expect("Failed to get outgoing links");

    assert_eq!(outgoing.len(), 1, "Should have 1 outgoing link");
    assert_eq!(outgoing[0].id, link_id, "Link ID should match");
    assert_eq!(
        outgoing[0].from_note_id, note1_id,
        "Link should be from note1"
    );
    assert_eq!(
        outgoing[0].to_note_id,
        Some(note2_id),
        "Link should be to note2"
    );
    assert_eq!(outgoing[0].kind, "semantic", "Link kind should be semantic");

    // Verify link exists in archive schema (not public)
    let link_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.link WHERE id = $1",
        schema
    ))
    .bind(link_id)
    .fetch_one(&db.pool)
    .await
    .expect("Failed to count links");

    assert_eq!(link_count, 1, "Link should exist in archive schema");

    // Verify link does NOT exist in public schema
    let public_link_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM public.link WHERE id = $1")
            .bind(link_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to count public links");

    assert_eq!(
        public_link_count, 0,
        "Link should not exist in public schema"
    );

    cleanup_archive(&db, &archive_name).await;
}

/// Test 6: Verify multiple archives can queue jobs independently (Issue #413 isolation).
#[tokio::test]
async fn test_multiple_archives_independent_job_queues() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_multi_job1_{}", Uuid::now_v7());
    let archive2_name = format!("test_multi_job2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes in both archives
    let ctx1 = db.for_schema(&schema1).expect("Failed to create ctx1");
    let ctx2 = db.for_schema(&schema2).expect("Failed to create ctx2");

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
        .expect("Failed to create note1");

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
        .expect("Failed to create note2");

    // Queue jobs for both notes with different schemas
    let payload1 = serde_json::json!({"schema": schema1});
    let payload2 = serde_json::json!({"schema": schema2});

    let job1_id = db
        .jobs
        .queue(Some(note1_id), JobType::TitleGeneration, 5, Some(payload1))
        .await
        .expect("Failed to queue job1");

    let job2_id = db
        .jobs
        .queue(Some(note2_id), JobType::AiRevision, 5, Some(payload2))
        .await
        .expect("Failed to queue job2");

    // Verify both jobs exist with correct payloads
    let job1: (serde_json::Value,) =
        sqlx::query_as("SELECT payload FROM public.job_queue WHERE id = $1")
            .bind(job1_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to fetch job1");

    let job2: (serde_json::Value,) =
        sqlx::query_as("SELECT payload FROM public.job_queue WHERE id = $1")
            .bind(job2_id)
            .fetch_one(&db.pool)
            .await
            .expect("Failed to fetch job2");

    assert_eq!(
        job1.0["schema"].as_str().unwrap(),
        schema1,
        "Job1 should have schema1"
    );
    assert_eq!(
        job2.0["schema"].as_str().unwrap(),
        schema2,
        "Job2 should have schema2"
    );

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

/// Test 7: Verify that jobs can be queued for notes without FK constraint (regression test).
#[tokio::test]
async fn test_job_queue_without_fk_constraint() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("test_no_fk_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = CreateNoteRequest {
        content: "Test note for FK constraint check".to_string(),
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
        .expect("Failed to create note");

    // Verify FK constraint does NOT exist on job_queue.note_id
    let fk_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM information_schema.table_constraints tc
            JOIN information_schema.constraint_column_usage ccu
                ON tc.constraint_name = ccu.constraint_name
            WHERE tc.table_schema = 'public'
                AND tc.table_name = 'job_queue'
                AND tc.constraint_type = 'FOREIGN KEY'
                AND ccu.column_name = 'note_id'
        )
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("Failed to check FK constraint");

    assert!(
        !fk_exists,
        "FK constraint on job_queue.note_id should be dropped"
    );

    // Queue job - should succeed even though note is in different schema
    let payload = serde_json::json!({"schema": schema});
    let result = db
        .jobs
        .queue(Some(note_id), JobType::Embedding, 5, Some(payload))
        .await;

    assert!(
        result.is_ok(),
        "Job queueing should succeed without FK constraint: {:?}",
        result
    );

    cleanup_archive(&db, &archive_name).await;
}
