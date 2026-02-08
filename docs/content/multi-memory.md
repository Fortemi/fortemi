# Multi-Memory System

Fortemi supports parallel memory archives, allowing you to maintain multiple isolated knowledge bases within a single deployment. Each memory operates as a separate PostgreSQL schema with its own notes, tags, collections, embeddings, links, and templates.

## Overview

### What are Memories?

A memory (formerly called "archive") is an isolated namespace for your knowledge base. Think of memories as separate workspaces or projects, each with complete data isolation:

- **Work Memory**: Professional projects and documentation
- **Personal Memory**: Private notes and journal entries
- **Research Memory**: Academic papers and literature reviews
- **Client Memories**: Separate workspace per client for data isolation

### Key Features

1. **Complete Isolation**: Each memory has its own PostgreSQL schema. Notes, tags, collections, embeddings, and links never cross memory boundaries
2. **Per-Request Routing**: Use the `X-Fortemi-Memory` HTTP header to select which memory to operate on
3. **Federated Search**: Search across multiple memories simultaneously with unified result ranking
4. **Memory Cloning**: Deep copy entire memories including all notes, embeddings, and relationships
5. **Auto-Migration**: Memories are automatically updated when new table structures are added
6. **Capacity Management**: System-wide limits and per-memory statistics via overview endpoint

## Architecture

### Schema Isolation

Each memory operates in its own PostgreSQL schema:

```
Database: matric
├── public (shared tables)
│   ├── archive_registry (memory metadata)
│   ├── oauth_clients
│   ├── api_keys
│   └── ... (14 shared tables total)
├── default (default memory)
│   ├── note
│   ├── note_original
│   ├── embedding
│   ├── note_links
│   ├── skos_concepts
│   └── ... (41 per-memory tables)
├── work-2026 (custom memory)
│   ├── note
│   ├── note_original
│   └── ...
└── research (custom memory)
    └── ...
```

### Shared vs Per-Memory Tables

**Shared Tables (14 total):**
- Authentication: OAuth clients, API keys, sessions
- System: Job queue, event subscriptions, webhooks
- Registry: Archive metadata, embedding configurations

These tables live in the `public` schema and are shared across all memories.

**Per-Memory Tables (41 total):**
- Notes: note, note_original, note_revision
- Embeddings: embedding, embedding_set, embedding_set_member
- Links: note_links
- Tags: tag, tag_note, skos_concepts, skos_labels, skos_relations
- Collections: collection, collection_note
- Templates: template
- Attachments: file_attachment, file_provenance
- Document Types: Custom types per memory
- Versioning: Content history tables

### Deny-List Approach

The system uses a **deny-list** approach: all tables are per-memory except the 14 explicitly shared tables defined in `SHARED_TABLES` constant. This ensures zero drift when new tables are added - they automatically become per-memory unless explicitly added to the shared list.

## Creating and Managing Memories

### Creating a Memory

**Via API:**

```bash
curl -X POST http://localhost:3000/api/v1/memories \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "name": "work-2026",
    "description": "Work-related notes for 2026"
  }'
```

**Via MCP:**

```javascript
create_memory({
  name: "work-2026",
  description: "Work-related notes for 2026"
})
```

Memory names must be valid PostgreSQL schema identifiers (lowercase letters, numbers, underscores, hyphens).

### Listing Memories

**Via API:**

```bash
curl http://localhost:3000/api/v1/memories \
  -H "Authorization: Bearer $TOKEN"
```

**Response:**

```json
{
  "memories": [
    {
      "name": "default",
      "description": "Default memory",
      "created_at": "2026-01-15T10:00:00Z",
      "note_count": 1523,
      "size_bytes": 52428800,
      "schema_version": 41
    },
    {
      "name": "work-2026",
      "description": "Work-related notes for 2026",
      "created_at": "2026-02-01T12:00:00Z",
      "note_count": 245,
      "size_bytes": 8388608,
      "schema_version": 41
    }
  ]
}
```

### Getting Memory Details

```bash
curl http://localhost:3000/api/v1/memories/work-2026 \
  -H "Authorization: Bearer $TOKEN"
```

### Updating Memory Metadata

```bash
curl -X PATCH http://localhost:3000/api/v1/memories/work-2026 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "description": "Updated description"
  }'
```

### Deleting a Memory

```bash
curl -X DELETE http://localhost:3000/api/v1/memories/work-2026 \
  -H "Authorization: Bearer $TOKEN"
```

Deletes the memory's schema and all data within it. This operation is irreversible.

## Using Memories

### Per-Request Memory Selection

