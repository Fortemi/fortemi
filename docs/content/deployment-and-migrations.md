# Deployment and Migrations Guide

This guide covers the complete process for building, deploying, and managing database migrations for Fortémi.

## Quick Reference

```bash
# Full deployment from scratch
cargo clean
cargo build --workspace --release
cargo test --workspace --release
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

## Build Process

### 1. Clean Build

For a guaranteed fresh build:

```bash
cargo clean                           # Remove all build artifacts
cargo build --workspace --release     # Full release build
```

### 2. Run Test Suite

Always run the full test suite before deployment:

```bash
cargo test --workspace --release
```

Verify:
- All tests pass (0 failures)
- No clippy warnings
- Code formatting is correct

### 3. Pre-commit Checks

The repository has pre-commit hooks that enforce:
- `cargo fmt --check` - Code formatting
- `cargo clippy -- -D warnings` - Lint checks

To run manually:
```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## Docker Container Build

### Build the Bundle Image

```bash
docker compose -f docker-compose.bundle.yml build
```

This creates an all-in-one container with:
- PostgreSQL 16 with pgvector and PostGIS
- Matric API server (port 3000)
- MCP server (port 3001)
- All database migrations

### Image Details

| Component | Version | Port |
|-----------|---------|------|
| PostgreSQL | 16 | 5432 (internal) |
| pgvector | Latest | - |
| PostGIS | 3.x | - |
| API Server | 2026.x | 3000 |
| MCP Server | Node.js | 3001 |

## Deployment

### Standard Deployment (Preserves Data)

```bash
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

This will:
1. Stop the existing container
2. Start the new container
3. Detect existing PostgreSQL data directory
4. Run any pending migrations automatically
5. Start all services

### Clean Install (Wipes Data)

```bash
docker compose -f docker-compose.bundle.yml down -v
docker compose -f docker-compose.bundle.yml up -d
```

The `-v` flag removes volumes, wiping all data.

### View Logs

```bash
# All logs
docker compose -f docker-compose.bundle.yml logs -f

# Last 100 lines
docker compose -f docker-compose.bundle.yml logs --tail=100
```

### Health Check

```bash
curl http://localhost:3000/health
# Expected: {"status":"healthy","version":"2026.1.x"}
```

## Database Migrations

### Migration Location

Migrations are stored in `/path/to/fortemi/migrations/` with naming convention:

```
YYYYMMDDHHMMSS_description.sql
```

### Automatic Migration

Migrations run automatically on container startup via the entrypoint script:

1. Container starts PostgreSQL
2. Waits for database readiness
3. Scans `migrations/` directory
4. Applies any unapplied migrations in order
5. Records applied migrations in `_migrations` table

### Migration Tracking

The `_migrations` table tracks applied migrations:

```sql
SELECT * FROM _migrations ORDER BY applied_at;
```

### Current Migrations

The system includes 55 migrations organized in phases. Key milestones:

| Migration | Description | Phase |
|-----------|-------------|-------|
| `20240101000000_initial_schema.sql` | Core tables (note, tag, collection, link) | Foundation |
| `20250115000000_embedding_sets.sql` | Embedding set management, MRL support | Embeddings |
| `20260201100000_multilingual_fts_phase1.sql` | matric_simple config, websearch_to_tsquery | FTS Phase 1 |
| `20260201200000_multilingual_fts_phase2.sql` | pg_trgm extension, trigram indexes | FTS Phase 2 |
| `20260201300000_multilingual_fts_phase3.sql` | pg_bigm (optional), language configs | FTS Phase 3 |
| `20260203000000_attachment_doctype_integration.sql` | File attachments, document type registry | Attachments |
| `20260204000000_temporal_spatial_provenance.sql` | PostGIS, W3C PROV provenance, spatial indexes | Memory Search |
| `20260205000000_colbert_embeddings.sql` | ColBERT token-level embeddings | Embeddings v2 |
| `20260205100000_webhooks.sql` | Webhook subscriptions and delivery log | Events |
| `20260206000000_seed_mime_types.sql` | MIME type seed data for document detection | Document Types |
| `20260207000000_add_document_type_inference_job.sql` | DocumentTypeInference job type | Extraction |

All 55 migrations run automatically on startup. Use `SELECT version, description FROM _sqlx_migrations ORDER BY version` to verify.

### Verifying Migrations

After deployment, verify migrations applied:

```bash
# Check migration log
docker compose -f docker-compose.bundle.yml logs | grep -i migration

# Connect to database and check
docker compose -f docker-compose.bundle.yml exec matric psql -U matric -d matric -c "SELECT * FROM _migrations ORDER BY applied_at DESC LIMIT 5;"
```

### Verify FTS Configurations

```sql
-- List all matric text search configurations
SELECT cfgname FROM pg_ts_config WHERE cfgname LIKE 'matric_%';

-- Expected output:
-- matric_english
-- matric_simple
-- matric_german
-- matric_french
-- matric_spanish
-- matric_russian
-- matric_portuguese

-- Check pg_trgm extension
SELECT extname, extversion FROM pg_extension WHERE extname = 'pg_trgm';

-- Check PostGIS extension (required for memory search)
SELECT extname, extversion FROM pg_extension WHERE extname = 'postgis';

-- Check pg_bigm extension (optional)
SELECT extname, extversion FROM pg_extension WHERE extname = 'pg_bigm';
```

### Verify Trigram Indexes

```sql
SELECT indexname FROM pg_indexes WHERE indexname LIKE '%trgm%';

