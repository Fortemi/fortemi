# Operations and Deployment Guide

This guide covers deployment, operations, and troubleshooting for Fortémi using Docker.

## System Overview

- **Deployment:** Docker bundle (all-in-one container)
- **Components:** PostgreSQL 16 + pgvector + PostGIS, Rust API, Node.js MCP server
- **Ports:** 3000 (API), 3001 (MCP)
- **Data:** PostgreSQL data in Docker volume `matric-pgdata`

## Table of Contents

1. [Initial Setup](#initial-setup)
2. [Deployment Procedures](#deployment-procedures)
3. [Container Management](#container-management)
4. [Database Operations](#database-operations)
5. [MCP Server Operations](#mcp-server-operations)
6. [Monitoring and Health Checks](#monitoring-and-health-checks)
7. [Troubleshooting](#troubleshooting)
8. [Backup and Recovery](#backup-and-recovery)
9. [Configuration](#configuration)

## Initial Setup

### Prerequisites

- Docker and Docker Compose
- Nginx (for reverse proxy)
- Domain with SSL certificate

### First-Time Deployment

```bash
# 1. Clone repository
git clone https://github.com/Fortemi/fortemi.git
cd Fortémi

# 2. Start container (creates database)
docker compose -f docker-compose.bundle.yml up -d

# 3. Wait for initialization (first run takes ~60 seconds)
docker compose -f docker-compose.bundle.yml logs -f

# 4. Register MCP OAuth client (REQUIRED for MCP authentication)
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'
# Save the returned client_id and client_secret

# 5. Configure environment
cat > .env <<EOF
ISSUER_URL=https://your-domain.com
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx
EOF

# 6. Restart with configuration
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# 7. Verify
curl http://localhost:3000/health
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
```

### Nginx Configuration

Configure nginx to proxy to the container:

```nginx
server {
    listen 443 ssl http2;
    server_name your-domain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    # API routes
    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket and SSE support for real-time events
    location /api/v1/ws {
        proxy_pass http://localhost:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400;
    }

    location /api/v1/events {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header Connection '';
        proxy_http_version 1.1;
        chunked_transfer_encoding off;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400;
    }

    # MCP routes
    location = /mcp {
        proxy_pass http://localhost:3001/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }

    location /mcp/ {
        proxy_pass http://localhost:3001/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Deployment Procedures

### Standard Update Workflow

```bash
# 1. Pull latest code
git pull origin main

# 2. Backup database (recommended)
docker exec Fortémi-matric-1 pg_dump -U matric matric > backup_$(date +%Y%m%d_%H%M%S).sql

# 3. Rebuild and restart
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# 4. Verify
curl http://localhost:3000/health
docker compose -f docker-compose.bundle.yml logs --tail=50
```

### Critical Rules

1. **Backup before major updates** - Database migrations run automatically on container start
2. **Check logs after restart** - Verify migrations applied successfully
3. **Test health endpoint** - Confirm API is responding

### Rollback Procedure

If deployment fails:

```bash
# 1. Stop container
docker compose -f docker-compose.bundle.yml down

# 2. Restore database from backup
docker compose -f docker-compose.bundle.yml up -d
sleep 30  # Wait for PostgreSQL to start
docker exec -i Fortémi-matric-1 psql -U matric -d matric < backup_YYYYMMDD_HHMMSS.sql

# 3. Checkout previous version
git checkout <previous-commit>

# 4. Rebuild with old code
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# 5. Verify
curl http://localhost:3000/health
```

## Container Management

### Common Commands

```bash
# Status
docker compose -f docker-compose.bundle.yml ps

# Logs (follow)
docker compose -f docker-compose.bundle.yml logs -f

# Logs (last N lines)
docker compose -f docker-compose.bundle.yml logs --tail=100

# Restart
docker compose -f docker-compose.bundle.yml restart

# Stop
docker compose -f docker-compose.bundle.yml down

# Start
docker compose -f docker-compose.bundle.yml up -d

# Rebuild
docker compose -f docker-compose.bundle.yml build

# Shell access
docker exec -it Fortémi-matric-1 /bin/bash
```

### Full Reset (Wipes Database)

```bash
docker compose -f docker-compose.bundle.yml down -v
docker compose -f docker-compose.bundle.yml up -d
```

## Database Operations

### Connection

```bash
# Interactive psql
docker exec -it Fortémi-matric-1 psql -U matric -d matric

# Run single command
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT count(*) FROM notes;"
```

### Common Queries

```bash
# Database size
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT pg_size_pretty(pg_database_size('matric'));"

# Table sizes
docker exec Fortémi-matric-1 psql -U matric -d matric -c "
SELECT relname AS table_name, pg_size_pretty(pg_total_relation_size(relid)) AS size
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;"

# Note count
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT count(*) FROM notes;"

# Active connections
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT count(*) FROM pg_stat_activity WHERE datname = 'matric';"
```

### Maintenance

```bash
# Vacuum analyze (weekly recommended)
docker exec Fortémi-matric-1 psql -U matric -d matric -c "VACUUM ANALYZE;"

# Refresh embedding set stats
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "REFRESH MATERIALIZED VIEW embedding_set_stats;"

# Reindex (if query performance degrades)
docker exec Fortémi-matric-1 psql -U matric -d matric -c "REINDEX DATABASE matric;"
```

## MCP Server Operations

The MCP server runs automatically inside the Docker bundle on port 3001.

### Verify MCP Configuration

```bash
# Check OAuth protected resource metadata
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource

# Expected response:
# {
#   "resource": "https://your-domain.com/mcp",
#   "authorization_servers": ["https://your-domain.com"],
#   ...
# }
```

### Claude Code Integration

Project `.mcp.json`:

```json
{
  "mcpServers": {
    "fortemi": {
      "url": "https://your-domain.com/mcp"
    }
  }
}
```

### MCP Health Check

```bash
curl https://your-domain.com/mcp/health
```

## Monitoring and Health Checks

### Health Endpoints

```bash
# API health
curl http://localhost:3000/health

# MCP health (via nginx)
curl https://your-domain.com/mcp/health
```

### Real-Time Event Monitoring

Fortémi provides real-time event streaming for live job and note monitoring. See [Real-Time Events](./real-time-events.md) for full documentation.

**SSE for live job monitoring:**

```bash
# Stream all events (useful for monitoring job processing)
curl -N http://localhost:3000/api/v1/events
```

Events include `QueueStatus` (every 5s), `JobQueued`, `JobStarted`, `JobProgress`, `JobCompleted`, `JobFailed`, and `NoteUpdated`.

**Webhook alerts for failures:**

Set up a webhook to receive alerts when jobs fail:

```bash
# Create webhook for failure alerts
curl -X POST http://localhost:3000/api/v1/webhooks \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://your-alerting-service.com/webhook",
    "events": ["JobFailed"],
    "secret": "your-hmac-secret"
  }'
```

**WebSocket for dashboard integration:**

Connect to `ws://localhost:3000/api/v1/ws` for real-time dashboard updates. Send `"refresh"` to trigger an immediate queue status broadcast.

### Container Health

```bash
# Docker health status
docker inspect Fortémi-matric-1 --format='{{.State.Health.Status}}'

# Recent health check results
docker inspect Fortémi-matric-1 --format='{{json .State.Health}}' | jq
```

### Log Analysis

```bash
# All logs
docker compose -f docker-compose.bundle.yml logs

# Errors only
docker compose -f docker-compose.bundle.yml logs 2>&1 | grep -i error

# Since specific time
docker compose -f docker-compose.bundle.yml logs --since "1h"
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker compose -f docker-compose.bundle.yml logs

# Check if port is in use
ss -tlnp | grep -E '3000|3001'

# Verify Docker is running
docker ps
```

### MCP Authentication Fails

**Symptom:** "Protected resource URL mismatch" error

**Cause:** Missing or incorrect `ISSUER_URL` in `.env`

**Fix:**
```bash
# Create/update .env with ISSUER_URL
echo "ISSUER_URL=https://your-domain.com" >> .env

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Verify
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
```

**Symptom:** MCP returns "unauthorized" even with valid token

**Cause:** Missing `MCP_CLIENT_ID` and `MCP_CLIENT_SECRET` - the MCP server cannot introspect tokens

**Fix:**
```bash
# Register an OAuth client for MCP
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'

# Add returned credentials to .env
echo "MCP_CLIENT_ID=mm_xxxxx" >> .env
echo "MCP_CLIENT_SECRET=xxxxx" >> .env

# Restart container
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

### Database Connection Errors

```bash
# Check PostgreSQL is running inside container
docker exec Fortémi-matric-1 pg_isready -U matric

# Check database exists
docker exec Fortémi-matric-1 psql -U matric -l

# Verify required extensions
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT extname, extversion FROM pg_extension WHERE extname IN ('vector', 'postgis');"
```

### Slow Performance

```bash
# Run vacuum
docker exec Fortémi-matric-1 psql -U matric -d matric -c "VACUUM ANALYZE;"

# Check for long-running queries
docker exec Fortémi-matric-1 psql -U matric -d matric -c "
SELECT pid, state, query_start, query
FROM pg_stat_activity
WHERE state = 'active' AND datname = 'matric';"
```

### Out of Disk Space

```bash
# Check Docker disk usage
docker system df

# Clean unused images
docker image prune -a

# Check volume size
docker system df -v | grep matric
```

## Backup and Recovery

### Manual Backup

```bash
# Backup to local file
docker exec Fortémi-matric-1 pg_dump -U matric matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Verify backup
ls -lh backup_*.sql | tail -1
head -50 backup_*.sql | tail -1
```

### Restore from Backup

```bash
# Stop and start fresh container (preserves volume)
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Wait for PostgreSQL
sleep 30

# Restore
docker exec -i Fortémi-matric-1 psql -U matric -d matric < backup_YYYYMMDD_HHMMSS.sql

# Verify
curl http://localhost:3000/health
```

### Automated Backup Script

```bash
#!/bin/bash
# backup-matric.sh
BACKUP_DIR="/path/to/backups"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/matric_$DATE.sql"

mkdir -p "$BACKUP_DIR"
docker exec Fortémi-matric-1 pg_dump -U matric matric > "$BACKUP_FILE"
gzip "$BACKUP_FILE"

# Keep only last 7 days
find "$BACKUP_DIR" -name "matric_*.sql.gz" -mtime +7 -delete

echo "Backup completed: ${BACKUP_FILE}.gz"
```

Add to crontab for daily backups:
```bash
0 2 * * * /path/to/backup-matric.sh
```

## Configuration

### Environment Variables Reference

All environment variables are optional unless marked as required. The API reads these values at startup.

#### Server Configuration

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `HOST` | `0.0.0.0` | HTTP server bind address | `127.0.0.1` |
| `PORT` | `3000` | HTTP server port | `8080` |
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection string | `postgres://user:pass@host:5432/db` |
| `FILE_STORAGE_PATH` | `/var/lib/matric/files` | Directory for file attachments | `/mnt/storage/files` |

#### Rate Limiting

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `RATE_LIMIT_ENABLED` | `true` | Enable rate limiting | `false` |
| `RATE_LIMIT_REQUESTS` | `100` | Max requests per period | `1000` |
| `RATE_LIMIT_PERIOD_SECS` | `60` | Rate limit period in seconds | `300` |

#### CORS Configuration

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `ALLOWED_ORIGINS` | `https://memory.integrolabs.net,http://localhost:3000` | Comma-separated CORS origins | `https://app.example.com,https://staging.example.com` |

#### Logging

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `RUST_LOG` | `matric_api=debug,tower_http=debug` | Tracing filter directives | `info` or `matric_api=trace` |
| `LOG_FORMAT` | `text` | Log output format (`text` or `json`) | `json` |
| `LOG_FILE` | (none) | Path to log file (enables file logging) | `/var/log/matric/api.log` |
| `LOG_ANSI` | (auto-detected) | Force ANSI colors in logs (`true` or `false`) | `false` |

#### OAuth / MCP

| Variable | Required | Default | Description | Example |
|----------|----------|---------|-------------|---------|
| `ISSUER_URL` | Yes | `http://HOST:PORT` | External OAuth issuer URL | `https://memory.example.com` |
| `MCP_CLIENT_ID` | Yes | (none) | OAuth client ID for MCP server token introspection | `mm_abc123` |
| `MCP_CLIENT_SECRET` | Yes | (none) | OAuth client secret for MCP server | `secret_xyz789` |
| `MCP_BASE_URL` | No | `${ISSUER_URL}/mcp` | MCP protected resource URL | `https://memory.example.com/mcp` |

#### Ollama Backend (Primary AI Backend)

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `OLLAMA_BASE` | `http://127.0.0.1:11434` | Ollama API base URL | `http://host.docker.internal:11434` |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Ollama embedding model name | `mxbai-embed-large` |
| `OLLAMA_GEN_MODEL` | `gpt-oss:20b` | Ollama generation model name | `llama3.2` |
| `OLLAMA_EMBED_DIM` | `768` | Embedding vector dimension | `1024` |
| `OLLAMA_HOST` | `http://localhost:11434` | Ollama host for model discovery | `http://ollama:11434` |

#### OpenAI Backend (Alternative AI Backend)

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `OPENAI_API_KEY` | (none) | OpenAI API key | `sk-proj-...` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | OpenAI API base URL | `https://api.openai.com/v1` |
| `OPENAI_EMBED_MODEL` | `text-embedding-3-small` | OpenAI embedding model | `text-embedding-3-large` |
| `OPENAI_GEN_MODEL` | `gpt-4o-mini` | OpenAI generation model | `gpt-4o` |
| `OPENAI_EMBED_DIM` | `1536` | OpenAI embedding dimension | `3072` |
| `OPENAI_TIMEOUT` | `60` | OpenAI request timeout (seconds) | `120` |
| `OPENAI_SKIP_TLS_VERIFY` | `false` | Skip TLS certificate verification | `true` |
| `OPENAI_HTTP_REFERER` | (none) | HTTP Referer header for OpenAI requests | `https://example.com` |
| `OPENAI_X_TITLE` | (none) | X-Title header for OpenAI requests | `My App` |

#### Advanced Inference Configuration

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `MATRIC_INFERENCE_DEFAULT` | `ollama` | Default inference backend (`ollama` or `openai`) | `openai` |
| `MATRIC_OLLAMA_URL` | `http://localhost:11434` | Ollama URL (alternative to `OLLAMA_BASE`) | `http://ollama:11434` |
| `MATRIC_OLLAMA_GENERATION_MODEL` | `gpt-oss:20b` | Ollama generation model | `llama3.2` |
| `MATRIC_OLLAMA_EMBEDDING_MODEL` | `nomic-embed-text` | Ollama embedding model | `mxbai-embed-large` |
| `MATRIC_OPENAI_URL` | `https://api.openai.com/v1` | OpenAI URL | `https://custom-proxy.example.com/v1` |
| `MATRIC_OPENAI_API_KEY` | (none) | OpenAI API key | `sk-proj-...` |
| `MATRIC_OPENAI_GENERATION_MODEL` | `gpt-4o-mini` | OpenAI generation model | `gpt-4o` |
| `MATRIC_OPENAI_EMBEDDING_MODEL` | `text-embedding-3-small` | OpenAI embedding model | `text-embedding-3-large` |
| `MATRIC_GEN_TIMEOUT_SECS` | `120` | Generation request timeout (seconds) | `180` |
| `MATRIC_EMBED_TIMEOUT_SECS` | `30` | Embedding request timeout (seconds) | `60` |

#### Job Worker

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `WORKER_ENABLED` | `true` | Enable background job worker | `false` |

#### Real-Time Events

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `MATRIC_EVENT_BUS_CAPACITY` | `256` | Event bus broadcast channel capacity | `1024` |
| `MATRIC_WEBHOOK_TIMEOUT_SECS` | `10` | Webhook HTTP request timeout (seconds) | `30` |
| `MATRIC_MAX_BODY_SIZE_BYTES` | `2147483648` | Maximum request body size (2 GB for database backups) | `1073741824` |

#### Full-Text Search (FTS)

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `FTS_WEBSEARCH_TO_TSQUERY` | `true` | Enable websearch syntax (OR, NOT, phrase) | `false` |
| `FTS_TRIGRAM_FALLBACK` | `true` | Enable trigram search for emoji/symbols | `false` |
| `FTS_BIGRAM_CJK` | `true` | Enable bigram search for CJK text | `false` |
| `FTS_SCRIPT_DETECTION` | `true` | Auto-detect query language script | `false` |
| `FTS_MULTILINGUAL_CONFIGS` | `true` | Enable language-specific text search configs | `false` |

#### Redis Cache

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `REDIS_ENABLED` | `true` | Enable Redis search result caching | `false` |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL | `redis://redis:6379/0` |
| `REDIS_CACHE_TTL` | `300` | Cache TTL in seconds (5 minutes) | `600` |

#### Backup Operations

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `BACKUP_DEST` | `/var/backups/matric-memory` | Backup destination directory | `/mnt/backups` |
| `BACKUP_SCRIPT_PATH` | `/usr/local/bin/backup-matric.sh` | Path to backup script | `/opt/scripts/backup.sh` |

#### PostgreSQL (Bundle Deployment Only)

| Variable | Default | Description | Example |
|----------|---------|-------------|---------|
| `POSTGRES_USER` | `matric` | PostgreSQL superuser | `postgres` |
| `POSTGRES_PASSWORD` | `matric` | PostgreSQL password | `secure_password` |
| `POSTGRES_DB` | `matric` | PostgreSQL database name | `matric_prod` |

### Configuration Precedence

Environment variables are read with the following precedence:

1. **System environment variables** (highest priority)
2. **`.env` file** (loaded via `dotenvy::dotenv()`)
3. **Hard-coded defaults** in `crates/matric-core/src/defaults.rs`

### AI Features Configuration

AI features (embedding generation, auto-titling, AI revision) require either Ollama or OpenAI to be configured.

**Using Ollama (local/self-hosted):**

1. Install and run Ollama on your host machine
2. Pull required models:
   ```bash
   ollama pull nomic-embed-text
   ollama pull gpt-oss:20b
   ```
3. Configure Docker to access Ollama:
   ```bash
   # For Docker Desktop (macOS/Windows)
   OLLAMA_BASE=http://host.docker.internal:11434

   # For Linux with Ollama on same host
   OLLAMA_BASE=http://172.17.0.1:11434
   ```
4. Add to `.env` or uncomment in `docker-compose.bundle.yml`:
   ```
   OLLAMA_BASE=http://host.docker.internal:11434
   OLLAMA_EMBED_MODEL=nomic-embed-text
   OLLAMA_GEN_MODEL=gpt-oss:20b
   ```

**Using OpenAI:**

Add to `.env`:
```
OPENAI_API_KEY=sk-proj-...your-key...
OPENAI_EMBED_MODEL=text-embedding-3-small
OPENAI_GEN_MODEL=gpt-4o-mini
```

**Verify AI Features:**

```bash
# Create a test note and request embedding generation
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{"content": "Test note for embedding generation"}'

# Check job queue for embedding job
curl http://localhost:3000/api/v1/jobs?type=embedding

# View logs for embedding/generation errors
docker compose -f docker-compose.bundle.yml logs | grep -i "ollama\|openai\|embedding"
```

**Common AI Issues:**

| Symptom | Cause | Fix |
|---------|-------|-----|
| Embedding jobs stuck | Ollama not reachable | Set `OLLAMA_BASE` env var |
| Auto-titling not working | No LLM configured | Configure Ollama or OpenAI |
| "connection refused" errors | Wrong Ollama host | Use `host.docker.internal` for Docker Desktop |

### Modifying Configuration

```bash
# Edit .env for external URLs
nano .env

# Edit docker-compose for container settings
nano docker-compose.bundle.yml

# Apply changes
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d
```

## Quick Reference

### Daily Operations

```bash
# Check status
docker compose -f docker-compose.bundle.yml ps
curl http://localhost:3000/health

# View logs
docker compose -f docker-compose.bundle.yml logs --tail=50
```

### Weekly Maintenance

```bash
# Vacuum database
docker exec Fortémi-matric-1 psql -U matric -d matric -c "VACUUM ANALYZE;"

# Backup
docker exec Fortémi-matric-1 pg_dump -U matric matric > backup_$(date +%Y%m%d).sql
```

### Emergency Procedures

```bash
# Quick restart
docker compose -f docker-compose.bundle.yml restart

# Full restart
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Restore from backup
docker exec -i Fortémi-matric-1 psql -U matric -d matric < latest_backup.sql
```

## Resources

- **Repository:** https://github.com/Fortemi/fortemi
- **Operators Guide:** [operators-guide.md](./operators-guide.md)
- **MCP Documentation:** [mcp-server/README.md](../../mcp-server/README.md)
- **Real-Time Events:** [real-time-events.md](./real-time-events.md)
