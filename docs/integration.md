# Integration Guide

This guide covers integrating matric-memory into your application.

## Prerequisites

- PostgreSQL 14+ with pgvector extension
- Rust 1.70+ (if building from source)
- Ollama (optional, for local inference) OR OpenAI API key (for cloud inference)

## Installation

### Option 1: As HTTP API

Use the matric-memory API server directly:

```bash
# Clone repository
git clone https://git.integrolabs.net/roctinam/matric-memory

# Build
cargo build --release -p matric-api

# Run
DATABASE_URL="postgres://user:pass@localhost/matric" ./target/release/matric-api
```

### Option 2: As Rust Crate (Future)

```toml
# Cargo.toml
[dependencies]
matric-memory = { git = "https://git.integrolabs.net/roctinam/matric-memory" }
```

## Database Setup

### 1. Create Database

```sql
CREATE DATABASE matric;
\c matric
CREATE EXTENSION IF NOT EXISTS vector;
```

### 2. Run Migrations

```bash
cd matric-memory
DATABASE_URL="postgres://user:pass@localhost/matric" sqlx migrate run
```

### 3. Verify Setup

```bash
curl http://localhost:3000/health
# {"status":"healthy","version":"0.1.0"}
```

## API Usage

### Creating Notes

```bash
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# My Note\n\nThis is a markdown note.",
    "tags": ["work", "important"]
  }'
# {"id":"uuid-here"}
```

### Searching Notes

```bash
# Hybrid search (default)
curl "http://localhost:3000/api/v1/search?q=markdown+notes"

# FTS only
curl "http://localhost:3000/api/v1/search?q=markdown&mode=fts"

# Semantic only
curl "http://localhost:3000/api/v1/search?q=markdown&mode=semantic"
```

### Strict Tag Filtering

Use strict filters for guaranteed data isolation (e.g., multi-tenancy, client segregation):

```bash
# Search within a specific client's notes only
curl -X POST "http://localhost:3000/api/v1/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "quarterly report",
    "strict_filter": {
      "required_schemes": ["client-acme"]
    }
  }'

# Search with multiple filter conditions
curl -X POST "http://localhost:3000/api/v1/search" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "api design",
    "strict_filter": {
      "required_tags": ["project:matric"],
      "any_tags": ["priority:high", "priority:critical"],
      "excluded_tags": ["status:archived", "draft"]
    }
  }'
```

**Filter Types:**
- `required_tags`: Notes MUST have ALL these (AND logic)
- `any_tags`: Notes MUST have AT LEAST ONE (OR logic)
- `excluded_tags`: Notes MUST NOT have ANY (exclusion)
- `required_schemes`: Notes ONLY from these SKOS vocabulary schemes
- `excluded_schemes`: Notes NOT from these schemes

