# Multi-Memory Architecture Design

> **Status**: IMPLEMENTED (2026-02-08, commit dfbdeac)
> **Author**: Architecture Team
> **Date**: 2026-02-08
> **Epic**: #441 (Parallel Memory Archives)
> **Supersedes**: Current hardcoded `create_archive_tables()` in `crates/matric-db/src/archives.rs`

---

## Implementation Summary

Implementation is **COMPLETE** as of 2026-02-08. All 91 API handlers in `crates/matric-api/src/main.rs` route through `SchemaContext` for full schema isolation. Key implementation decisions:

- **`_tx` method pattern chosen** (Option A from ADR-068) over schema-scoped repositories (Option B). Each repository has parallel `_tx` methods accepting `&mut Transaction<'_, Postgres>`.
- **Two transaction patterns**: `execute()` for simple single-operation handlers (closure-based, automatic commit/rollback), and `begin_tx()` for complex handlers that need multiple repository calls or cannot move repos into closures (file_storage, analytics with loops).
- **Current limitation**: The standard hybrid search endpoint (`GET /api/v1/search`) rejects non-public archives. Federated search (`POST /api/v1/search/federated`) works across all memories.
- **Default archive with caching**: `DefaultArchiveCache` with 60-second TTL resolves the default archive when no `X-Fortemi-Memory` header is present. Falls back to `public` if no default is set.

See `docs/adr/ADR-068-archive-isolation-routing.md` for the full implementation status and test verification details.

---

## Table of Contents