Select which memory to operate on using the `X-Fortemi-Memory` HTTP header:

```bash
# Create note in work memory
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -H "X-Fortemi-Memory: work-2026" \
  -d '{
    "content": "# Project Documentation\n\nInternal documentation for project X."
  }'

# Search in work memory
curl "http://localhost:3000/api/v1/search?q=project+documentation" \
  -H "Authorization: Bearer $TOKEN" \
  -H "X-Fortemi-Memory: work-2026"

# List notes from work memory
curl http://localhost:3000/api/v1/notes \
  -H "Authorization: Bearer $TOKEN" \
  -H "X-Fortemi-Memory: work-2026"
```

**If no header is provided**, the request operates on the **default** memory.

### MCP Memory Management

The MCP server provides memory management tools with session-based memory context:

#### select_memory

Switch the active memory for the current MCP session:

```javascript
select_memory({ name: "work-2026" })
// All subsequent operations use work-2026 memory
```

#### get_active_memory

Check which memory is currently active:

```javascript
get_active_memory()
// Returns: { name: "work-2026" }
```

#### list_memories

List all available memories:

```javascript
list_memories()
```

#### create_memory

Create a new memory:

```javascript
create_memory({
  name: "research",
  description: "Academic research notes"
})
```

#### delete_memory

Delete a memory and all its data:

```javascript
delete_memory({ name: "old-project" })
```

## Federated Search

Search across multiple memories simultaneously with unified result ranking.

### Search All Memories

```bash
curl -X POST http://localhost:3000/api/v1/search/federated \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "query": "machine learning",
    "memories": ["all"]
  }'
```

### Search Specific Memories

```bash
curl -X POST http://localhost:3000/api/v1/search/federated \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "query": "project documentation",
    "memories": ["work-2026", "research"]
  }'
```

### Response Format

```json
{
  "results": [
    {
      "note_id": "550e8400-...",
      "memory": "work-2026",
      "score": 0.92,
      "title": "Project Documentation",
      "snippet": "...machine learning algorithms...",
      "tags": ["project", "ml"]
    },
    {
      "note_id": "660e8400-...",
      "memory": "research",
      "score": 0.85,
      "title": "ML Research Papers",
      "snippet": "...deep learning techniques...",
      "tags": ["research", "ml"]
    }
  ],
  "total": 2,
  "memories_searched": ["work-2026", "research"]
}
```

### How Federated Search Works

1. **Parallel Execution**: Search runs concurrently across all specified memories
2. **Score Normalization**: Scores are normalized to [0,1] range per memory
3. **Unified Ranking**: Results are merged and re-sorted by score
4. **Memory Attribution**: Each result includes its source memory name

**MCP Tool:**

```javascript
search_memories_federated({
  query: "machine learning",
  memories: ["all"]
})
```

## Memory Cloning

Deep copy entire memories including all notes, embeddings, links, and relationships.

### Clone a Memory

```bash
curl -X POST http://localhost:3000/api/v1/archives/work-2026/clone \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "new_name": "work-2026-backup",
    "description": "Backup of work memory before major refactoring"
  }'
```

### Clone Process

1. **Schema Creation**: Creates new PostgreSQL schema with `new_name`
2. **Table Copy**: Copies all 41 per-memory tables
3. **Data Copy**: Uses `INSERT INTO ... SELECT` with `session_replication_role = 'replica'` to bypass triggers and constraints
4. **Relationship Preservation**: UUIDs remain identical, preserving all links and embeddings
5. **Auto-Migration**: New memory is automatically at current schema version

### Use Cases

- **Backup before major changes**: Clone before bulk deletions or schema migrations
- **Testing environments**: Clone production memory for testing without affecting live data
- **Client project templates**: Clone a template memory structure for new clients
- **Archival**: Create point-in-time snapshots of memories

**MCP Tool:**

```javascript
clone_memory({
  source_name: "work-2026",
  new_name: "work-2026-backup",
  description: "Backup before migration"
})
```

## Capacity Planning and Monitoring

### Memory Overview

Get aggregate statistics across all memories:

```bash
curl http://localhost:3000/api/v1/memories/overview \
  -H "Authorization: Bearer $TOKEN"
```

**Response:**

```json
{
  "capacity": {
    "max_memories": 100,
    "current_count": 3,
    "available": 97
  },
  "usage": {
    "total_notes": 1768,
    "total_size_bytes": 60817408,
    "total_size_human": "58.02 MB"
  },
  "memories": [
    {
      "name": "default",
      "note_count": 1523,
      "size_bytes": 52428800,
      "size_human": "50.00 MB",
      "schema_version": 41
    },
    {
      "name": "work-2026",
      "note_count": 245,
      "size_bytes": 8388608,
      "size_human": "8.00 MB",
      "schema_version": 41
    }
  ],
  "database": {
    "total_size_bytes": 104857600,
    "total_size_human": "100.00 MB"
  }
}
```

