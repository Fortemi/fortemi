# UAT Phase 18: Caching & Performance

**Purpose**: Verify caching behavior and performance characteristics through MCP tool interactions
**Duration**: ~10 minutes
**Tools Tested**: `search_notes`, `create_note`, `update_note`, `delete_note`, `memory_info`
**Dependencies**: Phase 0 (preflight), Phase 1 (seed data), Phase 3 (search)
**Critical**: Yes (100% pass required)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

This phase tests search caching, cache invalidation, and performance from the agent's perspective. All tests use MCP tools exactly as an agent would in a real session. Caching is observed through response characteristics (speed, consistency) rather than direct cache inspection.

> **MCP-First Principle**: An agent experiences caching through MCP tool response times and result consistency. These tests verify caching behavior as the agent sees it.

---

## Important Notes

- Redis MUST be configured for caching tests to pass
- Search cache uses query hash as key
- Cache TTL is configurable (default: 5 minutes)
- Cache invalidated on note create/update/delete
- Embedding set changes invalidate related caches

---

## Test Cases

### Search Cache Behavior

#### CACHE-001: First Search (Baseline)

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "machine learning algorithms", mode: "hybrid", limit: 10 })
```

**Pass Criteria**: Returns results successfully

**Store**: Result count and note IDs for comparison

---

#### CACHE-002: Repeated Search (Consistency)

**MCP Tool**: `search_notes`

```javascript
// Same query immediately after CACHE-001
search_notes({ query: "machine learning algorithms", mode: "hybrid", limit: 10 })
```

**Pass Criteria**:
- Returns same results as CACHE-001
- Result ordering is consistent

---

#### CACHE-003: Multiple Repeated Searches (Stability)

**MCP Tool**: `search_notes`

```javascript
// Run same query 5 times
for (let i = 0; i < 5; i++) {
  search_notes({ query: "machine learning algorithms", mode: "hybrid", limit: 10 })
}
```

**Pass Criteria**: All iterations return consistent results (cache serving stable data)

---

### Cache Invalidation on Note Operations

#### CACHE-004: Cache Invalidation on Note Create

**MCP Tool**: `search_notes`, `create_note`

**Setup**: Run a search to populate cache

```javascript
search_notes({ query: "kubernetes deployment", limit: 5 })
```

**Action**: Create a note matching the search

```javascript
create_note({
  content: "# Kubernetes Deployment\n\nA new note about kubernetes deployment strategies.",
  tags: ["uat/cache-test", "kubernetes"],
  revision_mode: "none"
})
```

**Verify**: Search again after creation

```javascript
// Wait briefly for cache invalidation
search_notes({ query: "kubernetes deployment", limit: 5 })
```

**Pass Criteria**:
- Search completes successfully
- New note may appear in results (if embedding completed)
- Cache was invalidated (fresh results returned)

**Store**: Created note ID as `CACHE_NOTE_ID`

---

#### CACHE-005: Cache Invalidation on Note Update

**MCP Tool**: `search_notes`, `update_note`

**Setup**: Cache a search

```javascript
search_notes({ query: "kubernetes", limit: 5 })
```

**Action**: Update the test note

```javascript
update_note({
  id: "<CACHE_NOTE_ID>",
  content: "# Kubernetes Deployment - Updated\n\nUpdated content about kubernetes deployment automation."
})
```

**Verify**: Search again

```javascript
search_notes({ query: "kubernetes", limit: 5 })
```

**Pass Criteria**: Cache invalidated, search executes with fresh data

---

#### CACHE-006: Cache Invalidation on Note Delete

**MCP Tool**: `search_notes`, `delete_note`

**Setup**: Cache a search

```javascript
search_notes({ query: "kubernetes deployment", limit: 5 })
```

**Action**: Delete the test note

```javascript
delete_note({ id: "<CACHE_NOTE_ID>" })
```

**Verify**: Search again

```javascript
search_notes({ query: "kubernetes deployment", limit: 5 })
```

**Pass Criteria**: Cache invalidated, deleted note no longer in results

---

### System Health

#### CACHE-007: System Health via MCP

**MCP Tool**: `memory_info`

```javascript
memory_info()
```

**Pass Criteria**:
- Returns system status information
- System is operational (healthy)

---

### Search Parameter Isolation

#### CACHE-008: Different Embedding Sets Return Different Results

**MCP Tools**: `create_embedding_set`, `search_notes`, `delete_embedding_set`

**Setup**: Create a test embedding set for this test

```javascript
// Create embedding set for cache isolation testing
create_embedding_set({
  slug: "uat-cache-test-set",
  name: "UAT Cache Test Set",
  description: "Embedding set for cache isolation testing"
})
```

**Test**:

```javascript
// Search with default embedding set
search_notes({ query: "neural networks", limit: 5 })

