# UAT Phase 15: Final Cleanup

## Purpose
Remove ALL UAT test data from the system to restore clean state. This phase MUST run last and ensures no test artifacts remain after the test suite completes.

## Duration
~3 minutes

## Prerequisites
- All previous phases (1-14) completed
- MCP server healthy
- Access to all memories used during testing

## Tools Tested
- `list_notes`
- `delete_note`
- `manage_collection`
- `manage_embeddings`
- `manage_archives`
- `select_memory`
- `purge_note`
- `purge_notes`
- `purge_all_notes`

---

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Direct API calls or curl commands are NOT acceptable. This cleanup phase removes all notes with `uat/` tag prefix, all UAT-related collections, and all UAT embedding sets.

---

## Test Cases

### CLEAN-001: List All UAT Notes
**MCP Tool**: `list_notes`

```javascript
async function listAllUatNotes() {
  const notes = [];
  for (let offset = 0; ; offset += 100) {
    const page = await mcp.call_tool("list_notes", { limit: 100, offset });
    notes.push(...page.notes);
    if (page.notes.length < 100) break;
  }
  return notes.filter(note =>
    (note.tags || []).some(tag => tag === "uat" || tag.startsWith("uat/"))
  );
}

const uatNotesByMemory = {};
for (const memory of ["public", "test-archive", "uat-test-memory", "uat-pagination-memory"]) {
  try {
    await mcp.call_tool("select_memory", { name: memory });
    uatNotesByMemory[memory] = await listAllUatNotes();
  } catch (error) {
    // An archive may not exist if its setup phase failed.
  }
}
await mcp.call_tool("select_memory", { name: "public" });
```

**Expected**: List of all notes tagged with `uat` or `uat/*`
**Pass Criteria**:
- Pagination continues until each archive is exhausted
- Every stored note has `uat` or a `uat/` hierarchical tag
- All four possible UAT archives are checked when present

**Store**: `uatNotesByMemory` (note arrays keyed by archive)

---

### CLEAN-002: Delete All UAT Notes
**MCP Tool**: `delete_note`

```javascript
const deletedByMemory = {};
for (const [memory, notes] of Object.entries(uatNotesByMemory)) {
  await mcp.call_tool("select_memory", { name: memory });
  deletedByMemory[memory] = notes.map(note => note.id);
  for (const id of deletedByMemory[memory]) {
    await mcp.call_tool("delete_note", { id });
  }
}
await mcp.call_tool("select_memory", { name: "public" });
```

**Expected**: All UAT notes deleted successfully
**Pass Criteria**:
- Each delete operation completes without errors
- No UAT notes remain active
- Deletion count matches list count from CLEAN-001

---

### CLEAN-002A: Permanently Purge UAT Notes

**MCP Tools**: `purge_note`, `purge_notes`

```javascript
for (const [memory, ids] of Object.entries(deletedByMemory)) {
  await mcp.call_tool("select_memory", { name: memory });
  if (ids.length > 0) {
    await mcp.call_tool("purge_note", { id: ids[0] });
  }
  if (ids.length > 1) {
    const result = await mcp.call_tool("purge_notes", { note_ids: ids.slice(1) });
    if (result.failed.length > 0) throw new Error(JSON.stringify(result.failed));
  }
}
await mcp.call_tool("select_memory", { name: "public" });
```

**Pass Criteria**:
- [ ] Single-note purge succeeds for each non-empty archive
- [ ] Batch purge reports no failures
- [ ] Only IDs collected from the UAT tag query are purged

---

### CLEAN-002B: Purge-All Confirmation Guard

**Isolation**: Required

```javascript
await mcp.call_tool("purge_all_notes", { confirm: false });
```

**Pass Criteria**:
- [ ] The isolated call fails and requires `confirm: true`
- [ ] No notes are purged

> The suite deliberately does not call `purge_all_notes` with `confirm: true`, because a shared UAT instance may contain unrelated soft-deleted notes.

---

### CLEAN-003: Delete UAT Collections
**MCP Tool**: `manage_collection`

```javascript
// Switch to default memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

// List all collections
const defaultCollections = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: { action: "list" }
});

// Delete collections with "uat" or "UAT" in name
const uatCollections = defaultCollections.collections.filter(c =>
  c.name.toLowerCase().includes("uat")
);

for (const collection of uatCollections) {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "manage_collection",
    arguments: {
      action: "delete",
      collection_id: collection.id
    }
  });
}

// Repeat for test memory if it exists
try {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "select_memory",
    arguments: { name: "uat-test-memory" }
  });

  const testCollections = await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "manage_collection",
    arguments: { action: "list" }
  });

  const testUatCollections = testCollections.collections.filter(c =>
    c.name.toLowerCase().includes("uat")
  );

  for (const collection of testUatCollections) {
    await use_mcp_tool({
      server_name: "matric-memory",
      tool_name: "manage_collection",
      arguments: {
        action: "delete",
        collection_id: collection.id
      }
    });
  }
} catch (e) {
  // Test memory may not exist
}
```

**Expected**: All UAT collections deleted
**Pass Criteria**:
- Each collection delete completes without errors
- No UAT-named collections remain
- Notes previously in collections still deleted (from CLEAN-002)

---

### CLEAN-004: Verify Cleanup
**MCP Tool**: `list_notes`

```javascript
let totalRemaining = 0;
for (const memory of Object.keys(uatNotesByMemory)) {
  await mcp.call_tool("select_memory", { name: memory });
  totalRemaining += (await listAllUatNotes()).length;
}
await mcp.call_tool("select_memory", { name: "public" });
```