### Configuring Memory Limits

Set the maximum number of memories via environment variable:

```bash
# .env
MAX_MEMORIES=100  # Default value
```

Attempts to create memories beyond this limit will fail with HTTP 400:

```json
{
  "error": "Memory limit reached. Maximum 100 memories allowed."
}
```

### Per-Memory Statistics

Each memory tracks:

- **note_count**: Total number of notes
- **size_bytes**: Estimated size on disk (all tables combined)
- **schema_version**: Number of tables (for auto-migration tracking)
- **last_accessed**: Timestamp of last operation (updated automatically)

**MCP Tool:**

```javascript
get_memories_overview()
```

## Auto-Migration

Memories are automatically migrated when new table structures are added to the system.

### How Auto-Migration Works

1. **Schema Version Tracking**: Each memory stores its `schema_version` (current table count)
2. **On Access Check**: When a memory is accessed, the system compares its schema version to the expected version
3. **Missing Table Detection**: If `schema_version < expected`, missing tables are created automatically
4. **Create Tables**: Uses the same `CREATE TABLE` statements that initialized the default memory
5. **Version Update**: `schema_version` is updated to reflect the new table count

### Migration Trigger

Auto-migration runs when:
- A memory is accessed via `X-Fortemi-Memory` header
- MCP selects a memory via `select_memory`
- Federated search includes a memory
- Memory clone operation completes

### Migration Safety

- **Non-Destructive**: Only creates missing tables, never modifies existing ones
- **Idempotent**: Safe to run multiple times
- **Logged**: Migration events are logged for debugging
- **Fast**: Typically completes in <100ms (empty table creation only)

### Manual Migration Verification

Check if a memory needs migration:

```bash
curl http://localhost:3000/api/v1/memories/work-2026 \
  -H "Authorization: Bearer $TOKEN"
```

If `schema_version < 41` (current expected version), the memory will be auto-migrated on next access.

## Data Isolation Guarantees

### What is Isolated

Each memory has complete isolation of:

1. **Notes**: All note content, revisions, and original versions
2. **Embeddings**: Vector embeddings and embedding sets
3. **Links**: Semantic relationships between notes
4. **Tags**: User tags and SKOS concept taxonomies
5. **Collections**: Folder hierarchies
6. **Templates**: Note templates and their instantiations
7. **Attachments**: File attachments and provenance data
8. **Document Types**: Custom document type definitions

### What is Shared

The following data is shared across all memories:

1. **Authentication**: OAuth clients, API keys, user sessions
2. **Job Queue**: Background processing jobs (though jobs operate on specific memories)
3. **Event Subscriptions**: Webhooks and event stream configuration
4. **System Configuration**: Embedding configurations, backup metadata

### Cross-Memory Operations

Operations **cannot** cross memory boundaries:

- Notes in memory A cannot link to notes in memory B
- Search in memory A will never return notes from memory B (unless using federated search)
- Tags in memory A are separate from tags in memory B (even if identically named)

**Exception**: Federated search explicitly searches multiple memories and attributes results to their source memory.

## Migration from Single-Memory Deployment

Existing deployments automatically have a `default` memory containing all current data. No migration is required.

### Step-by-Step Migration

1. **Create New Memories**: Create memories for different projects or clients
2. **Move Notes**: Use export/import or manual copying to move notes between memories
3. **Update Integrations**: Add `X-Fortemi-Memory` header to API calls that should target specific memories
4. **Test Isolation**: Verify notes don't cross memory boundaries
5. **Archive Old Data**: Move historical data to archived memories for cleanup

### Backward Compatibility

- All API calls without `X-Fortemi-Memory` header operate on the `default` memory
- Existing code continues to work without changes
- No data loss or schema migration required

## API Reference

