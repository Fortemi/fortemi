# Multi-Schema PostgreSQL Patterns - Quick Reference

**Date:** 2026-02-01
**Full Report:** `docs/research/postgresql-multi-schema-patterns.md`

---

## Executive Summary

**Current State:**
- matric-memory uses row-level multi-tenancy with `tenant_id` column
- Single schema (`public`) with all data in shared tables
- sqlx 0.8.6 with shared connection pool (10 connections)

**Recommendation:**
Use **schema-per-archive** pattern with shared pool and per-transaction `search_path` switching.

---

## Key Code Patterns

### 1. Schema Context Abstraction

```rust
// crates/matric-db/src/schema_context.rs

pub struct SchemaContext {
    pool: PgPool,
    schema: String,
}

impl SchemaContext {
    pub fn new(pool: PgPool, schema: impl Into<String>) -> Self {
        Self { pool, schema: schema.into() }
    }

    /// Execute within schema-specific transaction
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'a> FnOnce(&'a mut Transaction<'_, Postgres>)
            -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>> + Send,
    {
        let mut tx = self.pool.begin().await?;

        // Set search_path for this transaction only (SET LOCAL)
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", self.schema))
            .execute(&mut *tx)
            .await?;

        let result = f(&mut tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
```

### 2. Schema-Aware Repository Pattern

```rust
pub struct SchemaAwareNoteRepository {
    pool: Pool<Postgres>,
    schema: String,
}

impl SchemaAwareNoteRepository {
    pub fn new(pool: Pool<Postgres>, schema: impl Into<String>) -> Self {
        Self { pool, schema: schema.into() }
    }

    async fn insert(&self, req: CreateNoteRequest) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;

        // Set schema for this transaction
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", self.schema))
            .execute(&mut *tx)
            .await?;

        // Use unqualified table names - search_path handles routing
        let note_id = new_v7();
        sqlx::query(
            "INSERT INTO note (id, content, created_at_utc) VALUES ($1, $2, $3)"
        )
        .bind(note_id)
        .bind(&req.content)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(note_id)
    }
}
```

### 3. Database Extension for Schema Support

```rust
// crates/matric-db/src/lib.rs

impl Database {
    /// Get schema-specific context
    pub fn for_schema(&self, schema: &str) -> SchemaContext {
        SchemaContext::new(self.pool.clone(), schema)
    }

    /// Create new archive schema with migrations
    pub async fn create_archive_schema(&self, schema: &str) -> Result<()> {
        // Validate schema name
        if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::InvalidInput("Invalid schema name".into()));
        }

        let mut tx = self.pool.begin().await?;

        // Create schema
        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema))
            .execute(&mut *tx)
            .await?;

        // Set search_path for migration
        sqlx::query(&format!("SET LOCAL search_path TO {}, public", schema))
            .execute(&mut *tx)
            .await?;

        // Run migrations in this schema
        sqlx::migrate!("../../migrations")
            .run(&mut *tx)
            .await
            .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;

        tx.commit().await?;
        Ok(())
    }

    /// List all archive schemas
    pub async fn list_archive_schemas(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT schema_name FROM information_schema.schemata
             WHERE schema_name LIKE 'archive_%'
             ORDER BY schema_name"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.get("schema_name")).collect())
    }
}
```

### 4. API Handler Pattern

```rust
// API route handler
async fn create_note_in_archive(
    State(db): State<Database>,
    Path(archive_id): Path<String>,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<NoteResponse>> {
    // Construct schema name from archive ID
    let schema = format!("archive_{}", archive_id);

    // Get schema context
    let ctx = db.for_schema(&schema);

    // Use schema-aware repository
    let note_id = ctx.notes().insert(req).await?;

    Ok(Json(NoteResponse { id: note_id }))
}

// List archives
async fn list_archives(
    State(db): State<Database>,
) -> Result<Json<Vec<String>>> {
    let archives = db.list_archive_schemas().await?;
    Ok(Json(archives))
}
```

---

## PostgreSQL Schema Management

### Create Archive Schema

```sql
-- Create schema with owner
CREATE SCHEMA archive_2024 AUTHORIZATION matric_admin;

-- Grant usage to application user
GRANT USAGE ON SCHEMA archive_2024 TO matric;

-- Grant table permissions
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA archive_2024 TO matric;

-- Set default privileges for new tables
ALTER DEFAULT PRIVILEGES FOR ROLE matric_admin IN SCHEMA archive_2024
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO matric;
```

### Set Search Path

```sql
-- Session-level (until connection close)
SET search_path TO archive_2024, public;

-- Transaction-level (recommended - auto-resets after commit/rollback)
SET LOCAL search_path TO archive_2024, public;

-- Per-user default
ALTER ROLE matric SET search_path TO public;
```

### Query with Schema Qualification

```sql
-- Unqualified (uses search_path)
SELECT * FROM note WHERE id = '...';

-- Qualified (explicit schema)
SELECT * FROM archive_2024.note WHERE id = '...';

-- Cross-archive query
SELECT * FROM archive_2024.note
UNION ALL
SELECT * FROM archive_2025.note;
```

### Backup/Restore

```bash
# Backup single archive
pg_dump -n archive_2024 -F c matric_db > archive_2024.dump

# Restore single archive
pg_restore -n archive_2024 -d matric_db archive_2024.dump

# List schemas in dump
pg_restore -l archive_2024.dump | grep 'SCHEMA'

# Drop archive
DROP SCHEMA archive_2024 CASCADE;
```

