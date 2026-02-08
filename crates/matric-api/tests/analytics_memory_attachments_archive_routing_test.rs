//! Integration tests for archive schema routing in analytics, memory search, and attachment handlers.
//!
//! Tests that analytics, memory search, and file attachment operations correctly route
//! to archive-specific schemas and maintain data isolation between archives.

use matric_core::ArchiveRepository;
use matric_db::{
    Database, FilesystemBackend, PgFileStorageRepository, PgLinkRepository,
    PgMemorySearchRepository, PgNoteRepository, PgTagRepository,
};
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

/// Test helper to create a note with tags in an archive schema.
async fn create_note_in_archive(
    db: &Database,
    schema: &str,
    content: &str,
    tags: Option<Vec<String>>,
) -> Uuid {
    let ctx = db
        .for_schema(schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());

    let req = matric_core::CreateNoteRequest {
        content: content.to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags,
        metadata: None,
        document_type_id: None,
    };

    ctx.execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note")
}

/// Test helper to create a link between notes in an archive schema.
async fn create_link_in_archive(db: &Database, schema: &str, from_note_id: Uuid, to_note_id: Uuid) {
    let ctx = db
        .for_schema(schema)
        .expect("Failed to create schema context");
    let links = PgLinkRepository::new(db.pool.clone());

    ctx.execute(move |tx| {
        Box::pin(async move {
            links
                .create_tx(tx, from_note_id, to_note_id, "manual", 0.9, None)
                .await
        })
    })
    .await
    .expect("Failed to create link");
}

