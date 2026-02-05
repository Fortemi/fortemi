# MCP Tool Validation Test Suite

Automated test suite for validating MCP tool definitions in Fortémi (Issue #344).

## Overview

This test suite validates all 117+ MCP tools for:
- **Schema correctness** - Valid JSON Schema structures
- **Annotation completeness** - Proper readOnlyHint/destructiveHint annotations
- **Error handling** - Correct HTTP status codes and error messages

## Test Files

### 1. Schema Validation (`schema-validation.test.js`)

Validates that all tool definitions have correct structure:

**Tests:**
- All tools have required fields (name, description, inputSchema)
- Tool names are unique and follow snake_case convention
- InputSchemas use valid JSON Schema structure
- Required fields exist in properties
- All properties have type definitions
- Enum and array properties are properly defined
- UUID fields have format annotations

**Coverage Metrics:**
- 117 tools validated
- 100% have required top-level fields
- 76.9% have required fields
- 45.3% have optional fields
- 2.24 average properties per tool

**Known Issues Found:**
- 1 tool (`knowledge_shard`) missing type field in `include` property
- 17 properties missing description fields (documentation incomplete)

### 2. Annotation Validation (`annotations.test.js`)

Validates that tools have proper MCP annotations for client behavior:

**Annotation Types:**
- `readOnlyHint: true` - Read-only operations (safe to cache)
- `destructiveHint: true` - Destructive operations (require confirmation)
- `destructiveHint: false` - Safe write operations

**Tests:**
- Read-only tools (list_*, get_*, search_*, export_*) have readOnlyHint
- Destructive tools (delete_*, purge_*, wipe_*) have destructiveHint: true
- Safe write tools (create_*, update_*, set_*) have destructiveHint: false
- No tool has both readOnlyHint and destructiveHint
- Annotation values are boolean

**Coverage Metrics:**
- 117 tools (100% annotated)
- 44 read-only tools
- 13 destructive tools
- 26 safe-write tools
- 34 unknown classification

**Known Issues Found:**
- 3 tools incorrectly marked as non-destructive:
  - `delete_note` (should be destructiveHint: true)
  - `delete_collection` (should be destructiveHint: true)
  - `delete_template` (should be destructiveHint: true)

### 3. Error Response (`error-responses.test.js`)

Validates error handling and HTTP status codes:

**Tests:**
- Error messages follow standard format: `API error {status}: {message}`
- 404 Not Found for non-existent resources
- 400 Bad Request for invalid input
- 401 Unauthorized for missing/invalid auth
- 403 Forbidden for access denied
- 500 Internal Server Error for server errors
- Network errors handled gracefully

**Coverage:**
- 5 error types tested
- 4 client errors (4xx)
- 1 server error (5xx)
- 14 example error messages
- 21 test cases (all passing)

## Running Tests

### Run All Tests
```bash
npm test
```

### Run Individual Test Suites
```bash
npm run test:schema        # Schema validation
npm run test:annotations   # Annotation validation
npm run test:errors        # Error response validation
```

### Run Connectivity Tests (Legacy)
```bash
npm run test:connectivity  # Test MCP server connectivity
npm run test:local         # Test against local instance
```

## Test Results Summary

| Metric | Value | Status |
|--------|-------|--------|
| Total Tests | 44 | ✓ |
| Passing | 40 | ✓ |
| Failing | 4 | ⚠️ |
| Test Suites | 19 | ✓ |
| Tools Validated | 117 | ✓ |
| Schema Coverage | 100% | ✓ |
| Annotation Coverage | 100% | ✓ |
| Error Coverage | 5 status codes | ✓ |

## Test Data & Fixtures

### Schema Fixtures

```javascript
const validToolExample = {
  name: "example_tool",
  description: "Example tool description",
  inputSchema: {
    type: "object",
    properties: {
      id: { type: "string", description: "Example ID" }
    },
    required: ["id"]
  }
};
```

### Annotation Fixtures

```javascript
const READ_ONLY_PATTERNS = {
  prefixes: ["list_", "get_", "search_", "export_"],
  exact: ["health_check", "get_queue_stats"]
};

const DESTRUCTIVE_PATTERNS = {
  prefixes: ["delete_", "purge_", "remove_", "wipe_"],
  exact: []
};
```

### Error Fixtures

```javascript
const ERROR_FIXTURES = {
  not_found: { status: 404, message: "Resource not found" },
  bad_request: { status: 400, message: "Invalid input" },
  unauthorized: { status: 401, message: "Authentication required" },
  forbidden: { status: 403, message: "Access denied" },
  internal_error: { status: 500, message: "Internal server error" }
};
```

## Mock API Request Function

All tests use a mock API request function to avoid external dependencies:

```javascript
function createMockApiRequest(scenario = "success") {
  return async (method, path, body = null) => {
    switch (scenario) {
      case "not_found":
        throw new Error("API error 404: Resource not found");
      case "invalid_uuid":
        throw new Error("API error 400: Invalid UUID format");
      case "success":
        return { id: "uuid", content: "test", tags: [] };
      // ... other scenarios
    }
  };
}
```

## Issues Found

### Critical (Must Fix)

1. **Missing Type Definitions** (1 tool)
   - `knowledge_shard.include` property missing `type` field
   - Violates JSON Schema standard
   - **Fix:** Add `type: "object"` or appropriate type

2. **Incorrect Destructive Annotations** (3 tools)
   - `delete_note`, `delete_collection`, `delete_template`
   - Currently marked as `destructiveHint: false` (safe)
   - Should be `destructiveHint: true` (destructive)
   - **Impact:** MCP clients won't ask for confirmation before deletion

### Minor (Should Fix)

3. **Missing Property Descriptions** (17 properties)
   - Optional properties without description fields
   - Reduces API documentation quality
   - **Fix:** Add description strings to all properties

4. **Missing UUID Format Annotations** (52 properties)
   - UUID properties without `format: "uuid"` annotation
   - Reduces schema validation quality
   - **Fix:** Add `format: "uuid"` to ID fields

## Test Architecture

### Design Principles

1. **No External Dependencies** - Uses Node.js built-in test runner
2. **Static Analysis** - Parses tool definitions without server startup
3. **Comprehensive Fixtures** - Extensive test data for edge cases
4. **Clear Reporting** - Detailed statistics and issue lists
5. **Fast Execution** - All tests run in <100ms

### Test Structure

```
tests/
├── schema-validation.test.js    # JSON Schema validation
├── annotations.test.js          # Annotation validation
├── error-responses.test.js      # Error handling validation
├── observability_tools_test.js  # Legacy observability tests
├── collection_filters_test.js   # Legacy collection tests
├── issue_features_test.js       # Legacy issue tests
└── README.md                     # This file
```

## Integration with CI/CD

### GitHub Actions

Add to `.github/workflows/test.yml`:

```yaml
- name: MCP Tool Validation
  run: |
    cd mcp-server
    npm test
```

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
cd mcp-server && npm test
if [ $? -ne 0 ]; then
  echo "MCP tool validation failed"
  exit 1
fi
```

## Coverage Goals

| Category | Current | Target | Status |
|----------|---------|--------|--------|
| Schema Structure | 100% | 100% | ✓ |
| Type Definitions | 99.1% | 100% | ⚠️ |
| Property Docs | 85.5% | 95% | ⚠️ |
| Annotations | 100% | 100% | ✓ |
| Correct Annotations | 97.4% | 100% | ⚠️ |
| Error Handling | 5 codes | 7 codes | 🔄 |

## Future Enhancements

### Planned Tests

1. **Integration Tests**
   - Test actual API requests against live server
   - Validate response formats match schemas
   - Test end-to-end tool execution

2. **Performance Tests**
   - Tool execution latency benchmarks
   - Concurrent request handling
   - Memory usage profiling

3. **Security Tests**
   - SQL injection prevention
   - XSS prevention in markdown
   - Authorization enforcement

4. **Regression Tests**
   - Golden test set for search results
   - Fixed test data for deterministic results
   - Snapshot testing for tool outputs

### Additional Error Codes

- 409 Conflict (duplicate resources)
- 422 Unprocessable Entity (semantic validation)
- 429 Too Many Requests (rate limiting)
- 503 Service Unavailable (maintenance mode)

## References

- [MCP Tool Annotation Spec](https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/)
- [JSON Schema Specification](https://json-schema.org/specification.html)
- [HTTP Status Codes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status)
- [Node.js Test Runner](https://nodejs.org/api/test.html)

## Contributing

When adding new MCP tools:

1. **Run tests** before committing: `npm test`
2. **Add schemas** for all properties with type and description
3. **Add annotations** appropriate to tool behavior
4. **Handle errors** with proper HTTP status codes
5. **Update tests** if adding new patterns or edge cases

## License

Same as parent project (see `../LICENSE`)
