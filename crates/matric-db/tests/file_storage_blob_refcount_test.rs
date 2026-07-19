//! Tests for blob reference counting and cleanup during attachment deletion.
//!
//! Verifies issue #353: delete_attachment must NOT destroy shared blobs when
//! other attachments still reference them.
//!
//! UAT references: UAT-2B-018, UAT-2B-019

use matric_core::AttachmentScanStatus;
use matric_db::{FilesystemBackend, PgFileStorageRepository};
use sqlx::PgPool;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

fn setup_file_storage(pool: PgPool, temp_dir: &TempDir) -> PgFileStorageRepository {
    let backend = FilesystemBackend::new(temp_dir.path());
    PgFileStorageRepository::new(pool, backend, 10_485_760)
}

/// Create a test note and return its ID.
async fn create_test_note(pool: &PgPool) -> Uuid {
    let note_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
           VALUES ($1, 'markdown', 'test', NOW(), NOW())"#,
    )
    .bind(note_id)
    .execute(pool)
    .await
    .expect("Failed to create test note");
    note_id
}

/// Get the blob reference count for a given blob_id.
async fn get_blob_refcount(pool: &PgPool, blob_id: Uuid) -> Option<i32> {
    sqlx::query_scalar::<_, i32>("SELECT reference_count FROM attachment_blob WHERE id = $1")
        .bind(blob_id)
        .fetch_optional(pool)
        .await
        .expect("Failed to query blob refcount")
}

/// Check if a blob row exists.
async fn blob_exists(pool: &PgPool, blob_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM attachment_blob WHERE id = $1)")
        .bind(blob_id)
        .fetch_one(pool)
        .await
        .expect("Failed to check blob existence")
}

/// UAT-2B-019: Delete one attachment sharing a blob — other attachment still works.
///
/// When two attachments share the same blob via content deduplication,
/// deleting one attachment must NOT destroy the blob. The remaining
/// attachment must still be downloadable.
#[tokio::test]
async fn test_shared_blob_survives_sibling_deletion() {
    let pool = setup_test_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_storage = setup_file_storage(pool.clone(), &temp_dir);

    let note_id = create_test_note(&pool).await;

    let unique = Uuid::now_v7().to_string();
    let file_data = format!("shared content for dedup test {}", unique);
    let file_data = file_data.as_bytes();

    // Upload same content twice → should produce two attachments sharing one blob
    let attachment1 = file_storage
        .store_file(note_id, "file1.txt", "text/plain", file_data)
        .await
        .expect("Failed to store first attachment");

    let attachment2 = file_storage
        .store_file(note_id, "file2.txt", "text/plain", file_data)
        .await
        .expect("Failed to store second attachment");

    // Both attachments should share the same blob
    assert_eq!(
        attachment1.blob_id, attachment2.blob_id,
        "Attachments with identical content should share the same blob"
    );
    let shared_blob_id = attachment1.blob_id;

    // Blob reference count should be 2
    let refcount = get_blob_refcount(&pool, shared_blob_id)
        .await
        .expect("Blob should exist");
    assert_eq!(refcount, 2, "Shared blob should have reference_count = 2");

    // Delete the second attachment
    file_storage
        .delete(attachment2.id)
        .await
        .expect("Failed to delete second attachment");

    // Blob should still exist with reference_count = 1
    assert!(
        blob_exists(&pool, shared_blob_id).await,
        "Blob must NOT be deleted when other attachments still reference it"
    );
    let refcount = get_blob_refcount(&pool, shared_blob_id)
        .await
        .expect("Blob should still exist");
    assert_eq!(
        refcount, 1,
        "Blob reference_count should be 1 after deleting one of two attachments"
    );

    file_storage
        .set_scan_verdict(
            attachment1.id,
            AttachmentScanStatus::Clean,
            Some("test-scanner"),
            None,
            None,
            Some("scanner_clean"),
            Some(&matric_db::compute_content_hash(file_data)),
        )
        .await
        .expect("Failed to mark attachment clean");

    // Original attachment should still be downloadable
    let (data, content_type, filename) = file_storage
        .download_file(attachment1.id)
        .await
        .expect("First attachment should still be downloadable after sibling deletion");

    assert_eq!(data, file_data, "Downloaded data should match original");
    assert_eq!(content_type, "text/plain");
    assert_eq!(filename, "file1.txt");
}

