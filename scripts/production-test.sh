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

# 3. API Endpoints (all under /api/v1/)
echo "=== 3. API Endpoints ==="
curl -sf "$BASE_URL/api/v1/notes?limit=1" | jq -e '.notes' >/dev/null 2>&1 && pass "GET /api/v1/notes" || fail "GET /api/v1/notes"
curl -sf "$BASE_URL/api/v1/search?q=test&limit=1" 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "GET /api/v1/search" || fail "GET /api/v1/search"
curl -sf "$BASE_URL/api/v1/collections" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/collections" || fail "GET /api/v1/collections"
curl -sf "$BASE_URL/api/v1/tags" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/tags" || fail "GET /api/v1/tags"
curl -sf "$BASE_URL/api/v1/embedding-sets" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/embedding-sets" || fail "GET /api/v1/embedding-sets"
curl -sf "$BASE_URL/api/v1/templates" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/templates" || fail "GET /api/v1/templates"
curl -sf "$BASE_URL/api/v1/jobs/stats" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/jobs/stats" || fail "GET /api/v1/jobs/stats"
curl -sf "$BASE_URL/api/v1/backup/status" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/backup/status" || fail "GET /api/v1/backup/status"
echo

# 4. Search modes (search endpoint uses GET with q= param)
echo "=== 4. Search Functionality ==="
curl -sf "$BASE_URL/api/v1/search?q=knowledge&limit=5" 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "Hybrid search" || fail "Hybrid search"
curl -sf "$BASE_URL/api/v1/search?q=test&limit=5&mode=fts" 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "FTS search" || fail "FTS search"
curl -sf "$BASE_URL/api/v1/search?q=semantic&limit=5&mode=semantic" 2>&1 | jq -e '.results' >/dev/null 2>&1 && pass "Semantic search" || fail "Semantic search"
echo

# 5. MCP Server & OAuth
echo "=== 5. MCP Server & OAuth ==="
MCP_URL="${MCP_URL:-http://localhost:3001}"
# MCP server should return 401 (auth required) not connection refused
MCP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$MCP_URL" 2>/dev/null || echo "000")
[ "$MCP_CODE" = "401" ] && pass "MCP server reachable (returns 401)" || fail "MCP server unreachable (HTTP $MCP_CODE)"
# OAuth discovery endpoint
curl -sf "$BASE_URL/.well-known/oauth-authorization-server" | grep -q "issuer" 2>/dev/null && pass "OAuth discovery endpoint" || fail "OAuth discovery endpoint"
# OAuth registration endpoint (should accept POST)
REG_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/oauth/register" -H "Content-Type: application/json" -d '{}' 2>/dev/null || echo "000")
[ "$REG_CODE" != "000" ] && [ "$REG_CODE" != "404" ] && pass "OAuth register endpoint (HTTP $REG_CODE)" || fail "OAuth register endpoint (HTTP $REG_CODE)"
# Check MCP credentials are configured in running container
if docker compose -f docker-compose.bundle.yml exec -T matric printenv MCP_CLIENT_ID 2>/dev/null | grep -q "mm_"; then
    pass "MCP_CLIENT_ID configured in container"
else
    warn "MCP_CLIENT_ID not configured in container"
fi
echo

# 6. SKOS (concepts/schemes endpoints)
echo "=== 6. SKOS Taxonomy ==="
curl -sf "$BASE_URL/api/v1/concepts/schemes" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/concepts/schemes" || fail "GET /api/v1/concepts/schemes"
curl -sf "$BASE_URL/api/v1/concepts/governance" | jq -e '.' >/dev/null 2>&1 && pass "GET /api/v1/concepts/governance" || fail "GET /api/v1/concepts/governance"
echo

# 7. Jobs
echo "=== 7. Background Jobs ==="
FAILED_JOBS=$(psql -U matric -h localhost -d matric -t -c "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND created_at > NOW() - INTERVAL '1 hour';" 2>/dev/null | tr -d ' \n' || echo "0")
FAILED_JOBS=${FAILED_JOBS:-0}
[ "$FAILED_JOBS" -lt 10 ] 2>/dev/null && pass "Failed jobs acceptable ($FAILED_JOBS)" || warn "High failed jobs ($FAILED_JOBS)"
echo

# 8. Recent errors check
echo "=== 8. Log Health ==="
RECENT_ERRORS=$(journalctl -u matric-api --since "10 minutes ago" 2>/dev/null | grep -ci error 2>/dev/null || echo "0")
RECENT_ERRORS=${RECENT_ERRORS:-0}
[ "$RECENT_ERRORS" -lt 5 ] 2>/dev/null && pass "Recent errors acceptable ($RECENT_ERRORS)" || warn "Recent errors elevated ($RECENT_ERRORS)"
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
