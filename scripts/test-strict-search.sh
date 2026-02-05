#!/bin/bash
# Strict Search Isolation Tests
# Tests that strict_filter options guarantee complete data isolation
# using SKOS taxonomy-based filtering
#
# This test creates:
# 1. Two separate "client" concept schemes (simulating multi-tenant isolation)
# 2. Concepts within each scheme
# 3. Notes tagged with concepts from each scheme
# 4. Tests that required_schemes isolates data properly
#
# Usage: ./scripts/test-strict-search.sh [BASE_URL]

BASE_URL="${1:-${MATRIC_API_URL:-http://localhost:3000}}"
TIMESTAMP=$(date +%s)

echo "=========================================="
echo "Strict Search Isolation Tests"
echo "=========================================="
echo "Target: $BASE_URL"
echo "Date: $(date)"
echo "=========================================="
echo

PASSED=0
FAILED=0
CREATED_NOTE_IDS=""
CREATED_CONCEPT_IDS=""

pass() { echo "  ✓ $1"; ((PASSED++)) || true; }
fail() { echo "  ✗ $1"; ((FAILED++)) || true; }

cleanup() {
    echo
    echo "=== Cleanup ==="
    # Purge test notes
    for nid in $CREATED_NOTE_IDS; do
        curl -s -X POST "$BASE_URL/api/v1/notes/$nid/purge" -H "Content-Type: application/json" -d '{}' >/dev/null 2>&1 || true
    done
    # Delete test concepts
    for cid in $CREATED_CONCEPT_IDS; do
        curl -s -X DELETE "$BASE_URL/api/v1/concepts/$cid" >/dev/null 2>&1 || true
    done
    echo "  Cleanup complete"
}

trap cleanup EXIT

# ============================================
# 1. CREATE SKOS TAXONOMY FOR TEST
# ============================================
echo "=== 1. Creating SKOS Taxonomy for Test ==="

# Create Client A scheme
echo "  Creating Client A scheme..."
SCHEME_A_RESULT=$(curl -s -X POST "$BASE_URL/api/v1/concepts/schemes" \
    -H "Content-Type: application/json" \
    -d "{\"notation\":\"strict-client-a-$TIMESTAMP\",\"title\":\"Strict Test Client A $TIMESTAMP\",\"description\":\"Client A data isolation scheme\"}" 2>&1)
SCHEME_A=$(echo "$SCHEME_A_RESULT" | jq -r '.id // empty')

if [ -n "$SCHEME_A" ]; then
    echo "    Scheme A: $SCHEME_A"
else
    fail "Create Client A scheme"
    echo "    Response: $SCHEME_A_RESULT"
    exit 1
fi

# Create Client B scheme
echo "  Creating Client B scheme..."
SCHEME_B_RESULT=$(curl -s -X POST "$BASE_URL/api/v1/concepts/schemes" \
    -H "Content-Type: application/json" \
    -d "{\"notation\":\"strict-client-b-$TIMESTAMP\",\"title\":\"Strict Test Client B $TIMESTAMP\",\"description\":\"Client B data isolation scheme\"}" 2>&1)
SCHEME_B=$(echo "$SCHEME_B_RESULT" | jq -r '.id // empty')

if [ -n "$SCHEME_B" ]; then
    echo "    Scheme B: $SCHEME_B"
else
    fail "Create Client B scheme"
    exit 1
fi

# Create concepts in each scheme
echo "  Creating concepts..."
CONCEPT_A=$(curl -s -X POST "$BASE_URL/api/v1/concepts" \
    -H "Content-Type: application/json" \
    -d "{\"scheme_id\":\"$SCHEME_A\",\"pref_label\":\"Client A Data $TIMESTAMP\",\"notation\":\"client-a-data-$TIMESTAMP\"}" | jq -r '.id // empty')
CREATED_CONCEPT_IDS="$CONCEPT_A"
echo "    Concept A: $CONCEPT_A"

CONCEPT_B=$(curl -s -X POST "$BASE_URL/api/v1/concepts" \
    -H "Content-Type: application/json" \
    -d "{\"scheme_id\":\"$SCHEME_B\",\"pref_label\":\"Client B Data $TIMESTAMP\",\"notation\":\"client-b-data-$TIMESTAMP\"}" | jq -r '.id // empty')
CREATED_CONCEPT_IDS="$CREATED_CONCEPT_IDS $CONCEPT_B"
echo "    Concept B: $CONCEPT_B"

if [ -z "$CONCEPT_A" ] || [ -z "$CONCEPT_B" ]; then
    fail "Create concepts"
    exit 1
fi

echo

# ============================================
# 2. CREATE TEST NOTES WITH SKOS TAGS
# ============================================
echo "=== 2. Creating Test Notes ==="

# Create Client A notes
echo "  Creating Client A notes..."
NOTE_A1=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Strict isolation test - Client A confidential record 1 - $TIMESTAMP\",\"tags\":[],\"revision_mode\":\"none\"}" | jq -r '.id // empty')
CREATED_NOTE_IDS="$NOTE_A1"
echo "    Note A1: $NOTE_A1"

