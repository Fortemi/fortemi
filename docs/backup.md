# Matric Memory Backup Guide

This guide covers backup and restore procedures for Matric Memory.

## Overview

Matric Memory provides multiple backup options:

| Method | Use Case | Format | Includes |
|--------|----------|--------|----------|
| **JSON Export** | App-level backup | JSON | Notes, collections, tags, templates |
| **Archive Export** | Full portable backup | tar.gz | Notes, links, embeddings, sets, checksums |
| **Shell script** | Automated scheduled backups | pg_dump | Full database with compression |
| **Manual pg_dump** | Quick one-off backups | SQL/custom | Full database |

### Choosing a Backup Method

- **JSON Export** (`/api/v1/backup/export`): Quick export of note content. Best for migration to other systems.
- **Archive Export** (`/api/v1/backup/archive`): Complete backup with semantic links and embeddings. Best for full restore.
- **pg_dump** (`scripts/backup.sh`): Database-level backup. Best for disaster recovery.

## Quick Start

### Using REST API

```bash
# Export all notes as JSON
curl http://localhost:3000/api/v1/backup/export

# Create full archive (tar.gz with links, embeddings)
curl http://localhost:3000/api/v1/backup/archive -o backup.tar.gz

# Create archive with specific components
curl "http://localhost:3000/api/v1/backup/archive?include=notes,links,embeddings" -o backup.tar.gz

# Trigger database backup
curl -X POST http://localhost:3000/api/v1/backup/trigger

# Check backup status
curl http://localhost:3000/api/v1/backup/status
```

### Using MCP Tools (Recommended for AI Agents)

```javascript
// Export all notes to JSON
export_all_notes()

// Create full archive with embeddings and links
backup_archive()

// Create archive with specific components
backup_archive({ include: "notes,links" })

// Restore from archive
archive_import({ archive_base64: "...", dry_run: true })

// Trigger database backup
backup_now()

// Check backup status
backup_status()
```

### Using Shell Script

```bash
# Run immediate backup
./scripts/backup.sh

# Run with verbose output
./scripts/backup.sh -v

# Dry run (show what would happen)
./scripts/backup.sh -n

# Backup to specific destination
./scripts/backup.sh -d local
```

### Using Systemd (Scheduled)

```bash
# Enable daily backups at 2:30 AM
sudo systemctl enable matric-backup.timer
sudo systemctl start matric-backup.timer

# Check timer status
systemctl list-timers matric-backup.timer
```

## MCP Backup Tools

### export_all_notes

Exports all notes as a complete JSON archive.

**Parameters:**
- `filter.starred_only` - Only export starred notes
- `filter.tags` - Only notes with these tags
- `filter.created_after` / `created_before` - Date range

**Returns:**
```json
{
  "manifest": {
    "version": "1.0.0",
    "format": "matric-backup",
    "created_at": "2026-01-17T12:00:00Z",
    "counts": {
      "notes": 150,
      "collections": 5,
      "tags": 30,
      "templates": 3
    }
  },
  "notes": [...],
  "collections": [...],
  "tags": [...],
  "templates": [...]
}
```

**Use cases:**
- Complete knowledge base backup
- Migration to another instance
- Offline analysis
- Snapshots before major changes

### backup_now

Triggers an immediate database backup using the backup script.

**Parameters:**
- `destinations` - Array of: `["local", "s3", "rsync"]`
- `dry_run` - Preview without executing

**Returns:**
```json
{
  "status": "success",
  "output": "[2026-01-17 12:00:00] Starting Matric Memory backup...\n...",
  "timestamp": "2026-01-17T12:00:00Z"
}
```

### backup_status

Check the status of the backup system.

**Returns:**
```json
{
  "backup_directory": "/var/backups/matric-memory",
  "disk_usage": "1.2G",
  "latest_backup": {
    "path": "/var/backups/matric-memory/matric_backup_20260117_120000.sql.gz",
    "size_bytes": 52428800,
    "timestamp": "2026-01-17T12:00:00Z"
  },
  "status": "healthy"
}
```

### backup_archive

Create a comprehensive tar.gz archive with full data including semantic links and embeddings.

**Parameters:**
- `include` - Components to include (comma-separated or array):
  - `notes` - All notes with original/revised content
  - `collections` - Folder hierarchy
  - `tags` - All tags
  - `templates` - Note templates
  - `links` - Semantic relationships between notes
  - `embedding_sets` - Embedding set definitions and members
  - `embeddings` - Vector embeddings (large, optional)
  - Default: `notes,collections,tags,templates,links,embedding_sets`

