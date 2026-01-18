# Matric Memory Backup Automation Design

## Overview

Automated backup system for matric-memory PostgreSQL database with support for multiple destinations, retention policies, compression, encryption, and systemd integration.

## 1. Backup Script Design

### 1.1 Script Architecture

**File:** `scripts/backup.sh`

```
backup.sh
├── Configuration Loading
├── Validation & Preflight Checks
├── Backup Execution
│   ├── Database Dump (pg_dump)
│   ├── Compression (optional)
│   └── Encryption (optional)
├── Multi-Destination Distribution
│   ├── Local Storage
│   ├── Remote Rsync
│   └── S3 Upload
├── Retention Policy Enforcement
└── Logging & Notifications
```

### 1.2 Pseudocode

```bash
#!/bin/bash
# scripts/backup.sh - Matric Memory Database Backup

main() {
    # 1. Initialize
    load_configuration()
    validate_environment()
    setup_logging()

    # 2. Generate backup metadata
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    BACKUP_FILE="matric_backup_${TIMESTAMP}.sql"

    # 3. Create database backup
    log "Starting backup: ${BACKUP_FILE}"
    if ! create_database_dump "${BACKUP_FILE}"; then
        error_exit "Database dump failed"
    fi

    # 4. Post-process backup
    if [[ "${BACKUP_COMPRESS}" != "none" ]]; then
        compress_backup "${BACKUP_FILE}"
        BACKUP_FILE="${BACKUP_FILE}.${BACKUP_COMPRESS}"
    fi

    if [[ -n "${BACKUP_ENCRYPT}" ]]; then
        encrypt_backup "${BACKUP_FILE}"
        BACKUP_FILE="${BACKUP_FILE}.enc"
    fi

    # 5. Distribute to destinations
    distribute_backup "${BACKUP_FILE}"

    # 6. Enforce retention policy
    cleanup_old_backups

    # 7. Verify and report
    verify_backup "${BACKUP_FILE}"
    log "Backup completed successfully: ${BACKUP_FILE}"
}

create_database_dump() {
    local output_file="$1"
    local temp_path="${BACKUP_TEMP_DIR}/${output_file}"

    # Use pg_dump with custom format for better compression
    PGPASSWORD="${PGPASSWORD:-matric}" \
        pg_dump \
        -U "${PGUSER:-matric}" \
        -h "${PGHOST:-localhost}" \
        -p "${PGPORT:-5432}" \
        -d "${PGDATABASE:-matric}" \
        --format=custom \
        --compress=0 \
        --file="${temp_path}" \
        --verbose \
        2>> "${LOG_FILE}"

    return $?
}

compress_backup() {
    local file="$1"

    case "${BACKUP_COMPRESS}" in
        gzip)
            gzip -9 "${BACKUP_TEMP_DIR}/${file}"
            ;;
        zstd)
            zstd -19 --rm "${BACKUP_TEMP_DIR}/${file}"
            ;;
        xz)
            xz -9 -T0 "${BACKUP_TEMP_DIR}/${file}"
            ;;
        *)
            log "Unknown compression: ${BACKUP_COMPRESS}"
            return 1
            ;;
    esac
}

encrypt_backup() {
    local file="$1"

    if [[ ! -f "${BACKUP_ENCRYPT}" ]]; then
        error_exit "Encryption key not found: ${BACKUP_ENCRYPT}"
    fi

    # Use age for modern encryption
    age --encrypt --recipients-file "${BACKUP_ENCRYPT}" \
        -o "${BACKUP_TEMP_DIR}/${file}.enc" \
        "${BACKUP_TEMP_DIR}/${file}"

    # Remove unencrypted version
    shred -u "${BACKUP_TEMP_DIR}/${file}"
}

distribute_backup() {
    local file="$1"
    local success=0

    # Local destination (primary)
    if copy_to_local "${file}"; then
        ((success++))
    fi

    # Remote rsync destination
    if [[ -n "${BACKUP_REMOTE_RSYNC}" ]]; then
        if sync_to_remote "${file}"; then
            ((success++))
        fi
    fi

    # S3 destination
    if [[ -n "${BACKUP_REMOTE_S3}" ]]; then
        if upload_to_s3 "${file}"; then
            ((success++))
        fi
    fi

    if [[ ${success} -eq 0 ]]; then
        error_exit "All backup destinations failed"
    fi

    # Cleanup temp file
    rm -f "${BACKUP_TEMP_DIR}/${file}"
}

copy_to_local() {
    local file="$1"
    local dest="${BACKUP_DEST}/${file}"

    mkdir -p "${BACKUP_DEST}"
    cp "${BACKUP_TEMP_DIR}/${file}" "${dest}"
    chmod 600 "${dest}"

    return $?
}

sync_to_remote() {
    local file="$1"

    rsync -avz \
        --timeout=300 \
        "${BACKUP_TEMP_DIR}/${file}" \
        "${BACKUP_REMOTE_RSYNC}/" \
        2>> "${LOG_FILE}"

    return $?
}

upload_to_s3() {
    local file="$1"

    # Requires AWS CLI or s3cmd
    if command -v aws &> /dev/null; then
        aws s3 cp \
            "${BACKUP_TEMP_DIR}/${file}" \
            "${BACKUP_REMOTE_S3}/${file}" \
            --storage-class STANDARD_IA \
            2>> "${LOG_FILE}"
    else
        s3cmd put \
            "${BACKUP_TEMP_DIR}/${file}" \
            "${BACKUP_REMOTE_S3}/${file}" \
            2>> "${LOG_FILE}"
    fi

    return $?
}

cleanup_old_backups() {
    local retain="${BACKUP_RETAIN:-7}"

    # Cleanup local backups
    if [[ -d "${BACKUP_DEST}" ]]; then
        find "${BACKUP_DEST}" \
            -name "matric_backup_*.sql*" \
            -type f \
            -mtime "+${retain}" \
            -delete \
            2>> "${LOG_FILE}"
    fi

    # Cleanup remote rsync backups
    if [[ -n "${BACKUP_REMOTE_RSYNC}" ]]; then
        ssh "${BACKUP_REMOTE_RSYNC%%:*}" \
            "find ${BACKUP_REMOTE_RSYNC##*:} -name 'matric_backup_*.sql*' -mtime +${retain} -delete" \
            2>> "${LOG_FILE}"
    fi

    # Cleanup S3 (using lifecycle policy preferred)
    if [[ -n "${BACKUP_REMOTE_S3}" ]] && command -v aws &> /dev/null; then
        aws s3 ls "${BACKUP_REMOTE_S3}/" | \
            awk '{print $4}' | \
            sort -r | \
            tail -n +$((retain + 1)) | \
            xargs -I {} aws s3 rm "${BACKUP_REMOTE_S3}/{}" \
            2>> "${LOG_FILE}"
    fi
}

verify_backup() {
    local file="$1"
    local backup_path="${BACKUP_DEST}/${file}"

    # Basic verification: file exists and non-empty
    if [[ ! -f "${backup_path}" ]]; then
        log "WARNING: Backup file not found at ${backup_path}"
        return 1
    fi

    local size=$(stat -f%z "${backup_path}" 2>/dev/null || stat -c%s "${backup_path}" 2>/dev/null)
    if [[ ${size} -lt 1024 ]]; then
        log "WARNING: Backup file suspiciously small: ${size} bytes"
        return 1
    fi

    log "Backup verified: ${size} bytes"
    return 0
}

load_configuration() {
    # Load from config file if exists
    if [[ -f /etc/matric-memory/backup.conf ]]; then
        source /etc/matric-memory/backup.conf
    fi

    # Environment variables override config file
    BACKUP_DEST="${BACKUP_DEST:-/var/backups/matric-memory}"
    BACKUP_TEMP_DIR="${BACKUP_TEMP_DIR:-/tmp/matric-backup}"
    BACKUP_RETAIN="${BACKUP_RETAIN:-7}"
    BACKUP_COMPRESS="${BACKUP_COMPRESS:-gzip}"
    BACKUP_ENCRYPT="${BACKUP_ENCRYPT:-}"
    BACKUP_REMOTE_RSYNC="${BACKUP_REMOTE_RSYNC:-}"
    BACKUP_REMOTE_S3="${BACKUP_REMOTE_S3:-}"

    # Database connection
    PGUSER="${PGUSER:-matric}"
    PGPASSWORD="${PGPASSWORD:-matric}"
    PGHOST="${PGHOST:-localhost}"
    PGPORT="${PGPORT:-5432}"
    PGDATABASE="${PGDATABASE:-matric}"

    # Logging
    LOG_FILE="${LOG_FILE:-/var/log/matric-memory/backup.log}"
    LOG_LEVEL="${LOG_LEVEL:-INFO}"
}

validate_environment() {
    # Check required commands
    local required_cmds="pg_dump"
    for cmd in ${required_cmds}; do
        if ! command -v ${cmd} &> /dev/null; then
            error_exit "Required command not found: ${cmd}"
        fi
    done

    # Check compression tools
    if [[ "${BACKUP_COMPRESS}" != "none" ]]; then
        if ! command -v ${BACKUP_COMPRESS} &> /dev/null; then
            error_exit "Compression tool not found: ${BACKUP_COMPRESS}"
        fi
    fi

    # Check encryption tool
    if [[ -n "${BACKUP_ENCRYPT}" ]]; then
        if ! command -v age &> /dev/null; then
            error_exit "Encryption tool (age) not found"
        fi
    fi

    # Create directories
    mkdir -p "${BACKUP_DEST}"
    mkdir -p "${BACKUP_TEMP_DIR}"
    mkdir -p "$(dirname ${LOG_FILE})"
}

setup_logging() {
    exec 1> >(tee -a "${LOG_FILE}")
    exec 2>&1
}

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

error_exit() {
    log "ERROR: $*"
    exit 1
}

# Execute main function
main "$@"
```

