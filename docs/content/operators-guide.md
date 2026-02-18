# Operators Guide

Quick reference for deploying, monitoring, and maintaining Fortemi with Docker.

## Deployment (Docker Bundle)

### First-Time Setup

```bash
# 1. Configure URL (required for OAuth/MCP)
cat > .env <<EOF
ISSUER_URL=http://localhost:3000
EOF

# 2. Start container (initializes database, auto-registers MCP credentials)
docker compose -f docker-compose.bundle.yml up -d

# 3. Wait for healthy status
docker compose -f docker-compose.bundle.yml logs -f
# Look for: "=== Matric Memory Bundle Ready ==="

# 4. Verify
curl http://localhost:3000/health
curl http://localhost:3001/.well-known/oauth-protected-resource
```

MCP OAuth credentials are managed automatically. The bundle registers an OAuth client on startup and persists credentials on the database volume. No manual credential configuration is needed.

### Standard Deployment

```bash
# Pull latest code
git pull origin main

# Rebuild and restart
docker compose -f docker-compose.bundle.yml build
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Verify
docker compose -f docker-compose.bundle.yml logs -f
curl http://localhost:3000/health
```

### Container Management

```bash
# Status
docker compose -f docker-compose.bundle.yml ps

# Logs
docker compose -f docker-compose.bundle.yml logs -f
docker compose -f docker-compose.bundle.yml logs --tail=100

# Restart
docker compose -f docker-compose.bundle.yml restart

# Stop
docker compose -f docker-compose.bundle.yml down

# Clean restart (preserves data)
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# Full reset (wipes database — MCP credentials auto-regenerate)
docker compose -f docker-compose.bundle.yml down -v
docker compose -f docker-compose.bundle.yml up -d
```

## Health Checks

| Endpoint | Expected | Purpose |
|----------|----------|---------|
| `GET /health` | `200 OK` | API availability |
| `GET /api/v1/notes?limit=1` | `200 OK` | Database connectivity |
| `GET /mcp/.well-known/oauth-protected-resource` | `200 OK` | MCP OAuth metadata |

## Database Operations

```bash
# Connect to database inside container
docker exec -it Fortémi-matric-1 psql -U matric -d matric

# Run SQL command
docker exec Fortémi-matric-1 psql -U matric -d matric -c "SELECT count(*) FROM notes;"

# Database size
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT pg_size_pretty(pg_database_size('matric'));"
```

## Backup and Recovery

```bash
# Backup database
docker exec Fortémi-matric-1 pg_dump -U matric matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Restore from backup
docker exec -i Fortémi-matric-1 psql -U matric -d matric < backup_YYYYMMDD_HHMMSS.sql
```

## Environment Variables

Set in `.env` file (project root):

| Variable | Required | Description |
|----------|----------|-------------|
| `ISSUER_URL` | Yes | External URL for OAuth/MCP |
| `MCP_CLIENT_ID` | No | OAuth client ID (auto-managed, set only for manual override) |
| `MCP_CLIENT_SECRET` | No | OAuth client secret (auto-managed, set only for manual override) |
| `MCP_BASE_URL` | No | MCP resource URL (default: `${ISSUER_URL}/mcp`) |

Container variables (docker-compose.bundle.yml):

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Logging level |
| `RATE_LIMIT_ENABLED` | `false` | Rate limiting |
| `OLLAMA_BASE` | `http://host.docker.internal:11434` | Ollama API endpoint |
| `OLLAMA_EMBED_MODEL` | `nomic-embed-text` | Embedding model name |
| `OLLAMA_GEN_MODEL` | `gpt-oss:20b` | Generation model name |
| `EXTRACTION_TARGET_CONCEPTS` | `5` | Target number of concepts to extract per note (GLiNER→fast model escalation triggers below this; fast→standard model escalation triggers below half this value) |
| `JOB_MAX_CONCURRENT` | `4` | Maximum number of background jobs to process concurrently |

## Ollama Configuration

The AI features (embeddings, revision, title generation) require Ollama running on the host machine.

### Linux Docker Setup

On Linux, Docker containers need special configuration to reach the host's Ollama service:

```yaml
# docker-compose.bundle.yml
services:
  matric:
    extra_hosts:
      - "host.docker.internal:host-gateway"
    environment:
      - OLLAMA_BASE=http://host.docker.internal:11434
```

### Verify Ollama Connectivity

```bash
# Test from inside container
docker compose -f docker-compose.bundle.yml exec matric \
  curl http://host.docker.internal:11434/api/tags

# Should return JSON with available models
```

### Troubleshooting Ollama

| Symptom | Cause | Fix |
|---------|-------|-----|
| Embedding jobs fail with "connection refused" | Ollama not reachable | Add `extra_hosts` mapping |
| Jobs stuck in "failed" state | Old Ollama URL cached | Reset jobs: `UPDATE job_queue SET status = 'pending' WHERE status = 'failed'` |
| "Model not found" errors | Missing model | Run `ollama pull nomic-embed-text` on host |

## Background Jobs

Background jobs handle embedding generation, concept extraction, link detection, and other async processing. The job worker runs inside the bundle container alongside the API.

### Pause/Resume Job Processing

Pausing job processing is useful during:

- **Bulk imports** — prevent the job queue from overwhelming Ollama while ingesting large datasets
- **Maintenance windows** — stop background activity before database backups or schema changes
- **Performance issues** — reduce load when the host machine is under resource pressure

Pausing stops the worker from picking up new jobs. Any job already running will complete normally. State is persisted in the `system_config` table and survives container restarts — remember to resume when the maintenance window ends.

