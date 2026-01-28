# Operators Guide

Quick reference for deploying, monitoring, and maintaining matric-memory.

For comprehensive procedures, see the full [Operations and Deployment Guide](./operations.md).

## Deployment Checklist

```bash
# 1. Pull latest code
git pull origin main

# 2. Backup database (mandatory before migrations)
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql

# 3. Apply pending migrations
ls -lt migrations/
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/<new_file>.sql

# 4. Build release
cargo build --release

# 5. Restart and verify
sudo systemctl restart matric-api
curl http://localhost:3000/health
```

## Service Management

```bash
systemctl status matric-api        # Check status
sudo systemctl restart matric-api  # Restart
journalctl -u matric-api -f        # Tail logs
journalctl -u matric-api --since "1 hour ago"  # Recent logs
```

## Health Checks

| Endpoint | Expected | Purpose |
|----------|----------|---------|
| `GET /health` | `200 OK` | API availability |
| `GET /api/v1/notes?limit=1` | `200 OK` | Database connectivity |
| `GET /docs` | `200 OK` | Swagger UI |

## Database Operations

```bash
# Connection
PGPASSWORD=matric psql -U matric -h localhost -d matric

# Table sizes
SELECT relname, pg_size_pretty(pg_total_relation_size(relid))
FROM pg_catalog.pg_statio_user_tables ORDER BY pg_total_relation_size(relid) DESC;

# Active connections
SELECT count(*) FROM pg_stat_activity WHERE datname = 'matric';

# Reindex vectors (after bulk imports)
REINDEX INDEX idx_embedding_vector;
```

## Backup and Recovery

```bash
# Full backup
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Restore from backup
psql -U matric -h localhost -d matric < backup_YYYYMMDD_HHMMSS.sql
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://localhost/matric` | PostgreSQL connection |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `3000` | API port |
| `OLLAMA_URL` | `http://localhost:11434` | Inference backend |
| `RUST_LOG` | `matric_api=debug` | Logging level |

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| "column X does not exist" | Missing migration | Run pending migration SQL files |
| Connection refused on :3000 | Service not running | `sudo systemctl restart matric-api` |
| Slow search responses | Stale vector index | `REINDEX INDEX idx_embedding_vector;` |
| High memory usage | Connection pool exhaustion | Check `pg_stat_activity`, restart service |

## MCP Server

```bash
# Start MCP server (stdio mode for Claude Desktop)
cd mcp-server && node index.js

# HTTP mode (for remote access)
MCP_TRANSPORT=http node index.js
```

See [MCP documentation](./mcp.md) for integration details.