See [Strict Tag Filtering](./tags.md#strict-tag-filtering) for details.

### Managing Tags

```bash
# List all tags
curl http://localhost:3000/api/v1/tags

# Set tags for a note
curl -X PUT http://localhost:3000/api/v1/notes/{id}/tags \
  -H "Content-Type: application/json" \
  -d '{"tags": ["work", "project"]}'
```

### Background Jobs

```bash
# Create embedding job
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "uuid-here",
    "job_type": "embedding"
  }'

# Check job status
curl http://localhost:3000/api/v1/jobs/{job_id}
```

## HotM Migration

If migrating from HotM's embedded backend:

### 1. Export Existing Data

```bash
# From HotM database
pg_dump hotm > hotm_backup.sql
```

### 2. Update Configuration

```typescript
// HotM frontend config
const API_URL = "https://memory.integrolabs.net";
```

### 3. Update API Calls

```typescript
// Before (HotM backend)
const notes = await fetch('/api/notes').then(r => r.json());

// After (matric-memory)
const notes = await fetch(`${API_URL}/api/v1/notes`).then(r => r.json());
```

### 4. Schema Mapping

| HotM Field | matric-memory Field |
|------------|---------------------|
| id | id |
| content | note.original.content |
| revised | note.revised.content |
| embedding | embedding.embedding |
| created_at | note.created_at_utc |
| tags | tags[] |

## MCP Server Integration

For AI agent integration:

### 1. Install Dependencies

```bash
cd mcp-server
npm install
```

### 2. Configure Claude Desktop

Add to `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "matric-memory": {
      "command": "node",
      "args": ["/path/to/matric-memory/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "https://memory.integrolabs.net"
      }
    }
  }
}
```

### 3. Available Tools

**Note Management:**
| Tool | Description |
|------|-------------|
| list_notes | List all notes |
| get_note | Get full note details |
| create_note | Create new note |
| update_note | Update existing note |
| delete_note | Soft delete note |
| search_notes | Hybrid search with optional strict filtering |
| search_notes_strict | Strict tag-filtered search (guaranteed isolation) |
| list_tags | List all tags |
| set_note_tags | Update note tags |
| get_note_links | Get note relationships |
| create_job | Queue background job |

**Backup & Export:**
| Tool | Description |
|------|-------------|
| export_all_notes | Export notes as JSON |
| knowledge_shard | Create knowledge shard backup |
| knowledge_shard_import | Import from knowledge shard |
| backup_now | Trigger database backup |
| backup_status | Check backup health |

**Encryption (PKE):**
| Tool | Description |
|------|-------------|
| pke_generate_keypair | Generate X25519 keypair |
| pke_encrypt | Encrypt for multiple recipients |
| pke_decrypt | Decrypt with private key |
| pke_list_recipients | List recipient addresses |
| pke_get_address | Get address from public key |
| pke_verify_address | Verify address checksum |

See [Encryption Guide](./encryption.md) for encryption details.

## Configuration Reference

### Core Settings

| Variable | Default | Description |
|----------|---------|-------------|
| DATABASE_URL | Required | PostgreSQL connection URL |
| HOST | 0.0.0.0 | API server bind address |
| PORT | 3000 | API server port |
| RUST_LOG | info | Log level |

### Inference Backend (Ollama)

| Variable | Default | Description |
|----------|---------|-------------|
| OLLAMA_URL | http://localhost:11434 | Ollama endpoint |
| OLLAMA_EMBEDDING_MODEL | nomic-embed-text | Model for embeddings |
| OLLAMA_GENERATION_MODEL | llama3.2:3b | Model for generation |
| OLLAMA_EMBEDDING_DIMENSION | 768 | Embedding vector size |

### Inference Backend (OpenAI)

| Variable | Default | Description |
|----------|---------|-------------|
| OPENAI_API_KEY | Required | API key for OpenAI |
| OPENAI_BASE_URL | https://api.openai.com/v1 | API base URL |
| OPENAI_EMBEDDING_MODEL | text-embedding-3-small | Model for embeddings |
| OPENAI_GENERATION_MODEL | gpt-4o-mini | Model for generation |
| OPENAI_EMBEDDING_DIMENSION | 1536 | Embedding vector size |
| INFERENCE_BACKEND | ollama | Backend to use (ollama or openai) |

See [Inference Backends Guide](./inference-backends.md) for detailed configuration.

## Error Handling

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 204 | No Content (success, no body) |
| 400 | Bad Request (validation error) |
| 404 | Not Found |
| 500 | Server Error |

### Error Response Format

```json
{
  "error": "Note not found"
}
```

## Troubleshooting

### Connection Issues

```bash
# Test database connection
psql "$DATABASE_URL" -c "SELECT 1"

# Test pgvector
psql "$DATABASE_URL" -c "SELECT '[1,2,3]'::vector"
```

### Search Not Working

```bash
# Check if embeddings exist
psql "$DATABASE_URL" -c "SELECT COUNT(*) FROM embedding"

# Check FTS index
psql "$DATABASE_URL" -c "SELECT * FROM note_original LIMIT 1"
```

### Job Queue Issues

```bash
# Check pending jobs
curl http://localhost:3000/api/v1/jobs/pending

# Check failed jobs
psql "$DATABASE_URL" -c "SELECT * FROM job_queue WHERE status = 'failed'"
```

## Performance Tuning

### PostgreSQL Settings

```sql
-- Increase work memory for vector ops
SET work_mem = '256MB';

-- Tune HNSW index
SET hnsw.ef_search = 100;  -- higher = more accurate, slower
```

### Connection Pooling

The API uses sqlx connection pooling. Default pool size is based on database limits.

### Search Optimization

```bash
# Use FTS for keyword queries
curl "http://localhost:3000/api/v1/search?q=exact+phrase&mode=fts"

# Use semantic for conceptual queries
curl "http://localhost:3000/api/v1/search?q=ideas+about+productivity&mode=semantic"
```

## Support

- **Issues**: https://git.integrolabs.net/roctinam/matric-memory/issues
- **API Docs**: https://memory.integrolabs.net/docs
- **OpenAPI Spec**: https://memory.integrolabs.net/openapi.yaml
