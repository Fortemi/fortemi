# UAT Phase 9: Edge Cases

**Purpose**: Verify error handling and edge case behavior
**Duration**: ~5 minutes
**Prerequisites**: None (test data recommended)
**Tools Tested**: `create_note`, `update_note`, `delete_note`, `get_note`, `list_notes`, `search_notes`

> **Test Data**: Pre-built edge case files available in `tests/uat/data/edge-cases/`:
> `empty.txt`, `large-text-100kb.txt`, `binary-wrong-ext.jpg`, `unicode-filename-测试.txt`,
> `whitespace-only.txt`, `malformed-json.json`.
> Generate with: `cd tests/uat/data/scripts && ./generate-test-data.sh`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Input Validation

### EDGE-001a: Empty Content — Accept

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "",
  tags: ["uat/edge"]
})
```

**Pass Criteria**: Creates note with empty content. Returns note_id. No crash or 500 error.

---

### EDGE-002: Very Long Content

**MCP Tool**: `create_note`

```javascript
const longContent = "# Test\n\n" + "Lorem ipsum ".repeat(10000)
create_note({
  content: longContent,
  tags: ["uat/edge"],
  revision_mode: "none"
})
```

**Pass Criteria**: Note created (may be chunked) or clear size limit error

---

### EDGE-003: Invalid UUID

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note`

```javascript
get_note({ id: "not-a-uuid" })
```

**Pass Criteria**: Clear validation error (400 Bad Request)

---

### EDGE-004: Non-existent UUID

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note`

```javascript
get_note({ id: "00000000-0000-0000-0000-000000000000" })
```

**Pass Criteria**: 404 Not Found error

---

### EDGE-005: Null Parameters

**Isolation**: Required — negative test expects error response

**MCP Tool**: `create_note`

```javascript
create_note({
  content: null
})
```

**Pass Criteria**: Returns **400 Bad Request** validation error

---

## Security Testing

### EDGE-006: SQL Injection Attempt

**MCP Tool**: `search_notes`

```javascript
search_notes({
  query: "'; DROP TABLE notes; --",
  mode: "fts"
})
```

**Pass Criteria**: Query treated as literal text, no SQL execution
**Verify**: Notes still exist after query

---

### EDGE-007: XSS in Content

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "<script>alert('xss')</script>",
  tags: ["uat/edge"],
  revision_mode: "none"
})
```

**Pass Criteria**: Content stored (may be escaped), no script execution

---

### EDGE-008: Path Traversal in Metadata

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "Test",
  metadata: { "file": "../../../etc/passwd" },
  revision_mode: "none"
})
```

**Pass Criteria**: Metadata stored as-is, no file system access

---

## Concurrent Operations

### EDGE-009: Rapid Updates

**MCP Tool**: `update_note`

```javascript
// Send 5 rapid updates to same note
for (let i = 0; i < 5; i++) {
  update_note({
    id: "<note_id>",
    content: `Update ${i}`
  })
}
```

**Pass Criteria**: All updates processed, final state is consistent

---

### EDGE-010: Delete During Update

**MCP Tool**: `update_note`, `delete_note`

```javascript
// Start long update
update_note({ id: "<note_id>", content: "...", revision_mode: "full" })

// Immediately delete
delete_note({ id: "<note_id>" })
```

**Pass Criteria**: Either update completes then delete, or delete wins cleanly

---

## Boundary Conditions

### EDGE-011: Maximum Tags

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "Tag limit test",
  tags: Array(100).fill(0).map((_, i) => `uat/tag-${i}`),
  revision_mode: "none"
})
```

**Pass Criteria**: Note created with all tags OR clear limit error

---

### EDGE-012: Deeply Nested Tags

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "Deep tag test",
  tags: ["a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t"],
  revision_mode: "none"
})
```

**Pass Criteria**: Tag created and searchable

---

### EDGE-013: Unicode Normalization

**MCP Tool**: `search_notes`

```javascript
// café in two different Unicode forms
const nfc = "café"  // NFC: é as single codepoint
const nfd = "café"  // NFD: e + combining acute

search_notes({ query: nfc, mode: "fts" })
search_notes({ query: nfd, mode: "fts" })
```

**Pass Criteria**: Both queries return same results

---

### EDGE-014: Zero-Width Characters

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "Test\u200Bcontent\u200B",  // Zero-width spaces
  tags: ["uat/edge"],
  revision_mode: "none"
})
```

**Pass Criteria**: Content stored, searchable by visible text

---

## Error Recovery

### EDGE-015: Retry After Error

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note`, `list_notes`

```javascript
// First: cause an error
get_note({ id: "invalid" })

// Then: normal operation should work
list_notes({ limit: 5 })
```

**Pass Criteria**: First call returns **400 Bad Request**, second call returns **200 OK** — system recovers gracefully

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| EDGE-001a | Empty Content Accept | `create_note` | |
| EDGE-002 | Very Long Content | `create_note` | |
| EDGE-003 | Invalid UUID | `get_note` | |
| EDGE-004 | Non-existent UUID | `get_note` | |
| EDGE-005 | Null Parameters | `create_note` | |
| EDGE-006 | SQL Injection | `search_notes` | |
| EDGE-007 | XSS in Content | `create_note` | |
| EDGE-008 | Path Traversal | `create_note` | |
| EDGE-009 | Rapid Updates | `update_note` | |
| EDGE-010 | Delete During Update | `update_note`, `delete_note` | |
| EDGE-011 | Maximum Tags | `create_note` | |
| EDGE-012 | Deeply Nested Tags | `create_note` | |
| EDGE-013 | Unicode Normalization | `search_notes` | |
| EDGE-014 | Zero-Width Characters | `create_note` | |
| EDGE-015 | Retry After Error | `get_note`, `list_notes` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
