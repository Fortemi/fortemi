#!/usr/bin/env bash
#
# fortemi-backup.sh - Automated backup script for Fortémi
#
# Usage: backup.sh [options]
#   -c, --config FILE    Configuration file (default: /etc/fortemi/backup.conf)
#   -d, --destination    Specific destination (local, s3, rsync)
#   -n, --dry-run        Show what would be done
#   -v, --verbose        Verbose output
#   -q, --quiet          Quiet mode (errors only)
#   -h, --help           Show this help
#
# Environment variables:
#   BACKUP_DEST          Local backup directory (default: /var/backups/fortemi)
#   BACKUP_RETAIN        Days to retain backups (default: 7)
#   BACKUP_COMPRESS      Compression: gzip, zstd, none (default: gzip)
#   BACKUP_REMOTE_RSYNC  Rsync destination (user@host:/path)
#   BACKUP_REMOTE_S3     S3 bucket path (s3://bucket/prefix)
#   PGUSER, PGPASSWORD, PGHOST, PGPORT, PGDATABASE
#

set -euo pipefail

# Script metadata
readonly SCRIPT_NAME="fortemi-backup"
readonly SCRIPT_VERSION="1.0.0"

# Default configuration
CONFIG_FILE="${FORTEMI_BACKUP_CONFIG:-/etc/fortemi/backup.conf}"
BACKUP_DEST="${BACKUP_DEST:-/var/backups/fortemi}"
BACKUP_TEMP_DIR="${BACKUP_TEMP_DIR:-/tmp/fortemi-backup}"
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
LOG_FILE="${LOG_FILE:-/var/log/fortemi/backup.log}"
VERBOSE=false
QUIET=false
DRY_RUN=false
SPECIFIC_DEST=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log() {
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    if [[ "$QUIET" != "true" ]]; then
        echo -e "${BLUE}[$timestamp]${NC} $*"
    fi
    if [[ -n "$LOG_FILE" ]] && [[ -w "$(dirname "$LOG_FILE")" ]]; then
        echo "[$timestamp] $*" >> "$LOG_FILE"
    fi
}

log_verbose() {
    if [[ "$VERBOSE" == "true" ]]; then
        log "$*"
    fi
}

log_success() {
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${GREEN}[$timestamp] SUCCESS:${NC} $*"
    if [[ -n "$LOG_FILE" ]] && [[ -w "$(dirname "$LOG_FILE")" ]]; then
        echo "[$timestamp] SUCCESS: $*" >> "$LOG_FILE"
    fi
}

log_warn() {
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${YELLOW}[$timestamp] WARNING:${NC} $*" >&2
    if [[ -n "$LOG_FILE" ]] && [[ -w "$(dirname "$LOG_FILE")" ]]; then
        echo "[$timestamp] WARNING: $*" >> "$LOG_FILE"
    fi
}

log_error() {
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${RED}[$timestamp] ERROR:${NC} $*" >&2
    if [[ -n "$LOG_FILE" ]] && [[ -w "$(dirname "$LOG_FILE")" ]]; then
        echo "[$timestamp] ERROR: $*" >> "$LOG_FILE"
    fi
}

error_exit() {
    log_error "$*"
    exit 1
}

# Show help
show_help() {
    cat <<EOF
$SCRIPT_NAME v$SCRIPT_VERSION - Fortémi Database Backup

Usage: $(basename "$0") [options]

Options:
  -c, --config FILE      Configuration file (default: /etc/fortemi/backup.conf)
  -d, --destination STR  Specific destination: local, s3, rsync, or all (default: all enabled)
  -n, --dry-run          Show what would be done without executing
  -v, --verbose          Enable verbose output
  -q, --quiet            Quiet mode (errors only)
  -h, --help             Show this help

Environment Variables:
  BACKUP_DEST            Local backup directory (default: /var/backups/fortemi)
  BACKUP_RETAIN          Days to retain backups (default: 7)
  BACKUP_COMPRESS        Compression: gzip, zstd, none (default: gzip)
  BACKUP_ENCRYPT         Path to age public key for encryption
  BACKUP_REMOTE_RSYNC    Rsync destination (user@host:/path)
  BACKUP_REMOTE_S3       S3 bucket path (s3://bucket/prefix)
  PGUSER                 PostgreSQL user (default: matric)
  PGPASSWORD             PostgreSQL password (default: matric)
  PGHOST                 PostgreSQL host (default: localhost)
  PGPORT                 PostgreSQL port (default: 5432)
  PGDATABASE             PostgreSQL database (default: matric)
  LOG_FILE               Log file path (default: /var/log/fortemi/backup.log)

Examples:
  $(basename "$0")                          # Full backup with all configured destinations
  $(basename "$0") -d local                 # Backup to local only
  $(basename "$0") -d s3 -v                 # Backup to S3 with verbose output
  $(basename "$0") -n                       # Dry run - show what would happen
  BACKUP_RETAIN=14 $(basename "$0")         # Override retention to 14 days

Configuration File:
  The script looks for /etc/fortemi/backup.conf by default.
  This file can contain any of the environment variables above.
  Example: BACKUP_DEST=/mnt/backups/fortemi

EOF
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -c|--config)
                CONFIG_FILE="$2"
                shift 2
                ;;
            -d|--destination)
                SPECIFIC_DEST="$2"
                shift 2
                ;;
            -n|--dry-run)
                DRY_RUN=true
                shift
                ;;
            -v|--verbose)
                VERBOSE=true
                shift
                ;;
            -q|--quiet)
                QUIET=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                error_exit "Unknown option: $1. Use -h for help."
                ;;
        esac
    done
}

