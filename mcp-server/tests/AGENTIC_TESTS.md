# MCP Agentic Integration Tests

Comprehensive integration test suite for the Fortemi MCP server. These tests validate end-to-end functionality by making actual MCP tool calls through the test client.

## Overview

Unlike the existing static/unit tests that parse tool definitions, these agentic tests:

- **Connect to a live MCP server** via the test client
- **Execute actual tool calls** and verify real responses
- **Test complete workflows** from creation to cleanup
- **Validate data integrity** across multiple operations
- **Use UUIDs for isolation** to prevent test collisions

## Test Files

### Phase 0: Preflight Checks (`preflight.test.js`)

**Purpose**: Verify server connectivity and basic functionality before running comprehensive tests.

**Tests**:
- `PREFLIGHT-001`: Server info returns server name and version
- `PREFLIGHT-002`: Health check via API returns success
- `PREFLIGHT-003`: Tools list returns 100+ tools
- `PREFLIGHT-004`: Critical tools are present
- `PREFLIGHT-005`: Session management works correctly

**Run**: `node --test mcp-server/tests/preflight.test.js`

### Phase 2: CRUD Operations (`crud.test.js`) - CRITICAL

**Purpose**: Test core note lifecycle operations that form the foundation of the knowledge base.

**Tests**:
- `CRUD-001`: Create note with content and tags returns ID
- `CRUD-002`: Get note by ID returns content, tags, and metadata
- `CRUD-003`: Update note changes content
- `CRUD-004`: Delete note marks as deleted
- `CRUD-005`: List notes returns array
- `CRUD-006`: List notes with tag filter
- `CRUD-007`: Bulk create notes creates multiple
- `CRUD-008`: Get note for non-existent UUID returns error
- `CRUD-009`: Create note with empty content returns error
- `CRUD-010`: Update note with invalid ID returns error
- `CRUD-011`: Create and retrieve note with special characters

**Coverage**: 11 test cases covering happy path, error handling, filtering, bulk ops, and edge cases.

**Run**: `node --test mcp-server/tests/crud.test.js`

### Phase 3: Search Operations (`search.test.js`) - CRITICAL

**Purpose**: Test hybrid search capabilities including full-text search, semantic search, and tag filtering.

**Tests**:
- `SEARCH-001`: Search notes with text query returns results
- `SEARCH-002`: Search with specific term finds correct notes
- `SEARCH-003`: Search with tag filter returns filtered results
- `SEARCH-004`: Search with combined query and tag filter
- `SEARCH-005`: Search with limit parameter restricts results
- `SEARCH-006`: Search with offset parameter skips results
- `SEARCH-007`: Empty query returns results
- `SEARCH-008`: Non-matching query returns empty or minimal results
- `SEARCH-009`: Search result includes relevance metadata
- `SEARCH-010`: Search handles special characters in query
- `SEARCH-011`: Search with multiple tags (AND logic)
- `SEARCH-012`: Case-insensitive search

**Coverage**: 12 test cases covering text search, tag filtering, pagination, edge cases, and multilingual support.

**Setup**: Creates 4 test notes with different topics and tags in `before()` hook.

**Run**: `node --test mcp-server/tests/search.test.js`

### Phase 4: Tag Operations (`tags.test.js`)

**Purpose**: Test tag management functionality for organizing knowledge.

**Tests**:
- `TAG-001`: List tags returns array
- `TAG-002`: Create note with tags adds tags to note
- `TAG-003`: Created tags appear in list_tags
- `TAG-004`: Update note to add tags
- `TAG-005`: Update note to remove tags
- `TAG-006`: Hierarchical tags with slashes
- `TAG-007`: Tags with special characters
- `TAG-008`: Empty tags array creates note without tags
- `TAG-009`: Duplicate tags are handled correctly
- `TAG-010`: Tag list includes usage counts
- `TAG-011`: Update note with null/empty tags removes all tags

**Coverage**: 11 test cases covering tag CRUD, hierarchies, special characters, and edge cases.

