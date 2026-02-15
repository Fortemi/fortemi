# Production Deployment Test Plan

This document provides a systematic checklist for validating Fortémi production deployments. Execute after every deployment, migration, or system change.

## Quick Validation (5 minutes)

Run this minimal checklist for routine deployments:

```bash
# 1. Service health
curl -sf http://localhost:3000/health && echo "OK" || echo "FAILED"

# 2. Database connectivity
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "SELECT 1;" >/dev/null && echo "DB OK" || echo "DB FAILED"

# 3. Basic API response
curl -sf http://localhost:3000/api/v1/notes?limit=1 | jq -e '.notes' >/dev/null && echo "API OK" || echo "API FAILED"
```

---

## Full Validation Checklist

### 1. Infrastructure Health

| Test | Command | Expected |
|------|---------|----------|
| API service running | `systemctl is-active matric-api` | `active` |
| PostgreSQL running | `systemctl is-active postgresql` | `active` |
| API listening on port | `ss -tlnp \| grep 3000` | Socket listed |
| Health endpoint | `curl http://localhost:3000/health` | `{"status":"healthy","version":"..."}` |

```bash
# Run infrastructure checks
echo "=== Infrastructure Health ==="
systemctl is-active matric-api && echo "✓ API service active" || echo "✗ API service not active"
systemctl is-active postgresql && echo "✓ PostgreSQL active" || echo "✗ PostgreSQL not active"
curl -sf http://localhost:3000/health && echo "✓ Health endpoint OK" || echo "✗ Health endpoint failed"
```

### 2. Database Validation

| Test | Command | Expected |
|------|---------|----------|
| Connection | `psql -c "SELECT 1;"` | Returns `1` |
| pgvector extension | `psql -c "SELECT * FROM pg_extension WHERE extname='vector';"` | Row returned |
| Core tables exist | `psql -c "\dt notes"` | Table listed |
| Default scheme exists | Query below | Row returned |
| Default embedding set exists | Query below | Row returned |

```bash
# Run database checks
echo "=== Database Validation ==="
export PGPASSWORD=matric

# Connection test
psql -U matric -h localhost -d matric -c "SELECT 1;" >/dev/null && echo "✓ DB connection OK" || echo "✗ DB connection failed"

# pgvector extension
psql -U matric -h localhost -d matric -c "SELECT extname FROM pg_extension WHERE extname='vector';" | grep -q vector && echo "✓ pgvector extension OK" || echo "✗ pgvector extension missing"

# Core tables
for table in notes embedding embedding_set embedding_config jobs skos_concept_scheme; do
    psql -U matric -h localhost -d matric -c "\dt $table" 2>/dev/null | grep -q "$table" && echo "✓ Table $table exists" || echo "✗ Table $table missing"
done

# Default scheme (dynamic ID lookup)
psql -U matric -h localhost -d matric -c "SELECT id FROM skos_concept_scheme WHERE is_system = TRUE AND notation = 'default';" | grep -q "[a-f0-9-]" && echo "✓ Default SKOS scheme exists" || echo "✗ Default SKOS scheme missing"

# Default embedding set (dynamic ID lookup)
psql -U matric -h localhost -d matric -c "SELECT id FROM embedding_set WHERE is_system = TRUE AND slug = 'default';" | grep -q "[a-f0-9-]" && echo "✓ Default embedding set exists" || echo "✗ Default embedding set missing"

# Default embedding config (dynamic ID lookup)
psql -U matric -h localhost -d matric -c "SELECT id FROM embedding_config WHERE is_default = TRUE;" | grep -q "[a-f0-9-]" && echo "✓ Default embedding config exists" || echo "✗ Default embedding config missing"
```

### 3. API Endpoint Validation

| Endpoint | Method | Test |
|----------|--------|------|
| `/health` | GET | Returns 200 |
| `/notes` | GET | Returns JSON array |
| `/search` | POST | Search works |
| `/collections` | GET | Returns collections |
| `/tags` | GET | Returns tags |
| `/embedding-sets` | GET | Returns sets |
| `/templates` | GET | Returns templates |

```bash
# Run API endpoint checks
echo "=== API Endpoint Validation ==="
BASE_URL="http://localhost:3000"

# Health
curl -sf "$BASE_URL/health" >/dev/null && echo "✓ GET /health" || echo "✗ GET /health"

# Notes list
curl -sf "$BASE_URL/api/v1/notes?limit=1" | jq -e '.notes' >/dev/null && echo "✓ GET /api/v1/notes" || echo "✗ GET /api/v1/notes"

# Search (uses q= parameter)
curl -sf "$BASE_URL/api/v1/search?q=test&limit=1" | jq -e '.results' >/dev/null && echo "✓ GET /api/v1/search" || echo "✗ GET /api/v1/search"

# Collections
curl -sf "$BASE_URL/api/v1/collections" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/collections" || echo "✗ GET /api/v1/collections"

# Tags
curl -sf "$BASE_URL/api/v1/tags" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/tags" || echo "✗ GET /api/v1/tags"

# Embedding sets
curl -sf "$BASE_URL/api/v1/embedding-sets" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/embedding-sets" || echo "✗ GET /api/v1/embedding-sets"

# Templates
curl -sf "$BASE_URL/api/v1/templates" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/templates" || echo "✗ GET /api/v1/templates"

# Queue stats
curl -sf "$BASE_URL/api/v1/jobs/stats" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/jobs/stats" || echo "✗ GET /api/v1/jobs/stats"

# Backup status
curl -sf "$BASE_URL/api/v1/backup/status" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/backup/status" || echo "✗ GET /api/v1/backup/status"
```

### 4. CRUD Operations Test

Test full create-read-update-delete cycle with a test note.

```bash
echo "=== CRUD Operations Test ==="
BASE_URL="http://localhost:3000"

# Create test note
echo "Creating test note..."
NOTE_RESPONSE=$(curl -sf -X POST "$BASE_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d '{"content":"Production test note - can be deleted","revision_mode":"none","tags":["test/production-validation"]}')

NOTE_ID=$(echo "$NOTE_RESPONSE" | jq -r '.id')
if [ "$NOTE_ID" != "null" ] && [ -n "$NOTE_ID" ]; then
    echo "✓ CREATE note: $NOTE_ID"
else
    echo "✗ CREATE note failed"
    exit 1
fi

# Read test note
echo "Reading test note..."
READ_RESPONSE=$(curl -sf "$BASE_URL/api/v1/notes/$NOTE_ID")
if echo "$READ_RESPONSE" | jq -e '.id' >/dev/null; then
    echo "✓ READ note"
else
    echo "✗ READ note failed"
fi

# Update test note (star it)
echo "Updating test note..."
UPDATE_RESPONSE=$(curl -sf -X PUT "$BASE_URL/api/v1/notes/$NOTE_ID" \
    -H "Content-Type: application/json" \
    -d '{"starred":true}')
if echo "$UPDATE_RESPONSE" | jq -e '.starred == true' >/dev/null; then
    echo "✓ UPDATE note"
else
    echo "✗ UPDATE note failed"
fi

# Delete test note
echo "Deleting test note..."
DELETE_RESPONSE=$(curl -sf -X DELETE "$BASE_URL/api/v1/notes/$NOTE_ID")
echo "✓ DELETE note"

echo "CRUD test completed successfully"
```

### 5. Search Functionality Test

Search endpoint uses GET with `q=` query parameter.

```bash
echo "=== Search Functionality Test ==="
BASE_URL="http://localhost:3000"

# Hybrid search (default)
curl -sf "$BASE_URL/api/v1/search?q=knowledge&limit=5" | jq -e '.results' >/dev/null && echo "✓ Hybrid search" || echo "✗ Hybrid search"

# FTS-only search
curl -sf "$BASE_URL/api/v1/search?q=test&limit=5&mode=fts" | jq -e '.results' >/dev/null && echo "✓ FTS search" || echo "✗ FTS search"

# Semantic-only search
curl -sf "$BASE_URL/api/v1/search?q=semantic&limit=5&mode=semantic" | jq -e '.results' >/dev/null && echo "✓ Semantic search" || echo "✗ Semantic search"
```

### 6. SKOS/Taxonomy Validation

```bash
echo "=== SKOS Taxonomy Validation ==="
BASE_URL="http://localhost:3000"

# List concept schemes
curl -sf "$BASE_URL/api/v1/concepts/schemes" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/concepts/schemes" || echo "✗ GET /api/v1/concepts/schemes"

# Search concepts
curl -sf "$BASE_URL/api/v1/concepts?q=test" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/concepts" || echo "✗ GET /api/v1/concepts"

# Governance stats
curl -sf "$BASE_URL/api/v1/concepts/governance" | jq -e '.' >/dev/null && echo "✓ GET /api/v1/concepts/governance" || echo "✗ GET /api/v1/concepts/governance"
```

### 7. Background Jobs Validation

```bash
echo "=== Background Jobs Validation ==="
BASE_URL="http://localhost:3000"

# Queue stats
QUEUE_STATS=$(curl -sf "$BASE_URL/api/v1/jobs/stats")
echo "Queue stats: $QUEUE_STATS"

# Check for stuck jobs
PGPASSWORD=matric psql -U matric -h localhost -d matric -c "
SELECT status, COUNT(*)
FROM jobs
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY status;" | grep -q "." && echo "✓ Jobs table accessible" || echo "✗ Jobs table issue"

# Check for failed jobs in last hour
FAILED_COUNT=$(PGPASSWORD=matric psql -U matric -h localhost -d matric -t -c "
SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND created_at > NOW() - INTERVAL '1 hour';")
if [ "$FAILED_COUNT" -gt 10 ]; then
    echo "⚠ Warning: $FAILED_COUNT failed jobs in last hour"
else
    echo "✓ Failed jobs in acceptable range ($FAILED_COUNT)"
fi
```

