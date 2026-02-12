#!/usr/bin/env bash
# container-api-tests.sh - Comprehensive API test suite for Fortémi container
#
# Validates the full API surface of a running Fortémi instance.
# Designed for both CI (DinD with docker exec) and local testing.
#
# Environment variables:
#   API_BASE    - Base URL for API requests (default: http://localhost:3000)
#   CURL_CMD    - Curl command prefix (default: curl)
#                 In CI/DinD: "docker exec fortemi-test-api curl"
#   VERBOSE     - Set to 1 for verbose output (default: 0)
#
# Usage:
#   # Local testing against running instance
#   bash scripts/container-api-tests.sh
#
#   # CI with Docker-in-Docker
#   CURL_CMD="docker exec fortemi-test-api curl" bash scripts/container-api-tests.sh
#
#   # Verbose mode
#   VERBOSE=1 bash scripts/container-api-tests.sh

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
API_BASE="${API_BASE:-http://localhost:3000}"
CURL_CMD="${CURL_CMD:-curl}"
VERBOSE="${VERBOSE:-0}"

# Counters
PASS=0
FAIL=0
TOTAL=0

# Stored IDs for cross-test references
NOTE_ID=""
NOTE_ID_2=""
COLLECTION_ID=""
TEMPLATE_ID=""
TEMPLATE_NOTE_ID=""
BULK_NOTE_IDS=""

# Zero UUID for 404 tests
ZERO_UUID="00000000-0000-0000-0000-000000000000"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Make an API call: api_call METHOD PATH [DATA]
# Sets global: RESPONSE_STATUS, RESPONSE_BODY
api_call() {
  local method="$1"
  local path="$2"
  local data="${3:-}"
  local url="${API_BASE}${path}"

  local tmpfile
  tmpfile=$(mktemp)

  local curl_args=(-s -w "\n%{http_code}" -X "$method")
  curl_args+=(-H "Content-Type: application/json")

  if [[ -n "$data" ]]; then
    curl_args+=(-d "$data")
  fi

  curl_args+=("$url")

  # Run curl (may be "docker exec ... curl")
  local raw
  raw=$($CURL_CMD "${curl_args[@]}" 2>/dev/null) || true

  # Last line is status code, everything before is body
  RESPONSE_STATUS=$(echo "$raw" | tail -n1)
  RESPONSE_BODY=$(echo "$raw" | sed '$d')

  if [[ "$VERBOSE" == "1" ]]; then
    echo "  >> $method $path -> $RESPONSE_STATUS"
    if [[ -n "$RESPONSE_BODY" ]]; then
      echo "$RESPONSE_BODY" | head -5
    fi
  fi

  rm -f "$tmpfile"
}

# Assert HTTP status code
# assert_status TEST_NAME EXPECTED_STATUS
assert_status() {
  local name="$1"
  local expected="$2"
  TOTAL=$((TOTAL + 1))

  if [[ "$RESPONSE_STATUS" == "$expected" ]]; then
    echo "  [PASS] $name (HTTP $expected)"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] $name - expected HTTP $expected, got HTTP $RESPONSE_STATUS"
    if [[ -n "$RESPONSE_BODY" ]]; then
      echo "         Body: $(echo "$RESPONSE_BODY" | head -3)"
    fi
    FAIL=$((FAIL + 1))
  fi
}

# Assert JSON field exists and optionally matches value
# assert_json_field TEST_NAME JQ_EXPRESSION [EXPECTED_VALUE]
assert_json_field() {
  local name="$1"
  local jq_expr="$2"
  local expected="${3:-}"
  TOTAL=$((TOTAL + 1))

  local actual
  actual=$(echo "$RESPONSE_BODY" | jq -r "$jq_expr" 2>/dev/null) || actual="<jq_error>"

  if [[ "$actual" == "<jq_error>" ]] || [[ "$actual" == "null" && -z "$expected" ]]; then
    echo "  [FAIL] $name - field $jq_expr not found or null"
    if [[ -n "$RESPONSE_BODY" ]]; then
      echo "         Body: $(echo "$RESPONSE_BODY" | head -3)"
    fi
    FAIL=$((FAIL + 1))
    return
  fi

  if [[ -n "$expected" ]]; then
    if [[ "$actual" == "$expected" ]]; then
      echo "  [PASS] $name ($jq_expr == $expected)"
      PASS=$((PASS + 1))
    else
      echo "  [FAIL] $name - expected $jq_expr == '$expected', got '$actual'"
      FAIL=$((FAIL + 1))
    fi
  else
    echo "  [PASS] $name ($jq_expr present)"
    PASS=$((PASS + 1))
  fi
}

