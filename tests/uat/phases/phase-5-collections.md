# UAT Phase 5: Collections

**Purpose**: Validate collection management including creation, updates, note organization, and export.

**Duration**: ~5 minutes

**Prerequisites**: Phase 1 completion (notes must exist for collection operations)

**Tools Tested**: `manage_collection` (8 actions: list, create, get, update, delete, list_notes, move_note, export)

> **MCP-First Requirement**: All tests in this phase use MCP tool calls exclusively. No direct HTTP requests are permitted. The `manage_collection` tool provides complete access to the hierarchical folder system.

---

## Test Cases

### COL-001: List Collections

**Test ID**: COL-001
**MCP Tool**: `manage_collection` (action: list)
**Description**: List all collections in the system

```javascript
const result = await useTool('manage_collection', {
  action: 'list'
});
```

**Expected Response**:
- Array of collection objects
- Each collection has `id`, `name`, `description` fields
- May be empty if no collections exist

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Collection structure includes core fields
- [ ] No error if empty
- [ ] Handles zero collections gracefully

---

### COL-002: Create Collection

**Test ID**: COL-002
**MCP Tool**: `manage_collection` (action: create)
**Description**: Create a new collection

```javascript
const result = await useTool('manage_collection', {
  action: 'create',
  name: 'uat-test-collection',
  description: 'UAT test collection for Phase 5'
});
```

**Expected Response**:
- Newly created collection object
- Contains assigned `id`
- Name and description match input

**Pass Criteria**:
- [ ] Returns valid JSON object
- [ ] `id` field present and non-null
- [ ] `name` equals "uat-test-collection"
- [ ] `description` matches input

**Store**: `collection_id` (for subsequent tests)

---

### COL-003: Get Collection

**Test ID**: COL-003
**MCP Tool**: `manage_collection` (action: get)
**Description**: Retrieve collection by ID

```javascript
const result = await useTool('manage_collection', {
  action: 'get',
  id: collection_id
});
```

**Expected Response**:
- Single collection object
- ID matches requested collection
- Includes metadata and note count

**Pass Criteria**:
- [ ] Returns valid JSON object
- [ ] `id` matches `collection_id`
- [ ] `name` equals "uat-test-collection"
- [ ] Metadata fields present

---

### COL-004: Update Collection

**Test ID**: COL-004
**MCP Tool**: `manage_collection` (action: update)
**Description**: Update collection name

```javascript
const result = await useTool('manage_collection', {
  action: 'update',
  id: collection_id,
  name: 'uat-renamed-collection'
});
```

**Expected Response**:
- Updated collection object
- Name changed to new value
- ID remains unchanged

**Pass Criteria**:
- [ ] Returns valid JSON object
- [ ] `id` still matches `collection_id`
- [ ] `name` equals "uat-renamed-collection"
- [ ] Update confirmed

---

### COL-005: Move Note to Collection

**Test ID**: COL-005
**MCP Tool**: `manage_collection` (action: move_note)
**Description**: Move a note into the collection
**Store**: `note_id` (from Phase 1)

```javascript
const result = await useTool('manage_collection', {
  action: 'move_note',
  note_id: note_id,
  collection_id: collection_id
});
```

**Expected Response**:
- Success response confirming move
- Note now associated with collection

**Pass Criteria**:
- [ ] Returns success status
- [ ] No error messages
- [ ] Move operation acknowledged
- [ ] Note ID and collection ID match

---

### COL-006: List Collection Notes

**Test ID**: COL-006
**MCP Tool**: `manage_collection` (action: list_notes)
**Description**: List all notes in the collection

```javascript
const result = await useTool('manage_collection', {
  action: 'list_notes',
  id: collection_id
});
```

**Expected Response**:
- Array of note objects in collection
- Contains the note moved in COL-005
- Note count ≥ 1

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Array contains at least 1 note
- [ ] Moved note appears in results
- [ ] Note structure includes `id`, `title`

---

### COL-007: Export Collection

**Test ID**: COL-007
**MCP Tool**: `manage_collection` (action: export)
**Description**: Export collection to markdown archive

```javascript
const result = await useTool('manage_collection', {
  action: 'export',
  id: collection_id
});
```

**Expected Response**:
- Markdown archive content or download link
- Includes all notes from collection
- Valid markdown formatting

**Pass Criteria**:
- [ ] Returns valid export data
- [ ] Contains markdown content
- [ ] Includes notes from collection
- [ ] No corruption or errors

---

### COL-008: Delete Collection

**Test ID**: COL-008
**MCP Tool**: `manage_collection` (action: delete)
**Description**: Delete the test collection

```javascript
const result = await useTool('manage_collection', {
  action: 'delete',
  id: collection_id
});
```

**Expected Response**:
- Success response confirming deletion
- Collection removed from system

**Pass Criteria**:
- [ ] Returns success status
- [ ] No error messages
- [ ] Deletion confirmed
- [ ] Collection ID acknowledged

**Store**: `deleted_collection_id` (same as `collection_id`, for COL-009)

---

### COL-009: Get Deleted Collection Error

**Test ID**: COL-009
**MCP Tool**: `manage_collection` (action: get)
**Description**: Verify deleted collection returns error
**Isolation**: Required

```javascript
const result = await useTool('manage_collection', {
  action: 'get',
  id: deleted_collection_id
});
```

**Expected Response**:
- Error response indicating collection not found
- HTTP 404 or similar

**Pass Criteria**:
- [ ] Returns error (not success)
- [ ] Error indicates collection not found
- [ ] HTTP 404 status or equivalent
- [ ] Deletion confirmed by error

---

### COL-010: Invalid Action

**Test ID**: COL-010
**MCP Tool**: `manage_collection` (action: fly)
**Description**: Validate error handling for invalid action
**Isolation**: Required

```javascript
const result = await useTool('manage_collection', {
  action: 'fly',
  id: collection_id
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

## Phase 5 Summary

| Category | Count | Pass | Fail |
|----------|-------|------|------|
| List Collections | 1 | - | - |
| Create Collection | 1 | - | - |
| Get Collection | 1 | - | - |
| Update Collection | 1 | - | - |
| Move Note | 1 | - | - |
| List Notes | 1 | - | - |
| Export Collection | 1 | - | - |
| Delete Collection | 1 | - | - |
| Error Handling | 2 | - | - |
| **Total** | **10** | **-** | **-** |

**Phase 5 Result**: [ ] PASS [ ] FAIL

**Notes**:
- Collection deletion does NOT cascade to notes (notes remain, just unassociated)
- Export format is markdown with YAML frontmatter
- Collections support hierarchical nesting (not tested in this phase)
- `move_note` replaces previous collection association
- Phase 1 seed notes used for collection membership tests
