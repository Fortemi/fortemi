//! Archive schema repository implementation (Epic #441: Parallel Memory Archives).
//!
//! Provides schema-level data isolation by creating separate PostgreSQL schemas
//! for each archive, each with its own complete set of tables.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{new_v7, ArchiveInfo, ArchiveRepository, Error, Result};

/// Tables shared across all memories (deny list).
///
/// These tables contain global system data and are NOT cloned per-memory.
/// Any table in `public` NOT in this list is automatically cloned when
/// creating a new memory, ensuring zero-drift as migrations add tables.
const SHARED_TABLES: &[&str] = &[
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
];

/// Map PostgreSQL foreign key action code to SQL clause.
fn fk_action_sql(code: &str) -> &str {
    match code {
        "c" => "CASCADE",
        "n" => "SET NULL",
        "d" => "SET DEFAULT",
        "r" => "RESTRICT",
        _ => "NO ACTION", // 'a' = no action (default)
    }
}

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
    /// Sanitizes the input to only allow alphanumeric characters and underscores,
    /// preventing SQL injection in DDL statements that use format!().
    fn generate_schema_name(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| if c == '-' || c == ' ' { '_' } else { c })
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>()
            .to_lowercase();
        format!("archive_{}", sanitized)
    }

    /// Create all necessary tables in the archive schema by dynamically cloning
    /// from the public schema.
    ///
    /// Uses `CREATE TABLE ... (LIKE public.table INCLUDING ALL)` to copy table
    /// structure including columns, defaults, constraints (CHECK, NOT NULL),
    /// indexes, generated columns, and storage parameters. Foreign keys and
    /// triggers are cloned separately since LIKE does not copy them.
    ///
    /// This approach uses a deny list ([`SHARED_TABLES`]) instead of an allow
    /// list, so new tables added by migrations are automatically included —
    /// zero drift by design.
    async fn create_archive_tables(&self, schema_name: &str) -> Result<()> {
        // Step 1: Discover all per-memory tables from the public schema.
        // Any table NOT in SHARED_TABLES and NOT extension-owned is cloned.
        let shared: Vec<String> = SHARED_TABLES.iter().map(|s| s.to_string()).collect();
        let tables: Vec<String> = sqlx::query_scalar(
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
        .bind(&shared)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        if tables.is_empty() {
            return Err(Error::Internal(
                "No per-memory tables found in public schema".to_string(),
            ));
        }

        // Step 2: Discover foreign key constraints on per-memory tables.
        // Uses pg_constraint with unnest to properly handle composite FKs.
        let fk_rows = sqlx::query(
            r#"
            SELECT
                c.conname::text AS constraint_name,
                src.relname::text AS source_table,
                ref.relname::text AS reference_table,
                c.confdeltype::text AS delete_action,
                c.confupdtype::text AS update_action,
                array_agg(sa.attname::text ORDER BY u.ord) AS source_columns,
                array_agg(ra.attname::text ORDER BY u.ord) AS reference_columns
            FROM pg_constraint c
            JOIN pg_class src ON c.conrelid = src.oid
            JOIN pg_namespace sn ON src.relnamespace = sn.oid
            JOIN pg_class ref ON c.confrelid = ref.oid
            CROSS JOIN LATERAL (
                SELECT *
                FROM unnest(c.conkey, c.confkey)
                    WITH ORDINALITY AS t(src_num, ref_num, ord)
            ) u
            JOIN pg_attribute sa
                ON sa.attrelid = c.conrelid AND sa.attnum = u.src_num
            JOIN pg_attribute ra
                ON ra.attrelid = c.confrelid AND ra.attnum = u.ref_num
            WHERE c.contype = 'f'
                AND sn.nspname = 'public'
                AND src.relname = ANY($1::text[])
            GROUP BY c.conname, src.relname, ref.relname,
                     c.confdeltype, c.confupdtype
            ORDER BY src.relname, c.conname
            "#,
        )
        .bind(&tables)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Step 3: Discover triggers on per-memory tables.
        // LIKE never copies triggers, so we clone them from public.
        let trigger_rows = sqlx::query(
            r#"
            SELECT
                c.relname::text AS table_name,
                pg_get_triggerdef(t.oid) AS trigger_def
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = 'public'
                AND NOT t.tgisinternal
                AND c.relname = ANY($1::text[])
            ORDER BY c.relname, t.tgname
            "#,
        )
        .bind(&tables)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Step 4: Discover custom text search configurations in public schema.
        // Custom configs (e.g., matric_english) must exist in the memory's
        // schema for search_path resolution during full-text search queries.
        let ts_configs: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT c.cfgname::text
            FROM pg_ts_config c
            JOIN pg_namespace n ON n.oid = c.cfgnamespace
            WHERE n.nspname = 'public'
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Execute all DDL in a single transaction for atomicity.
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Step 5: Create tables using LIKE ... INCLUDING ALL.
        // Copies columns, defaults, CHECK/NOT NULL constraints, indexes,
        // generated columns, identity, and storage. FKs and triggers excluded.
        for table in &tables {
            sqlx::query(&format!(
                "CREATE TABLE {}.{} (LIKE public.{} INCLUDING ALL)",
                schema_name, table, table
            ))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Step 6: Add foreign key constraints.
        // Per-memory table references point to new schema; shared table
        // references point to public (e.g., note.document_type_id → public.document_type).
        for row in &fk_rows {
            let source_table: &str = row.get("source_table");
            let constraint_name: &str = row.get("constraint_name");
            let ref_table: &str = row.get("reference_table");
            let delete_action: &str = row.get("delete_action");
            let update_action: &str = row.get("update_action");
            let source_columns: Vec<String> = row.get("source_columns");
            let ref_columns: Vec<String> = row.get("reference_columns");

            let ref_schema = if tables.contains(&ref_table.to_string()) {
                schema_name
            } else {
                "public"
            };

            let fk_sql = format!(
                "ALTER TABLE {}.{} ADD CONSTRAINT {} \
                 FOREIGN KEY ({}) REFERENCES {}.{} ({}) \
                 ON DELETE {} ON UPDATE {}",
                schema_name,
                source_table,
                constraint_name,
                source_columns.join(", "),
                ref_schema,
                ref_table,
                ref_columns.join(", "),
                fk_action_sql(delete_action),
                fk_action_sql(update_action),
            );

            sqlx::query(&fk_sql)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        // Step 7: Clone triggers (LIKE never copies triggers).
        // Rewrite the table reference from public to the new schema.
        // Trigger function references stay in public — they resolve via
        // search_path at runtime, correctly operating on per-memory data.
        for row in &trigger_rows {
            let table_name: &str = row.get("table_name");
            let trigger_def: &str = row.get("trigger_def");

            let new_def = trigger_def.replace(
                &format!("ON public.{}", table_name),
                &format!("ON {}.{}", schema_name, table_name),
            );

            sqlx::query(&new_def)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        // Step 8: Clone text search configurations.
        for config in &ts_configs {
            sqlx::query(&format!(
                "CREATE TEXT SEARCH CONFIGURATION {}.{} (COPY = public.{})",
                schema_name, config, config
            ))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    /// Compute the current schema version (count of per-memory tables in public).
    ///
    /// Used to detect when an archive is outdated and needs auto-migration.
    async fn current_schema_version(&self) -> Result<i32> {
        let shared: Vec<String> = SHARED_TABLES.iter().map(|s| s.to_string()).collect();
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = 'public'
                AND c.relkind = 'r'
                AND c.relname != ALL($1::text[])
                AND NOT EXISTS (
                    SELECT 1 FROM pg_depend d
                    WHERE d.objid = c.oid AND d.deptype = 'e'
                )
            "#,
        )
        .bind(&shared)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(count as i32)
    }

    /// Synchronize an archive schema with the current public schema.
    ///
    /// Detects tables that exist in public but are missing from the archive,
    /// and creates them using `LIKE ... INCLUDING ALL` plus FKs and triggers.
    /// This handles archives created before recent migrations added new tables.
    pub async fn sync_archive_schema(&self, archive_name: &str) -> Result<()> {
        let archive = self
            .get_archive_by_name(archive_name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Archive not found: {}", archive_name)))?;

        let current_version = self.current_schema_version().await?;
        if archive.schema_version >= current_version {
            return Ok(()); // Already up to date
        }

        let schema_name = &archive.schema_name;

        // Find tables in public but missing from the archive
        let shared: Vec<String> = SHARED_TABLES.iter().map(|s| s.to_string()).collect();
        let missing_tables: Vec<String> = sqlx::query_scalar(
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
                AND NOT EXISTS (
                    SELECT 1 FROM pg_class ac
                    JOIN pg_namespace an ON ac.relnamespace = an.oid
                    WHERE an.nspname = $2
                        AND ac.relname = c.relname
                        AND ac.relkind = 'r'
                )
            ORDER BY c.relname
            "#,
        )
        .bind(&shared)
        .bind(schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        if missing_tables.is_empty() {
            // Tables match — just update the version
            sqlx::query("UPDATE archive_registry SET schema_version = $1 WHERE name = $2")
                .bind(current_version)
                .bind(archive_name)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
            return Ok(());
        }

        // Get all per-memory tables (needed for FK reference resolution)
        let all_per_memory: Vec<String> = sqlx::query_scalar(
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
        .bind(&shared)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Create missing tables
        for table in &missing_tables {
            sqlx::query(&format!(
                "CREATE TABLE {}.{} (LIKE public.{} INCLUDING ALL)",
                schema_name, table, table
            ))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Add FKs for the new tables only
        let fk_rows = sqlx::query(
            r#"
            SELECT
                c.conname::text AS constraint_name,
                src.relname::text AS source_table,
                ref.relname::text AS reference_table,
                c.confdeltype::text AS delete_action,
                c.confupdtype::text AS update_action,
                array_agg(sa.attname::text ORDER BY u.ord) AS source_columns,
                array_agg(ra.attname::text ORDER BY u.ord) AS reference_columns
            FROM pg_constraint c
            JOIN pg_class src ON c.conrelid = src.oid
            JOIN pg_namespace sn ON src.relnamespace = sn.oid
            JOIN pg_class ref ON c.confrelid = ref.oid
            CROSS JOIN LATERAL (
                SELECT *
                FROM unnest(c.conkey, c.confkey)
                    WITH ORDINALITY AS t(src_num, ref_num, ord)
            ) u
            JOIN pg_attribute sa
                ON sa.attrelid = c.conrelid AND sa.attnum = u.src_num
            JOIN pg_attribute ra
                ON ra.attrelid = c.confrelid AND ra.attnum = u.ref_num
            WHERE c.contype = 'f'
                AND sn.nspname = 'public'
                AND src.relname = ANY($1::text[])
            GROUP BY c.conname, src.relname, ref.relname,
                     c.confdeltype, c.confupdtype
            ORDER BY src.relname, c.conname
            "#,
        )
        .bind(&missing_tables)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;

        for row in &fk_rows {
            let source_table: &str = row.get("source_table");
            let constraint_name: &str = row.get("constraint_name");
            let ref_table: &str = row.get("reference_table");
            let delete_action: &str = row.get("delete_action");
            let update_action: &str = row.get("update_action");
            let source_columns: Vec<String> = row.get("source_columns");
            let ref_columns: Vec<String> = row.get("reference_columns");

            let ref_schema = if all_per_memory.contains(&ref_table.to_string()) {
                schema_name.as_str()
            } else {
                "public"
            };

            let fk_sql = format!(
                "ALTER TABLE {}.{} ADD CONSTRAINT {} \
                 FOREIGN KEY ({}) REFERENCES {}.{} ({}) \
                 ON DELETE {} ON UPDATE {}",
                schema_name,
                source_table,
                constraint_name,
                source_columns.join(", "),
                ref_schema,
                ref_table,
                ref_columns.join(", "),
                fk_action_sql(delete_action),
                fk_action_sql(update_action),
            );

            sqlx::query(&fk_sql)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        // Clone triggers for missing tables
        let trigger_rows = sqlx::query(
            r#"
            SELECT
                c.relname::text AS table_name,
                pg_get_triggerdef(t.oid) AS trigger_def
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = 'public'
                AND NOT t.tgisinternal
                AND c.relname = ANY($1::text[])
            ORDER BY c.relname, t.tgname
            "#,
        )
        .bind(&missing_tables)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;

        for row in &trigger_rows {
            let table_name: &str = row.get("table_name");
            let trigger_def: &str = row.get("trigger_def");

            let new_def = trigger_def.replace(
                &format!("ON public.{}", table_name),
                &format!("ON {}.{}", schema_name, table_name),
            );

            sqlx::query(&new_def)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
        }

        // Clone any missing text search configs
        let ts_configs: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT c.cfgname::text
            FROM pg_ts_config c
            JOIN pg_namespace n ON n.oid = c.cfgnamespace
            WHERE n.nspname = 'public'
                AND NOT EXISTS (
                    SELECT 1 FROM pg_ts_config ac
                    JOIN pg_namespace an ON an.oid = ac.cfgnamespace
                    WHERE an.nspname = $1
                        AND ac.cfgname = c.cfgname
                )
            "#,
        )
        .bind(schema_name)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;

        for config in &ts_configs {
            sqlx::query(&format!(
                "CREATE TEXT SEARCH CONFIGURATION {}.{} (COPY = public.{})",
                schema_name, config, config
            ))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Update version to current
        sqlx::query("UPDATE archive_registry SET schema_version = $1 WHERE name = $2")
            .bind(current_version)
            .bind(archive_name)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

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

        // Compute current schema version (count of per-memory tables)
        let schema_version = self.current_schema_version().await?;

        // Create the archive registry entry first
        sqlx::query(
            r#"
            INSERT INTO archive_registry (id, name, schema_name, description, created_at, note_count, size_bytes, is_default, schema_version)
            VALUES ($1, $2, $3, $4, $5, 0, 0, false, $6)
            "#
        )
        .bind(id)
        .bind(name)
        .bind(&schema_name)
        .bind(description)
        .bind(now)
        .bind(schema_version)
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
            schema_version,
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
                   note_count, size_bytes, is_default, schema_version
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
                   note_count, size_bytes, is_default, schema_version
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
                   note_count, size_bytes, is_default, schema_version
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
                   note_count, size_bytes, is_default, schema_version
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

    async fn sync_archive_schema(&self, name: &str) -> Result<()> {
        // Delegates to the inherent method on PgArchiveRepository
        PgArchiveRepository::sync_archive_schema(self, name).await
    }

    async fn clone_archive_schema(
        &self,
        source_name: &str,
        new_name: &str,
        description: Option<&str>,
    ) -> Result<ArchiveInfo> {
        // Verify source exists
        let source = self
            .get_archive_by_name(source_name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Source archive not found: {}", source_name)))?;

        // Check new name doesn't already exist
        if self.get_archive_by_name(new_name).await?.is_some() {
            return Err(Error::Internal(format!(
                "Archive '{}' already exists",
                new_name
            )));
        }

        // Create the new archive with empty tables
        let new_archive = self.create_archive_schema(new_name, description).await?;

        // Discover per-memory tables in the source schema
        let tables: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT c.relname::text
            FROM pg_class c
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = $1
                AND c.relkind = 'r'
            ORDER BY c.relname
            "#,
        )
        .bind(&source.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Copy data in a single transaction with FK checks disabled
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Disable FK checks for bulk copy (session_replication_role = 'replica'
        // tells PostgreSQL to skip trigger-based FK enforcement)
        sqlx::query("SET LOCAL session_replication_role = 'replica'")
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        for table in &tables {
            sqlx::query(&format!(
                "INSERT INTO {}.{} SELECT * FROM {}.{}",
                new_archive.schema_name, table, source.schema_name, table
            ))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        // Re-enable FK checks
        sqlx::query("SET LOCAL session_replication_role = 'origin'")
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;

        // Update stats on the new archive
        let _ = self.update_archive_stats(new_name).await;

        Ok(new_archive)
    }
}
