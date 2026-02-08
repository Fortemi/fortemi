# Fortémi Backup Guide

This guide covers backup and restore procedures for Fortémi.

## Overview

Fortémi provides multiple backup options:

| Method | Use Case | Format | Includes |
|--------|----------|--------|----------|
| **JSON Export** | App-level backup | JSON | Notes, collections, tags, templates |
| **Knowledge Shard** | Full portable backup | .shard | Notes, links, embeddings, sets, checksums |
| **Knowledge Archive** | Backup + metadata bundle | .archive | Backup file + metadata.json |
| **Database Snapshot** | Full database backup | pg_dump | Complete database with embeddings |
| **Shell script** | Automated scheduled backups | pg_dump | Full database with compression |

### Encryption Options

All backup methods support optional PKE encryption:

| Encryption | Format | Use Case |
|------------|--------|----------|
| **PKE** | .mmpke (MMPKE01) | Multi-recipient wallet-style encryption |

See [Encryption Guide](./encryption.md) for cryptographic details and [Shard Exchange Primer](./shard-exchange.md) for practical sharing workflows.

### Choosing a Backup Method

- **JSON Export** (`/api/v1/backup/export`): Quick export of note content. Best for migration to other systems.
- **Knowledge Shard** (`/api/v1/backup/knowledge-shard`): Complete app-level backup with semantic links. Best for full restore.
- **Knowledge Archive** (`/api/v1/backup/knowledge-archive`): Bundles any backup with its metadata sidecar. Best for transferring backups between systems.

## Shard Versioning

Knowledge shards use **semantic versioning** (MAJOR.MINOR.PATCH) to ensure compatibility across different Fortémi versions.

### Current Version

- **Shard format version**: `1.0.0`
- **Defined in**: `crates/matric-core/src/shard/version.rs`

### Version Compatibility

When importing a shard, Fortémi automatically checks version compatibility:

| Scenario | Behavior |
|----------|----------|
| Same version (1.0.0 → 1.0.0) | Import directly |
| Older minor (1.0.0 → 1.1.0) | Import directly, ignore unknown fields |
| Newer minor (1.1.0 → 1.0.0) | Import with warning, some features may be unavailable |
| Older major (1.x.x → 2.0.0) | Auto-migrate via registry |
| Newer major (2.0.0 → 1.0.0) | Fail with upgrade guidance |

### Migration Support

Breaking changes (major version bumps) include automatic migrations:

```
Shard: 1.0.0, Current: 2.0.0
↻ Automatic migration applied
✓ Data transformed to new format
⚠ Migration warnings logged
```

For detailed information about versioning, compatibility, and troubleshooting, see the [Shard Migration Guide](./shard-migration.md).

- **Database Snapshot** (`/api/v1/backup/database/snapshot`): Full pg_dump backup. Best for disaster recovery.

## Quick Start

### Using REST API

```bash
# Export all notes as JSON
curl http://localhost:3000/api/v1/backup/export

# Create knowledge shard (with links, embeddings)
curl http://localhost:3000/api/v1/backup/knowledge-shard -o backup.shard

# Create shard with specific components
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,links,embeddings" -o backup.shard

# Trigger database backup
curl -X POST http://localhost:3000/api/v1/backup/trigger

# Check backup status
curl http://localhost:3000/api/v1/backup/status
```

### Using MCP Tools (Recommended for AI Agents)

```javascript
// Export all notes to JSON
export_all_notes()

// Create knowledge shard with embeddings and links
knowledge_shard()

// Create shard with specific components
knowledge_shard({ include: "notes,links" })

// Restore from shard
knowledge_shard_import({ shard_base64: "...", dry_run: true })

// Download backup as portable knowledge archive (.archive)
knowledge_archive_download({ filename: "snapshot_database_20260117.sql.gz" })

// Upload and extract knowledge archive
knowledge_archive_upload({ archive_base64: "..." })

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

## Memory-Scoped Backups

All backup operations respect the `X-Fortemi-Memory` header. Without it, backups operate on the `default` memory.

### Backup Specific Memory

```bash
# Export work memory only
curl http://localhost:3000/api/v1/backup/export \
  -H "X-Fortemi-Memory: work-notes" \
  -o work-notes-backup.json

# Create knowledge shard for specific memory
curl http://localhost:3000/api/v1/backup/knowledge-shard \
  -H "X-Fortemi-Memory: work-notes" \
  -o work-notes.shard
```

### Full Database Backup (All Memories)

Database backup includes all memories and shared tables:

```bash
# Database backup includes all memories and shared tables
curl http://localhost:3000/api/v1/backup/database -o full-backup.sql
```

### Restore to Different Memory

```bash
# First, create the target memory
curl -X POST http://localhost:3000/api/v1/memories \
  -H "Content-Type: application/json" \
  -d '{
    "name": "work-notes-restored",
    "description": "Restored from backup"
  }'