**Returns:**
```json
{
  "success": true,
  "filename": "matric-archive-20260117-143800.tar.gz",
  "size_bytes": 52428,
  "size_human": "51.20 KB",
  "content_type": "application/gzip",
  "base64_data": "H4sIAAAAAAAA...",
  "message": "Archive created. Use base64_data to save the file."
}
```

**Archive contents:**
```
manifest.json           # Version, counts, SHA256 checksums
notes.jsonl             # Notes in streaming JSONL format
collections.json        # Folder hierarchy
tags.json               # Tags with timestamps
templates.json          # Note templates
links.jsonl             # Semantic links between notes
embedding_sets.json     # Set definitions
embedding_set_members.jsonl
embedding_configs.json  # Model configurations
embeddings.jsonl        # Vector data (if included)
```

### archive_import

Restore from a tar.gz archive created by `backup_archive`.

**Parameters:**
- `archive_base64` - The archive as base64 string (from backup_archive.base64_data)
- `include` - Components to import (default: all in archive)
- `dry_run` - Validate without importing (default: false)
- `on_conflict` - For existing notes: `skip`, `replace`, `merge` (default: skip)
- `skip_embedding_regen` - Don't regenerate embeddings (default: false)

**Returns:**
```json
{
  "status": "success",
  "manifest": { "version": "1.0.0", "counts": {...} },
  "imported": { "notes": 4, "collections": 2, "links": 12 },
  "skipped": { "notes": 0 },
  "errors": [],
  "dry_run": false
}
```

**Conflict behavior:**
- Same archive twice → `on_conflict` determines behavior
- Different archive → New IDs create new notes (merge)

## REST API Endpoints

The backup system exposes REST API endpoints that can be called directly or via MCP tools.

### GET /api/v1/backup/export

Export all notes as a complete JSON archive.

**Query Parameters:**
- `starred_only` - Only export starred notes (boolean)
- `tags` - Comma-separated list of tags to filter by
- `created_after` - Only notes created after this date (ISO 8601)
- `created_before` - Only notes created before this date (ISO 8601)

**Example:**
```bash
# Export all notes
curl http://localhost:3000/api/v1/backup/export

# Export starred notes created in 2024
curl "http://localhost:3000/api/v1/backup/export?starred_only=true&created_after=2024-01-01T00:00:00Z"
```

**Response:**
```json
{
  "manifest": {
    "version": "1.0.0",
    "format": "matric-backup",
    "created_at": "2026-01-17T12:00:00Z",
    "counts": { "notes": 150, "collections": 5, "tags": 30, "templates": 3 }
  },
  "notes": [...],
  "collections": [...],
  "tags": [...],
  "templates": [...]
}
```

### POST /api/v1/backup/trigger

Trigger an immediate database backup using the backup script.

**Request Body (optional):**
```json
{
  "destinations": ["local", "s3", "rsync"],
  "dry_run": false
}
```

**Example:**
```bash
# Trigger backup to all destinations
curl -X POST http://localhost:3000/api/v1/backup/trigger

# Dry run (preview only)
curl -X POST http://localhost:3000/api/v1/backup/trigger \
  -H "Content-Type: application/json" \
  -d '{"dry_run": true}'
```

**Response:**
```json
{
  "status": "success",
  "output": "[2026-01-17 12:00:00] Starting Matric Memory backup...\n...",
  "timestamp": "2026-01-17T12:00:00Z"
}
```

### GET /api/v1/backup/status

Get the current status of the backup system.

**Example:**
```bash
curl http://localhost:3000/api/v1/backup/status
```

**Response:**
```json
{
  "backup_directory": "/var/backups/matric-memory",
  "disk_usage": "1.2G",
  "backup_count": 7,
  "latest_backup": {
    "path": "/var/backups/matric-memory/matric_backup_20260117_120000.sql.gz",
    "filename": "matric_backup_20260117_120000.sql.gz",
    "size_bytes": 52428800,
    "modified": "2026-01-17T12:00:00Z"
  },
  "status": "healthy"
}
```

**Status Values:**
- `healthy` - Backups exist and are recent
- `no_backups` - No backup files found
- `no_backup_directory` - Backup directory doesn't exist

### GET /api/v1/backup/archive

Create a comprehensive tar.gz archive with selected components.

**Query Parameters:**
- `include` - Comma-separated components: `notes,collections,tags,templates,links,embedding_sets,embeddings`
- Default includes everything except embeddings (which can be large)

**Example:**
```bash
# Full archive with all data
curl http://localhost:3000/api/v1/backup/archive -o backup.tar.gz

# Archive with specific components
curl "http://localhost:3000/api/v1/backup/archive?include=notes,links" -o backup.tar.gz

# Include embeddings (large)
curl "http://localhost:3000/api/v1/backup/archive?include=notes,links,embeddings" -o full-backup.tar.gz
```