**Run**: `node --test mcp-server/tests/tags.test.js`

### Phase 5: Collections (`collections.test.js`)

**Purpose**: Test collection management for organizing notes into logical groups.

**Tests**:
- `COLL-001`: Create collection with name and description
- `COLL-002`: List collections returns array
- `COLL-003`: Created collection appears in list
- `COLL-004`: Add note to collection
- `COLL-005`: List notes in collection
- `COLL-006`: Remove note from collection
- `COLL-007`: Delete collection
- `COLL-008`: Get collection details
- `COLL-009`: Collection with empty description
- `COLL-010`: Add same note to multiple collections
- `COLL-011`: Delete collection with notes does not delete notes
- `COLL-012`: Update collection name and description

**Coverage**: 12 test cases covering collection CRUD, note relationships, and cascade behavior.

**Run**: `node --test mcp-server/tests/collections.test.js`

## Test Architecture

### Test Client (`helpers/mcp-client.js`)

The `MCPTestClient` class provides:

```javascript
// Initialize connection
const client = new MCPTestClient();
await client.initialize();

// Call tools
const result = await client.callTool("create_note", { content: "..." });

// Expect errors
const error = await client.callToolExpectError("get_note", { id: "invalid" });

// Utilities
const id = MCPTestClient.uniqueId();
const tag = MCPTestClient.testTag("phase", "suffix");

// Cleanup
await client.close();
```

### Test Pattern

All tests follow this pattern:

```javascript
describe("Phase X: Description", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Cleanup test data
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  test("TEST-001: description", async () => {
    // Test logic with assertions
  });
});
```

### Isolation Strategy

**UUID-based unique identifiers** for complete test isolation:

- `MCPTestClient.uniqueId()` - Generate UUID for IDs
- `MCPTestClient.testTag(phase, suffix)` - Generate unique test tags
- All test data tracked in `cleanup` object for teardown
- Tests can run in parallel without collisions

**NEVER use timestamps** - parallel tests can generate same millisecond values.

## Running Tests

### Prerequisites

1. **MCP Server Running**:
   ```bash
   # In one terminal
   cd mcp-server
   npm start
   ```

2. **API Server Running**:
   ```bash
   # In another terminal
   cd crates/matric-api
   cargo run
   ```

3. **Environment Variables** (optional):
   ```bash
   export MCP_BASE_URL=http://localhost:3001
   export FORTEMI_API_KEY=your-api-key
   ```

### Run All Agentic Tests

```bash
# Run all agentic tests in sequence
node --test mcp-server/tests/preflight.test.js \
            mcp-server/tests/crud.test.js \
            mcp-server/tests/search.test.js \
            mcp-server/tests/tags.test.js \
            mcp-server/tests/collections.test.js
```

### Run Individual Test Suites

```bash
# Phase 0: Preflight
node --test mcp-server/tests/preflight.test.js

# Phase 2: CRUD (CRITICAL)
node --test mcp-server/tests/crud.test.js

# Phase 3: Search (CRITICAL)
node --test mcp-server/tests/search.test.js

# Phase 4: Tags
node --test mcp-server/tests/tags.test.js

# Phase 5: Collections
node --test mcp-server/tests/collections.test.js
```

### Run with Verbose Output

```bash
node --test --test-reporter=spec mcp-server/tests/crud.test.js
```

## Test Coverage Summary

| Phase | File | Tests | Focus | Priority |
|-------|------|-------|-------|----------|
| 0 | preflight.test.js | 5 | Server connectivity | High |
| 2 | crud.test.js | 11 | Note lifecycle | **CRITICAL** |
| 3 | search.test.js | 12 | Hybrid search | **CRITICAL** |
| 4 | tags.test.js | 11 | Tag management | High |
| 5 | collections.test.js | 12 | Collections | Medium |
| **Total** | **5 files** | **51 tests** | **End-to-end** | - |

## Expected Test Results

### Success Output

```
✔ PREFLIGHT-001: Server info returns server name and version (15ms)
✔ CRUD-001: Create note with content and tags returns ID (42ms)
✔ SEARCH-001: Search notes with text query returns results (67ms)
...
✔ 51 tests complete (3.2s)
```

