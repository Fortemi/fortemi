//! File storage repository with BLAKE3 deduplication and filesystem backend.
//!
//! This module provides content-addressable storage for file attachments with:
//! - BLAKE3 hashing for deduplication
//! - Filesystem backend with UUIDv7-based paths
//! - Reference counting for garbage collection
//! - Inline storage for small files (<threshold)
//! - Atomic write operations
//!
//! ## Example
//!
//! ```rust,ignore
//! use matric_db::file_storage::{FilesystemBackend, PgFileStorageRepository};
//!
//! let backend = FilesystemBackend::new("/var/matric/blobs");
//! let repo = PgFileStorageRepository::new(pool, backend, 10_485_760); // 10MB threshold
//!
//! // Store a file
//! let attachment = repo.store_file(note_id, "document.pdf", "application/pdf", &data).await?;
//!
//! // Download a file
//! let (data, content_type, filename) = repo.download_file(attachment.id).await?;
//! ```

use async_trait::async_trait;
use matric_core::{
    Attachment, AttachmentBlob, AttachmentStatus, AttachmentSummary, Error, ExtractionStrategy,
    Result,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};
use uuid::Uuid;

/// Storage backend trait for different storage implementations.
///
/// Allows abstracting over filesystem, S3, or other storage providers.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Write data to the specified path.
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;

    /// Read data from the specified path.
    async fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Delete data at the specified path.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Check if data exists at the specified path.
    async fn exists(&self, path: &str) -> Result<bool>;
}

/// Filesystem storage backend.
///
/// Stores files in a directory hierarchy based on UUIDv7 blob IDs.
/// Path format: `{base_path}/blobs/{first-2-hex}/{next-2-hex}/{uuid}.bin`
pub struct FilesystemBackend {
    base_path: PathBuf,
}

impl FilesystemBackend {
    /// Create a new filesystem backend with the given base directory.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path)
    }

    /// Validate that the storage backend can write, read, and delete files.
    ///
    /// Performs a full round-trip test at startup to catch filesystem issues
    /// (overlayfs quirks, permission errors, missing directories) early.
    pub async fn validate(&self) -> std::result::Result<(), String> {
        let test_dir = self.base_path.join("blobs/.health-check");
        let test_file = test_dir.join("test.bin");

        // Step 1: Create directory
        fs::create_dir_all(&test_dir)
            .await
            .map_err(|e| format!("create_dir_all({:?}): {}", test_dir, e))?;

        // Step 2: Write file
        let data = b"storage-health-check";
        fs::write(&test_file, data)
            .await
            .map_err(|e| format!("write({:?}): {}", test_file, e))?;

        // Step 3: Read file
        let read_data = fs::read(&test_file)
            .await
            .map_err(|e| format!("read({:?}): {}", test_file, e))?;
        if read_data != data {
            return Err("read-back mismatch".to_string());
        }

        // Step 4: Delete file and directory
        fs::remove_file(&test_file)
            .await
            .map_err(|e| format!("remove_file({:?}): {}", test_file, e))?;
        let _ = fs::remove_dir(&test_dir).await; // Best-effort cleanup

        Ok(())
    }
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let full_path = self.full_path(path);
        debug!(storage_path = %path, full_path = %full_path.display(), size = data.len(), "file_storage: write");

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                warn!(parent = %parent.display(), error = %e, "file_storage: create_dir_all failed");
                e
            })?;
        }

        // Atomic write: temp file + rename
        let temp_path = full_path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path).await.map_err(|e| {
            warn!(temp_path = %temp_path.display(), error = %e, "file_storage: File::create failed");
            e
        })?;
        file.write_all(data).await.map_err(|e| {
            warn!(error = %e, "file_storage: write_all failed");
            e
        })?;
        file.sync_all().await?;
        drop(file);

        fs::rename(&temp_path, &full_path).await.map_err(|e| {
            warn!(from = %temp_path.display(), to = %full_path.display(), error = %e, "file_storage: rename failed");
            e
        })?;

        // Set permissions to 0644 (rw-r--r--, no execute)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&full_path, std::fs::Permissions::from_mode(0o644)).await?;
        }

        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.full_path(path);
        Ok(fs::read(full_path).await?)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        if tokio::fs::try_exists(&full_path).await? {
            fs::remove_file(full_path).await?;
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.full_path(path);
        Ok(tokio::fs::try_exists(full_path).await?)
    }
}

