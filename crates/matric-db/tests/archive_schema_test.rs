//! Test suite for archive schema management (Epic #441: Parallel Memory Archives).
//!
//! Tests the creation, listing, and management of isolated PostgreSQL schemas
//! for parallel memory archives.

use chrono::Utc;
use matric_db::{ArchiveRepository, Database};
use sqlx::PgPool;
use uuid::Uuid;

/// Helper to create a test database pool.
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_create_archive_schema() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a unique archive name
    let archive_name = format!("test-archive-{}", Utc::now().timestamp_millis());
    let schema_name = format!("archive_{}", archive_name.replace('-', "_"));

    // Test: Create a new archive schema
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("Test archive for parallel memory"))
        .await
        .expect("Failed to create archive schema");

    // Verify archive info
    assert_eq!(archive.name, archive_name);
    assert_eq!(archive.schema_name, schema_name);
    assert_eq!(
        archive.description,
        Some("Test archive for parallel memory".to_string())
    );
    assert_eq!(archive.note_count, Some(0));
    assert_eq!(archive.size_bytes, Some(0));
    assert!(!archive.is_default);

    // Cleanup: Drop the schema
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_list_archive_schemas() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create two test archives
    let archive1_name = format!("test-list-archive1-{}", Utc::now().timestamp_millis());
    let archive2_name = format!("test-list-archive2-{}", Utc::now().timestamp_millis());

    let archive1 = db
        .archives
        .create_archive_schema(&archive1_name, Some("First test archive"))
        .await
        .expect("Failed to create first archive");

    let archive2 = db
        .archives
        .create_archive_schema(&archive2_name, Some("Second test archive"))
        .await
        .expect("Failed to create second archive");

    // Test: List all archives
    let archives = db
        .archives
        .list_archive_schemas()
        .await
        .expect("Failed to list archives");

    // Verify both archives are in the list
    let archive1_found = archives.iter().any(|a| a.id == archive1.id);
    let archive2_found = archives.iter().any(|a| a.id == archive2.id);

    assert!(archive1_found, "First archive should be in the list");
    assert!(archive2_found, "Second archive should be in the list");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive1_name)
        .await
        .expect("Failed to drop first test archive");
    db.archives
        .drop_archive_schema(&archive2_name)
        .await
        .expect("Failed to drop second test archive");
}

#[tokio::test]
async fn test_get_archive_by_name() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-get-archive-{}", Utc::now().timestamp_millis());
    let description = "Archive for get test";

    // Create archive
    let created_archive = db
        .archives
        .create_archive_schema(&archive_name, Some(description))
        .await
        .expect("Failed to create archive");

    // Test: Get archive by name
    let retrieved_archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");

    // Verify retrieved data matches created data
    assert_eq!(retrieved_archive.id, created_archive.id);
    assert_eq!(retrieved_archive.name, archive_name);
    assert_eq!(retrieved_archive.description, Some(description.to_string()));

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_default_archive_uniqueness() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive1_name = format!("test-default1-{}", Utc::now().timestamp_millis());
    let archive2_name = format!("test-default2-{}", Utc::now().timestamp_millis());

    // Create first archive and mark as default
    db.archives
        .create_archive_schema(&archive1_name, Some("First default test"))
        .await
        .expect("Failed to create first archive");

    db.archives
        .set_default_archive(&archive1_name)
        .await
        .expect("Failed to set first archive as default");

    // Create second archive
    db.archives
        .create_archive_schema(&archive2_name, Some("Second default test"))
        .await
        .expect("Failed to create second archive");

    // Test: Setting second archive as default should unset the first
    db.archives
        .set_default_archive(&archive2_name)
        .await
        .expect("Failed to set second archive as default");

    // Verify only one is default
    let archive1 = db
        .archives
        .get_archive_by_name(&archive1_name)
        .await
        .expect("Failed to get first archive")
        .expect("First archive should exist");

    let archive2 = db
        .archives
        .get_archive_by_name(&archive2_name)
        .await
        .expect("Failed to get second archive")
        .expect("Second archive should exist");

    assert!(!archive1.is_default, "First archive should not be default");
    assert!(archive2.is_default, "Second archive should be default");

    // Reset default before cleanup (can't drop default archive)
    db.archives
        .set_default_archive("public")
        .await
        .expect("Failed to reset default");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive1_name)
        .await
        .expect("Failed to drop first archive");
    db.archives
        .drop_archive_schema(&archive2_name)
        .await
        .expect("Failed to drop second archive");
}

