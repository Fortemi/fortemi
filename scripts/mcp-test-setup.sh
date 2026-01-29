#!/bin/bash
# MCP Integration Test - Setup Script
# Creates all test data fixtures required for the full test suite
#
# Usage: ./scripts/mcp-test-setup.sh [BASE_URL]
# Example: ./scripts/mcp-test-setup.sh http://localhost:3000

set -e

BASE_URL="${1:-${MATRIC_API_URL:-http://localhost:3000}}"
SCRATCHPAD="${MCP_TEST_SCRATCHPAD:-/tmp/mcp-test-$$}"

echo "========================================"
echo "MCP Integration Test Setup"
echo "========================================"
echo "Target: $BASE_URL"
echo "Scratchpad: $SCRATCHPAD"
echo "Date: $(date)"
echo "========================================"
echo

# Create scratchpad directory
mkdir -p "$SCRATCHPAD"

# Output file for test IDs
TEST_IDS="$SCRATCHPAD/test-ids.env"
echo "# MCP Test IDs - Generated $(date)" > "$TEST_IDS"

api_post() {
    local endpoint="$1"
    local data="$2"
    curl -sf -X POST "$BASE_URL/api/v1$endpoint" \
        -H "Content-Type: application/json" \
        -d "$data"
}

api_get() {
    local endpoint="$1"
    curl -sf "$BASE_URL/api/v1$endpoint"
}

extract_id() {
    grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4
}

# ============================================
# 1. NOTES
# ============================================
echo "=== Creating Test Notes ==="

# 1.1 Full revision note
echo -n "  Creating full revision note... "
NOTE_FULL=$(api_post "/notes" '{
    "content": "# MCP Integration Test - Full Revision\n\nThis note tests the complete AI revision pipeline.\n\n## Features Tested\n- Contextual enhancement\n- Related note discovery\n- Semantic expansion\n\n## Expected Behavior\nThe AI should enhance this content with additional context from the knowledge base.",
    "tags": ["mcp-test", "integration", "full-revision"],
    "revision_mode": "full"
}' | extract_id)
echo "ID: $NOTE_FULL"
echo "NOTE_FULL=$NOTE_FULL" >> "$TEST_IDS"

# 1.2 Light revision note
echo -n "  Creating light revision note... "
NOTE_LIGHT=$(api_post "/notes" '{
    "content": "Quick observation: MCP tests are running at timestamp placeholder.\n\nThis note should only receive formatting improvements, not content expansion.",
    "tags": ["mcp-test", "light-revision"],
    "revision_mode": "light"
}' | extract_id)
echo "ID: $NOTE_LIGHT"
echo "NOTE_LIGHT=$NOTE_LIGHT" >> "$TEST_IDS"

# 1.3 Raw note (no revision)
echo -n "  Creating raw note... "
NOTE_RAW=$(api_post "/notes" '{
    "content": "EXACT: This content must remain completely unchanged.\n\nNo AI modifications allowed.\nPreserve whitespace and formatting exactly.",
    "tags": ["mcp-test", "raw", "no-revision"],
    "revision_mode": "none"
}' | extract_id)
echo "ID: $NOTE_RAW"
echo "NOTE_RAW=$NOTE_RAW" >> "$TEST_IDS"

# 1.4 Bulk notes
echo -n "  Creating bulk notes... "
BULK_RESULT=$(api_post "/notes/bulk" '{
    "notes": [
        {"content": "Bulk note 1 - Testing batch creation", "tags": ["mcp-test", "bulk"], "revision_mode": "none"},
        {"content": "Bulk note 2 - Second in batch", "tags": ["mcp-test", "bulk"], "revision_mode": "none"},
        {"content": "Bulk note 3 - Third in batch", "tags": ["mcp-test", "bulk"], "revision_mode": "none"}
    ]
}')
BULK_IDS=$(echo "$BULK_RESULT" | grep -o '"ids":\[[^]]*\]' | sed 's/"ids":\[//;s/\]//;s/"//g')
echo "IDs: $BULK_IDS"
echo "BULK_IDS=$BULK_IDS" >> "$TEST_IDS"

