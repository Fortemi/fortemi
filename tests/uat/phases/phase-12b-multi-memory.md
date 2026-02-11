# UAT Phase 12b: Multi-Memory Architecture

**Duration**: ~8 minutes
**Tools Tested**: `select_memory`, `get_active_memory`, `list_memories`, `create_memory`, `delete_memory`, `clone_memory`, `get_memories_overview`
**Dependencies**: Phase 0 (preflight), Phase 12 (archives)

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

Multi-Memory provides parallel memory archives for data isolation. Each memory operates as a separate PostgreSQL schema with its own notes, tags, collections, embeddings, and configuration. This phase tests the MCP session-level memory selection and lifecycle management tools.

---

## Important Notes

- `select_memory` persists for the duration of the MCP session
- The default memory is "public" (maps to the `public` schema)
- Memory names must be alphanumeric with hyphens/underscores
- Cannot delete the default or "public" memory
- `clone_memory` performs a deep copy including all data
- Search (FTS + semantic) is currently restricted to the default memory only

---

## Test Cases

### MEM-001: List Memories (Initial)

**MCP Tool**: `list_memories`

```javascript
list_memories()
```

**Expected**:
```json
{
  "memories": [
    {
      "name": "public",
      "description": "...",
      "is_default": true,
      "note_count": "<n>",
      "size_bytes": "<n>",
      "created_at_utc": "<timestamp>"
    }
  ]
}
```

**Pass Criteria**:
- Returns array of memories
- At least one memory exists (the default "public" memory)
- Default memory has `is_default: true`

---

### MEM-002: Get Active Memory (Default)

**MCP Tool**: `get_active_memory`

```javascript
get_active_memory()
```

**Expected**:
```json
{
  "name": "public",
  "is_default": true
}
```

**Pass Criteria**:
- Returns the currently active memory
- Before any `select_memory` call, should be the default ("public")

---

### MEM-003: Get Memories Overview

**MCP Tool**: `get_memories_overview`

```javascript
get_memories_overview()
```

**Expected**:
```json
{
  "total_memories": "<n>",
  "total_notes": "<n>",
  "total_size_bytes": "<n>",
  "memories": [
    {
      "name": "public",
      "note_count": "<n>",
      "size_bytes": "<n>"
    }
  ]
}
```

**Pass Criteria**:
- Returns aggregate statistics across all memories
- `total_memories` >= 1
- `total_notes` is a non-negative integer
- Per-memory breakdown included

---

### MEM-004: Create Memory

**MCP Tool**: `create_memory`

```javascript
create_memory({
  name: "uat-test-memory",
  description: "UAT test memory for multi-memory phase"
})
```

**Expected**:
```json
{
  "name": "uat-test-memory",
  "description": "UAT test memory for multi-memory phase",
  "is_default": false,
  "created_at_utc": "<timestamp>"
}
```

**Pass Criteria**:
- Returns created memory metadata
- Name matches requested name
- `is_default` is false
- Memory is now visible in `list_memories`

---

### MEM-005: Verify New Memory in List

**MCP Tool**: `list_memories`

```javascript
list_memories()
```

**Pass Criteria**:
- Returns at least 2 memories (default + uat-test-memory)
- "uat-test-memory" appears in the list with correct description

---

### MEM-006: Select Memory

**MCP Tool**: `select_memory`

```javascript
select_memory({ name: "uat-test-memory" })
```

**Expected**:
```json
{
  "selected": "uat-test-memory",
  "message": "..."
}
```

**Pass Criteria**:
- Returns confirmation of memory selection
- Selected memory name matches request

---

### MEM-007: Verify Active Memory Changed

**MCP Tool**: `get_active_memory`

```javascript
get_active_memory()
```

**Pass Criteria**:
- Returns "uat-test-memory" as the active memory
- `is_default` should be false

---

### MEM-008: Create Note in Selected Memory

**MCP Tool**: `create_note`

```javascript
create_note({
  content: "# Multi-Memory Test Note\n\nThis note exists in uat-test-memory.",
  tags: ["uat/multi-memory-test"],
  revision_mode: "none"
})
```

**Pass Criteria**:
- Note created successfully in the selected memory
- Returns valid note ID

---

### MEM-009: Switch Back to Default Memory

**MCP Tool**: `select_memory`

```javascript
select_memory({ name: "public" })
```

**Pass Criteria**:
- Successfully switches back to default memory
- `get_active_memory()` confirms "public" is active

---

### MEM-010: Verify Note Isolation

**MCP Tool**: `list_notes`

```javascript
list_notes({ tags: ["uat/multi-memory-test"], limit: 10 })
```

**Pass Criteria**:
- The note created in MEM-008 should NOT appear in the default memory
- Notes are isolated per memory

---