// Search with specific embedding set
search_notes({ query: "neural networks", embedding_set: "uat-cache-test-set", limit: 5 })
```

**Pass Criteria**: Both searches complete successfully; different embedding sets use separate cache entries

**Cleanup**:

```javascript
// Remove test embedding set
delete_embedding_set({ slug: "uat-cache-test-set" })
```

---

#### CACHE-009: Multilingual Query Isolation

**MCP Tool**: `search_notes`

```javascript
// English query
search_notes({ query: "artificial intelligence", limit: 5 })

// German query (different cache entry)
search_notes({ query: "kunstliche intelligenz", limit: 5 })
```

**Pass Criteria**: Different language queries return language-appropriate results

---

#### CACHE-010: Tag Filter Creates Separate Cache Entries

**MCP Tool**: `search_notes`

```javascript
// Query without tag filter
search_notes({ query: "test", limit: 5 })

// Same query with tag filter (different cache key)
search_notes({ query: "test", required_tags: ["uat/cache-test"], limit: 5 })
```

**Pass Criteria**: Tag-filtered results are a subset of (or different from) unfiltered results

---

### Cache Under Load

#### CACHE-011: Sequential Search Burst

**MCP Tool**: `search_notes`

```javascript
// Run 10 sequential searches with the same query
for (let i = 0; i < 10; i++) {
  search_notes({ query: "performance test", limit: 10 })
}
```

**Pass Criteria**:
- All searches complete successfully
- Results remain consistent across iterations

---

#### CACHE-012: Varied Query Burst

**MCP Tool**: `search_notes`

```javascript
// Run searches with different queries
const queries = [
  "machine learning",
  "neural networks",
  "deep learning",
  "natural language processing",
  "computer vision"
];

for (const q of queries) {
  search_notes({ query: q, limit: 5 })
}
```

**Pass Criteria**: All queries return results without errors

---

#### CACHE-013: Cache Stampede Prevention via Create + Search

**MCP Tool**: `create_note`, `search_notes`

```javascript
// Create a note to invalidate cache
create_note({
  content: "# Stampede Test\n\nNote for cache stampede testing.",
  tags: ["uat/stampede"],
  revision_mode: "none"
})

// Immediately search (cache is cold for this query)
search_notes({ query: "stampede test", limit: 5 })
search_notes({ query: "stampede test", limit: 5 })
search_notes({ query: "stampede test", limit: 5 })
```

**Pass Criteria**: All searches complete without errors, cache handles cold misses gracefully

---

### FTS Mode Performance

#### CACHE-014: FTS Search Consistency

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "programming", mode: "fts", limit: 10 })
search_notes({ query: "programming", mode: "fts", limit: 10 })
```

**Pass Criteria**: FTS-only search returns consistent results across repeated calls

---

#### CACHE-015: Semantic Search Consistency

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "programming", mode: "semantic", limit: 10 })
search_notes({ query: "programming", mode: "semantic", limit: 10 })
```

**Pass Criteria**: Semantic-only search returns consistent results across repeated calls

---

## Cleanup

```javascript
// Delete notes created during cache testing
search_notes({ query: "tags:uat/cache-test OR tags:uat/stampede", limit: 100 })
// Delete each found note via delete_note()
```

---

## Success Criteria

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| CACHE-001 | First Search (Baseline) | `search_notes` | |
| CACHE-002 | Repeated Search (Consistency) | `search_notes` | |
| CACHE-003 | Multiple Repeated Searches | `search_notes` | |
| CACHE-004 | Invalidation on Create | `search_notes`, `create_note` | |
| CACHE-005 | Invalidation on Update | `search_notes`, `update_note` | |
| CACHE-006 | Invalidation on Delete | `search_notes`, `delete_note` | |
| CACHE-007 | System Health via MCP | `memory_info` | |
| CACHE-008 | Embedding Set Isolation | `create_embedding_set`, `search_notes`, `delete_embedding_set` | |
| CACHE-009 | Multilingual Query Isolation | `search_notes` | |
| CACHE-010 | Tag Filter Cache Keys | `search_notes` | |
| CACHE-011 | Sequential Search Burst | `search_notes` | |
| CACHE-012 | Varied Query Burst | `search_notes` | |
| CACHE-013 | Cache Stampede Prevention | `create_note`, `search_notes` | |
| CACHE-014 | FTS Search Consistency | `search_notes` | |
| CACHE-015 | Semantic Search Consistency | `search_notes` | |

**Pass Rate Required**: 100% (15/15)

---

## Performance Expectations

| Scenario | Expected Behavior |
|----------|-------------------|
| Cached Search | Consistent results, fast response |
| Cache Miss | Completes successfully, results returned |
| Cache Invalidation | Fresh results after create/update/delete |
| Sequential Burst | All queries succeed, no degradation |
| Multilingual Queries | Separate results per language |

---

## MCP Tools Covered

`search_notes`, `create_note`, `update_note`, `delete_note`, `memory_info`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
