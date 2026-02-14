# UAT Phase 11: Edge Cases & Error Handling

## Purpose
Test error handling, boundary conditions, and resilience across core MCP tools to ensure graceful degradation and proper error reporting.

## Duration
~5 minutes

## Prerequisites
- MCP server healthy
- Default memory active
- Phase 1 (Smoke Test) passed

## Tools Tested
- `capture_knowledge`
- `update_note`
- `delete_note`
- `restore_note`
- `search`
- `manage_tags`

---

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Direct API calls or curl commands are NOT acceptable. All note creation uses `revision_mode: "none"` and tags with `uat/` prefix.

---

## Test Cases

### EDGE-001: Empty Content Note
**MCP Tool**: `capture_knowledge`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "",
    tags: ["uat/edge-cases", "uat/empty"],
    revision_mode: "none"
  }
});
```

**Expected**: Either succeeds (empty notes allowed) or returns clear error message
**Pass Criteria**:
- No server crash or 500 error
- If success: response contains `note_id`
- If error: response contains actionable error message

**Store**: `empty_note_id` (if successful)

---

### EDGE-002: Very Long Content
**MCP Tool**: `capture_knowledge`

```javascript
const longContent = "A".repeat(50000); // 50KB content
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: longContent,
    tags: ["uat/edge-cases", "uat/large"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created successfully or clear size limit error
**Pass Criteria**:
- Response contains `note_id` OR clear size limit message
- No timeout or server crash
- Content length preserved if successful

**Store**: `large_note_id`

---

### EDGE-003: Unicode Content
**MCP Tool**: `capture_knowledge`

```javascript
const unicodeContent = `
# Unicode Test 测试
CJK: 日本語 中文 한국어
Emoji: 🚀 ✅ 🎯 💡
RTL: مرحبا العالم
Math: ∑ ∫ π √
Symbols: © ® ™ ¥ €
`;

const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: unicodeContent,
    tags: ["uat/edge-cases", "uat/unicode"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created with Unicode preserved exactly
**Pass Criteria**:
- Response contains `note_id`
- Unicode characters not corrupted or stripped
- Search can find note by CJK characters

**Store**: `unicode_note_id`

---

### EDGE-004: Special Characters in Tags
**MCP Tool**: `manage_tags`

```javascript
// First create a note to tag
const note = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Test note for special tag characters",
    tags: ["uat/edge-cases"],
    revision_mode: "none"
  }
});

const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_tags",
  arguments: {
    action: "set",
    note_id: note.note_id,
    tags: ["uat/special-chars!@#", "uat/dots.dots", "uat/under_score"]
  }
});
```

**Expected**: Either tags accepted with special chars OR clear validation error
**Pass Criteria**:
- No server crash
- If success: tags stored and retrievable
- If error: clear validation message about disallowed characters

---

### EDGE-005: Update Non-Existent Note
**MCP Tool**: `update_note`
**Isolation**: Required

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "update_note",
  arguments: {
    id: "00000000-0000-0000-0000-000000000000",
    content: "This note does not exist"
  }
});
```

**Expected**: Clear "note not found" error (404)
**Pass Criteria**:
- Error response contains "not found" or similar message
- Error clearly identifies the missing note ID
- No server crash

---

### EDGE-006: Delete Non-Existent Note
**MCP Tool**: `delete_note`
**Isolation**: Required

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "delete_note",
  arguments: {
    id: "00000000-0000-0000-0000-000000000001"
  }
});
```

**Expected**: Clear "note not found" error (404) OR successful idempotent deletion
**Pass Criteria**:
- No server crash
- Response indicates note doesn't exist OR deletion succeeded
- Consistent behavior with database state

---

### EDGE-007: Double Delete
**MCP Tool**: `delete_note`
**Isolation**: Required

```javascript
// Create a note to delete
const note = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Note to be deleted twice",
    tags: ["uat/edge-cases", "uat/double-delete"],
    revision_mode: "none"
  }
});

// First delete
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "delete_note",
  arguments: { id: note.note_id }
});

// Second delete
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "delete_note",
  arguments: { id: note.note_id }
});
```

**Expected**: Second delete either succeeds idempotently OR returns "already deleted" error
**Pass Criteria**:
- No server crash
- Second delete does not resurrect the note
- Error message (if any) clearly indicates already-deleted state

---

### EDGE-008: Restore Non-Deleted Note
**MCP Tool**: `restore_note`
**Isolation**: Required

```javascript
// Create an active note (not deleted)
const note = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Active note, never deleted",
    tags: ["uat/edge-cases", "uat/restore-test"],
    revision_mode: "none"
  }
});

// Attempt to restore it
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "restore_note",
  arguments: {
    id: note.note_id
  }
});
```

**Expected**: Either idempotent success OR clear "note not deleted" error
**Pass Criteria**:
- No server crash
- Note remains active and unchanged
- Response clearly indicates restore not needed OR succeeded idempotently

---

### EDGE-009: Search with Empty Query
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: ""
  }
});
```

**Expected**: Either returns all notes (empty query = match all) OR validation error
**Pass Criteria**:
- No server crash
- If returns results: paginated list of notes
- If error: clear message about empty query requirement
- Behavior documented and consistent

---

### EDGE-010: Bulk Create Empty Array
**MCP Tool**: `capture_knowledge`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "bulk_create",
    notes: []
  }
});
```

**Expected**: Either succeeds with empty results OR validation error
**Pass Criteria**:
- No server crash
- If success: returns empty array or zero count
- If error: clear message about empty bulk operation
- Idempotent behavior

---

## Phase Summary

| Category | Pass | Fail | Skip | Total |
|----------|------|------|------|-------|
| Boundary Conditions | 0 | 0 | 0 | 4 |
| Error Handling | 0 | 0 | 0 | 5 |
| Input Validation | 0 | 0 | 0 | 1 |
| **Total** | **0** | **0** | **0** | **10** |

## Phase Result
- [ ] **Phase 11 PASSED** - All edge cases handled gracefully
- [ ] **Phase 11 FAILED** - See failure details above
- [ ] **Phase 11 SKIPPED** - Reason: _______________