# Assert response body is a JSON array
assert_is_array() {
  local name="$1"
  TOTAL=$((TOTAL + 1))

  local is_arr
  is_arr=$(echo "$RESPONSE_BODY" | jq 'if type == "array" then "yes" else "no" end' -r 2>/dev/null) || is_arr="no"

  if [[ "$is_arr" == "yes" ]]; then
    echo "  [PASS] $name (is array)"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] $name - expected JSON array"
    FAIL=$((FAIL + 1))
  fi
}

section() {
  echo ""
  echo "======================================================================"
  echo "  $1"
  echo "======================================================================"
}

# ---------------------------------------------------------------------------
# 1. Health & Meta
# ---------------------------------------------------------------------------
section "1. Health & Meta"

api_call GET "/health"
assert_status "GET /health" "200"
assert_json_field "Health status field" ".status" "healthy"

api_call GET "/api/v1/memory/info"
assert_status "GET /memory/info" "200"

api_call GET "/api/v1/rate-limit/status"
assert_status "GET /rate-limit/status" "200"

# ---------------------------------------------------------------------------
# 2. Notes CRUD
# ---------------------------------------------------------------------------
section "2. Notes CRUD"

# Create a note (revision_mode: none to skip Ollama)
api_call POST "/api/v1/notes" '{"content":"Container test note - automated CI","revision_mode":"none"}'
assert_status "POST /notes (create)" "201"
assert_json_field "Create returns id" ".id"
NOTE_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
echo "  -> Created note: $NOTE_ID"

# Create a second note for relation/link tests
api_call POST "/api/v1/notes" '{"content":"Second test note for linking","revision_mode":"none"}'
assert_status "POST /notes (create second)" "201"
NOTE_ID_2=$(echo "$RESPONSE_BODY" | jq -r '.id')
echo "  -> Created note 2: $NOTE_ID_2"

# List notes
api_call GET "/api/v1/notes"
assert_status "GET /notes (list)" "200"
assert_json_field "List has total field" ".total"

# Get single note
api_call GET "/api/v1/notes/$NOTE_ID"
assert_status "GET /notes/:id" "200"
assert_json_field "Note has id" ".note.id"

# Update note (returns full note object as of v2026.1.6)
api_call PATCH "/api/v1/notes/$NOTE_ID" '{"content":"Updated CI test note","revision_mode":"none"}'
assert_status "PATCH /notes/:id (update)" "200"

# ---------------------------------------------------------------------------
# 3. Note Tags
# ---------------------------------------------------------------------------
section "3. Note Tags"

api_call PUT "/api/v1/notes/$NOTE_ID/tags" '{"tags":["ci-test","container","automated"]}'
assert_status "PUT /notes/:id/tags" "204"

api_call GET "/api/v1/notes/$NOTE_ID/tags"
assert_status "GET /notes/:id/tags" "200"

# ---------------------------------------------------------------------------
# 4. Note Relations (Links & Backlinks)
# ---------------------------------------------------------------------------
section "4. Note Relations"

api_call GET "/api/v1/notes/$NOTE_ID/links"
assert_status "GET /notes/:id/links" "200"

api_call GET "/api/v1/notes/$NOTE_ID/backlinks"
assert_status "GET /notes/:id/backlinks" "200"

# ---------------------------------------------------------------------------
# 5. Note Export / Versions / Provenance
# ---------------------------------------------------------------------------
section "5. Note Export, Versions, Provenance"

api_call GET "/api/v1/notes/$NOTE_ID/export"
assert_status "GET /notes/:id/export" "200"

api_call GET "/api/v1/notes/$NOTE_ID/full"
assert_status "GET /notes/:id/full" "200"

api_call GET "/api/v1/notes/$NOTE_ID/versions"
assert_status "GET /notes/:id/versions" "200"

api_call GET "/api/v1/notes/$NOTE_ID/provenance"
assert_status "GET /notes/:id/provenance" "200"

# ---------------------------------------------------------------------------
# 6. Note Status & Lifecycle
# ---------------------------------------------------------------------------
section "6. Note Status & Lifecycle"