# 1.5 Large document for chunking tests
echo -n "  Creating large chunked document... "
LARGE_CONTENT=$(cat << 'ENDLARGE'
# MCP Chunking Test Document

This is a large document designed to trigger the chunking system.

## Section 1: Introduction

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

## Section 2: Technical Details

The semantic chunking algorithm splits documents at natural boundaries such as:
- Paragraph breaks
- Section headings
- Code block boundaries
- List item separations

Each chunk maintains overlap with adjacent chunks to preserve context across boundaries.

### 2.1 Configuration Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| max_chunk_size | 1000 | Maximum characters per chunk |
| min_chunk_size | 100 | Minimum size before merging |
| overlap | 100 | Characters of overlap |

### 2.2 Chunking Strategies

1. **SemanticChunker** - Uses markdown structure
2. **ParagraphChunker** - Splits on double newlines
3. **SentenceChunker** - Splits on sentence boundaries
4. **SlidingWindowChunker** - Fixed-size windows

## Section 3: Implementation Notes

The chunking system processes documents in multiple passes:

First pass: Identify structural boundaries
Second pass: Calculate optimal split points
Third pass: Generate chunks with overlap
Fourth pass: Validate chunk sizes

## Section 4: Testing Considerations

When testing the chunking system:
- Verify chunk boundaries align with semantic units
- Check overlap preservation
- Validate reconstruction from chunks
- Test edge cases (empty sections, long lines)

## Section 5: Conclusion

This document should be split into multiple chunks based on its structure. The search system should be able to find relevant chunks and reconstruct the full document when needed.
ENDLARGE
)
NOTE_CHUNKED=$(api_post "/notes" "{
    \"content\": $(echo "$LARGE_CONTENT" | jq -Rs .),
    \"tags\": [\"mcp-test\", \"chunked\", \"large-document\"],
    \"revision_mode\": \"none\"
}" | extract_id)
echo "ID: $NOTE_CHUNKED"
echo "NOTE_CHUNKED=$NOTE_CHUNKED" >> "$TEST_IDS"

# 1.6 Note for star/archive testing
echo -n "  Creating status test note... "
NOTE_STATUS=$(api_post "/notes" '{
    "content": "This note will be used to test starring and archiving functionality.",
    "tags": ["mcp-test", "status-test"],
    "revision_mode": "none"
}' | extract_id)
echo "ID: $NOTE_STATUS"
echo "NOTE_STATUS=$NOTE_STATUS" >> "$TEST_IDS"

# 1.7 Note for version testing
echo -n "  Creating version test note... "
NOTE_VERSION=$(api_post "/notes" '{
    "content": "Version 1: Original content for version testing.",
    "tags": ["mcp-test", "version-test"],
    "revision_mode": "none"
}' | extract_id)
echo "ID: $NOTE_VERSION"
echo "NOTE_VERSION=$NOTE_VERSION" >> "$TEST_IDS"

echo

# ============================================
# 2. COLLECTIONS
# ============================================
echo "=== Creating Test Collections ==="

echo -n "  Creating root collection... "
COLL_ROOT=$(api_post "/collections" '{
    "name": "MCP Test Root",
    "description": "Root collection for MCP integration tests"
}' | extract_id)
echo "ID: $COLL_ROOT"
echo "COLL_ROOT=$COLL_ROOT" >> "$TEST_IDS"

echo -n "  Creating child collection... "
COLL_CHILD=$(api_post "/collections" "{
    \"name\": \"MCP Test Child\",
    \"description\": \"Nested collection for testing hierarchy\",
    \"parent_id\": \"$COLL_ROOT\"
}" | extract_id)
echo "ID: $COLL_CHILD"
echo "COLL_CHILD=$COLL_CHILD" >> "$TEST_IDS"

echo

# ============================================
# 3. TEMPLATES
# ============================================
echo "=== Creating Test Templates ==="

echo -n "  Creating test template... "
TEMPLATE=$(api_post "/templates" '{
    "name": "MCP Test Template",
    "content": "# {{title}}\n\nDate: {{date}}\nAuthor: {{author}}\n\n## Summary\n{{summary}}\n\n## Details\n{{details}}",
    "description": "Template with multiple variables for testing",
    "default_tags": ["mcp-test", "from-template"]
}' | extract_id)
echo "ID: $TEMPLATE"
echo "TEMPLATE=$TEMPLATE" >> "$TEST_IDS"

echo

# ============================================
# 4. SKOS TAXONOMY
# ============================================
echo "=== Creating Test SKOS Taxonomy ==="

echo -n "  Creating concept scheme... "
SCHEME=$(api_post "/concepts/schemes" '{
    "notation": "mcp-test-scheme",
    "title": "MCP Test Scheme",
    "description": "Concept scheme for MCP integration tests"
}' | extract_id)
echo "ID: $SCHEME"
echo "SCHEME=$SCHEME" >> "$TEST_IDS"

echo -n "  Creating root concept... "
CONCEPT_ROOT=$(api_post "/concepts" "{
    \"scheme_id\": \"$SCHEME\",
    \"pref_label\": \"MCP Testing\",
    \"definition\": \"Root concept for all MCP integration test content\",
    \"scope_note\": \"Use this concept for tagging MCP test-related notes\"
}" | extract_id)
echo "ID: $CONCEPT_ROOT"
echo "CONCEPT_ROOT=$CONCEPT_ROOT" >> "$TEST_IDS"