**Response:** Binary tar.gz file with `Content-Disposition: attachment` header.

**Verify archive:**
```bash
# List contents
tar -tzf backup.tar.gz

# View manifest
tar -xzf backup.tar.gz -O manifest.json | jq .
```

### POST /api/v1/backup/archive/import

Restore from a tar.gz archive.

**Request Body:**
```json
{
  "archive_base64": "H4sIAAAAAAAA...",
  "include": "notes,collections",
  "dry_run": false,
  "on_conflict": "skip",
  "skip_embedding_regen": false
}
```

**Example:**
```bash
# Encode archive and import
ARCHIVE=$(base64 -w0 backup.tar.gz)
curl -X POST http://localhost:3000/api/v1/backup/archive/import \
  -H "Content-Type: application/json" \
  -d "{\"archive_base64\": \"$ARCHIVE\", \"dry_run\": true}"
```

**Response:**
```json
{
  "status": "success",
  "manifest": { "version": "1.0.0", "counts": {...} },
  "imported": { "notes": 4, "collections": 2 },
  "skipped": { "notes": 0 },
  "errors": [],
  "dry_run": false
}
```

## Backup Script

The backup script (`scripts/backup.sh`) provides full database backups with:

- **Compression**: gzip (default), zstd, xz, or none
- **Encryption**: Optional age encryption
- **Multiple destinations**: Local, rsync, S3
- **Retention policy**: Automatic cleanup of old backups
- **Verification**: File integrity checks

### Configuration

Create `/etc/matric-memory/backup.conf`:

```bash
# Destinations
BACKUP_DEST=/var/backups/matric-memory
BACKUP_REMOTE_RSYNC=backup@nas.local:/backups/matric
BACKUP_REMOTE_S3=s3://my-bucket/matric-backups

# Retention (days)
BACKUP_RETAIN=7

# Compression: gzip, zstd, xz, none
BACKUP_COMPRESS=gzip

# Encryption (optional - path to age public key)
BACKUP_ENCRYPT=/etc/matric-memory/backup-key.pub

# Database
PGUSER=matric
PGPASSWORD=matric
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

# Logging
LOG_FILE=/var/log/matric-memory/backup.log
```

Or use environment variables:

```bash
BACKUP_DEST=/custom/path ./scripts/backup.sh
```

### Command Line Options

```
Usage: backup.sh [options]

Options:
  -c, --config FILE      Configuration file
  -d, --destination STR  Specific destination: local, s3, rsync, or all
  -n, --dry-run          Show what would be done without executing
  -v, --verbose          Enable verbose output
  -q, --quiet            Quiet mode (errors only)
  -h, --help             Show help
```

## Systemd Integration

### Service Unit

The service unit (`deploy/matric-backup.service`) runs the backup script with:

- Resource limits (50% CPU, 2GB RAM)
- Security hardening (PrivateTmp, ProtectSystem)
- 1-hour timeout
- Journal logging

### Timer Unit

The timer unit (`deploy/matric-backup.timer`) schedules backups:

- Daily at 2:30 AM
- 15-minute randomized delay
- Persistent (catches up after downtime)
- 5-minute delay on first boot

### Installation

```bash
# Copy unit files
sudo cp deploy/matric-backup.service /etc/systemd/system/
sudo cp deploy/matric-backup.timer /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable and start timer
sudo systemctl enable matric-backup.timer
sudo systemctl start matric-backup.timer

# Verify
systemctl list-timers matric-backup.timer
```

### Manual Trigger

```bash
# Run backup immediately
sudo systemctl start matric-backup.service

# Check status
sudo systemctl status matric-backup.service

# View logs
journalctl -u matric-backup.service -f
```

## Restore Procedures

### From JSON Export (export_all_notes)

1. Parse the JSON archive
2. Use `create_note` or `bulk_create_notes` MCP tools to recreate notes
3. Use `create_collection` to recreate collections
4. Use `create_template` to recreate templates

### From pg_dump Backup

