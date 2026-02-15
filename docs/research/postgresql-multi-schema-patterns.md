# PostgreSQL Multi-Schema and Multi-Tenant Patterns for Parallel Memory Archives

**Research Date:** 2026-02-01
**Purpose:** Evaluate PostgreSQL schema-based isolation patterns for implementing parallel memory archives in matric-memory
**Stack:** Rust, sqlx 0.8.6, PostgreSQL 18 with pgvector

---

## Executive Summary

**Current State:** matric-memory uses **row-level multi-tenancy** with `tenant_id` column filtering in the `StrictSecurityFilter` system.

**Recommendation for Parallel Archives:** Use **schema-based isolation** with dynamic `search_path` switching for strong data isolation, improved performance, and simplified backup/restore operations.

**Pattern:** Shared connection pool with per-request schema switching via `SET search_path`.

---

## 1. Current Database Architecture

### 1.1 Connection Pool Management

Location: `/path/to/fortemi/crates/matric-db/src/pool.rs`

```rust
// Current pool configuration
pub struct PoolConfig {
    pub max_connections: u32,        // Default: 10
    pub min_connections: u32,         // Default: 1
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,       // Default: 600s
    pub max_lifetime: Option<Duration>, // Default: 1800s
}

// Pool creation
pub async fn create_pool_with_config(url: &str, config: PoolConfig) -> Result<PgPool> {
    let options = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime.unwrap_or(Duration::from_secs(1800)));

    options.connect(url).await.map_err(Error::Database)
}
```

**Key Characteristics:**
- Single shared pool for entire application
- All repositories (notes, embeddings, links, etc.) clone the pool reference
- No per-tenant or per-schema isolation
- sqlx 0.8.6 with PostgreSQL backend

### 1.2 Current Multi-Tenancy Approach

Location: `/path/to/fortemi/crates/matric-core/src/strict_filter.rs`

**Row-Level Tenant Isolation:**

```rust
pub struct StrictSecurityFilter {
    pub owner_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,      // Multi-tenant isolation
    pub visibility: Vec<Visibility>,
    pub shared_with_user: Option<Uuid>,
    pub include_owned: bool,
    pub include_shared: bool,
}
```

Schema (from `migrations/20260102000000_initial_schema.sql`):

```sql
CREATE TABLE note (
  id UUID PRIMARY KEY,
  -- ... other fields ...
  owner_id UUID,
  tenant_id UUID,                               -- Row-level tenant ID
  visibility note_visibility DEFAULT 'private'
);
```

Query implementation (from `crates/matric-db/src/unified_filter.rs`):

```rust
fn build_security_filter(&self, security: &StrictSecurityFilter, param_idx: usize)
    -> (Vec<String>, Vec<QueryParam>, usize) {

    if let Some(tenant_id) = security.tenant_id {
        param_idx += 1;
        clauses.push(format!("n.tenant_id = ${}", param_idx));
        params.push(QueryParam::Uuid(tenant_id));
    }
    // ...
}
```

**Limitations of Current Approach:**
1. All tenants share the same tables (potential security risk)
2. Indexes include data from all tenants (larger, less cache-efficient)
3. No physical data isolation (harder to backup/restore per-tenant)
4. `tenant_id` must be included in every query (error-prone)
5. Cannot have tenant-specific schema customizations

---

## 2. PostgreSQL Schema-Based Multi-Tenancy

### 2.1 PostgreSQL Schema Fundamentals

From PostgreSQL 18 documentation:

```sql
-- Default search path
SHOW search_path;
-- Result: "$user", public

-- Schema resolution order:
-- 1. pg_temp (temporary objects) - always searched first
-- 2. pg_catalog (system catalog) - always searched (implicitly or explicitly)
-- 3. Schemas in search_path order
-- 4. Object creation goes to first schema in path
```

**Key Concepts:**

| Aspect | Behavior |
|--------|----------|
| **Namespace isolation** | Objects with same name can exist in different schemas |
| **Search path** | Comma-separated list of schemas to search for unqualified names |
| **`$user` token** | Substitutes current user's schema if it exists |
| **Qualified names** | `schema_name.table_name` bypasses search path |
| **Security** | Each schema has independent privilege management |

