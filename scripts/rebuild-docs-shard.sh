#!/bin/bash
# rebuild-docs-shard.sh - Rebuild the fortemi-docs shard from current sources
#
# Usage: ./scripts/rebuild-docs-shard.sh [API_URL]
#
# Sources:
#   - docs/**/*.md       (user guides, architecture, research, ADRs)
#   - .aiwg/**/*.md      (SDLC artifacts, analyses, reports, tracks)
#   - CHANGELOG.md       (version history)
#   - README.md          (project overview)
#
# This script:
# 1. Deletes the existing fortemi-docs archive (if it exists)
# 2. Creates a fresh archive
# 3. Imports all source files as notes with appropriate tags
# 4. Exports a new shard to docker/seed-data/fortemi-docs.shard
#
# Requires: curl, python3

set -euo pipefail

API_URL="${1:-http://localhost:3000}"
ARCHIVE_NAME="fortemi-docs"
SHARD_OUTPUT="docker/seed-data/fortemi-docs.shard"
MEMORY_HEADER="X-Fortemi-Memory: $ARCHIVE_NAME"

# Change to repo root
cd "$(git rev-parse --show-toplevel)"

echo "=== Rebuilding fortemi-docs shard ==="
echo "API: $API_URL"
echo ""

# Health check
if ! curl -sf "$API_URL/health" >/dev/null 2>&1; then
    echo "ERROR: API not reachable at $API_URL"
    exit 1
fi

# ----- Step 1: Delete existing archive -----
echo "Step 1: Deleting existing archive..."
curl -sf -X DELETE "$API_URL/api/v1/archives/$ARCHIVE_NAME" 2>/dev/null && {
    echo "  Deleted existing '$ARCHIVE_NAME' archive"
} || {
    echo "  No existing archive to delete (or already gone)"
}

# Small delay for schema cleanup
sleep 1

# ----- Step 2: Create fresh archive -----
echo "Step 2: Creating fresh archive..."
curl -sf -X POST "$API_URL/api/v1/archives" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"$ARCHIVE_NAME\",\"description\":\"Fortemi product documentation and knowledge base\"}" >/dev/null || {
    echo "ERROR: Failed to create archive"
    exit 1
}
echo "  Archive created"

# ----- Step 3: Import files as notes -----
echo "Step 3: Importing documentation files..."

SUCCESS=0
FAILED=0
TOTAL=0

