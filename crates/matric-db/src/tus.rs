//! Tus resumable upload repository.
//!
//! Manages the `tus_upload` table for tracking in-progress resumable uploads.
//! Each upload session has a staging file on disk; when complete, the file is
//! handed off to the standard `store_file_tx` attachment pipeline.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::models::TusUpload;

/// Repository for tus upload session CRUD operations.
#[derive(Clone)]
pub struct PgTusRepository {
    pool: PgPool,
}

impl PgTusRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new tus upload session.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        filename: &str,
        content_type: &str,
        total_size: i64,
        storage_path: &str,
        expires_at: DateTime<Utc>,
        metadata: serde_json::Value,
    ) -> Result<TusUpload, sqlx::Error> {
        sqlx::query_as::<_, TusUpload>(
            r#"
            INSERT INTO tus_upload (note_id, filename, content_type, total_size, storage_path, expires_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, note_id, filename, content_type, total_size,
                      current_offset, storage_path, metadata,
                      created_at, updated_at, expires_at
            "#,
        )
        .bind(note_id)
        .bind(filename)
        .bind(content_type)
        .bind(total_size)
        .bind(storage_path)
        .bind(expires_at)
        .bind(metadata)
        .fetch_one(&mut **tx)
        .await
    }

    /// Get a tus upload by ID.
    pub async fn get(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        upload_id: Uuid,
    ) -> Result<Option<TusUpload>, sqlx::Error> {
        sqlx::query_as::<_, TusUpload>(
            r#"
            SELECT id, note_id, filename, content_type, total_size,
                   current_offset, storage_path, metadata,
                   created_at, updated_at, expires_at
            FROM tus_upload
            WHERE id = $1
            "#,
        )
        .bind(upload_id)
        .fetch_optional(&mut **tx)
        .await
    }

    /// Get a tus upload by ID (non-transactional, for HEAD requests).
    #[allow(dead_code)]
    pub async fn get_direct(&self, upload_id: Uuid) -> Result<Option<TusUpload>, sqlx::Error> {
        sqlx::query_as::<_, TusUpload>(
            r#"
            SELECT id, note_id, filename, content_type, total_size,
                   current_offset, storage_path, metadata,
                   created_at, updated_at, expires_at
            FROM tus_upload
            WHERE id = $1
            "#,
        )
        .bind(upload_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Update the current offset after a chunk is appended.
    /// Returns the updated upload row.
    pub async fn update_offset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        upload_id: Uuid,
        new_offset: i64,
    ) -> Result<TusUpload, sqlx::Error> {
        sqlx::query_as::<_, TusUpload>(
            r#"
            UPDATE tus_upload
            SET current_offset = $2
            WHERE id = $1
            RETURNING id, note_id, filename, content_type, total_size,
                      current_offset, storage_path, metadata,
                      created_at, updated_at, expires_at
            "#,
        )
        .bind(upload_id)
        .bind(new_offset)
        .fetch_one(&mut **tx)
        .await
    }

    /// Delete a tus upload session (cancel or after finalization).
    /// Returns the deleted row (for staging file cleanup).
    pub async fn delete(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        upload_id: Uuid,
    ) -> Result<Option<TusUpload>, sqlx::Error> {
        sqlx::query_as::<_, TusUpload>(
            r#"
            DELETE FROM tus_upload
            WHERE id = $1
            RETURNING id, note_id, filename, content_type, total_size,
                      current_offset, storage_path, metadata,
                      created_at, updated_at, expires_at
            "#,
        )
        .bind(upload_id)
        .fetch_optional(&mut **tx)
        .await
    }

    /// Clean up expired uploads. Returns (id, storage_path) pairs for disk cleanup.
    pub async fn cleanup_expired(&self) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        let rows = sqlx::query(
            "DELETE FROM tus_upload WHERE expires_at < NOW() RETURNING id, storage_path",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("storage_path")))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Compile-time trait bound checks
    // -------------------------------------------------------------------------

    #[test]
    fn pg_tus_repository_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PgTusRepository>();
    }

    /// `PgTusRepository` must be `Clone` so it can be cheaply distributed to
    /// Axum handler tasks (which require `Clone`-able state).
    #[test]
    fn pg_tus_repository_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<PgTusRepository>();
    }

    /// `TusUpload` must implement `Serialize + Deserialize` so it can be
    /// round-tripped through JSON (API responses, job payloads, backups).
    #[test]
    fn tus_upload_is_serialize_deserialize() {
        fn assert_serde<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
        assert_serde::<TusUpload>();
    }

    /// `TusUpload` must be `Clone` (derived alongside Serialize/Deserialize).
    #[test]
    fn tus_upload_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<TusUpload>();
    }

    /// `TusUpload` must be `Send + Sync` so it can be held across `.await`
    /// points in async handler tasks.
    #[test]
    fn tus_upload_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TusUpload>();
    }
}