echo -n "  Creating child concept... "
CONCEPT_CHILD=$(api_post "/concepts" "{
    \"scheme_id\": \"$SCHEME\",
    \"pref_label\": \"Integration Tests\",
    \"broader_ids\": [\"$CONCEPT_ROOT\"],
    \"definition\": \"Concept for integration test content\"
}" | extract_id)
echo "ID: $CONCEPT_CHILD"
echo "CONCEPT_CHILD=$CONCEPT_CHILD" >> "$TEST_IDS"

echo -n "  Creating related concept... "
CONCEPT_RELATED=$(api_post "/concepts" "{
    \"scheme_id\": \"$SCHEME\",
    \"pref_label\": \"Test Automation\",
    \"definition\": \"Concept for automated testing content\"
}" | extract_id)
echo "ID: $CONCEPT_RELATED"
echo "CONCEPT_RELATED=$CONCEPT_RELATED" >> "$TEST_IDS"

# Add related relationship
echo -n "  Adding related relationship... "
api_post "/concepts/$CONCEPT_CHILD/related" "{\"target_id\": \"$CONCEPT_RELATED\"}" > /dev/null
echo "done"

echo

# ============================================
# 5. EMBEDDING SETS
# ============================================
echo "=== Creating Test Embedding Sets ==="

echo -n "  Creating auto-mode embedding set... "
SET_AUTO=$(api_post "/embedding-sets" '{
    "name": "MCP Test Set (Auto)",
    "slug": "mcp-test-set",
    "description": "Auto-populated set for MCP test notes",
    "purpose": "Isolate MCP test notes for focused semantic search",
    "mode": "auto",
    "criteria": {"tags": ["mcp-test"]}
}' | extract_id)
echo "ID: $SET_AUTO"
echo "SET_AUTO=$SET_AUTO" >> "$TEST_IDS"

echo -n "  Creating manual-mode embedding set... "
SET_MANUAL=$(api_post "/embedding-sets" '{
    "name": "MCP Manual Set",
    "slug": "mcp-manual-set",
    "description": "Manually populated set for testing member management",
    "mode": "manual"
}' | extract_id)
echo "ID: $SET_MANUAL"
echo "SET_MANUAL=$SET_MANUAL" >> "$TEST_IDS"

echo

# ============================================
# 6. PKE TEST FILES
# ============================================
echo "=== Creating PKE Test Files ==="

echo -n "  Creating test file for encryption... "
echo "This is a test file for PKE encryption testing." > "$SCRATCHPAD/test-plaintext.txt"
echo "Created: $SCRATCHPAD/test-plaintext.txt"
echo "PKE_PLAINTEXT=$SCRATCHPAD/test-plaintext.txt" >> "$TEST_IDS"
echo "PKE_ENCRYPTED=$SCRATCHPAD/test-encrypted.mmpke" >> "$TEST_IDS"
echo "PKE_DECRYPTED=$SCRATCHPAD/test-decrypted.txt" >> "$TEST_IDS"
echo "PKE_PUBKEY=$SCRATCHPAD/test-key.pub" >> "$TEST_IDS"
echo "PKE_PRIVKEY=$SCRATCHPAD/test-key.priv" >> "$TEST_IDS"

echo

# ============================================
# 7. WAIT FOR JOBS
# ============================================
echo "=== Waiting for Background Jobs ==="
echo -n "  Waiting for embedding/revision jobs to complete"

MAX_WAIT=60
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    PENDING=$(api_get "/jobs/stats" | grep -o '"pending":[0-9]*' | cut -d: -f2)
    PROCESSING=$(api_get "/jobs/stats" | grep -o '"processing":[0-9]*' | cut -d: -f2)

    if [ "${PENDING:-0}" -eq 0 ] && [ "${PROCESSING:-0}" -eq 0 ]; then
        echo " done"
        break
    fi

    echo -n "."
    sleep 2
    WAITED=$((WAITED + 2))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo " timeout (jobs may still be running)"
fi

echo

# ============================================
# SUMMARY
# ============================================
echo "========================================"
echo "Setup Complete"
echo "========================================"
echo "Test IDs saved to: $TEST_IDS"
echo
echo "Created:"
echo "  - 7 test notes"
echo "  - 2 collections (nested)"
echo "  - 1 template"
echo "  - 1 concept scheme with 3 concepts"
echo "  - 2 embedding sets"
echo "  - PKE test files"
echo
echo "To use in tests:"
echo "  source $TEST_IDS"
echo
echo "To cleanup:"
echo "  ./scripts/mcp-test-cleanup.sh $BASE_URL $SCRATCHPAD"
echo "========================================"
