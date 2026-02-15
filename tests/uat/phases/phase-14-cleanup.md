# UAT Phase 14: Final Cleanup

## Purpose
Remove ALL UAT test data from the system to restore clean state. This phase MUST run last and ensures no test artifacts remain after the test suite completes.

## Duration
~3 minutes

## Prerequisites
- All previous phases (1-13) completed
- MCP server healthy
- Access to all memories used during testing

## Tools Tested
- `list_notes`
- `delete_note`
- `manage_collection`
- `manage_embeddings`
- `select_memory`

---

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Direct API calls or curl commands are NOT acceptable. This cleanup phase removes all notes with `uat/` tag prefix, all UAT-related collections, and all UAT embedding sets.

---

## Test Cases

### CLEAN-001: List All UAT Notes
**MCP Tool**: `list_notes`

```javascript
// Switch to default memory first
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

// List all UAT notes in default memory
const defaultNotes = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: {
    tags: ["uat"],
    limit: 500,
    include_deleted: false
  }
});

// Switch to test memory if it was created
try {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "select_memory",
    arguments: { name: "uat-test-memory" }
  });

  const testMemoryNotes = await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "list_notes",
    arguments: {
      tags: ["uat"],
      limit: 500,
      include_deleted: false
    }
  });
} catch (e) {
  // Test memory may not exist, continue
}
```

**Expected**: List of all notes tagged with `uat` or `uat/*`
**Pass Criteria**:
- Query completes without errors
- Returns array of note objects
- Each note has at least one tag starting with "uat"

**Store**: `uat_notes_to_delete` (array of note IDs)

---

### CLEAN-002: Delete All UAT Notes
**MCP Tool**: `delete_note`

```javascript
// Delete all UAT notes from default memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

const defaultNotes = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: { tags: ["uat"], limit: 500 }
});

for (const note of defaultNotes.notes) {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "delete_note",
    arguments: { id: note.id }
  });
}

// Delete all UAT notes from test memory if it exists
try {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "select_memory",
    arguments: { name: "uat-test-memory" }
  });

  const testMemoryNotes = await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "list_notes",
    arguments: { tags: ["uat"], limit: 500 }
  });

  for (const note of testMemoryNotes.notes) {
    await use_mcp_tool({
      server_name: "matric-memory",
      tool_name: "delete_note",
      arguments: { id: note.id }
    });
  }
} catch (e) {
  // Test memory may not exist or may be empty
}
```

**Expected**: All UAT notes deleted successfully
**Pass Criteria**:
- Each delete operation completes without errors
- No UAT notes remain active
- Deletion count matches list count from CLEAN-001

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
// Verify default memory cleanup
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

const defaultVerify = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: {
    tags: ["uat"],
    limit: 500,
    include_deleted: false
  }
});

// Verify test memory cleanup if it exists
let testVerify = { notes: [] };
try {
  await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "select_memory",
    arguments: { name: "uat-test-memory" }
  });

  testVerify = await use_mcp_tool({
    server_name: "matric-memory",
    tool_name: "list_notes",
    arguments: {
      tags: ["uat"],
      limit: 500,
      include_deleted: false
    }
  });
} catch (e) {
  // Expected if memory doesn't exist
}

const totalRemaining = defaultVerify.notes.length + testVerify.notes.length;
```

**Expected**: Zero UAT notes remaining
**Pass Criteria**:
- Default memory UAT note count = 0
- Test memory UAT note count = 0 (or memory doesn't exist)
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

### CLEAN-006: Switch to Default Memory
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
- Notes created across all phases: ~60-80 notes
- Collections created: 3-5 collections
- Embedding sets created: 2-3 sets (with `uat-embed-` prefix)
- Memories used: 1-2 (public + optional uat-test-memory)

**Cleanup verification:**
```javascript
// Final verification query
const finalCheck = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "list_notes",
  arguments: { tags: ["uat"], limit: 1 }
});

if (finalCheck.notes.length === 0) {
  console.log("Cleanup successful - zero UAT notes remaining");
} else {
  console.error(`Cleanup incomplete - ${finalCheck.notes.length} UAT notes still exist`);
}
```

---

## Phase Summary

| Category | Pass | Fail | Skip | Total |
|----------|------|------|------|-------|
| UAT Note Cleanup | 0 | 0 | 0 | 2 |
| Collection Cleanup | 0 | 0 | 0 | 1 |
| Verification | 0 | 0 | 0 | 1 |
| Embedding Set Cleanup | 0 | 0 | 0 | 1 |
| Memory Reset | 0 | 0 | 0 | 1 |
| **Total** | **0** | **0** | **0** | **6** |

## Phase Result
- [ ] **Phase 14 PASSED** - All UAT data successfully removed
- [ ] **Phase 14 FAILED** - See failure details above
- [ ] **Phase 14 SKIPPED** - Reason: _______________

## Post-Cleanup Checklist

After CLEAN-006 completes:
- [ ] Zero notes with `uat` tags remain
- [ ] Zero collections with "uat" in name remain
- [ ] Zero embedding sets with "uat-embed-" prefix remain
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
1. CLEAN-002 through CLEAN-005 all fail
2. MCP server is confirmed healthy
3. You have database admin access
4. Production data is confirmed safe

---

**End of UAT Phase 14**