#### Check Current State

```bash
curl http://localhost:3000/api/v1/jobs/status
```

Response example:

```json
{
  "paused": false,
  "paused_archives": []
}
```

#### Pause All Processing

```bash
curl -X POST http://localhost:3000/api/v1/jobs/pause
```

All archives stop picking up new jobs. Already-running jobs finish before the worker idles.

#### Resume All Processing

```bash
curl -X POST http://localhost:3000/api/v1/jobs/resume
```

The worker immediately begins picking up queued jobs again.

#### Pause a Specific Archive

```bash
# Replace "default" with your archive name
curl -X POST http://localhost:3000/api/v1/jobs/pause/default
```

Only jobs belonging to that archive are paused. Other archives continue processing normally.

#### Resume a Specific Archive

```bash
curl -X POST http://localhost:3000/api/v1/jobs/resume/default
```

#### With Authentication

If `REQUIRE_AUTH=true`, include your Bearer token:

```bash
curl -X POST http://localhost:3000/api/v1/jobs/pause \
  -H "Authorization: Bearer mm_at_xxxx"
```

#### Summary of Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/jobs/status` | Check current pause state |
| `POST` | `/api/v1/jobs/pause` | Pause all job processing |
| `POST` | `/api/v1/jobs/resume` | Resume all job processing |
| `POST` | `/api/v1/jobs/pause/{archive}` | Pause processing for one archive |
| `POST` | `/api/v1/jobs/resume/{archive}` | Resume processing for one archive |

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| MCP auth fails with "localhost" URL | Missing `ISSUER_URL` | Add `ISSUER_URL` to `.env`, restart |
| MCP returns "unauthorized" with valid token | Missing MCP credentials | Check startup logs, restart container (auto-registers on startup) |
| Container unhealthy | Startup still in progress | Wait 60s, check logs |
| "column X does not exist" | Old image | Rebuild: `docker compose build` |
| Connection refused on :3000 | Container not running | `docker compose up -d` |
| Embedding jobs failing | Ollama not reachable from container | Add `extra_hosts: host.docker.internal:host-gateway` to docker-compose |
| Tags not matching | Case mismatch or hierarchy | Tags are case-insensitive and support hierarchical matching |

## MCP Server

The MCP server runs automatically on port 3001 in the Docker bundle. OAuth credentials are auto-managed — no manual registration needed.

### Verify MCP Status

```bash
# Check startup logs for credential status
docker compose -f docker-compose.bundle.yml logs matric | grep -E "MCP|credential"
# Expected: "MCP credentials valid" or "Registered MCP client: mm_xxxxx"

# Check OAuth metadata returns correct URL
curl http://localhost:3001/.well-known/oauth-protected-resource
# Should show: "resource": "http://localhost:3000/mcp"
```

For advanced credential management and security considerations, see the [MCP Deployment Guide](./mcp-deployment.md).

### Claude Code Integration

Project `.mcp.json`:
```json
{
  "mcpServers": {
    "fortemi": {
      "url": "http://localhost:3001"
    }
  }
}
```

See [MCP documentation](./mcp.md) for details.

## File Attachments

File attachments are stored using content-addressable storage with BLAKE3 hashing for automatic deduplication.

### Storage Locations

| Storage | Threshold | Location |
|---------|-----------|----------|
| Inline (database) | < 10 MB | `attachment_blob.data` column |
| Filesystem | ≥ 10 MB | `blobs/{aa}/{bb}/{uuid}.bin` |

### Check Storage Usage

```bash
# Attachment blob sizes
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT storage_backend, count(*), pg_size_pretty(sum(size_bytes)) as total_size
   FROM attachment_blob GROUP BY storage_backend;"

# Deduplication stats
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT 'unique_blobs' as metric, count(*) FROM attachment_blob
   UNION ALL
   SELECT 'total_attachments', count(*) FROM attachment
   UNION ALL
   SELECT 'space_saved_bytes', sum(ab.size_bytes * (ref_count - 1))
   FROM attachment_blob ab
   JOIN (SELECT blob_id, count(*) as ref_count FROM attachment GROUP BY blob_id) refs
   ON ab.id = refs.blob_id;"
```

### Cleanup Orphaned Blobs

Blobs without references can be cleaned up safely:

```bash
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "DELETE FROM attachment_blob
   WHERE id NOT IN (SELECT DISTINCT blob_id FROM attachment);"
```

### File Safety Validation

Blocked file types include executables (.exe, .dll, .sh, .bat), scripts (.ps1, .vbs), and other dangerous formats.

Check quarantined files:

```bash
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT id, filename, status, error_message FROM attachment WHERE status = 'quarantined';"
```

See [File Attachments documentation](./file-attachments.md) for details.

## Memory Search (PostGIS)

Temporal-spatial queries require PostGIS extension.

### Verify PostGIS

```bash
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT PostGIS_Version();"
```

### Example Location Query

```bash
# Find memories within 1km of coordinates (lat: 48.8584, lon: 2.2945)
docker exec Fortémi-matric-1 psql -U matric -d matric -c \
  "SELECT fp.attachment_id, a.filename,
          ST_Distance(pl.point, ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography) as distance_m
   FROM file_provenance fp
   JOIN attachment a ON fp.attachment_id = a.id
   JOIN prov_location pl ON fp.location_id = pl.id
   WHERE ST_DWithin(pl.point, ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography, 1000)
   ORDER BY distance_m LIMIT 10;"
```

See [Memory Search documentation](./memory-search.md) for details.
