# Multi-Schema Implementation Code Snippets

Ready-to-use code snippets for implementing schema-based parallel archives in matric-memory.

---

## Core Implementation

### 1. SchemaContext - Main Abstraction

File: `crates/matric-db/src/schema_context.rs`

```rust
//! Schema context for multi-schema database operations.

use sqlx::{PgPool, Postgres, Transaction};
use std::future::Future;
use std::pin::Pin;
use matric_core::{Error, Result};

/// Context for executing queries within a specific PostgreSQL schema.
///
/// Manages `search_path` setting and provides schema-scoped repository access.
pub struct SchemaContext {
    pool: PgPool,
    schema: String,
}

impl SchemaContext {
    /// Create a new schema context.
    pub fn new(pool: PgPool, schema: impl Into<String>) -> Self {
        Self {
            pool,
            schema: schema.into(),
        }
    }

    /// Get the schema name.
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a function within a schema-specific transaction.
    ///
    /// Sets `search_path` to this schema for the duration of the transaction.
    /// The path automatically resets when the transaction commits or rolls back.
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a mut Transaction<'_, Postgres>)
            -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>> + Send,
    {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // SET LOCAL is transaction-scoped (auto-resets on commit/rollback)
        sqlx::query(&format!(
            "SET LOCAL search_path TO {}, public",
            self.schema
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        let result = f(&mut tx).await?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }

    /// Execute a read-only query within the schema.
    ///
    /// Does not create a transaction, suitable for SELECT queries.
    pub async fn query<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a PgPool, &'a str)
            -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>> + Send,
    {
        // For read-only, we can use a simpler approach with query params
        f(&self.pool, &self.schema).await
    }
}

impl Clone for SchemaContext {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            schema: self.schema.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_context_creation() {
        let pool = PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let ctx = SchemaContext::new(pool, "archive_2024");
        assert_eq!(ctx.schema(), "archive_2024");
    }
}
```

---

### 2. Database Extension Methods

File: `crates/matric-db/src/lib.rs` (add to existing Database impl)

```rust
impl Database {
    /// Create a schema-specific context for multi-schema operations.
    ///
    /// # Example
    ///
    /// ```
    /// let archive_ctx = db.for_schema("archive_2024");
    /// let note_id = archive_ctx.notes().insert(req).await?;
    /// ```
    pub fn for_schema(&self, schema: &str) -> SchemaContext {
        SchemaContext::new(self.pool.clone(), schema)
    }

    /// List all archive schemas in the database.
    ///
    /// Returns schemas matching the pattern 'archive_%'.
    pub async fn list_archive_schemas(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT schema_name
             FROM information_schema.schemata
             WHERE schema_name LIKE 'archive_%'
             ORDER BY schema_name"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("schema_name")).collect())
    }

    /// Create a new archive schema with all required tables.
    ///
    /// This creates the schema and runs all migrations to initialize tables,
    /// indexes, and constraints.
    ///
    /// # Security
    ///
    /// Schema names are validated to prevent SQL injection. Only alphanumeric
    /// characters and underscores are allowed.
    pub async fn create_archive_schema(&self, schema: &str) -> Result<()> {
        // Validate schema name
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::InvalidInput(
                "Schema name must contain only alphanumeric characters and underscores".into()
            ));
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Create schema
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Set search_path for migration
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Run migrations in this schema
        #[cfg(feature = "migrations")]
        {
            sqlx::migrate!("../../migrations")
                .run(&mut *tx)
                .await
                .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;
        }

        tx.commit().await.map_err(Error::Database)?;

        tracing::info!(
            subsystem = "database",
            component = "schema",
            op = "create",
            schema = schema,
            "Archive schema created successfully"
        );

        Ok(())
    }

    /// Drop an archive schema and all its data.
    ///
    /// # Warning
    ///
    /// This permanently deletes all data in the schema. Use with caution.
    pub async fn drop_archive_schema(&self, schema: &str) -> Result<()> {
        // Validate schema name
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::InvalidInput("Invalid schema name".into()));
        }

        // Prevent dropping non-archive schemas
        if !schema.starts_with("archive_") {
            return Err(Error::InvalidInput(
                "Can only drop schemas with 'archive_' prefix".into()
            ));
        }

        sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema))
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        tracing::warn!(
            subsystem = "database",
            component = "schema",
            op = "drop",
            schema = schema,
            "Archive schema dropped"
        );

        Ok(())
    }

    /// Get schema metadata (table count, size, etc.)
    pub async fn get_schema_info(&self, schema: &str) -> Result<SchemaInfo> {
        let row = sqlx::query(
            r#"
            SELECT
                schema_name,
                (SELECT COUNT(*)
                 FROM information_schema.tables
                 WHERE table_schema = $1 AND table_type = 'BASE TABLE') as table_count,
                pg_size_pretty(
                    SUM(pg_total_relation_size(quote_ident(schemaname) || '.' || quote_ident(tablename)))
                ) as size_pretty
            FROM pg_tables
            WHERE schemaname = $1
            GROUP BY schema_name
            "#
        )
        .bind(schema)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(row) = row {
            Ok(SchemaInfo {
                schema_name: row.get("schema_name"),
                table_count: row.get("table_count"),
                size_pretty: row.get("size_pretty"),
            })
        } else {
            Err(Error::NotFound(format!("Schema {} not found", schema)))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub schema_name: String,
    pub table_count: i64,
    pub size_pretty: String,
}
```

---

### 3. Schema-Aware Note Repository

File: `crates/matric-db/src/repositories/schema_aware_note.rs`

```rust
//! Schema-aware note repository for multi-schema operations.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use matric_core::{
    new_v7, CreateNoteRequest, Error, ListNotesRequest, ListNotesResponse,
    NoteFull, NoteRepository, Result, UpdateNoteStatusRequest,
};