1. [Overview and Goals](#1-overview-and-goals)
2. [Schema Design](#2-schema-design)
3. [Zero-Drift Schema Cloning](#3-zero-drift-schema-cloning)
4. [API Design](#4-api-design)
5. [MCP Integration](#5-mcp-integration)
6. [Memory-Scoped Backup and Restore](#6-memory-scoped-backup-and-restore)
7. [Migration Strategy for Existing Users](#7-migration-strategy-for-existing-users)
8. [Cross-Memory Search](#8-cross-memory-search)
9. [Error Handling and Edge Cases](#9-error-handling-and-edge-cases)
10. [Testing Strategy](#10-testing-strategy)
11. [Architectural Decision Records](#11-architectural-decision-records)
12. [Implementation Roadmap](#12-implementation-roadmap)

---

## 1. Overview and Goals

### Problem Statement

Fortemi's existing archive system suffers from **schema drift**: the `create_archive_tables()` function in
`crates/matric-db/src/archives.rs` uses hardcoded DDL that creates only 10 tables per archive schema, while
the public schema contains 25+ tables across 57 migrations. Every new migration that adds tables, columns,
indexes, or constraints to the public schema widens this gap. Archive schemas become second-class citizens
that cannot use SKOS concepts, file storage, ColBERT embeddings, webhooks, templates, or any feature added
after the initial archive implementation.

### What is a Memory?

A **memory** is a fully isolated PostgreSQL schema containing a complete set of user data tables. Each memory
operates independently: notes, embeddings, tags, collections, SKOS concepts, files, and all other per-user
data live within its schema boundary. Shared infrastructure (authentication, job queue, document type catalog,
migration tracking) lives in the `public` schema and is accessible to all memories via the PostgreSQL
`search_path`.

### Goals

| Goal | Description |
|------|-------------|
| **Full isolation** | Each memory is a complete, independent namespace |
| **Zero drift** | Archive schemas always match public schema structure |
| **Trivial backup** | `pg_dump --schema=X` captures an entire memory |
| **Trivial restore** | `DROP SCHEMA CASCADE` + `pg_restore` replaces a memory |
| **Session binding** | MCP sessions bind to a memory for the duration of a conversation |
| **Backward compatible** | Existing `/api/v1/archives/*` routes continue to work |
| **Manageable complexity** | No DDL duplication -- a table manifest replaces hardcoded SQL |

### Non-Goals

- Multi-tenant access control (memories are single-user or single-deployment)
- Cross-memory foreign keys (memories are fully independent)
- Automatic memory provisioning per OAuth client (future work)
- Real-time memory synchronization or replication

### High-Level Architecture

```
                                   +------------------+
                                   |   MCP Client     |
                                   | (Claude, etc.)   |
                                   +--------+---------+
                                            |
                                   X-Fortemi-Memory: "research"
                                            |
                                   +--------v---------+
                                   |   MCP Server     |
                                   | (mcp-server/)    |
                                   | Session state:   |
                                   | activeMemory     |
                                   +--------+---------+
                                            |
                                  Authorization: Bearer <token>
                                  X-Fortemi-Memory: research
                                            |
+---------------------------+      +--------v---------+
|     Nginx Reverse Proxy   | <--> |   Axum API       |
| :443 -> :3000 (API)       |      | (matric-api)     |
| :443/mcp -> :3001 (MCP)   |      +--------+---------+
+---------------------------+               |
                                   +--------v---------+
                                   | Archive Routing  |
                                   | Middleware        |
                                   | (resolve schema) |
                                   +--------+---------+
                                            |
                          SET LOCAL search_path TO memory_research, public
                                            |
                    +-----------------------------------------------+
                    |              PostgreSQL 18                     |
                    |  +----------+  +-----------+  +------------+  |
                    |  | public   |  | memory_   |  | memory_    |  |
                    |  | (shared) |  | default   |  | research   |  |
                    |  +----------+  +-----------+  +------------+  |
                    |  | _sqlx_   |  | note      |  | note       |  |
                    |  | migrat.  |  | embedding |  | embedding  |  |
                    |  | memory_  |  | tag       |  | tag        |  |
                    |  | registry |  | collection|  | collection |  |
                    |  | oauth_   |  | skos_*    |  | skos_*     |  |
                    |  | client   |  | file_*    |  | file_*     |  |
                    |  | api_key  |  | template  |  | template   |  |
                    |  | job_*    |  | webhook   |  | webhook    |  |
                    |  | doc_type |  | ...       |  | ...        |  |
                    |  | embed_   |  | (25 tbls) |  | (25 tbls)  |  |
                    |  | config   |  +-----------+  +------------+  |
                    |  | pke_pub  |                                  |
                    |  +----------+                                  |
                    +-----------------------------------------------+
```

---

## 2. Schema Design

### Shared Tables (always in `public` schema)

These tables contain global configuration, infrastructure, and data that is inherently cross-memory.
They are accessible from any memory schema via `search_path = memory_X, public`.

| Table | Rationale |
|-------|-----------|
| `_sqlx_migrations` | Migration tracking is global; each migration runs once |
| `memory_registry` | Registry of all memories (renamed from `archive_registry`) |
| `oauth_client` | OAuth clients authenticate globally, not per-memory |
| `api_key` | API keys are bound to OAuth clients, not memories |
| `document_type` | Catalog of 131 document types is shared reference data |
| `document_type_agentic_config` | Agentic config extends document types |
| `document_type_extraction_strategy` | Extraction strategies extend document types |
| `job_queue` | Job worker operates globally; jobs reference memory via payload |
| `job_history` | Historical job metrics are global |
| `pke_public_key` | Encryption key registry is global |
| `embedding_config` | Embedding model configurations are shared |

**Why these stay in public:** These tables either (a) track database-level state (`_sqlx_migrations`),
(b) provide shared reference data (`document_type`, `embedding_config`), or (c) manage cross-cutting
infrastructure (`job_queue`, `oauth_client`). Duplicating them per memory would create update anomalies
and waste storage.

### Per-Memory Tables (cloned into each memory schema)

Every memory schema contains an identical copy of these table *structures* (not data).
This is the **table manifest** -- the single source of truth replacing hardcoded DDL.

```rust
/// Tables cloned into each memory schema.
/// Order matters: tables with foreign key dependencies must come after
/// their referenced tables.
pub const MEMORY_TABLE_MANIFEST: &[&str] = &[
    // Core
    "note",
    "note_original",
    "note_revised_current",
    "note_revision",
    "note_share_grant",
    // Organization
    "collection",
    "tag",
    "note_tag",
    // Search & Linking
    "embedding",
    "embedding_set",
    "link",
    "colbert_embedding",
    // Provenance
    "provenance_edge",
    "activity_log",
    // SKOS Taxonomy
    "skos_concept_scheme",
    "skos_concept",
    "skos_relation",
    "skos_collection",
    "skos_label",
    "skos_note",
    // Files
    "file_storage",
    "attachment",
    "file_upload_audit",
    // Templates
    "template",
    // Webhooks
    "webhook",
    // User Preferences
    "user_metadata_label",
    "user_config",
];
```

**Count: 25 tables** per memory schema.

### Memory Registry Schema

The `memory_registry` table replaces `archive_registry` with additional fields for the
multi-memory lifecycle:

```sql
CREATE TABLE IF NOT EXISTS memory_registry (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL UNIQUE,
    schema_name     TEXT NOT NULL UNIQUE,
    description     TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed   TIMESTAMPTZ,
    note_count      INTEGER DEFAULT 0,
    size_bytes      BIGINT DEFAULT 0,
    is_default      BOOLEAN DEFAULT FALSE,
    -- New fields for multi-memory
    schema_version  INTEGER NOT NULL DEFAULT 0,   -- tracks last applied migration
    created_by      TEXT,                          -- OAuth client or user who created it
    max_notes       INTEGER,                       -- optional per-memory note limit
    locked          BOOLEAN DEFAULT FALSE          -- prevent writes during backup/restore
);

-- Only one default memory is allowed
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_registry_default
    ON memory_registry(is_default) WHERE is_default = TRUE;
```

### Schema Naming Convention

Memory schemas follow the pattern `memory_{sanitized_name}`:

```
User input:  "research-2026"   -> schema: memory_research_2026
User input:  "Project Alpha"   -> schema: memory_project_alpha
User input:  "default"         -> schema: memory_default (or public, see migration)
```

The `generate_schema_name()` function sanitizes input by:
1. Converting to lowercase
2. Replacing hyphens and spaces with underscores
3. Prefixing with `memory_` (changed from `archive_`)
4. Validating against `validate_schema_name()` (63-char PG identifier limit)

---

## 3. Zero-Drift Schema Cloning

### The Core Problem

The current `create_archive_tables()` contains ~230 lines of hardcoded DDL defining 10 tables.
It was written once and never updated as 47 subsequent migrations added tables, columns, indexes,
and constraints to the public schema. Every archive created today is missing:

- `note_share_grant` (sharing grants)
- `embedding_set` (embedding set management)
- `colbert_embedding` (ColBERT multi-vector embeddings)
- `provenance_edge` (content provenance)
- All 6 SKOS tables (semantic tagging)
- `file_storage`, `attachment`, `file_upload_audit` (file management)
- `template` (note templates)
- `webhook` (event webhooks)
- `user_metadata_label`, `user_config` (user preferences)
- Numerous columns added to existing tables by migrations
- All indexes added after the initial archive implementation

### Solution: `CREATE TABLE ... (LIKE ... INCLUDING ALL)`

Instead of maintaining DDL, clone table structure from the public schema at creation time:

```sql
CREATE TABLE new_schema.table_name (LIKE public.table_name INCLUDING ALL);
```

The `INCLUDING ALL` clause copies:

| Included | Description |
|----------|-------------|
| `INCLUDING DEFAULTS` | Column DEFAULT expressions |
| `INCLUDING CONSTRAINTS` | CHECK constraints, NOT NULL |
| `INCLUDING INDEXES` | All indexes (B-tree, GIN, GiST, IVFFlat) |
| `INCLUDING STORAGE` | TOAST storage parameters |
| `INCLUDING COMMENTS` | Column and table comments |
| `INCLUDING GENERATED` | Generated columns (like `tsv tsvector`) |
| `INCLUDING STATISTICS` | Extended statistics objects |

### What `LIKE` Does NOT Copy

| Not Copied | Solution |
|------------|----------|
| Foreign keys | Query `information_schema.table_constraints` and recreate with remapped schema |
| Triggers | Query `pg_trigger` and recreate pointing to shared functions |
| Sequences | Created automatically with `SERIAL`/`GENERATED` columns |
| Policies (RLS) | Not currently used; add if needed |
| Text search configs | Clone from `pg_ts_config` if custom configs exist |

### Implementation: Schema Cloner

```rust
/// Clone all per-memory tables from public schema into a new memory schema.
///
/// Uses `CREATE TABLE ... (LIKE ... INCLUDING ALL)` to avoid hardcoded DDL.
/// Foreign keys are introspected and remapped to the new schema.
pub async fn clone_schema_tables(
    pool: &PgPool,
    schema_name: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // 1. Create the schema
    sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
        .execute(&mut *tx).await?;

    // 2. Clone each table structure (order matters for FK dependencies)
    for table in MEMORY_TABLE_MANIFEST {
        let sql = format!(
            "CREATE TABLE {schema}.{table} (LIKE public.{table} INCLUDING ALL)",
            schema = schema_name,
            table = table,
        );
        sqlx::query(&sql).execute(&mut *tx).await?;
    }

    // 3. Introspect and recreate foreign keys with remapped schema
    let fk_rows = sqlx::query_as::<_, ForeignKeyInfo>(
        r#"
        SELECT
            tc.table_name,
            kcu.column_name,
            ccu.table_name AS referenced_table,
            ccu.column_name AS referenced_column,
            tc.constraint_name,
            rc.delete_rule
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
            ON tc.constraint_name = ccu.constraint_name
            AND tc.table_schema = ccu.table_schema
        JOIN information_schema.referential_constraints rc
            ON tc.constraint_name = rc.constraint_name
            AND tc.table_schema = rc.constraint_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
            AND tc.table_schema = 'public'
            AND tc.table_name = ANY($1)
        "#,
    )
    .bind(&MEMORY_TABLE_MANIFEST.to_vec())
    .fetch_all(&mut *tx)
    .await?;

    for fk in &fk_rows {
        // Only remap if the referenced table is in the manifest
        // (tables referencing shared public tables keep their cross-schema FK)
        let ref_schema = if MEMORY_TABLE_MANIFEST.contains(&fk.referenced_table.as_str()) {
            schema_name
        } else {
            "public"
        };

        let on_delete = match fk.delete_rule.as_str() {
            "CASCADE" => "ON DELETE CASCADE",
            "SET NULL" => "ON DELETE SET NULL",
            "SET DEFAULT" => "ON DELETE SET DEFAULT",
            "RESTRICT" => "ON DELETE RESTRICT",
            _ => "ON DELETE NO ACTION",
        };

        let fk_name = format!(
            "fk_{}_{}_{}", schema_name, fk.table_name, fk.column_name
        );

        let sql = format!(
            "ALTER TABLE {schema}.{table} ADD CONSTRAINT {fk_name} \
             FOREIGN KEY ({column}) REFERENCES {ref_schema}.{ref_table}({ref_column}) \
             {on_delete}",
            schema = schema_name,
            table = fk.table_name,
            fk_name = fk_name,
            column = fk.column_name,
            ref_schema = ref_schema,
            ref_table = fk.referenced_table,
            ref_column = fk.referenced_column,
            on_delete = on_delete,
        );
        sqlx::query(&sql).execute(&mut *tx).await?;
    }

    // 4. Recreate triggers (shared trigger functions live in public)
    let trigger_rows = sqlx::query_as::<_, TriggerInfo>(
        r#"
        SELECT
            t.tgname AS trigger_name,
            c.relname AS table_name,
            p.proname AS function_name,
            pg_get_triggerdef(t.oid) AS trigger_def
        FROM pg_trigger t
        JOIN pg_class c ON t.tgrelid = c.oid
        JOIN pg_namespace n ON c.relnamespace = n.oid
        JOIN pg_proc p ON t.tgfoid = p.oid
        WHERE n.nspname = 'public'
            AND NOT t.tgisinternal
            AND c.relname = ANY($1)
        "#,
    )
    .bind(&MEMORY_TABLE_MANIFEST.to_vec())
    .fetch_all(&mut *tx)
    .await?;

    for trigger in &trigger_rows {
        // Rewrite trigger definition to target new schema
        let remapped = trigger.trigger_def
            .replace(
                &format!("ON public.{}", trigger.table_name),
                &format!("ON {}.{}", schema_name, trigger.table_name),
            );
        sqlx::query(&remapped).execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}
```

### Schema Version Tracking

Each memory records the `schema_version` corresponding to the number of migrations applied
when it was created (or last upgraded). This enables:

1. **Drift detection**: Compare `memory.schema_version` to the current migration count
2. **Upgrade path**: If a memory is behind, re-clone missing tables or alter existing ones
3. **Health checks**: The `/api/v1/memories/:name` endpoint reports version status

### Upgrade Strategy for Existing Memories

When a new migration adds a table to the public schema:

1. The migration also inserts the table name into a `schema_changes` tracking table
2. On startup, a reconciliation process checks all memory schemas against the manifest
3. Missing tables are cloned via `CREATE TABLE ... (LIKE ... INCLUDING ALL)`
4. New columns on existing tables are added via `ALTER TABLE ... ADD COLUMN`
5. The `schema_version` is bumped

This runs as part of the standard `Database::migrate()` startup path, gated behind the
`migrations` feature flag.

---

## 4. API Design

### Memory Management Endpoints

All endpoints live under `/api/v1/memories`. The existing `/api/v1/archives` routes become
aliases for backward compatibility.

```
POST   /api/v1/memories                 Create a new memory
GET    /api/v1/memories                 List all memories
GET    /api/v1/memories/:name           Get memory details
PATCH  /api/v1/memories/:name           Update memory metadata
DELETE /api/v1/memories/:name           Delete memory (DROP SCHEMA CASCADE)
POST   /api/v1/memories/:name/set-default   Set as default memory
GET    /api/v1/memories/:name/stats     Get memory statistics
POST   /api/v1/memories/:name/clone     Clone memory to new name
POST   /api/v1/memories/:name/backup    Export memory (pg_dump --schema)
POST   /api/v1/memories/:name/restore   Restore memory from backup
```

### Per-Request Memory Selection

```
                    +-------------------+
                    | Incoming Request  |
                    +--------+----------+
                             |
                    +--------v----------+
                    | Has X-Fortemi-    |
                    | Memory header?    |
                    +--------+----------+
                        |          |
                       yes         no
                        |          |
              +---------v---+  +---v-----------+
              | Resolve     |  | Use default   |
              | named memory|  | memory from   |
              | from        |  | cache (60s    |
              | registry    |  | TTL)          |
              +------+------+  +---+-----------+
                     |             |
                     |      +------v------+
                     |      | Default set?|
                     |      +------+------+
                     |         |        |
                     |        yes       no
                     |         |        |
                     |    +----v--+  +--v-------+
                     |    |default|  | "public" |
                     |    |archive|  | schema   |
                     |    +---+---+  +--+-------+
                     |        |         |
              +------v--------v---------v---+
              | Validate schema exists      |
              | in PostgreSQL               |
              +----------+------------------+
                         |
              +----------v------------------+
              | Auto-migrate if outdated    |
              | (sync_archive_schema)       |
              +----------+------------------+
                         |
              +----------v------------------+
              | Inject ArchiveContext        |
              | { schema, is_default }      |
              | into request extensions     |
              +----------+------------------+
                         |
              +----------v------------------+
              | Handler executes with       |
              | SET LOCAL search_path       |
              | TO memory_X, public         |
              +-----------------------------+
```

**Header format:**

```
X-Fortemi-Memory: research-2026
```

**Resolution rules:**
1. If `X-Fortemi-Memory` header is present, look up the memory by name in `memory_registry`
2. If not found, return `404 Not Found` with message "Memory 'X' not found"
3. If header is absent, use the cached default memory (`DefaultArchiveCache`, 60-second TTL)
4. If no default memory is set, fall back to `public` schema

**Middleware implementation** (`crates/matric-api/src/middleware/archive_routing.rs`):

```rust
pub async fn archive_routing_middleware(
    State(state): State<AppState>,
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Check for explicit memory selection header
    let ctx = if let Some(memory_name) = req.headers()
        .get("x-fortemi-memory")
        .and_then(|v| v.to_str().ok())
    {
        resolve_named_memory(&state, memory_name).await
    } else {
        resolve_archive_context(&state).await
    };

    match ctx {
        Ok(ctx) => {
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        Err(e) => e.into_response(),
    }
}
```

### Handler Transaction Patterns (Implemented)

All 91 handlers use one of two patterns:

**Pattern 1: `execute()` -- Simple operations (most handlers)**

```rust
async fn create_note(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Json(body): Json<CreateNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let repo = PgNoteRepository::new(state.db.pool.clone());
    let result = ctx.execute(move |tx| Box::pin(async move {
        repo.insert_tx(tx, req).await
    })).await?;
    // ...
}
```

**Pattern 2: `begin_tx()` -- Complex operations (file_storage, analytics, loops)**

```rust
async fn list_attachments(
    State(state): State<AppState>,
    Extension(archive_ctx): Extension<ArchiveContext>,
    Path(note_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let ctx = state.db.for_schema(&archive_ctx.schema)?;
    let mut tx = ctx.begin_tx().await?;
    let files = file_storage.list_by_note_tx(&mut tx, note_id).await?;
    tx.commit().await.map_err(|e| ApiError::Internal(e.to_string()))?;
    // ...
}
```

### Request and Response Types

**Create Memory Request:**
```json
{
    "name": "research-2026",
    "description": "Research notes for 2026 projects"
}
```

**Create Memory Response (201 Created):**
```json
{
    "id": "019478a3-...",
    "name": "research-2026",
    "schema_name": "memory_research_2026",
    "description": "Research notes for 2026 projects",
    "created_at": "2026-02-08T12:00:00Z",
    "schema_version": 57,
    "note_count": 0,
    "size_bytes": 0,
    "is_default": false
}
```

**Memory Info Response (GET /api/v1/memories/:name):**
```json
{
    "id": "019478a3-...",
    "name": "research-2026",
    "schema_name": "memory_research_2026",
    "description": "Research notes for 2026 projects",
    "created_at": "2026-02-08T12:00:00Z",
    "last_accessed": "2026-02-08T14:30:00Z",
    "schema_version": 57,
    "note_count": 342,
    "size_bytes": 15728640,
    "is_default": false,
    "table_count": 25,
    "drift_status": "current"
}
```

The `drift_status` field reports:
- `"current"` -- schema_version matches latest migration count
- `"behind"` -- memory needs upgrade (schema_version < current)
- `"unknown"` -- unable to determine (schema introspection failed)

### Clone Memory

`POST /api/v1/memories/:name/clone`

```json
{
    "target_name": "research-2026-backup",
    "description": "Snapshot of research before restructuring"
}
```

Implementation approach:

```sql
-- For each table in MEMORY_TABLE_MANIFEST:
CREATE TABLE memory_target.table_name
    (LIKE memory_source.table_name INCLUDING ALL);

INSERT INTO memory_target.table_name
    SELECT * FROM memory_source.table_name;
```

This is an online operation that does not require downtime. For large memories,
the clone endpoint returns a job ID and processes asynchronously via the job queue.

### Backward Compatibility Aliases

```rust
// In main.rs router setup:
// New memory routes
.route("/api/v1/memories", get(list_memories).post(create_memory))
.route("/api/v1/memories/:name", get(get_memory).patch(update_memory).delete(delete_memory))
.route("/api/v1/memories/:name/set-default", post(set_default_memory))
.route("/api/v1/memories/:name/stats", get(get_memory_stats))
.route("/api/v1/memories/:name/clone", post(clone_memory))
.route("/api/v1/memories/:name/backup", post(backup_memory))
.route("/api/v1/memories/:name/restore", post(restore_memory))

// Backward-compatible aliases (same handlers)
.route("/api/v1/archives", get(list_memories).post(create_memory))
.route("/api/v1/archives/:name", get(get_memory).patch(update_memory).delete(delete_memory))
.route("/api/v1/archives/:name/set-default", post(set_default_memory))
.route("/api/v1/archives/:name/stats", get(get_memory_stats))
```

---

## 5. MCP Integration

### Session-Level Memory Selection

The MCP server (`mcp-server/index.js`) already tracks sessions via `mcp-session-id` headers
and a per-connection `Map`. The multi-memory feature adds an `activeMemory` field to session
state.

```
+-------------------+       +-------------------+       +-------------------+
| MCP Session A     |       | MCP Session B     |       | MCP Session C     |
| activeMemory:     |       | activeMemory:     |       | activeMemory:     |
|   "research"      |       |   "personal"      |       |   null (default)  |
+--------+----------+       +--------+----------+       +--------+----------+
         |                           |                            |
         v                           v                            v
  X-Fortemi-Memory:           X-Fortemi-Memory:          (no header, uses
    research                    personal                   default memory)
```

### New MCP Tools

**`switch_memory`** -- Set the active memory for the current MCP session.

```json
{
    "name": "switch_memory",
    "description": "Switch to a different memory for this conversation. All subsequent operations (notes, search, tags, etc.) will operate within the selected memory. Use list_archives to see available memories.",
    "inputSchema": {
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Name of the memory to switch to"
            }
        },
        "required": ["name"]
    }
}
```

**`get_current_memory`** -- Show which memory is active in the current session.

```json
{
    "name": "get_current_memory",
    "description": "Show which memory is currently active for this conversation session.",
    "inputSchema": {
        "type": "object",
        "properties": {}
    }
}
```

### Implementation in `index.js`

```javascript
// Per-session state (already exists for session tracking)
const sessionState = new Map(); // sessionId -> { activeMemory: string | null }

// In the tool handler switch:
case "switch_memory": {
    const sessionId = tokenStorage.getStore()?.sessionId;
    const state = sessionState.get(sessionId) || {};

    // Validate memory exists via API
    const memoryInfo = await apiRequest("GET", `/api/v1/memories/${args.name}`);

    state.activeMemory = args.name;
    sessionState.set(sessionId, state);

    result = {
        success: true,
        active_memory: args.name,
        message: `Switched to memory "${args.name}". All operations now target this memory.`
    };
    break;
}

case "get_current_memory": {
    const sessionId = tokenStorage.getStore()?.sessionId;
    const state = sessionState.get(sessionId) || {};

    result = {
        active_memory: state.activeMemory || null,
        message: state.activeMemory
            ? `Currently using memory "${state.activeMemory}"`
            : "Using default memory (no explicit memory selected)"
    };
    break;
}
```

### Header Injection

The `apiRequest()` helper function is modified to inject the `X-Fortemi-Memory` header
when a session has an active memory:

```javascript
async function apiRequest(method, path, body = null) {
    const url = `${API_BASE}${path}`;
    const headers = { "Content-Type": "application/json" };

    // Auth header (existing)
    const sessionToken = tokenStorage.getStore()?.token;
    if (sessionToken) {
        headers["Authorization"] = `Bearer ${sessionToken}`;
    } else if (API_KEY) {
        headers["Authorization"] = `Bearer ${API_KEY}`;
    }

    // Memory header (new)
    const sessionId = tokenStorage.getStore()?.sessionId;
    const state = sessionState.get(sessionId);
    if (state?.activeMemory) {
        headers["X-Fortemi-Memory"] = state.activeMemory;
    }

    // ... rest of fetch logic
}
```

All existing MCP tools automatically inherit the active memory without any changes to their
individual implementations. The `create_note`, `search_notes`, `list_notes`, and every other
tool will operate within the selected memory transparently.

### Enhanced `memory_info` Tool

The existing `memory_info` tool is enhanced to include per-memory breakdowns:

```json
{
    "system": {
        "total_memories": 3,
        "total_notes": 1247,
        "total_size_bytes": 52428800
    },
    "current_memory": {
        "name": "research",
        "note_count": 342,
        "size_bytes": 15728640,
        "schema_version": 57,
        "drift_status": "current"
    },
    "memories": [
        { "name": "default", "note_count": 800, "size_bytes": 31457280, "is_default": true },
        { "name": "research", "note_count": 342, "size_bytes": 15728640, "is_default": false },
        { "name": "archive-2025", "note_count": 105, "size_bytes": 5242880, "is_default": false }
    ]
}
```

---

## 6. Memory-Scoped Backup and Restore

### Current Problems

The existing backup/restore system operates on the entire database. Restoring a single
memory requires careful enumeration of object types and manual conflict resolution. The
schema-per-memory architecture eliminates this entirely.

### Backup

**Endpoint:** `POST /api/v1/memories/:name/backup`

**Implementation:**

```bash
pg_dump \
    --schema=memory_research \
    --format=custom \
    --file=/backups/memory_research_20260208.dump \
    "$DATABASE_URL"
```

The `--schema` flag captures everything within the memory schema:
- All tables and their data
- All indexes
- All constraints
- All triggers
- All sequences
- All table comments

**What is NOT captured (by design):**
- Shared tables in `public` (auth, jobs, document types)
- Other memory schemas
- Database roles and permissions

**Response:**

```json
{
    "backup_id": "019478a3-...",
    "memory_name": "research",
    "filename": "memory_research_20260208_143000.dump",
    "size_bytes": 15728640,
    "created_at": "2026-02-08T14:30:00Z",
    "tables_included": 25,
    "note_count": 342
}
```

### Restore

**Endpoint:** `POST /api/v1/memories/:name/restore`

```json
{
    "backup_id": "019478a3-...",
    "strategy": "replace"
}
```

**Strategy `replace` (default):**

```
+-------------------+     +-------------------+     +-------------------+
| 1. Lock memory    |     | 2. DROP SCHEMA    |     | 3. pg_restore     |
|    in registry    | --> |    CASCADE        | --> |    --schema=...   |
| (locked = true)   |     | (atomic drop)     |     | (atomic restore)  |
+-------------------+     +-------------------+     +-------------------+
         |
         v
+-------------------+     +-------------------+
| 4. REINDEX SCHEMA | --> | 5. ANALYZE        |
| (rebuild indexes) |     | (update stats)    |
+-------------------+     +-------------------+
         |
         v
+-------------------+
| 6. Unlock memory  |
| (locked = false)  |
+-------------------+
```

**Implementation steps:**

```sql
-- 1. Lock the memory to prevent concurrent writes
UPDATE memory_registry SET locked = true WHERE name = 'research';

-- 2. Drop the existing schema (all objects gone instantly)
DROP SCHEMA memory_research CASCADE;

-- 3. Restore from backup (run via command)
-- pg_restore --schema=memory_research --dbname=$DATABASE_URL backup.dump

-- 4. Rebuild all indexes for fresh statistics
REINDEX SCHEMA memory_research;

-- 5. Update table statistics for query planner
ANALYZE memory_research.note;
ANALYZE memory_research.embedding;
-- ... (for each table)

-- 6. Unlock the memory
UPDATE memory_registry SET locked = false WHERE name = 'research';
```

**Strategy `merge` (future work):**
Restore into a temporary schema, then merge data into the existing memory.
This is significantly more complex and deferred to a future release.

### Advantages over Current Backup

| Aspect | Current (whole-DB) | Multi-Memory (per-schema) |
|--------|-------------------|---------------------------|
| Backup scope | Entire database | Single memory |
| Backup size | All memories + shared | One memory only |
| Restore time | Minutes (full DB) | Seconds (one schema) |
| Restore risk | Overwrites everything | Isolated to one memory |
| Concurrent use | Database offline | Other memories unaffected |
| Complexity | Complex object enumeration | `pg_dump/pg_restore --schema` |

---

## 7. Migration Strategy for Existing Users

### Phase 1: Backward-Compatible Introduction

This phase adds the new memory infrastructure without breaking existing deployments.

**Migration: Rename `archive_registry` to `memory_registry`**

```sql
-- Rename table (preserves all data, indexes, constraints)
ALTER TABLE archive_registry RENAME TO memory_registry;

-- Add new columns
ALTER TABLE memory_registry ADD COLUMN IF NOT EXISTS schema_version INTEGER DEFAULT 0;
ALTER TABLE memory_registry ADD COLUMN IF NOT EXISTS created_by TEXT;
ALTER TABLE memory_registry ADD COLUMN IF NOT EXISTS max_notes INTEGER;
ALTER TABLE memory_registry ADD COLUMN IF NOT EXISTS locked BOOLEAN DEFAULT FALSE;

-- Rename indexes for consistency
ALTER INDEX idx_archive_registry_name RENAME TO idx_memory_registry_name;
ALTER INDEX idx_archive_registry_default RENAME TO idx_memory_registry_default;
ALTER INDEX idx_archive_registry_last_accessed RENAME TO idx_memory_registry_last_accessed;

-- Update the seed default entry
UPDATE memory_registry SET schema_version = (
    SELECT COUNT(*) FROM _sqlx_migrations
) WHERE name = 'default';
```

**API routes:** Both `/api/v1/archives/*` and `/api/v1/memories/*` work, pointing to the
same handlers. The `ArchiveContext` struct is kept internally (renaming it would touch dozens
of files for no functional benefit).

**Schema handling:** The public schema continues to serve as the `default` memory. Existing
archives (if any) continue to work with their current table set. New memories created after
this migration use the zero-drift cloner.

**What stays the same:**
- `SchemaContext` with `SET LOCAL search_path` (proven pattern)
- TTL-based default archive cache (60-second TTL via `DefaultArchiveCache`)
- `ArchiveContext` in request extensions
- All existing queries (they reference unqualified table names resolved via `search_path`)

### Phase 2: Existing Archive Upgrade (Optional, Admin-Initiated)

For deployments that created archives under the old hardcoded DDL system, an upgrade
command brings them to parity with public.

**Endpoint:** `POST /api/v1/memories/:name/upgrade`

**Process:**

```
For each table in MEMORY_TABLE_MANIFEST:
    if table does NOT exist in memory schema:
        CREATE TABLE memory_X.table (LIKE public.table INCLUDING ALL)
        Recreate foreign keys (remapped)
    else:
        Compare columns between public.table and memory_X.table
        ADD COLUMN for any missing columns
        Note: column type changes are logged but not auto-applied (requires manual review)

Update memory_registry SET schema_version = current_migration_count
```

This is safe to run multiple times (idempotent) and does not modify existing data.

### Phase 3: Public Schema Separation (Future, Optional)

In a future release, the public schema user data can be moved into a dedicated `memory_default`
schema, leaving `public` as a pure shared-infrastructure namespace. This is **not required** for
multi-memory to work and is deferred because:

1. It requires migrating potentially large amounts of data
2. All existing queries work correctly with `search_path = public` (tables resolve in public)
3. It can be done incrementally when the deployment is ready

**Process (when ready):**

```sql
-- Create the default memory schema
CREATE SCHEMA memory_default;

-- For each per-memory table:
ALTER TABLE public.note SET SCHEMA memory_default;
-- (repeat for all 25 tables)

-- Update registry
UPDATE memory_registry
SET schema_name = 'memory_default'
WHERE name = 'default';
```

---

## 8. Cross-Memory Search

### Endpoint

`POST /api/v1/search/global`

```json
{
    "query": "transformer architecture attention mechanism",
    "mode": "hybrid",
    "limit": 20,
    "memories": ["research", "personal"]
}
```

If `memories` is omitted, all memories are searched. If specified, only the listed
memories are searched.

### Current Limitation

The standard hybrid search endpoint (`GET /api/v1/search`) currently rejects requests for non-public archives:

```rust
if archive_ctx.schema != "public" {
    return Err(ApiError::BadRequest(
        "Search not yet supported for non-default archives".to_string(),
    ));
}
```

This is because the `HybridSearchEngine` operates directly on the connection pool and does not yet support `SchemaContext`-based scoping. Federated search (`POST /api/v1/search/federated`) works across all memories using dynamically-built schema-qualified queries.

### Implementation

The cross-memory search dynamically builds a `UNION ALL` query across memory schemas:

```sql
-- Generated dynamically based on memory_registry entries
(
    SELECT 'research' AS memory_name, n.id, n.title, nrc.content,
           ts_rank(nrc.tsv, websearch_to_tsquery('english', $1)) AS fts_score
    FROM memory_research.note n
    JOIN memory_research.note_revised_current nrc ON n.id = nrc.note_id
    WHERE nrc.tsv @@ websearch_to_tsquery('english', $1)
      AND n.soft_deleted = false
)
UNION ALL
(
    SELECT 'personal' AS memory_name, n.id, n.title, nrc.content,
           ts_rank(nrc.tsv, websearch_to_tsquery('english', $1)) AS fts_score
    FROM memory_personal.note n
    JOIN memory_personal.note_revised_current nrc ON n.id = nrc.note_id
    WHERE nrc.tsv @@ websearch_to_tsquery('english', $1)
      AND n.soft_deleted = false
)
ORDER BY fts_score DESC
LIMIT $2
```

For semantic search, the same pattern applies to the `embedding` table:

```sql
(
    SELECT 'research' AS memory_name, e.note_id, e.text,
           1 - (e.vector <=> $1::vector) AS similarity
    FROM memory_research.embedding e
    WHERE 1 - (e.vector <=> $1::vector) > 0.3
)
UNION ALL
(
    SELECT 'personal' AS memory_name, e.note_id, e.text,
           1 - (e.vector <=> $1::vector) AS similarity
    FROM memory_personal.embedding e
    WHERE 1 - (e.vector <=> $1::vector) > 0.3
)
ORDER BY similarity DESC
LIMIT $2
```

### Response Format

```json
{
    "results": [
        {
            "memory_name": "research",
            "note_id": "019478a3-...",
            "title": "Attention Is All You Need - Summary",
            "snippet": "The transformer architecture replaces recurrence with...",
            "score": 0.92,
            "tags": ["ai/transformers", "papers"]
        },
        {
            "memory_name": "personal",
            "note_id": "019478b7-...",
            "title": "ML Study Notes",
            "snippet": "Self-attention mechanism allows the model to...",
            "score": 0.87,
            "tags": ["study", "ml"]
        }
    ],
    "memories_searched": ["research", "personal"],
    "total_results": 2
}
```

### Performance Considerations

- Cross-memory search queries N schemas (one per memory). For deployments with many
  memories (>10), consider:
  - Parallel query execution using `unnest` + lateral joins
  - Limiting to specific memories via the `memories` parameter
  - Adding a `max_memories` config (default: 20) to prevent runaway queries
- Each memory's indexes are fully independent, so per-memory search performance is
  unaffected by the number of memories
- The query planner handles UNION ALL efficiently when each branch uses its own indexes

### MCP Tool

```json
{
    "name": "search_all_memories",
    "description": "Search across all memories simultaneously. Returns results tagged with their source memory.",
    "inputSchema": {
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Search query" },
            "limit": { "type": "number", "default": 20 },
            "mode": { "type": "string", "enum": ["hybrid", "fts", "semantic"] },
            "memories": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Specific memories to search (omit for all)"
            }
        },
        "required": ["query"]
    }
}
```

---

## 9. Error Handling and Edge Cases

### Memory Name Validation

Memory names are validated at the API layer before reaching the database:

| Rule | Constraint | Error |
|------|-----------|-------|
| Non-empty | `name.trim().len() > 0` | 400: "Memory name cannot be empty" |
| Max length | User name <= 50 chars | 400: "Memory name exceeds 50 characters" |
| PG identifier | Generated schema name <= 63 chars | 400: "Generated schema name too long" |
| Characters | `[a-zA-Z0-9_-]` (user-facing) | 400: "Memory name contains invalid characters" |
| No reserved | Not `public`, `pg_catalog`, etc. | 400: "Memory name 'X' is reserved" |
| Unique | UNIQUE constraint on registry | 409: "Memory 'X' already exists" |

The schema name is generated by `generate_schema_name()` which sanitizes the user-facing
name (lowercasing, replacing hyphens with underscores, prefixing `memory_`).

### Default Memory Protection

```rust
pub async fn delete_memory(...) -> Result<StatusCode, ApiError> {
    let memory = state.db.memories.get_by_name(&name).await?
        .ok_or(ApiError::NotFound(...))?;

    if memory.is_default {
        return Err(ApiError::BadRequest(
            "Cannot delete the default memory. Set another memory as default first.".into()
        ));
    }

    // Proceed with DROP SCHEMA CASCADE
    ...
}
```

### Concurrent Memory Creation

The `UNIQUE` constraint on `memory_registry.name` and `memory_registry.schema_name` prevents
duplicate creation. If two concurrent requests try to create the same memory:

1. First request wins (INSERT succeeds, schema created)
2. Second request fails at INSERT with a uniqueness violation
3. The handler maps this to `409 Conflict`

The schema creation (`CREATE SCHEMA`) is inside a transaction with the registry INSERT, so
partial states are impossible.

### In-Flight Request During Memory Deletion

When a memory is deleted while requests are in-flight targeting that memory:

1. Active transactions continue (PostgreSQL transaction isolation)
2. New transactions fail with "schema does not exist" error
3. The middleware catches this and returns `410 Gone: Memory 'X' has been deleted`
4. The TTL cache is invalidated, so subsequent requests resolve the new state

### Memory Lock During Backup/Restore

The `locked` field in `memory_registry` prevents writes during backup/restore operations:

```rust
// In archive_routing_middleware, after resolving the memory:
if memory_info.locked {
    return Err(ApiError::ServiceUnavailable(
        format!("Memory '{}' is locked for backup/restore", memory_info.name)
    ));
}
```

Read operations are allowed during lock (backup reads are point-in-time consistent anyway).

### Maximum Memory Limit

A configurable maximum prevents resource exhaustion:

```rust
const DEFAULT_MAX_MEMORIES: i64 = 100;

pub async fn create_memory(...) -> Result<...> {
    let max = std::env::var("MAX_MEMORIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_MEMORIES);

    let current_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_registry"
    ).fetch_one(&pool).await?;

    if current_count >= max {
        return Err(ApiError::BadRequest(format!(
            "Maximum memory limit reached ({}). Delete unused memories first.", max
        )));
    }

    // Proceed with creation
    ...
}
```

### Orphaned Schemas

A startup reconciliation detects schemas that exist in PostgreSQL but not in the registry
(or vice versa):

```sql
-- Schemas in PG but not in registry (orphaned)
SELECT nspname FROM pg_namespace
WHERE nspname LIKE 'memory_%'
  AND nspname NOT IN (SELECT schema_name FROM memory_registry);

-- Registry entries with no matching schema (stale)
SELECT name, schema_name FROM memory_registry
WHERE schema_name != 'public'
  AND schema_name NOT IN (
      SELECT nspname FROM pg_namespace
  );
```

Orphaned schemas are logged as warnings. Stale registry entries are cleaned up automatically.

---

## 10. Testing Strategy

### Unit Tests

Unit tests validate individual components in isolation:

| Component | Tests |
|-----------|-------|
| `validate_schema_name()` | Valid names, invalid chars, reserved words, length limits |
| `generate_schema_name()` | Sanitization rules, prefix, case handling |
| `MEMORY_TABLE_MANIFEST` | Non-empty, no duplicates, valid PG identifiers |
| `ArchiveContext` | Default values, clone behavior |
| `DefaultArchiveCache` | TTL expiration, invalidation |

### Integration Tests (Implemented)

Five test files cover the full multi-memory lifecycle:

| Test File | Coverage |
|-----------|----------|
| `archives_api_test.rs` | Memory CRUD, cloning, federated search, lifecycle |
| `archive_schema_routing_test.rs` | Schema isolation, per-request routing, default archive resolution |
| `archive_template_routing_test.rs` | Template CRUD within memory schemas |
| `analytics_memory_attachments_archive_routing_test.rs` | Analytics, file attachments within memory schemas |
| `archive_version_metadata_test.rs` | Version metadata within archives |

### Schema Drift Detection Test

This test is the **automated guard** against future drift. It runs in CI on every push
and fails if a new migration adds a table to public that is not in the manifest.

```rust
#[tokio::test]
async fn test_no_schema_drift_between_public_and_manifest() {
    let pool = setup_test_pool().await;

    // Get all user tables in public schema (exclude system tables)
    let public_tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_type = 'BASE TABLE'
           AND table_name NOT LIKE '_sqlx%'
         ORDER BY table_name"
    )
    .fetch_all(&pool).await.unwrap();

    let shared_tables = vec![
        "_sqlx_migrations",
        "memory_registry",       // or "archive_registry" during transition
        "oauth_client",
        "api_key",
        "document_type",
        "document_type_agentic_config",
        "document_type_extraction_strategy",
        "job_queue",
        "job_history",
        "pke_public_key",
        "embedding_config",
    ];

    let manifest: Vec<&str> = MEMORY_TABLE_MANIFEST.to_vec();

    for table in &public_tables {
        let is_shared = shared_tables.contains(&table.as_str());
        let is_in_manifest = manifest.contains(&table.as_str());

        assert!(
            is_shared || is_in_manifest,
            "Table '{}' exists in public schema but is neither in \
             SHARED_TABLES nor MEMORY_TABLE_MANIFEST. \
             Add it to one of these lists.",
            table
        );
    }
}
```

### Memory Lifecycle Integration Tests

```rust
#[tokio::test]
async fn test_memory_create_use_delete_lifecycle() {
    let pool = setup_test_pool().await;
    let memory_name = format!("test-{}", Uuid::new_v4().simple());

    // Create
    let info = repo.create_memory(&memory_name, Some("test")).await.unwrap();
    assert_eq!(info.note_count, Some(0));

    // Insert a note into the memory
    let ctx = SchemaContext::new(pool.clone(), &info.schema_name).unwrap();
    ctx.execute(|tx| Box::pin(async move {
        sqlx::query("INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
                      VALUES ($1, 'md', 'test', NOW(), NOW())")
            .bind(Uuid::new_v4())
            .execute(&mut **tx).await.map_err(Error::Database)?;
        Ok(())
    })).await.unwrap();

    // Verify note is in memory, not in public
    let memory_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.note", info.schema_name
    )).fetch_one(&pool).await.unwrap();
    assert_eq!(memory_count, 1);

    // Delete
    repo.drop_memory(&memory_name).await.unwrap();

    // Verify schema is gone
    let exists: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = '{}')",
        info.schema_name
    )).fetch_one(&pool).await.unwrap();
    assert!(!exists);
}
```

### Cross-Memory Search Tests

```rust
#[tokio::test]
async fn test_cross_memory_fts_search() {
    let pool = setup_test_pool().await;

    // Create two memories with distinct content
    let mem_a = create_test_memory(&pool, "alpha").await;
    let mem_b = create_test_memory(&pool, "beta").await;

    insert_note_in_memory(&pool, &mem_a, "Rust programming language").await;
    insert_note_in_memory(&pool, &mem_b, "Rust prevention for metals").await;

    // Cross-memory search
    let results = search_global(&pool, "rust", &[&mem_a, &mem_b]).await.unwrap();

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.memory_name == "alpha"));
    assert!(results.iter().any(|r| r.memory_name == "beta"));

    // Cleanup
    drop_test_memory(&pool, &mem_a).await;
    drop_test_memory(&pool, &mem_b).await;
}
```

### Backup/Restore Tests

```rust
#[tokio::test]
async fn test_memory_backup_and_restore() {
    let pool = setup_test_pool().await;
    let memory = create_test_memory(&pool, "backup-test").await;

    // Insert data
    insert_note_in_memory(&pool, &memory, "Important note").await;
    let original_count = count_notes(&pool, &memory).await;
    assert_eq!(original_count, 1);

    // Backup (pg_dump --schema)
    let backup_path = backup_memory(&pool, &memory).await.unwrap();

    // Corrupt data (delete everything)
    sqlx::query(&format!("DELETE FROM {}.note", memory.schema_name))
        .execute(&pool).await.unwrap();
    assert_eq!(count_notes(&pool, &memory).await, 0);

    // Restore
    restore_memory(&pool, &memory, &backup_path).await.unwrap();
    assert_eq!(count_notes(&pool, &memory).await, 1);

    // Cleanup
    drop_test_memory(&pool, &memory).await;
}
```

---

## 11. Architectural Decision Records

### ADR-080: Use PostgreSQL Schemas for Memory Isolation

**Status:** Accepted

**Context:**
We need to provide data isolation between different knowledge bases (memories) within a
single Fortemi deployment. Options considered:

1. **Row-level filtering** (tenant_id column on every table)
2. **Separate databases** (one PostgreSQL database per memory)
3. **PostgreSQL schemas** (one schema per memory within the same database)

**Decision:** PostgreSQL schemas.

**Consequences:**
- (+) Clean isolation via `search_path` -- no query changes needed
- (+) Shared connection pool and infrastructure
- (+) `pg_dump --schema` for trivial per-memory backup
- (+) `DROP SCHEMA CASCADE` for instant cleanup
- (+) Cross-schema queries possible for global search
- (-) Must manage schema creation/migration for each memory
- (-) Cannot use different PostgreSQL extensions per memory (not needed)

**Rejected alternatives:**
- Row-level filtering: Requires adding `memory_id` to every table, every query, every index.
  One missing WHERE clause leaks data. Performance degrades with large tenant counts.
- Separate databases: No cross-database queries. Cannot share connection pools. Increases
  operational complexity per-memory.

### ADR-081: LIKE INCLUDING ALL Instead of Hardcoded DDL

**Status:** Accepted

**Context:**
The current archive implementation hardcodes DDL for 10 tables. This drifts from the
25+ table public schema after every migration. Maintaining parallel DDL is error-prone
and has already produced broken archives.

**Decision:** Use `CREATE TABLE ... (LIKE public.X INCLUDING ALL)` to clone table
structures from the public schema at creation time.

**Consequences:**
- (+) Zero-drift by construction -- memories always match public at creation time
- (+) ~200 lines of hardcoded DDL replaced by a 25-item table name list
- (+) New tables added to public are available in memories after adding one line to the manifest
- (-) Foreign keys must be introspected and recreated (additional SQL queries during creation)
- (-) Triggers must be introspected and recreated
- (-) `INCLUDING ALL` copies index *structures* but not index *names*, which may differ

### ADR-082: Table Manifest Over Automated Discovery

**Status:** Accepted

**Context:**
Instead of maintaining a table manifest, we could automatically discover per-memory tables
by subtracting known shared tables from all public tables.

**Decision:** Explicit manifest (allowlist), not automated discovery (denylist).

**Note:** The implementation uses a deny-list approach (14 shared tables in `SHARED_TABLES`) for runtime classification, but still maintains the table manifest for schema cloning order (FK dependency ordering). The deny-list determines which tables are per-memory; the manifest determines cloning order.

**Consequences:**
- (+) Adding a shared table does not accidentally clone it into memories
- (+) Manifest is self-documenting and reviewable
- (+) Drift detection test can verify manifest completeness
- (-) New tables require a manifest update (but this is caught by the drift detection test)

### ADR-083: Memory Selection via HTTP Header

**Status:** Accepted

**Context:**
Per-request memory selection could be done via: (a) HTTP header, (b) query parameter,
(c) URL path prefix, or (d) subdomain.

**Decision:** HTTP header `X-Fortemi-Memory`.

**Consequences:**
- (+) Does not pollute URL space or API paths
- (+) Easy for MCP server to inject transparently
- (+) Compatible with existing route structure
- (+) Falls back cleanly to default memory when absent
- (-) Not visible in browser URL bar (irrelevant for API-first product)
- (-) Some HTTP clients make headers harder to set (minimal concern)

### ADR-084: Rename Archives to Memories in API

**Status:** Accepted

**Context:**
The existing API uses `/api/v1/archives/*` but "archive" suggests cold storage or
backup, not active isolated workspaces.

**Decision:** New canonical routes at `/api/v1/memories/*` with `/api/v1/archives/*`
kept as backward-compatible aliases.

**Consequences:**
- (+) "Memory" better communicates the concept of an active knowledge space
- (+) Aligns with MCP tool naming (`switch_memory`, `get_current_memory`)
- (+) No breaking changes (old routes keep working)
- (-) Two sets of routes to maintain (trivial: they point to the same handlers)

---

## 12. Implementation Roadmap

**All phases COMPLETE as of 2026-02-08.**

### Phase 1: Foundation (Week 1-2) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| Table manifest | Define `MEMORY_TABLE_MANIFEST` constant | `crates/matric-db/src/archives.rs` |
| Schema cloner | Implement `clone_schema_tables()` | `crates/matric-db/src/archives.rs` |
| FK introspection | Query and remap foreign keys | `crates/matric-db/src/archives.rs` |
| Trigger cloning | Query and recreate triggers | `crates/matric-db/src/archives.rs` |
| Drift test | CI test comparing manifest to public schema | `crates/matric-db/tests/` |
| Registry migration | Rename `archive_registry` to `memory_registry` | `migrations/` |

### Phase 2: API and Middleware (Week 2-3) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| Memory header | Parse `X-Fortemi-Memory` in middleware | `crates/matric-api/src/middleware/archive_routing.rs` |
| Memory routes | Add `/api/v1/memories/*` endpoints | `crates/matric-api/src/handlers/archives.rs`, `main.rs` |
| Archive aliases | Keep `/api/v1/archives/*` as aliases | `crates/matric-api/src/main.rs` |
| Clone endpoint | `POST /memories/:name/clone` | `crates/matric-api/src/handlers/archives.rs` |
| Drift status | Add `drift_status` to memory info response | `crates/matric-api/src/handlers/archives.rs` |
| `_tx` methods | Add transaction-aware methods to all repos | `crates/matric-db/src/*.rs` |
| 91 handlers | Update all handlers to use SchemaContext | `crates/matric-api/src/main.rs` |

### Phase 3: Backup/Restore (Week 3-4) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| Backup endpoint | `POST /memories/:name/backup` using `pg_dump --schema` | `crates/matric-api/src/handlers/` |
| Restore endpoint | `POST /memories/:name/restore` using `pg_restore` | `crates/matric-api/src/handlers/` |
| Lock mechanism | `locked` flag in registry, checked by middleware | `crates/matric-db/`, middleware |
| Backup storage | Integration with existing backup directory | `crates/matric-api/src/services/` |

### Phase 4: MCP Integration (Week 4) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| `switch_memory` tool | Set active memory for MCP session | `mcp-server/tools.js`, `index.js` |
| `get_current_memory` tool | Show active memory | `mcp-server/tools.js`, `index.js` |
| Header injection | Add `X-Fortemi-Memory` to `apiRequest()` | `mcp-server/index.js` |
| Enhanced `memory_info` | Per-memory breakdowns | `mcp-server/index.js` |
| `search_all_memories` tool | Cross-memory search via MCP | `mcp-server/tools.js`, `index.js` |

### Phase 5: Cross-Memory Search (Week 5) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| Federated search endpoint | `POST /api/v1/search/federated` | `crates/matric-api/src/handlers/` |
| Dynamic UNION builder | Generate cross-schema queries | `crates/matric-search/` |
| Memory filter | Optional `memories` parameter | `crates/matric-search/` |
| Integration tests | Cross-memory search validation | `crates/matric-search/tests/` |

### Phase 6: Hardening (Week 5-6) -- COMPLETE

| Task | Description | Files Affected |
|------|-------------|----------------|
| Orphan detection | Startup reconciliation of schemas vs registry | `crates/matric-db/` |
| Memory limits | `MAX_MEMORIES` environment variable | `crates/matric-api/` |
| Upgrade command | `POST /memories/:name/upgrade` for old archives | `crates/matric-api/`, `crates/matric-db/` |
| Documentation | Update CLAUDE.md, OpenAPI spec, MCP README | `CLAUDE.md`, `openapi.yaml`, `mcp-server/README.md` |
| Load testing | Verify performance with 10+ memories | Test scripts |

---

## Appendix A: Full Table Classification

| Table | Location | Rationale |
|-------|----------|-----------|
| `_sqlx_migrations` | `public` | Global migration tracking |
| `memory_registry` | `public` | Memory metadata registry |
| `oauth_client` | `public` | Authentication infrastructure |
| `api_key` | `public` | API key management |
| `document_type` | `public` | Shared catalog (131 types) |
| `document_type_agentic_config` | `public` | Extends document_type |
| `document_type_extraction_strategy` | `public` | Extends document_type |
| `job_queue` | `public` | Global job worker |
| `job_history` | `public` | Job metrics |
| `pke_public_key` | `public` | Encryption key registry |
| `embedding_config` | `public` | Embedding model configs |
| `note` | per-memory | Core user data |
| `note_original` | per-memory | Immutable original content |
| `note_revised_current` | per-memory | AI-enhanced content |
| `note_revision` | per-memory | Revision history |
| `note_share_grant` | per-memory | Sharing permissions |
| `collection` | per-memory | Note organization |
| `tag` | per-memory | Tags (per-memory namespace) |
| `note_tag` | per-memory | Note-tag associations |
| `embedding` | per-memory | Vector embeddings |
| `embedding_set` | per-memory | Embedding set management |
| `link` | per-memory | Note relationships |
| `colbert_embedding` | per-memory | ColBERT multi-vector |
| `provenance_edge` | per-memory | Content provenance |
| `activity_log` | per-memory | Audit trail |
| `skos_concept_scheme` | per-memory | SKOS taxonomy schemes |
| `skos_concept` | per-memory | SKOS concepts |
| `skos_relation` | per-memory | SKOS relations |
| `skos_collection` | per-memory | SKOS collections |
| `skos_label` | per-memory | SKOS labels |
| `skos_note` | per-memory | SKOS notes |
| `file_storage` | per-memory | File metadata |
| `attachment` | per-memory | Note attachments |
| `file_upload_audit` | per-memory | Upload audit trail |
| `template` | per-memory | Note templates |
| `webhook` | per-memory | Event webhooks |
| `user_metadata_label` | per-memory | Custom labels |
| `user_config` | per-memory | User preferences |

## Appendix B: Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MAX_MEMORIES` | `100` | Maximum number of memories per deployment |
| `MEMORY_CACHE_TTL` | `300` | Default memory cache TTL in seconds |
| `MEMORY_BACKUP_DIR` | `/backups` | Directory for memory backup files |
| `MEMORY_CLONE_ASYNC` | `true` | Clone large memories asynchronously via job queue |

## Appendix C: Migration Checklist for New Tables

When adding a new table to the public schema via a migration:

1. Determine if the table is **shared** or **per-memory**
2. If per-memory: add the table name to `MEMORY_TABLE_MANIFEST` in dependency order
3. If shared: add the table name to the `SHARED_TABLES` list in the drift detection test
4. Run `cargo test test_no_schema_drift` to verify no drift
5. Existing memories will be upgraded on next startup (Phase 2 reconciliation)

This checklist prevents the exact schema drift problem that motivated this architecture.