# Star the note
api_call PATCH "/api/v1/notes/$NOTE_ID/status" '{"starred":true}'
assert_status "PATCH /notes/:id/status (star)" "204"

# Soft delete
api_call DELETE "/api/v1/notes/$NOTE_ID_2"
assert_status "DELETE /notes/:id (soft delete)" "204"

# Restore from trash
api_call POST "/api/v1/notes/$NOTE_ID_2/restore"
assert_status "POST /notes/:id/restore" "200"

# Re-delete then purge
api_call DELETE "/api/v1/notes/$NOTE_ID_2"
assert_status "DELETE /notes/:id (re-delete for purge)" "204"

api_call POST "/api/v1/notes/$NOTE_ID_2/purge"
assert_status "POST /notes/:id/purge" "200"
assert_json_field "Purge returns status" ".status" "queued"

# ---------------------------------------------------------------------------
# 7. Bulk Notes
# ---------------------------------------------------------------------------
section "7. Bulk Notes"

api_call POST "/api/v1/notes/bulk" '{"notes":[{"content":"Bulk note 1","revision_mode":"none"},{"content":"Bulk note 2","revision_mode":"none"},{"content":"Bulk note 3","revision_mode":"none"}]}'
assert_status "POST /notes/bulk" "201"
assert_json_field "Bulk returns count" ".count" "3"
BULK_NOTE_IDS=$(echo "$RESPONSE_BODY" | jq -r '.ids[]' 2>/dev/null) || true

# ---------------------------------------------------------------------------
# 8. Collections
# ---------------------------------------------------------------------------
section "8. Collections"

# Create collection
api_call POST "/api/v1/collections" '{"name":"CI Test Collection","description":"Created by container tests"}'
assert_status "POST /collections (create)" "201"
assert_json_field "Collection returns id" ".id"
COLLECTION_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
echo "  -> Created collection: $COLLECTION_ID"

# List collections
api_call GET "/api/v1/collections"
assert_status "GET /collections (list)" "200"

# Get collection
api_call GET "/api/v1/collections/$COLLECTION_ID"
assert_status "GET /collections/:id" "200"
assert_json_field "Collection has name" ".name" "CI Test Collection"

# Update collection
api_call PATCH "/api/v1/collections/$COLLECTION_ID" '{"name":"Updated CI Collection","description":"Updated by tests"}'
assert_status "PATCH /collections/:id" "204"

# Move note into collection
api_call POST "/api/v1/notes/$NOTE_ID/move" "{\"collection_id\":\"$COLLECTION_ID\"}"
assert_status "POST /notes/:id/move (to collection)" "204"

# List notes in collection
api_call GET "/api/v1/collections/$COLLECTION_ID/notes"
assert_status "GET /collections/:id/notes" "200"

# Delete collection (force=true because we moved a note into it)
api_call DELETE "/api/v1/collections/$COLLECTION_ID?force=true"
assert_status "DELETE /collections/:id (force)" "204"

# ---------------------------------------------------------------------------
# 9. Templates
# ---------------------------------------------------------------------------
section "9. Templates"

# Create template
api_call POST "/api/v1/templates" '{"name":"CI Test Template","content":"Template content: {{title}}","description":"Test template"}'
assert_status "POST /templates (create)" "201"
assert_json_field "Template returns id" ".id"
TEMPLATE_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
echo "  -> Created template: $TEMPLATE_ID"

# List templates
api_call GET "/api/v1/templates"
assert_status "GET /templates (list)" "200"

# Get template
api_call GET "/api/v1/templates/$TEMPLATE_ID"
assert_status "GET /templates/:id" "200"
assert_json_field "Template has name" ".name" "CI Test Template"

# Instantiate template as note
api_call POST "/api/v1/templates/$TEMPLATE_ID/instantiate" '{}'
assert_status "POST /templates/:id/instantiate" "201"
assert_json_field "Instantiate returns note id" ".id"
TEMPLATE_NOTE_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
echo "  -> Instantiated note: $TEMPLATE_NOTE_ID"

# Delete template
api_call DELETE "/api/v1/templates/$TEMPLATE_ID"
assert_status "DELETE /templates/:id" "204"

# ---------------------------------------------------------------------------
# 10. Search & Tags
# ---------------------------------------------------------------------------
section "10. Search & Tags"

# FTS search (works without Ollama)
api_call GET "/api/v1/search?q=container+test&mode=fts"
assert_status "GET /search (FTS mode)" "200"
assert_json_field "Search has results field" ".results"

# List legacy tags
api_call GET "/api/v1/tags"
assert_status "GET /tags" "200"

# Timeline
api_call GET "/api/v1/notes/timeline"
assert_status "GET /notes/timeline" "200"

# Activity
api_call GET "/api/v1/notes/activity"
assert_status "GET /notes/activity" "200"

# ---------------------------------------------------------------------------
# 11. Knowledge Health
# ---------------------------------------------------------------------------
section "11. Knowledge Health"

api_call GET "/api/v1/health/knowledge"
assert_status "GET /health/knowledge" "200"

api_call GET "/api/v1/health/orphan-tags"
assert_status "GET /health/orphan-tags" "200"

api_call GET "/api/v1/health/stale-notes"
assert_status "GET /health/stale-notes" "200"

api_call GET "/api/v1/health/unlinked-notes"
assert_status "GET /health/unlinked-notes" "200"

# ---------------------------------------------------------------------------
# 12. Jobs
# ---------------------------------------------------------------------------
section "12. Jobs"

api_call GET "/api/v1/jobs"
assert_status "GET /jobs (list)" "200"
assert_json_field "Jobs has jobs field" ".jobs"

api_call GET "/api/v1/jobs/pending"
assert_status "GET /jobs/pending" "200"

api_call GET "/api/v1/jobs/stats"
assert_status "GET /jobs/stats" "200"

# ---------------------------------------------------------------------------
# 13. Error Handling
# ---------------------------------------------------------------------------
section "13. Error Handling"

# 404 - non-existent note
api_call GET "/api/v1/notes/$ZERO_UUID"
assert_status "GET /notes/<zero-uuid> (404)" "404"

# 400/422 - malformed create request (missing required content field)
api_call POST "/api/v1/notes" '{"not_a_field":"bad"}'
# Accept either 400 or 422 as valid error response
TOTAL=$((TOTAL + 1))
if [[ "$RESPONSE_STATUS" == "400" ]] || [[ "$RESPONSE_STATUS" == "422" ]]; then
  echo "  [PASS] POST /notes malformed body (HTTP $RESPONSE_STATUS)"
  PASS=$((PASS + 1))
else
  echo "  [FAIL] POST /notes malformed body - expected HTTP 400 or 422, got HTTP $RESPONSE_STATUS"
  FAIL=$((FAIL + 1))
fi

# 404 - non-existent collection
api_call GET "/api/v1/collections/$ZERO_UUID"
assert_status "GET /collections/<zero-uuid> (404)" "404"

# ---------------------------------------------------------------------------
# Cleanup: delete remaining test notes
# ---------------------------------------------------------------------------
section "Cleanup"

# Delete the main test note
api_call DELETE "/api/v1/notes/$NOTE_ID"
if [[ "$RESPONSE_STATUS" == "204" ]]; then
  echo "  Cleaned up note $NOTE_ID"
else
  echo "  Warning: failed to clean up note $NOTE_ID (HTTP $RESPONSE_STATUS)"
fi

# Delete template-instantiated note
if [[ -n "$TEMPLATE_NOTE_ID" ]]; then
  api_call DELETE "/api/v1/notes/$TEMPLATE_NOTE_ID"
  if [[ "$RESPONSE_STATUS" == "204" ]]; then
    echo "  Cleaned up template note $TEMPLATE_NOTE_ID"
  else
    echo "  Warning: failed to clean up template note (HTTP $RESPONSE_STATUS)"
  fi
fi

# Delete bulk-created notes
for bid in $BULK_NOTE_IDS; do
  api_call DELETE "/api/v1/notes/$bid"
  if [[ "$RESPONSE_STATUS" == "204" ]]; then
    echo "  Cleaned up bulk note $bid"
  fi
done

echo "  Cleanup complete."

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "======================================================================"
echo "  FORTÉMI CONTAINER API TEST RESULTS"
echo "======================================================================"
echo ""
echo "  Total:  $TOTAL"
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [[ "$FAIL" -gt 0 ]]; then
  echo "  RESULT: FAILED ($FAIL failures)"
  echo "======================================================================"
  exit 1
else
  echo "  RESULT: ALL TESTS PASSED"
  echo "======================================================================"
  exit 0
fi