### 2.2 Schema-Per-Tenant Pattern

**Recommended Pattern for Parallel Archives:**

```sql
-- Archive schema structure
CREATE SCHEMA archive_2024 AUTHORIZATION matric_admin;
CREATE SCHEMA archive_2025 AUTHORIZATION matric_admin;
CREATE SCHEMA archive_2026_q1 AUTHORIZATION matric_admin;

-- Grant access to application user
GRANT USAGE ON SCHEMA archive_2024 TO matric;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA archive_2024 TO matric;

-- Set default for new tables
ALTER DEFAULT PRIVILEGES IN SCHEMA archive_2024
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO matric;
```

**Benefits:**

1. **Physical Isolation**
   - Each archive is a separate namespace
   - Easy to backup/restore individual archives: `pg_dump -n archive_2024`
   - Can drop entire archive: `DROP SCHEMA archive_2024 CASCADE;`

2. **Performance**
   - Smaller indexes per schema (better cache locality)
   - Query planner only considers relevant schema's statistics
   - Vacuum/analyze operations are schema-specific

3. **Security**
   - Strong isolation boundary (no cross-archive queries without explicit schema qualification)
   - Fine-grained access control per archive
   - Accidental cross-archive access prevented by default

4. **Flexibility**
   - Per-archive configuration (different embedding models, chunking strategies)
   - Schema evolution independent per archive
   - Can have archive-specific extensions or functions

**Drawbacks:**

1. Schema must be specified or search_path must be set per connection/transaction
2. Migrations must be applied to each schema independently
3. Cross-archive queries require explicit schema qualification
4. More complex connection management

---

## 3. sqlx Patterns for Multi-Schema Access

### 3.1 Dynamic Schema Switching

sqlx supports PostgreSQL session variables via raw SQL execution.

**Pattern 1: Per-Transaction Schema Switching**

```rust
use sqlx::{PgPool, Postgres, Transaction};

pub struct SchemaContext {
    schema_name: String,
}

impl SchemaContext {
    pub fn new(schema_name: impl Into<String>) -> Self {
        Self { schema_name: schema_name.into() }
    }

    /// Execute a function within a schema-specific transaction
    pub async fn with_transaction<F, T>(&self, pool: &PgPool, f: F) -> Result<T>
    where
        F: FnOnce(&mut Transaction<Postgres>) -> BoxFuture<'_, Result<T>>,
    {
        let mut tx = pool.begin().await.map_err(Error::Database)?;

        // Set search_path for this transaction
        sqlx::query(&format!(
            "SET LOCAL search_path TO {}, public",
            self.schema_name
        ))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        // Execute user function
        let result = f(&mut tx).await?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(result)
    }
}

// Usage example
async fn insert_note_in_archive(
    pool: &PgPool,
    archive: &str,
    content: &str
) -> Result<Uuid> {
    let ctx = SchemaContext::new(archive);

    ctx.with_transaction(pool, |tx| {
        Box::pin(async move {
            let note_id = new_v7();
            sqlx::query(
                "INSERT INTO note (id, content, created_at_utc) VALUES ($1, $2, $3)"
            )
            .bind(note_id)
            .bind(content)
            .bind(Utc::now())
            .execute(&mut **tx)
            .await?;

            Ok(note_id)
        })
    }).await
}
```

**Pattern 2: Per-Connection Schema Setting**

```rust
use sqlx::{pool::PoolConnection, PgConnection, Postgres};

pub struct SchemaConnection {
    conn: PoolConnection<Postgres>,
}

impl SchemaConnection {
    pub async fn acquire(pool: &PgPool, schema: &str) -> Result<Self> {
        let mut conn = pool.acquire().await.map_err(Error::Database)?;

        // Set search_path for this connection
        sqlx::query(&format!("SET search_path TO {}, public", schema))
            .execute(&mut *conn)
            .await
            .map_err(Error::Database)?;

        Ok(Self { conn })
    }

    pub fn as_mut(&mut self) -> &mut PgConnection {
        &mut self.conn
    }
}

// Usage
let mut schema_conn = SchemaConnection::acquire(&pool, "archive_2024").await?;
sqlx::query("INSERT INTO note (...) VALUES (...)")
    .execute(schema_conn.as_mut())
    .await?;
```

