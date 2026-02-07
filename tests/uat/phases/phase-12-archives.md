# UAT Phase 12: Archives

**Duration**: ~8 minutes
**Tools Tested**: `list_archives`, `create_archive`, `get_archive`, `update_archive`, `delete_archive`, `set_default_archive`, `get_archive_stats`
**Dependencies**: Phase 0 (preflight)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Overview

Archives provide schema-level data isolation, allowing multiple independent knowledge bases within a single deployment. This phase tests archive creation, switching, and data isolation.

---

## Important Notes

- Archives use PostgreSQL schemas for complete data isolation
- Each archive has its own notes, tags, collections, embeddings
- Operations within an archive don't affect other archives
- Default archive is used when no archive is specified

---

## Test Cases

### ARCH-001: List Archives (Initial)

**MCP Tool**: `list_archives`

```javascript
list_archives()
```

**Expected**:
```json
{
  "archives": [
    {
      "id": "<uuid>",
      "name": "default",
      "schema_name": "public",
      "description": "Default archive",
      "created_at": "<timestamp>",
      "note_count": <n>,
      "size_bytes": <n>,
      "is_default": true
    }
  ]
}
```

**Pass Criteria**: At least default archive exists

---

### ARCH-002: Create Archive

**MCP Tool**: `create_archive`

```javascript
create_archive({
  name: "uat-test-archive",
  description: "Archive for UAT testing"
})
```

**Expected**:
```json
{
  "id": "<uuid>",
  "name": "uat-test-archive",
  "schema_name": "archive_uat_test_archive"
}
```

**Pass Criteria**: Archive created with generated schema name

**Store**: `test_archive_name = "uat-test-archive"`

---

### ARCH-003: Create Second Archive

**MCP Tool**: `create_archive`

```javascript
create_archive({
  name: "uat-secondary",
  description: "Secondary test archive"
})
```

**Expected**: Second archive created

**Store**: `secondary_archive_name = "uat-secondary"`

---

### ARCH-004: List Archives (After Creation)

**MCP Tool**: `list_archives`

```javascript
list_archives()
```

**Expected**:
- Contains "default", "uat-test-archive", "uat-secondary"
- Each has unique schema_name

**Pass Criteria**: At least 3 archives

---

### ARCH-005: Get Archive Details

**MCP Tool**: `get_archive`

```javascript
get_archive({ name: "uat-test-archive" })
```

**Expected**:
```json
{
  "id": "<uuid>",
  "name": "uat-test-archive",
  "schema_name": "archive_uat_test_archive",
  "description": "Archive for UAT testing",
  "created_at": "<timestamp>",
  "note_count": 0,
  "size_bytes": 0,
  "is_default": false
}
```

**Pass Criteria**: All fields present, note_count = 0

---

### ARCH-006: Get Archive Stats

**MCP Tool**: `get_archive_stats`

```javascript
get_archive_stats({ name: "uat-test-archive" })
```

**Expected**:
```json
{
  "note_count": 0,
  "size_bytes": 0,
  "last_accessed": "<timestamp>"
}
```

**Pass Criteria**: Stats returned for empty archive

---

### ARCH-007: Update Archive Metadata

**MCP Tool**: `update_archive`

```javascript
update_archive({
  name: "uat-test-archive",
  description: "Updated description for UAT testing archive"
})
```

**Expected**: Description updated

**Verify**: `get_archive` shows new description

---

### ARCH-008: Set Default Archive

**MCP Tool**: `set_default_archive`

```javascript
set_default_archive({ name: "uat-test-archive" })
```

**Expected**: uat-test-archive becomes default

**Verify**: `list_archives` shows uat-test-archive with `is_default: true`

---

### ARCH-009: Verify Default Changed

**MCP Tool**: `list_archives`

```javascript
list_archives()
```

**Expected**:
- "uat-test-archive" has `is_default: true`
- "default" has `is_default: false`

**Pass Criteria**: Only one archive is default

---

### ARCH-010: Create Note in Archive

**MCP Tool**: `create_note`

```javascript
// With uat-test-archive as default
create_note({
  content: "# Archive Test Note\n\nThis note is in uat-test-archive.",
  tags: ["uat/archives"],
  revision_mode: "none"
})
```

**Expected**: Note created in uat-test-archive

**Store**: `archive_note_id`

---

### ARCH-011: Verify Note in Archive Stats