### 1.3 Features

1. **Multiple Destinations**
   - Local filesystem (primary)
   - Remote rsync over SSH
   - S3-compatible object storage

2. **Compression Options**
   - gzip (default, fast)
   - zstd (best compression/speed ratio)
   - xz (maximum compression)
   - none (for custom format already compressed)

3. **Encryption**
   - Modern age encryption
   - Public key or passphrase
   - Automatic cleanup of unencrypted temp files

4. **Retention Policy**
   - Configurable days to retain
   - Automatic cleanup across all destinations
   - S3 lifecycle integration

5. **Error Handling**
   - Validation before execution
   - Partial success handling
   - Detailed logging
   - Non-zero exit on failure

6. **Verification**
   - File existence check
   - Size validation
   - Optional pg_restore dry-run

## 2. Systemd Units

### 2.1 Service Unit

**File:** `deploy/matric-backup.service`

```ini
[Unit]
Description=Matric Memory Database Backup
After=network.target postgresql.service
Requires=postgresql.service
Documentation=https://github.com/roctinam/matric-memory

[Service]
Type=oneshot
User=roctinam
Group=roctinam

# Environment configuration
EnvironmentFile=-/etc/matric-memory/backup.conf
Environment=PGUSER=matric
Environment=PGPASSWORD=matric
Environment=PGHOST=localhost
Environment=PGPORT=5432
Environment=PGDATABASE=matric

# Working directory
WorkingDirectory=/home/roctinam/dev/matric-memory

# Execute backup script
ExecStart=/home/roctinam/dev/matric-memory/scripts/backup.sh

# Resource limits
CPUQuota=50%
MemoryMax=2G
IOWeight=100

# Security hardening
PrivateTmp=true
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/backups/matric-memory /var/log/matric-memory /tmp

# Timeout
TimeoutStartSec=1h

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=matric-backup

[Install]
WantedBy=multi-user.target
```