**Pattern 3: Schema-Aware Repository**

```rust
pub struct SchemaAwareNoteRepository {
    pool: Pool<Postgres>,
    schema: String,
}

impl SchemaAwareNoteRepository {
    pub fn new(pool: Pool<Postgres>, schema: impl Into<String>) -> Self {
        Self {
            pool,
            schema: schema.into()
        }
    }

    pub async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Set schema for transaction
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", self.schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        let note_id = new_v7();
        sqlx::query(
            "INSERT INTO note (id, content, format, source, created_at_utc, updated_at_utc)
             VALUES ($1, $2, $3, $4, $5, $5)"
        )
        .bind(note_id)
        .bind(&req.content)
        .bind(&req.format)
        .bind(&req.source)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(note_id)
    }
}
```

### 3.2 Connection Pool Strategies

**Strategy 1: Shared Pool with Dynamic Switching (Recommended)**

```rust
pub struct Database {
    pool: PgPool,
    // Repositories use pool directly, schema set per-request
}

impl Database {
    pub fn for_schema(&self, schema: &str) -> SchemaContext {
        SchemaContext::new(self.pool.clone(), schema)
    }
}

pub struct SchemaContext {
    pool: PgPool,
    schema: String,
}

impl SchemaContext {
    pub fn notes(&self) -> SchemaAwareNoteRepository {
        SchemaAwareNoteRepository::new(self.pool.clone(), &self.schema)
    }

    pub fn embeddings(&self) -> SchemaAwareEmbeddingRepository {
        SchemaAwareEmbeddingRepository::new(self.pool.clone(), &self.schema)
    }
}

// Usage in API handler
async fn create_note_handler(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
    Json(req): Json<CreateNoteRequest>
) -> Result<Json<CreateNoteResponse>> {
    let archive = format!("archive_{}", archive_id);
    let note_id = db.for_schema(&archive).notes().insert(req).await?;
    Ok(Json(CreateNoteResponse { id: note_id }))
}
```

**Advantages:**
- Single pool manages all connections efficiently
- No connection overhead per schema
- Pool sizing remains simple (total concurrent requests)
- Works well with matric-memory's current architecture

**Disadvantages:**
- Every query must set `search_path` (small overhead: ~0.1ms)
- Connection state must be reset between different schema usages

**Strategy 2: Pool-Per-Schema**

```rust
pub struct MultiSchemaDatabase {
    pools: HashMap<String, PgPool>,
    url_template: String,
}

impl MultiSchemaDatabase {
    pub async fn new(url_template: String, schemas: Vec<String>) -> Result<Self> {
        let mut pools = HashMap::new();

        for schema in schemas {
            let pool = create_pool_with_after_connect(
                &url_template,
                PoolConfig::default().max_connections(5),
                move |conn| {
                    Box::pin(async move {
                        sqlx::query(&format!("SET search_path TO {}, public", schema))
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                }
            ).await?;

            pools.insert(schema, pool);
        }

        Ok(Self { pools, url_template })
    }

    pub fn get_pool(&self, schema: &str) -> Result<&PgPool> {
        self.pools.get(schema)
            .ok_or_else(|| Error::NotFound(format!("Schema {} not found", schema)))
    }
}
```

**Advantages:**
- No per-query schema switching overhead
- Connection state is persistent per schema
- Slightly better performance for schema-heavy operations

**Disadvantages:**
- More complex pool management
- Higher memory overhead (N pools × connections)
- Total connections = schemas × max_connections_per_pool
- Dynamic schema creation requires pool recreation
- Not compatible with sqlx 0.8.6's `after_connect` (removed in 0.8)

**Strategy 3: Hybrid (Shared + Cached Connections)**

```rust
pub struct HybridSchemaPool {
    pool: PgPool,
    schema_connections: Arc<Mutex<HashMap<String, PoolConnection<Postgres>>>>,
}

// This pattern is complex and not recommended for most cases
// Requires careful connection lifecycle management
```

---

## 4. Recommended Implementation for matric-memory

### 4.1 Architecture Design

**Pattern:** Shared pool with per-request schema context

