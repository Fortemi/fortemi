# UAT Phase 3: Search

**Purpose**: Validate all search capabilities including text, spatial, temporal, spatial-temporal, and federated search.

**Duration**: ~8 minutes

**Prerequisites**: Phase 1 completion (seed notes must exist with `uat/capture` tags and location data)

**Tools Tested**: `search` (5 actions: text, spatial, temporal, spatial_temporal, federated)

> **MCP-First Requirement**: All tests in this phase use MCP tool calls exclusively. No direct HTTP requests are permitted. The `search` tool provides unified access to all search modalities through its action-based interface.

---

## Test Cases

### SRCH-001: Text Search Basic

**Test ID**: SRCH-001
**MCP Tool**: `search` (action: text)
**Description**: Perform basic text search with limit

```javascript
const result = await useTool('search', {
  action: 'text',
  query: 'test',
  limit: 5
});
```

**Expected Response**:
- `results` array with up to 5 note objects
- Each note has `id`, `title`, `content` fields
- Results ordered by relevance

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] At least 1 result (from Phase 1 seed data)
- [ ] All results contain the query term
- [ ] Respects limit parameter

---

### SRCH-002: Text Search with Tag Filter

**Test ID**: SRCH-002
**MCP Tool**: `search` (action: text)
**Description**: Text search filtered by tags

```javascript
const result = await useTool('search', {
  action: 'text',
  query: 'test',
  tags: ['uat/capture'],
  limit: 5
});
```

**Expected Response**:
- `results` array with notes tagged `uat/capture`
- All results match both text query and tag filter

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] All results have `uat/capture` tag
- [ ] Result count ≤ 5
- [ ] All results contain query term

---

### SRCH-003: Text Search Phrase

**Test ID**: SRCH-003
**MCP Tool**: `search` (action: text)
**Description**: Search for exact phrase using quoted syntax

```javascript
const result = await useTool('search', {
  action: 'text',
  query: '"exact phrase"',
  limit: 5
});
```

**Expected Response**:
- `results` array with notes containing the exact phrase
- Empty array if no exact matches exist

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] If results exist, they contain the exact phrase "exact phrase"
- [ ] No false positives (words in different order)
- [ ] Respects quoted phrase semantics

---

### SRCH-004: Text Search OR Operator

**Test ID**: SRCH-004
**MCP Tool**: `search` (action: text)
**Description**: Search with OR boolean operator

```javascript
const result = await useTool('search', {
  action: 'text',
  query: 'apple OR orange',
  limit: 10
});
```

**Expected Response**:
- `results` array with notes containing "apple" OR "orange"
- Includes notes with either term

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Results contain at least one of the terms
- [ ] OR operator recognized (not literal "OR" search)
- [ ] Union behavior confirmed

---

### SRCH-005: Text Search NOT Operator

**Test ID**: SRCH-005
**MCP Tool**: `search` (action: text)
**Description**: Search with exclusion operator

```javascript
const result = await useTool('search', {
  action: 'text',
  query: 'test -excluded',
  limit: 10
});
```

**Expected Response**:
- `results` array with notes containing "test" but NOT "excluded"
- Exclusion operator properly filters results

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] All results contain "test"
- [ ] No results contain "excluded"
- [ ] Exclusion operator recognized

---

### SRCH-006: Spatial Search

**Test ID**: SRCH-006
**MCP Tool**: `search` (action: spatial)
**Description**: Search by geographic location and radius

```javascript
const result = await useTool('search', {
  action: 'spatial',
  lat: 40.7128,
  lon: -74.0060,
  radius: 10000,
  limit: 5
});
```

**Expected Response**:
- `results` array with notes within 10km of New York City
- Each result includes location metadata
- Distance calculation respects radius

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] All results have location data
- [ ] Distance from center ≤ 10000m
- [ ] Respects limit parameter

---

### SRCH-007: Temporal Search

**Test ID**: SRCH-007
**MCP Tool**: `search` (action: temporal)
**Description**: Search by time range

