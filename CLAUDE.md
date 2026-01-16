# Matric Memory

AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines.

## Architecture

- **crates/matric-api** - Axum HTTP API server
- **crates/matric-core** - Core types, traits, models
- **crates/matric-db** - PostgreSQL repositories (sqlx)
- **crates/matric-search** - Hybrid search (FTS + semantic + RRF)
- **crates/matric-inference** - Ollama embedding/generation
- **crates/matric-jobs** - Background job worker
- **mcp-server/** - MCP (Model Context Protocol) server in Node.js

## Deployment

### CRITICAL: Always Backup Before Migrations

**Before running ANY migration, create a database backup:**

```bash
# Backup database before migration
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Verify backup was created and has content
ls -lh backup_*.sql | tail -1
```

This is non-negotiable. Migrations can fail or have unintended effects. Always have a restore point.

### IMPORTANT: Always Run Migrations Before Restarting

Schema changes require migrations to be applied BEFORE the new code runs:

```bash
# 1. Backup first (see above)

# 2. Apply any new migrations
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/<new_migration>.sql

# 3. Then restart the service
sudo systemctl restart matric-api
```

Failing to run migrations first will cause database errors like:
- "column X does not exist"
- "relation X does not exist"

### Deployment Steps

1. Push to main (triggers CI/CD)
2. **Run new migrations** (see above)
3. Build release: `cargo build --release`
4. Restart service: `sudo systemctl restart matric-api`
5. Verify: `curl http://localhost:3000/health`

### Service Management

```bash
# Status
systemctl status matric-api

# Logs
journalctl -u matric-api -f

# Restart
sudo systemctl restart matric-api
```

## Database

- PostgreSQL 16 with pgvector extension
- Connection: `postgres://matric:matric@localhost/matric`
- Migrations: `migrations/` directory

## MCP Server

The MCP server provides Claude/AI integration:

```bash
cd mcp-server
node index.js  # stdio mode
MCP_TRANSPORT=http node index.js  # HTTP mode
```

## Testing

```bash
cargo test              # Unit tests
cargo test --workspace  # All crates
```

## Key Features

- Hybrid search (FTS + semantic + RRF fusion)
- AI revision with context from related notes
- Automatic semantic linking (>70% similarity)
- Collections/folders with hierarchy
- Note templates with variable substitution
- Graph exploration with recursive CTE
- Export to markdown with YAML frontmatter
