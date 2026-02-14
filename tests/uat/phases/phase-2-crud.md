# UAT Phase 2: Notes CRUD

**Purpose**: Verify complete lifecycle management of notes including retrieval, listing, updating, archiving, deletion, and restoration.

**Duration**: ~8 minutes

**Prerequisites**:
- Phase 0 (Preflight & System) completed successfully
- Phase 1 (Knowledge Capture) completed successfully
- At least one note created with `basic_note_id` available

**Tools Tested**: `list_notes`, `get_note`, `update_note`, `delete_note`, `restore_note`

---

> **MCP-First Requirement**
>
> All tests use dedicated CRUD tools (not consolidated actions). Tests verify data integrity, pagination, filtering, and soft-delete/restore workflows. Notes created in Phase 1 are reused where possible.

---

## Test Cases

### CRUD-001: Get Note by ID
**MCP Tool**: `get_note`

```javascript
// Use basic_note_id from Phase 1 (CK-001)
const result = await mcp.call_tool("get_note", {
  id: basic_note_id
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-phase-1",
  "content": "# UAT Test Note\n\nThis is a basic test note created during Phase 1.",
  "tags": ["uat/capture"],
  "created_at": "2026-02-14T...",
  "updated_at": "2026-02-14T...",
  "starred": false,
  "archived": false
}
```

**Pass Criteria**:
- [ ] Response contains complete note object
- [ ] ID matches input parameter
- [ ] Content matches original from Phase 1
- [ ] Tags array preserved
- [ ] Boolean flags (starred, archived) are present

---

### CRUD-002: Get Non-Existent Note
**MCP Tool**: `get_note`

**Isolation**: Required

```javascript
try {
  await mcp.call_tool("get_note", {
    id: "00000000-0000-0000-0000-000000000000"
  });
  console.error("FAIL: Expected 404 error for non-existent note");
} catch (error) {
  console.log("PASS: Error caught as expected");
  console.log("Error type:", error.code || error.status);
}
```

**Expected Response**:
```json
{
  "error": "Note not found",
  "status": 404
}
```

**Pass Criteria**:
- [ ] Tool call throws or returns error
- [ ] Error indicates resource not found (404 or equivalent)
- [ ] Error message clearly states note doesn't exist
- [ ] No partial data returned

---

### CRUD-003: List Notes Basic
**MCP Tool**: `list_notes`

```javascript
const result = await mcp.call_tool("list_notes", {
  limit: 10
});
```

**Expected Response**:
```json
{
  "notes": [
    { "id": "uuid-1", "content": "...", "tags": [...] },
    { "id": "uuid-2", "content": "...", "tags": [...] }
  ],
  "total": 15,
  "offset": 0,
  "limit": 10
}
```

**Pass Criteria**:
- [ ] Response contains `notes` array
- [ ] Array length ≤ limit parameter
- [ ] `total` count indicates total available notes
- [ ] `offset` and `limit` echo request parameters
- [ ] Notes from Phase 1 present in results

---

### CRUD-004: List Notes with Tag Filter
**MCP Tool**: `list_notes`

```javascript
const result = await mcp.call_tool("list_notes", {
  tags: ["uat/capture"],
  limit: 20
});
```

**Expected Response**:
```json
{
  "notes": [
    { "id": "uuid-1", "tags": ["uat/capture", ...] },
    { "id": "uuid-2", "tags": ["uat/capture", "uat/metadata"] }
  ],
  "total": 8,
  "offset": 0,
  "limit": 20
}
```

**Pass Criteria**:
- [ ] All returned notes contain "uat/capture" tag
- [ ] Notes without this tag are excluded
- [ ] Total count reflects filtered results
- [ ] At least notes from CK-001 through CK-005 are included

---

### CRUD-005: List Notes with Hierarchical Tag Filter
**MCP Tool**: `list_notes`

```javascript
const result = await mcp.call_tool("list_notes", {
  tags: ["uat"],
  limit: 50
});
```

**Expected Response**:
```json
{
  "notes": [
    { "tags": ["uat/capture"] },
    { "tags": ["uat/capture/hierarchy/deep/nested"] },
    { "tags": ["uat/metadata"] }
  ],
  "total": 12
}
```