### Memory Management

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/memories` | GET | List all memories |
| `/api/v1/memories` | POST | Create new memory |
| `/api/v1/memories/:name` | GET | Get memory details |
| `/api/v1/memories/:name` | PATCH | Update memory metadata |
| `/api/v1/memories/:name` | DELETE | Delete memory |
| `/api/v1/memories/overview` | GET | Get aggregate statistics |
| `/api/v1/archives/:name/clone` | POST | Clone memory (deep copy) |
| `/api/v1/search/federated` | POST | Search across multiple memories |

### Request Headers

| Header | Values | Description |
|--------|--------|-------------|
| `X-Fortemi-Memory` | Memory name | Routes request to specified memory (default: "default") |

### MCP Tools

| Tool | Description |
|------|-------------|
| `list_memories` | List all memories |
| `get_active_memory` | Get current session's active memory |
| `select_memory` | Switch active memory for session |
| `create_memory` | Create new memory |
| `delete_memory` | Delete memory |
| `clone_memory` | Clone memory with all data |
| `search_memories_federated` | Search across multiple memories |
| `get_memories_overview` | Get capacity and usage statistics |

## Backup and Restore

### Per-Memory Backup

Backups can be scoped to specific memories:

```bash
# Backup work memory only
curl http://localhost:3000/api/v1/backup/knowledge-shard \
  -H "X-Fortemi-Memory: work-2026" \
  -H "Authorization: Bearer $TOKEN" \
  -o work-2026-backup.shard
```

### Restore to Specific Memory

```bash
# Restore to a different memory
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -H "X-Fortemi-Memory: work-2026-restored" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"shard_base64": "..."}'
```

### Full Database Backup

Database backups include all memories and shared tables:

```bash
# Full pg_dump backup
curl http://localhost:3000/api/v1/backup/database \
  -H "Authorization: Bearer $TOKEN" \
  -o full-backup.sql
```

See [Backup Guide](./backup.md) for comprehensive backup strategies.

## Best Practices

### When to Use Multiple Memories

**Use multiple memories when you need:**

- **Client isolation**: Separate data per client with strict isolation guarantees
- **Project separation**: Isolate different projects with distinct knowledge domains
- **Personal vs professional**: Keep personal notes separate from work
- **Archival**: Move old projects to archived memories without deletion

**Don't use multiple memories for:**

- **Tagging or categorization**: Use tags and collections within a single memory
- **Temporary organization**: Use collections for temporary grouping
- **Search filtering**: Use strict tag filtering for data isolation within a memory

### Naming Conventions

- Use lowercase with hyphens: `client-acme-2026`
- Include dates for temporal organization: `work-2026-q1`
- Avoid special characters: stick to letters, numbers, underscores, hyphens
- Keep names short but descriptive: `research-ml` not `machine-learning-research-notes`

### Memory Size Guidelines

- **Small memories**: <10,000 notes, <100MB - Fast operations, minimal overhead
- **Medium memories**: 10,000-100,000 notes, 100MB-1GB - Good performance with proper indexing
- **Large memories**: >100,000 notes, >1GB - Consider splitting into multiple memories

### Performance Considerations

- Each memory adds minimal overhead (<1MB) for metadata and indexes
- Search performance scales with memory size, not total number of memories
- Federated search adds latency proportional to number of memories searched
- Memory cloning time scales with source memory size (typical: 1GB/minute)

## Troubleshooting

### "Memory not found" Error

**Symptom**: HTTP 404 when accessing a memory

**Causes:**
- Typo in memory name (names are case-sensitive)
- Memory was deleted
- Memory name not valid PostgreSQL identifier

**Fix:**
```bash
# List all memories
curl http://localhost:3000/api/v1/memories -H "Authorization: Bearer $TOKEN"
```

### Memory Limit Reached

**Symptom**: HTTP 400 "Memory limit reached"

**Cause**: MAX_MEMORIES environment variable limit hit

**Fix:**
```bash
# Increase limit in .env
MAX_MEMORIES=200

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### Slow Federated Search

**Symptom**: Federated search takes >5 seconds

**Cause**: Searching too many memories or very large memories

**Fix:**
- Reduce number of memories searched
- Use specific memory names instead of `["all"]`
- Optimize individual memory search performance (add embeddings, vacuum database)

### Schema Version Mismatch

**Symptom**: Queries fail with "relation does not exist"

**Cause**: Memory hasn't been auto-migrated yet

**Fix**: Access the memory to trigger auto-migration:
```bash
curl http://localhost:3000/api/v1/notes \
  -H "X-Fortemi-Memory: memory-name" \
  -H "Authorization: Bearer $TOKEN"
```

## Related Documentation

- [Backup Guide](./backup.md) - Per-memory backup strategies
- [Search Guide](./search-guide.md) - Search modes and federated search
- [MCP Server](./mcp.md) - Memory management via MCP tools
- [Configuration Reference](./configuration.md) - MAX_MEMORIES and other settings
- [API Reference](./api.md) - Complete API endpoint documentation
- [Architecture](./architecture.md) - Multi-memory system architecture