# Encode shard and restore to target memory
SHARD=$(base64 -w0 work-notes.shard)
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -H "X-Fortemi-Memory: work-notes-restored" \
  -d "{\"shard_base64\": \"$SHARD\"}"
```

Memory-scoped backups are useful for:
- Creating per-project backups before major changes
- Migrating specific memories between instances
- Isolating backup/restore operations by project or team

See the [Multi-Memory Guide](./multi-memory.md) for comprehensive memory management documentation.

## MCP Backup Tools

### export_all_notes

Exports all notes as a complete JSON export.

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
  "output": "[2026-01-17 12:00:00] Starting Fortémi backup...\n...",
  "timestamp": "2026-01-17T12:00:00Z"
}
```

### backup_status

Check the status of the backup system.

**Returns:**
```json
{
  "backup_directory": "/var/backups/fortemi",
  "disk_usage": "1.2G",
  "latest_backup": {
    "path": "/var/backups/fortemi/matric_backup_20260117_120000.sql.gz",
    "size_bytes": 52428800,
    "timestamp": "2026-01-17T12:00:00Z"
  },
  "status": "healthy"
}
```

### knowledge_shard

Create a comprehensive knowledge shard with full data including semantic links and embeddings.

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
  "filename": "matric-shard-20260117-143800.tar.gz",
  "size_bytes": 52428,
  "size_human": "51.20 KB",
  "content_type": "application/gzip",
  "base64_data": "H4sIAAAAAAAA...",
  "message": "Archive created. Use base64_data to save the file."
}
```

**Shard contents:**
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

### knowledge_shard_import

Restore from a knowledge shard created by `knowledge_shard`.

**Parameters:**
- `shard_base64` - The shard as base64 string (from knowledge_shard.base64_data)
- `include` - Components to import (default: all in shard)
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
- Same shard twice → `on_conflict` determines behavior
- Different shard → New IDs create new notes (merge)

### knowledge_archive_download

Download a backup file bundled with its metadata as a portable `.archive` file.

**Parameters:**
- `filename` - Backup filename from list_backups (e.g., `snapshot_database_20260117.sql.gz`)

**Returns:**
```json
{
  "success": true,
  "filename": "snapshot_database_20260117.archive",
  "size_bytes": 1234567,
  "base64_data": "..."
}
```

**Archive contents:**
```
snapshot_database_20260117.archive (tar format)
├── snapshot_database_20260117.sql.gz  # The backup file
└── metadata.json                       # Title, description, note_count, etc.
```

**Use case:** Transfer backups between systems while preserving metadata context.

### knowledge_archive_upload

Upload a `.archive` file and extract both the backup and its metadata.

**Parameters:**
- `archive_base64` - Base64-encoded .archive file
- `filename` - Original filename (optional, for logging)

**Returns:**
```json
{
  "success": true,
  "filename": "upload_snapshot_database_20260117.sql.gz",
  "path": "/var/backups/fortemi/upload_snapshot_database_20260117.sql.gz",
  "size_bytes": 1234567,
  "size_human": "1.23 MB",
  "metadata": {
    "title": "Pre-migration backup",
    "description": "Full backup before schema changes",
    "backup_type": "snapshot",
    "note_count": 42
  }
}
```

**Workflow for transferring backups:**
1. Source system: `list_backups` → `knowledge_archive_download`
2. Transfer the .archive file
3. Target system: `knowledge_archive_upload` → `database_restore`

## REST API Endpoints

The backup system exposes REST API endpoints that can be called directly or via MCP tools.

### GET /api/v1/backup/export

Export all notes as a complete JSON export.

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
  "output": "[2026-01-17 12:00:00] Starting Fortémi backup...\n...",
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
  "backup_directory": "/var/backups/fortemi",
  "disk_usage": "1.2G",
  "backup_count": 7,
  "latest_backup": {
    "path": "/var/backups/fortemi/matric_backup_20260117_120000.sql.gz",
    "filename": "matric_backup_20260117_120000.sql.gz",
    "size_bytes": 52428800,
    "modified": "2026-01-17T12:00:00Z"
  },
  "status": "healthy"
}
```

**Status Values:**
- `healthy` - Backups exist and are recent
- `no_backups` - No backup files found (directory auto-created if needed)
- `cannot_create_directory: <error>` - Failed to create backup directory (permission error)

**Note:** The backup directory is automatically created if it doesn't exist. If creation fails due to permissions, the status will indicate the error.

### GET /api/v1/backup/knowledge-shard

Create a comprehensive knowledge shard with selected components.