-- Expected:
-- idx_note_revised_trgm
-- idx_note_title_trgm
-- idx_skos_label_trgm
```

## Rollback Procedures

### Rollback Container

```bash
# Stop current container
docker compose -f docker-compose.bundle.yml down

# Tag current image for safety
docker tag ghcr.io/fortemi/fortemi:bundle ghcr.io/fortemi/fortemi:bundle-backup

# Pull/checkout previous version
git checkout <previous-commit>
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml up -d
```

### Rollback Migrations

Migration rollback is manual. Each migration file includes rollback instructions:

```sql
-- Example: Rollback Phase 3
DROP INDEX IF EXISTS idx_note_revised_bigm;
DROP INDEX IF EXISTS idx_note_title_bigm;
DROP INDEX IF EXISTS idx_skos_label_bigm;
DROP EXTENSION IF EXISTS pg_bigm;

DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_german;
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_french;
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_spanish;
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_russian;
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_portuguese;

-- Remove from tracking
DELETE FROM _migrations WHERE filename = '20260201300000_multilingual_fts_phase3.sql';
```

## Feature Flags

Multilingual FTS features are controlled by environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `FTS_WEBSEARCH_TO_TSQUERY` | true | Enable OR/NOT/phrase operators |
| `FTS_SCRIPT_DETECTION` | false | Auto-detect query script |
| `FTS_TRIGRAM_FALLBACK` | false | Enable emoji/symbol search |
| `FTS_BIGRAM_CJK` | false | Optimized CJK search |
| `FTS_MULTILINGUAL_CONFIGS` | false | Language-specific stemming |

Enable all multilingual features:

```bash
export FTS_SCRIPT_DETECTION=true
export FTS_TRIGRAM_FALLBACK=true
export FTS_BIGRAM_CJK=true
export FTS_MULTILINGUAL_CONFIGS=true
```

## Ollama Configuration

Ollama is required for AI features (embeddings, revision, title generation).

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_BASE` | `http://host.docker.internal:11434` | Ollama API endpoint |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model |
| `OLLAMA_GEN_MODEL` | `gpt-oss:20b` | Generation model |

### Linux Docker Configuration

On Linux, containers need explicit host mapping to reach Ollama:

```yaml
# docker-compose.bundle.yml
services:
  matric:
    extra_hosts:
      - "host.docker.internal:host-gateway"
```

### Verify Ollama

```bash
# Test from container
docker compose -f docker-compose.bundle.yml exec matric \
  curl http://host.docker.internal:11434/api/tags

# Check required models are available
ollama list
# Should show: nomic-embed-text, gpt-oss:20b (or your configured models)
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker compose -f docker-compose.bundle.yml logs

# Common issues:
# - Port 3000/3001 already in use
# - PostgreSQL data directory permissions
```

### Migration Failed

```bash
# Connect to database
docker compose -f docker-compose.bundle.yml exec matric psql -U matric -d matric

# Check migration status
SELECT * FROM _migrations;

# Run migration manually if needed
\i /path/to/migration.sql
```

### Search Not Working

1. Verify FTS configurations exist:
   ```sql
   SELECT cfgname FROM pg_ts_config WHERE cfgname LIKE 'matric_%';
   ```

2. Check feature flags are enabled (if using advanced features)

3. Test basic search:
   ```bash
   curl "http://localhost:3000/api/v1/search?q=test"
   ```

### CJK Search Not Working

1. Verify pg_trgm is installed:
   ```sql
   SELECT * FROM pg_extension WHERE extname = 'pg_trgm';
   ```

2. Check trigram indexes exist:
   ```sql
   SELECT indexname FROM pg_indexes WHERE indexname LIKE '%trgm%';
   ```

3. Enable feature flag:
   ```bash
   export FTS_TRIGRAM_FALLBACK=true
   ```

## Nginx Reverse Proxy

When deploying with HotM SPA at the site root, nginx configuration requires careful attention to route ordering.

### Critical: OAuth Endpoint Routing

OAuth endpoints (`/.well-known/oauth-authorization-server`, `/oauth/*`) **must be explicitly routed** to the API backend before the SPA catch-all. Without this, OAuth requests receive HTML error pages instead of JSON responses.

See `deploy/nginx/README.md` for complete documentation including:
- Route priority guidelines
- Common mistakes to avoid
- Testing procedures
- Troubleshooting

### Quick Setup

```bash
# Copy and enable nginx config
sudo cp deploy/nginx/your-domain.conf /etc/nginx/sites-available/memory
sudo ln -sf /etc/nginx/sites-available/memory /etc/nginx/sites-enabled/memory
sudo nginx -t && sudo systemctl reload nginx
```

### Verify OAuth Routing

```bash
# Should return JSON metadata, not HTML
curl -s https://your-domain/.well-known/oauth-authorization-server | head -1
# Expected: {"issuer":"https://your-domain",...}
```

## CI/CD Integration

### GitHub/Gitea Actions

The repository uses Actions for:
- Running tests on PR
- Building container on main push
- Tagging releases

### Manual Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create git tag: `git tag -a v2026.1.x -m "Release v2026.1.x"`
4. Push: `git push origin main --tags`
5. Build and deploy container

---

*See also: [CLAUDE.md](../../CLAUDE.md) | [Search Guide](./search-guide.md) | [Nginx Proxy Config](../../deploy/nginx/README.md)*