/// Compute BLAKE3 hash of data with "blake3:" prefix.
///
/// Returns a string in the format: `blake3:{64-char-hex}`
pub fn compute_content_hash(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    format!("blake3:{}", hash.to_hex())
}

/// Generate storage path from UUID.
///
/// Path format: `blobs/{first-2-hex}/{next-2-hex}/{uuid}.bin`
///
/// Example: `blobs/01/94/01948f7e-8b2a-7c3d-9e4f-5a6b7c8d9e0f.bin`
pub fn generate_storage_path(uuid: &Uuid) -> String {
    let hex = uuid.as_hyphenated().to_string().replace('-', "");
    format!(
        "blobs/{}/{}/{}.bin",
        &hex[0..2],
        &hex[2..4],
        uuid.as_hyphenated()
    )
}

/// PostgreSQL file storage repository.
///
/// Handles file attachments with content-addressable storage, BLAKE3 deduplication,
/// and pluggable storage backends (filesystem, S3, etc.).
pub struct PgFileStorageRepository {
    pool: PgPool,
    backend: Box<dyn StorageBackend>,
    /// Retained for API backward compatibility but no longer used.
    /// All new uploads use filesystem storage regardless of size.
    #[allow(dead_code)]
    inline_threshold: i64,
}

impl PgFileStorageRepository {
    /// Create a new file storage repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    /// * `backend` - Storage backend (filesystem, S3, etc.)
    /// * `inline_threshold` - Files smaller than this (in bytes) are stored inline in the database
    pub fn new(
        pool: PgPool,
        backend: impl StorageBackend + 'static,
        inline_threshold: i64,
    ) -> Self {
        Self {
            pool,
            backend: Box::new(backend),
            inline_threshold,
        }
    }

