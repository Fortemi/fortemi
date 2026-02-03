# Phase 20: Redis Caching & Performance

**Duration**: ~10 minutes
**Tools Tested**: API endpoints (curl-based)
**Dependencies**: Phase 0 (preflight), Phase 1 (seed data), Phase 3 (search)

---

## Overview

This phase tests Redis-based caching for search queries, cache invalidation strategies, and performance characteristics. Matric Memory uses Redis to cache search results for improved response times on repeated queries.

---

## Important Notes

- Redis is optional; API works without it (degrades to no caching)
- Search cache uses query hash as key
- Cache TTL is configurable (default: 5 minutes)
- Cache invalidated on note create/update/delete
- Embedding set changes invalidate related caches
- Base URL: `http://localhost:3000`

---

## Test Setup

For these tests, you'll need:
- A running matric-memory API with Redis enabled
- `curl` command-line tool
- `jq` for JSON parsing
- Authentication token (from Phase 19)

```bash
BASE_URL="http://localhost:3000"

# Get auth token
CLIENT_ID="your_client_id"
CLIENT_SECRET="your_client_secret"
ACCESS_TOKEN=$(curl -s -X POST "$BASE_URL/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" \
  -d "client_id=$CLIENT_ID" \
  -d "client_secret=$CLIENT_SECRET" | jq -r .access_token)
```

---

## Test Cases

### Search Cache Behavior

#### CACHE-001: First Search (Cache Miss)

**Command**:
```bash
time curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "machine learning algorithms",
    "limit": 10
  }' | jq '.results | length'
```

**Expected**:
- Returns results
- Response time logged (baseline)

**Pass Criteria**: Search completes successfully

**Store**: Response time as `FIRST_SEARCH_TIME`

---

#### CACHE-002: Repeated Search (Cache Hit)

**Command**:
```bash
time curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "machine learning algorithms",
    "limit": 10
  }' | jq '.results | length'
```

**Expected**:
- Returns same results
- Response time significantly faster (cache hit)

**Pass Criteria**:
- Results match CACHE-001
- Response time < 50% of `FIRST_SEARCH_TIME`

**Store**: Response time as `CACHED_SEARCH_TIME`

---

#### CACHE-003: Cache Hit Verification

**Repeat same query multiple times**:

```bash
for i in {1..5}; do
  time curl -s -X POST "$BASE_URL/api/v1/search" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"query": "machine learning algorithms", "limit": 10}' > /dev/null
  echo "Run $i complete"
done
```

**Pass Criteria**: All subsequent queries are fast (cache serving)

---

### Cache Invalidation on Note Operations

#### CACHE-004: Cache Invalidation on Note Create

**Setup**: Ensure search is cached

```bash
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes deployment", "limit": 5}' > /dev/null
```

**Command**: Create a note

```bash
NEW_NOTE=$(curl -s -X POST "$BASE_URL/api/v1/notes" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Kubernetes Deployment\n\nA new note about kubernetes deployments.",
    "tags": ["uat/cache-test", "kubernetes"],
    "revision_mode": "none"
  }' | jq -r .id)

# Wait for cache invalidation to propagate
sleep 1

# Search again (should be cache miss due to invalidation)
time curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes deployment", "limit": 5}' | jq '.results | length'
```

**Pass Criteria**:
- Search completes
- New note may appear in results if embedding is complete
- Cache was invalidated (slower response than cached query)

**Store**: `NEW_NOTE_ID`

---

#### CACHE-005: Cache Invalidation on Note Update

**Setup**: Cache a search

```bash
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes", "limit": 5}' > /dev/null
```

**Command**: Update note

```bash
curl -s -X PATCH "$BASE_URL/api/v1/notes/$NEW_NOTE_ID" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Kubernetes Deployment - Updated\n\nUpdated content about kubernetes.",
    "revision_mode": "none"
  }' > /dev/null

sleep 1

# Search again (cache should be invalidated)
time curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes", "limit": 5}' | jq
```

**Pass Criteria**: Cache invalidated, search executes fresh