```bash
# 1. Stop the API service
sudo systemctl stop matric-api

# 2. List available backups
ls -lh /var/backups/matric-memory/

# 3. Drop and recreate database
PGPASSWORD=matric psql -U matric -h localhost -c "DROP DATABASE matric;"
PGPASSWORD=matric psql -U matric -h localhost -c "CREATE DATABASE matric;"

# 4. Restore from backup
# For .sql files:
PGPASSWORD=matric psql -U matric -h localhost -d matric -f backup.sql

# For .sql.gz files:
gunzip -c matric_backup_YYYYMMDD.sql.gz | PGPASSWORD=matric psql -U matric -h localhost -d matric

# For pg_dump custom format (.sql without compression):
PGPASSWORD=matric pg_restore -U matric -h localhost -d matric backup.sql

# 5. Verify
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT COUNT(*) FROM note;"

# 6. Restart API
sudo systemctl start matric-api

# 7. Verify health
curl http://localhost:3000/health
```

## Encryption Setup

### Generate Encryption Key

```bash
# Install age
sudo apt install age

# Generate key pair
age-keygen -o /etc/matric-memory/backup-key.txt

# Extract public key
age-keygen -y /etc/matric-memory/backup-key.txt > /etc/matric-memory/backup-key.pub

# Secure private key
sudo chmod 600 /etc/matric-memory/backup-key.txt
sudo chown root:root /etc/matric-memory/backup-key.txt
```

### Configure Encryption

Add to backup.conf:

```bash
BACKUP_ENCRYPT=/etc/matric-memory/backup-key.pub
```

### Decrypt Backup

```bash
age --decrypt -i /etc/matric-memory/backup-key.txt backup.sql.gz.age > backup.sql.gz
```

## Remote Destinations

### S3 Setup

```bash
# Install AWS CLI
sudo apt install awscli

# Configure credentials
aws configure

# Test access
aws s3 ls s3://your-bucket/

# Configure backup
echo "BACKUP_REMOTE_S3=s3://your-bucket/matric-backups" >> /etc/matric-memory/backup.conf
```

### Rsync Setup

```bash
# Generate SSH key
ssh-keygen -t ed25519 -f ~/.ssh/backup_key -N ""

# Copy to remote server
ssh-copy-id -i ~/.ssh/backup_key backup@nas.local

# Test connection
ssh -i ~/.ssh/backup_key backup@nas.local "mkdir -p /backups/matric"

# Configure backup
echo "BACKUP_REMOTE_RSYNC=backup@nas.local:/backups/matric" >> /etc/matric-memory/backup.conf
```

## Pre-Migration Backup

Always create a backup before running database migrations:

```bash
# Create backup with special retention
BACKUP_DEST=/var/backups/matric-memory/migrations \
BACKUP_RETAIN=30 \
./scripts/backup.sh

# Verify backup was created
ls -lh /var/backups/matric-memory/migrations/

# Now run migration
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/new_migration.sql
```

## Monitoring

### Check Backup Health

```bash
# Via MCP
backup_status()

# Via command line
ls -lh /var/backups/matric-memory/ | tail -5
```

### View Logs

```bash
# Script logs
tail -f /var/log/matric-memory/backup.log

# Systemd logs
journalctl -u matric-backup.service -f
```

### Alerting

Configure webhook notifications in backup.conf:

```bash
NOTIFY_WEBHOOK=https://hooks.slack.com/services/XXX/YYY/ZZZ
```

## Troubleshooting

### Backup fails with "permission denied"

```bash
# Fix directory permissions
sudo chown -R roctinam:roctinam /var/backups/matric-memory
sudo chmod 755 /var/backups/matric-memory
```

### Timer not running

```bash
# Check timer status
systemctl status matric-backup.timer

# Enable and start
sudo systemctl enable matric-backup.timer
sudo systemctl start matric-backup.timer
```

### S3 upload fails

```bash
# Check AWS credentials
aws sts get-caller-identity

# Test S3 access
aws s3 ls s3://your-bucket/
```

### Backup files too large

```bash
# Switch to zstd compression (better ratio)
echo "BACKUP_COMPRESS=zstd" >> /etc/matric-memory/backup.conf
```

## Best Practices

1. **Test restores regularly** - Backups are only useful if they work
2. **Use multiple destinations** - Local + remote for redundancy
3. **Enable encryption** for sensitive data
4. **Monitor disk space** - Set up alerts for backup directory
5. **Keep at least 3 backups** - Never reduce below minimum retention
6. **Backup before migrations** - Always create a restore point
7. **Document restore procedures** - Keep runbooks up to date

## File Locations

| Path | Description |
|------|-------------|
| `/home/roctinam/dev/matric-memory/scripts/backup.sh` | Backup script |
| `/etc/matric-memory/backup.conf` | Configuration file |
| `/var/backups/matric-memory/` | Default backup destination |
| `/var/log/matric-memory/backup.log` | Backup logs |
| `/etc/systemd/system/matric-backup.service` | Systemd service |
| `/etc/systemd/system/matric-backup.timer` | Systemd timer |