use crate::notes::PgNoteRepository;

/// Note repository that operates within a specific schema.
///
/// Wraps PgNoteRepository and sets search_path for each operation.
pub struct SchemaAwareNoteRepository {
    pool: Pool<Postgres>,
    schema: String,
}

impl SchemaAwareNoteRepository {
    pub fn new(pool: Pool<Postgres>, schema: impl Into<String>) -> Self {
        Self {
            pool,
            schema: schema.into(),
        }
    }

    /// Execute a repository operation within the schema context.
    async fn with_schema<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a mut sqlx::Transaction<'_, Postgres>)
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send + 'a>>
            + Send,
    {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Set search_path for this transaction
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", self.schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        let result = f(&mut tx).await?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }
}

#[async_trait]
impl NoteRepository for SchemaAwareNoteRepository {
    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid> {
        self.with_schema(|tx| {
            Box::pin(async move {
                let note_id = new_v7();
                let now = Utc::now();
                let hash = PgNoteRepository::hash_content(&req.content);

                // All queries use unqualified table names
                sqlx::query(
                    "INSERT INTO note (id, collection_id, format, source, created_at_utc, updated_at_utc, metadata)
                     VALUES ($1, $2, $3, $4, $5, $5, COALESCE($6, '{}'::jsonb))"
                )
                .bind(note_id)
                .bind(req.collection_id)
                .bind(&req.format)
                .bind(&req.source)
                .bind(now)
                .bind(req.metadata.as_ref().unwrap_or(&serde_json::json!({})))
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                // Insert original content
                sqlx::query(
                    "INSERT INTO note_original (id, note_id, content, hash) VALUES ($1, $2, $3, $4)"
                )
                .bind(new_v7())
                .bind(note_id)
                .bind(&req.content)
                .bind(&hash)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                // Insert initial revision
                let revision_id = new_v7();
                sqlx::query(
                    "INSERT INTO note_revision (id, note_id, content, rationale, created_at_utc, revision_number)
                     VALUES ($1, $2, $3, NULL, $4, 1)"
                )
                .bind(revision_id)
                .bind(note_id)
                .bind(&req.content)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                // Populate current revised content
                sqlx::query(
                    "INSERT INTO note_revised_current (note_id, content, last_revision_id)
                     VALUES ($1, $2, $3)"
                )
                .bind(note_id)
                .bind(&req.content)
                .bind(revision_id)
                .execute(&mut **tx)
                .await
                .map_err(Error::Database)?;

                // Add tags if provided
                if let Some(tags) = req.tags {
                    for tag in tags {
                        sqlx::query(
                            "INSERT INTO tag (name, created_at_utc)
                             VALUES ($1, $2) ON CONFLICT DO NOTHING"
                        )
                        .bind(&tag)
                        .bind(now)
                        .execute(&mut **tx)
                        .await
                        .map_err(Error::Database)?;

                        sqlx::query(
                            "INSERT INTO note_tag (note_id, tag_name, source)
                             VALUES ($1, $2, 'user')"
                        )
                        .bind(note_id)
                        .bind(&tag)
                        .execute(&mut **tx)
                        .await
                        .map_err(Error::Database)?;
                    }
                }

                Ok(note_id)
            })
        }).await
    }

    async fn fetch(&self, id: Uuid) -> Result<NoteFull> {
        // Delegate to standard repository with schema context
        self.with_schema(|tx| {
            Box::pin(async move {
                // Fetch note (search_path handles schema routing)
                let repo = PgNoteRepository::new(self.pool.clone());
                repo.fetch(id).await
            })
        }).await
    }

    async fn list(&self, req: ListNotesRequest) -> Result<ListNotesResponse> {
        // Delegate to standard repository with schema context
        let repo = PgNoteRepository::new(self.pool.clone());

        self.with_schema(|tx| {
            Box::pin(async move {
                repo.list(req).await
            })
        }).await
    }