---

#### CACHE-006: Cache Invalidation on Note Delete

**Setup**: Cache a search

```bash
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes deployment", "limit": 5}' > /dev/null
```

**Command**: Delete note

```bash
curl -s -X DELETE "$BASE_URL/api/v1/notes/$NEW_NOTE_ID" \
  -H "Authorization: Bearer $ACCESS_TOKEN" > /dev/null

sleep 1

# Search again (cache invalidated)
time curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes deployment", "limit": 5}' | jq
```

**Pass Criteria**: Cache invalidated, results updated

---

### Redis Health

#### CACHE-007: Health Endpoint with Redis Status

**Command**:
```bash
curl -s "$BASE_URL/health" | jq
```

**Expected Response**:
```json
{
  "status": "healthy",
  "version": "2026.2.0",
  "database": "connected",
  "redis": "connected",
  "uptime_seconds": 12345
}
```

**Pass Criteria**:
- `status` is "healthy"
- `redis` is "connected" (or "not_configured" if Redis disabled)

---

#### CACHE-008: Health with Redis Disconnected

**Manual Test** (requires stopping Redis):

```bash
# Stop Redis (if running in Docker)
# docker stop matric-redis

# Check health
curl -s "$BASE_URL/health" | jq

# Restart Redis
# docker start matric-redis
```

**Expected**:
- `redis` status reflects disconnection
- API continues to function (graceful degradation)

**Pass Criteria**: API remains operational without Redis

---

### Rate Limiting

#### CACHE-009: Rate Limit Verification

**Command**: Send burst of requests

```bash
for i in {1..50}; do
  STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE_URL/api/v1/search" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"query": "test query '$i'", "limit": 5}')
  echo "Request $i: HTTP $STATUS"

  # Check for rate limit
  if [ "$STATUS" -eq 429 ]; then
    echo "Rate limit hit at request $i"
    break
  fi
done
```

**Expected**:
- First N requests succeed (200)
- Eventually hit rate limit (429 Too Many Requests)

**Pass Criteria**: Rate limiting enforced (if configured)

**Note**: If no rate limit hit, rate limiting may be disabled or limit is very high

---

### Cache with Different Parameters

#### CACHE-010: Different Embedding Sets Use Separate Caches

**Command**: Search with default embedding set

```bash
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "neural networks",
    "limit": 5
  }' | jq '.results[0].id' > /tmp/cache_default.txt

# Search with different embedding set (if available)
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "neural networks",
    "embedding_set": "alternative-set",
    "limit": 5
  }' | jq '.results[0].id' > /tmp/cache_alt.txt

# Compare results
diff /tmp/cache_default.txt /tmp/cache_alt.txt
```

**Pass Criteria**: Different embedding sets maintain separate caches

**Note**: Requires alternative embedding set to exist

---

#### CACHE-011: Multilingual Query Caching

**Command**: Cache different language queries separately

```bash
# English query
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "artificial intelligence", "limit": 5}' > /tmp/cache_en.json

# German query (different cache key)
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "künstliche intelligenz", "limit": 5}' > /tmp/cache_de.json

# Verify different results cached
diff /tmp/cache_en.json /tmp/cache_de.json
```

**Pass Criteria**: Different query strings have separate cache entries

---

### Concurrent Performance

#### CACHE-012: Concurrent Search Requests

**Command**: Test concurrent cache hits

```bash
# Pre-warm cache
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "performance test", "limit": 10}' > /dev/null

# Launch concurrent requests
for i in {1..10}; do
  (time curl -s -X POST "$BASE_URL/api/v1/search" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"query": "performance test", "limit": 10}' > /dev/null) 2>&1 &
done

wait
echo "All concurrent requests completed"
```

**Pass Criteria**:
- All requests complete successfully
- No cache corruption or errors
- Fast response times (cache serving)

---

#### CACHE-013: Cache Stampede Prevention

**Command**: Invalidate cache and send concurrent requests