#[tokio::test]
async fn test_archive_schema_isolation() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-isolation-{}", Utc::now().timestamp_millis());
    let schema_name = format!("archive_{}", archive_name.replace('-', "_"));

    // Create archive
    db.archives
        .create_archive_schema(&archive_name, Some("Isolation test"))
        .await
        .expect("Failed to create archive");

    // Test: Verify schema exists in PostgreSQL
    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check schema existence");

    assert!(schema_exists, "Archive schema should exist in PostgreSQL");

    // Test: Verify schema has a note table
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = $1 AND table_name = 'note'
        )",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check table existence");

    assert!(table_exists, "Archive schema should have a note table");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");

    // Verify schema was dropped
    let schema_exists_after: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = $1)",
    )
    .bind(&schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check schema existence after drop");

    assert!(
        !schema_exists_after,
        "Archive schema should be dropped from PostgreSQL"
    );
}

#[tokio::test]
async fn test_duplicate_archive_name_fails() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-duplicate-{}", Utc::now().timestamp_millis());

    // Create first archive
    db.archives
        .create_archive_schema(&archive_name, Some("First archive"))
        .await
        .expect("Failed to create first archive");

    // Test: Attempt to create duplicate should fail
    let result = db
        .archives
        .create_archive_schema(&archive_name, Some("Duplicate archive"))
        .await;

    assert!(result.is_err(), "Creating duplicate archive should fail");

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

#[tokio::test]
async fn test_update_archive_metadata() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-update-{}", Utc::now().timestamp_millis());

    // Create archive
    db.archives
        .create_archive_schema(&archive_name, Some("Original description"))
        .await
        .expect("Failed to create archive");

    // Test: Update archive metadata
    db.archives
        .update_archive_metadata(&archive_name, Some("Updated description"))
        .await
        .expect("Failed to update archive metadata");

    // Verify update
    let archive = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");

    assert_eq!(
        archive.description,
        Some("Updated description".to_string()),
        "Description should be updated"
    );

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop test archive");
}

/// Schema drift detection test (Issue #180).
///
/// Verifies that a newly created archive schema contains all per-memory tables
/// from public, with matching columns and types. This catches drift between
/// the public schema (evolved by migrations) and the dynamic cloning logic.
#[tokio::test]
async fn test_schema_drift_detection() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("drift-test-{}", Uuid::now_v7());

    // Create an archive — the dynamic cloning should mirror public exactly
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("Drift detection test"))
        .await
        .expect("Failed to create archive schema");

    let archive_schema = &archive.schema_name;

    // Shared tables that should NOT be in the archive schema (deny list).
    let shared_tables: Vec<String> = vec![
        "_sqlx_migrations",
        "api_key",
        "archive_registry",
        "document_type",
        "embedding_config",
        "file_upload_audit",
        "job_history",
        "job_queue",
        "oauth_authorization_code",
        "oauth_client",
        "oauth_token",
        "pke_public_keys",
        "user_config",
        "user_metadata_label",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    // Get all per-memory tables from public schema (excluding shared + extension-owned)
    let public_tables: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT c.relname::text
        FROM pg_class c
        JOIN pg_namespace n ON c.relnamespace = n.oid
        WHERE n.nspname = 'public'
            AND c.relkind = 'r'
            AND c.relname != ALL($1::text[])
            AND NOT EXISTS (
                SELECT 1 FROM pg_depend d
                WHERE d.objid = c.oid AND d.deptype = 'e'
            )
        ORDER BY c.relname
        "#,
    )
    .bind(&shared_tables)
    .fetch_all(&pool)
    .await
    .expect("Failed to query public tables");

    // Get all tables in the archive schema
    let archive_tables: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT c.relname::text
        FROM pg_class c
        JOIN pg_namespace n ON c.relnamespace = n.oid
        WHERE n.nspname = $1
            AND c.relkind = 'r'
        ORDER BY c.relname
        "#,
    )
    .bind(archive_schema)
    .fetch_all(&pool)
    .await
    .expect("Failed to query archive tables");

    // Verify all public per-memory tables exist in the archive
    for table in &public_tables {
        assert!(
            archive_tables.contains(table),
            "Table '{}' exists in public but NOT in archive schema '{}'. \
             This indicates schema drift — the SHARED_TABLES deny list may \
             need updating, or the dynamic cloning missed this table.",
            table,
            archive_schema
        );
    }

    // Verify no extra tables in archive that aren't in public
    for table in &archive_tables {
        assert!(
            public_tables.contains(table),
            "Table '{}' exists in archive schema '{}' but NOT in public. \
             This should not happen with LIKE cloning.",
            table,
            archive_schema
        );
    }

    // Verify column-level parity for each table
    for table in &public_tables {
        let public_cols: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT a.attname::text, format_type(a.atttypid, a.atttypmod)::text
            FROM pg_attribute a
            JOIN pg_class c ON a.attrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = 'public'
                AND c.relname = $1
                AND a.attnum > 0
                AND NOT a.attisdropped
            ORDER BY a.attnum
            "#,
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .expect("Failed to query public columns");

        let archive_cols: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT a.attname::text, format_type(a.atttypid, a.atttypmod)::text
            FROM pg_attribute a
            JOIN pg_class c ON a.attrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = $1
                AND c.relname = $2
                AND a.attnum > 0
                AND NOT a.attisdropped
            ORDER BY a.attnum
            "#,
        )
        .bind(archive_schema)
        .bind(table)
        .fetch_all(&pool)
        .await
        .expect("Failed to query archive columns");

        assert_eq!(
            public_cols, archive_cols,
            "Column mismatch for table '{}': public and archive '{}' differ",
            table, archive_schema
        );
    }

    // Verify shared tables are NOT in the archive schema
    for shared in &shared_tables {
        assert!(
            !archive_tables.contains(shared),
            "Shared table '{}' should NOT be in archive schema '{}'. \
             It belongs only in the public schema.",
            shared,
            archive_schema
        );
    }

    // Cleanup
    db.archives
        .drop_archive_schema(&archive_name)
        .await
        .expect("Failed to drop drift test archive");
}