### 2.2 Timer Unit

**File:** `deploy/matric-backup.timer`

```ini
[Unit]
Description=Matric Memory Database Backup Timer
Documentation=https://github.com/roctinam/matric-memory
Requires=matric-backup.service

[Timer]
# Run daily at 2:30 AM
OnCalendar=daily
OnCalendar=*-*-* 02:30:00

# Randomize start time by up to 15 minutes
RandomizedDelaySec=15min

# If system was off, run missed backup on next boot
Persistent=true

# Run 5 minutes after boot if never run
OnBootSec=5min

# Accuracy (allow up to 1 hour slack)
AccuracySec=1h

[Install]
WantedBy=timers.target
```

### 2.3 Timer Scheduling Options

Common scheduling patterns:

```ini
# Daily at specific time
OnCalendar=*-*-* 02:30:00

# Every 6 hours
OnCalendar=00/6:00:00

# Weekdays at 3 AM
OnCalendar=Mon-Fri *-*-* 03:00:00

# Weekly on Sunday 1 AM
OnCalendar=Sun *-*-* 01:00:00

# Monthly on 1st at midnight
OnCalendar=*-*-01 00:00:00

# Multiple times per day
OnCalendar=*-*-* 02:00:00
OnCalendar=*-*-* 14:00:00
```

## 3. Configuration Schema

### 3.1 Configuration File

**File:** `/etc/matric-memory/backup.conf`

