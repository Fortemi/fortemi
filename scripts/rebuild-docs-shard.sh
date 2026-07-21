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

    # Build JSON payload via python3 (handles content escaping + title
    # derivation). Title precedence (#675):
    #   1. First "# H1" line in the markdown (skipping blank lines + the
    #      conventional YAML front-matter block)
    #   2. Filepath-derived: basename, strip .md, swap separators for spaces
    # We never use revision_mode-driven AI title generation here — keeps the
    # rebuild path inference-free so it runs in CI without Ollama.
    local response_file
    response_file=$(mktemp)
    IMPORT_RESPONSE_FILE="$response_file"

    # Stream the generated payload. Linux limits each argv entry to roughly
    # 128 KiB, so passing note JSON via -d breaks for large tracked sources.
    if ! python3 -c "
import json, re, os, sys
filepath = sys.argv[1]
content = open(filepath, 'r').read()
tags = json.loads(sys.argv[2])

def title_from_path(p):
    stem = os.path.splitext(os.path.basename(p))[0]
    # Replace separator runs with single spaces; trim.
    return re.sub(r'[-_]+', ' ', stem).strip() or os.path.basename(p)

def title_from_h1(text):
    # Skip a leading YAML front-matter block if present, then find the
    # first '# heading' line. Bound the scan to the first 200 lines so a
    # huge file doesn't slow the import loop measurably.
    lines = text.splitlines()
    i = 0
    if lines and lines[0].strip() == '---':
        # YAML front-matter: skip until closing '---'
        i = 1
        while i < len(lines) and lines[i].strip() != '---':
            i += 1
        i += 1  # past the closing '---'
    for line in lines[i:i+200]:
        m = re.match(r'^#\s+(.+?)\s*$', line)
        if m:
            return m.group(1).strip()
    return None

title = title_from_h1(content) or title_from_path(filepath)

payload = {
    'title': title,
    'content': content,
    'tags': tags,
    'revision_mode': 'none'
}
print(json.dumps(payload))
" "$filepath" "$tags" | curl --silent --show-error --fail-with-body \
        -X POST "$API_URL/api/v1/notes" \
        -H "Content-Type: application/json" \
        -H "$MEMORY_HEADER" \
        --data-binary @- \
        -o "$response_file"; then
        echo "  API response (first 4096 bytes):" >&2
        head -c 4096 "$response_file" >&2 || true
        echo >&2
        rm -f "$response_file"
        IMPORT_RESPONSE_FILE=""
        return 1
    fi

    rm -f "$response_file"
    IMPORT_RESPONSE_FILE=""

    return 0
}

# Collect all source files
FILELIST=$(mktemp)
IMPORT_RESPONSE_FILE=""
EXPORT_TEMP_FILE=""
EXPORT_HEADERS_FILE=""
cleanup() {
    rm -f "$FILELIST"
    if [ -n "$IMPORT_RESPONSE_FILE" ]; then
        rm -f "$IMPORT_RESPONSE_FILE"
    fi
    if [ -n "$EXPORT_TEMP_FILE" ]; then
        rm -f "$EXPORT_TEMP_FILE"
    fi
    if [ -n "$EXPORT_HEADERS_FILE" ]; then
        rm -f "$EXPORT_HEADERS_FILE"
    fi
}
trap cleanup EXIT

# docs/ — all markdown (include READMEs, exclude ADR template)
find docs -type f -size +0c -name '*.md' -not -name 'ADR-TEMPLATE.md' | sort >> "$FILELIST"

# .aiwg/ — all markdown
find .aiwg -type f -size +0c -name '*.md' -not -path '*/node_modules/*' | sort >> "$FILELIST"

# Root files
[ ! -s CHANGELOG.md ] || echo "CHANGELOG.md" >> "$FILELIST"
[ ! -s README.md ] || echo "README.md" >> "$FILELIST"

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
        echo "ERROR: Refusing to export a shard missing tracked source '$filepath'." >&2
        exit 1
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

EXPORT_TEMP_FILE=$(mktemp "${SHARD_OUTPUT}.tmp.XXXXXX")
EXPORT_HEADERS_FILE=$(mktemp)
if ! curl --silent --show-error --fail-with-body \
    --dump-header "$EXPORT_HEADERS_FILE" \
    "$API_URL/api/v1/backup/knowledge-shard" \
    -H "$MEMORY_HEADER" \
    -o "$EXPORT_TEMP_FILE"; then
    echo "ERROR: Failed to export shard"
    head -c 4096 "$EXPORT_TEMP_FILE" >&2 || true
    echo >&2
    exit 1
fi

if [ ! -s "$EXPORT_TEMP_FILE" ]; then
    echo "ERROR: Shard export returned an empty body; preserving $SHARD_OUTPUT" >&2
    echo "Response headers:" >&2
    head -40 "$EXPORT_HEADERS_FILE" >&2 || true
    exit 1
fi
if ! tar -tzf "$EXPORT_TEMP_FILE" >/dev/null 2>&1; then
    echo "ERROR: Shard export is not a readable gzip archive; preserving $SHARD_OUTPUT" >&2
    exit 1
fi

SHARD_SIZE=$(stat -c%s "$EXPORT_TEMP_FILE" 2>/dev/null || stat -f%z "$EXPORT_TEMP_FILE" 2>/dev/null)
SHARD_SIZE_KB=$((SHARD_SIZE / 1024))
echo "  Shard candidate downloaded ($SHARD_SIZE_KB KB)"

# ----- Step 5: Verify -----
echo ""
echo "Step 5: Verifying shard..."
MANIFEST=$(tar -xzf "$EXPORT_TEMP_FILE" manifest.json -O 2>/dev/null)
NOTES_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['notes'])" 2>/dev/null || echo "?")
LINKS_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['links'])" 2>/dev/null || echo "?")
TAGS_COUNT=$(echo "$MANIFEST" | python3 -c "import sys,json; print(json.load(sys.stdin)['counts']['tags'])" 2>/dev/null || echo "?")

python3 scripts/ci/verify-docs-shard-coverage.py \
    --file-list "$FILELIST" \
    --shard "$EXPORT_TEMP_FILE"

mv "$EXPORT_TEMP_FILE" "$SHARD_OUTPUT"
EXPORT_TEMP_FILE=""
rm -f "$EXPORT_HEADERS_FILE"
EXPORT_HEADERS_FILE=""
echo "  Shard exported: $SHARD_OUTPUT ($SHARD_SIZE_KB KB)"

echo "  Notes: $NOTES_COUNT"
echo "  Links: $LINKS_COUNT"
echo "  Tags: $TAGS_COUNT"
echo ""
echo "=== Done ==="