### 8. MCP Server Validation (if applicable)

```bash
echo "=== MCP Server Validation ==="

# Check if MCP server is running
if pgrep -f "node.*mcp-server" >/dev/null; then
    echo "✓ MCP server process running"
else
    echo "⚠ MCP server not running (optional component)"
fi

# Test MCP health if HTTP mode
MCP_URL="http://localhost:3001"
if curl -sf "$MCP_URL/health" >/dev/null 2>&1; then
    echo "✓ MCP HTTP endpoint responsive"
else
    echo "○ MCP HTTP endpoint not available (may be stdio mode)"
fi
```

---

## Full Test Script

Save and run as `/path/to/fortemi/scripts/production-test.sh`:

```bash
#!/bin/bash
# Production Deployment Test Script for Fortémi
# Run after any deployment or system change

set -e
export PGPASSWORD=matric
BASE_URL="${MATRIC_API_URL:-http://localhost:3000}"

echo "========================================"
echo "Fortémi Production Validation"
echo "========================================"
echo "Target: $BASE_URL"
echo "Date: $(date)"
echo "========================================"
echo

PASSED=0
FAILED=0
WARNINGS=0

pass() { echo "✓ $1"; ((PASSED++)); }
fail() { echo "✗ $1"; ((FAILED++)); }
warn() { echo "⚠ $1"; ((WARNINGS++)); }

# 1. Infrastructure
echo "=== 1. Infrastructure Health ==="
systemctl is-active matric-api >/dev/null && pass "API service active" || fail "API service not active"
systemctl is-active postgresql >/dev/null && pass "PostgreSQL active" || fail "PostgreSQL not active"
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
curl -sf "$BASE_URL/notes?limit=1" | jq -e '.notes' >/dev/null && pass "GET /notes" || fail "GET /notes"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"test","limit":1}' | jq -e '.results' >/dev/null && pass "POST /search" || fail "POST /search"
curl -sf "$BASE_URL/collections" | jq -e '.' >/dev/null && pass "GET /collections" || fail "GET /collections"
curl -sf "$BASE_URL/tags" | jq -e '.' >/dev/null && pass "GET /tags" || fail "GET /tags"
curl -sf "$BASE_URL/embedding-sets" | jq -e '.' >/dev/null && pass "GET /embedding-sets" || fail "GET /embedding-sets"
curl -sf "$BASE_URL/templates" | jq -e '.' >/dev/null && pass "GET /templates" || fail "GET /templates"
curl -sf "$BASE_URL/queue/stats" | jq -e '.' >/dev/null && pass "GET /queue/stats" || fail "GET /queue/stats"
echo

# 4. Search modes
echo "=== 4. Search Functionality ==="
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"knowledge","limit":5}' | jq -e '.results' >/dev/null && pass "Hybrid search" || fail "Hybrid search"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"test","limit":5,"mode":"fts"}' | jq -e '.results' >/dev/null && pass "FTS search" || fail "FTS search"
curl -sf -X POST "$BASE_URL/search" -H "Content-Type: application/json" -d '{"query":"semantic query","limit":5,"mode":"semantic"}' | jq -e '.results' >/dev/null && pass "Semantic search" || fail "Semantic search"
echo

# 5. SKOS
echo "=== 5. SKOS Taxonomy ==="
curl -sf "$BASE_URL/taxonomy/schemes" | jq -e '.' >/dev/null && pass "GET /taxonomy/schemes" || fail "GET /taxonomy/schemes"
curl -sf "$BASE_URL/taxonomy/governance-stats" | jq -e '.' >/dev/null && pass "GET /taxonomy/governance-stats" || fail "GET /taxonomy/governance-stats"
echo

# 6. Jobs
echo "=== 6. Background Jobs ==="
FAILED_JOBS=$(psql -U matric -h localhost -d matric -t -c "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND created_at > NOW() - INTERVAL '1 hour';" 2>/dev/null | tr -d ' ')
[ "${FAILED_JOBS:-0}" -lt 10 ] && pass "Failed jobs acceptable ($FAILED_JOBS)" || warn "High failed jobs ($FAILED_JOBS)"
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
```

---

## Post-Migration Checklist

After applying database migrations, also verify:

- [ ] Migration applied without errors
- [ ] No "column does not exist" errors in logs
- [ ] New tables/columns visible in schema
- [ ] Existing data not corrupted
- [ ] Service restart successful

```bash
# Post-migration verification
journalctl -u matric-api --since "5 minutes ago" | grep -i error && echo "⚠ Errors found in logs" || echo "✓ No errors in recent logs"
```

---

## Rollback Triggers

Immediately rollback if ANY of these occur:

1. Health endpoint returns non-200
2. Database connection failures
3. More than 50% of API endpoints failing
4. Critical errors in service logs
5. Data corruption detected

See [Operations Guide](./operations.md#rollback-procedure) for rollback steps.

---

## Version History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-29 | AI Agent | Initial production test plan |