```javascript
const result = await useTool('search', {
  action: 'temporal',
  start: '2020-01-01T00:00:00Z',
  end: '2030-12-31T23:59:59Z',
  limit: 5
});
```

**Expected Response**:
- `results` array with notes created within the time range
- All timestamps fall between start and end

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] All results created between 2020-2030
- [ ] Timestamp parsing correct
- [ ] Respects limit parameter

---

### SRCH-008: Spatial-Temporal Combined

**Test ID**: SRCH-008
**MCP Tool**: `search` (action: spatial_temporal)
**Description**: Search by both location and time range

```javascript
const result = await useTool('search', {
  action: 'spatial_temporal',
  lat: 40.7128,
  lon: -74.0060,
  radius: 50000,
  start: '2020-01-01T00:00:00Z',
  end: '2030-12-31T23:59:59Z',
  limit: 5
});
```

**Expected Response**:
- `results` array with notes matching BOTH spatial and temporal criteria
- Intersection of location and time filters

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] All results within 50km of NYC
- [ ] All results created between 2020-2030
- [ ] Combined filter logic correct

---

### SRCH-009: Federated Search

**Test ID**: SRCH-009
**MCP Tool**: `search` (action: federated)
**Description**: Search across multiple memory archives

```javascript
const result = await useTool('search', {
  action: 'federated',
  query: 'test',
  memories: ['public'],
  limit: 3
});
```

**Expected Response**:
- `results` array with notes from specified memories
- Each result includes memory source identifier

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Results include memory metadata
- [ ] Respects limit per memory
- [ ] Handles single-memory case gracefully

---

### SRCH-010: Empty Results

**Test ID**: SRCH-010
**MCP Tool**: `search` (action: text)
**Description**: Search with no matching results

```javascript
const result = await useTool('search', {
  action: 'text',
  query: 'xyznonexistent99999',
  limit: 5
});
```

**Expected Response**:
- Empty `results` array
- No errors, graceful empty response

**Pass Criteria**:
- [ ] Returns valid JSON with empty array
- [ ] No error status
- [ ] `results: []` structure correct
- [ ] Handles zero results gracefully

---

### SRCH-011: Invalid Action

**Test ID**: SRCH-011
**MCP Tool**: `search` (action: bogus)
**Description**: Validate error handling for invalid action
**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
const result = await useTool('search', {
  action: 'bogus',
  query: 'test'
});
```

**Expected Response**:
- Error response indicating invalid action
- HTTP 400 or validation error

**Pass Criteria**:
- [ ] Returns error (not success)
- [ ] Error message mentions invalid action
- [ ] Does not crash or hang
- [ ] Clear error response format

---

### SRCH-012: Missing Query Parameter

**Test ID**: SRCH-012
**MCP Tool**: `search` (action: text)
**Description**: Validate error handling for missing required parameter
**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
const result = await useTool('search', {
  action: 'text'
  // Missing required 'query' parameter
});
```

**Expected Response**:
- Error response indicating missing required parameter
- HTTP 400 or validation error

**Pass Criteria**:
- [ ] Returns error (not success)
- [ ] Error message mentions missing parameter
- [ ] Does not crash or return invalid results
- [ ] Clear error response format

---

## Phase 3 Summary

| Category | Count | Pass | Fail |
|----------|-------|------|------|
| Text Search | 5 | - | - |
| Spatial Search | 1 | - | - |
| Temporal Search | 1 | - | - |
| Spatial-Temporal | 1 | - | - |
| Federated Search | 1 | - | - |
| Error Handling | 2 | - | - |
| **Total** | **12** | **-** | **-** |

**Phase 3 Result**: [ ] PASS [ ] FAIL

**Notes**:
- Default revision mode is "light" (not "full")
- All search tests assume Phase 1 seed data exists
- Spatial/temporal searches may return fewer results depending on seed data
- Federated search tests single-memory case (standard deployment)
