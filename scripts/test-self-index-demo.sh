#!/bin/bash
# Test suite for self-index-demo.sh
# Validates script structure, dependencies, and behavior

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEMO_SCRIPT="${SCRIPT_DIR}/self-index-demo.sh"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "=== Self-Index Demo Test Suite ==="
echo ""

# Test 1: Script exists and is executable
echo "Test 1: Script exists and is executable"
if [ ! -f "$DEMO_SCRIPT" ]; then
  echo "  FAIL: Script not found at $DEMO_SCRIPT"
  exit 1
fi

if [ ! -x "$DEMO_SCRIPT" ]; then
  echo "  FAIL: Script is not executable"
  exit 1
fi
echo "  PASS"
echo ""

# Test 2: Required commands are available
echo "Test 2: Required commands are available"
missing_commands=()

for cmd in curl jq cat basename; do
  if ! command -v "$cmd" &> /dev/null; then
    missing_commands+=("$cmd")
  fi
done

if [ ${#missing_commands[@]} -ne 0 ]; then
  echo "  FAIL: Missing required commands: ${missing_commands[*]}"
  echo "  Install with: apt-get install curl jq (or equivalent)"
  exit 1
fi
echo "  PASS"
echo ""

# Test 3: Script has proper error handling
echo "Test 3: Script has proper error handling"
if ! grep -q "set -e" "$DEMO_SCRIPT"; then
  echo "  FAIL: Script missing 'set -e' for error handling"
  exit 1
fi
echo "  PASS"
echo ""

# Test 4: Source files exist
echo "Test 4: Source files to index exist"
cd "$REPO_ROOT"

# Check for Rust files
rust_files=$(find crates/matric-core/src -name "*.rs" 2>/dev/null | wc -l)
if [ "$rust_files" -eq 0 ]; then
  echo "  FAIL: No Rust files found in crates/matric-core/src"
  exit 1
fi
echo "  Found $rust_files Rust files in matric-core"

# Check for TypeScript files
ts_files=$(find mcp-server -name "*.ts" 2>/dev/null | wc -l)
if [ "$ts_files" -eq 0 ]; then
  echo "  WARN: No TypeScript files found in mcp-server"
else
  echo "  Found $ts_files TypeScript files in mcp-server"
fi

# Check for SQL migrations
sql_files=$(find migrations -name "*.sql" 2>/dev/null | wc -l)
if [ "$sql_files" -eq 0 ]; then
  echo "  WARN: No SQL files found in migrations"
else
  echo "  Found $sql_files SQL migration files"
fi

# Check for documentation
if [ ! -f "docs/content/architecture.md" ]; then
  echo "  FAIL: Required documentation file not found: docs/content/architecture.md"
  exit 1
fi
echo "  PASS"
echo ""

# Test 5: Script uses correct API endpoints
echo "Test 5: Script uses correct API endpoints"
if ! grep -q "/api/v1/collections" "$DEMO_SCRIPT"; then
  echo "  FAIL: Script not using /api/v1/collections endpoint"
  exit 1
fi

if ! grep -q "/api/v1/notes" "$DEMO_SCRIPT"; then
  echo "  FAIL: Script not using /api/v1/notes endpoint"
  exit 1
fi

if ! grep -q "/api/v1/search" "$DEMO_SCRIPT"; then
  echo "  FAIL: Script not using /api/v1/search endpoint"
  exit 1
fi
echo "  PASS"
echo ""

# Test 6: Script uses proper document formats
echo "Test 6: Script uses proper document formats"
for format in rust typescript sql markdown; do
  # Handle escaped quotes in the script
  if ! grep -q "format.*$format" "$DEMO_SCRIPT"; then
    echo "  FAIL: Script missing format: $format"
    exit 1
  fi
done
echo "  PASS"
echo ""

# Test 7: Script includes proper tags
echo "Test 7: Script includes proper tags"
for tag in rust typescript sql documentation source-code mcp-server migration; do
  if ! grep -q "$tag" "$DEMO_SCRIPT"; then
    echo "  FAIL: Script missing tag: $tag"
    exit 1
  fi
done
echo "  PASS"
echo ""

# Test 8: Script has demo queries
echo "Test 8: Script has semantic search demo queries"
query_count=$(grep -c "/api/v1/search?q=" "$DEMO_SCRIPT" || true)
if [ "$query_count" -lt 4 ]; then
  echo "  FAIL: Expected at least 4 demo search queries, found $query_count"
  exit 1
fi
echo "  Found $query_count demo search queries"
echo "  PASS"
echo ""

# Test 9: Script handles API_URL environment variable
echo "Test 9: Script handles API_URL environment variable"
if ! grep -q 'MATRIC_API_URL:-http://localhost:3000' "$DEMO_SCRIPT"; then
  echo "  FAIL: Script doesn't properly handle MATRIC_API_URL with default"
  exit 1
fi
echo "  PASS"
echo ""

# Test 10: Script syntax is valid
echo "Test 10: Script syntax is valid"
if ! bash -n "$DEMO_SCRIPT"; then
  echo "  FAIL: Script has syntax errors"
  exit 1
fi
echo "  PASS"
echo ""

# Test 11: Dry run (syntax check without execution)
echo "Test 11: Dry run validation"
# Extract and validate JSON payloads
json_count=$(grep -o 'content.*jq' "$DEMO_SCRIPT" | wc -l)
if [ "$json_count" -eq 0 ]; then
  echo "  FAIL: No JSON payloads found in script"
  exit 1
fi
echo "  Found $json_count content processing operations"
echo "  PASS"
echo ""

# Test 12: Documentation exists
echo "Test 12: Documentation exists"
DOC_FILE="${REPO_ROOT}/docs/content/self-maintenance.md"
if [ ! -f "$DOC_FILE" ]; then
  echo "  FAIL: Documentation file not found: $DOC_FILE"
  exit 1
fi

# Check documentation has required sections
for section in "Overview" "Quick Start" "Use Cases" "Document Types" "How It Works"; do
  if ! grep -q "## $section" "$DOC_FILE"; then
    echo "  FAIL: Documentation missing section: $section"
    exit 1
  fi
done
echo "  PASS"
echo ""

# All tests passed
echo "==================================="
echo "All tests passed!"
echo "==================================="
echo ""
echo "To run the demo (requires API server running):"
echo "  export MATRIC_API_URL=http://localhost:3000"
echo "  $DEMO_SCRIPT"
echo ""
