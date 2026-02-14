#!/usr/bin/env bash
#
# Run all MCP integration tests sequentially to avoid PostgreSQL deadlocks.
#
# Usage:
#   FORTEMI_API_KEY="mm_at_..." ./mcp-server/tests/run-all.sh
#
# The tests MUST be run sequentially because concurrent MCP sessions
# cause PostgreSQL deadlocks due to shared database state.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -z "${FORTEMI_API_KEY:-}" ]; then
  echo "ERROR: FORTEMI_API_KEY must be set"
  echo "Get a token: curl -sf -X POST http://localhost:3000/oauth/token \\"
  echo "  -u 'CLIENT_ID:CLIENT_SECRET' -d 'grant_type=client_credentials&scope=mcp read write'"
  exit 1
fi

# Test files in execution order (matches UAT phase order)
TEST_FILES=(
  # Static analysis (no MCP connection needed)
  "schema-validation.test.js"
  "branding.test.js"
  "error-responses.test.js"

  # Foundation
  "preflight.test.js"
  "seed-data.test.js"
  "crud.test.js"
  "attachments.test.js"
  "vision.test.js"

  # Search
  "search.test.js"
  "memory-search.test.js"

  # Core features
  "tags.test.js"
  "collections.test.js"
  "links.test.js"
  "embeddings.test.js"
  "document-types.test.js"
  "edge-cases.test.js"

  # Advanced features
  "templates.test.js"
  "versioning.test.js"
  "archives.test.js"
  "skos.test.js"
  "pke.test.js"
  "jobs.test.js"
  "observability.test.js"
  "memories.test.js"

  # Auth & integration
  "oauth.test.js"
  "api-management.test.js"

  # Consolidated tool surface (issue #365)
  "consolidated-tools.test.js"

  # E2E & export
  "feature-chains.test.js"
  "data-export.test.js"
  "annotations.test.js"

  # Cleanup
  "cleanup.test.js"
)

TOTAL=0
PASSED=0
FAILED=0
FAILED_FILES=()

echo "=== MCP Integration Test Suite ==="
echo "Running ${#TEST_FILES[@]} test files sequentially"
echo ""

for file in "${TEST_FILES[@]}"; do
  filepath="$SCRIPT_DIR/$file"
  if [ ! -f "$filepath" ]; then
    echo "SKIP  $file (not found)"
    continue
  fi

  printf "%-45s " "$file"

  output=$(FORTEMI_API_KEY="$FORTEMI_API_KEY" node --test "$filepath" 2>&1)
  exit_code=$?

  # Extract pass/fail counts from output
  tests=$(echo "$output" | grep -oP '(?<=ℹ tests )\d+' | tail -1 || echo "0")
  pass=$(echo "$output" | grep -oP '(?<=ℹ pass )\d+' | tail -1 || echo "0")
  fail=$(echo "$output" | grep -oP '(?<=ℹ fail )\d+' | tail -1 || echo "0")

  TOTAL=$((TOTAL + tests))
  PASSED=$((PASSED + pass))
  FAILED=$((FAILED + fail))

  if [ "$exit_code" -eq 0 ] && [ "$fail" = "0" ]; then
    echo "PASS  ($pass/$tests)"
  else
    echo "FAIL  ($pass/$tests, $fail failed)"
    FAILED_FILES+=("$file")
  fi
done

echo ""
echo "=== Summary ==="
echo "Files:  ${#TEST_FILES[@]}"
echo "Tests:  $TOTAL"
echo "Passed: $PASSED"
echo "Failed: $FAILED"

if [ ${#FAILED_FILES[@]} -gt 0 ]; then
  echo ""
  echo "Failed files:"
  for f in "${FAILED_FILES[@]}"; do
    echo "  - $f"
  done
  exit 1
fi

echo ""
echo "All tests passed!"
exit 0