# Load configuration from file
load_config() {
    if [[ -f "$CONFIG_FILE" ]]; then
        log_verbose "Loading config from $CONFIG_FILE"
        # shellcheck source=/dev/null
        source "$CONFIG_FILE"
    else
        log_verbose "Config file not found: $CONFIG_FILE (using defaults/environment)"
    fi
}

# Validate environment and dependencies
validate_environment() {
    log_verbose "Validating environment..."

    # Check required commands
    local required_cmds=("pg_dump")
    for cmd in "${required_cmds[@]}"; do
        if ! command -v "$cmd" &> /dev/null; then
            error_exit "Required command not found: $cmd"
        fi
    done

    # Check compression tools
    if [[ "$BACKUP_COMPRESS" != "none" ]]; then
        if ! command -v "$BACKUP_COMPRESS" &> /dev/null; then
            error_exit "Compression tool not found: $BACKUP_COMPRESS"
        fi
    fi

    # Check encryption tool if configured
    if [[ -n "$BACKUP_ENCRYPT" ]]; then
        if ! command -v age &> /dev/null; then
            error_exit "Encryption tool (age) not found"
        fi
        if [[ ! -f "$BACKUP_ENCRYPT" ]]; then
            error_exit "Encryption key file not found: $BACKUP_ENCRYPT"
        fi
    fi

    # Check S3 tools if S3 destination configured
    if [[ -n "$BACKUP_REMOTE_S3" ]] && [[ "$SPECIFIC_DEST" == "s3" || "$SPECIFIC_DEST" == "all" || -z "$SPECIFIC_DEST" ]]; then
        if ! command -v aws &> /dev/null; then
            log_warn "AWS CLI not found - S3 backup will be skipped"
        fi
    fi

    # Create directories
    if [[ "$DRY_RUN" != "true" ]]; then
        mkdir -p "$BACKUP_DEST" 2>/dev/null || true
        mkdir -p "$BACKUP_TEMP_DIR" 2>/dev/null || true
        if [[ -n "$LOG_FILE" ]]; then
            mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true
        fi
    fi

    log_verbose "Environment validated"
}

# Create database dump
create_database_dump() {
    local output_file="$1"
    local temp_path="${BACKUP_TEMP_DIR}/${output_file}"

    log "Creating database dump: $output_file"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would create pg_dump to $temp_path"
        # Create empty file for dry run
        touch "$temp_path"
        return 0
    fi

    # Use pg_dump with custom format for better compression and selective restore
    PGPASSWORD="$PGPASSWORD" pg_dump \
        -U "$PGUSER" \
        -h "$PGHOST" \
        -p "$PGPORT" \
        -d "$PGDATABASE" \
        --format=custom \
        --compress=0 \
        --file="$temp_path" \
        --verbose 2>&1 | while read -r line; do
            log_verbose "pg_dump: $line"
        done

    if [[ ! -f "$temp_path" ]]; then
        error_exit "Database dump failed - output file not created"
    fi

    local size
    size=$(stat -c%s "$temp_path" 2>/dev/null || stat -f%z "$temp_path" 2>/dev/null || echo "0")
    log_verbose "Database dump created: $(numfmt --to=iec "$size" 2>/dev/null || echo "${size} bytes")"
}

# Compress backup file
compress_backup() {
    local file="$1"
    local input_path="${BACKUP_TEMP_DIR}/${file}"

    if [[ "$BACKUP_COMPRESS" == "none" ]]; then
        log_verbose "Compression disabled, skipping"
        return 0
    fi

    log "Compressing backup with $BACKUP_COMPRESS..."

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would compress $input_path with $BACKUP_COMPRESS"
        return 0
    fi

    case "$BACKUP_COMPRESS" in
        gzip)
            gzip -9 "$input_path"
            ;;
        zstd)
            zstd -19 --rm -q "$input_path"
            ;;
        xz)
            xz -9 -T0 "$input_path"
            ;;
        *)
            log_warn "Unknown compression: $BACKUP_COMPRESS, skipping"
            return 0
            ;;
    esac

    log_verbose "Compression complete"
}

