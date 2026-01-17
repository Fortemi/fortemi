# Operations and Deployment Guide

This guide covers day-to-day operations, deployment procedures, and troubleshooting for Matric Memory running on a single Linux server.

## System Overview

- **Platform:** Linux (systemd-based)
- **API Server:** Rust/Axum HTTP server (matric-api)
- **Database:** PostgreSQL 16 with pgvector extension
- **MCP Server:** Node.js (optional, for Claude integration)
- **Binary Location:** `/home/roctinam/dev/matric-memory/target/release/matric-api`
- **Service:** systemd unit `matric-api.service`

## Table of Contents

1. [Deployment Procedures](#deployment-procedures)
2. [Service Management](#service-management)
3. [Database Operations](#database-operations)
4. [MCP Server Operations](#mcp-server-operations)
5. [Monitoring and Health Checks](#monitoring-and-health-checks)
6. [Troubleshooting](#troubleshooting)
7. [Backup and Recovery](#backup-and-recovery)
8. [Configuration](#configuration)

## Deployment Procedures

### Standard Deployment Workflow

Follow these steps in order. Do not skip steps.

```bash
# 1. Pull latest code
git pull origin main

# 2. CRITICAL: Backup database before any migration
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql

# Verify backup was created and has content
ls -lh backup_*.sql | tail -1

# 3. Apply new migrations (if any)
# Check migrations/ directory for new files
ls -lt migrations/

# Apply each new migration in chronological order
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/20260117000001_fix_embedding_set_stats.sql

# Verify migration applied successfully
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\d+ notes"

# 4. Build release binary
cargo build --release

# 5. Restart service
sudo systemctl restart matric-api

# 6. Verify deployment
curl http://localhost:3000/health
systemctl status matric-api
journalctl -u matric-api -n 50
```

### Critical Rules

1. **ALWAYS backup before migrations** - No exceptions. Migrations can fail or have unintended effects.
2. **Run migrations BEFORE restarting** - New code expects new schema. Restarting before migrations causes errors like:
   - "column X does not exist"
   - "relation X does not exist"
   - "function X does not exist"
3. **Verify each step** - Check status after migrations and service restart.

### Migration Order

Apply migrations in chronological order (filename timestamp):

```bash
# Current migrations (as of 2026-01-17)
migrations/20260102000000_initial_schema.sql          # Initial tables
migrations/20260115000000_templates.sql               # Templates feature
migrations/20260116000000_collection_hierarchy.sql    # Collections feature
migrations/20260117000000_embedding_sets.sql          # Embedding sets feature
migrations/20260117000001_fix_embedding_set_stats.sql # Stats view fix
```

### Rollback Procedure

If deployment fails:

```bash
# 1. Stop service
sudo systemctl stop matric-api

# 2. Restore database from backup
PGPASSWORD=matric psql -U matric -h localhost -d matric < backup_20260117_120000.sql

# 3. Checkout previous working commit
git checkout <previous-commit-hash>

# 4. Rebuild
cargo build --release

# 5. Restart service
sudo systemctl start matric-api

# 6. Verify
curl http://localhost:3000/health
```

## Service Management

### Systemd Service

The API server runs as a systemd service.

**Service file location:** `/etc/systemd/system/matric-api.service`

**Configuration:**

```ini
[Unit]
Description=Matric Memory API Server
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=roctinam
Group=roctinam
WorkingDirectory=/home/roctinam/dev/matric-memory
Environment=DATABASE_URL=postgres://matric:matric@localhost:5432/matric
Environment=HOST=0.0.0.0
Environment=PORT=3000
Environment=RUST_LOG=matric_api=info,tower_http=info
ExecStart=/home/roctinam/dev/matric-memory/target/release/matric-api
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Common Service Commands

```bash
# Check service status
systemctl status matric-api

# Start service
sudo systemctl start matric-api

# Stop service
sudo systemctl stop matric-api

# Restart service (standard deployment)
sudo systemctl restart matric-api

# Enable service (start on boot)
sudo systemctl enable matric-api

# Disable service (do not start on boot)
sudo systemctl disable matric-api

# View real-time logs
journalctl -u matric-api -f

# View last 100 log lines
journalctl -u matric-api -n 100

# View logs from specific time
journalctl -u matric-api --since "2026-01-17 10:00:00"

# View logs with errors only
journalctl -u matric-api -p err
```

### Reload Service Configuration

If you modify the service file:

```bash
# Reload systemd configuration
sudo systemctl daemon-reload

# Restart service with new configuration
sudo systemctl restart matric-api
```

## Database Operations

### Connection Information

- **Host:** localhost
- **Port:** 5432 (default)
- **Database:** matric
- **User:** matric
- **Password:** matric
- **Connection String:** `postgres://matric:matric@localhost:5432/matric`

### Common Database Commands

```bash
# Connect to database
PGPASSWORD=matric psql -U matric -h localhost -d matric

# List all tables
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\dt"

# Check table schema
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\d+ notes"

# Check database size
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT pg_size_pretty(pg_database_size('matric'));"

# Check table sizes
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  relname AS table_name,
  pg_size_pretty(pg_total_relation_size(relid)) AS total_size
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;"
```

### Applying Migrations

```bash
# 1. ALWAYS backup first
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql

# 2. Check migration file for syntax
cat migrations/20260117000001_fix_embedding_set_stats.sql

# 3. Apply migration
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/20260117000001_fix_embedding_set_stats.sql

# 4. Verify migration applied
# Check for new tables/columns/views mentioned in migration
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\d+ embedding_sets"
```

### Embedding Set Statistics

Refresh materialized views for embedding set statistics:

```bash
# Refresh embedding set stats
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "REFRESH MATERIALIZED VIEW embedding_set_stats;"

# Verify stats updated
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT * FROM embedding_set_stats ORDER BY updated_at DESC LIMIT 5;"
```

### Database Maintenance

```bash
# Vacuum analyze all tables (recommended weekly)
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "VACUUM ANALYZE;"

# Check for bloat (unused space)
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  schemaname,
  tablename,
  pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size,
  n_tup_ins AS inserts,
  n_tup_upd AS updates,
  n_tup_del AS deletes
FROM pg_stat_user_tables
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;"

# Reindex (if query performance degrades)
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "REINDEX DATABASE matric;"
```

## MCP Server Operations

The MCP (Model Context Protocol) server provides Claude/AI integration.

### Starting MCP Server

```bash
cd /home/roctinam/dev/matric-memory/mcp-server

# Stdio mode (for local Claude Desktop)
node index.js

# HTTP mode (for remote access)
MCP_TRANSPORT=http node index.js

# Background mode with logging
nohup node index.js > mcp-server.log 2>&1 &
```

### MCP Server Configuration

**Environment variables:**

- `MCP_TRANSPORT` - Transport mode: `stdio` (default) or `http`
- `MCP_PORT` - HTTP port (default: 3001)
- `MATRIC_API_URL` - Matric API endpoint (default: http://localhost:3000)

### MCP Server Scripts

```bash
# Using npm scripts
npm start           # stdio mode
npm run start:http  # HTTP mode
```

### Stopping MCP Server

```bash
# Find process
ps aux | grep "node index.js"

# Kill process
kill <PID>

# Or use pkill
pkill -f "node index.js"
```

## Monitoring and Health Checks

### Health Endpoint

The API provides a health check endpoint:

```bash
# Basic health check
curl http://localhost:3000/health

# Expected response
# HTTP 200 OK
# {"status":"ok"}

# With details (if implemented)
curl http://localhost:3000/health?details=true
```

### API Endpoints

```bash
# Swagger/OpenAPI documentation
curl http://localhost:3000/swagger-ui/

# List all notes
curl http://localhost:3000/notes

# Search notes
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query": "test", "limit": 10}'

# Get note by ID
curl http://localhost:3000/notes/<note-id>
```

### Job Queue Monitoring

Check background job status:

```bash
# If jobs table exists, query job status
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  job_type,
  status,
  COUNT(*) as count,
  MAX(updated_at) as last_updated
FROM jobs
GROUP BY job_type, status
ORDER BY job_type, status;"

# Check for failed jobs
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT *
FROM jobs
WHERE status = 'failed'
ORDER BY updated_at DESC
LIMIT 10;"
```

### Database Connection Pool

Check active connections:

```bash
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  count(*),
  state,
  application_name
FROM pg_stat_activity
WHERE datname = 'matric'
GROUP BY state, application_name;"
```

### Log Monitoring

```bash
# Watch logs in real-time
journalctl -u matric-api -f

# Look for errors
journalctl -u matric-api -p err --since "1 hour ago"

# Count log levels
journalctl -u matric-api --since "today" | grep -oE 'ERROR|WARN|INFO|DEBUG' | sort | uniq -c
```

## Troubleshooting

### Service Won't Start

**Symptom:** `systemctl start matric-api` fails

**Diagnosis:**

```bash
# Check service status
systemctl status matric-api

# View recent logs
journalctl -u matric-api -n 50

# Check if port is in use
ss -tlnp | grep 3000

# Check binary exists and is executable
ls -l /home/roctinam/dev/matric-memory/target/release/matric-api
```

**Common causes:**

1. **Database connection failure**
   - Check PostgreSQL is running: `systemctl status postgresql`
   - Test connection: `PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT 1;"`
   - Verify credentials in service file

2. **Port already in use**
   - Find process: `lsof -i :3000`
   - Kill process: `kill <PID>`
   - Or change port in service file

3. **Binary missing or wrong version**
   - Rebuild: `cargo build --release`
   - Check binary: `ls -l target/release/matric-api`

### Database Errors After Deployment

**Symptom:** Errors like "column X does not exist" or "relation X does not exist"

**Cause:** Code updated but migrations not applied

**Solution:**

```bash
# 1. Stop service
sudo systemctl stop matric-api

# 2. Apply missing migrations
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/<new_migration>.sql

# 3. Restart service
sudo systemctl start matric-api

# 4. Verify
curl http://localhost:3000/health
```

### Migration Failure

**Symptom:** Migration SQL script fails

**Diagnosis:**

```bash
# Check detailed error
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/<migration>.sql

# Check database state
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\d+"
```

**Common causes:**

1. **Migration already applied**
   - Check if table/column already exists
   - Skip migration if already applied

2. **Syntax error in migration**
   - Review migration SQL file
   - Test SQL manually in psql

3. **Foreign key violation**
   - Check for orphaned data
   - Clean up data before migration

**Recovery:**

```bash
# 1. Restore from backup
PGPASSWORD=matric psql -U matric -h localhost -d matric < backup_<timestamp>.sql

# 2. Fix migration file
nano migrations/<migration>.sql

# 3. Reapply migration
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/<migration>.sql
```

### Slow Query Performance

**Symptom:** API endpoints respond slowly

**Diagnosis:**

```bash
# Check slow queries
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  query,
  calls,
  mean_exec_time,
  max_exec_time
FROM pg_stat_statements
WHERE query NOT LIKE '%pg_stat_statements%'
ORDER BY mean_exec_time DESC
LIMIT 10;"

# Check active queries
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  pid,
  state,
  query_start,
  state_change,
  query
FROM pg_stat_activity
WHERE state = 'active' AND query NOT LIKE '%pg_stat_activity%';"
```

**Solutions:**

1. **Missing indexes**
   - Review query plans: `EXPLAIN ANALYZE <query>`
   - Add indexes for frequently queried columns

2. **Table bloat**
   - Run vacuum: `VACUUM ANALYZE;`
   - Reindex if needed: `REINDEX TABLE notes;`

3. **Outdated statistics**
   - Analyze tables: `ANALYZE;`
   - Refresh materialized views

### Out of Disk Space

**Symptom:** Database writes fail, service crashes

**Diagnosis:**

```bash
# Check disk usage
df -h

# Check database size
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT pg_size_pretty(pg_database_size('matric'));"

# Check largest tables
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT
  relname,
  pg_size_pretty(pg_total_relation_size(relid))
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;"
```

**Solutions:**

1. **Clean up old backups**
   ```bash
   # List backups
   ls -lh backup_*.sql

   # Remove old backups (keep last 7 days)
   find . -name "backup_*.sql" -mtime +7 -delete
   ```

2. **Vacuum full (reclaim space)**
   ```bash
   # WARNING: Locks tables during operation
   PGPASSWORD=matric psql -U matric -h localhost -d matric -c "VACUUM FULL;"
   ```

3. **Archive old data**
   - Export old notes to files
   - Delete archived notes from database

## Backup and Recovery

### Backup Procedures

**Before every migration (mandatory):**

```bash
pg_dump -U matric -h localhost matric > backup_$(date +%Y%m%d_%H%M%S).sql
ls -lh backup_*.sql | tail -1
```

**Daily automated backup (recommended):**

```bash
# Create backup script
cat > /home/roctinam/bin/backup-matric.sh <<'EOF'
#!/bin/bash
BACKUP_DIR="/home/roctinam/backups/matric"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/matric_$DATE.sql"

mkdir -p "$BACKUP_DIR"
pg_dump -U matric -h localhost matric > "$BACKUP_FILE"

# Compress backup
gzip "$BACKUP_FILE"

# Keep only last 7 days of backups
find "$BACKUP_DIR" -name "matric_*.sql.gz" -mtime +7 -delete

echo "Backup completed: ${BACKUP_FILE}.gz"
EOF

chmod +x /home/roctinam/bin/backup-matric.sh

# Add to crontab (daily at 2 AM)
(crontab -l 2>/dev/null; echo "0 2 * * * /home/roctinam/bin/backup-matric.sh") | crontab -
```

**Manual backup:**

```bash
# Full database backup
pg_dump -U matric -h localhost matric > matric_backup.sql

# Compressed backup
pg_dump -U matric -h localhost matric | gzip > matric_backup.sql.gz

# Schema only (no data)
pg_dump -U matric -h localhost matric --schema-only > matric_schema.sql

# Data only (no schema)
pg_dump -U matric -h localhost matric --data-only > matric_data.sql

# Specific table
pg_dump -U matric -h localhost matric -t notes > notes_backup.sql
```

### Restore Procedures

**Full database restore:**

```bash
# 1. Stop service
sudo systemctl stop matric-api

# 2. Drop and recreate database
PGPASSWORD=matric psql -U matric -h localhost -c "DROP DATABASE IF EXISTS matric;"
PGPASSWORD=matric psql -U matric -h localhost -c "CREATE DATABASE matric;"

# 3. Restore from backup
PGPASSWORD=matric psql -U matric -h localhost -d matric < backup_20260117_120000.sql

# Or if compressed
gunzip -c matric_backup.sql.gz | PGPASSWORD=matric psql -U matric -h localhost -d matric

# 4. Restart service
sudo systemctl start matric-api

# 5. Verify
curl http://localhost:3000/health
```

**Partial restore (specific table):**

```bash
# 1. Drop table
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "DROP TABLE notes CASCADE;"

# 2. Restore table
PGPASSWORD=matric psql -U matric -h localhost -d matric < notes_backup.sql

# 3. Restart service
sudo systemctl restart matric-api
```

### Disaster Recovery

**Complete system failure:**

```bash
# 1. Reinstall dependencies
sudo apt update
sudo apt install postgresql-16 postgresql-16-pgvector

# 2. Create database and user
sudo -u postgres psql <<EOF
CREATE USER matric WITH PASSWORD 'matric';
CREATE DATABASE matric OWNER matric;
\c matric
CREATE EXTENSION vector;
GRANT ALL PRIVILEGES ON DATABASE matric TO matric;
EOF

# 3. Restore database
PGPASSWORD=matric psql -U matric -h localhost -d matric < latest_backup.sql

# 4. Rebuild application
cd /home/roctinam/dev/matric-memory
cargo build --release

# 5. Setup systemd service
sudo cp deploy/matric-api.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable matric-api
sudo systemctl start matric-api

# 6. Verify
systemctl status matric-api
curl http://localhost:3000/health
```

## Configuration

### Environment Variables

The service is configured via environment variables in the systemd service file.

**Current configuration:**

- `DATABASE_URL` - PostgreSQL connection string
- `HOST` - Bind address (0.0.0.0 = all interfaces)
- `PORT` - HTTP port (default: 3000)
- `RUST_LOG` - Logging level (matric_api=info,tower_http=info)

**To modify:**

```bash
# Edit service file
sudo nano /etc/systemd/system/matric-api.service

# Example: Change port to 8080
# Environment=PORT=8080

# Reload and restart
sudo systemctl daemon-reload
sudo systemctl restart matric-api
```

### Logging Configuration

**Log levels:**

- `error` - Errors only
- `warn` - Warnings and errors
- `info` - Informational messages (default)
- `debug` - Detailed debugging
- `trace` - Very verbose debugging

**Change log level:**

```bash
# Edit service file
sudo nano /etc/systemd/system/matric-api.service

# Modify RUST_LOG environment variable
# Environment=RUST_LOG=matric_api=debug,tower_http=debug

# Reload and restart
sudo systemctl daemon-reload
sudo systemctl restart matric-api
```

### Database Connection Tuning

**PostgreSQL connection pool settings (if implemented in code):**

- `POOL_MAX_CONNECTIONS` - Maximum connections (default: 10)
- `POOL_TIMEOUT` - Connection timeout in seconds (default: 30)

**PostgreSQL server tuning:**

```bash
# Edit PostgreSQL configuration
sudo nano /etc/postgresql/16/main/postgresql.conf

# Recommended settings for single-server deployment
max_connections = 100
shared_buffers = 256MB
effective_cache_size = 1GB
maintenance_work_mem = 64MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB
default_statistics_target = 100
random_page_cost = 1.1
effective_io_concurrency = 200

# Restart PostgreSQL
sudo systemctl restart postgresql
```

## Best Practices

1. **Always backup before migrations** - No exceptions
2. **Apply migrations before restarting** - Avoid schema mismatch errors
3. **Monitor logs regularly** - Catch issues early
4. **Run vacuum weekly** - Maintain database performance
5. **Keep backups for 7 days** - Balance between safety and disk space
6. **Test deployments** - Verify health endpoint and API functionality
7. **Document changes** - Update this guide when procedures change
8. **Monitor disk space** - Database and logs can grow quickly
9. **Use structured logging** - Makes troubleshooting easier
10. **Version your schema** - Track migration history

## Quick Reference

### Daily Operations

```bash
# Check service status
systemctl status matric-api

# View recent logs
journalctl -u matric-api -n 50

# Check health
curl http://localhost:3000/health

# Check database size
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT pg_size_pretty(pg_database_size('matric'));"
```

### Weekly Maintenance

```bash
# Vacuum database
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "VACUUM ANALYZE;"

# Refresh embedding set stats
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "REFRESH MATERIALIZED VIEW embedding_set_stats;"

# Clean up old backups
find /home/roctinam/backups/matric -name "matric_*.sql.gz" -mtime +7 -delete
```

### Emergency Procedures

```bash
# Stop everything
sudo systemctl stop matric-api

# Restore from latest backup
PGPASSWORD=matric psql -U matric -h localhost -d matric < latest_backup.sql

# Restart
sudo systemctl start matric-api

# Verify
curl http://localhost:3000/health
systemctl status matric-api
```

## Support and Resources

- **Repository:** https://git.integrolabs.net/roctinam/matric-memory
- **Architecture Documentation:** `/home/roctinam/dev/matric-memory/docs/architecture.md`
- **Integration Guide:** `/home/roctinam/dev/matric-memory/docs/integration.md`
- **Migrations:** `/home/roctinam/dev/matric-memory/migrations/`
- **Service File:** `/etc/systemd/system/matric-api.service`

## Version History

- **2026-01-17** - Initial operations guide created
