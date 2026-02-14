# UAT Phase 8: Multi-Memory Architecture

**Purpose**: Validate parallel memory archive isolation, switching, and federated search through MCP tools.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 1 completed (default memory has notes)
- System supports multi-memory architecture (MAX_MEMORIES env var set)

**Tools Tested**:
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

### MEM-004: Create Test Archive (Conditional)

**Note**: Archive creation via `POST /api/v1/archives` is NOT available as an MCP core tool. This test documents the workaround for UAT environments.

**Workaround Options**:
1. **Pre-create archive**: Use HTTP API before UAT suite runs
   ```bash
   curl -X POST http://localhost:3000/api/v1/archives \
     -H "Content-Type: application/json" \
     -d '{"name":"test-archive","description":"UAT test memory"}'
   ```

2. **Skip if archive doesn't exist**: Test suite should gracefully handle missing test archive
   ```javascript
   // Attempt to select test archive
   try {
     const response = await use_mcp_tool({
       server_name: "matric-memory",
       tool_name: "select_memory",
       arguments: {
         name: "test-archive"
       }
     });
     console.log("✓ Test archive exists, proceeding with MEM-004");
   } catch (error) {
     console.log("⚠ Test archive not found - skipping MEM-004 through MEM-007");
     console.log("  To enable: POST /api/v1/archives with name='test-archive'");
     // Skip dependent tests
   }
   ```

**Pass Criteria**:
- [ ] Test archive is available OR tests skip gracefully
- [ ] Documentation notes archive creation is API-only
- [ ] Users understand prerequisite for multi-memory tests

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
| Tests Passed | 10 (or 7 if no test-archive) | ___ |
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
- Archive creation requires API access (`POST /api/v1/archives`) - not available via MCP core tools
- `X-Fortemi-Memory` HTTP header selects memory per request (MCP tools handle this internally)
- Federated search is the ONLY way to search across multiple memories
- Memory switching persists for the duration of the MCP session
- Maximum memories configurable via `MAX_MEMORIES` env var (default: 100)

**Test Archive Setup for UAT**:
```bash
# Create test archive before running UAT Phase 8
curl -X POST http://localhost:3000/api/v1/archives \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-archive",
    "description": "UAT test memory for multi-memory validation"
  }'
```

**Cleanup After UAT**:
```bash
# Optional: Remove test archive after UAT completion
# (Currently no delete endpoint - archives are long-lived by design)
# Manual cleanup via direct SQL if needed:
# DROP SCHEMA "test-archive" CASCADE;
```