/// UAT-2B-018: Delete last attachment referencing a blob → blob is cleaned up.
///
/// When the last attachment referencing a blob is deleted, both the blob row
/// and the physical file should be removed.
#[tokio::test]
async fn test_orphaned_blob_cleaned_up_on_last_delete() {
    let pool = setup_test_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_storage = setup_file_storage(pool.clone(), &temp_dir);

    let note_id = create_test_note(&pool).await;

    let unique = Uuid::now_v7().to_string();
    let file_data = format!("unique content for cleanup test {}", unique);
    let file_data = file_data.as_bytes();

    // Upload a file
    let attachment = file_storage
        .store_file(note_id, "cleanup.txt", "text/plain", file_data)
        .await
        .expect("Failed to store attachment");

    let blob_id = attachment.blob_id;

    // Blob should exist with refcount 1
    assert_eq!(
        get_blob_refcount(&pool, blob_id).await,
        Some(1),
        "Blob should have reference_count = 1"
    );

    // Get storage path for physical file check
    let storage_path: Option<String> =
        sqlx::query_scalar("SELECT storage_path FROM attachment_blob WHERE id = $1")
            .bind(blob_id)
            .fetch_optional(&pool)
            .await
            .expect("Failed to query storage path");

    // Delete the only attachment
    file_storage
        .delete(attachment.id)
        .await
        .expect("Failed to delete attachment");

    // Blob row should be deleted (no remaining references)
    assert!(
        !blob_exists(&pool, blob_id).await,
        "Orphaned blob row should be deleted when last reference is removed"
    );

    // Physical file should also be deleted
    if let Some(path) = storage_path {
        let full_path = temp_dir.path().join("blobs").join(&path);
        assert!(
            !full_path.exists(),
            "Physical blob file should be deleted when last reference is removed"
        );
    }
}

#[tokio::test]
async fn attachment_download_requires_allowed_scan_verdict() {
    let pool = setup_test_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_storage = setup_file_storage(pool.clone(), &temp_dir);
    let note_id = create_test_note(&pool).await;
    let data = format!("bounded clean attachment {note_id}");
    let data = data.as_bytes();

    let attachment = file_storage
        .store_file(note_id, "scan.txt", "text/plain", data)
        .await
        .expect("Failed to store attachment");
    let content_hash = matric_db::compute_content_hash(data);
    assert_eq!(attachment.virus_scan_status, AttachmentScanStatus::Pending);
    assert!(matches!(
        file_storage
            .download_file(attachment.id)
            .await
            .expect_err("pending attachment must not be readable"),
        matric_core::Error::Forbidden(_)
    ));

    let derived = {
        let mut tx = pool.begin().await.expect("Failed to begin derived file tx");
        let derived = file_storage
            .store_derived_attachment_tx(
                &mut tx,
                note_id,
                attachment.id,
                "embedded.txt",
                "text/plain",
                b"untrusted embedded bytes",
                "archive_entry",
            )
            .await
            .expect("Failed to store derived attachment");
        tx.commit().await.expect("Failed to commit derived file");
        derived
    };
    assert!(matches!(
        file_storage
            .download_file(derived.id)
            .await
            .expect_err("untrusted derived attachment must remain scan-gated"),
        matric_core::Error::Forbidden(_)
    ));

    assert!(matches!(
        file_storage
            .set_scan_verdict(
                attachment.id,
                AttachmentScanStatus::Clean,
                Some("test-scanner"),
                Some("1.0"),
                Some("42"),
                Some("scanner_clean"),
                Some("mismatched-hash"),
            )
            .await
            .expect_err("verdict for a different blob must be rejected"),
        matric_core::Error::InvalidInput(_)
    ));

    file_storage
        .set_scan_verdict(
            attachment.id,
            AttachmentScanStatus::Infected,
            Some("test-scanner"),
            Some("1.0"),
            Some("42"),
            Some("malware_detected"),
            Some(&content_hash),
        )
        .await
        .expect("Failed to persist infected verdict");
    assert!(matches!(
        file_storage
            .download_file(attachment.id)
            .await
            .expect_err("infected attachment must not be readable"),
        matric_core::Error::Forbidden(_)
    ));
    assert_eq!(
        file_storage
            .get(attachment.id)
            .await
            .expect("Failed to reload attachment")
            .status,
        matric_core::AttachmentStatus::Quarantined
    );

    file_storage
        .set_scan_verdict(
            attachment.id,
            AttachmentScanStatus::Clean,
            Some("test-scanner"),
            Some("1.0"),
            Some("42"),
            Some("scanner_clean"),
            Some(&content_hash),
        )
        .await
        .expect("Failed to persist clean verdict");
    let (downloaded, _, _) = file_storage
        .download_file(attachment.id)
        .await
        .expect("clean attachment should be readable");
    assert_eq!(downloaded, data);

    sqlx::query("UPDATE attachment SET virus_scan_blob_hash = 'stale-hash' WHERE id = $1")
        .bind(attachment.id)
        .execute(&pool)
        .await
        .expect("Failed to simulate a stale verdict");
    assert!(matches!(
        file_storage
            .download_file(attachment.id)
            .await
            .expect_err("a clean verdict for different bytes must not authorize access"),
        matric_core::Error::Forbidden(_)
    ));
}