**Pass Criteria**:
- [ ] All notes with tags starting with "uat/" are returned
- [ ] Hierarchical prefix matching works correctly
- [ ] Note from CK-003 with deep hierarchy included
- [ ] Total count includes all matching notes

---

### CRUD-006: Pagination
**MCP Tool**: `list_notes`

```javascript
// Page 1
const page1 = await mcp.call_tool("list_notes", {
  tags: ["uat/capture"],
  offset: 0,
  limit: 5
});

// Page 2
const page2 = await mcp.call_tool("list_notes", {
  tags: ["uat/capture"],
  offset: 5,
  limit: 5
});

// Verify no overlap
const ids1 = page1.notes.map(n => n.id);
const ids2 = page2.notes.map(n => n.id);
const overlap = ids1.filter(id => ids2.includes(id));
console.assert(overlap.length === 0, "Pages should not overlap");
```

**Expected Response** (page 1):
```json
{
  "notes": [ /* 5 notes */ ],
  "total": 12,
  "offset": 0,
  "limit": 5
}
```

**Expected Response** (page 2):
```json
{
  "notes": [ /* up to 5 notes */ ],
  "total": 12,
  "offset": 5,
  "limit": 5
}
```

**Pass Criteria**:
- [ ] Page 1 returns first 5 results
- [ ] Page 2 returns next batch with offset 5
- [ ] No note IDs overlap between pages
- [ ] Both pages report same total count
- [ ] Combined results ≤ total count

---

### CRUD-007: Limit Zero Returns Metadata Only
**MCP Tool**: `list_notes`

```javascript
const result = await mcp.call_tool("list_notes", {
  tags: ["uat/capture"],
  offset: 0,
  limit: 0
});
```

**Expected Response**:
```json
{
  "notes": [],
  "total": 12,
  "offset": 0,
  "limit": 0
}
```

**Pass Criteria**:
- [ ] `notes` array is empty
- [ ] `total` count is accurate (not zero)
- [ ] Useful for counting without fetching data
- [ ] No error thrown

---

### CRUD-008: Update Note Content
**MCP Tool**: `update_note`

```javascript
// Use basic_note_id from Phase 1
const result = await mcp.call_tool("update_note", {
  id: basic_note_id,
  content: "# UPDATED UAT Test Note\n\nThis content has been modified in Phase 2."
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-phase-1",
  "content": "# UPDATED UAT Test Note\n\nThis content has been modified in Phase 2.",
  "tags": ["uat/capture"],
  "updated_at": "2026-02-14T...(later timestamp)"
}
```

**Pass Criteria**:
- [ ] Content updated successfully
- [ ] Response reflects new content
- [ ] `updated_at` timestamp changed
- [ ] Tags preserved (not modified)
- [ ] Other fields unchanged

---

### CRUD-009: Star Note
**MCP Tool**: `update_note`