    /// Store a file, deduplicating by content hash.
    ///
    /// If a blob with the same content hash already exists, it will be reused.
    /// Otherwise, a new blob is created and stored according to the size threshold:
    /// - Small files (<inline_threshold) are stored inline in the database
    /// - Large files (>=inline_threshold) are stored in the filesystem backend
    ///
    /// # Arguments
    ///
    /// * `note_id` - ID of the note to attach the file to
    /// * `filename` - Name of the file
    /// * `content_type` - MIME type of the file
    /// * `data` - File content
    ///
    /// # Returns
    ///
    /// The created `Attachment` record.
    pub async fn store_file(
        &self,
        note_id: Uuid,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<Attachment> {
        let content_hash = compute_content_hash(data);
        let size_bytes = data.len() as i64;

        // Check for existing blob with same hash
        let existing_blob: Option<AttachmentBlob> = sqlx::query(
            r#"SELECT id, content_hash, content_type, size_bytes,
                      storage_backend, storage_path, reference_count, created_at
               FROM attachment_blob WHERE content_hash = $1"#,
        )
        .bind(&content_hash)
        .fetch_optional(&self.pool)
        .await?
        .map(|row| attachment_blob_from_row(&row))
        .transpose()?;

        let blob_id = if let Some(blob) = existing_blob {
            // Reuse existing blob (deduplication)
            blob.id
        } else {
            // Create new blob - always use filesystem storage
            let blob_id = Uuid::now_v7();
            let path = generate_storage_path(&blob_id);
            self.backend.write(&path, data).await?;

            sqlx::query(
                r#"INSERT INTO attachment_blob
                   (id, content_hash, content_type, size_bytes, storage_backend, storage_path)
                   VALUES ($1, $2, $3, $4, 'filesystem', $5)"#,
            )
            .bind(blob_id)
            .bind(&content_hash)
            .bind(content_type)
            .bind(size_bytes)
            .bind(&path)
            .execute(&self.pool)
            .await?;

            blob_id
        };

        // Create attachment record
        let attachment_id = Uuid::now_v7();
        let row = sqlx::query(
            r#"INSERT INTO attachment
               (id, note_id, blob_id, filename, original_filename, status)
               VALUES ($1, $2, $3, $4, $4, 'uploaded')
               RETURNING id, note_id, blob_id, filename, original_filename,
                         document_type_id, status::TEXT, extraction_strategy::TEXT,
                         extracted_text, extracted_metadata,
                         has_preview, is_canonical_content,
                         detected_document_type_id, detection_confidence, detection_method,
                         created_at, updated_at"#,
        )
        .bind(attachment_id)
        .bind(note_id)
        .bind(blob_id)
        .bind(filename)
        .fetch_one(&self.pool)
        .await?;

        attachment_from_row(&row)
    }

    /// Download file content by attachment ID.
    ///
    /// Returns a tuple of (data, content_type, filename).
    ///
    /// # Errors
    ///
    /// Returns `Error::NotFound` if the attachment or blob doesn't exist.
    pub async fn download_file(&self, attachment_id: Uuid) -> Result<(Vec<u8>, String, String)> {
        let row = sqlx::query(
            r#"SELECT ab.content_type, ab.storage_backend, ab.storage_path, ab.data, a.filename
               FROM attachment a
               JOIN attachment_blob ab ON a.blob_id = ab.id
               WHERE a.id = $1"#,
        )
        .bind(attachment_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound("Attachment not found".into()))?;

        let storage_backend: String = row.get("storage_backend");
        let content_type: String = row.get("content_type");
        let filename: String = row.get("filename");

        let data = if storage_backend == "database" {
            row.get::<Option<Vec<u8>>, _>("data")
                .ok_or_else(|| Error::NotFound("Blob data missing".into()))?
        } else {
            let path: String = row
                .get::<Option<String>, _>("storage_path")
                .ok_or_else(|| Error::NotFound("Storage path missing".into()))?;
            self.backend.read(&path).await?
        };

        Ok((data, content_type, filename))
    }

    /// List all attachments for a note.
    ///
    /// Returns a list of `AttachmentSummary` objects ordered by display_order and created_at.
    pub async fn list_by_note(&self, note_id: Uuid) -> Result<Vec<AttachmentSummary>> {
        let rows = sqlx::query(
            r#"SELECT a.id, a.note_id, a.filename, ab.content_type, ab.size_bytes,
                      a.status::TEXT, dt.name as document_type_name,
                      ddt.name as detected_document_type_name, a.detection_confidence,
                      a.has_preview, a.is_canonical_content, a.created_at
               FROM attachment a
               JOIN attachment_blob ab ON a.blob_id = ab.id
               LEFT JOIN document_type dt ON a.document_type_id = dt.id
               LEFT JOIN document_type ddt ON a.detected_document_type_id = ddt.id
               WHERE a.note_id = $1
               ORDER BY a.display_order, a.created_at"#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            summaries.push(AttachmentSummary {
                id: row.get("id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                size_bytes: row.get("size_bytes"),
                status: parse_attachment_status(row.get("status")),
                document_type_name: row.get("document_type_name"),
                detected_document_type_name: row.get("detected_document_type_name"),
                detection_confidence: row.get("detection_confidence"),
                has_preview: row.get("has_preview"),
                is_canonical_content: row.get("is_canonical_content"),
                created_at: row.get("created_at"),
            });
        }

        Ok(summaries)
    }