---

## Performance Characteristics

| Operation | Overhead | Notes |
|-----------|----------|-------|
| `SET LOCAL search_path` | ~0.05-0.1ms | Per transaction |
| Schema switching | Negligible | Cached in connection state |
| Index lookup | Faster | Smaller per-schema indexes |
| Query planning | Faster | Fewer statistics to consider |
| Vacuum/Analyze | Parallelizable | Per-schema operations |

**Benchmark:** 10,000 `SET search_path` operations = ~500-1000ms total

---

## Connection Pool Strategy

**Recommended: Shared Pool with Dynamic Switching**

```
┌─────────────────────────────────────┐
│     Shared PgPool (10 connections)  │
│     All schemas use same pool       │
└──────────────┬──────────────────────┘
               │
    ┌──────────┴──────────┐
    │                     │
    ▼                     ▼
┌─────────────┐      ┌─────────────┐
│ archive_2024│      │ archive_2025│
│  (SET path) │      │  (SET path) │
└─────────────┘      └─────────────┘
```

**Advantages:**
- Single pool to manage
- No connection overhead
- Simple pool sizing (total concurrent requests)
- Works with existing matric-memory architecture

**Alternatives Rejected:**

1. **Pool-per-schema:** High memory overhead, complex management
2. **Separate databases:** Too isolated, hard to query across archives
3. **Row-level security:** Performance overhead, no physical isolation

---

## Security Best Practices

```sql
-- 1. Revoke public CREATE privilege
REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- 2. Create archives with admin ownership
CREATE SCHEMA archive_2024 AUTHORIZATION matric_admin;

-- 3. Grant minimal privileges to app user
GRANT USAGE ON SCHEMA archive_2024 TO matric;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA archive_2024 TO matric;

-- 4. Validate schema names in application
-- Only allow: alphanumeric + underscore
if !schema.chars().all(|c| c.is_alphanumeric() || c == '_') {
    return Err("Invalid schema name");
}
```

**Security Warning from PostgreSQL Docs:**

> "Adding a schema to search_path effectively trusts all users having CREATE
> privilege on that schema."

**Mitigation:** Ensure application user (`matric`) only has `USAGE` privilege, not `CREATE`.

---

## Comparison: Row-Level vs Schema-Based

| Feature | Row-Level (`tenant_id`) | Schema-Based |
|---------|------------------------|--------------|
| **Isolation** | Logical | Physical namespace |
| **Index Size** | Large (all data) | Small (per archive) |
| **Backup** | All-or-nothing | Per-archive |
| **Security** | App-enforced | DB-enforced |
| **Cross-Archive Query** | Easy (same table) | Medium (qualified) |
| **Schema Evolution** | Single migration | Per-schema |
| **Performance** | Good | Better |

---

## Implementation Roadmap

### Phase 1: Foundation (1-2 days)
- [ ] Create `SchemaContext` struct
- [ ] Add `Database::for_schema()` method
- [ ] Implement schema creation/listing
- [ ] Write unit tests

### Phase 2: Repositories (1-2 days)
- [ ] Create `SchemaAwareNoteRepository`
- [ ] Create `SchemaAwareEmbeddingRepository`
- [ ] Create `SchemaAwareLinkRepository`
- [ ] Implement transaction-level `SET LOCAL`

### Phase 3: API (1 day)
- [ ] Add `/archives` CRUD endpoints
- [ ] Add `archive_id` to existing routes
- [ ] Implement archive metadata storage
- [ ] Add validation middleware

### Phase 4: Testing (1 day)
- [ ] Integration tests for multi-schema
- [ ] Performance benchmarks
- [ ] Cross-archive query tests
- [ ] Backup/restore tests

**Total Effort:** ~4-6 days

---

## Key Takeaways

1. **Use schema-per-archive with shared pool** - Best balance of isolation and performance
2. **`SET LOCAL search_path`** - Transaction-scoped schema switching (auto-resets)
3. **Unqualified table names** - Let `search_path` handle routing
4. **Validate schema names** - Prevent SQL injection via schema names
5. **Independent backups** - `pg_dump -n schema_name` per archive
6. **Smaller indexes** - Per-schema data improves cache efficiency
7. **Security via privileges** - Use `GRANT USAGE`, not `CREATE`

---

## Files to Create/Modify

**New Files:**
```
crates/matric-db/src/
  ├── schema_context.rs              # SchemaContext implementation
  └── repositories/
      ├── schema_aware_note.rs       # Schema-aware note repository
      ├── schema_aware_embedding.rs  # Schema-aware embedding repository
      └── schema_aware_link.rs       # Schema-aware link repository

crates/matric-api/src/routes/
  └── archives.rs                    # Archive management API
```

**Modified Files:**
```
crates/matric-db/src/lib.rs          # Add for_schema(), create_archive_schema()
crates/matric-api/src/main.rs        # Register archive routes
```

---

## References

- Full research report: `/path/to/fortemi/docs/research/postgresql-multi-schema-patterns.md`
- PostgreSQL Schemas: https://www.postgresql.org/docs/current/ddl-schemas.html
- sqlx Documentation: https://docs.rs/sqlx/0.8.6/sqlx/