    // ... implement remaining NoteRepository methods similarly
}
```

---

### 4. Archive Management API

File: `crates/matric-api/src/routes/archives.rs`

```rust
//! Archive management API routes.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use matric_db::Database;
use matric_core::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct CreateArchiveRequest {
    /// Archive name (alphanumeric + underscore only)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArchiveResponse {
    pub id: Uuid,
    pub schema: String,
    pub name: String,
    pub description: Option<String>,
    pub note_count: i64,
    pub embedding_count: i64,
    pub created_at: DateTime<Utc>,
    pub size_pretty: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_archive).get(list_archives))
        .route("/:archive_id", get(get_archive).delete(delete_archive))
        .route("/:archive_id/stats", get(get_archive_stats))
}

/// POST /archives - Create a new archive
async fn create_archive(
    State(db): State<Database>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<(StatusCode, Json<ArchiveResponse>)> {
    // Validate name
    if !req.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(Error::InvalidInput(
            "Archive name must contain only alphanumeric characters and underscores".into()
        ));
    }

    let schema = format!("archive_{}", req.name);

    // Create schema and run migrations
    db.create_archive_schema(&schema).await?;

    // Store archive metadata in registry (in public schema)
    let archive_id = matric_core::new_v7();
    sqlx::query(
        "INSERT INTO archive_registry (id, schema_name, display_name, description, created_at_utc)
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(archive_id)
    .bind(&schema)
    .bind(&req.name)
    .bind(&req.description)
    .bind(Utc::now())
    .execute(db.pool())
    .await
    .map_err(Error::Database)?;

    Ok((StatusCode::CREATED, Json(ArchiveResponse {
        id: archive_id,
        schema: schema.clone(),
        name: req.name,
        description: req.description,
        note_count: 0,
        embedding_count: 0,
        created_at: Utc::now(),
        size_pretty: None,
    })))
}

/// GET /archives - List all archives
async fn list_archives(
    State(db): State<Database>,
) -> Result<Json<Vec<ArchiveResponse>>> {
    let rows = sqlx::query(
        "SELECT id, schema_name, display_name, description, created_at_utc
         FROM archive_registry
         ORDER BY created_at_utc DESC"
    )
    .fetch_all(db.pool())
    .await
    .map_err(Error::Database)?;

    let mut archives = Vec::new();

    for row in rows {
        let schema: String = row.get("schema_name");
        let ctx = db.for_schema(&schema);

        // Get note count from schema
        let note_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM note WHERE deleted_at IS NULL"
        )
        .fetch_one(ctx.pool())
        .await
        .unwrap_or(0);

        archives.push(ArchiveResponse {
            id: row.get("id"),
            schema: schema.clone(),
            name: row.get("display_name"),
            description: row.get("description"),
            note_count,
            embedding_count: 0, // TODO
            created_at: row.get("created_at_utc"),
            size_pretty: None,
        });
    }

    Ok(Json(archives))
}