# Encrypt backup file
encrypt_backup() {
    local file="$1"
    local input_path="${BACKUP_TEMP_DIR}/${file}"

    if [[ -z "$BACKUP_ENCRYPT" ]]; then
        return 0
    fi

    log "Encrypting backup with age..."

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would encrypt $input_path"
        return 0
    fi

    age --encrypt --recipients-file "$BACKUP_ENCRYPT" \
        -o "${input_path}.age" \
        "$input_path"

    # Securely remove unencrypted version
    if command -v shred &> /dev/null; then
        shred -u "$input_path"
    else
        rm -f "$input_path"
    fi

    log_verbose "Encryption complete"
}

# Get final backup filename
get_backup_filename() {
    local base="$1"
    local filename="$base"

    case "$BACKUP_COMPRESS" in
        gzip) filename="${filename}.gz" ;;
        zstd) filename="${filename}.zst" ;;
        xz) filename="${filename}.xz" ;;
    esac

    if [[ -n "$BACKUP_ENCRYPT" ]]; then
        filename="${filename}.age"
    fi

    echo "$filename"
}

# Copy to local destination
copy_to_local() {
    local file="$1"
    local source="${BACKUP_TEMP_DIR}/${file}"
    local dest="${BACKUP_DEST}/${file}"

    log "Copying backup to local: $BACKUP_DEST"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would copy $source -> $dest"
        return 0
    fi

    cp "$source" "$dest"
    chmod 600 "$dest"

    local size
    size=$(stat -c%s "$dest" 2>/dev/null || stat -f%z "$dest" 2>/dev/null || echo "0")
    log_success "Local backup: $dest ($(numfmt --to=iec "$size" 2>/dev/null || echo "${size} bytes"))"
}

# Sync to remote via rsync
sync_to_remote() {
    local file="$1"
    local source="${BACKUP_TEMP_DIR}/${file}"

    if [[ -z "$BACKUP_REMOTE_RSYNC" ]]; then
        log_verbose "No rsync destination configured"
        return 0
    fi

    log "Syncing backup to rsync: $BACKUP_REMOTE_RSYNC"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would rsync $source -> $BACKUP_REMOTE_RSYNC/"
        return 0
    fi

    rsync -avz --timeout=300 "$source" "${BACKUP_REMOTE_RSYNC}/" 2>&1 | while read -r line; do
        log_verbose "rsync: $line"
    done

    log_success "Rsync backup: ${BACKUP_REMOTE_RSYNC}/${file}"
}

# Upload to S3
upload_to_s3() {
    local file="$1"
    local source="${BACKUP_TEMP_DIR}/${file}"

    if [[ -z "$BACKUP_REMOTE_S3" ]]; then
        log_verbose "No S3 destination configured"
        return 0
    fi

    if ! command -v aws &> /dev/null; then
        log_warn "AWS CLI not found, skipping S3 upload"
        return 1
    fi

    log "Uploading backup to S3: $BACKUP_REMOTE_S3"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would upload $source -> ${BACKUP_REMOTE_S3}/${file}"
        return 0
    fi

    aws s3 cp "$source" "${BACKUP_REMOTE_S3}/${file}" \
        --storage-class STANDARD_IA 2>&1 | while read -r line; do
        log_verbose "s3: $line"
    done

    log_success "S3 backup: ${BACKUP_REMOTE_S3}/${file}"
}

# Distribute backup to all configured destinations
distribute_backup() {
    local file="$1"
    local success=0
    local total=0

    # Local destination
    if [[ -z "$SPECIFIC_DEST" ]] || [[ "$SPECIFIC_DEST" == "local" ]] || [[ "$SPECIFIC_DEST" == "all" ]]; then
        ((++total))
        if copy_to_local "$file"; then
            ((++success))
        fi
    fi

    # Rsync destination
    if [[ -n "$BACKUP_REMOTE_RSYNC" ]]; then
        if [[ -z "$SPECIFIC_DEST" ]] || [[ "$SPECIFIC_DEST" == "rsync" ]] || [[ "$SPECIFIC_DEST" == "all" ]]; then
            ((++total))
            if sync_to_remote "$file"; then
                ((++success))
            fi
        fi
    fi

    # S3 destination
    if [[ -n "$BACKUP_REMOTE_S3" ]]; then
        if [[ -z "$SPECIFIC_DEST" ]] || [[ "$SPECIFIC_DEST" == "s3" ]] || [[ "$SPECIFIC_DEST" == "all" ]]; then
            ((++total))
            if upload_to_s3 "$file"; then
                ((++success))
            fi
        fi
    fi

    if [[ $success -eq 0 ]] && [[ $total -gt 0 ]]; then
        error_exit "All backup destinations failed"
    fi

    log "Backup distributed to $success/$total destinations"
}

