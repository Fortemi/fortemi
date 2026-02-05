#!/bin/bash
# Script to update issue #61 with Redis bundle integration requirements
# Run: ./scripts/update-issue-61-redis-bundle.sh

DESCRIPTION=$(cat << 'EOF'
## Summary

Implement Redis caching for hybrid search results to reduce latency and compute load for repeated/similar queries.

## Motivation

- Hybrid search involves FTS query + vector similarity + RRF fusion - expensive pipeline
- Many queries are repeated (same user, same context)
- Semantic similarity means near-duplicate queries could share cached results

## Proposed Architecture

```
[Query] -> [Cache Key Generation] -> [Redis Lookup]
                                          |
                                [Hit] -> Return cached results
                                [Miss] -> Execute search -> Cache results -> Return
```

### Cache Key Strategy

| Option | Description |
|--------|-------------|
| **Query hash** | SHA256 of normalized query string |
| **Vector hash** | Hash of query embedding (catches semantic duplicates) |
| **Hybrid** | Combine both for maximum hit rate |

### Suggested Configuration

```toml
[cache]
enabled = true
backend = "redis"
ttl_seconds = 300  # 5 minutes
max_entries = 10000
prefix = "mm:search:"
```

## Trade-offs

| Factor | Consideration |
|--------|---------------|
| **Freshness** | New notes won't appear in cached results until TTL expires |
| **Memory** | Each cached result set consumes Redis memory |
| **Invalidation** | Note create/update/delete should optionally bust cache |

## Invalidation Strategy

- **Conservative**: TTL-based only (simple, eventually consistent)
- **Aggressive**: Invalidate on any note mutation (fresh, more complex)
- **Selective**: Invalidate only queries that would match the changed note

---

## Docker Bundle Integration Requirements (2026-02-02)

**Requirement**: Redis should be included in the Docker bundle and be configurable.

### docker-compose.bundle.yml Changes

```yaml
services:
  redis:
    image: redis:7-alpine
    container_name: fortemi-redis
    restart: unless-stopped
    command: redis-server --appendonly yes
    volumes:
      - fortemi-redis:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  fortemi:
    environment:
      # Redis caching (optional, enabled by default)
      - REDIS_ENABLED=${REDIS_ENABLED:-true}
      - REDIS_URL=redis://redis:6379
      - REDIS_CACHE_TTL=${REDIS_CACHE_TTL:-300}
    depends_on:
      redis:
        condition: service_healthy

volumes:
  fortemi-redis:
    driver: local
```

### Configuration Options

| Variable | Default | Description |
|----------|---------|-------------|
| REDIS_ENABLED | true | Enable/disable Redis caching |
| REDIS_URL | redis://redis:6379 | Redis connection string |
| REDIS_CACHE_TTL | 300 | Cache TTL in seconds |
| REDIS_MAX_ENTRIES | 10000 | Maximum cache entries |
| REDIS_PREFIX | mm:search: | Cache key prefix |

### Behavior

1. **Default**: Redis runs and caching is enabled
2. **Disable caching**: Set `REDIS_ENABLED=false` in .env
3. **Graceful fallback**: If Redis unavailable, queries bypass cache

---

## Acceptance Criteria

- [ ] Redis cache integration for search endpoint
- [ ] Configurable TTL
- [ ] Cache hit/miss metrics exposed
- [ ] Optional cache bypass header for debugging
- [ ] Documentation for cache configuration
- [ ] **Redis in docker-compose.bundle.yml**
- [ ] **REDIS_ENABLED environment variable (default: true)**
- [ ] **Graceful fallback when Redis unavailable**

## Related

- Captured in memory note: "Consider adding Redis caching to Fortémi hybrid search"
- Existing Redis service in MATRIC stack (port 6379)

## Labels

deferred, performance, infrastructure
EOF
)

tea issue edit --repo fortemi/fortemi 61 -d "$DESCRIPTION"
echo "Issue #61 updated with Redis bundle integration requirements"