### Failure Scenarios

Tests are designed to handle and report:

- **404 Not Found** - Non-existent resources
- **400 Bad Request** - Invalid input/empty content
- **Network errors** - Server unavailable
- **Validation errors** - Schema mismatches
- **Permission errors** - Unauthorized access

## Debugging

### Enable Verbose Logging

```javascript
// In test file
console.log("Response:", JSON.stringify(result, null, 2));
```

### Check MCP Server Logs

```bash
# Server logs show incoming requests
tail -f mcp-server/logs/server.log
```

### Inspect Test Data

```bash
# List all test tags in database
curl http://localhost:3000/api/v1/tags | jq '.[] | select(.name | startswith("test/mcp"))'
```

### Manual Cleanup

```bash
# If tests fail to cleanup, manually delete test data
curl -X DELETE http://localhost:3000/api/v1/notes/{note-id}
```

## CI/CD Integration

### GitHub Actions / Gitea Actions

Add to `.gitea/workflows/test.yml`:

```yaml
- name: Run MCP Agentic Tests
  run: |
    # Start API server in background
    cd crates/matric-api
    cargo run &
    API_PID=$!

    # Wait for API to be ready
    sleep 5

    # Start MCP server in background
    cd ../../mcp-server
    npm start &
    MCP_PID=$!

    # Wait for MCP to be ready
    sleep 3

    # Run tests
    node --test tests/preflight.test.js \
                tests/crud.test.js \
                tests/search.test.js \
                tests/tags.test.js \
                tests/collections.test.js

    # Cleanup
    kill $API_PID $MCP_PID
```

### Docker Compose Testing

```yaml
# docker-compose.test.yml
services:
  api:
    build: .
    environment:
      DATABASE_URL: postgres://matric:matric@db/matric

  mcp:
    build: ./mcp-server
    depends_on:
      - api

  test:
    image: node:20
    depends_on:
      - mcp
    command: |
      sh -c "
        cd /app/mcp-server/tests
        node --test preflight.test.js crud.test.js search.test.js tags.test.js collections.test.js
      "
```

## Future Enhancements

### Additional Test Phases

- **Phase 1**: Authentication & Authorization
- **Phase 6**: Templates
- **Phase 7**: Links & Graph
- **Phase 8**: Embeddings & Semantic Search
- **Phase 9**: SKOS Concepts
- **Phase 10**: Document Extraction

### Performance Tests

```javascript
test("PERF-001: Create 100 notes in under 5 seconds", async () => {
  const start = Date.now();
  for (let i = 0; i < 100; i++) {
    await client.callTool("create_note", { content: `Note ${i}` });
  }
  const duration = Date.now() - start;
  assert.ok(duration < 5000, `Took ${duration}ms, expected < 5000ms`);
});
```

### Concurrent Tests

```javascript
test("CONC-001: Concurrent note creation", async () => {
  const promises = Array.from({ length: 10 }, (_, i) =>
    client.callTool("create_note", { content: `Concurrent ${i}` })
  );
  const results = await Promise.all(promises);
  assert.strictEqual(results.length, 10);
});
```

## Contributing

When adding new agentic tests:

1. **Follow naming convention**: `PHASE-NNN: Description`
2. **Use UUID isolation**: `MCPTestClient.uniqueId()` and `testTag()`
3. **Track all created resources** in `cleanup` object
4. **Clean up in `after()` hook** with try/catch
5. **Add test to this README** with description
6. **Verify test passes** before committing

## References

- Test Helper: `/home/roctinam/dev/fortemi/mcp-server/tests/helpers/mcp-client.js`
- MCP Server: `/home/roctinam/dev/fortemi/mcp-server/index.js`
- API Endpoints: `http://localhost:3000/api/v1/*`
- Node.js Test Runner: https://nodejs.org/api/test.html

## License

Same as parent project (see `/home/roctinam/dev/fortemi/LICENSE`)
