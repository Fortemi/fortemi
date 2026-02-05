# UAT Phase 9: Edge Cases

**Purpose**: Verify error handling and edge case behavior
**Duration**: ~5 minutes
**Prerequisites**: None (test data recommended)

> **Test Data**: Pre-built edge case files available in `tests/uat/data/edge-cases/`:
> `empty.txt`, `large-text-100kb.txt`, `binary-wrong-ext.jpg`, `unicode-filename-测试.txt`,
> `whitespace-only.txt`, `malformed-json.json`.
> Generate with: `cd tests/uat/data/scripts && ./generate-test-data.sh`

---

## Input Validation

### EDGE-001: Empty Content

```javascript
create_note({
  content: "",
  tags: ["uat/edge"]
})
```

**Pass Criteria**: Either:
- Returns error with clear message, OR
- Creates note with empty content (depending on design)

**Not Acceptable**: Crash or 500 error

---

### EDGE-002: Very Long Content

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

```javascript
get_note({ id: "not-a-uuid" })
```

**Pass Criteria**: Clear validation error (400 Bad Request)

---

### EDGE-004: Non-existent UUID

```javascript
get_note({ id: "00000000-0000-0000-0000-000000000000" })
```

**Pass Criteria**: 404 Not Found error

---

### EDGE-005: Null Parameters

```javascript
create_note({
  content: null
})
```

**Pass Criteria**: Clear validation error

---

## Security Testing

### EDGE-006: SQL Injection Attempt

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

```javascript
// First: cause an error
get_note({ id: "invalid" })

// Then: normal operation should work
list_notes({ limit: 5 })
```

**Pass Criteria**: System recovers, normal operations work

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| EDGE-001 | Empty Content | |
| EDGE-002 | Very Long Content | |
| EDGE-003 | Invalid UUID | |
| EDGE-004 | Non-existent UUID | |
| EDGE-005 | Null Parameters | |
| EDGE-006 | SQL Injection | |
| EDGE-007 | XSS in Content | |
| EDGE-008 | Path Traversal | |
| EDGE-009 | Rapid Updates | |
| EDGE-010 | Delete During Update | |
| EDGE-011 | Maximum Tags | |
| EDGE-012 | Deeply Nested Tags | |
| EDGE-013 | Unicode Normalization | |
| EDGE-014 | Zero-Width Characters | |
| EDGE-015 | Retry After Error | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