    /// Delete an attachment.
    ///
    /// The attachment is removed, but the blob is retained (with decremented reference count)
    /// for potential reuse by other attachments. Orphaned blobs (reference_count = 0) can be
    /// cleaned up separately by a garbage collection process.
    pub async fn delete(&self, attachment_id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM attachment WHERE id = $1")
            .bind(attachment_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "Attachment {} not found",
                attachment_id
            )));
        }
        Ok(())
    }

    /// Get attachment by ID.
    pub async fn get(&self, attachment_id: Uuid) -> Result<Attachment> {
        let row = sqlx::query(
            r#"SELECT id, note_id, blob_id, filename, original_filename,
                      document_type_id, status::TEXT, extraction_strategy::TEXT,
                      extracted_text, extracted_metadata,
                      has_preview, is_canonical_content,
                      detected_document_type_id, detection_confidence, detection_method,
                      created_at, updated_at
               FROM attachment
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound("Attachment not found".into()))?;

        attachment_from_row(&row)
    }

    /// Update attachment metadata (status, extracted text, etc.).
    pub async fn update_status(
        &self,
        attachment_id: Uuid,
        status: AttachmentStatus,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET status = $2, processing_error = $3, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(status.to_string())
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update extracted content for an attachment.
    pub async fn update_extracted_content(
        &self,
        attachment_id: Uuid,
        extracted_text: Option<&str>,
        extracted_metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET extracted_text = $2, extracted_metadata = $3, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(extracted_text)
        .bind(extracted_metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Set the document type and extraction strategy for an attachment.
    pub async fn set_document_type(
        &self,
        attachment_id: Uuid,
        document_type_id: Uuid,
        extraction_strategy: Option<ExtractionStrategy>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET document_type_id = $2, extraction_strategy = $3::extraction_strategy, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(document_type_id)
        .bind(extraction_strategy.map(|s| s.to_string()))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Set only the extraction strategy (determined from MIME type, no document type lookup).
    pub async fn set_extraction_strategy(
        &self,
        attachment_id: Uuid,
        strategy: ExtractionStrategy,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET extraction_strategy = $2::extraction_strategy, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(strategy.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Set AI-detected document type with confidence score.
    ///
    /// If `auto_promote` is true and the detection is high-confidence,
    /// also sets `document_type_id` (user-visible assignment).
    pub async fn set_detected_document_type(
        &self,
        attachment_id: Uuid,
        detected_id: Uuid,
        confidence: f32,
        method: &str,
        auto_promote: bool,
    ) -> Result<()> {
        if auto_promote {
            sqlx::query(
                r#"UPDATE attachment
                   SET detected_document_type_id = $2,
                       detection_confidence = $3,
                       detection_method = $4,
                       document_type_id = $2,
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(attachment_id)
            .bind(detected_id)
            .bind(confidence)
            .bind(method)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"UPDATE attachment
                   SET detected_document_type_id = $2,
                       detection_confidence = $3,
                       detection_method = $4,
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(attachment_id)
            .bind(detected_id)
            .bind(confidence)
            .bind(method)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Clean up orphaned blobs (reference_count = 0) older than the specified age.
    ///
    /// Returns the number of blobs deleted.
    pub async fn cleanup_orphaned_blobs(&self, min_age_hours: i32) -> Result<i32> {
        let result = sqlx::query(
            r#"DELETE FROM attachment_blob
               WHERE reference_count = 0
               AND created_at < NOW() - ($1 || ' hours')::interval"#,
        )
        .bind(min_age_hours)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as i32)
    }
}

/// Transaction-aware variants for archive-scoped operations.
impl PgFileStorageRepository {
    /// Transaction-aware variant of list_by_note.
    pub async fn list_by_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<AttachmentSummary>> {
        let rows = sqlx::query(
            r#"SELECT a.id, a.note_id, a.filename, ab.content_type, ab.size_bytes,
                      a.status::TEXT, dt.name as document_type_name,
                      ddt.name as detected_document_type_name, a.detection_confidence,
                      a.has_preview, a.is_canonical_content, a.created_at
               FROM attachment a
               JOIN attachment_blob ab ON a.blob_id = ab.id
               LEFT JOIN document_type dt ON a.document_type_id = dt.id
               LEFT JOIN document_type ddt ON a.detected_document_type_id = ddt.id
               WHERE a.note_id = $1
               ORDER BY a.display_order, a.created_at"#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            summaries.push(AttachmentSummary {
                id: row.get("id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                size_bytes: row.get("size_bytes"),
                status: parse_attachment_status(row.get("status")),
                document_type_name: row.get("document_type_name"),
                detected_document_type_name: row.get("detected_document_type_name"),
                detection_confidence: row.get("detection_confidence"),
                has_preview: row.get("has_preview"),
                is_canonical_content: row.get("is_canonical_content"),
                created_at: row.get("created_at"),
            });
        }

        Ok(summaries)
    }

    /// Transaction-aware variant of store_file.
    pub async fn store_file_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<Attachment> {
        let content_hash = compute_content_hash(data);
        let size_bytes = data.len() as i64;

        // Check for existing blob with same hash
        let existing_blob: Option<AttachmentBlob> = sqlx::query(
            r#"SELECT id, content_hash, content_type, size_bytes,
                      storage_backend, storage_path, reference_count, created_at
               FROM attachment_blob WHERE content_hash = $1"#,
        )
        .bind(&content_hash)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| attachment_blob_from_row(&row))
        .transpose()?;

        let blob_id = if let Some(blob) = existing_blob {
            // Reuse existing blob (deduplication)
            blob.id
        } else {
            // Create new blob - always use filesystem storage
            let blob_id = Uuid::now_v7();
            let path = generate_storage_path(&blob_id);
            self.backend.write(&path, data).await?;

            sqlx::query(
                r#"INSERT INTO attachment_blob
                   (id, content_hash, content_type, size_bytes, storage_backend, storage_path)
                   VALUES ($1, $2, $3, $4, 'filesystem', $5)"#,
            )
            .bind(blob_id)
            .bind(&content_hash)
            .bind(content_type)
            .bind(size_bytes)
            .bind(&path)
            .execute(&mut **tx)
            .await?;

            blob_id
        };

        // Increment blob reference count (fixes #197 - phantom write)
        sqlx::query(
            "UPDATE attachment_blob SET reference_count = reference_count + 1 WHERE id = $1",
        )
        .bind(blob_id)
        .execute(&mut **tx)
        .await?;

        // Create attachment record
        let attachment_id = Uuid::now_v7();
        let row = sqlx::query(
            r#"INSERT INTO attachment
               (id, note_id, blob_id, filename, original_filename, status)
               VALUES ($1, $2, $3, $4, $4, 'uploaded')
               RETURNING id, note_id, blob_id, filename, original_filename,
                         document_type_id, status::TEXT, extraction_strategy::TEXT,
                         extracted_text, extracted_metadata,
                         has_preview, is_canonical_content,
                         detected_document_type_id, detection_confidence, detection_method,
                         created_at, updated_at"#,
        )
        .bind(attachment_id)
        .bind(note_id)
        .bind(blob_id)
        .bind(filename)
        .fetch_one(&mut **tx)
        .await?;

        attachment_from_row(&row)
    }

    /// Transaction-aware variant of get.
    pub async fn get_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        attachment_id: Uuid,
    ) -> Result<Attachment> {
        let row = sqlx::query(
            r#"SELECT id, note_id, blob_id, filename, original_filename,
                      document_type_id, status::TEXT, extraction_strategy::TEXT,
                      extracted_text, extracted_metadata,
                      has_preview, is_canonical_content,
                      detected_document_type_id, detection_confidence, detection_method,
                      created_at, updated_at
               FROM attachment
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Error::NotFound("Attachment not found".into()))?;

        attachment_from_row(&row)
    }

    /// Transaction-aware variant of download_file.
    pub async fn download_file_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        attachment_id: Uuid,
    ) -> Result<(Vec<u8>, String, String)> {
        let row = sqlx::query(
            r#"SELECT ab.content_type, ab.storage_backend, ab.storage_path, ab.data, a.filename
               FROM attachment a
               JOIN attachment_blob ab ON a.blob_id = ab.id
               WHERE a.id = $1"#,
        )
        .bind(attachment_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Error::NotFound("Attachment not found".into()))?;

        let storage_backend: String = row.get("storage_backend");
        let content_type: String = row.get("content_type");
        let filename: String = row.get("filename");

        let data = if storage_backend == "database" {
            row.get::<Option<Vec<u8>>, _>("data")
                .ok_or_else(|| Error::NotFound("Blob data missing".into()))?
        } else {
            let path: String = row
                .get::<Option<String>, _>("storage_path")
                .ok_or_else(|| Error::NotFound("Storage path missing".into()))?;
            self.backend.read(&path).await?
        };

        Ok((data, content_type, filename))
    }

    /// Transaction-aware variant of delete.
    pub async fn delete_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        attachment_id: Uuid,
    ) -> Result<()> {
        let result = sqlx::query("DELETE FROM attachment WHERE id = $1")
            .bind(attachment_id)
            .execute(&mut **tx)
            .await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!(
                "Attachment {} not found",
                attachment_id
            )));
        }
        Ok(())
    }

    /// Transaction-aware variant of set_extraction_strategy.
    pub async fn set_extraction_strategy_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        attachment_id: Uuid,
        strategy: ExtractionStrategy,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET extraction_strategy = $2::extraction_strategy, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(strategy.to_string())
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Transaction-aware variant of set_document_type.
    pub async fn set_document_type_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        attachment_id: Uuid,
        document_type_id: Uuid,
        extraction_strategy: Option<ExtractionStrategy>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE attachment
               SET document_type_id = $2, extraction_strategy = $3::extraction_strategy, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(attachment_id)
        .bind(document_type_id)
        .bind(extraction_strategy.map(|s| s.to_string()))
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

