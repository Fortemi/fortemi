//! Archive schema repository implementation (Epic #441: Parallel Memory Archives).
//!
//! Provides schema-level data isolation by creating separate PostgreSQL schemas
//! for each archive, each with its own complete set of tables.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use matric_core::{new_v7, ArchiveInfo, ArchiveRepository, Error, Result};

/// PostgreSQL implementation of ArchiveRepository.
pub struct PgArchiveRepository {
    pool: Pool<Postgres>,
}

impl PgArchiveRepository {
    /// Create a new PgArchiveRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Generate a valid PostgreSQL schema name from an archive name.
    ///
    /// Replaces hyphens with underscores and ensures it starts with a letter.
    fn generate_schema_name(name: &str) -> String {
        let sanitized = name.replace(['-', ' '], "_").to_lowercase();
        format!("archive_{}", sanitized)
    }

    /// Create all necessary tables in the archive schema.
    ///
    /// Replicates the public schema structure but isolated within the archive.
    async fn create_archive_tables(&self, schema_name: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Note table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.note (
                id UUID PRIMARY KEY,
                collection_id UUID,
                format TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                starred BOOLEAN DEFAULT FALSE,
                archived BOOLEAN DEFAULT FALSE,
                soft_deleted BOOLEAN DEFAULT FALSE,
                soft_deleted_at TIMESTAMPTZ,
                last_accessed_at TIMESTAMPTZ,
                title TEXT,
                metadata JSONB DEFAULT '{{}}',
                chunk_metadata JSONB,
                document_type_id UUID
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Note original content table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.note_original (
                note_id UUID PRIMARY KEY REFERENCES {schema}.note(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                hash TEXT NOT NULL,
                user_created_at TIMESTAMPTZ,
                user_last_edited_at TIMESTAMPTZ
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Note revised content table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.note_revised (
                note_id UUID PRIMARY KEY REFERENCES {schema}.note(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                last_revision_id UUID,
                ai_metadata JSONB,
                ai_generated_at TIMESTAMPTZ,
                user_last_edited_at TIMESTAMPTZ,
                is_user_edited BOOLEAN DEFAULT FALSE,
                generation_count INTEGER DEFAULT 0,
                model TEXT
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Collection table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.collection (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                parent_id UUID,
                created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Tag table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.tag (
                name TEXT PRIMARY KEY,
                created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                soft_deleted BOOLEAN DEFAULT FALSE,
                soft_deleted_at TIMESTAMPTZ
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Note-tag association table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.note_tag (
                note_id UUID REFERENCES {schema}.note(id) ON DELETE CASCADE,
                tag_name TEXT REFERENCES {schema}.tag(name) ON DELETE CASCADE,
                PRIMARY KEY (note_id, tag_name)
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Embedding table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.embedding (
                id UUID PRIMARY KEY,
                note_id UUID REFERENCES {schema}.note(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                text TEXT NOT NULL,
                vector vector(768),
                model TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Link table
        sqlx::query(&format!(
            r#"
            CREATE TABLE {schema}.link (
                id UUID PRIMARY KEY,
                from_note_id UUID REFERENCES {schema}.note(id) ON DELETE CASCADE,
                to_note_id UUID,
                to_url TEXT,
                kind TEXT NOT NULL,
                score REAL DEFAULT 0.0,
                created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                snippet TEXT,
                metadata JSONB
            )
            "#,
            schema = schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        // Create indexes for performance (one at a time to avoid prepared statement issues)
        // Use IF NOT EXISTS for idempotency - PostgreSQL truncates identifiers to 63 chars,
        // which can cause name collisions with long schema names (especially UUID-based test schemas)
        let index_statements = vec![
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_coll ON {}.note(collection_id)", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_crtd ON {}.note(created_at_utc DESC)", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_updt ON {}.note(updated_at_utc DESC)", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_star ON {}.note(starred) WHERE starred = true", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_arch ON {}.note(archived) WHERE archived = true", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_note_sdel ON {}.note(soft_deleted) WHERE soft_deleted = false", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_emb_note ON {}.embedding(note_id)", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_link_frm ON {}.link(from_note_id)", schema_name.replace(".", "_"), schema_name),
            format!("CREATE INDEX IF NOT EXISTS idx_{}_link_to ON {}.link(to_note_id) WHERE to_note_id IS NOT NULL", schema_name.replace(".", "_"), schema_name),
        ];

        for stmt in index_statements {
            sqlx::query(&stmt)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }
        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}

#[async_trait]
impl ArchiveRepository for PgArchiveRepository {
    async fn create_archive_schema(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<ArchiveInfo> {
        let id = new_v7();
        let schema_name = Self::generate_schema_name(name);
        let now = Utc::now();

        // Create the archive registry entry first
        sqlx::query(
            r#"
            INSERT INTO archive_registry (id, name, schema_name, description, created_at, note_count, size_bytes, is_default)
            VALUES ($1, $2, $3, $4, $5, 0, 0, false)
            "#
        )
        .bind(id)
        .bind(name)
        .bind(&schema_name)
        .bind(description)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Create the PostgreSQL schema
        sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        // Create all tables in the new schema
        if let Err(e) = self.create_archive_tables(&schema_name).await {
            // Rollback: drop the schema and registry entry
            let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema_name))
                .execute(&self.pool)
                .await;
            let _ = sqlx::query("DELETE FROM archive_registry WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await;
            return Err(e);
        }

        Ok(ArchiveInfo {
            id,
            name: name.to_string(),
            schema_name,
            description: description.map(String::from),
            created_at: now,
            last_accessed: None,
            note_count: Some(0),
            size_bytes: Some(0),
            is_default: false,
        })
    }

    async fn drop_archive_schema(&self, name: &str) -> Result<()> {
        // Get the schema name
        let archive = self
            .get_archive_by_name(name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Archive not found: {}", name)))?;

        // Drop the PostgreSQL schema (CASCADE will drop all tables)
        sqlx::query(&format!(
            "DROP SCHEMA IF EXISTS {} CASCADE",
            archive.schema_name
        ))
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Remove the registry entry
        sqlx::query("DELETE FROM archive_registry WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn list_archive_schemas(&self) -> Result<Vec<ArchiveInfo>> {
        let archives = sqlx::query_as::<_, ArchiveInfo>(
            r#"
            SELECT id, name, schema_name, description, created_at, last_accessed,
                   note_count, size_bytes, is_default
            FROM archive_registry
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(archives)
    }

    async fn get_archive_by_name(&self, name: &str) -> Result<Option<ArchiveInfo>> {
        let archive = sqlx::query_as::<_, ArchiveInfo>(
            r#"
            SELECT id, name, schema_name, description, created_at, last_accessed,
                   note_count, size_bytes, is_default
            FROM archive_registry
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(archive)
    }

    async fn get_archive_by_id(&self, id: Uuid) -> Result<Option<ArchiveInfo>> {
        let archive = sqlx::query_as::<_, ArchiveInfo>(
            r#"
            SELECT id, name, schema_name, description, created_at, last_accessed,
                   note_count, size_bytes, is_default
            FROM archive_registry
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(archive)
    }

    async fn get_default_archive(&self) -> Result<Option<ArchiveInfo>> {
        let archive = sqlx::query_as::<_, ArchiveInfo>(
            r#"
            SELECT id, name, schema_name, description, created_at, last_accessed,
                   note_count, size_bytes, is_default
            FROM archive_registry
            WHERE is_default = true
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(archive)
    }

    async fn set_default_archive(&self, name: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Unset all defaults first
        sqlx::query("UPDATE archive_registry SET is_default = false WHERE is_default = true")
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Set the new default
        let result = sqlx::query("UPDATE archive_registry SET is_default = true WHERE name = $1")
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Archive not found: {}", name)));
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn update_archive_metadata(&self, name: &str, description: Option<&str>) -> Result<()> {
        let result = sqlx::query("UPDATE archive_registry SET description = $1 WHERE name = $2")
            .bind(description)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("Archive not found: {}", name)));
        }

        Ok(())
    }

    async fn update_archive_stats(&self, name: &str) -> Result<()> {
        // Get the schema name
        let archive = self
            .get_archive_by_name(name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Archive not found: {}", name)))?;

        // Count notes in the archive schema
        let note_count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {}.note WHERE soft_deleted = false",
            archive.schema_name
        ))
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Estimate size (simplified - could be more sophisticated)
        let size_bytes: i64 = sqlx::query_scalar(&format!(
            "SELECT pg_total_relation_size('{}.note'::regclass) +
                    pg_total_relation_size('{}.embedding'::regclass)",
            archive.schema_name, archive.schema_name
        ))
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Update registry
        sqlx::query(
            "UPDATE archive_registry SET note_count = $1, size_bytes = $2, last_accessed = NOW() WHERE name = $3"
        )
        .bind(note_count as i32)
        .bind(size_bytes)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }
}