# Cleanup old backups based on retention policy
cleanup_old_backups() {
    log "Applying retention policy: keep $BACKUP_RETAIN days"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would cleanup backups older than $BACKUP_RETAIN days"
        find "$BACKUP_DEST" -name "fortemi_backup_*.sql*" -type f -mtime "+$BACKUP_RETAIN" 2>/dev/null | while read -r file; do
            log "DRY RUN: Would delete $file"
        done
        return 0
    fi

    # Cleanup local backups
    if [[ -d "$BACKUP_DEST" ]]; then
        local deleted=0
        while IFS= read -r -d '' file; do
            rm -f "$file"
            log_verbose "Deleted old backup: $file"
            ((++deleted))
        done < <(find "$BACKUP_DEST" -name "fortemi_backup_*.sql*" -type f -mtime "+$BACKUP_RETAIN" -print0 2>/dev/null)

        if [[ $deleted -gt 0 ]]; then
            log "Cleaned up $deleted old backup(s)"
        fi
    fi

    # Cleanup remote rsync (if accessible)
    if [[ -n "$BACKUP_REMOTE_RSYNC" ]]; then
        local remote_host="${BACKUP_REMOTE_RSYNC%%:*}"
        local remote_path="${BACKUP_REMOTE_RSYNC#*:}"
        if ssh -o ConnectTimeout=5 "$remote_host" "find $remote_path -name 'fortemi_backup_*.sql*' -mtime +$BACKUP_RETAIN -delete" 2>/dev/null; then
            log_verbose "Cleaned up old rsync backups"
        fi
    fi

    # Note: S3 cleanup should use lifecycle policies instead
    if [[ -n "$BACKUP_REMOTE_S3" ]]; then
        log_verbose "S3 cleanup: Use S3 lifecycle policies for retention"
    fi
}

# Verify backup integrity
verify_backup() {
    local file="$1"
    local backup_path="${BACKUP_DEST}/${file}"

    log "Verifying backup..."

    if [[ "$DRY_RUN" == "true" ]]; then
        log "DRY RUN: Would verify backup at $backup_path"
        return 0
    fi

    if [[ ! -f "$backup_path" ]]; then
        log_warn "Backup file not found at $backup_path"
        return 1
    fi

    local size
    size=$(stat -c%s "$backup_path" 2>/dev/null || stat -f%z "$backup_path" 2>/dev/null || echo "0")

    if [[ "$size" -lt 1024 ]]; then
        log_warn "Backup file suspiciously small: $size bytes"
        return 1
    fi

    log_success "Backup verified: $(numfmt --to=iec "$size" 2>/dev/null || echo "${size} bytes")"
    return 0
}

# Cleanup temporary files
cleanup_temp() {
    if [[ -d "$BACKUP_TEMP_DIR" ]]; then
        rm -f "${BACKUP_TEMP_DIR}"/fortemi_backup_*.sql* 2>/dev/null || true
    fi
}

# Main backup function
main() {
    local start_time
    start_time=$(date +%s)

    parse_args "$@"
    load_config
    validate_environment

    # Generate backup filename
    local timestamp
    timestamp=$(date '+%Y%m%d_%H%M%S')
    local base_filename="fortemi_backup_${timestamp}.sql"

    log "Starting Fortémi backup..."
    log_verbose "Timestamp: $timestamp"
    log_verbose "Destination: ${SPECIFIC_DEST:-all enabled}"
    log_verbose "Compression: $BACKUP_COMPRESS"
    log_verbose "Encryption: ${BACKUP_ENCRYPT:-disabled}"

    # Trap to ensure cleanup
    trap cleanup_temp EXIT

    # Create database dump
    create_database_dump "$base_filename"

    # Compress if configured
    compress_backup "$base_filename"

    # Encrypt if configured
    local final_filename
    final_filename=$(get_backup_filename "$base_filename")
    if [[ -n "$BACKUP_ENCRYPT" ]]; then
        # Need to encrypt the compressed file
        local compressed_name
        case "$BACKUP_COMPRESS" in
            gzip) compressed_name="${base_filename}.gz" ;;
            zstd) compressed_name="${base_filename}.zst" ;;
            xz) compressed_name="${base_filename}.xz" ;;
            *) compressed_name="$base_filename" ;;
        esac
        encrypt_backup "$compressed_name"
    fi

    # Distribute to destinations
    distribute_backup "$final_filename"

    # Apply retention policy
    cleanup_old_backups

    # Verify
    verify_backup "$final_filename"

    # Calculate duration
    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - start_time))

    log_success "Backup completed in ${duration}s"

    # Output backup info for scripting
    echo "$final_filename"
}

# Run main
main "$@"
