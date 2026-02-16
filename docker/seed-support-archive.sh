#!/bin/bash
# seed-support-archive.sh - Load the fortemi-docs support archive on first boot
#
# Called by bundle-entrypoint.sh after the API is healthy.
# Idempotent: checks if the archive already exists before creating it.
#
# Environment:
#   DISABLE_SUPPORT_MEMORY=true  - Skip support archive creation
#   PORT (default: 3000)         - API port

set -euo pipefail

API_URL="http://localhost:${PORT:-3000}"
SEED_DIR="/app/seed-data"
SHARD_FILE="$SEED_DIR/fortemi-docs.shard"
FLAG_FILE="${PGDATA:-/var/lib/postgresql/data}/.fortemi-docs-seeded"
ARCHIVE_NAME="fortemi-docs"

# Check opt-out
if [ "${DISABLE_SUPPORT_MEMORY:-false}" = "true" ]; then
    echo "  Support memory disabled (DISABLE_SUPPORT_MEMORY=true), skipping"
    return 0 2>/dev/null || exit 0
fi

# Check if already seeded (flag file on pgdata volume survives restarts)
if [ -f "$FLAG_FILE" ]; then
    echo "  Support archive already seeded (flag: $FLAG_FILE), skipping"
    return 0 2>/dev/null || exit 0
fi

# Check if shard file exists
if [ ! -f "$SHARD_FILE" ]; then
    echo "  WARNING: Shard file not found at $SHARD_FILE, skipping support archive"
    return 0 2>/dev/null || exit 0
fi

# Check if archive already exists (handles case where flag file was lost)
ARCHIVE_EXISTS=$(curl -sf "$API_URL/api/v1/archives" 2>/dev/null \
    | grep -o "\"name\":\"$ARCHIVE_NAME\"" || true)
if [ -n "$ARCHIVE_EXISTS" ]; then
    echo "  Archive '$ARCHIVE_NAME' already exists, writing flag and skipping"
    touch "$FLAG_FILE"
    return 0 2>/dev/null || exit 0
fi

echo "  Creating '$ARCHIVE_NAME' archive..."

# Create the archive
CREATE_RESPONSE=$(curl -sf -X POST "$API_URL/api/v1/archives" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$ARCHIVE_NAME\",\"description\":\"Fortemi product documentation and knowledge base\"}" 2>&1) || {
    echo "  WARNING: Failed to create archive: $CREATE_RESPONSE"
    return 0 2>/dev/null || exit 0
}

echo "  Archive created. Importing shard..."

# Import the shard — use a temp file to avoid ARG_MAX limit with large shards
TMPFILE=$(mktemp /tmp/shard-import-XXXXXX.json)
trap "rm -f $TMPFILE" EXIT

# Build JSON body with base64 shard data written to file (avoids shell arg limit)
printf '{"shard_base64":"' > "$TMPFILE"
base64 -w0 "$SHARD_FILE" >> "$TMPFILE"
printf '","on_conflict":"skip"}' >> "$TMPFILE"

IMPORT_RESPONSE=$(curl -sf -X POST "$API_URL/api/v1/backup/knowledge-shard/import" \
    -H "Content-Type: application/json" \
    -H "X-Fortemi-Memory: $ARCHIVE_NAME" \
    -d @"$TMPFILE" 2>&1) || {
    echo "  WARNING: Shard import failed: $IMPORT_RESPONSE"
    return 0 2>/dev/null || exit 0
}

rm -f "$TMPFILE"

# Parse import counts (no jq dependency — try common response field names)
NOTES_IMPORTED=$(echo "$IMPORT_RESPONSE" | grep -oP '"notes_imported"\s*:\s*\K[0-9]+' || \
    echo "$IMPORT_RESPONSE" | grep -oP '"notes"\s*:\s*\K[0-9]+' || echo "?")
LINKS_IMPORTED=$(echo "$IMPORT_RESPONSE" | grep -oP '"links_imported"\s*:\s*\K[0-9]+' || \
    echo "$IMPORT_RESPONSE" | grep -oP '"links"\s*:\s*\K[0-9]+' || echo "?")

echo "  Shard imported: $NOTES_IMPORTED notes, $LINKS_IMPORTED links"

# Write flag file
touch "$FLAG_FILE"
echo "  Support archive seeded successfully"