### MEM-011: Clone Memory

**MCP Tool**: `clone_memory`

```javascript
clone_memory({
  source_name: "uat-test-memory",
  new_name: "uat-cloned-memory",
  description: "Clone of uat-test-memory for testing"
})
```

**Expected**:
```json
{
  "name": "uat-cloned-memory",
  "source": "uat-test-memory",
  "description": "Clone of uat-test-memory for testing"
}
```

**Pass Criteria**:
- Clone created successfully
- Returns cloned memory metadata
- New memory appears in `list_memories`

---

### MEM-012: Verify Clone Has Data

**MCP Tool**: `select_memory`, `list_notes`

```javascript
select_memory({ name: "uat-cloned-memory" })
list_notes({ tags: ["uat/multi-memory-test"], limit: 10 })
```

**Pass Criteria**:
- After selecting the cloned memory, the note from MEM-008 should be present
- Clone is a deep copy of the source data

---

### MEM-013: Memories Overview Updated

**MCP Tool**: `get_memories_overview`

```javascript
select_memory({ name: "public" })
get_memories_overview()
```

**Pass Criteria**:
- `total_memories` >= 3 (public + uat-test-memory + uat-cloned-memory)
- Overview reflects the newly created memories

---

### MEM-014: Create Duplicate Memory (Error)

**MCP Tool**: `create_memory`

```javascript
create_memory({ name: "uat-test-memory" })
```

**Pass Criteria**:
- Returns an error (memory already exists)
- Error message is descriptive

---

### MEM-015: Select Non-Existent Memory (Error)

**MCP Tool**: `select_memory`

```javascript
select_memory({ name: "does-not-exist-memory" })
```

**Pass Criteria**:
- Returns an error (memory not found)
- Error message is descriptive

---

### MEM-016: Delete Cloned Memory

**MCP Tool**: `delete_memory`

```javascript
delete_memory({ name: "uat-cloned-memory" })
```

**Pass Criteria**:
- Successfully deletes the cloned memory
- Memory no longer appears in `list_memories`

---

### MEM-017: Delete Test Memory

**MCP Tool**: `delete_memory`

```javascript
delete_memory({ name: "uat-test-memory" })
```

**Pass Criteria**:
- Successfully deletes the test memory
- Memory no longer appears in `list_memories`

---

### MEM-018: Delete Default Memory (Error)

**MCP Tool**: `delete_memory`

```javascript
delete_memory({ name: "public" })
```

**Pass Criteria**:
- Returns an error (cannot delete default memory)
- Default memory remains intact

---

### MEM-019: Final State Verification

**MCP Tool**: `list_memories`, `get_active_memory`

```javascript
list_memories()
get_active_memory()
```

**Pass Criteria**:
- Only original memories remain (test memories cleaned up)
- Active memory is "public" (default)
- System is in clean state for subsequent phases

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| MEM-001 | List Memories (Initial) | `list_memories` | |
| MEM-002 | Get Active Memory (Default) | `get_active_memory` | |
| MEM-003 | Get Memories Overview | `get_memories_overview` | |
| MEM-004 | Create Memory | `create_memory` | |
| MEM-005 | Verify New Memory in List | `list_memories` | |
| MEM-006 | Select Memory | `select_memory` | |
| MEM-007 | Verify Active Memory Changed | `get_active_memory` | |
| MEM-008 | Create Note in Selected Memory | `create_note` | |
| MEM-009 | Switch Back to Default | `select_memory` | |
| MEM-010 | Verify Note Isolation | `list_notes` | |
| MEM-011 | Clone Memory | `clone_memory` | |
| MEM-012 | Verify Clone Has Data | `select_memory`, `list_notes` | |
| MEM-013 | Memories Overview Updated | `get_memories_overview` | |
| MEM-014 | Create Duplicate (Error) | `create_memory` | |
| MEM-015 | Select Non-Existent (Error) | `select_memory` | |
| MEM-016 | Delete Cloned Memory | `delete_memory` | |
| MEM-017 | Delete Test Memory | `delete_memory` | |
| MEM-018 | Delete Default (Error) | `delete_memory` | |
| MEM-019 | Final State Verification | `list_memories`, `get_active_memory` | |

**Pass Rate Required**: 100% (19/19)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `select_memory` | MEM-006, MEM-009, MEM-012, MEM-015 |
| `get_active_memory` | MEM-002, MEM-007, MEM-019 |
| `list_memories` | MEM-001, MEM-005, MEM-019 |
| `create_memory` | MEM-004, MEM-014 |
| `delete_memory` | MEM-016, MEM-017, MEM-018 |
| `clone_memory` | MEM-011 |
| `get_memories_overview` | MEM-003, MEM-013 |

**Coverage**: 7/7 multi-memory tools (100%)

---

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