// =============================================================================
// NEW TESTS FOR MULTI-MEMORY CAPABILITIES
// =============================================================================

#[tokio::test]
async fn test_clone_archive_schema() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Use UUID-based names for parallel test safety
    let source_name = format!("test-clone-source-{}", Uuid::now_v7());
    let clone_name = format!("test-clone-target-{}", Uuid::now_v7());

    // Create source archive
    let source = db
        .archives
        .create_archive_schema(&source_name, Some("Source archive for cloning"))
        .await
        .expect("Failed to create source archive");

    // Insert a test note into the source archive using direct SQL
    let note_id = Uuid::now_v7();
    let now = Utc::now();

    sqlx::query(&format!(
        "INSERT INTO {}.note (id, format, source, created_at_utc, updated_at_utc, metadata)
         VALUES ($1, 'markdown', 'test-source', $2, $2, '{{}}')",
        source.schema_name
    ))
    .bind(note_id)
    .bind(now)
    .execute(&pool)
    .await
    .expect("Failed to insert test note");

    sqlx::query(&format!(
        "INSERT INTO {}.note_original (note_id, content, hash)
         VALUES ($1, $2, 'testhash')",
        source.schema_name
    ))
    .bind(note_id)
    .bind("Test note content for cloning")
    .execute(&pool)
    .await
    .expect("Failed to insert test note content");

    // Clone the archive
    let clone = db
        .archives
        .clone_archive_schema(&source_name, &clone_name, Some("Cloned archive"))
        .await
        .expect("Failed to clone archive");

    // Verify clone has different ID but same schema structure
    assert_ne!(clone.id, source.id, "Clone should have different ID");
    assert_eq!(clone.name, clone_name);
    assert_eq!(clone.description, Some("Cloned archive".to_string()));

    // Verify cloned data exists
    let note_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.note WHERE id = $1",
        clone.schema_name
    ))
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count notes in clone");

    assert_eq!(note_count, 1, "Cloned archive should contain the test note");

    // Verify cloned note content
    let cloned_content: String = sqlx::query_scalar(&format!(
        "SELECT content FROM {}.note_original WHERE note_id = $1",
        clone.schema_name
    ))
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch cloned note content");

    assert_eq!(
        cloned_content, "Test note content for cloning",
        "Cloned note should have same content"
    );

    // Cleanup
    let _ = db.archives.drop_archive_schema(&source_name).await;
    let _ = db.archives.drop_archive_schema(&clone_name).await;
}