# Tag with SKOS concept (POST to /concepts with concept_id in body)
curl -s -X POST "$BASE_URL/api/v1/notes/$NOTE_A1/concepts" -H "Content-Type: application/json" -d "{\"concept_id\":\"$CONCEPT_A\"}" >/dev/null

NOTE_A2=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Strict isolation test - Client A public record 2 - $TIMESTAMP\",\"tags\":[],\"revision_mode\":\"none\"}" | jq -r '.id // empty')
CREATED_NOTE_IDS="$CREATED_NOTE_IDS $NOTE_A2"
echo "    Note A2: $NOTE_A2"
curl -s -X POST "$BASE_URL/api/v1/notes/$NOTE_A2/concepts" -H "Content-Type: application/json" -d "{\"concept_id\":\"$CONCEPT_A\"}" >/dev/null

# Create Client B notes
echo "  Creating Client B notes..."
NOTE_B1=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Strict isolation test - Client B confidential record 1 - $TIMESTAMP\",\"tags\":[],\"revision_mode\":\"none\"}" | jq -r '.id // empty')
CREATED_NOTE_IDS="$CREATED_NOTE_IDS $NOTE_B1"
echo "    Note B1: $NOTE_B1"
curl -s -X POST "$BASE_URL/api/v1/notes/$NOTE_B1/concepts" -H "Content-Type: application/json" -d "{\"concept_id\":\"$CONCEPT_B\"}" >/dev/null

NOTE_B2=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Strict isolation test - Client B public record 2 - $TIMESTAMP\",\"tags\":[],\"revision_mode\":\"none\"}" | jq -r '.id // empty')
CREATED_NOTE_IDS="$CREATED_NOTE_IDS $NOTE_B2"
echo "    Note B2: $NOTE_B2"
curl -s -X POST "$BASE_URL/api/v1/notes/$NOTE_B2/concepts" -H "Content-Type: application/json" -d "{\"concept_id\":\"$CONCEPT_B\"}" >/dev/null

# Create untagged note
echo "  Creating untagged note..."
NOTE_UNTAGGED=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{\"content\":\"Strict isolation test - Untagged system note - $TIMESTAMP\",\"tags\":[],\"revision_mode\":\"none\"}" | jq -r '.id // empty')
CREATED_NOTE_IDS="$CREATED_NOTE_IDS $NOTE_UNTAGGED"
echo "    Untagged: $NOTE_UNTAGGED"

# Wait for embeddings
echo "  Waiting for embeddings..."
sleep 5

echo
echo "  Test Data Summary:"
echo "    Scheme A: $SCHEME_A (notation: strict-client-a-$TIMESTAMP)"
echo "    Scheme B: $SCHEME_B (notation: strict-client-b-$TIMESTAMP)"
echo "    Client A: 2 notes ($NOTE_A1, $NOTE_A2)"
echo "    Client B: 2 notes ($NOTE_B1, $NOTE_B2)"
echo "    Untagged: 1 note  ($NOTE_UNTAGGED)"
echo "    Total:    5 notes"
echo

# ============================================
# 3. TEST REQUIRED_SCHEMES (Scheme Isolation)
# ============================================
echo "=== 3. Required Schemes (Scheme Isolation) ==="

# URL-encode the filter
FILTER_A="{\"required_schemes\":[\"strict-client-a-$TIMESTAMP\"]}"
ENCODED_A=$(echo -n "$FILTER_A" | jq -sRr @uri)

# Search with required_schemes: Client A only
RESULT_A=$(curl -s "http://localhost:3000/api/v1/search?q=Strict%20isolation%20test&limit=20&strict_filter=$ENCODED_A" 2>&1)
COUNT_A=$(echo "$RESULT_A" | jq '.total // 0')

if [ "$COUNT_A" = "2" ]; then
    pass "required_schemes [Client A] returns 2 notes"
else
    fail "required_schemes [Client A] expected 2, got $COUNT_A"
    echo "    Filter: $FILTER_A"
    echo "    Response: $(echo "$RESULT_A" | jq -c '.error // .total')"
fi

# Verify correct notes returned
HAS_A1=$(echo "$RESULT_A" | jq --arg id "$NOTE_A1" '[.results[]? | select(.note_id == $id)] | length')
HAS_A2=$(echo "$RESULT_A" | jq --arg id "$NOTE_A2" '[.results[]? | select(.note_id == $id)] | length')
HAS_B1=$(echo "$RESULT_A" | jq --arg id "$NOTE_B1" '[.results[]? | select(.note_id == $id)] | length')
HAS_B2=$(echo "$RESULT_A" | jq --arg id "$NOTE_B2" '[.results[]? | select(.note_id == $id)] | length')

if [ "$HAS_A1" = "1" ] && [ "$HAS_A2" = "1" ]; then
    pass "Client A notes present in results"
else
    fail "Client A notes missing (A1=$HAS_A1, A2=$HAS_A2)"
fi

if [ "$HAS_B1" = "0" ] && [ "$HAS_B2" = "0" ]; then
    pass "Client B notes excluded from results (isolation verified)"
