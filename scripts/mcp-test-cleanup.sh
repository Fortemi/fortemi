#!/bin/bash
# MCP Integration Test - Cleanup Script
# Removes all test data fixtures created by mcp-test-setup.sh
#
# Usage: ./scripts/mcp-test-cleanup.sh [BASE_URL] [SCRATCHPAD]
# Example: ./scripts/mcp-test-cleanup.sh http://localhost:3000 /tmp/mcp-test-12345

set -e

BASE_URL="${1:-${MATRIC_API_URL:-http://localhost:3000}}"
SCRATCHPAD="${2:-${MCP_TEST_SCRATCHPAD:-}}"

echo "========================================"
echo "MCP Integration Test Cleanup"
echo "========================================"
echo "Target: $BASE_URL"
echo "Scratchpad: ${SCRATCHPAD:-not specified}"
echo "Date: $(date)"
echo "========================================"
echo

CLEANED=0
FAILED=0

clean_success() { echo "  ✓ $1"; ((CLEANED++)) || true; }
clean_fail() { echo "  ✗ $1"; ((FAILED++)) || true; }

api_delete() {
    local endpoint="$1"
    curl -sf -X DELETE "$BASE_URL/api/v1$endpoint" 2>/dev/null
}

api_get() {
    local endpoint="$1"
    curl -sf "$BASE_URL/api/v1$endpoint" 2>/dev/null
}

api_post() {
    local endpoint="$1"
    local data="$2"
    curl -sf -X POST "$BASE_URL/api/v1$endpoint" \
        -H "Content-Type: application/json" \
        -d "$data" 2>/dev/null
}

# Load test IDs if available
if [ -n "$SCRATCHPAD" ] && [ -f "$SCRATCHPAD/test-ids.env" ]; then
    echo "Loading test IDs from $SCRATCHPAD/test-ids.env"
    source "$SCRATCHPAD/test-ids.env"
    echo
fi

# ============================================
# 1. PURGE TEST NOTES
# ============================================
echo "=== Cleaning Test Notes ==="

# Find all notes with mcp-test tag
echo "  Searching for notes with 'mcp-test' tag..."
TEST_NOTES=$(api_get "/notes?tags=mcp-test&limit=100" | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)

if [ -n "$TEST_NOTES" ]; then
    NOTE_COUNT=$(echo "$TEST_NOTES" | wc -l)
    echo "  Found $NOTE_COUNT test notes to purge"

    for note_id in $TEST_NOTES; do
        if api_post "/notes/$note_id/purge" '{}' > /dev/null 2>&1; then
            clean_success "Purged note $note_id"
        else
            clean_fail "Failed to purge note $note_id"
        fi
    done
else
    echo "  No test notes found"
fi

echo

# ============================================
# 2. DELETE TEST COLLECTIONS
# ============================================
echo "=== Cleaning Test Collections ==="

# Find collections by name pattern
COLLECTIONS=$(api_get "/collections" | grep -o '"id":"[^"]*","name":"MCP Test[^"]*"' | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)

if [ -n "$COLLECTIONS" ]; then
    for coll_id in $COLLECTIONS; do
        if api_delete "/collections/$coll_id" > /dev/null 2>&1; then
            clean_success "Deleted collection $coll_id"
        else
            clean_fail "Failed to delete collection $coll_id"
        fi
    done
else
    echo "  No test collections found"
fi

echo

# ============================================
# 3. DELETE TEST TEMPLATES
# ============================================
echo "=== Cleaning Test Templates ==="

# Find templates by name pattern
TEMPLATES=$(api_get "/templates" | grep -o '"id":"[^"]*","name":"MCP Test[^"]*"' | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)

if [ -n "$TEMPLATES" ]; then
    for tmpl_id in $TEMPLATES; do
        if api_delete "/templates/$tmpl_id" > /dev/null 2>&1; then
            clean_success "Deleted template $tmpl_id"
        else
            clean_fail "Failed to delete template $tmpl_id"
        fi
    done
else
    echo "  No test templates found"
fi

echo

# ============================================
# 4. DELETE TEST EMBEDDING SETS
# ============================================
echo "=== Cleaning Test Embedding Sets ==="

# Delete by known slugs
for slug in "mcp-test-set" "mcp-manual-set"; do
    if api_delete "/embedding-sets/$slug" > /dev/null 2>&1; then
        clean_success "Deleted embedding set $slug"
    else
        echo "  - Embedding set $slug not found or already deleted"
    fi
done

echo

# ============================================
# 5. DELETE TEST SKOS CONCEPTS
# ============================================
echo "=== Cleaning Test SKOS Taxonomy ==="

# Find the test scheme
SCHEME_ID=$(api_get "/concepts/schemes" | grep -o '"id":"[^"]*","notation":"mcp-test-scheme"' | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)

if [ -n "$SCHEME_ID" ]; then
    echo "  Found test scheme: $SCHEME_ID"

    # Find all concepts in the scheme
    CONCEPTS=$(api_get "/concepts?scheme_id=$SCHEME_ID&limit=100" | grep -o '"id":"[^"]*"' | cut -d'"' -f4 || true)

    if [ -n "$CONCEPTS" ]; then
        # Delete concepts (children first to avoid constraint issues)
        for concept_id in $CONCEPTS; do
            if api_delete "/concepts/$concept_id" > /dev/null 2>&1; then
                clean_success "Deleted concept $concept_id"
            else
                clean_fail "Failed to delete concept $concept_id"
            fi
        done
    fi

    # Note: Scheme deletion may require all concepts to be deleted first
    # The API might not support scheme deletion yet
    echo "  Note: Concept scheme cleanup may require manual database intervention"
else
    echo "  No test concept scheme found"
fi

echo

# ============================================
# 6. CLEANUP SCRATCHPAD
# ============================================
echo "=== Cleaning Scratchpad ==="

if [ -n "$SCRATCHPAD" ] && [ -d "$SCRATCHPAD" ]; then
    rm -rf "$SCRATCHPAD"
    clean_success "Removed scratchpad directory: $SCRATCHPAD"
else
    echo "  No scratchpad to clean"
fi

echo

# ============================================
# 7. WAIT FOR PURGE JOBS
# ============================================
echo "=== Waiting for Purge Jobs ==="
echo -n "  Waiting for background purge jobs"

MAX_WAIT=30
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    PENDING=$(api_get "/jobs/stats" | grep -o '"pending":[0-9]*' | cut -d: -f2 2>/dev/null || echo "0")
    PROCESSING=$(api_get "/jobs/stats" | grep -o '"processing":[0-9]*' | cut -d: -f2 2>/dev/null || echo "0")

    if [ "${PENDING:-0}" -eq 0 ] && [ "${PROCESSING:-0}" -eq 0 ]; then
        echo " done"
        break
    fi

    echo -n "."
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo " timeout"
fi

echo

# ============================================
# SUMMARY
# ============================================
echo "========================================"
echo "Cleanup Complete"
echo "========================================"
echo "Cleaned: $CLEANED items"
echo "Failed:  $FAILED items"
echo "========================================"

if [ "$FAILED" -gt 0 ]; then
    echo
    echo "Some cleanup operations failed. You may need to:"
    echo "  1. Check if items were already deleted"
    echo "  2. Manually remove remaining test data"
    echo "  3. Check the API logs for errors"
    exit 1
fi