```bash
# Matric Memory Backup Configuration

# ============================================================================
# BACKUP DESTINATIONS
# ============================================================================

# Primary local destination (required)
BACKUP_DEST=/var/backups/matric-memory

# Temporary directory for backup processing
BACKUP_TEMP_DIR=/tmp/matric-backup

# Remote rsync destination (optional)
# Format: user@host:/path/to/backups
# Requires SSH key authentication
BACKUP_REMOTE_RSYNC=

# S3 destination (optional)
# Format: s3://bucket-name/prefix
# Requires AWS CLI configured
BACKUP_REMOTE_S3=

# ============================================================================
# RETENTION POLICY
# ============================================================================

# Number of days to retain backups (default: 7)
BACKUP_RETAIN=7

# ============================================================================
# COMPRESSION & ENCRYPTION
# ============================================================================

# Compression type: gzip, zstd, xz, none (default: gzip)
BACKUP_COMPRESS=gzip

# Encryption key path (optional)
# Use 'age' format public key or recipients file
# Example: /etc/matric-memory/backup-key.pub
BACKUP_ENCRYPT=

# ============================================================================
# DATABASE CONNECTION
# ============================================================================

# PostgreSQL connection parameters
PGUSER=matric
PGPASSWORD=matric
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

# ============================================================================
# LOGGING
# ============================================================================

# Log file location
LOG_FILE=/var/log/matric-memory/backup.log

# Log level: DEBUG, INFO, WARN, ERROR
LOG_LEVEL=INFO

# ============================================================================
# NOTIFICATIONS (optional)
# ============================================================================

# Email notification on failure
NOTIFY_EMAIL=

# Webhook URL for notifications (e.g., Slack, Discord)
NOTIFY_WEBHOOK=

# ============================================================================
# ADVANCED OPTIONS
# ============================================================================

# Custom pg_dump options
# Default: --format=custom --compress=0
PGDUMP_OPTIONS=

# Lock file to prevent concurrent backups
LOCK_FILE=/var/run/matric-backup.lock

# Enable verification with pg_restore --list
VERIFY_RESTORE=false

# Parallel compression threads (zstd only)
COMPRESS_THREADS=4
```

### 3.2 Environment Variables

All configuration options can be overridden via environment variables:

```bash
# Quick backup to alternate location
BACKUP_DEST=/mnt/external/matric-backups ./scripts/backup.sh

# One-time encrypted backup
BACKUP_ENCRYPT=/path/to/key.pub ./scripts/backup.sh

# No compression for fast backup
BACKUP_COMPRESS=none ./scripts/backup.sh
```

### 3.3 Configuration Precedence

1. Environment variables (highest priority)
2. `/etc/matric-memory/backup.conf`
3. Script defaults (lowest priority)

## 4. Installation & Setup

### 4.1 Initial Setup

```bash
# 1. Create required directories
sudo mkdir -p /etc/matric-memory
sudo mkdir -p /var/backups/matric-memory
sudo mkdir -p /var/log/matric-memory

# 2. Set ownership
sudo chown -R roctinam:roctinam /var/backups/matric-memory
sudo chown -R roctinam:roctinam /var/log/matric-memory

# 3. Create configuration file
sudo cp examples/backup.conf /etc/matric-memory/backup.conf
sudo chown roctinam:roctinam /etc/matric-memory/backup.conf
sudo chmod 600 /etc/matric-memory/backup.conf

# 4. Make backup script executable
chmod +x scripts/backup.sh

# 5. Install systemd units
sudo cp deploy/matric-backup.service /etc/systemd/system/
sudo cp deploy/matric-backup.timer /etc/systemd/system/

# 6. Reload systemd
sudo systemctl daemon-reload

# 7. Enable timer
sudo systemctl enable matric-backup.timer

# 8. Start timer
sudo systemctl start matric-backup.timer
```

### 4.2 Optional: Encryption Setup

```bash
# 1. Install age
# Ubuntu/Debian
sudo apt install age

# macOS
brew install age

# 2. Generate encryption key
age-keygen -o /etc/matric-memory/backup-key.txt

# 3. Extract public key
age-keygen -y /etc/matric-memory/backup-key.txt > /etc/matric-memory/backup-key.pub

# 4. Secure the private key
sudo chmod 600 /etc/matric-memory/backup-key.txt
sudo chown root:root /etc/matric-memory/backup-key.txt

# 5. Update configuration
echo "BACKUP_ENCRYPT=/etc/matric-memory/backup-key.pub" | sudo tee -a /etc/matric-memory/backup.conf
```

### 4.3 Optional: S3 Setup