/// Verify that store_file_tx does NOT double-increment the reference count.
///
/// The trigger update_blob_refcount() handles increment on INSERT.
/// store_file_tx previously had an additional explicit UPDATE that
/// caused double-counting (issue #353).
#[tokio::test]
async fn test_store_file_tx_correct_refcount() {
    let pool = setup_test_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_storage = setup_file_storage(pool.clone(), &temp_dir);

    let note_id = create_test_note(&pool).await;

    let unique = Uuid::now_v7().to_string();
    let file_data = format!("tx refcount test content {}", unique);
    let file_data = file_data.as_bytes();

    // Use store_file_tx via a transaction
    let mut tx = pool.begin().await.expect("Failed to begin transaction");
    let attachment = file_storage
        .store_file_tx(&mut tx, note_id, "tx-test.txt", "text/plain", file_data)
        .await
        .expect("Failed to store file in transaction");
    tx.commit().await.expect("Failed to commit transaction");

    let blob_id = attachment.blob_id;

    // Reference count should be exactly 1, not 2
    let refcount = get_blob_refcount(&pool, blob_id)
        .await
        .expect("Blob should exist");
    assert_eq!(
        refcount, 1,
        "store_file_tx should result in reference_count = 1, not 2 (no double-increment)"
    );
}

/// Verify deduplication + correct refcount across tx and non-tx store methods.
#[tokio::test]
async fn test_mixed_store_methods_refcount_consistency() {
    let pool = setup_test_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_storage = setup_file_storage(pool.clone(), &temp_dir);

    let note_id = create_test_note(&pool).await;

    let unique = Uuid::now_v7().to_string();
    let file_data = format!("mixed method dedup content {}", unique);
    let file_data = file_data.as_bytes();

    // Store via non-tx method
    let att1 = file_storage
        .store_file(note_id, "nontx.txt", "text/plain", file_data)
        .await
        .expect("Failed to store via non-tx");

    // Store same content via tx method
    let mut tx = pool.begin().await.expect("Failed to begin transaction");
    let att2 = file_storage
        .store_file_tx(&mut tx, note_id, "tx.txt", "text/plain", file_data)
        .await
        .expect("Failed to store via tx");
    tx.commit().await.expect("Failed to commit transaction");

    // Both should share the same blob
    assert_eq!(
        att1.blob_id, att2.blob_id,
        "Deduplication should work across store methods"
    );

    // Reference count should be exactly 2
    let refcount = get_blob_refcount(&pool, att1.blob_id)
        .await
        .expect("Blob should exist");
    assert_eq!(
        refcount, 2,
        "Two attachments sharing a blob should have reference_count = 2"
    );

    // Delete first, verify blob survives
    file_storage
        .delete(att1.id)
        .await
        .expect("Failed to delete first attachment");
    assert_eq!(
        get_blob_refcount(&pool, att2.blob_id).await,
        Some(1),
        "Refcount should be 1 after deleting one attachment"
    );

    // Delete second, verify blob is cleaned up
    let mut tx = pool.begin().await.expect("Failed to begin transaction");
    file_storage
        .delete_tx(&mut tx, att2.id)
        .await
        .expect("Failed to delete second attachment via tx");
    tx.commit().await.expect("Failed to commit transaction");

    assert!(
        !blob_exists(&pool, att2.blob_id).await,
        "Blob should be cleaned up after all references removed"
    );
}