```javascript
const result = await mcp.call_tool("update_note", {
  id: basic_note_id,
  starred: true
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-phase-1",
  "starred": true,
  "updated_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] `starred` field set to true
- [ ] Note content unchanged
- [ ] `updated_at` timestamp updated
- [ ] Note retrievable with starred filter (if supported)

**Store**: Note is now starred for subsequent tests

---

### CRUD-010: Archive Note
**MCP Tool**: `update_note`

```javascript
// Use metadata_note_id from CK-002
const result = await mcp.call_tool("update_note", {
  id: metadata_note_id,
  archived: true
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-ck-002",
  "archived": true,
  "updated_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] `archived` field set to true
- [ ] Note content unchanged
- [ ] Note excluded from default list results (unless include_archived specified)
- [ ] Note still retrievable by ID

**Store**: `archived_note_id` for verification test

---

### CRUD-011: Update Note Metadata
**MCP Tool**: `update_note`

```javascript
// Use metadata_note_id from CK-002
const result = await mcp.call_tool("update_note", {
  id: metadata_note_id,
  metadata: {
    test_phase: "phase-2",
    priority: "critical",
    status: "in_progress"
  }
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-ck-002",
  "metadata": {
    "test_phase": "phase-2",
    "priority": "critical",
    "status": "in_progress"
  },
  "updated_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] Metadata completely replaced with new object
- [ ] All three new fields present
- [ ] Previous metadata fields removed (test_phase changed, custom_field gone)
- [ ] Note content unchanged

---

### CRUD-012: Soft Delete Note
**MCP Tool**: `delete_note`

```javascript
// Use one of the bulk_note_ids from CK-005
const delete_target_id = bulk_note_ids[0];
const result = await mcp.call_tool("delete_note", {
  id: delete_target_id
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-bulk-notes",
  "deleted": true,
  "deleted_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] Response confirms deletion
- [ ] Note marked as deleted (soft delete)
- [ ] Timestamp recorded
- [ ] Note data not permanently destroyed

**Store**: `deleted_note_id` for restore test

---

### CRUD-013: Verify Deleted Note Excluded from List
**MCP Tool**: `list_notes`

```javascript
const result = await mcp.call_tool("list_notes", {
  tags: ["uat/bulk"],
  limit: 10
});

// Verify deleted note is not in results
const ids = result.notes.map(n => n.id);
console.assert(!ids.includes(deleted_note_id), "Deleted note should not appear in list");
console.log("Notes remaining with uat/bulk tag:", result.total);
```

**Expected Response**:
```json
{
  "notes": [
    { "id": "uuid-2", "tags": ["uat/capture", "uat/bulk"] },
    { "id": "uuid-3", "tags": ["uat/capture", "uat/bulk"] }
  ],
  "total": 2
}
```

**Pass Criteria**:
- [ ] Deleted note ID not present in results
- [ ] Total count decremented by 1 (from 3 to 2)
- [ ] Other bulk notes still listed
- [ ] No error accessing list

---

### CRUD-014: Restore Deleted Note
**MCP Tool**: `restore_note`

```javascript
const result = await mcp.call_tool("restore_note", {
  id: deleted_note_id
});
```

**Expected Response**:
```json
{
  "id": "uuid-from-deleted-note",
  "restored": true,
  "deleted_at": null,
  "updated_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] Response confirms restoration
- [ ] `deleted_at` field cleared or null
- [ ] Note becomes visible in list queries again
- [ ] `updated_at` timestamp reflects restoration time

---

### CRUD-015: Verify Restored Note Content Preserved
**MCP Tool**: `get_note`

```javascript
const result = await mcp.call_tool("get_note", {
  id: deleted_note_id
});

// Verify original content intact
console.assert(result.content.includes("Bulk Note"), "Original content preserved");
console.assert(result.tags.includes("uat/bulk"), "Original tags preserved");
console.log("Restored note content:", result.content);
```

**Expected Response**:
```json
{
  "id": "uuid-from-deleted-note",
  "content": "# Bulk Note 1\n\nFirst note in batch.",
  "tags": ["uat/capture", "uat/bulk"],
  "deleted_at": null
}
```

**Pass Criteria**:
- [ ] Original content unchanged
- [ ] Tags preserved exactly
- [ ] Metadata (if any) preserved
- [ ] No data loss from delete/restore cycle
- [ ] Timestamps updated appropriately

---

## Phase Summary

| Test ID | Tool | Focus | Result |
|---------|------|-------|--------|
| CRUD-001 | get_note | Retrieve by ID | ⬜ |
| CRUD-002 | get_note | 404 handling | ⬜ |
| CRUD-003 | list_notes | Basic listing | ⬜ |
| CRUD-004 | list_notes | Tag filtering | ⬜ |
| CRUD-005 | list_notes | Hierarchical tag filter | ⬜ |
| CRUD-006 | list_notes | Pagination | ⬜ |
| CRUD-007 | list_notes | Limit zero (count only) | ⬜ |
| CRUD-008 | update_note | Content update | ⬜ |
| CRUD-009 | update_note | Star note | ⬜ |
| CRUD-010 | update_note | Archive note | ⬜ |
| CRUD-011 | update_note | Metadata update | ⬜ |
| CRUD-012 | delete_note | Soft delete | ⬜ |
| CRUD-013 | list_notes | Deleted note excluded | ⬜ |
| CRUD-014 | restore_note | Note restoration | ⬜ |
| CRUD-015 | get_note | Restored content intact | ⬜ |

**Phase Result**: ⬜ PASS / ⬜ FAIL

**Notes**:
