#!/bin/bash
# Production Deployment Test Script for Matric Memory
# Run after any deployment or system change
#
# Usage: ./scripts/production-test.sh [BASE_URL]
# Example: ./scripts/production-test.sh http://localhost:3000

set -e
export PGPASSWORD=matric
BASE_URL="${1:-${MATRIC_API_URL:-http://localhost:3000}}"

echo "========================================"
echo "Matric Memory Production Validation"
echo "========================================"
echo "Target: $BASE_URL"
echo "Date: $(date)"
echo "========================================"
echo

PASSED=0
FAILED=0
WARNINGS=0

pass() { echo "✓ $1"; ((PASSED++)) || true; }
fail() { echo "✗ $1"; ((FAILED++)) || true; }
warn() { echo "⚠ $1"; ((WARNINGS++)) || true; }

# 1. Infrastructure
echo "=== 1. Infrastructure Health ==="
systemctl is-active matric-api >/dev/null 2>&1 && pass "API service active" || fail "API service not active"
systemctl is-active postgresql >/dev/null 2>&1 && pass "PostgreSQL active" || fail "PostgreSQL not active"
curl -sf "$BASE_URL/health" >/dev/null && pass "Health endpoint" || fail "Health endpoint"
echo

# 2. Database
echo "=== 2. Database Validation ==="
psql -U matric -h localhost -d matric -c "SELECT 1;" >/dev/null 2>&1 && pass "DB connection" || fail "DB connection"
psql -U matric -h localhost -d matric -c "SELECT extname FROM pg_extension WHERE extname='vector';" 2>/dev/null | grep -q vector && pass "pgvector extension" || fail "pgvector extension"
psql -U matric -h localhost -d matric -c "SELECT id FROM skos_concept_scheme WHERE is_system = TRUE AND notation = 'default';" 2>/dev/null | grep -q "[a-f0-9-]" && pass "Default SKOS scheme" || fail "Default SKOS scheme"
psql -U matric -h localhost -d matric -c "SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default';" 2>/dev/null | grep -q "[a-f0-9-]" && pass "Default embedding set" || fail "Default embedding set"
psql -U matric -h localhost -d matric -c "SELECT id FROM embedding_config WHERE is_default = TRUE;" 2>/dev/null | grep -q "[a-f0-9-]" && pass "Default embedding config" || fail "Default embedding config"
echo

# 3. API Endpoints
echo "=== 3. API Endpoints ==="
curl -sf "$BASE_URL/notes?limit=1" | jq -e '.notes' >/dev/null 2>&1 && pass "GET /notes" || fail "GET /notes"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"test","limit":1}' 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "POST /search" || fail "POST /search"
curl -sf "$BASE_URL/collections" | jq -e '.' >/dev/null 2>&1 && pass "GET /collections" || fail "GET /collections"
curl -sf "$BASE_URL/tags" | jq -e '.' >/dev/null 2>&1 && pass "GET /tags" || fail "GET /tags"
curl -sf "$BASE_URL/embedding-sets" | jq -e '.' >/dev/null 2>&1 && pass "GET /embedding-sets" || fail "GET /embedding-sets"
curl -sf "$BASE_URL/templates" | jq -e '.' >/dev/null 2>&1 && pass "GET /templates" || fail "GET /templates"
curl -sf "$BASE_URL/queue/stats" | jq -e '.' >/dev/null 2>&1 && pass "GET /queue/stats" || fail "GET /queue/stats"
curl -sf "$BASE_URL/backup/status" | jq -e '.' >/dev/null 2>&1 && pass "GET /backup/status" || fail "GET /backup/status"
echo

# 4. Search modes
echo "=== 4. Search Functionality ==="
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"knowledge","limit":5}' 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "Hybrid search" || fail "Hybrid search"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"test","limit":5,"mode":"fts"}' 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "FTS search" || fail "FTS search"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"semantic query","limit":5,"mode":"semantic"}' 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "Semantic search" || fail "Semantic search"
echo

# 5. SKOS
echo "=== 5. SKOS Taxonomy ==="
curl -sf "$BASE_URL/taxonomy/schemes" | jq -e '.' >/dev/null 2>&1 && pass "GET /taxonomy/schemes" || fail "GET /taxonomy/schemes"
curl -sf "$BASE_URL/taxonomy/governance-stats" | jq -e '.' >/dev/null 2>&1 && pass "GET /taxonomy/governance-stats" || fail "GET /taxonomy/governance-stats"
echo

# 6. Jobs
echo "=== 6. Background Jobs ==="
FAILED_JOBS=$(psql -U matric -h localhost -d matric -t -c "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND created_at > NOW() - INTERVAL '1 hour';" 2>/dev/null | tr -d ' ' || echo "0")
[ "${FAILED_JOBS:-0}" -lt 10 ] && pass "Failed jobs acceptable ($FAILED_JOBS)" || warn "High failed jobs ($FAILED_JOBS)"
echo

# 7. Recent errors check
echo "=== 7. Log Health ==="
RECENT_ERRORS=$(journalctl -u matric-api --since "10 minutes ago" 2>/dev/null | grep -ci error || echo "0")
[ "${RECENT_ERRORS:-0}" -lt 5 ] && pass "Recent errors acceptable ($RECENT_ERRORS)" || warn "Recent errors elevated ($RECENT_ERRORS)"
echo

# Summary
echo "========================================"
echo "RESULTS"
echo "========================================"
echo "Passed:   $PASSED"
echo "Failed:   $FAILED"
echo "Warnings: $WARNINGS"
echo "========================================"

if [ "$FAILED" -gt 0 ]; then
    echo "DEPLOYMENT VALIDATION FAILED"
    exit 1
else
    echo "DEPLOYMENT VALIDATION PASSED"
    exit 0
fi