else
    fail "Client B notes leaked! (B1=$HAS_B1, B2=$HAS_B2) - DATA ISOLATION FAILURE"
fi

echo

# ============================================
# 4. TEST EXCLUDED_SCHEMES
# ============================================
echo "=== 4. Excluded Schemes ==="

# Search excluding Client A - verify by note IDs, not count (may include leftover test data)
FILTER_EXCL="{\"excluded_schemes\":[\"strict-client-a-$TIMESTAMP\"],\"include_untagged\":false}"
ENCODED_EXCL=$(echo -n "$FILTER_EXCL" | jq -sRr @uri)

RESULT_EXCL=$(curl -s "http://localhost:3000/api/v1/search?q=Strict%20isolation%20test&limit=20&strict_filter=$ENCODED_EXCL" 2>&1)

# Verify Client A notes are EXCLUDED
HAS_A1_EXCL=$(echo "$RESULT_EXCL" | jq --arg id "$NOTE_A1" '[.results[]? | select(.note_id == $id)] | length')
HAS_A2_EXCL=$(echo "$RESULT_EXCL" | jq --arg id "$NOTE_A2" '[.results[]? | select(.note_id == $id)] | length')

if [ "$HAS_A1_EXCL" = "0" ] && [ "$HAS_A2_EXCL" = "0" ]; then
    pass "Client A notes correctly excluded from results"
else
    fail "Client A notes leaked in exclusion test (A1=$HAS_A1_EXCL, A2=$HAS_A2_EXCL)"
fi

# Verify Client B notes are PRESENT
HAS_B1_EXCL=$(echo "$RESULT_EXCL" | jq --arg id "$NOTE_B1" '[.results[]? | select(.note_id == $id)] | length')
HAS_B2_EXCL=$(echo "$RESULT_EXCL" | jq --arg id "$NOTE_B2" '[.results[]? | select(.note_id == $id)] | length')

if [ "$HAS_B1_EXCL" = "1" ] && [ "$HAS_B2_EXCL" = "1" ]; then
    pass "Client B notes present in results (exclusion working)"
else
    fail "Client B notes missing from exclusion test (B1=$HAS_B1_EXCL, B2=$HAS_B2_EXCL)"
fi

echo

# ============================================
# 5. TEST REQUIRED_TAGS (Concept Isolation)
# ============================================
echo "=== 5. Required Tags (Concept Isolation) ==="

# Test with concept notation
FILTER_TAG="{\"required_tags\":[\"client-a-data-$TIMESTAMP\"]}"
ENCODED_TAG=$(echo -n "$FILTER_TAG" | jq -sRr @uri)

RESULT_TAG=$(curl -s "http://localhost:3000/api/v1/search?q=Strict%20isolation%20test&limit=20&strict_filter=$ENCODED_TAG" 2>&1)
COUNT_TAG=$(echo "$RESULT_TAG" | jq '.total // 0')

if [ "$COUNT_TAG" = "2" ]; then
    pass "required_tags [client-a-data] returns 2 notes"
else
    fail "required_tags expected 2, got $COUNT_TAG"
    echo "    Response: $(echo "$RESULT_TAG" | jq -c '.error // .total')"
fi

echo

# ============================================
# 6. VERIFY NO DATA LEAKAGE
# ============================================
echo "=== 6. Data Leakage Verification ==="

# Client A searches should NEVER see Client B data (by note ID, not result count)
# Semantic search may return Client A notes that are similar to "Client B" query
FILTER_LEAK="{\"required_schemes\":[\"strict-client-a-$TIMESTAMP\"]}"
ENCODED_LEAK=$(echo -n "$FILTER_LEAK" | jq -sRr @uri)

RESULT_LEAK=$(curl -s "http://localhost:3000/api/v1/search?q=Client%20B&limit=20&strict_filter=$ENCODED_LEAK" 2>&1)

# Check that no Client B note IDs appear in results (true isolation test)
HAS_B1_LEAK=$(echo "$RESULT_LEAK" | jq --arg id "$NOTE_B1" '[.results[]? | select(.note_id == $id)] | length')
HAS_B2_LEAK=$(echo "$RESULT_LEAK" | jq --arg id "$NOTE_B2" '[.results[]? | select(.note_id == $id)] | length')

if [ "$HAS_B1_LEAK" = "0" ] && [ "$HAS_B2_LEAK" = "0" ]; then
    pass "No Client B notes visible in Client A search (isolation verified by note ID)"
else
    fail "DATA LEAKAGE DETECTED: Client B notes found in Client A results (B1=$HAS_B1_LEAK, B2=$HAS_B2_LEAK)"
fi

echo

# ============================================
# SUMMARY
# ============================================
echo "=========================================="
echo "Strict Search Isolation Test Results"
echo "=========================================="
echo "Passed:  $PASSED"
echo "Failed:  $FAILED"
echo "=========================================="

if [ "$FAILED" -gt 0 ]; then
    echo "ISOLATION TESTS FAILED - DATA LEAKAGE POSSIBLE"
    exit 1
else
    echo "ALL ISOLATION TESTS PASSED"
    exit 0
fi