#[tokio::test]
async fn test_clone_nonexistent_source() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let clone_name = format!("test-clone-fail-{}", Uuid::now_v7());

    // Attempt to clone from non-existent source
    let result = db
        .archives
        .clone_archive_schema(
            "nonexistent-source-archive",
            &clone_name,
            Some("Should fail"),
        )
        .await;

    assert!(
        result.is_err(),
        "Cloning from non-existent source should fail"
    );

    // Verify clone was not created
    let clone_exists = db
        .archives
        .get_archive_by_name(&clone_name)
        .await
        .expect("Failed to check clone existence");

    assert!(clone_exists.is_none(), "Failed clone should not exist");
}

#[tokio::test]
async fn test_clone_duplicate_target() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let source_name = format!("test-clone-dup-source-{}", Uuid::now_v7());
    let target_name = format!("test-clone-dup-target-{}", Uuid::now_v7());

    // Create source archive
    db.archives
        .create_archive_schema(&source_name, Some("Source"))
        .await
        .expect("Failed to create source");

    // Create target archive (duplicate target)
    db.archives
        .create_archive_schema(&target_name, Some("Existing target"))
        .await
        .expect("Failed to create target");

    // Attempt to clone to existing name
    let result = db
        .archives
        .clone_archive_schema(&source_name, &target_name, Some("Should fail"))
        .await;

    assert!(
        result.is_err(),
        "Cloning to existing archive name should fail"
    );

    // Cleanup
    let _ = db.archives.drop_archive_schema(&source_name).await;
    let _ = db.archives.drop_archive_schema(&target_name).await;
}

#[tokio::test]
async fn test_sync_archive_schema() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-sync-{}", Uuid::now_v7());

    // Create archive
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("Sync test"))
        .await
        .expect("Failed to create archive");

    // Verify schema_version is set
    assert!(
        archive.schema_version > 0,
        "New archive should have schema_version set"
    );

    // Call sync_archive_schema (should be no-op since it's current)
    let result = db.archives.sync_archive_schema(&archive_name).await;

    assert!(
        result.is_ok(),
        "Sync should succeed for current archive: {:?}",
        result
    );

    // Verify archive still exists and version unchanged
    let synced = db
        .archives
        .get_archive_by_name(&archive_name)
        .await
        .expect("Failed to get synced archive")
        .expect("Archive should still exist");

    assert_eq!(
        synced.schema_version, archive.schema_version,
        "Schema version should be unchanged"
    );

    // Cleanup
    let _ = db.archives.drop_archive_schema(&archive_name).await;
}

#[tokio::test]
async fn test_schema_version_tracking() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive1_name = format!("test-version1-{}", Uuid::now_v7());
    let archive2_name = format!("test-version2-{}", Uuid::now_v7());

    // Create first archive
    let archive1 = db
        .archives
        .create_archive_schema(&archive1_name, Some("Version test 1"))
        .await
        .expect("Failed to create first archive");

    // Verify schema_version > 0
    assert!(
        archive1.schema_version > 0,
        "Archive should have positive schema_version"
    );

    // Create second archive
    let archive2 = db
        .archives
        .create_archive_schema(&archive2_name, Some("Version test 2"))
        .await
        .expect("Failed to create second archive");

    // Both should have same schema_version (based on current public schema)
    assert_eq!(
        archive1.schema_version, archive2.schema_version,
        "Archives created at same time should have same schema_version"
    );

    // Cleanup
    let _ = db.archives.drop_archive_schema(&archive1_name).await;
    let _ = db.archives.drop_archive_schema(&archive2_name).await;
}