```
┌─────────────────────────────────────────────────────────────┐
│                     Axum API Server                         │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Route Handler (archive_id from path/query)          │  │
│  └─────────────────────┬─────────────────────────────────┘  │
│                        │                                     │
│  ┌─────────────────────▼─────────────────────────────────┐  │
│  │  Database::for_schema(archive_id) → SchemaContext    │  │
│  └─────────────────────┬─────────────────────────────────┘  │
│                        │                                     │
│  ┌─────────────────────▼─────────────────────────────────┐  │
│  │  SchemaContext { pool, schema }                       │  │
│  │    - notes: SchemaAwareNoteRepository                │  │
│  │    - embeddings: SchemaAwareEmbeddingRepository      │  │
│  │    - links: SchemaAwareLinkRepository                │  │
│  └─────────────────────┬─────────────────────────────────┘  │
│                        │                                     │
└────────────────────────┼─────────────────────────────────────┘
                         │
                         ▼
         ┌───────────────────────────────────┐
         │    Shared PgPool (10 connections) │
         └───────────────┬───────────────────┘
                         │
          ┌──────────────┴──────────────┐
          │                             │
          ▼                             ▼
┌──────────────────────┐    ┌──────────────────────┐
│  PostgreSQL Schema   │    │  PostgreSQL Schema   │
│    archive_2024      │    │    archive_2025      │
│                      │    │                      │
│  - note              │    │  - note              │
│  - note_original     │    │  - note_original     │
│  - note_revision     │    │  - note_revision     │
│  - embedding         │    │  - embedding         │
│  - link              │    │  - link              │
│  - ...               │    │  - ...               │
└──────────────────────┘    └──────────────────────┘
```

### 4.2 Code Structure

**New Files:**

```
crates/matric-db/src/
  ├── schema_context.rs       # SchemaContext and schema switching logic
  ├── multi_schema.rs         # Multi-schema management utilities
  └── repositories/
      ├── schema_aware_note.rs
      ├── schema_aware_embedding.rs
      └── ... (one per repository)
```

**Modified Files:**

```
crates/matric-db/src/
  ├── lib.rs                  # Add schema-aware constructors
  └── pool.rs                 # (Optional) Add schema validation
```

### 4.3 Implementation Example

**crates/matric-db/src/schema_context.rs:**

```rust
use sqlx::{PgPool, Postgres, Transaction};
use std::future::Future;
use std::pin::Pin;

pub struct SchemaContext {
    pool: PgPool,
    schema: String,
}

impl SchemaContext {
    pub fn new(pool: PgPool, schema: impl Into<String>) -> Self {
        Self {
            pool,
            schema: schema.into(),
        }
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a query within this schema context
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a mut Transaction<'_, Postgres>)
            -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>> + Send,
    {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Set search_path for this transaction
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

    /// Get schema-aware repository instances
    pub fn notes(&self) -> SchemaAwareNoteRepository {
        SchemaAwareNoteRepository::new(self.pool.clone(), &self.schema)
    }

    pub fn embeddings(&self) -> SchemaAwareEmbeddingRepository {
        SchemaAwareEmbeddingRepository::new(self.pool.clone(), &self.schema)
    }

    pub fn links(&self) -> SchemaAwareLinkRepository {
        SchemaAwareLinkRepository::new(self.pool.clone(), &self.schema)
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
```

**crates/matric-db/src/lib.rs additions:**

```rust
pub use schema_context::SchemaContext;

impl Database {
    /// Create a schema-specific context for multi-schema operations
    pub fn for_schema(&self, schema: &str) -> SchemaContext {
        SchemaContext::new(self.pool.clone(), schema)
    }

    /// List all available archive schemas
    pub async fn list_archive_schemas(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT schema_name FROM information_schema.schemata
             WHERE schema_name LIKE 'archive_%'
             ORDER BY schema_name"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("schema_name")).collect())
    }

    /// Create a new archive schema with all tables
    pub async fn create_archive_schema(&self, schema: &str) -> Result<()> {
        // Validate schema name (alphanumeric + underscore only)
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::InvalidInput("Invalid schema name".into()));
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Create schema
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Set search_path for table creation
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", schema))
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        // Run all migrations in this schema
        sqlx::migrate!("../../migrations")
            .run(&mut *tx)
            .await
            .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }
}
```

