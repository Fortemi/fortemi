# UAT Phase 11: Versioning

**Duration**: ~7 minutes
**Tools Tested**: `list_note_versions`, `get_note_version`, `diff_note_versions`, `restore_note_version`, `delete_note_version`
**Dependencies**: Phase 2 (CRUD - need notes with history)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

Note versioning tracks content changes over time, enabling restoration to previous states and diffing between versions. This phase tests the complete version history lifecycle.

---

## Test Data Setup

Create a note that will be updated multiple times to build version history:

```javascript
// Create initial note
const version_test_note = create_note({
  content: "# Version Test Note\n\nVersion 1: Initial content.",
  tags: ["uat/versioning"],
  revision_mode: "none"
})

// Store the ID
const VERSION_NOTE_ID = version_test_note.id
```

---

## Test Cases

### VER-001: List Versions (Initial)

**MCP Tool**: `list_note_versions`

```javascript
list_note_versions({ note_id: VERSION_NOTE_ID })
```

**Expected**:
```json
{
  "note_id": "<uuid>",
  "current_original_version": 1,
  "current_revision_number": 0,
  "original_versions": [
    {
      "version": 1,
      "created_at": "<timestamp>",
      "content_preview": "# Version Test Note..."
    }
  ],
  "revised_versions": []
}
```

**Pass Criteria**: At least version 1 exists

---

### VER-002: Create Version History (Update 1)

**MCP Tool**: `update_note`

```javascript
update_note({
  id: VERSION_NOTE_ID,
  content: "# Version Test Note\n\nVersion 2: Added more content.\n\n## Section A\nThis is new content.",
  revision_mode: "none"
})
```

**Pass Criteria**: Update succeeds

---

### VER-003: Create Version History (Update 2)

**MCP Tool**: `update_note`

```javascript
update_note({
  id: VERSION_NOTE_ID,
  content: "# Version Test Note\n\nVersion 3: Major revision.\n\n## Section A\nUpdated content.\n\n## Section B\nBrand new section.",
  revision_mode: "none"
})
```

**Pass Criteria**: Update succeeds

---

### VER-004: List Versions (After Updates)

**MCP Tool**: `list_note_versions`

```javascript
list_note_versions({ note_id: VERSION_NOTE_ID })
```

**Expected**:
- `current_original_version` >= 3
- `original_versions` array has 3+ entries
- Versions ordered by version number

**Pass Criteria**: Version count matches update count

---

### VER-005: Get Specific Version

**MCP Tool**: `get_note_version`

```javascript
get_note_version({
  note_id: VERSION_NOTE_ID,
  version: 1,
  track: "original"
})
```

**Expected**:
```json
{
  "note_id": "<uuid>",
  "version": 1,
  "track": "original",
  "content": "# Version Test Note\n\nVersion 1: Initial content.",
  "created_at": "<timestamp>"
}
```

**Pass Criteria**: Returns original v1 content

---

### VER-006: Get Version 2

**MCP Tool**: `get_note_version`

```javascript
get_note_version({
  note_id: VERSION_NOTE_ID,
  version: 2,
  track: "original"
})
```

**Expected**: Contains "Version 2: Added more content"

**Pass Criteria**: Content matches v2

---

### VER-007: Diff Between Versions

**MCP Tool**: `diff_note_versions`

```javascript
diff_note_versions({
  note_id: VERSION_NOTE_ID,
  from_version: 1,
  to_version: 3
})
```

**Expected**: Unified diff format showing:
- Lines removed from v1 (prefixed with -)
- Lines added in v3 (prefixed with +)
- Context lines (no prefix)

**Pass Criteria**: Valid diff output

---

### VER-008: Diff Adjacent Versions

**MCP Tool**: `diff_note_versions`

```javascript
diff_note_versions({
  note_id: VERSION_NOTE_ID,
  from_version: 2,
  to_version: 3
})
```

**Expected**: Smaller diff showing only v2→v3 changes

**Pass Criteria**: Diff is smaller than v1→v3

---

### VER-009: Restore Previous Version

**MCP Tool**: `restore_note_version`

```javascript
// Current is v3, restore to v1
restore_note_version({
  note_id: VERSION_NOTE_ID,
  version: 1,
  restore_tags: false
})
```

