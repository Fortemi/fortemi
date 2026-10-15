//! Frozen v2026.10.5 archive constructor from b5f9b0056dc1228f049c374758915f5b8d7f2a63.
//! DDL, shared-table list, FK/trigger cloning and seeds are the release code path.
//! Only imports, visibility and unused methods were adjusted for this fixture.
#![allow(dead_code)]
//! Archive schema repository implementation (Epic #441: Parallel Memory Archives).
//!
//! Provides schema-level data isolation by creating separate PostgreSQL schemas
//! for each archive, each with its own complete set of tables.

use chrono::Utc;
use sqlx::{Pool, Postgres, Row};
use std::fmt;
use uuid::Uuid;

use matric_core::{new_v7, ArchiveInfo, Error, Result};
use matric_db::vector_index;

fn archive_not_found_error(name: &str) -> Error {
    Error::NotFound(format!(
        "Archive not found; name_len={}",
        name.chars().count()
    ))
}

fn source_archive_not_found_error(name: &str) -> Error {
    Error::NotFound(format!(
        "Source archive not found; name_len={}",
        name.chars().count()
    ))
}

fn archive_already_exists_error(name: &str) -> Error {
    Error::InvalidInput(format!(
        "Archive already exists; name_len={}",
        name.chars().count()
    ))
}

/// FTS generated columns and functional indexes that use custom text search
/// configurations from the `public` schema (e.g., `matric_english`, `matric_simple`).
///
/// PostgreSQL's `LIKE ... INCLUDING ALL` uses `pg_get_expr()` to deparse generated
/// column expressions, which strips the schema qualifier for objects in the default
/// search_path. This causes FTS tokenization failures in non-default schemas when
/// multi-byte UTF-8 characters are processed (Issue #412).
///
/// After each `LIKE ... INCLUDING ALL`, we must drop and recreate these columns/indexes
/// with explicitly schema-qualified config names (`public.matric_english`, etc.).
///
/// Built-in configs like `english` (in `pg_catalog`) are unaffected since `pg_catalog`
/// is always in the search_path.
struct FtsFixDefinition {
    table: &'static str,
    /// Generated column to drop and recreate (if any).
    generated_column: Option<FtsGeneratedColumn>,
    /// Functional indexes to drop and recreate.
    indexes: &'static [FtsIndex],
}

struct FtsGeneratedColumn {
    column_name: &'static str,
    expression: &'static str,
    gin_index_name: &'static str,
}

struct FtsIndex {
    index_name: &'static str,
    definition: &'static str,
}

struct MissingColumn {
    table_name: String,
    column_name: String,
    column_type: String,
    is_not_null: bool,
    column_default: Option<String>,
}

impl fmt::Debug for MissingColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MissingColumn")
            .field("table_name_len", &self.table_name.len())
            .field("column_name_len", &self.column_name.len())
            .field("column_type_len", &self.column_type.len())
            .field("is_not_null", &self.is_not_null)
            .field(
                "column_default_len",
                &self.column_default.as_ref().map(String::len),
            )
            .finish()
    }
}

const FTS_FIX_DEFINITIONS: &[FtsFixDefinition] = &[
    FtsFixDefinition {
        table: "note_revised_current",
        generated_column: Some(FtsGeneratedColumn {
            column_name: "tsv",
            expression: "to_tsvector('public.matric_english', content)",
            gin_index_name: "idx_note_revised_tsv",
        }),
        indexes: &[FtsIndex {
            index_name: "idx_note_revised_tsv_simple",
            definition: "USING gin (to_tsvector('public.matric_simple', content))",
        }],
    },
    FtsFixDefinition {
        table: "note_original",
        generated_column: None,
        indexes: &[FtsIndex {
            index_name: "idx_note_original_fts",
            definition: "USING gin (to_tsvector('public.matric_english', content))",
        }],
    },
    FtsFixDefinition {
        table: "note",
        generated_column: None,
        indexes: &[FtsIndex {
            index_name: "idx_note_title_tsv_simple",
            definition: "USING gin (to_tsvector('public.matric_simple', COALESCE(title, '')))",
        }],
    },
    FtsFixDefinition {
        table: "skos_concept_label",
        generated_column: None,
        indexes: &[FtsIndex {
            index_name: "idx_skos_label_tsv_simple",
            definition: "USING gin (to_tsvector('public.matric_simple', value))",
        }],
    },
];