```bash
# 1. Install AWS CLI
sudo apt install awscli

# 2. Configure credentials
aws configure
# Enter: Access Key, Secret Key, Region, Output format

# 3. Create S3 bucket
aws s3 mb s3://matric-memory-backups

# 4. Set lifecycle policy (optional)
cat > lifecycle.json <<EOF
{
  "Rules": [
    {
      "Id": "Archive old backups",
      "Status": "Enabled",
      "Transitions": [
        {
          "Days": 30,
          "StorageClass": "GLACIER"
        }
      ],
      "Expiration": {
        "Days": 365
      }
    }
  ]
}
EOF

aws s3api put-bucket-lifecycle-configuration \
  --bucket matric-memory-backups \
  --lifecycle-configuration file://lifecycle.json

# 5. Update configuration
echo "BACKUP_REMOTE_S3=s3://matric-memory-backups/postgres" | sudo tee -a /etc/matric-memory/backup.conf
```

### 4.4 Optional: Rsync Setup

```bash
# 1. Generate SSH key if needed
ssh-keygen -t ed25519 -f ~/.ssh/backup_key -N ""

# 2. Copy key to remote server
ssh-copy-id -i ~/.ssh/backup_key user@backup-server

# 3. Test connection
ssh -i ~/.ssh/backup_key user@backup-server "mkdir -p /backups/matric-memory"

# 4. Add to SSH config
cat >> ~/.ssh/config <<EOF
Host backup-server
  User backup-user
  HostName backup.example.com
  IdentityFile ~/.ssh/backup_key
  Compression yes
EOF

# 5. Update configuration
echo "BACKUP_REMOTE_RSYNC=backup-server:/backups/matric-memory" | sudo tee -a /etc/matric-memory/backup.conf
```

## 5. Example Configurations

### 5.1 Minimal Local Backup

**Use case:** Development environment, simple local backups

```bash
# /etc/matric-memory/backup.conf
BACKUP_DEST=/var/backups/matric-memory
BACKUP_RETAIN=7
BACKUP_COMPRESS=gzip

PGUSER=matric
PGPASSWORD=matric
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

LOG_FILE=/var/log/matric-memory/backup.log
```

**Schedule:** Daily at 2:30 AM

### 5.2 Production with Encryption & Remote

**Use case:** Production environment with off-site backups

```bash
# /etc/matric-memory/backup.conf
BACKUP_DEST=/var/backups/matric-memory
BACKUP_TEMP_DIR=/tmp/matric-backup
BACKUP_RETAIN=14
BACKUP_COMPRESS=zstd
BACKUP_ENCRYPT=/etc/matric-memory/backup-key.pub

# Remote destinations
BACKUP_REMOTE_RSYNC=backup@backup-1.example.com:/backups/matric
BACKUP_REMOTE_S3=s3://prod-backups/matric-memory

# Database
PGUSER=matric
PGPASSWORD=$(cat /etc/matric-memory/pgpass)
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

# Logging
LOG_FILE=/var/log/matric-memory/backup.log
LOG_LEVEL=INFO

# Notifications
NOTIFY_EMAIL=ops@example.com
NOTIFY_WEBHOOK=https://hooks.slack.com/services/XXX/YYY/ZZZ

# Advanced
VERIFY_RESTORE=true
COMPRESS_THREADS=8
```

**Schedule:** Every 6 hours + before migrations

### 5.3 High-Frequency with Multiple Retention

**Use case:** Critical production with multiple backup tiers

```bash
# /etc/matric-memory/backup.conf
BACKUP_DEST=/var/backups/matric-memory
BACKUP_RETAIN=3  # Keep 3 days locally

# Remote with longer retention
BACKUP_REMOTE_S3=s3://backups/matric/daily
BACKUP_COMPRESS=zstd
BACKUP_ENCRYPT=/etc/matric-memory/backup-key.pub

# Database
PGUSER=matric
PGPASSWORD=$(cat /etc/matric-memory/pgpass)
PGHOST=localhost
PGPORT=5432
PGDATABASE=matric

LOG_FILE=/var/log/matric-memory/backup.log
LOG_LEVEL=INFO
VERIFY_RESTORE=true
```

**Timer configuration:**

```ini
# deploy/matric-backup.timer
[Timer]
# Run every 4 hours
OnCalendar=00/4:00:00
RandomizedDelaySec=10min
Persistent=true
AccuracySec=30min
```

**S3 lifecycle:**
- Daily backups: 90 days
- Transition to Glacier: 30 days
- Delete after: 365 days

### 5.4 Pre-Migration Backup

**Use case:** Manual backup before running migrations