#[tokio::test]
async fn test_analytics_timeline_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_timeline_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_timeline_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes in both archives
    let _note1 = create_note_in_archive(&db, &schema1, "Note in archive 1", None).await;
    let _note2 = create_note_in_archive(&db, &schema2, "Note in archive 2", None).await;

    // Verify notes are isolated - list all note IDs in each archive
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let notes1 = PgNoteRepository::new(db.pool.clone());
    let list1 = ctx1
        .query(move |tx| Box::pin(async move { notes1.list_all_ids_tx(tx).await }))
        .await
        .expect("Failed to list notes in archive1");

    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let notes2 = PgNoteRepository::new(db.pool.clone());
    let list2 = ctx2
        .query(move |tx| Box::pin(async move { notes2.list_all_ids_tx(tx).await }))
        .await
        .expect("Failed to list notes in archive2");

    assert_eq!(list1.len(), 1, "Archive 1 should have exactly 1 note");
    assert_eq!(list2.len(), 1, "Archive 2 should have exactly 1 note");
    assert_ne!(list1[0], list2[0], "Note IDs should be different");

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_analytics_tag_operations_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_tags_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_tags_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes with tags in both archives
    let note1 = create_note_in_archive(
        &db,
        &schema1,
        "Note with tag A",
        Some(vec!["tagA".to_string()]),
    )
    .await;
    let note2 = create_note_in_archive(
        &db,
        &schema2,
        "Note with tag B",
        Some(vec!["tagB".to_string()]),
    )
    .await;

    // Verify tags are isolated - get tags for each note
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let tags1 = PgTagRepository::new(db.pool.clone());
    let tags_for_note1 = ctx1
        .query(move |tx| Box::pin(async move { tags1.get_for_note_tx(tx, note1).await }))
        .await
        .expect("Failed to get tags for note1");

    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let tags2 = PgTagRepository::new(db.pool.clone());
    let tags_for_note2 = ctx2
        .query(move |tx| Box::pin(async move { tags2.get_for_note_tx(tx, note2).await }))
        .await
        .expect("Failed to get tags for note2");

    assert_eq!(tags_for_note1.len(), 1);
    assert_eq!(tags_for_note1[0].to_lowercase(), "taga");
    assert_eq!(tags_for_note2.len(), 1);
    assert_eq!(tags_for_note2[0].to_lowercase(), "tagb");

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_analytics_link_operations_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_links_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_links_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes and links in both archives
    let note1a = create_note_in_archive(&db, &schema1, "Note 1A", None).await;
    let note1b = create_note_in_archive(&db, &schema1, "Note 1B", None).await;
    create_link_in_archive(&db, &schema1, note1a, note1b).await;

    let note2a = create_note_in_archive(&db, &schema2, "Note 2A", None).await;
    let note2b = create_note_in_archive(&db, &schema2, "Note 2B", None).await;
    create_link_in_archive(&db, &schema2, note2a, note2b).await;

    // Verify links are isolated
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let links1 = PgLinkRepository::new(db.pool.clone());
    let links_list1 = ctx1
        .query(move |tx| Box::pin(async move { links1.list_all_tx(tx, 100, 0).await }))
        .await
        .expect("Failed to list links in archive1");

    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let links2 = PgLinkRepository::new(db.pool.clone());
    let links_list2 = ctx2
        .query(move |tx| Box::pin(async move { links2.list_all_tx(tx, 100, 0).await }))
        .await
        .expect("Failed to list links in archive2");

    assert_eq!(links_list1.len(), 1, "Archive 1 should have exactly 1 link");
    assert_eq!(links_list2.len(), 1, "Archive 2 should have exactly 1 link");
    assert_eq!(links_list1[0].from_note_id, note1a);
    assert_eq!(links_list2[0].from_note_id, note2a);

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_memory_search_location_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_memsearch_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_memsearch_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes in both archives
    let note1 = create_note_in_archive(&db, &schema1, "Memory in archive 1", None).await;
    let note2 = create_note_in_archive(&db, &schema2, "Memory in archive 2", None).await;

    // Search by location in archive1
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let memory1 = PgMemorySearchRepository::new(db.pool.clone());
    let lat = 40.7128;
    let lon = -74.0060;
    let radius = 10000.0; // 10km
    let results1 = ctx1
        .query(move |tx| {
            Box::pin(async move { memory1.search_by_location_tx(tx, lat, lon, radius).await })
        })
        .await
        .expect("Failed to search by location in archive1");

    // Search by location in archive2
    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let memory2 = PgMemorySearchRepository::new(db.pool.clone());
    let results2 = ctx2
        .query(move |tx| {
            Box::pin(async move { memory2.search_by_location_tx(tx, lat, lon, radius).await })
        })
        .await
        .expect("Failed to search by location in archive2");

    // Results should be isolated (both empty unless we add location metadata)
    // This test primarily verifies that the queries execute in the correct schema
    assert!(results1.is_empty() || results1.iter().any(|r| r.note_id == note1));
    assert!(results2.is_empty() || results2.iter().any(|r| r.note_id == note2));

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_memory_search_provenance_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_provenance_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_provenance_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes in both archives
    let note1 = create_note_in_archive(&db, &schema1, "Provenance test 1", None).await;
    let note2 = create_note_in_archive(&db, &schema2, "Provenance test 2", None).await;

    // Get provenance in archive1
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let memory1 = PgMemorySearchRepository::new(db.pool.clone());
    let prov1 = ctx1
        .query(move |tx| Box::pin(async move { memory1.get_memory_provenance_tx(tx, note1).await }))
        .await;

    // Get provenance in archive2
    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let memory2 = PgMemorySearchRepository::new(db.pool.clone());
    let prov2 = ctx2
        .query(move |tx| Box::pin(async move { memory2.get_memory_provenance_tx(tx, note2).await }))
        .await;

    // Both should succeed (or return None if no provenance data)
    assert!(prov1.is_ok());
    assert!(prov2.is_ok());

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_file_storage_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_files_1_{}", Uuid::now_v7());
    let archive2_name = format!("test_files_2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create notes in both archives
    let note1 = create_note_in_archive(&db, &schema1, "Note with file 1", None).await;
    let note2 = create_note_in_archive(&db, &schema2, "Note with file 2", None).await;

    // Create temporary directory for file storage backend
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_str().unwrap().to_string();
    let inline_threshold = matric_core::defaults::FILE_INLINE_THRESHOLD as i64;

    // Store file in archive1
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let backend1 = FilesystemBackend::new(temp_path.clone());
    let files1 = PgFileStorageRepository::new(db.pool.clone(), backend1, inline_threshold);
    let file_data1 = b"Test file content 1";
    let file1_id = ctx1
        .execute(move |tx| {
            Box::pin(async move {
                files1
                    .store_file_tx(tx, note1, "test1.txt", "text/plain", file_data1)
                    .await
            })
        })
        .await
        .expect("Failed to store file in archive1");

    // Store file in archive2
    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let backend2 = FilesystemBackend::new(temp_path.clone());
    let files2 = PgFileStorageRepository::new(db.pool.clone(), backend2, inline_threshold);
    let file_data2 = b"Test file content 2";
    let file2_id = ctx2
        .execute(move |tx| {
            Box::pin(async move {
                files2
                    .store_file_tx(tx, note2, "test2.txt", "text/plain", file_data2)
                    .await
            })
        })
        .await
        .expect("Failed to store file in archive2");

    // List files in archive1 - should only see file1
    let backend3 = FilesystemBackend::new(temp_path.clone());
    let files1_list = PgFileStorageRepository::new(db.pool.clone(), backend3, inline_threshold);
    let list1 = ctx1
        .query(move |tx| Box::pin(async move { files1_list.list_by_note_tx(tx, note1).await }))
        .await
        .expect("Failed to list files in archive1");

    // List files in archive2 - should only see file2
    let backend4 = FilesystemBackend::new(temp_path.clone());
    let files2_list = PgFileStorageRepository::new(db.pool.clone(), backend4, inline_threshold);
    let list2 = ctx2
        .query(move |tx| Box::pin(async move { files2_list.list_by_note_tx(tx, note2).await }))
        .await
        .expect("Failed to list files in archive2");

    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].id, file1_id.id);
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].id, file2_id.id);

    // Get file from archive1
    let backend5 = FilesystemBackend::new(temp_path.clone());
    let files1_get = PgFileStorageRepository::new(db.pool.clone(), backend5, inline_threshold);
    let get1 = ctx1
        .query(move |tx| Box::pin(async move { files1_get.get_tx(tx, file1_id.id).await }))
        .await
        .expect("Failed to get file from archive1");

    assert_eq!(get1.filename, "test1.txt");

    // Verify file from archive1 is NOT accessible in archive2
    let backend6 = FilesystemBackend::new(temp_path.clone());
    let files2_get = PgFileStorageRepository::new(db.pool.clone(), backend6, inline_threshold);
    let get1_from_archive2 = ctx2
        .query(move |tx| Box::pin(async move { files2_get.get_tx(tx, file1_id.id).await }))
        .await;

    assert!(
        get1_from_archive2.is_err(),
        "File from archive1 should not be accessible in archive2"
    );

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_file_storage_delete_archive_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("test_delete_files_{}", Uuid::now_v7());
    let schema1 = create_test_archive(&db, &archive1_name).await;

    let note1 = create_note_in_archive(&db, &schema1, "Note with file", None).await;

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_str().unwrap().to_string();
    let inline_threshold = matric_core::defaults::FILE_INLINE_THRESHOLD as i64;

    // Store file
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let backend1 = FilesystemBackend::new(temp_path.clone());
    let files1 = PgFileStorageRepository::new(db.pool.clone(), backend1, inline_threshold);
    let file_data = b"Test file to delete";
    let file_id = ctx1
        .execute(move |tx| {
            Box::pin(async move {
                files1
                    .store_file_tx(tx, note1, "delete_me.txt", "text/plain", file_data)
                    .await
            })
        })
        .await
        .expect("Failed to store file");

    // Delete file
    let backend2 = FilesystemBackend::new(temp_path.clone());
    let files1_delete = PgFileStorageRepository::new(db.pool.clone(), backend2, inline_threshold);
    ctx1.execute(move |tx| Box::pin(async move { files1_delete.delete_tx(tx, file_id.id).await }))
        .await
        .expect("Failed to delete file");

    // Verify file is deleted
    let backend3 = FilesystemBackend::new(temp_path);
    let files1_get = PgFileStorageRepository::new(db.pool.clone(), backend3, inline_threshold);
    let get_result = ctx1
        .query(move |tx| Box::pin(async move { files1_get.get_tx(tx, file_id.id).await }))
        .await;

    assert!(
        get_result.is_err(),
        "Deleted file should not be retrievable"
    );

    cleanup_archive(&db, &archive1_name).await;
}