/// Parse attachment status from database string.
fn parse_attachment_status(s: &str) -> AttachmentStatus {
    match s {
        "uploaded" => AttachmentStatus::Uploaded,
        "queued" => AttachmentStatus::Queued,
        "processing" => AttachmentStatus::Processing,
        "completed" => AttachmentStatus::Completed,
        "failed" => AttachmentStatus::Failed,
        "quarantined" => AttachmentStatus::Quarantined,
        _ => AttachmentStatus::Uploaded,
    }
}

/// Parse extraction strategy from database string.
fn parse_extraction_strategy(s: Option<&str>) -> Option<ExtractionStrategy> {
    s.and_then(|s| match s {
        "text_native" => Some(ExtractionStrategy::TextNative),
        "pdf_text" => Some(ExtractionStrategy::PdfText),
        "pdf_ocr" => Some(ExtractionStrategy::PdfOcr),
        "vision" => Some(ExtractionStrategy::Vision),
        "audio_transcribe" => Some(ExtractionStrategy::AudioTranscribe),
        "video_multimodal" => Some(ExtractionStrategy::VideoMultimodal),
        "code_ast" => Some(ExtractionStrategy::CodeAst),
        "office_convert" => Some(ExtractionStrategy::OfficeConvert),
        "structured_extract" => Some(ExtractionStrategy::StructuredExtract),
        _ => None,
    })
}

