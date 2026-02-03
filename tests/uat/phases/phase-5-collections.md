# UAT Phase 5: Collections

**Purpose**: Verify hierarchical folder organization
**Duration**: ~3 minutes
**Prerequisites**: Phase 1 seed data exists

---

## Collection CRUD

### COLL-001: Create Collection

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

```javascript
list_collections()
```

**Pass Criteria**: Returns array with UAT collections

---

### COLL-004: List Child Collections

```javascript
list_collections({ parent_id: "<test_collection_id>" })
```

**Pass Criteria**: Returns only child collections of specified parent

---

### COLL-005: Get Collection

```javascript
get_collection({ id: "<test_collection_id>" })
```

**Pass Criteria**: Returns full collection details

---

## Note Organization

### COLL-006: Move Note to Collection

```javascript
move_note_to_collection({
  note_id: "<seed_ml_note_id>",
  collection_id: "<test_collection_id>"
})
```

**Pass Criteria**: Success response

---

### COLL-007: Get Collection Notes

```javascript
get_collection_notes({ id: "<test_collection_id>" })
```

**Pass Criteria**: Contains the moved note

---

### COLL-008: Verify Note Collection Assignment

```javascript
get_note({ id: "<moved_note_id>" })
```

**Pass Criteria**: Note shows `collection_id` matching test collection

---

## Collection Deletion

### COLL-009: Delete Empty Collection

```javascript
// First move note out
move_note_to_collection({ note_id: "<note_id>", collection_id: null })

// Then delete
delete_collection({ id: "<empty_collection_id>" })
```

**Pass Criteria**: Collection deleted successfully

---

### COLL-010: Delete Collection with Notes (Behavior)

```javascript
// Try to delete collection containing notes
delete_collection({ id: "<collection_with_notes>" })
```

**Pass Criteria**: Either fails with error OR notes become unassigned

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| COLL-001 | Create Collection | |
| COLL-002 | Create Nested Collection | |
| COLL-003 | List Collections | |
| COLL-004 | List Child Collections | |
| COLL-005 | Get Collection | |
| COLL-006 | Move Note to Collection | |
| COLL-007 | Get Collection Notes | |
| COLL-008 | Verify Note Assignment | |
| COLL-009 | Delete Empty Collection | |
| COLL-010 | Delete Collection with Notes | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
