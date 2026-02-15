# UAT Phase 8: Multi-Memory Architecture

**Purpose**: Validate parallel memory archive isolation, switching, and federated search through MCP tools.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 1 completed (default memory has notes)
- System supports multi-memory architecture (MAX_MEMORIES env var set)

**Tools Tested**:
- `manage_archives` (create test archive)
- `select_memory` (switch active memory context)
- `get_active_memory` (query current memory)
- `search` (federated action for cross-archive search)
- `capture_knowledge` (create action, to test per-memory isolation)
- `list_notes` (verify memory-scoped note lists)

> **MCP-First Requirement**: Every test in this phase MUST use MCP tool calls exclusively. No direct HTTP requests, no curl commands. This validates the agent-first workflow that real AI assistants will experience.

---

## Test Cases

### MEM-001: Get Active Memory (Default)

**MCP Tool**: `get_active_memory`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_active_memory",
  arguments: {}
});
```

**Expected Response**:
- Returns current active memory name
- Default memory is "public" (or system default)

**Pass Criteria**:
- [ ] Response contains `name` field
- [ ] Default memory name is returned (typically "public")
- [ ] No error thrown

**Store**: `default_memory_name` for later restoration

---

### MEM-002: Select Default Memory Explicitly

**MCP Tool**: `select_memory`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "public"
  }
});
```

**Expected Response**:
- Confirmation of memory switch
- Active memory is now "public"

**Pass Criteria**:
- [ ] Response confirms memory selected
- [ ] `get_active_memory` returns "public"
- [ ] No errors during switch

---

### MEM-003: Create Note in Default Memory

**MCP Tool**: `capture_knowledge`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Default memory note for multi-memory testing",
    tags: ["uat/memory", "uat/default"],
    revision_mode: "none"
  }
});
```

**Expected Response**:
- Note created in default (public) memory
- Note ID returned

**Pass Criteria**:
- [ ] Response contains note ID
- [ ] Note created successfully
- [ ] Note is in default memory context

**Store**: `default_memory_note_id` for federated search validation

---

### MEM-004: Provision and Select Test Archive

**MCP Tool**: `manage_archives`, `select_memory`

> **Self-Provisioning**: The test creates the archive itself via `manage_archives` — no manual pre-setup required. If the archive already exists (e.g., from PF-006 or a prior UAT run), the error response is safe to ignore.

```javascript
// Step 1: Provision test archive via MCP
const archiveResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_archives",
  arguments: {
    action: "create",
    name: "test-archive",
    description: "UAT test memory for multi-memory validation"
  }
});
// Success or "already exists" error — both are OK

// Step 2: Select the test archive via MCP
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "test-archive"
  }
});
```

**Pass Criteria**:
- [ ] Archive created or already exists (no unexpected error)
- [ ] `select_memory` switches to "test-archive" successfully
- [ ] MEM-005 through MEM-008 can proceed

---

### MEM-005: Verify Memory Switched to Test Archive

**MCP Tool**: `get_active_memory`

**Prerequisites**: MEM-004 succeeded (test-archive exists)

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_active_memory",
  arguments: {}
});
```

**Expected Response**:
- Active memory is "test-archive"
- Memory context persists across tool calls

**Pass Criteria**:
- [ ] Response name is "test-archive"
- [ ] Memory switch was persistent
- [ ] Subsequent operations use test-archive context

---

### MEM-006: Create Note in Test Archive

**MCP Tool**: `capture_knowledge`

**Prerequisites**: MEM-004 succeeded and MEM-005 verified active memory

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Test archive note for isolation validation",
    tags: ["uat/memory", "uat/test-archive"],
    revision_mode: "none"
  }
});
```

**Expected Response**:
- Note created in test-archive memory
- Note ID returned
- Note is isolated from default memory

**Pass Criteria**:
- [ ] Response contains note ID
- [ ] Note created successfully
- [ ] Note exists only in test-archive (not in public)

**Store**: `test_archive_note_id` for isolation verification

---

### MEM-007: Verify Memory Isolation (List Notes)

**MCP Tool**: `list_notes`

**Prerequisites**: MEM-006 succeeded

```javascript
// List notes in test-archive (current active memory)
const test_notes = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: {
    limit: 100,
    tag: "uat/test-archive"
  }
});

// Switch to public memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

// List notes in public memory
const public_notes = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: {
    limit: 100,
    tag: "uat/test-archive"
  }
});
```

**Expected Response**:
- Test-archive contains note with "uat/test-archive" tag
- Public memory does NOT contain that note
- Each memory is fully isolated

**Pass Criteria**:
- [ ] Test-archive list includes test_archive_note_id
- [ ] Public memory list does NOT include test_archive_note_id
- [ ] Data isolation is enforced

---

### MEM-008: Federated Search Across Memories

**MCP Tool**: `search` (federated action)

**Prerequisites**: MEM-003 and MEM-006 succeeded (notes in both memories)

```javascript
// Ensure we have notes in both memories
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "federated",
    query: "memory",
    memories: ["public", "test-archive"],
    limit: 20
  }
});
```

**Expected Response**:
- Search results from BOTH public and test-archive memories
- Each result annotated with source memory
- Results include notes from MEM-003 and MEM-006

**Pass Criteria**:
- [ ] Response contains results array
- [ ] Results include notes from multiple memories
- [ ] Each result indicates source memory (public or test-archive)
- [ ] Default memory note (MEM-003) found
- [ ] Test archive note (MEM-006) found (if archive exists)

---

### MEM-009: Switch Back to Default Memory

**MCP Tool**: `select_memory`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: default_memory_name
  }
});

// Verify switch
const verify = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_active_memory",
  arguments: {}
});
```

**Expected Response**:
- Memory switched back to default
- Active memory confirmed via `get_active_memory`

**Pass Criteria**:
- [ ] Memory switch successful
- [ ] Active memory is default_memory_name
- [ ] Session restored to original state

---

### MEM-010: Select Non-Existent Memory (Error Handling)

**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

**MCP Tool**: `select_memory`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "nonexistent-memory-archive"
  }
});
```

**Expected Response**:
- Error indicating memory not found
- Clear error message

**Pass Criteria**:
- [ ] Tool call fails with appropriate error
- [ ] Error message mentions memory not found
- [ ] Active memory remains unchanged (still default)

---

## Phase Summary

| Metric | Target | Actual |
|--------|--------|--------|
| Tests Executed | 10 | ___ |
| Tests Passed | 10 | ___ |
| Tests Failed | 0 | ___ |
| Duration | ~5 min | ___ |
| Memory Switching Validated | ✓ | ___ |
| Isolation Validated | ✓ | ___ |
| Federated Search Validated | ✓ | ___ |
| Error Handling Validated | ✓ | ___ |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Multi-memory architecture enables parallel data isolation (e.g., personal vs work vs project)
- Each memory maps to a separate PostgreSQL schema
- Default memory is "public" (backward compatible with pre-multi-memory systems)
- Archive creation available via `manage_archives` MCP tool (action: `create`)
- `X-Fortemi-Memory` HTTP header selects memory per request (MCP tools handle this internally)
- Federated search is the ONLY way to search across multiple memories
- Memory switching persists for the duration of the MCP session
- Maximum memories configurable via `MAX_MEMORIES` env var (default: 10)

**Cleanup After UAT**:
```javascript
// Remove test archive via MCP (Phase 14 handles this)
await mcp.call_tool("manage_archives", {
  action: "delete",
  name: "test-archive"
});
```