```bash
#!/bin/bash
# deploy/pre-migration-backup.sh

# Temporary configuration for migration backups
export BACKUP_DEST=/var/backups/matric-memory/migrations
export BACKUP_COMPRESS=none  # Fast, no compression needed
export BACKUP_RETAIN=30  # Keep migration backups longer

# Create backup
/home/roctinam/dev/matric-memory/scripts/backup.sh

# Verify it worked
if [ $? -eq 0 ]; then
    echo "✓ Pre-migration backup completed"
    echo "  Location: ${BACKUP_DEST}"
    echo "  Latest: $(ls -t ${BACKUP_DEST}/matric_backup_*.sql | head -1)"
else
    echo "✗ Pre-migration backup FAILED"
    echo "  DO NOT PROCEED with migration!"
    exit 1
fi
```

**Integration with migration workflow:**

```bash
#!/bin/bash
# deploy/run-migration.sh

set -e

MIGRATION_FILE="$1"

if [ -z "$MIGRATION_FILE" ]; then
    echo "Usage: $0 <migration.sql>"
    exit 1
fi

# 1. Create backup
echo "Creating pre-migration backup..."
./deploy/pre-migration-backup.sh

# 2. Apply migration
echo "Applying migration: $MIGRATION_FILE"
PGPASSWORD=matric psql -U matric -h localhost -d matric -f "$MIGRATION_FILE"

# 3. Verify
echo "Verifying database..."
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\dt"

echo "✓ Migration completed successfully"
```

## 6. Operations & Maintenance

### 6.1 Manual Backup

```bash
# Run backup immediately
sudo systemctl start matric-backup.service

# Check status
sudo systemctl status matric-backup.service

# View logs
journalctl -u matric-backup.service -f
```

### 6.2 Verify Timer

```bash
# Check timer status
systemctl status matric-backup.timer

# List all timers
systemctl list-timers

# See next scheduled run
systemctl list-timers matric-backup.timer

# View timer logs
journalctl -u matric-backup.timer
```

### 6.3 Monitor Backups

```bash
# List recent backups
ls -lh /var/backups/matric-memory/ | tail -10

# Check backup size trends
du -sh /var/backups/matric-memory/matric_backup_*.sql* | tail -20

# Verify latest backup
LATEST=$(ls -t /var/backups/matric-memory/matric_backup_*.sql | head -1)
pg_restore --list "$LATEST" | head -20

# Check logs
tail -f /var/log/matric-memory/backup.log
```

### 6.4 Restore Procedure

```bash
# 1. Stop the API service
sudo systemctl stop matric-api

# 2. List available backups
ls -lh /var/backups/matric-memory/

# 3. Choose backup to restore
BACKUP_FILE=/var/backups/matric-memory/matric_backup_20260117_023000.sql

# 4. Drop and recreate database (DANGEROUS!)
PGPASSWORD=matric psql -U matric -h localhost -c "DROP DATABASE matric;"
PGPASSWORD=matric psql -U matric -h localhost -c "CREATE DATABASE matric;"

# 5. Restore backup
PGPASSWORD=matric pg_restore \
  -U matric \
  -h localhost \
  -d matric \
  --verbose \
  "$BACKUP_FILE"

# 6. Verify restore
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "\dt"
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT COUNT(*) FROM notes;"

# 7. Restart API service
sudo systemctl start matric-api

# 8. Verify application
curl http://localhost:3000/health
```

### 6.5 Troubleshooting

**Problem:** Backup fails with "permission denied"

```bash
# Check directory permissions
ls -ld /var/backups/matric-memory
ls -ld /var/log/matric-memory

# Fix permissions
sudo chown -R roctinam:roctinam /var/backups/matric-memory
sudo chown -R roctinam:roctinam /var/log/matric-memory
sudo chmod 755 /var/backups/matric-memory
```

**Problem:** Timer not running

```bash
# Check timer status
systemctl status matric-backup.timer

# Enable timer
sudo systemctl enable matric-backup.timer
sudo systemctl start matric-backup.timer

# Verify
systemctl list-timers matric-backup.timer
```

**Problem:** Backup files too large

```bash
# Switch to zstd compression
echo "BACKUP_COMPRESS=zstd" | sudo tee -a /etc/matric-memory/backup.conf

# Or use pg_dump custom format with compression
echo "PGDUMP_OPTIONS=--format=custom --compress=9" | sudo tee -a /etc/matric-memory/backup.conf
```

**Problem:** S3 upload fails

```bash
# Check AWS credentials
aws sts get-caller-identity

# Test S3 access
aws s3 ls s3://matric-memory-backups/

# Check logs
journalctl -u matric-backup.service | grep -i s3
```

## 7. Monitoring & Alerts

### 7.1 Backup Monitoring Script

**File:** `scripts/check-backup.sh`

