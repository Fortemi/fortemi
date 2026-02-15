# UAT Phase 13: Embedding Sets

## Purpose
Validate embedding set management via the `manage_embeddings` consolidated tool (9 actions) and search scoping via the `search` tool's `set` parameter. Covers set CRUD, membership management, search integration, and error handling.

## Duration
~8 minutes

## Prerequisites
- Phases 0-3 completed (system healthy, notes exist)
- MCP server healthy
- At least 3 notes tagged with `uat/embed` created in this phase

## Tools Tested
- `manage_embeddings` (actions: list, get, create, update, delete, list_members, add_members, remove_member, refresh)
- `search` (with `set` parameter)
- `capture_knowledge` (for creating prerequisite notes)

---

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Direct API calls or curl commands are NOT acceptable. If an MCP tool fails or is missing, FILE A BUG ISSUE.

---

## Setup: Create Prerequisite Notes

Before running tests, create 3 notes tagged `uat/embed` for membership testing:

```javascript
const noteIds = [];
for (let i = 1; i <= 3; i++) {
  const result = await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "capture_knowledge",
    arguments: {
      action: "create",
      content: `# UAT Embed Test Note ${i}\n\nThis is embedding set test note number ${i}. It contains unique content for search validation: alpha-${i}-bravo.`,
      tags: ["uat", "uat/embed"],
      revision_mode: "none"
    }
  });
  noteIds.push(result.id);
}
```

**Store**: `embed_note_ids` (array of 3 note UUIDs)

---

## Section 1: Set CRUD (6 tests)

### ESET-001: List Embedding Sets (Default Exists)
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: { action: "list" }
});
```

**Expected**: Returns array of embedding sets including the default set
**Pass Criteria**:
- Response contains an array of sets
- Default embedding set is present
- Each set has `slug`, `name`, and `mode` fields

---

### ESET-002: Create Manual Embedding Set
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "create",
    name: "uat-embed-manual-set",
    description: "UAT test manual embedding set",
    purpose: "Testing embedding set CRUD and membership",
    usage_hints: "Use for UAT validation only",
    keywords: ["uat", "testing", "embeddings"],
    mode: "manual"
  }
});
```

**Expected**: New manual embedding set created
**Pass Criteria**:
- Response confirms creation with a slug
- `mode` is "manual"
- Name, description, purpose match input

**Store**: `manual_set_slug` (slug from response)

---

### ESET-003: Get Set by Slug
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "get",
    slug: manual_set_slug
  }
});
```

**Expected**: Returns full details of the embedding set
**Pass Criteria**:
- `slug` matches `manual_set_slug`
- `name` is "uat-embed-manual-set"
- `mode` is "manual"
- Contains `description`, `purpose`, `usage_hints`, `keywords` fields

---

### ESET-004: Update Set Metadata
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "update",
    slug: manual_set_slug,
    name: "uat-embed-manual-set-updated",
    description: "Updated UAT test embedding set",
    purpose: "Testing embedding set update operations"
  }
});
```

**Expected**: Set metadata updated successfully
**Pass Criteria**:
- Response confirms update
- Subsequent `get` shows updated name, description, and purpose
- Slug remains unchanged

---

### ESET-005: Delete Set and Verify Gone
**MCP Tool**: `manage_embeddings`

```javascript
// Create a temporary set to delete
const tempSet = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "create",
    name: "uat-embed-delete-me",
    description: "Temporary set for deletion test",
    mode: "manual"
  }
});

// Delete it
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "delete",
    slug: tempSet.slug
  }
});

// Verify it's gone (expect error)
const verify = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "get",
    slug: tempSet.slug
  }
});
```

**Expected**: Set deleted, subsequent get returns error or empty
**Pass Criteria**:
- Delete operation completes without errors
- Get after delete returns error (not found) or empty result
- Set no longer appears in `list` results

---

### ESET-006: Create Auto-Mode Set with Tag Criteria
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "create",
    name: "uat-embed-auto-set",
    description: "UAT auto-mode embedding set with tag criteria",
    mode: "auto",
    criteria: {
      tags: ["uat/embed"],
      exclude_archived: true
    }
  }
});
```

**Expected**: Auto-mode set created with tag criteria
**Pass Criteria**:
- Response confirms creation
- `mode` is "auto"
- Criteria includes the tag filter
- Slug is returned

**Store**: `auto_set_slug` (slug from response)

---

## Section 2: Membership Management (5 tests)

### ESET-007: Add Members to Manual Set
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "add_members",
    slug: manual_set_slug,
    note_ids: embed_note_ids,  // All 3 note UUIDs
    added_by: "uat"
  }
});
```

**Expected**: All 3 notes added to the manual set
**Pass Criteria**:
- Response confirms members added
- Added count matches number of note IDs provided (3)
- No errors for any of the note IDs

---

### ESET-008: List Members with Pagination
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "list_members",
    slug: manual_set_slug,
    limit: 2,
    offset: 0
  }
});
```

**Expected**: Returns first 2 members with pagination info
**Pass Criteria**:
- Returns exactly 2 members (limit=2)
- Each member has a `note_id` field
- Response includes pagination metadata (total count >= 3)

---

### ESET-009: Remove Member
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "remove_member",
    slug: manual_set_slug,
    note_id: embed_note_ids[0]  // Remove first note
  }
});
```

**Expected**: First note removed from the set
**Pass Criteria**:
- Response confirms removal
- No error returned
- Removed note ID matches input

---

### ESET-010: Verify Count After Removal
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "list_members",
    slug: manual_set_slug
  }
});
```

**Expected**: 2 members remaining after removal
**Pass Criteria**:
- Returns exactly 2 members
- Removed note ID (`embed_note_ids[0]`) is NOT in the list
- Remaining IDs match `embed_note_ids[1]` and `embed_note_ids[2]`

---

### ESET-011: Refresh Embedding Set
**MCP Tool**: `manage_embeddings`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "refresh",
    slug: manual_set_slug
  }
});
```

**Expected**: Embedding set refreshed (re-embeds members)
**Pass Criteria**:
- Response confirms refresh initiated or completed
- No error returned

---

## Section 3: Search Integration (4 tests)

### ESET-012: Search Scoped to Set Returns Members Only
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "embedding set test note",
    set: manual_set_slug
  }
});
```

**Expected**: Search results limited to notes in the manual set
**Pass Criteria**:
- Results contain only notes that are members of the set
- Note IDs in results are a subset of the set's member IDs
- Results do NOT include `embed_note_ids[0]` (removed in ESET-009)

---

### ESET-013: Search Without Set Returns Broader Results
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "embedding set test note"
  }
});
```

**Expected**: Broader search results including all matching notes
**Pass Criteria**:
- Results include notes from outside the embedding set
- Result count >= set-scoped search count (ESET-012)
- All 3 `embed_note_ids` may appear (including the removed one, since it still exists as a note)

---

### ESET-014: Search with Non-Existent Set Slug
**MCP Tool**: `search`
**Isolation**: Required

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "test",
    set: "nonexistent-xxx-uat"
  }
});
```

**Expected**: Error or empty results for non-existent set
**Pass Criteria**:
- Returns error indicating set not found, OR returns empty results
- Does NOT return unscoped results (must not silently ignore invalid set)

---

### ESET-015: Semantic Search with Set Scope
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "embedding test content",
    mode: "semantic",
    set: manual_set_slug
  }
});
```

**Expected**: Semantic search scoped to the embedding set
**Pass Criteria**:
- Search completes without error
- Results limited to set members
- Mode used is semantic (vector similarity)

---

## Section 4: Error Handling (3 tests)

### ESET-016: Get Non-Existent Slug
**MCP Tool**: `manage_embeddings`
**Isolation**: Required

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "get",
    slug: "nonexistent-xxx-uat"
  }
});
```

**Expected**: Error for non-existent slug
**Pass Criteria**:
- Returns error response (not found)
- Error message indicates slug does not exist
- No crash or unhandled exception

---

### ESET-017: Duplicate Name
**MCP Tool**: `manage_embeddings`
**Isolation**: Required

```javascript
// Attempt to create a set with same name as existing one
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "create",
    name: "uat-embed-manual-set-updated",  // Name from ESET-004
    description: "Duplicate name test",
    mode: "manual"
  }
});
```

**Expected**: Error for duplicate name
**Pass Criteria**:
- Returns error response (conflict or duplicate)
- Error message indicates name already exists
- Original set is not modified

---

### ESET-018: Invalid Action
**MCP Tool**: `manage_embeddings`
**Isolation**: Required

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: {
    action: "bogus"
  }
});
```

**Expected**: Error for invalid action
**Pass Criteria**:
- Returns error response (invalid action or validation error)
- Error message indicates "bogus" is not a valid action
- No crash or unhandled exception

---

## Phase Summary

| Category | Pass | Fail | Skip | Total |
|----------|------|------|------|-------|
| Set CRUD | 0 | 0 | 0 | 6 |
| Membership Management | 0 | 0 | 0 | 5 |
| Search Integration | 0 | 0 | 0 | 4 |
| Error Handling | 0 | 0 | 0 | 3 |
| **Total** | **0** | **0** | **0** | **18** |

## Phase Result
- [ ] **Phase 13 PASSED** - All embedding set tests passed
- [ ] **Phase 13 FAILED** - See failure details above
- [ ] **Phase 13 SKIPPED** - Reason: _______________

## Notes

- All test sets use `uat-embed-` name prefix for easy identification and cleanup
- Phase 14 (Cleanup) handles deletion of all embedding sets with this prefix
- Tests marked with `**Isolation**: Required` must be executed as standalone MCP calls
- The `refresh` action may be asynchronous; verify completion if the system supports it
- Search scoping depends on embeddings being generated; some tests may need retry if embedding is async

---

**End of UAT Phase 13**