/// Test that new archives have a default embedding set (Issue #414).
///
/// Without a default embedding set, store_tx() fails when trying to create
/// embeddings, returning "No default embedding set found".
#[tokio::test]
async fn test_new_archive_has_default_embedding_set() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-embedding-set-{}", Uuid::now_v7());

    // Create a new archive
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("Embedding set test"))
        .await
        .expect("Failed to create archive");

    // Verify get_default_embedding_set_id() function exists in the archive schema
    let function_exists: bool = sqlx::query_scalar(&format!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM pg_proc p
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE n.nspname = $1
                AND p.proname = 'get_default_embedding_set_id'
        )
        "#
    ))
    .bind(&archive.schema_name)
    .fetch_one(&pool)
    .await
    .expect("Failed to check function existence");

    assert!(
        function_exists,
        "Archive schema should have get_default_embedding_set_id() function"
    );

    // Verify the function returns a valid UUID (the default embedding set ID)
    let default_set_id: Option<Uuid> = sqlx::query_scalar(&format!(
        "SELECT {}.get_default_embedding_set_id()",
        archive.schema_name
    ))
    .fetch_one(&pool)
    .await
    .expect("Failed to call get_default_embedding_set_id()");

    assert!(
        default_set_id.is_some(),
        "Archive should have a default embedding set"
    );

    // Verify the default embedding set row exists with expected properties
    let (slug, name, is_system, is_active): (String, String, bool, bool) =
        sqlx::query_as(&format!(
            "SELECT slug, name, is_system, is_active FROM {}.embedding_set WHERE id = $1",
            archive.schema_name
        ))
        .bind(default_set_id.unwrap())
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch default embedding set");

    assert_eq!(slug, "default", "Default set should have slug 'default'");
    assert_eq!(name, "Default", "Default set should have name 'Default'");
    assert!(is_system, "Default set should be marked as system");
    assert!(is_active, "Default set should be active");

    // Cleanup
    let _ = db.archives.drop_archive_schema(&archive_name).await;
}

/// End-to-end test: Create archive and store embeddings (Issue #414 regression test).
/// End-to-end test: Create archive and store embeddings (Issue #414 regression test).
///
/// Before the fix, creating a note with embeddings in a new archive would fail with
/// "No default embedding set found" because the default embedding set wasn't seeded.
#[tokio::test]
async fn test_archive_can_store_embeddings() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    let archive_name = format!("test-embed-e2e-{}", Uuid::now_v7());

    // Create a new archive
    let archive = db
        .archives
        .create_archive_schema(&archive_name, Some("E2E embedding test"))
        .await
        .expect("Failed to create archive");

    // Create a test note in the archive
    let note_id = Uuid::now_v7();
    let now = Utc::now();

    sqlx::query(&format!(
        "INSERT INTO {}.note (id, format, source, created_at_utc, updated_at_utc, metadata) VALUES ($1, $2, $3, $4, $4, $5)",
        archive.schema_name
    ))
    .bind(note_id)
    .bind("markdown")
    .bind("test-embedding-source")
    .bind(now)
    .bind(serde_json::json!({}))
    .execute(&pool)
    .await
    .expect("Failed to insert test note");

    sqlx::query(&format!(
        "INSERT INTO {}.note_original (note_id, content, hash) VALUES ($1, $2, $3)",
        archive.schema_name
    ))
    .bind(note_id)
    .bind("Test content for embedding generation")
    .bind("testhash")
    .execute(&pool)
    .await
    .expect("Failed to insert test note content");

    // Test: Store embeddings via store_tx (this would fail before the fix)
    use matric_db::Vector;
    let chunks: Vec<(String, Vector)> = vec![
        ("Test chunk 1".to_string(), Vector::from(vec![0.1; 768])),
        ("Test chunk 2".to_string(), Vector::from(vec![0.2; 768])),
    ];

    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    // Use schema-qualified transaction
    sqlx::query(&format!(
        "SET LOCAL search_path TO {}, public",
        archive.schema_name
    ))
    .execute(&mut *tx)
    .await
    .expect("Failed to set search path");

    let result = db
        .embeddings
        .store_tx(&mut tx, note_id, chunks, "nomic-embed-text")
        .await;

    assert!(
        result.is_ok(),
        "Storing embeddings should succeed with default embedding set: {:?}",
        result
    );

    tx.commit().await.expect("Failed to commit transaction");

    // Verify embeddings were actually stored
    let embedding_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.embedding WHERE note_id = $1",
        archive.schema_name
    ))
    .bind(note_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count embeddings");

    assert_eq!(
        embedding_count, 2,
        "Should have 2 embeddings (one per chunk)"
    );

    // Cleanup
    let _ = db.archives.drop_archive_schema(&archive_name).await;
}