**Query Parameters:**
- `include` - Comma-separated components: `notes,collections,tags,templates,links,embedding_sets,embeddings`
- Default includes everything except embeddings (which can be large)

**Example:**
```bash
# Knowledge shard with all data
curl http://localhost:3000/api/v1/backup/knowledge-shard -o backup.shard

# Shard with specific components
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,links" -o backup.shard

# Include embeddings (large)
curl "http://localhost:3000/api/v1/backup/knowledge-shard?include=notes,links,embeddings" -o full-backup.shard
```

**Response:** Binary .shard file (gzipped tar) with `Content-Disposition: attachment` header.

**Verify shard:**
```bash
# List contents (it's a gzipped tar internally)
tar -tzf backup.shard

# View manifest
tar -xzf backup.shard -O manifest.json | jq .
```

### POST /api/v1/backup/knowledge-shard/import

Restore from a knowledge shard.

**Request Body:**
```json
{
  "shard_base64": "H4sIAAAAAAAA...",
  "include": "notes,collections",
  "dry_run": false,
  "on_conflict": "skip",
  "skip_embedding_regen": false
}
```

**Example:**
```bash
# Encode shard and import
ARCHIVE=$(base64 -w0 backup.shard)
curl -X POST http://localhost:3000/api/v1/backup/knowledge-shard/import \
  -H "Content-Type: application/json" \
  -d "{\"shard_base64\": \"$ARCHIVE\", \"dry_run\": true}"
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

Create `/etc/Fortémi/backup.conf`:

```bash
# Destinations
BACKUP_DEST=/var/backups/fortemi
BACKUP_REMOTE_RSYNC=backup@nas.local:/backups/matric
BACKUP_REMOTE_S3=s3://my-bucket/matric-backups

# Retention (days)
BACKUP_RETAIN=7

# Compression: gzip, zstd, xz, none
BACKUP_COMPRESS=gzip

# Encryption (optional - path to age public key)
BACKUP_ENCRYPT=/etc/Fortémi/backup-key.pub

# Database
PGUSER=matric
PGPASSWORD=matric
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

# Logging
LOG_FILE=/var/log/Fortémi/backup.log
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

1. Parse the JSON export
2. Use `create_note` or `bulk_create_notes` MCP tools to recreate notes
3. Use `create_collection` to recreate collections
4. Use `create_template` to recreate templates

### From pg_dump Backup

```bash
# 1. Stop the API service
sudo systemctl stop matric-api

# 2. List available backups
ls -lh /var/backups/fortemi/

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
age-keygen -o /etc/Fortémi/backup-key.txt

# Extract public key
age-keygen -y /etc/Fortémi/backup-key.txt > /etc/Fortémi/backup-key.pub

# Secure private key
sudo chmod 600 /etc/Fortémi/backup-key.txt
sudo chown root:root /etc/Fortémi/backup-key.txt
```

### Configure Encryption

Add to backup.conf:

```bash
BACKUP_ENCRYPT=/etc/Fortémi/backup-key.pub
```

### Decrypt Backup

```bash
age --decrypt -i /etc/Fortémi/backup-key.txt backup.sql.gz.age > backup.sql.gz
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
echo "BACKUP_REMOTE_S3=s3://your-bucket/matric-backups" >> /etc/Fortémi/backup.conf
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
echo "BACKUP_REMOTE_RSYNC=backup@nas.local:/backups/matric" >> /etc/Fortémi/backup.conf
```

## Pre-Migration Backup

Always create a backup before running database migrations:

```bash
# Create backup with special retention
BACKUP_DEST=/var/backups/fortemi/migrations \
BACKUP_RETAIN=30 \
./scripts/backup.sh

# Verify backup was created
ls -lh /var/backups/fortemi/migrations/

# Now run migration
PGPASSWORD=matric psql -U matric -h localhost -d matric -f migrations/new_migration.sql
```

## Monitoring

### Check Backup Health

```bash
# Via MCP
backup_status()

# Via command line
ls -lh /var/backups/fortemi/ | tail -5
```

### View Logs

```bash
# Script logs
tail -f /var/log/Fortémi/backup.log

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
sudo chown -R fortemi:fortemi /var/backups/fortemi
sudo chmod 755 /var/backups/fortemi
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
echo "BACKUP_COMPRESS=zstd" >> /etc/Fortémi/backup.conf
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
| `/path/to/fortemi/scripts/backup.sh` | Backup script |
| `/etc/Fortémi/backup.conf` | Configuration file |
| `/var/backups/fortemi/` | Default backup destination |
| `/var/log/Fortémi/backup.log` | Backup logs |
| `/etc/systemd/system/matric-backup.service` | Systemd service |
| `/etc/systemd/system/matric-backup.timer` | Systemd timer |