/// DELETE /archives/:archive_id - Delete an archive
async fn delete_archive(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
) -> Result<StatusCode> {
    let schema = format!("archive_{}", archive_id);

    // Drop schema cascade
    db.drop_archive_schema(&schema).await?;

    // Remove from registry
    sqlx::query("DELETE FROM archive_registry WHERE schema_name = $1")
        .bind(&schema)
        .execute(db.pool())
        .await
        .map_err(Error::Database)?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /archives/:archive_id/stats - Get archive statistics
async fn get_archive_stats(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
) -> Result<Json<ArchiveStats>> {
    let schema = format!("archive_{}", archive_id);
    let ctx = db.for_schema(&schema);

    let stats = ArchiveStats {
        schema: schema.clone(),
        note_count: get_count(ctx.pool(), "note").await?,
        embedding_count: get_count(ctx.pool(), "embedding").await?,
        collection_count: get_count(ctx.pool(), "collection").await?,
        link_count: get_count(ctx.pool(), "link").await?,
    };

    Ok(Json(stats))
}

#[derive(Debug, Serialize)]
pub struct ArchiveStats {
    pub schema: String,
    pub note_count: i64,
    pub embedding_count: i64,
    pub collection_count: i64,
    pub link_count: i64,
}

async fn get_count(pool: &sqlx::PgPool, table: &str) -> Result<i64> {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(pool)
        .await
        .map_err(Error::Database)
}
```

---

### 5. Archive Registry Migration

File: `migrations/20260202500000_archive_registry.sql`

```sql
-- Archive registry for tracking multi-schema archives
--
-- This table lives in the public schema and tracks metadata for all archive schemas.

CREATE TABLE archive_registry (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    schema_name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by_user_id UUID,
    archived_at_utc TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_archive_registry_created_at ON archive_registry(created_at_utc DESC);
CREATE INDEX idx_archive_registry_schema ON archive_registry(schema_name);

COMMENT ON TABLE archive_registry IS 'Registry of multi-schema archives for parallel memory management';
COMMENT ON COLUMN archive_registry.schema_name IS 'PostgreSQL schema name (e.g., archive_2024)';
COMMENT ON COLUMN archive_registry.display_name IS 'Human-readable archive name';
```

---

### 6. Integration in main.rs

File: `crates/matric-api/src/main.rs`

```rust
// Add archive routes to router
use crate::routes::archives;

async fn app_router(db: Database) -> Router {
    Router::new()
        .nest("/api/v1/notes", notes::routes())
        .nest("/api/v1/archives", archives::routes())  // NEW
        .nest("/api/v1/embeddings", embeddings::routes())
        // ... other routes
        .with_state(db)
}
```

---

### 7. Usage Examples

```rust
// Example 1: Create archive and add notes
async fn example_create_archive(db: &Database) -> Result<()> {
    // Create new archive
    db.create_archive_schema("archive_2024").await?;

    // Get schema context
    let ctx = db.for_schema("archive_2024");

    // Insert notes in this archive
    let note_id = ctx.notes().insert(CreateNoteRequest {
        content: "My first archived note".into(),
        format: "markdown".into(),
        source: "manual".into(),
        collection_id: None,
        tags: Some(vec!["archive".into()]),
        metadata: None,
    }).await?;

    println!("Created note {} in archive_2024", note_id);
    Ok(())
}

// Example 2: List all archives
async fn example_list_archives(db: &Database) -> Result<()> {
    let archives = db.list_archive_schemas().await?;

    for archive in archives {
        let ctx = db.for_schema(&archive);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note")
            .fetch_one(ctx.pool())
            .await?;

        println!("{}: {} notes", archive, count);
    }

    Ok(())
}

// Example 3: Cross-archive query
async fn example_cross_archive_query(db: &Database) -> Result<Vec<(String, Uuid, String)>> {
    // Query across multiple archives using qualified names
    let results = sqlx::query_as::<_, (String, Uuid, String)>(
        r#"
        SELECT 'archive_2024' as archive, id, title FROM archive_2024.note
        UNION ALL
        SELECT 'archive_2025' as archive, id, title FROM archive_2025.note
        ORDER BY archive, id
        "#
    )
    .fetch_all(db.pool())
    .await?;

    Ok(results)
}
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schema_creation() {
        let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();

        db.create_archive_schema("archive_test").await.unwrap();

        let schemas = db.list_archive_schemas().await.unwrap();
        assert!(schemas.contains(&"archive_test".to_string()));

        // Cleanup
        db.drop_archive_schema("archive_test").await.unwrap();
    }

    #[tokio::test]
    async fn test_schema_aware_repository() {
        let db = Database::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();

        db.create_archive_schema("archive_repo_test").await.unwrap();

        let ctx = db.for_schema("archive_repo_test");
        let note_id = ctx.notes().insert(CreateNoteRequest {
            content: "Test note".into(),
            format: "markdown".into(),
            source: "test".into(),
            collection_id: None,
            tags: None,
            metadata: None,
        }).await.unwrap();

        let note = ctx.notes().fetch(note_id).await.unwrap();
        assert_eq!(note.note.id, note_id);

        // Cleanup
        db.drop_archive_schema("archive_repo_test").await.unwrap();
    }
}
```

---

## Shell Commands

```bash
# Create archive via API
curl -X POST http://localhost:3000/api/v1/archives \
  -H "Content-Type: application/json" \
  -d '{"name":"2024","description":"Archive for 2024"}'

# List archives
curl http://localhost:3000/api/v1/archives

# Get archive stats
curl http://localhost:3000/api/v1/archives/2024/stats

# Create note in archive
curl -X POST http://localhost:3000/api/v1/archives/2024/notes \
  -H "Content-Type: application/json" \
  -d '{"content":"Archived note","format":"markdown","source":"api"}'

# Delete archive
curl -X DELETE http://localhost:3000/api/v1/archives/2024

# Backup archive (shell)
pg_dump -n archive_2024 -F c matric_db > archive_2024.dump

# Restore archive (shell)
pg_restore -n archive_2024 -d matric_db archive_2024.dump
```

---

## Performance Tips

1. **Connection pool size:** Keep at 10 connections (default) for shared pool
2. **Transaction scope:** Use `SET LOCAL` to auto-reset after commit
3. **Index strategy:** Create same indexes per schema for consistency
4. **Query optimization:** Use `EXPLAIN ANALYZE` with search_path set
5. **Monitoring:** Track `SET` command overhead in slow query log

---

These snippets are ready to copy-paste into your codebase with minimal modifications.
