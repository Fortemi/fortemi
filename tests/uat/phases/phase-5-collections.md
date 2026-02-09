# UAT Phase 5: Collections

**Purpose**: Verify hierarchical folder organization
**Duration**: ~3 minutes
**Prerequisites**: Phase 1 seed data exists
**Tools Tested**: `create_collection`, `list_collections`, `get_collection`, `move_note_to_collection`, `get_collection_notes`, `get_note`, `delete_collection`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Collection CRUD

### COLL-001: Create Collection

**MCP Tool**: `create_collection`

```javascript
create_collection({
  name: "UAT-Test-Collection",
  description: "Test collection for UAT"
})
```

**Pass Criteria**: Returns `{ id: "<uuid>" }`
**Store**: `test_collection_id`

---

### COLL-002: Create Nested Collection

**MCP Tool**: `create_collection`

```javascript
create_collection({
  name: "UAT-Subcollection",
  description: "Nested under test collection",
  parent_id: "<test_collection_id>"
})
```

**Pass Criteria**: Returns ID, collection has `parent_id` set

---

### COLL-003: List Collections

**MCP Tool**: `list_collections`

```javascript
list_collections()
```

**Pass Criteria**: Returns array with UAT collections

---

### COLL-004: List Child Collections

**MCP Tool**: `list_collections`

```javascript
list_collections({ parent_id: "<test_collection_id>" })
```

**Pass Criteria**: Returns only child collections of specified parent

---

### COLL-005: Get Collection

**MCP Tool**: `get_collection`

```javascript
get_collection({ id: "<test_collection_id>" })
```

**Pass Criteria**: Returns full collection details

---

## Note Organization

### COLL-006: Move Note to Collection

**MCP Tool**: `move_note_to_collection`

```javascript
move_note_to_collection({
  note_id: "<seed_ml_note_id>",
  collection_id: "<test_collection_id>"
})
```

**Pass Criteria**: Success response

---

### COLL-007: Get Collection Notes

**MCP Tool**: `get_collection_notes`

```javascript
get_collection_notes({ id: "<test_collection_id>" })
```

**Pass Criteria**: Contains the moved note

---

### COLL-008: Verify Note Collection Assignment

**MCP Tool**: `get_note`

```javascript
get_note({ id: "<moved_note_id>" })
```

**Pass Criteria**: Note shows `collection_id` matching test collection

---

## Collection Deletion

### COLL-009: Delete Empty Collection

**MCP Tool**: `move_note_to_collection`, `delete_collection`

```javascript
// First move note out
move_note_to_collection({ note_id: "<note_id>", collection_id: null })

// Then delete
delete_collection({ id: "<empty_collection_id>" })
```

**Pass Criteria**: Collection deleted successfully

---

### COLL-010a: Delete Collection with Notes — Cascade

**MCP Tool**: `delete_collection`

```javascript
// Delete collection containing notes — notes become unassigned
delete_collection({ id: "<collection_with_notes>" })
```

**Pass Criteria**: Collection deleted. Notes previously in collection have `collection_id` set to null. Notes themselves are NOT deleted.

---

### COLL-010b: Delete Collection with Notes — Reject

**Isolation**: Required — negative test expects error response

**MCP Tool**: `delete_collection`

```javascript
delete_collection({ id: "<collection_with_notes>" })
```

**Pass Criteria**: Returns **409 Conflict** — collection is not empty, requires `force: true` or notes must be moved first.

**Expected: XFAIL** — API currently cascades without requiring force flag.

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| COLL-001 | Create Collection | `create_collection` | |
| COLL-002 | Create Nested Collection | `create_collection` | |
| COLL-003 | List Collections | `list_collections` | |
| COLL-004 | List Child Collections | `list_collections` | |
| COLL-005 | Get Collection | `get_collection` | |
| COLL-006 | Move Note to Collection | `move_note_to_collection` | |
| COLL-007 | Get Collection Notes | `get_collection_notes` | |
| COLL-008 | Verify Note Assignment | `get_note` | |
| COLL-009 | Delete Empty Collection | `move_note_to_collection`, `delete_collection` | |
| COLL-010a | Delete Collection Cascade | `delete_collection` | |
| COLL-010b | Delete Collection Reject (XFAIL) | `delete_collection` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
