# UAT Phase 2: CRUD Operations

**Purpose**: Verify core Create, Read, Update, Delete operations
**Duration**: ~10 minutes
**Prerequisites**: Phase 1 completed
**Critical**: Yes (100% pass required)

---

## Create Operations

### CRUD-001: Create Note - Basic

```javascript
create_note({
  content: "# UAT Test Note\n\nThis is a basic test note.",
  tags: ["uat/crud-test"],
  revision_mode: "none"
})
```

**Pass Criteria**: Returns `{ id: "<uuid>" }`
**Store**: `crud_test_note_id`

---

### CRUD-002: Create Note - With Metadata

```javascript
create_note({
  content: "# Metadata Test\n\nNote with custom metadata.",
  tags: ["uat/crud-test", "uat/metadata"],
  metadata: { source: "uat-test", priority: "high", version: 1 },
  revision_mode: "none"
})
```

**Pass Criteria**: Returns valid ID
**Verify**: `get_note(id)` shows metadata

---

### CRUD-003: Create Note - Hierarchical Tags

```javascript
create_note({
  content: "# Hierarchical Tag Test",
  tags: ["uat/hierarchy/level1/level2/level3"],
  revision_mode: "none"
})
```

**Pass Criteria**: Returns valid ID
**Verify**: `list_tags()` contains the hierarchical tag

---

### CRUD-004: Bulk Create

```javascript
bulk_create_notes({
  notes: [
    { content: "Bulk note 1", tags: ["uat/bulk"], revision_mode: "none" },
    { content: "Bulk note 2", tags: ["uat/bulk"], revision_mode: "none" },
    { content: "Bulk note 3", tags: ["uat/bulk"], revision_mode: "none" }
  ]
})
```

**Pass Criteria**: Returns `{ count: 3, ids: [...] }`

---

## Read Operations

### CRUD-005: Get Note by ID

```javascript
get_note({ id: "<crud_test_note_id>" })
```

**Pass Criteria**: Returns full note with `note`, `original`, `revised`, `tags`

---

### CRUD-006: Get Note - Non-existent

```javascript
get_note({ id: "00000000-0000-0000-0000-000000000000" })
```

**Pass Criteria**: Returns error (not crash)

---

### CRUD-007: List Notes - Basic

```javascript
list_notes({ limit: 10 })
```

**Pass Criteria**: Returns `{ notes: [...], total: <n> }`

---

### CRUD-008: List Notes - Tag Filter

```javascript
list_notes({ tags: ["uat/bulk"], limit: 50 })
```

**Pass Criteria**: Returns exactly 3 notes (from CRUD-004)

---

### CRUD-009: List Notes - Hierarchical Tag Filter

```javascript
list_notes({ tags: ["uat"], limit: 100 })
```

**Pass Criteria**: Returns all UAT-tagged notes (prefix matching)

---

### CRUD-010: Pagination

```javascript
const page1 = list_notes({ limit: 5, offset: 0 })
const page2 = list_notes({ limit: 5, offset: 5 })
```

**Pass Criteria**: Different notes on each page, no overlap

---

### CRUD-011: Limit Zero

```javascript
list_notes({ limit: 0 })
```

**Pass Criteria**: Returns `{ notes: [], total: <n> }` (total still reported)

---

## Update Operations

### CRUD-012: Update Content

```javascript
update_note({
  id: "<crud_test_note_id>",
  content: "# Updated Content\n\nThis was updated.",
  revision_mode: "none"
})
```

**Pass Criteria**: Success
**Verify**: `get_note` shows new content

---

### CRUD-013: Star Note

```javascript
update_note({ id: "<note_id>", starred: true })
```

**Pass Criteria**: `get_note` shows `starred: true`

---

### CRUD-014: Archive Note

```javascript
update_note({ id: "<note_id>", archived: true })
```

**Pass Criteria**: Note appears in `list_notes({ filter: "archived" })`

---

### CRUD-015: Update Metadata

```javascript
update_note({
  id: "<note_id>",
  metadata: { updated: true, version: 2 }
})
```

**Pass Criteria**: `get_note` shows new metadata

---

## Delete Operations

### CRUD-016: Soft Delete Note

```javascript
delete_note({ id: "<note_to_delete>" })
```

**Pass Criteria**: Note no longer in `list_notes`
**Verify**: Note can still be restored

---

### CRUD-017: Purge Note

```javascript
purge_note({ id: "<already_deleted_note>" })
```

**Pass Criteria**: Note permanently removed, cannot be restored

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| CRUD-001 | Create Note - Basic | |
| CRUD-002 | Create Note - Metadata | |
| CRUD-003 | Create Note - Hierarchical Tags | |
| CRUD-004 | Bulk Create | |
| CRUD-005 | Get Note by ID | |
| CRUD-006 | Get Note - Non-existent | |
| CRUD-007 | List Notes - Basic | |
| CRUD-008 | List Notes - Tag Filter | |
| CRUD-009 | List Notes - Hierarchical Tag | |
| CRUD-010 | Pagination | |
| CRUD-011 | Limit Zero | |
| CRUD-012 | Update Content | |
| CRUD-013 | Star Note | |
| CRUD-014 | Archive Note | |
| CRUD-015 | Update Metadata | |
| CRUD-016 | Soft Delete | |
| CRUD-017 | Purge Note | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:
