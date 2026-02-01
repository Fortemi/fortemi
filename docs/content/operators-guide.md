# Operators Guide

Quick reference for deploying, monitoring, and maintaining matric-memory with Docker.

## Deployment (Docker Bundle)

### First-Time Setup

```bash
# 1. Start container (creates database)
docker compose -f docker-compose.bundle.yml up -d

# 2. Wait for healthy status
docker compose -f docker-compose.bundle.yml logs -f

# 3. Register MCP OAuth client (required for MCP token validation)
curl -X POST https://your-domain.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"MCP Server","grant_types":["client_credentials"],"scope":"mcp read"}'
# Save the returned client_id and client_secret

# 4. Configure environment
cat > .env <<EOF
ISSUER_URL=https://your-domain.com
MCP_CLIENT_ID=mm_xxxxx
MCP_CLIENT_SECRET=xxxxx
EOF

# 5. Restart with configuration
docker compose -f docker-compose.bundle.yml down
docker compose -f docker-compose.bundle.yml up -d

# 6. Verify
curl http://localhost:3000/health
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
```

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

# Full reset (wipes database)
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
docker exec -it matric-memory-matric-1 psql -U matric -d matric

# Run SQL command
docker exec matric-memory-matric-1 psql -U matric -d matric -c "SELECT count(*) FROM notes;"

# Database size
docker exec matric-memory-matric-1 psql -U matric -d matric -c \
  "SELECT pg_size_pretty(pg_database_size('matric'));"
```

## Backup and Recovery

```bash
# Backup database
docker exec matric-memory-matric-1 pg_dump -U matric matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Restore from backup
docker exec -i matric-memory-matric-1 psql -U matric -d matric < backup_YYYYMMDD_HHMMSS.sql
```

## Environment Variables

Set in `.env` file (project root):

| Variable | Required | Description |
|----------|----------|-------------|
| `ISSUER_URL` | Yes | External URL for OAuth/MCP |
| `MCP_CLIENT_ID` | Yes | OAuth client ID for MCP token introspection |
| `MCP_CLIENT_SECRET` | Yes | OAuth client secret |
| `MCP_BASE_URL` | No | MCP resource URL (default: `${ISSUER_URL}/mcp`) |

Container variables (docker-compose.bundle.yml):

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Logging level |
| `RATE_LIMIT_ENABLED` | `false` | Rate limiting |

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| MCP auth fails with "localhost" URL | Missing `ISSUER_URL` | Add `ISSUER_URL` to `.env`, restart |
| MCP returns "unauthorized" with valid token | Missing MCP credentials | Register OAuth client, add `MCP_CLIENT_ID/SECRET` to `.env` |
| Container unhealthy | Startup still in progress | Wait 60s, check logs |
| "column X does not exist" | Old image | Rebuild: `docker compose build` |
| Connection refused on :3000 | Container not running | `docker compose up -d` |

## MCP Server

The MCP server runs automatically on port 3001 in the Docker bundle.

### Verify MCP Configuration

```bash
# Check OAuth metadata returns correct URL
curl https://your-domain.com/mcp/.well-known/oauth-protected-resource
# Should show: "resource": "https://your-domain.com/mcp"
```

### Claude Code Integration

Project `.mcp.json`:
```json
{
  "mcpServers": {
    "matric-memory": {
      "url": "https://your-domain.com/mcp"
    }
  }
}
```

See [MCP documentation](./mcp.md) for details.