# Function to derive tags from file path
get_tags() {
    local filepath="$1"
    local tags=""

    case "$filepath" in
        # --- docs/architecture ---
        docs/architecture/adr/*)
            tags='["fortemi/architecture","fortemi/architecture/adr"]'
            ;;
        docs/architecture/*)
            tags='["fortemi/architecture"]'
            ;;

        # --- docs/content ---
        docs/content/api*.md|docs/content/mcp-rest-parity.md)
            tags='["fortemi/user-guide","fortemi/api-reference"]'
            ;;
        docs/content/mcp*.md)
            tags='["fortemi/user-guide","fortemi/mcp"]'
            ;;
        docs/content/getting-started.md|docs/content/executive-summary.md)
            tags='["fortemi/user-guide","fortemi/getting-started"]'
            ;;
        docs/content/backup.md|docs/content/shard-exchange.md|docs/content/shard-migration.md)
            tags='["fortemi/user-guide","fortemi/deployment"]'
            ;;
        docs/content/deployment*.md|docs/content/operations.md|docs/content/operators-guide.md|docs/content/hardware-planning.md|docs/content/configuration.md|docs/content/ci-cd.md)
            tags='["fortemi/user-guide","fortemi/operator-guide","fortemi/deployment"]'
            ;;
        docs/content/embedding*.md|docs/content/chunking*.md|docs/content/ollama*.md|docs/content/inference*.md)
            tags='["fortemi/user-guide","fortemi/embeddings"]'
            ;;
        docs/content/search*.md|docs/content/multilingual*.md|docs/content/memory-search.md)
            tags='["fortemi/user-guide","fortemi/embeddings"]'
            ;;
        docs/content/multi-memory*.md)
            tags='["fortemi/user-guide","fortemi/deployment"]'
            ;;
        docs/content/troubleshooting.md)
            tags='["fortemi/user-guide","fortemi/troubleshooting"]'
            ;;
        docs/content/test*.md)
            tags='["fortemi/operator-guide","fortemi/deployment"]'
            ;;
        docs/content/tags.md|docs/content/knowledge-graph*.md)
            tags='["fortemi/user-guide","fortemi/embeddings"]'
            ;;
        docs/content/encryption.md|docs/content/pke*.md)
            tags='["fortemi/user-guide"]'
            ;;
        docs/content/licensing.md)
            tags='["fortemi/operator-guide"]'
            ;;
        docs/content/workflows.md|docs/content/use-cases.md|docs/content/best-practices.md)
            tags='["fortemi/user-guide","fortemi/getting-started"]'
            ;;
        docs/content/*)
            tags='["fortemi/user-guide"]'
            ;;

        # --- docs/deployment ---
        docs/deployment/*)
            tags='["fortemi/deployment","fortemi/operator-guide"]'
            ;;

        # --- docs/releases ---
        docs/releases/*)
            tags='["fortemi/changelog"]'
            ;;

        # --- docs/reference ---
        docs/reference/*)
            tags='["fortemi/user-guide"]'
            ;;

        # --- docs/research ---
        docs/research/colbert/*|docs/research/text-embeddings*|docs/research/mrl-*|docs/research/code-embedding*|docs/research/beir-*|docs/research/context_length*|docs/research/MODEL_INVENTORY*)
            tags='["fortemi/research","fortemi/embeddings"]'
            ;;
        docs/research/skos/*)
            tags='["fortemi/research"]'
            ;;
        docs/research/paper-analysis/*)
            tags='["fortemi/research","fortemi/embeddings"]'
            ;;
        docs/research/multi-schema*|docs/research/postgresql-multi-schema*)
            tags='["fortemi/research","fortemi/architecture"]'
            ;;
        docs/research/graph-topology*|docs/research/knowledge-graph*)
            tags='["fortemi/research","fortemi/embeddings"]'
            ;;
        docs/research/postgresql-fts*)
            tags='["fortemi/research","fortemi/embeddings"]'
            ;;
        docs/research/attachment*|docs/research/file-attachments*|docs/research/3d-model*)
            tags='["fortemi/research","fortemi/architecture"]'
            ;;
        docs/research/data-format*)
            tags='["fortemi/research","fortemi/architecture"]'
            ;;
        docs/research/*)
            tags='["fortemi/research"]'
            ;;

        # --- .aiwg ---
        .aiwg/archive/*/sdlc-*|.aiwg/archive/*/implementations/*)
            tags='["fortemi/sdlc","fortemi/architecture"]'
            ;;
        .aiwg/archive/*/ralph/*)
            tags='["fortemi/sdlc","fortemi/changelog"]'
            ;;
        .aiwg/archive/*/discovery-*|.aiwg/archive/*/status-reports/*)
            tags='["fortemi/sdlc","fortemi/research"]'
            ;;
        .aiwg/archive/*)
            tags='["fortemi/sdlc"]'
            ;;
        .aiwg/gates/*)
            tags='["fortemi/sdlc","fortemi/architecture"]'
            ;;
        .aiwg/intake/*)
            tags='["fortemi/sdlc"]'
            ;;
        .aiwg/reports/uat/*)
            tags='["fortemi/sdlc","fortemi/operator-guide"]'
            ;;
        .aiwg/reports/*)
            tags='["fortemi/sdlc"]'
            ;;
        .aiwg/testing/*)
            tags='["fortemi/sdlc","fortemi/operator-guide"]'
            ;;
        .aiwg/tracks/*)
            tags='["fortemi/sdlc","fortemi/architecture"]'
            ;;
        .aiwg/working/*)
            tags='["fortemi/sdlc","fortemi/research"]'
            ;;
        .aiwg/planning/*)
            tags='["fortemi/sdlc"]'
            ;;
        .aiwg/analysis/*)
            tags='["fortemi/sdlc","fortemi/research"]'
            ;;
        .aiwg/*)
            tags='["fortemi/sdlc"]'
            ;;

        # --- Root files ---
        CHANGELOG.md)
            tags='["fortemi/changelog","fortemi/operator-guide"]'
            ;;
        README.md)
            tags='["fortemi/getting-started","fortemi/operator-guide"]'
            ;;
        *)
            tags='["fortemi/user-guide"]'
            ;;
    esac

    echo "$tags"
}

# Import a single file as a note
import_file() {
    local filepath="$1"

    # Skip empty files
    if [ ! -s "$filepath" ]; then
        return 1
    fi

    local tags
    tags=$(get_tags "$filepath")

    # Build JSON payload via python3 (handles content escaping)
    local json_payload
    json_payload=$(python3 -c "
import json, sys
content = open(sys.argv[1], 'r').read()
tags = json.loads(sys.argv[2])
payload = {
    'content': content,
    'tags': tags,
    'revision_mode': 'none'
}
print(json.dumps(payload))
" "$filepath" "$tags")

    # Create note via API
    curl -sf -X POST "$API_URL/api/v1/notes" \
        -H "Content-Type: application/json" \
        -H "$MEMORY_HEADER" \
        -d "$json_payload" >/dev/null 2>&1 || return 1

    return 0
}

# Collect all source files
FILELIST=$(mktemp)
trap "rm -f $FILELIST" EXIT

# docs/ — all markdown (include READMEs, exclude ADR template)
find docs -name '*.md' -not -name 'ADR-TEMPLATE.md' | sort >> "$FILELIST"

# .aiwg/ — all markdown
find .aiwg -name '*.md' -not -path '*/node_modules/*' | sort >> "$FILELIST"

# Root files
echo "CHANGELOG.md" >> "$FILELIST"
echo "README.md" >> "$FILELIST"

FILE_COUNT=$(wc -l < "$FILELIST")
echo "  Found $FILE_COUNT files to import"
echo ""

# Process all files
while IFS= read -r filepath; do
    [ -f "$filepath" ] || continue
    TOTAL=$((TOTAL + 1))

    if import_file "$filepath"; then
        SUCCESS=$((SUCCESS + 1))
        # Overwrite same line for progress
        printf "\r  [%d/%d] %s\033[K" "$SUCCESS" "$FILE_COUNT" "$filepath"
    else
        FAILED=$((FAILED + 1))
        echo ""
        echo "  FAILED: $filepath"
    fi
done < "$FILELIST"

echo ""
echo ""
echo "  Imported $SUCCESS/$TOTAL files ($FAILED failed)"

# ----- Step 4: Export new shard -----
echo ""
echo "Step 4: Exporting shard..."

# Wait a moment for async processing (FTS indexing, etc.)
sleep 2

curl -sf "$API_URL/api/v1/backup/knowledge-shard" \
    -H "$MEMORY_HEADER" \
    -o "$SHARD_OUTPUT" || {
    echo "ERROR: Failed to export shard"
    exit 1
}

SHARD_SIZE=$(stat -c%s "$SHARD_OUTPUT" 2>/dev/null || stat -f%z "$SHARD_OUTPUT" 2>/dev/null)
SHARD_SIZE_KB=$((SHARD_SIZE / 1024))
echo "  Shard exported: $SHARD_OUTPUT ($SHARD_SIZE_KB KB)"

# ----- Step 5: Verify -----
echo ""
echo "Step 5: Verifying shard..."
MANIFEST=$(tar -xzf "$SHARD_OUTPUT" manifest.json -O 2>/dev/null)
NOTES_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['notes'])" 2>/dev/null || echo "?")
LINKS_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['links'])" 2>/dev/null || echo "?")
TAGS_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['tags'])" 2>/dev/null || echo "?")

echo "  Notes: $NOTES_COUNT"
echo "  Links: $LINKS_COUNT"
echo "  Tags: $TAGS_COUNT"
echo ""
echo "=== Done ==="
