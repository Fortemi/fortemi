#!/bin/bash
# seed-support-archive.sh - Load the fortemi-docs support archive
#
# Two invocation modes:
#   1. Auto: bundle-entrypoint.sh runs this on first boot when
#      LOAD_SUPPORT_MEMORY=true. Default off — mirrors the native
#      build path which never seeds.
#   2. On-demand: operators can run this manually inside an already
#      deployed bundle to pull in the docs:
#        docker compose -f docker-compose.bundle.yml \
#          exec fortemi /app/seed-support-archive.sh
#      Idempotent — re-running is a no-op once seeded (flag file).
#
# Environment:
#   LOAD_SUPPORT_MEMORY=true     - Opt-in seeding on first boot
#   DISABLE_SUPPORT_MEMORY=true  - Legacy: forces skip even if LOAD_* set
#                                  (kept for back-compat with earlier
#                                  bundles where seeding was opt-out)
#   PORT (default: 3000)         - API port

set -euo pipefail

API_URL="http://localhost:${PORT:-3000}"
SEED_DIR="/app/seed-data"
SHARD_FILE="$SEED_DIR/fortemi-docs.shard"
FLAG_FILE="${PGDATA:-/var/lib/postgresql/data}/.fortemi-docs-seeded"
ARCHIVE_NAME="fortemi-docs"
# When invoked manually (by an operator running this script directly via
# `docker exec`), force the seed regardless of LOAD_SUPPORT_MEMORY. The
# entrypoint sets MANUAL_INVOCATION=false; an operator running it
# directly leaves it unset which is treated as manual.
MANUAL_INVOCATION="${MANUAL_INVOCATION:-true}"

# Check legacy hard-skip — operators who explicitly disabled the seed
# in the previous opt-out world keep that behavior.
if [ "${DISABLE_SUPPORT_MEMORY:-false}" = "true" ]; then
    echo "  Support memory disabled (DISABLE_SUPPORT_MEMORY=true), skipping"
    return 0 2>/dev/null || exit 0
fi

# Auto-invocation gate: only proceed automatically when the operator
# has explicitly opted in. Manual invocations bypass this check.
if [ "${MANUAL_INVOCATION}" = "false" ] \
   && [ "${LOAD_SUPPORT_MEMORY:-false}" != "true" ]; then
    echo "  Support memory not requested (LOAD_SUPPORT_MEMORY!=true), skipping auto-seed"
    echo "  To seed manually:"
    echo "    docker compose -f docker-compose.bundle.yml exec fortemi /app/seed-support-archive.sh"
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

echo "  Archive created. Importing shard (FTS-only, no embeddings)..."

# Import the shard via multipart file upload (no base64 overhead).
# skip_embedding_regen=true: import data only — no NLP pipeline, no
# embedding generation, no auto-linking. Postgres tsvector full-text
# search is populated by table triggers on insert and is fully usable.
# Operators who want semantic search over the support archive can opt
# in later with:
#   curl -X POST $API_URL/api/v1/notes/reprocess \
#     -H 'X-Fortemi-Memory: fortemi-docs' \
#     -H 'Content-Type: application/json' \
#     -d '{"steps":["embedding"],"revision_mode":"none"}'
IMPORT_RESPONSE=$(curl -sf -X POST \
    -H "X-Fortemi-Memory: $ARCHIVE_NAME" \
    -F "file=@$SHARD_FILE" \
    "$API_URL/api/v1/backup/knowledge-shard/upload?on_conflict=skip&skip_embedding_regen=true" 2>&1) || {
    echo "  WARNING: Shard import failed: $IMPORT_RESPONSE"
    return 0 2>/dev/null || exit 0
}

# Parse import counts (no jq dependency — try common response field names).
# `head -1` is critical: the response JSON has multiple matches for the
# generic "notes" / "links" fallbacks (e.g. notes_imported alongside
# notes_skipped), and without head the count came out as a multi-line
# string that broke the formatted summary. Observed in v2026.5.5 deploy.
# Also normalize to "0" instead of empty so the formatted line stays
# clean when grep finds nothing.
parse_count() {
    local resp="$1"; shift
    for key in "$@"; do
        local v
        v=$(printf '%s' "$resp" | grep -oP "\"${key}\"\\s*:\\s*\\K[0-9]+" | head -1)
        if [ -n "$v" ]; then printf '%s' "$v"; return; fi
    done
    printf '0'
}
NOTES_IMPORTED=$(parse_count "$IMPORT_RESPONSE" notes_imported notes_created notes)
LINKS_IMPORTED=$(parse_count "$IMPORT_RESPONSE" links_imported links_created links)

echo "  Shard imported: $NOTES_IMPORTED notes, $LINKS_IMPORTED links"
echo "  (FTS-ready; semantic embeddings NOT generated by default)"
echo ""
echo "  To enable semantic search over the support archive:"
echo "    curl -X POST $API_URL/api/v1/notes/reprocess \\"
echo "      -H 'X-Fortemi-Memory: $ARCHIVE_NAME' \\"
echo "      -H 'Content-Type: application/json' \\"
echo "      -d '{\"steps\":[\"embedding\"],\"revision_mode\":\"none\"}'"

# Write flag file
touch "$FLAG_FILE"
echo "  Support archive seeded successfully"