**Expected**: Zero UAT notes remaining
**Pass Criteria**:
- Every archive checked in CLEAN-001 has zero active UAT notes
- `totalRemaining === 0`

---

### CLEAN-005: Delete UAT Embedding Sets
**MCP Tool**: `manage_embeddings`

```javascript
// Switch to default memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

// List all embedding sets
const allSets = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: { action: "list" }
});

// Delete sets with "uat-embed-" prefix in name
const uatSets = allSets.sets.filter(s =>
  s.name.toLowerCase().startsWith("uat-embed-")
);

for (const set of uatSets) {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "manage_embeddings",
    arguments: {
      action: "delete",
      slug: set.slug
    }
  });
}

// Verify no UAT embedding sets remain
const verifyResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_embeddings",
  arguments: { action: "list" }
});

const remaining = verifyResult.sets.filter(s =>
  s.name.toLowerCase().startsWith("uat-embed-")
);
```

**Expected**: All UAT embedding sets deleted
**Pass Criteria**:
- Each set delete completes without errors
- No sets with `uat-embed-` prefix remain
- `remaining.length === 0`
- Default embedding set is NOT deleted (only UAT sets)

---

### CLEAN-006: Delete UAT Memory Archives
**MCP Tool**: `manage_archives`

```javascript
// Switch to default memory first
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

// Delete UAT test archives created during phases 0, 8, and 12
const archivesToDelete = ["test-archive", "uat-test-memory", "uat-pagination-memory"];

for (const archiveName of archivesToDelete) {
  try {
    await use_mcp_tool({
      server_name: "matric-memory",
      tool_name: "manage_archives",
      arguments: {
        action: "delete",
        name: archiveName
      }
    });
    console.log(`Deleted archive: ${archiveName}`);
  } catch (e) {
    // Archive may not exist if earlier phases failed — that's OK
    console.log(`Archive ${archiveName} not found (skipping)`);
  }
}
```

**Expected**: UAT archives deleted
**Pass Criteria**:
- Both archives deleted or confirmed non-existent
- No orphaned schemas remain
- Active memory is still "public"

---

### CLEAN-007: Switch to Default Memory
**MCP Tool**: `select_memory`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "public"
  }
});

// Verify active memory
const activeMemory = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_active_memory",
  arguments: {}
});
```

**Expected**: Active memory reset to default (public)
**Pass Criteria**:
- `select_memory` completes successfully
- `get_active_memory` returns "public"
- System ready for next test run

---

## Cleanup Metrics

**Expected deletions:**
- Notes created across all phases: ~165-185 notes, including the 105-note pagination archive
- Collections created: 3-5 collections
- Embedding sets created: 2-3 sets (with `uat-embed-` prefix)
- Archives to delete: 3 (`test-archive`, `uat-test-memory`, `uat-pagination-memory`)
- Memories used: public plus up to three UAT archives

**Cleanup verification:**
```javascript
// Final verification query
await mcp.call_tool("select_memory", { name: "public" });
const finalCheck = await listAllUatNotes();

if (finalCheck.length === 0) {
  console.log("Cleanup successful - zero UAT notes remaining");
} else {
  console.error(`Cleanup incomplete - ${finalCheck.length} UAT notes still exist`);
}
```

---

## Phase Summary

| Category | Pass | Fail | Skip | Total |
|----------|------|------|------|-------|
| UAT Note Cleanup | 0 | 0 | 0 | 4 |
| Collection Cleanup | 0 | 0 | 0 | 1 |
| Verification | 0 | 0 | 0 | 1 |
| Embedding Set Cleanup | 0 | 0 | 0 | 1 |
| Archive Cleanup | 0 | 0 | 0 | 1 |
| Memory Reset | 0 | 0 | 0 | 1 |
| **Total** | **0** | **0** | **0** | **9** |

## Phase Result
- [ ] **Phase 15 PASSED** - All UAT data successfully removed
- [ ] **Phase 15 FAILED** - See failure details above
- [ ] **Phase 15 SKIPPED** - Reason: _______________

## Post-Cleanup Checklist

After CLEAN-006 completes:
- [ ] Zero notes with `uat` tags remain
- [ ] Zero collections with "uat" in name remain
- [ ] Zero embedding sets with "uat-embed-" prefix remain
- [ ] UAT archives ("test-archive", "uat-test-memory") deleted
- [ ] Pagination archive (`uat-pagination-memory`) deleted
- [ ] Active memory is "public" (default)
- [ ] System ready for production use or next test run
- [ ] No orphaned tags or broken references

## Important Notes

- **This phase is destructive** - it permanently deletes all UAT test data
- Always run this phase LAST in the test suite
- If cleanup fails, manual intervention may be required
- Tag prefix `uat/` ensures only test data is deleted
- Production notes are never tagged with `uat/` prefix
- Safe to run multiple times (idempotent)

## Manual Cleanup (If Automated Fails)

If automated cleanup fails, use these SQL queries as a last resort:

```sql
-- DO NOT USE unless MCP cleanup fails
-- Run as matric user against matric database

-- Delete UAT notes
DELETE FROM notes WHERE tags @> ARRAY['uat']::text[];

-- Delete UAT collections
DELETE FROM collections WHERE name ILIKE '%uat%';

-- Delete UAT embedding sets
DELETE FROM embedding_sets WHERE name ILIKE 'uat-embed-%';

-- Verify cleanup
SELECT COUNT(*) FROM notes WHERE tags @> ARRAY['uat']::text[];
-- Expected: 0
```

**Only use manual cleanup if:**
1. CLEAN-002 through CLEAN-006 all fail
2. MCP server is confirmed healthy
3. You have database admin access
4. Production data is confirmed safe

---

**End of UAT Phase 15**