**Expected**:
- Note content reverted to v1
- New version created (v4) with v1 content
- Version history preserved

**Verify**: `get_note` shows v1 content

---

### VER-010: Verify Restore Created New Version

**MCP Tool**: `list_note_versions`

```javascript
list_note_versions({ note_id: VERSION_NOTE_ID })
```

**Expected**:
- `current_original_version` >= 4
- All previous versions still exist
- Restore didn't delete history

**Pass Criteria**: Version count increased

---

### VER-011: Restore With Tags

**MCP Tool**: `restore_note_version`

```javascript
// First update tags
update_note({
  id: VERSION_NOTE_ID,
  content: "# Version Test Note\n\nVersion 5: New content with new tags."
})
set_note_tags({
  id: VERSION_NOTE_ID,
  tags: ["uat/versioning", "uat/new-tag"]
})

// Now restore to v2 with tags
restore_note_version({
  note_id: VERSION_NOTE_ID,
  version: 2,
  restore_tags: true
})
```

**Expected**: Tags restored to v2 state (only `uat/versioning`, no `uat/new-tag`)

---

### VER-012: Delete Specific Version

**MCP Tool**: `delete_note_version`

```javascript
// Delete v2 (middle version)
delete_note_version({
  note_id: VERSION_NOTE_ID,
  version: 2
})
```

**Expected**: Version 2 removed from history

**Verify**: `list_note_versions` no longer includes v2

---

### VER-013: Verify Version Deleted

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note_version`

```javascript
get_note_version({
  note_id: VERSION_NOTE_ID,
  version: 2,
  track: "original"
})
```

**Expected**: 404 Not Found or error indicating version doesn't exist

**Pass Criteria**: Deleted version not accessible

---

### VER-014: Get Non-Existent Version

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_note_version`

```javascript
get_note_version({
  note_id: VERSION_NOTE_ID,
  version: 999,
  track: "original"
})
```

**Expected**: Error response (404 or similar)

**Pass Criteria**: Graceful error handling

---

### VER-015: Diff With Deleted Version

**Isolation**: Required — negative test expects error response

**MCP Tool**: `diff_note_versions`

```javascript
diff_note_versions({
  note_id: VERSION_NOTE_ID,
  from_version: 2,  // Deleted
  to_version: 3
})
```

**Pass Criteria**: Returns **404 Not Found** (version doesn't exist)

---

## Cleanup

```javascript
// Delete the test note (also deletes version history)
delete_note({ id: VERSION_NOTE_ID })

// Verify versions deleted with note
list_note_versions({ note_id: VERSION_NOTE_ID })  // Should error
```

---

## Success Criteria

| Test | MCP Tool(s) | Status | Notes |
|------|-------------|--------|-------|
| VER-001 | `list_note_versions` | | List initial versions |
| VER-002 | `update_note` | | Setup: Update 1 |
| VER-003 | `update_note` | | Setup: Update 2 |
| VER-004 | `list_note_versions` | | List after updates |
| VER-005 | `get_note_version` | | Get version 1 |
| VER-006 | `get_note_version` | | Get version 2 |
| VER-007 | `diff_note_versions` | | Diff v1 to v3 |
| VER-008 | `diff_note_versions` | | Diff adjacent versions |
| VER-009 | `restore_note_version` | | Restore to v1 |
| VER-010 | `list_note_versions` | | Verify restore creates version |
| VER-011 | `restore_note_version` | | Restore with tags |
| VER-012 | `delete_note_version` | | Delete version |
| VER-013 | `get_note_version` | | Verify version deleted |
| VER-014 | `get_note_version` | | Get non-existent version |
| VER-015 | `diff_note_versions` | | Diff with deleted version |

**Pass Rate Required**: 100% (15/15)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `list_note_versions` | VER-001, VER-004, VER-010 |
| `get_note_version` | VER-005, VER-006, VER-013, VER-014 |
| `diff_note_versions` | VER-007, VER-008, VER-015 |
| `restore_note_version` | VER-009, VER-011 |
| `delete_note_version` | VER-012 |

**Coverage**: 5/5 versioning tools (100%)
