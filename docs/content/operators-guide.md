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

## Graph Maintenance

The knowledge graph connects notes by semantic similarity. Over time, as notes are added and updated, edges can drift — weights become skewed, redundant shortcuts accumulate, and community structure degrades. Graph maintenance runs a quality pipeline to keep the graph clean.

### How It Works

Graph maintenance runs four steps in order:

1. **Normalize** — Edge weights are normalized using a gamma correction curve. This step runs automatically during graph traversal; the maintenance job just records the current gamma setting.
2. **SNN** (Shared Nearest Neighbors) — Edges are scored by how many neighbors the two endpoint notes share. Edges below a threshold are pruned. This removes spurious connections caused by embedding noise.
3. **PFNET** (Pathfinder Network) — Geometrically redundant edges are pruned: if a shorter indirect path exists between two notes, the direct edge is removed. This reduces clutter in the graph while preserving all reachable paths.
4. **Snapshot** — A diagnostics snapshot is saved so you can compare graph quality before and after.

### When to Run Maintenance

- After a large bulk import (hundreds of notes)
- After changing the embedding model (all embeddings were regenerated)
- When the graph visualization looks unusually dense or shows spiral/cluster artifacts
- On a regular schedule (e.g., weekly) for large knowledge bases

### Trigger Graph Maintenance

```bash
# Queue a full maintenance run (normalize → SNN → PFNET → snapshot)
curl -X POST http://localhost:3000/api/v1/graph/maintenance

# Run only specific steps
curl -X POST http://localhost:3000/api/v1/graph/maintenance \
  -H "Content-Type: application/json" \
  -d '{"steps": ["snn", "pfnet"]}'
```

Response when queued:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "queued",
  "steps": ["normalize", "snn", "pfnet", "snapshot"]
}
```

Response when a maintenance job is already pending (deduplicated — only one runs at a time):
```json
{
  "id": null,
  "status": "already_pending"
}
```

### Check Graph Quality

Get current graph health statistics:

```bash
curl http://localhost:3000/api/v1/graph/topology/stats
```

Run a deeper diagnostics check (samples 1000 random embedding pairs by default):

```bash
curl "http://localhost:3000/api/v1/graph/diagnostics"

# Sample more pairs for a larger knowledge base
curl "http://localhost:3000/api/v1/graph/diagnostics?sample_size=5000"
```

Key fields to watch in the diagnostics response:

| Field | Healthy Range | What It Means |
|-------|--------------|---------------|
| `embedding_space.anisotropy_score` | 0.0 – 0.3 | Near 0 = isotropic (good). Near 1 = all embeddings point the same direction (poor diversity). |
| `embedding_space.similarity_mean` | 0.1 – 0.6 | Average similarity between random pairs. Very high = embeddings are too similar (model issue). |
| `topology.degree_cv` | < 2.0 | Degree coefficient of variation. Very high = a few notes have thousands of edges (hub problem). |
| `topology.modularity_q` | > 0.3 | Louvain modularity. Low = no meaningful community structure. |
| `normalized_edges.pfnet_retention_ratio` | 0.1 – 0.5 | Fraction of edges surviving PFNET. Very high = graph is dense and not being pruned. |

### Save and Compare Snapshots

Snapshots let you compare graph quality before and after maintenance:

```bash
# Save a snapshot with a label
curl -X POST http://localhost:3000/api/v1/graph/diagnostics/snapshot \
  -H "Content-Type: application/json" \
  -d '{"label": "before-bulk-import"}'

# ... do work ...

curl -X POST http://localhost:3000/api/v1/graph/diagnostics/snapshot \
  -H "Content-Type: application/json" \
  -d '{"label": "after-maintenance"}'

# List saved snapshots
curl "http://localhost:3000/api/v1/graph/diagnostics/history"

# Compare two snapshots (replace UUIDs with actual snapshot IDs)
curl "http://localhost:3000/api/v1/graph/diagnostics/compare?before=UUID1&after=UUID2"
```

The comparison response includes a human-readable `summary` array describing what improved or regressed.

### Summary of Graph Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/graph/topology/stats` | Lightweight topology statistics |
| `GET` | `/api/v1/graph/diagnostics` | Full quality diagnostics (sampling) |
| `POST` | `/api/v1/graph/diagnostics/snapshot` | Save a named snapshot |
| `GET` | `/api/v1/graph/diagnostics/history` | List saved snapshots |
| `GET` | `/api/v1/graph/diagnostics/compare` | Compare two snapshots |
| `POST` | `/api/v1/graph/maintenance` | Trigger full maintenance pipeline |

## Embedding Set Management

Embedding sets group notes for focused semantic search. They let you build a curated or automatically-maintained subset of notes with their own search index — for example, all notes tagged `architecture`, or all notes in a specific collection.

### Set Types

| Type | Description | Use Case |
|------|-------------|----------|
| `filter` (default) | Shares embeddings with the default set. Zero storage overhead. | Searching a known tag or collection subset |
| `full` | Stores its own embeddings from a dedicated config. Can use a different model or MRL truncation. | Domain-specific search, different embedding model, MRL space savings |

### Membership Modes

| Mode | Description |
|------|-------------|
| `auto` (default) | Notes are added automatically when they match the criteria |
| `manual` | Only explicitly added notes are included |
| `mixed` | Auto criteria plus manual additions and exclusions |

### List and Inspect Sets

```bash
# List all embedding sets
curl http://localhost:3000/api/v1/embedding-sets

# Get a specific set by slug or UUID
curl http://localhost:3000/api/v1/embedding-sets/my-set-slug

# List members of a set
curl "http://localhost:3000/api/v1/embedding-sets/my-set-slug/members?limit=50"
```

### Create a Filter Set (Tag-Based)

A filter set automatically includes all notes with the specified tags:

```bash
curl -X POST http://localhost:3000/api/v1/embedding-sets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Architecture Notes",
    "slug": "architecture",
    "description": "All notes tagged with architecture topics",
    "set_type": "filter",
    "mode": "auto",
    "criteria": {
      "tags": ["architecture", "system-design"],
      "exclude_archived": true
    }
  }'
```

### Create a Filter Set (Collection-Based)

```bash
curl -X POST http://localhost:3000/api/v1/embedding-sets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Project Alpha",
    "slug": "project-alpha",
    "set_type": "filter",
    "mode": "auto",
    "criteria": {
      "collections": ["COLLECTION-UUID-HERE"],
      "exclude_archived": true
    }
  }'
```

### Create a Full Set with MRL Truncation

Full sets with MRL-enabled models can store truncated embeddings for storage savings and faster retrieval. Requires an embedding config that supports MRL:

```bash
curl -X POST http://localhost:3000/api/v1/embedding-sets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Code Snippets (MRL)",
    "slug": "code-mrl",
    "set_type": "full",
    "mode": "auto",
    "criteria": {
      "tags": ["code"],
      "exclude_archived": true
    },
    "embedding_config_id": "CONFIG-UUID-HERE",
    "truncate_dim": 128,
    "auto_embed_rules": {
      "on_create": true,
      "on_update": true
    }
  }'
```

### Create a Manual Set

Manual sets only include notes you explicitly add:

```bash
# Create the set
curl -X POST http://localhost:3000/api/v1/embedding-sets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Curated References",
    "slug": "curated-refs",
    "set_type": "filter",
    "mode": "manual"
  }'

# Add notes to it
curl -X POST http://localhost:3000/api/v1/embedding-sets/curated-refs/members \
  -H "Content-Type: application/json" \
  -d '{"note_ids": ["NOTE-UUID-1", "NOTE-UUID-2"]}'

# Remove a note
curl -X DELETE http://localhost:3000/api/v1/embedding-sets/curated-refs/members/NOTE-UUID-1
```

### Refresh a Set

For `auto` and `mixed` sets, refresh re-evaluates the criteria and adds/removes members accordingly. For `manual` sets, refresh re-queues embedding jobs for all members.

```bash
curl -X POST http://localhost:3000/api/v1/embedding-sets/my-set-slug/refresh
```

### Update and Delete a Set

```bash
# Update description or criteria
curl -X PATCH http://localhost:3000/api/v1/embedding-sets/my-set-slug \
  -H "Content-Type: application/json" \
  -d '{"description": "Updated description"}'

# Delete a set (not allowed for system sets)
curl -X DELETE http://localhost:3000/api/v1/embedding-sets/my-set-slug
```

### Embedding Set Criteria Reference

| Field | Type | Description |
|-------|------|-------------|
| `include_all` | bool | Include every note in the archive |
| `tags` | string[] | Include notes with any of these tags |
| `collections` | UUID[] | Include notes in any of these collections |
| `fts_query` | string | Include notes matching this full-text search query |
| `created_after` | datetime | Include notes created after this date |
| `created_before` | datetime | Include notes created before this date |
| `exclude_archived` | bool | Exclude archived notes (default: true) |

### Summary of Embedding Set Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/v1/embedding-sets` | List all sets |
| `POST` | `/api/v1/embedding-sets` | Create a new set |
| `GET` | `/api/v1/embedding-sets/{slug}` | Get a set by slug or UUID |
| `PATCH` | `/api/v1/embedding-sets/{slug}` | Update a set |
| `DELETE` | `/api/v1/embedding-sets/{slug}` | Delete a set |
| `GET` | `/api/v1/embedding-sets/{slug}/members` | List members |
| `POST` | `/api/v1/embedding-sets/{slug}/members` | Add notes to a set |
| `DELETE` | `/api/v1/embedding-sets/{slug}/members/{id}` | Remove a note from a set |
| `POST` | `/api/v1/embedding-sets/{slug}/refresh` | Refresh membership |

## Advanced Diagnostics

### Health Endpoints

| Endpoint | Purpose | Notes |
|----------|---------|-------|
| `GET /health` | Fast health check | Always returns 200 if the API process is running |
| `GET /api/v1/health/live` | Live connectivity check | Probes all backends; returns 503 if PostgreSQL is down |

The `/health` response includes capability flags that reflect what is actually running:

```bash
curl http://localhost:3000/health
```

```json
{
  "status": "healthy",
  "version": "2026.1.0",
  "git_sha": "abc1234",
  "build_date": "2026-01-15",
  "capabilities": {
    "vision": true,
    "audio_transcription": false,
    "ner": true,
    "auth_required": false,
    "extraction_strategies": ["text-native", "pdf-text", "code-ast", "vision", "audio-transcribe"]
  },
  "job_processing": "running"
}
```

**`capabilities` fields:**

| Field | Description |
|-------|-------------|
| `vision` | Ollama vision backend is configured and reachable |
| `audio_transcription` | Whisper-compatible backend is configured |
| `ner` | GLiNER NER backend is configured for concept extraction |
| `auth_required` | Whether `REQUIRE_AUTH=true` is set |
| `extraction_strategies` | List of registered attachment extraction adapters |
| `job_processing` | `"running"` or `"paused"` (reflects global pause state) |

### Live Health Check (Readiness Probe)

Use `/api/v1/health/live` as a readiness probe in container orchestration. It checks each backend concurrently with a 5-second timeout:

```bash
curl http://localhost:3000/api/v1/health/live
```

```json
{
  "status": "healthy",
  "check_duration_ms": 12,
  "services": {
    "postgresql": { "status": "ok" },
    "redis": { "status": "ok" },
    "vision": { "status": "ok" },
    "transcription": { "status": "not_configured" },
    "ner": { "status": "not_configured" }
  }
}
```

**Service statuses:**
- `ok` — reachable and responding
- `not_configured` — backend is not configured (not an error)
- `error` — configured but unreachable (check the `error` field for details)
- `unavailable` — connected but reporting unhealthy

**HTTP status codes:**
- `200` — healthy or degraded (optional services down; PostgreSQL is up)
- `503` — unhealthy (PostgreSQL is unreachable)

### Check Extraction Capabilities

The `extraction_strategies` array in `/health` is the authoritative list of what file types can be processed. Compare it to what you expect to be enabled:

| Strategy | Requires | What It Handles |
|----------|----------|-----------------|
| `text-native` | Always active | Plain text, Markdown |
| `pdf-text` | Always active | PDF text extraction |
| `code-ast` | Always active | Source code files |
| `vision` | `OLLAMA_VISION_MODEL` set | Images (JPEG, PNG, WEBP) |
| `audio-transcribe` | `WHISPER_BASE_URL` set | Audio files (MP3, WAV, M4A) |
| `video-multimodal` | Vision + audio both active | Video files |
| `office-convert` | LibreOffice installed | DOCX, XLSX, PPTX |
| `pdf-ocr` | `OCR_ENABLED=true` | Scanned PDFs |
| `gliner-ner` | `GLINER_BASE_URL` set | Structured entity extraction |

If a strategy you expect is missing, check the container startup logs:

```bash
docker compose -f docker-compose.bundle.yml logs matric | grep "Extraction adapters"
```

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
| Graph visualization shows spirals or tight clusters | Graph needs maintenance | Run `POST /api/v1/graph/maintenance`; check diagnostics for high `anisotropy_score` |
| Notes added but embedding set stays empty | Auto criteria not matching | Inspect criteria with `GET /api/v1/embedding-sets/{slug}`, then `POST /api/v1/embedding-sets/{slug}/refresh` |
| Job queue growing but no jobs completing | Job worker paused | Check `GET /api/v1/jobs/status`; resume with `POST /api/v1/jobs/resume` |
| `extraction_strategies` missing expected adapter | Backend not configured or unreachable | Check startup logs for adapter registration; verify env vars |
| Graph maintenance job not appearing | Already pending (deduplicated) | Response `"status": "already_pending"` is normal; the existing job will run |

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