```bash
# Create a note to invalidate cache
curl -s -X POST "$BASE_URL/api/v1/notes" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"content": "# Stampede Test", "tags": ["uat/stampede"], "revision_mode": "none"}' > /dev/null

sleep 1

# Send concurrent requests (cache is cold)
for i in {1..20}; do
  curl -s -X POST "$BASE_URL/api/v1/search" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"query": "stampede test", "limit": 5}' > /dev/null &
done

wait
echo "Cache stampede test complete"
```

**Pass Criteria**:
- Requests complete without errors
- Cache handles concurrent misses gracefully
- No duplicate computation (ideally)

---

### Tag Filter Cache

#### CACHE-014: Tag Filtering Affects Cache Keys

**Command**: Same query, different tag filters

```bash
# Query without tag filter
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "test", "limit": 5}' | jq '.results | length' > /tmp/no_tag.txt

# Query with tag filter (different cache key)
curl -s -X POST "$BASE_URL/api/v1/search" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "test", "tags": ["uat/cache-test"], "limit": 5}' | jq '.results | length' > /tmp/with_tag.txt

# Results should differ
diff /tmp/no_tag.txt /tmp/with_tag.txt
```

**Pass Criteria**: Tag filters create separate cache entries

---

### Cache Metrics

#### CACHE-015: Cache Hit Rate Monitoring

**Command**: Check if cache metrics are exposed

```bash
# Check health endpoint for cache stats
curl -s "$BASE_URL/health" | jq

# Or dedicated metrics endpoint (if available)
curl -s "$BASE_URL/metrics" 2>/dev/null | grep -i cache || echo "No metrics endpoint"
```

**Expected** (if metrics available):
- Cache hit/miss counts
- Cache size
- Eviction stats

**Pass Criteria**: Cache metrics accessible or gracefully unavailable

---

## Cleanup

```bash
# Delete test notes created during cache testing
# Clean up temp files
rm -f /tmp/cache_*.json /tmp/cache_*.txt /tmp/no_tag.txt /tmp/with_tag.txt

# Unset environment variables
unset ACCESS_TOKEN NEW_NOTE_ID FIRST_SEARCH_TIME CACHED_SEARCH_TIME
```

---

## Success Criteria

| Test ID | Name | Status |
|---------|------|--------|
| CACHE-001 | First Search (Cache Miss) | |
| CACHE-002 | Repeated Search (Cache Hit) | |
| CACHE-003 | Cache Hit Verification | |
| CACHE-004 | Invalidation on Create | |
| CACHE-005 | Invalidation on Update | |
| CACHE-006 | Invalidation on Delete | |
| CACHE-007 | Redis Health Status | |
| CACHE-008 | Graceful Redis Disconnect | |
| CACHE-009 | Rate Limiting | |
| CACHE-010 | Separate Embedding Caches | |
| CACHE-011 | Multilingual Caching | |
| CACHE-012 | Concurrent Cache Hits | |
| CACHE-013 | Cache Stampede Prevention | |
| CACHE-014 | Tag Filter Cache Keys | |
| CACHE-015 | Cache Metrics | |

**Pass Rate Required**: 90% (14/15)

---

## Performance Expectations

| Scenario | Expected Behavior |
|----------|-------------------|
| Cache Hit | < 50ms response time |
| Cache Miss | < 500ms response time (depends on corpus size) |
| Cache Invalidation | < 10ms propagation |
| Concurrent Hits | Linear scaling up to 100 concurrent |
| Redis Disconnect | Graceful degradation, no errors |

---

## API Endpoints Tested

| Endpoint | Method | Tests |
|----------|--------|-------|
| `/api/v1/search` | POST | All CACHE tests |
| `/health` | GET | CACHE-007, CACHE-008 |
| `/metrics` | GET | CACHE-015 |
| `/api/v1/notes` | POST | CACHE-004, CACHE-013 |
| `/api/v1/notes/:id` | PATCH | CACHE-005 |
| `/api/v1/notes/:id` | DELETE | CACHE-006 |

**Coverage**: 6 endpoints, 15 tests

---

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