**MCP Tool**: `get_archive_stats`

```javascript
get_archive_stats({ name: "uat-test-archive" })
```

**Expected**: `note_count: 1`

**Pass Criteria**: Note counted in archive

---

### ARCH-012: Switch Back to Default

**MCP Tool**: `set_default_archive`

```javascript
set_default_archive({ name: "default" })
```

**Expected**: Default archive restored

---

### ARCH-013: Verify Note Isolation

**MCP Tool**: `list_notes`

```javascript
// With "default" as current archive
list_notes({ tags: ["uat/archives"] })
```

**Expected**: Should NOT find the note created in uat-test-archive

**Pass Criteria**: Archives are isolated

---

### ARCH-014: Create Duplicate Archive Name

**Isolation**: Required — negative test expects error response

**MCP Tool**: `create_archive`

```javascript
create_archive({
  name: "uat-test-archive",  // Already exists
  description: "Duplicate"
})
```

**Expected**: Error - archive name must be unique

**Pass Criteria**: Graceful error handling

---

### ARCH-015: Delete Archive - Non-Empty Warning

**MCP Tool**: `delete_archive`

```javascript
// uat-test-archive has 1 note
delete_archive({ name: "uat-test-archive" })
```

**Expected**:
- Either succeeds with cascade delete
- Or requires confirmation/force flag

**Pass Criteria**: Defined behavior for non-empty archive

---

### ARCH-016: Delete Empty Archive

**MCP Tool**: `delete_archive`

```javascript
// uat-secondary has no notes
delete_archive({ name: "uat-secondary" })
```

**Expected**: Archive deleted successfully

**Verify**: `list_archives` no longer includes uat-secondary

---

### ARCH-017: Verify Archive Deleted

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_archive`

```javascript
get_archive({ name: "uat-secondary" })
```

**Expected**: 404 Not Found

**Pass Criteria**: Deleted archive not accessible

---

### ARCH-018: Delete Default Archive Prevention

**Isolation**: Required — negative test expects error response

**MCP Tool**: `delete_archive`

```javascript
delete_archive({ name: "default" })
```

**Expected**: Error - cannot delete default archive

**Pass Criteria**: Default archive protected

---

## Cleanup

```javascript
// Ensure default is restored
set_default_archive({ name: "default" })

// Delete test archives (cascade deletes notes)
delete_archive({ name: "uat-test-archive" })
// uat-secondary already deleted in ARCH-016

// Verify cleanup
list_archives()  // Should only show default (and any pre-existing)
```

---

## Success Criteria

| Test | MCP Tool(s) | Status | Notes |
|------|-------------|--------|-------|
| ARCH-001 | `list_archives` | | List initial archives |
| ARCH-002 | `create_archive` | | Create archive |
| ARCH-003 | `create_archive` | | Create second archive |
| ARCH-004 | `list_archives` | | List after creation |
| ARCH-005 | `get_archive` | | Get archive details |
| ARCH-006 | `get_archive_stats` | | Get archive stats |
| ARCH-007 | `update_archive` | | Update archive metadata |
| ARCH-008 | `set_default_archive` | | Set default archive |
| ARCH-009 | `list_archives` | | Verify default changed |
| ARCH-010 | `create_note` | | Create note in archive |
| ARCH-011 | `get_archive_stats` | | Verify stats updated |
| ARCH-012 | `set_default_archive` | | Switch back to default |
| ARCH-013 | `list_notes` | | Verify data isolation |
| ARCH-014 | `create_archive` | | Duplicate name error |
| ARCH-015 | `delete_archive` | | Delete non-empty archive |
| ARCH-016 | `delete_archive` | | Delete empty archive |
| ARCH-017 | `get_archive` | | Verify archive deleted |
| ARCH-018 | `delete_archive` | | Cannot delete default |

**Pass Rate Required**: 100% (18/18)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `list_archives` | ARCH-001, ARCH-004, ARCH-009 |
| `create_archive` | ARCH-002, ARCH-003, ARCH-014 |
| `get_archive` | ARCH-005, ARCH-017 |
| `update_archive` | ARCH-007 |
| `delete_archive` | ARCH-015, ARCH-016, ARCH-018 |
| `set_default_archive` | ARCH-008, ARCH-012 |
| `get_archive_stats` | ARCH-006, ARCH-011 |

**Coverage**: 7/7 archive tools (100%)