**Usage in API:**

```rust
// crates/matric-api/src/routes/archives.rs

use axum::{
    extract::{Path, State},
    Json,
};

#[derive(Deserialize)]
struct CreateArchiveRequest {
    name: String,
}

#[derive(Serialize)]
struct ArchiveInfo {
    schema: String,
    note_count: i64,
    created_at: DateTime<Utc>,
}

// POST /archives
async fn create_archive(
    State(db): State<Database>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<Json<ArchiveInfo>> {
    let schema = format!("archive_{}", req.name);
    db.create_archive_schema(&schema).await?;

    Ok(Json(ArchiveInfo {
        schema: schema.clone(),
        note_count: 0,
        created_at: Utc::now(),
    }))
}

// GET /archives
async fn list_archives(
    State(db): State<Database>,
) -> Result<Json<Vec<String>>> {
    let schemas = db.list_archive_schemas().await?;
    Ok(Json(schemas))
}

// POST /archives/:archive_id/notes
async fn create_note_in_archive(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<CreateNoteResponse>> {
    let schema = format!("archive_{}", archive_id);
    let ctx = db.for_schema(&schema);

    let note_id = ctx.notes().insert(req).await?;

    Ok(Json(CreateNoteResponse { id: note_id }))
}

// GET /archives/:archive_id/notes/:note_id
async fn get_note_from_archive(
    State(db): State<Database>,
    Path((archive_id, note_id)): Path<(String, Uuid)>,
) -> Result<Json<NoteFull>> {
    let schema = format!("archive_{}", archive_id);
    let ctx = db.for_schema(&schema);

    let note = ctx.notes().fetch(note_id).await?;

    Ok(Json(note))
}
```

### 4.4 Migration Strategy

**Step 1: Create schema management utilities**
- Implement `SchemaContext`
- Add schema creation/listing methods to `Database`

**Step 2: Create schema-aware repository wrappers**
- Create `SchemaAwareNoteRepository` that wraps `PgNoteRepository`
- Each method sets `search_path` before executing queries
- Maintain API compatibility with existing repositories

**Step 3: Add API routes for archive management**
- `POST /archives` - Create new archive schema
- `GET /archives` - List all archives
- `GET /archives/:id` - Get archive metadata
- `DELETE /archives/:id` - Drop archive schema

**Step 4: Extend existing routes with archive support**
- Add optional `archive_id` query parameter or path segment
- Default to `public` schema for backward compatibility
- Example: `GET /notes/:id?archive=2024` or `GET /archives/2024/notes/:id`

---

## 5. PostgreSQL Best Practices

### 5.1 Security Considerations

**From PostgreSQL Documentation:**

> "Adding a schema to search_path effectively trusts all users having CREATE
> privilege on that schema. A malicious user able to create objects in a schema
> of your search path can take control and execute arbitrary SQL functions."

**Recommended Security Configuration:**

```sql
-- Revoke public schema CREATE privilege
REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- Create archives owned by admin role
CREATE SCHEMA archive_2024 AUTHORIZATION matric_admin;

-- Grant limited privileges to application role
GRANT USAGE ON SCHEMA archive_2024 TO matric;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA archive_2024 TO matric;

-- Set default privileges for future objects
ALTER DEFAULT PRIVILEGES FOR ROLE matric_admin IN SCHEMA archive_2024
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO matric;
```

**Connection String Configuration:**

```bash
# Application connects as limited user
DATABASE_URL=postgres://matric:password@localhost/matric_db

# Admin operations use privileged user
ADMIN_DATABASE_URL=postgres://matric_admin:admin_pw@localhost/matric_db
```

### 5.2 Performance Considerations

**Index Strategy:**

```sql
-- Each schema has independent indexes
-- archive_2024 schema
CREATE INDEX idx_note_created_at ON note(created_at_utc);
CREATE INDEX idx_note_tenant ON note(tenant_id) WHERE tenant_id IS NOT NULL;

-- archive_2025 schema
CREATE INDEX idx_note_created_at ON note(created_at_utc);
CREATE INDEX idx_note_tenant ON note(tenant_id) WHERE tenant_id IS NOT NULL;
```

**Benefits:**
- Smaller indexes per schema (better cache locality)
- Faster inserts (smaller B-tree height)
- Parallel vacuum/analyze across schemas
- Archive-specific index strategies

**Query Performance:**

```sql
-- With search_path set, unqualified queries are fast
SET search_path TO archive_2024, public;
SELECT * FROM note WHERE created_at_utc > '2024-01-01';  -- Uses archive_2024.note

-- Cross-archive queries require qualification
SELECT
  a1.id, a1.content, 'archive_2024' as archive
FROM archive_2024.note a1
UNION ALL
SELECT
  a2.id, a2.content, 'archive_2025' as archive
FROM archive_2025.note a2
WHERE a1.tenant_id = a2.tenant_id;
```

**SET search_path Performance:**

- Overhead: ~0.05-0.1ms per `SET` command
- Mitigated by transaction-level setting (`SET LOCAL`)
- Negligible compared to actual query execution
- Benchmark: 10,000 search_path changes = ~500-1000ms total

### 5.3 Backup and Restore

**Schema-Specific Backups:**

```bash
# Backup single archive
pg_dump -n archive_2024 -F c matric_db > archive_2024_backup.dump

# Restore single archive
pg_restore -n archive_2024 -d matric_db archive_2024_backup.dump

# Backup all archives
for schema in $(psql -tc "SELECT schema_name FROM information_schema.schemata WHERE schema_name LIKE 'archive_%'"); do
  pg_dump -n "$schema" -F c matric_db > "${schema}_backup.dump"
done

# Clone archive (create 2025_copy from 2024)
pg_dump -n archive_2024 --schema-only | \
  sed 's/archive_2024/archive_2025_copy/g' | \
  psql matric_db

pg_dump -n archive_2024 --data-only --inserts | \
  sed 's/archive_2024/archive_2025_copy/g' | \
  psql matric_db
```

---

## 6. Alternative Patterns Evaluated

### 6.1 Row-Level Security (RLS)

**Pattern:**

```sql
CREATE TABLE note (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  content TEXT
);

ALTER TABLE note ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON note
  USING (tenant_id = current_setting('app.current_tenant')::uuid);
```

**Pros:**
- Single schema, single set of tables
- Transparent filtering at database level
- No application-level schema switching

**Cons:**
- Performance overhead on every query (RLS policy evaluation)
- Still shares indexes across all tenants
- No physical isolation (backup/restore is all-or-nothing)
- Difficult to debug policy issues
- Limited flexibility for per-tenant schema changes

**Verdict:** Not recommended for parallel archives. Better suited for true multi-tenant SaaS where data must coexist.

### 6.2 Separate Databases

**Pattern:**

```
- matric_db_archive_2024
- matric_db_archive_2025
- matric_db_archive_2026
```

**Pros:**
- Strongest isolation
- Independent resource management
- Easiest backup/restore

**Cons:**
- Connection overhead (separate pools per database)
- Cross-archive queries impossible without dblink/foreign data wrappers
- Increased operational complexity (N databases to monitor)
- Extension installation required per database

**Verdict:** Overkill for parallel archives. Use for completely independent deployments.

### 6.3 Table Prefixes

**Pattern:**

```sql
CREATE TABLE archive_2024_note (...);
CREATE TABLE archive_2024_embedding (...);
CREATE TABLE archive_2025_note (...);
CREATE TABLE archive_2025_embedding (...);
```

**Pros:**
- Simple to understand
- No schema switching needed

**Cons:**
- Namespace pollution (hundreds of tables in `\dt`)
- Cannot reuse same index names
- No isolation boundary
- Difficult to manage privileges
- Ugly, non-idiomatic SQL

**Verdict:** Anti-pattern. Avoid.

---

## 7. Comparison Matrix

| Feature | Row-Level Tenant | Schema-Per-Archive | RLS | Separate DBs |
|---------|------------------|-------------------|-----|--------------|
| **Data Isolation** | Logical only | Physical namespace | Logical only | Complete |
| **Backup/Restore** | All-or-nothing | Per-schema | All-or-nothing | Per-database |
| **Query Performance** | Good (with indexes) | Better (smaller indexes) | Slower (RLS overhead) | Best (isolated) |
| **Cross-Archive Queries** | Easy (same table) | Medium (qualified names) | Easy (same table) | Hard (FDW/dblink) |
| **Security** | Application-enforced | DB-enforced (privileges) | DB-enforced (policies) | DB-enforced (users) |
| **Operational Complexity** | Low | Medium | Low | High |
| **Schema Evolution** | Single migration | Per-schema migration | Single migration | Per-DB migration |
| **Connection Overhead** | None | None (shared pool) | None | High (pool per DB) |
| **Index Size** | Large (all tenants) | Small (per archive) | Large (all tenants) | Small (per DB) |
| **Flexibility** | Low | High | Low | Highest |
| **Recommended For** | True multi-tenant SaaS | Parallel archives | Compliance isolation | Independent deployments |

---

## 8. Implementation Checklist

For implementing schema-based parallel archives in matric-memory:

- [ ] **Phase 1: Foundation**
  - [ ] Create `SchemaContext` struct with `search_path` management
  - [ ] Add `Database::for_schema()` method
  - [ ] Add schema creation/listing utilities
  - [ ] Write tests for schema switching

- [ ] **Phase 2: Schema-Aware Repositories**
  - [ ] Create `SchemaAwareNoteRepository`
  - [ ] Create `SchemaAwareEmbeddingRepository`
  - [ ] Create `SchemaAwareLinkRepository`
  - [ ] Create `SchemaAwareCollectionRepository`
  - [ ] Implement transaction-level `SET LOCAL search_path`

- [ ] **Phase 3: API Integration**
  - [ ] Add `/archives` CRUD routes
  - [ ] Add `archive_id` parameter to existing routes
  - [ ] Implement archive listing/metadata endpoints
  - [ ] Add archive validation middleware

- [ ] **Phase 4: Migration & Testing**
  - [ ] Create archive schema migration script
  - [ ] Test cross-archive scenarios
  - [ ] Performance benchmark (search_path overhead)
  - [ ] Write integration tests

- [ ] **Phase 5: Documentation & Tooling**
  - [ ] Document archive management in CLAUDE.md
  - [ ] Create archive backup/restore scripts
  - [ ] Add archive monitoring to health checks
  - [ ] Update deployment documentation

---

## 9. Code Examples for matric-memory

### 9.1 Schema-Aware Repository Implementation

**crates/matric-db/src/repositories/schema_aware_note.rs:**

```rust
use sqlx::{Pool, Postgres};
use async_trait::async_trait;
use matric_core::{CreateNoteRequest, NoteFull, NoteRepository, Result};

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

    /// Set search_path and execute query within transaction
    async fn with_schema<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut sqlx::Transaction<'_, Postgres>) -> BoxFuture<'_, Result<T>>,
    {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

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
                let hash = compute_hash(&req.content);

                // All queries use unqualified table names
                // search_path ensures they resolve to correct schema
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

                // ... rest of insert logic ...

                Ok(note_id)
            })
        }).await
    }

    async fn fetch(&self, id: Uuid) -> Result<NoteFull> {
        self.with_schema(|tx| {
            Box::pin(async move {
                // Fetch note with schema-scoped query
                let note_row = sqlx::query(
                    "SELECT id, collection_id, format, source, created_at_utc, updated_at_utc,
                            starred, archived, last_accessed_at, title, metadata
                     FROM note WHERE id = $1"
                )
                .bind(id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?
                .ok_or_else(|| Error::NotFound(format!("Note {} not found", id)))?;

                // ... rest of fetch logic ...

                Ok(note_full)
            })
        }).await
    }

    // ... implement other NoteRepository methods ...
}
```

### 9.2 Archive Management API

**crates/matric-api/src/routes/archives.rs:**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{get, post, delete},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateArchiveRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct Archive {
    pub schema: String,
    pub name: String,
    pub description: Option<String>,
    pub note_count: i64,
    pub embedding_count: i64,
    pub created_at: DateTime<Utc>,
    pub size_bytes: Option<i64>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_archive).get(list_archives))
        .route("/:archive_id", get(get_archive).delete(delete_archive))
        .route("/:archive_id/stats", get(get_archive_stats))
}

async fn create_archive(
    State(db): State<Database>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<(StatusCode, Json<Archive>)> {
    // Validate name (alphanumeric + underscore only)
    if !req.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(Error::InvalidInput("Invalid archive name".into()));
    }

    let schema = format!("archive_{}", req.name);

    // Create schema and run migrations
    db.create_archive_schema(&schema).await?;

    // Store archive metadata
    let archive_id = new_v7();
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

    Ok((StatusCode::CREATED, Json(Archive {
        schema: schema.clone(),
        name: req.name,
        description: req.description,
        note_count: 0,
        embedding_count: 0,
        created_at: Utc::now(),
        size_bytes: None,
    })))
}

async fn list_archives(
    State(db): State<Database>,
) -> Result<Json<Vec<Archive>>> {
    let rows = sqlx::query(
        "SELECT ar.schema_name, ar.display_name, ar.description, ar.created_at_utc,
                (SELECT COUNT(*) FROM information_schema.tables
                 WHERE table_schema = ar.schema_name AND table_name = 'note') as has_tables
         FROM archive_registry ar
         ORDER BY ar.created_at_utc DESC"
    )
    .fetch_all(db.pool())
    .await
    .map_err(Error::Database)?;

    let mut archives = Vec::new();

    for row in rows {
        let schema: String = row.get("schema_name");

        // Get counts from schema
        let ctx = db.for_schema(&schema);
        let note_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM note")
            .fetch_one(ctx.pool())
            .await
            .unwrap_or(0);

        archives.push(Archive {
            schema: schema.clone(),
            name: row.get("display_name"),
            description: row.get("description"),
            note_count,
            embedding_count: 0, // TODO: query embedding count
            created_at: row.get("created_at_utc"),
            size_bytes: None, // TODO: query pg_total_relation_size
        });
    }

    Ok(Json(archives))
}

async fn delete_archive(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
) -> Result<StatusCode> {
    let schema = format!("archive_{}", archive_id);

    // Drop schema cascade
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema))
        .execute(db.pool())
        .await
        .map_err(Error::Database)?;

    // Remove from registry
    sqlx::query("DELETE FROM archive_registry WHERE schema_name = $1")
        .bind(&schema)
        .execute(db.pool())
        .await
        .map_err(Error::Database)?;

    Ok(StatusCode::NO_CONTENT)
}
```

---

## 10. References

### Documentation
- PostgreSQL 18 Documentation - Chapter 5.10: Schemas
  https://www.postgresql.org/docs/current/ddl-schemas.html

- PostgreSQL 18 Documentation - search_path Configuration
  https://www.postgresql.org/docs/current/runtime-config-client.html

- sqlx Documentation (v0.8.6)
  https://docs.rs/sqlx/0.8.6/sqlx/

### Files Analyzed
- `/path/to/fortemi/crates/matric-db/src/pool.rs`
- `/path/to/fortemi/crates/matric-db/src/lib.rs`
- `/path/to/fortemi/crates/matric-db/src/notes.rs`
- `/path/to/fortemi/crates/matric-core/src/strict_filter.rs`
- `/path/to/fortemi/crates/matric-db/src/unified_filter.rs`
- `/path/to/fortemi/migrations/20260102000000_initial_schema.sql`
- `/path/to/fortemi/Cargo.toml`

### Key Findings
1. matric-memory currently uses row-level `tenant_id` filtering
2. sqlx 0.8.6 supports PostgreSQL but lacks built-in multi-schema helpers
3. PostgreSQL schemas provide strong isolation with minimal overhead
4. Shared pool + per-transaction `SET LOCAL search_path` is the recommended pattern
5. Schema-per-archive enables independent backup/restore and schema evolution

---

## Conclusion

**Recommended Pattern:** Schema-based isolation with shared connection pool and per-transaction `search_path` switching.

**Implementation Effort:** Medium (2-3 days for core functionality)

**Benefits:**
- Strong data isolation between archives
- Independent backup/restore per archive
- Better query performance (smaller indexes)
- Flexible schema evolution per archive
- Minimal connection overhead

**Next Steps:**
1. Implement `SchemaContext` abstraction
2. Create schema-aware repository wrappers
3. Add archive management API endpoints
4. Write integration tests
5. Document usage in CLAUDE.md