/// Tables shared across all memories (deny list).
///
/// These tables contain global system data and are NOT cloned per-memory.
/// Any table in `public` NOT in this list is automatically cloned when
/// creating a new memory, ensuring zero-drift as migrations add tables.
pub(crate) const SHARED_TABLES: &[&str] = &[
    "_sqlx_migrations",
    "api_key",
    "archive_registry",
    "archive_inference_override",
    "audit_event",
    "call_sessions",
    "document_type",
    "embedding_config",
    "event_outbox",
    "file_upload_audit",
    "inbound_dlq",
    "inbound_source",
    "incoming_webhook_receiver",
    "inference_config_audit",
    "job_attempt",
    "job_history",
    "job_queue",
    "oauth_authorization_code",
    "oauth_client",
    "oauth_token",
    "pke_public_keys",
    "realtime_media_stream_attempt",
    "system_config",
    "tenant_registry",
    "transcript_segments",
    "usage_event_conflict",
    "usage_event_delivery",
    "usage_event_ledger",
    "usage_delivery_attempt",
    "usage_sink",
    "user_config",
    "user_metadata_label",
    "user_secrets",
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

/// Render an `ON DELETE` action without nulling a tenant discriminator that was
/// added to a composite tenant-guard foreign key.
fn fk_delete_action_sql(code: &str, source_columns: &[String]) -> String {
    if code != "n" || !source_columns.iter().any(|column| column == "tenant_id") {
        return fk_action_sql(code).to_string();
    }

    let nullable_columns: Vec<&str> = source_columns
        .iter()
        .map(String::as_str)
        .filter(|column| *column != "tenant_id")
        .collect();

    if nullable_columns.is_empty() {
        "NO ACTION".to_string()
    } else {
        format!("SET NULL ({})", nullable_columns.join(", "))
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

    /// Serialize archive schema DDL across processes and repository instances.
    ///
    /// PostgreSQL can deadlock when concurrent archive operations clone, alter,
    /// or drop the same set of public-schema dependencies in different orders.
    /// A transaction-scoped advisory lock keeps the lock and the protected DDL
    /// on the same connection, so it is always released on commit or rollback.
    async fn lock_archive_ddl(tx: &mut sqlx::Transaction<'_, Postgres>) -> Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('fortemi.archive-ddl', 0))")
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
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

    /// Fix FTS generated columns and functional indexes after `LIKE ... INCLUDING ALL`.
    ///
    /// `pg_get_expr()` strips the `public.` schema qualifier when deparsing generated
    /// column expressions, so `to_tsvector('public.matric_english', content)` becomes
    /// `to_tsvector('matric_english'::regconfig, content)` in the archive. This causes
    /// FTS tokenization to fail on multi-byte UTF-8 characters (Issue #412).
    ///
    /// This method drops and recreates the affected columns/indexes with explicitly
    /// qualified config names. Must be called within the same transaction as `LIKE`.
    async fn fix_fts_schema_qualification(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        schema_name: &str,
    ) -> Result<()> {
        for def in FTS_FIX_DEFINITIONS {
            // Check if the table exists in this schema
            let exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = $1 AND table_name = $2
                )
                "#,
            )
            .bind(schema_name)
            .bind(def.table)
            .fetch_one(&mut **tx)
            .await
            .map_err(Error::Database)?;

            if !exists {
                continue;
            }

            // Fix generated column (drop + recreate with qualified config)
            if let Some(gen_col) = &def.generated_column {
                // Dropping the column cascades to dependent indexes
                sqlx::query(&format!(
                    "ALTER TABLE {}.{} DROP COLUMN IF EXISTS {}",
                    schema_name, def.table, gen_col.column_name
                ))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                sqlx::query(&format!(
                    "ALTER TABLE {}.{} ADD COLUMN {} tsvector GENERATED ALWAYS AS ({}) STORED",
                    schema_name, def.table, gen_col.column_name, gen_col.expression
                ))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                // Rebuild GIN index on the stored column
                sqlx::query(&format!(
                    "CREATE INDEX {} ON {}.{} USING gin ({})",
                    gen_col.gin_index_name, schema_name, def.table, gen_col.column_name
                ))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
            }

            // Fix functional indexes
            for idx in def.indexes {
                sqlx::query(&format!(
                    "DROP INDEX IF EXISTS {}.{}",
                    schema_name, idx.index_name
                ))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                sqlx::query(&format!(
                    "CREATE INDEX {} ON {}.{} {}",
                    idx.index_name, schema_name, def.table, idx.definition
                ))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        Ok(())
    }

    /// Apply the same fail-closed tenant policy used by public tables.
    ///
    /// PostgreSQL `LIKE ... INCLUDING ALL` does not copy row-security state or
    /// policies, so every archive clone must be hardened explicitly.
    async fn apply_tenant_rls(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        schema_name: &str,
        tables: &[String],
    ) -> Result<()> {
        for table in tables {
            sqlx::query(&format!(
                "ALTER TABLE {}.{} ENABLE ROW LEVEL SECURITY",
                schema_name, table
            ))
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            sqlx::query(&format!(
                "ALTER TABLE {}.{} FORCE ROW LEVEL SECURITY",
                schema_name, table
            ))
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            sqlx::query(&format!(
                "DROP POLICY IF EXISTS tenant_isolation ON {}.{}",
                schema_name, table
            ))
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

            sqlx::query(&format!(
                "CREATE POLICY tenant_isolation ON {}.{} \
                 USING (tenant_id = current_setting('app.current_tenant')::uuid) \
                 WITH CHECK (tenant_id = current_setting('app.current_tenant')::uuid)",
                schema_name, table
            ))
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
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

        // Execute all DDL in a single transaction for atomicity and serialize
        // it with clone/sync/drop operations across all Fortemi processes.
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        Self::lock_archive_ddl(&mut tx).await?;

        sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

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

        // Step 5b: Fix FTS generated columns and functional indexes.
        // LIKE ... INCLUDING ALL strips the 'public.' schema qualifier from
        // generated column expressions (pg_get_expr behavior), causing FTS to
        // fail on multi-byte UTF-8 in non-default schemas (Issue #412).
        Self::fix_fts_schema_qualification(&mut tx, schema_name).await?;

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
                fk_delete_action_sql(delete_action, &source_columns),
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

        Self::apply_tenant_rls(&mut tx, schema_name, &tables).await?;

        // Step 8: Create the get_default_embedding_set_id() function in the new schema.
        // This function is called by store_tx() when creating embeddings.
        sqlx::query(&format!(
            r#"
            CREATE OR REPLACE FUNCTION {}.get_default_embedding_set_id()
            RETURNS UUID AS $$
                SELECT id FROM {}.embedding_set
                WHERE slug = 'default' AND is_active = TRUE
                LIMIT 1;
            $$ LANGUAGE SQL STABLE
            "#,
            schema_name, schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Step 10: Seed the default embedding set.
        // This is required for store_tx() to work correctly when creating embeddings.
        let default_set_id = new_v7();
        let default_config_id: Uuid =
            sqlx::query_scalar("SELECT id FROM embedding_config WHERE is_default = TRUE LIMIT 1")
                .fetch_one(&mut *tx)
                .await
                .map_err(Error::Database)?;

        sqlx::query(&format!(
            r#"
            INSERT INTO {}.embedding_set (
                id, name, slug, description, purpose, usage_hints, keywords,
                mode, criteria, embedding_config_id, is_system, is_active, index_status
            ) VALUES (
                $1, 'Default', 'default',
                'Primary embedding set containing all notes. Used for general semantic search.',
                'Provides semantic search across the entire knowledge base.',
                'Use this set for general queries when you want to search all content. This is the default set used when no specific set is specified.',
                ARRAY['all', 'general', 'default', 'everything', 'global'],
                'auto', $2, $3, TRUE, TRUE, 'ready'
            )
            "#,
            schema_name
        ))
        .bind(default_set_id)
        .bind(serde_json::json!({"include_all": true, "exclude_archived": true}))
        .bind(default_config_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        vector_index::enqueue_build_for_config_tx(
            &mut tx,
            schema_name,
            default_config_id,
            false,
            "archive_default_set_created",
        )
        .await?;

        sqlx::query(&format!(
            "INSERT INTO {}.shard_embedding_set_bootstrap (tenant_id, set_id)
             SELECT tenant_id, id FROM {}.embedding_set WHERE id = $1",
            schema_name, schema_name
        ))
        .bind(default_set_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Step 11: Seed the default SKOS concept scheme.
        // Required for ConceptTaggingHandler in the NLP pipeline (get_default_scheme_id_tx).
        // Uses ON CONFLICT DO NOTHING because archive cloning copies data from the source
        // archive, which already contains the default scheme.
        let default_scheme_id = new_v7();
        sqlx::query(&format!(
            r#"
            WITH seeded AS (INSERT INTO {}.skos_concept_scheme (id, notation, uri, title, description, is_system)
            VALUES ($1, 'default', 'https://matric.io/schemes/default', 'Default Tags',
                    'Default concept scheme for general-purpose tagging', TRUE)
            ON CONFLICT DO NOTHING RETURNING tenant_id, id)
            INSERT INTO {}.shard_skos_scheme_bootstrap (tenant_id, scheme_id)
            SELECT tenant_id, id FROM seeded
            "#,
            schema_name, schema_name
        ))
        .bind(default_scheme_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    /// Compute the current schema version from per-memory tables in public.
    ///
    /// Uses the total column count across all per-memory tables, so both
    /// new tables AND new columns on existing tables trigger auto-migration.
    /// Previously this only counted tables, which meant `ALTER TABLE ADD COLUMN`
    /// migrations were invisible to the version check (Issue #565).
    async fn current_schema_version(&self) -> Result<i32> {
        let shared: Vec<String> = SHARED_TABLES.iter().map(|s| s.to_string()).collect();
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pg_attribute a
            JOIN pg_class c ON a.attrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname = 'public'
                AND c.relkind = 'r'
                AND c.relname != ALL($1::text[])
                AND a.attnum > 0
                AND NOT a.attisdropped
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

    pub async fn create_archive_schema(
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

        // Create the PostgreSQL schema and all of its tables atomically under
        // the archive DDL advisory lock.
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
}
