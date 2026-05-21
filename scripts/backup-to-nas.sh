#!/usr/bin/env bash
#
# backup-to-nas.sh — Daily pg_dump of the Fortemi bundle container to a NAS path.
#
# Designed to be run from a launchd LaunchAgent on macOS (see
# `infra/launchd/com.fortemi.backup.plist.template`). The full path the dump
# lands at is:
#   ${BACKUP_DEST}/matric-YYYYMMDD-HHMMSS.dump
#
# The dump is written to a `.dump.partial` sibling and renamed atomically
# on success, so a launchd-killed mid-write run never leaves a half-written
# file at a name a restore tool would pick up. Dumps older than
# BACKUP_RETAIN_DAYS (default 14) are deleted at the end of each successful
# run; failed runs leave existing backups intact.
#
# Environment variables (all optional):
#   BACKUP_DEST         destination directory (default: /Volumes/home/macstudio/backups/fortemi)
#   BACKUP_RETAIN_DAYS  retention in days (default: 14)
#   CONTAINER_NAME      docker container to dump from (default: fortemi-matric-1)
#   PG_USER             pg role (default: matric)
#   PG_DB               pg database (default: matric)
#
# Exit codes:
#   0  — dump complete + retention pruned
#   2  — container not running
#   3  — destination directory missing or not writable (e.g. NAS unmounted)
#   4  — pg_dump failed
#

set -euo pipefail

BACKUP_DEST="${BACKUP_DEST:-/Volumes/home/macstudio/backups/fortemi}"
BACKUP_RETAIN_DAYS="${BACKUP_RETAIN_DAYS:-14}"
CONTAINER_NAME="${CONTAINER_NAME:-fortemi-matric-1}"
PG_USER="${PG_USER:-matric}"
PG_DB="${PG_DB:-matric}"

ts_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }
log() { printf '[%s] backup-to-nas: %s\n' "$(ts_utc)" "$*"; }

log "starting; dest=${BACKUP_DEST} retain=${BACKUP_RETAIN_DAYS}d container=${CONTAINER_NAME}"

# 1. Container check — only dump if the bundle is actually running. Refusing
# to start a dump when the container is down avoids a confusing pg_dump
# connection-refused failure that's easily mistaken for a NAS problem.
if ! docker ps --filter "name=^${CONTAINER_NAME}$" --format '{{.Status}}' | grep -q .; then
    log "ERROR: container '${CONTAINER_NAME}' is not running. Skipping backup."
    exit 2
fi

# 2. Destination check — must exist and be writable. We don't auto-create
# the directory because a missing /Volumes/home/* most likely means the
# NAS is not mounted, and silently writing to the unmounted mountpoint
# would fill the boot disk.
if [ ! -d "${BACKUP_DEST}" ] || [ ! -w "${BACKUP_DEST}" ]; then
    log "ERROR: BACKUP_DEST does not exist or is not writable: ${BACKUP_DEST}"
    log "       (is the NAS share mounted?)"
    exit 3
fi

# 3. Dump to a .partial sibling, then atomic-rename on success.
STAMP="$(date +%Y%m%d-%H%M%S)"
PARTIAL="${BACKUP_DEST}/matric-${STAMP}.dump.partial"
FINAL="${BACKUP_DEST}/matric-${STAMP}.dump"

log "dumping ${PG_DB} (user=${PG_USER}) from ${CONTAINER_NAME} → ${FINAL##*/}"
if ! docker exec -i "${CONTAINER_NAME}" pg_dump -U "${PG_USER}" -Fc "${PG_DB}" > "${PARTIAL}"; then
    log "ERROR: pg_dump exited non-zero; removing partial file"
    rm -f -- "${PARTIAL}"
    exit 4
fi

# Atomic rename now that we know the dump completed.
mv -f -- "${PARTIAL}" "${FINAL}"
SIZE_BYTES="$(stat -f %z "${FINAL}" 2>/dev/null || stat -c %s "${FINAL}" 2>/dev/null || echo "?")"
log "wrote ${FINAL} (${SIZE_BYTES} bytes)"

# 4. Retention pruning — only run after a successful dump, so a failed run
# never deletes existing backups to make room for a missing new one.
if [ "${BACKUP_RETAIN_DAYS}" -gt 0 ]; then
    DELETED=0
    while IFS= read -r -d '' old; do
        log "pruning aged-out dump: ${old##*/}"
        rm -f -- "${old}"
        DELETED=$((DELETED + 1))
    done < <(find "${BACKUP_DEST}" -maxdepth 1 -name 'matric-*.dump' -type f -mtime "+${BACKUP_RETAIN_DAYS}" -print0)
    log "retention pass complete (${DELETED} dumps removed, retain=${BACKUP_RETAIN_DAYS}d)"
fi

log "done"