```bash
#!/bin/bash
# Check if backups are current

MAX_AGE_HOURS=26  # Alert if no backup in 26 hours (daily + margin)
BACKUP_DIR=/var/backups/matric-memory

LATEST=$(find "$BACKUP_DIR" -name "matric_backup_*.sql*" -type f -printf '%T@ %p\n' | sort -n | tail -1 | cut -d' ' -f2-)

if [ -z "$LATEST" ]; then
    echo "CRITICAL: No backups found in $BACKUP_DIR"
    exit 2
fi

AGE_SECONDS=$(( $(date +%s) - $(stat -c %Y "$LATEST") ))
AGE_HOURS=$(( AGE_SECONDS / 3600 ))

if [ $AGE_HOURS -gt $MAX_AGE_HOURS ]; then
    echo "WARNING: Latest backup is $AGE_HOURS hours old: $LATEST"
    exit 1
else
    SIZE=$(du -h "$LATEST" | cut -f1)
    echo "OK: Latest backup is $AGE_HOURS hours old ($SIZE): $LATEST"
    exit 0
fi
```

### 7.2 Integration with Monitoring Systems

**Prometheus Node Exporter:**

```bash
# Add to cron for textfile collector
*/5 * * * * /home/roctinam/dev/matric-memory/scripts/check-backup.sh > /var/lib/node_exporter/matric_backup.prom
```

**Nagios/Icinga:**

```ini
define command{
    command_name    check_matric_backup
    command_line    /home/roctinam/dev/matric-memory/scripts/check-backup.sh
}

define service{
    use                     generic-service
    host_name               matric-server
    service_description     Matric Backup Age
    check_command           check_matric_backup
    check_interval          60
}
```

## 8. Security Considerations

### 8.1 Secrets Management

1. **Database Password**
   - Use `.pgpass` file instead of environment variables
   - File format: `hostname:port:database:username:password`
   - Permissions: `chmod 600 ~/.pgpass`

2. **Encryption Keys**
   - Store private keys with root-only access
   - Use public key encryption (age)
   - Rotate keys annually

3. **SSH Keys**
   - Use dedicated backup SSH keys
   - Restrict with `command=` in authorized_keys
   - Use SSH agent forwarding carefully

4. **S3 Credentials**
   - Use IAM roles when possible
   - Limit bucket permissions to backup prefix
   - Enable S3 bucket versioning

### 8.2 File Permissions

```bash
# Configuration
chmod 600 /etc/matric-memory/backup.conf
chown root:root /etc/matric-memory/backup.conf

# Encryption keys
chmod 600 /etc/matric-memory/backup-key.*
chown root:root /etc/matric-memory/backup-key.*

# Backup directory
chmod 700 /var/backups/matric-memory
chown roctinam:roctinam /var/backups/matric-memory

# Log files
chmod 640 /var/log/matric-memory/backup.log
chown roctinam:roctinam /var/log/matric-memory/backup.log
```

### 8.3 Systemd Security

The service unit includes security hardening:
- `PrivateTmp=true` - Isolated /tmp
- `NoNewPrivileges=true` - Prevent privilege escalation
- `ProtectSystem=strict` - Read-only system directories
- `ProtectHome=read-only` - Protect other users' data
- `ReadWritePaths=` - Explicitly allow backup locations

## 9. Performance Optimization

### 9.1 Compression Benchmarks

Based on typical 10GB PostgreSQL database:

| Method | Time  | Size  | CPU  | Notes                    |
|--------|-------|-------|------|--------------------------|
| none   | 2m    | 10GB  | Low  | Fast, large files        |
| gzip   | 5m    | 2GB   | Med  | Good default             |
| zstd   | 3m    | 1.8GB | Med  | Best ratio/speed         |
| xz     | 15m   | 1.5GB | High | Maximum compression      |

**Recommendation:** Use `zstd` for production backups.

### 9.2 Parallel Processing

For large databases, use parallel pg_dump:

```bash
# In backup.conf
PGDUMP_OPTIONS="--format=directory --jobs=4"
```

### 9.3 Network Optimization

For rsync transfers:

```bash
# Add to rsync command
--compress-level=9 \
--bwlimit=10000 \  # Limit to 10MB/s
--partial \
--progress
```

## 10. Testing & Validation

### 10.1 Backup Test Script

**File:** `scripts/test-backup-restore.sh`