/// Convert a database row to an AttachmentBlob.
fn attachment_blob_from_row(row: &sqlx::postgres::PgRow) -> Result<AttachmentBlob> {
    Ok(AttachmentBlob {
        id: row.get("id"),
        content_hash: row.get("content_hash"),
        content_type: row.get("content_type"),
        size_bytes: row.get("size_bytes"),
        storage_backend: row.get("storage_backend"),
        storage_path: row.get("storage_path"),
        reference_count: row.get("reference_count"),
        created_at: row.get("created_at"),
    })
}

/// Convert a database row to an Attachment.
fn attachment_from_row(row: &sqlx::postgres::PgRow) -> Result<Attachment> {
    Ok(Attachment {
        id: row.get("id"),
        note_id: row.get("note_id"),
        blob_id: row.get("blob_id"),
        filename: row.get("filename"),
        original_filename: row.get("original_filename"),
        document_type_id: row.get("document_type_id"),
        status: parse_attachment_status(row.get("status")),
        extraction_strategy: parse_extraction_strategy(row.get("extraction_strategy")),
        extracted_text: row.get("extracted_text"),
        extracted_metadata: row.get("extracted_metadata"),
        has_preview: row.get("has_preview"),
        is_canonical_content: row.get("is_canonical_content"),
        detected_document_type_id: row.get("detected_document_type_id"),
        detection_confidence: row
            .get::<Option<f64>, _>("detection_confidence")
            .map(|v| v as f32),
        detection_method: row.get("detection_method"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