```bash
#!/bin/bash
# Test backup and restore cycle

set -e

echo "=== Matric Backup Restore Test ==="

# 1. Create test backup
echo "Creating backup..."
./scripts/backup.sh

# 2. Find latest backup
LATEST=$(ls -t /var/backups/matric-memory/matric_backup_*.sql* | head -1)
echo "Latest backup: $LATEST"

# 3. Create test database
echo "Creating test database..."
PGPASSWORD=matric psql -U matric -h localhost -c "DROP DATABASE IF EXISTS matric_test;"
PGPASSWORD=matric psql -U matric -h localhost -c "CREATE DATABASE matric_test;"

# 4. Restore to test database
echo "Restoring backup..."
PGPASSWORD=matric pg_restore \
  -U matric \
  -h localhost \
  -d matric_test \
  --verbose \
  "$LATEST"

# 5. Compare row counts
echo "Verifying data..."
PROD_COUNT=$(PGPASSWORD=matric psql -U matric -h localhost -d matric -t -c "SELECT COUNT(*) FROM notes;")
TEST_COUNT=$(PGPASSWORD=matric psql -U matric -h localhost -d matric_test -t -c "SELECT COUNT(*) FROM notes;")

if [ "$PROD_COUNT" -eq "$TEST_COUNT" ]; then
    echo "✓ Backup restore successful: $PROD_COUNT notes"
else
    echo "✗ Backup restore failed: prod=$PROD_COUNT test=$TEST_COUNT"
    exit 1
fi

# 6. Cleanup
PGPASSWORD=matric psql -U matric -h localhost -c "DROP DATABASE matric_test;"

echo "=== Test completed successfully ==="
```

### 10.2 Quarterly DR Drill

1. Schedule quarterly disaster recovery drills
2. Document restore time (RTO)
3. Verify data integrity
4. Update runbooks based on findings

## 11. Integration with SDLC

### 11.1 CI/CD Integration

Add to GitHub Actions workflow:

```yaml
# .github/workflows/deploy.yml
jobs:
  deploy:
    steps:
      - name: Create pre-deployment backup
        run: |
          ssh $DEPLOY_SERVER "sudo systemctl start matric-backup.service"
          ssh $DEPLOY_SERVER "sudo systemctl status matric-backup.service"

      - name: Run migrations
        run: |
          ssh $DEPLOY_SERVER "cd /home/roctinam/dev/matric-memory && ./deploy/run-migration.sh migrations/new_migration.sql"

      - name: Deploy application
        run: |
          # Deploy steps...
```

### 11.2 Pre-Commit Hook

```bash
# .git/hooks/pre-commit
#!/bin/bash

# Check if migrations changed
if git diff --cached --name-only | grep -q "migrations/"; then
    echo "⚠ Migration detected!"
    echo "Remember to:"
    echo "  1. Create backup before deployment"
    echo "  2. Run: sudo systemctl start matric-backup.service"
    echo "  3. Verify: ls -lh /var/backups/matric-memory/"
fi
```

## 12. Cost Analysis

### 12.1 Storage Costs

**Local Storage (1TB SSD):**
- Cost: $100 one-time
- Retention: 7 days
- Daily backup: ~10GB compressed
- Total: ~70GB

**S3 Standard-IA:**
- Storage: $0.0125/GB/month
- Daily backup: 10GB
- 90 days retention: 900GB
- Monthly cost: ~$11.25

**S3 Glacier:**
- Storage: $0.004/GB/month
- Archive: 365 days
- Monthly cost: ~$14.60

**Total estimated monthly cost:** ~$26

### 12.2 Network Costs

**Rsync (1Gbps network):**
- Transfer time: ~2 minutes
- Cost: $0 (LAN)

**S3 Upload:**
- Data transfer out: $0.09/GB
- Daily 10GB: $0.90/day
- Monthly: ~$27

**Total estimated transfer cost:** ~$27/month

## Summary

This backup automation design provides:

1. Robust shell script with multiple destinations
2. Systemd integration for automated scheduling
3. Flexible configuration system
4. Comprehensive security hardening
5. Multiple example configurations
6. Detailed operational procedures
7. Monitoring and alerting integration
8. Disaster recovery testing framework

**Total estimated implementation time:** 4-6 hours

**Recommended deployment order:**
1. Implement backup script (1-2 hours)
2. Test manually with local storage (30 minutes)
3. Create systemd units (30 minutes)
4. Configure encryption (30 minutes)
5. Setup remote destinations (1 hour)
6. Implement monitoring (1 hour)
7. Document and test DR procedures (1 hour)

**Next steps:**
1. Review and approve design
2. Implement scripts/backup.sh
3. Create systemd units
4. Test in development environment
5. Deploy to production
6. Schedule first DR drill
