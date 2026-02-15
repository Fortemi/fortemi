# UAT Phase 12: Feature Chains (End-to-End)

## Purpose
Test cross-cutting workflows that exercise multiple MCP tools in realistic sequences. Validates that features integrate correctly and data flows properly through complex operations.

## Duration
~15 minutes

## Prerequisites
- MCP server healthy
- Default memory active
- All prior phases (1-11) passed
- At least 100MB free disk space for bulk operations

## Tools Tested
- `capture_knowledge`
- `manage_tags`
- `manage_collection`
- `search`
- `export_note`
- `explore_graph`
- `get_knowledge_health`
- `record_provenance`
- `select_memory`
- `bulk_reprocess_notes`

---

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Direct API calls or curl commands are NOT acceptable. All note creation uses `revision_mode: "none"` and tags with `uat/` prefix.

---

## Chain 1: Knowledge Capture → Organization → Discovery

### CHAIN-001: Create Note with Rich Content
**MCP Tool**: `capture_knowledge`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: `# Project Alpha Launch Plan

## Objectives
- Launch new product line by Q2 2026
- Target 10,000 early adopters
- Achieve 85% customer satisfaction

## Key Milestones
1. Beta testing (Feb 2026)
2. Marketing campaign (Mar 2026)
3. Public launch (Apr 2026)

## Resources
Budget: $500K
Team: 12 people
Timeline: 90 days`,
    tags: ["uat/chain-1", "uat/project-alpha", "uat/launch-plan"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created with rich markdown content
**Pass Criteria**:
- Response contains `note_id`
- Content preserved with formatting
- All tags applied

**Store**: `alpha_note_id`

---

### CHAIN-002: Tag and Organize
**MCP Tool**: `manage_tags`, `manage_collection`

```javascript
// Add additional tags
const tagResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_tags",
  arguments: {
    action: "set",
    note_id: alpha_note_id,
    tags: [
      "uat/chain-1",
      "uat/project-alpha",
      "uat/launch-plan",
      "uat/q2-2026",
      "uat/high-priority"
    ]
  }
});

// Create collection
const collection = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "create",
    name: "UAT Project Alpha Collection",
    description: "All notes related to Project Alpha launch"
  }
});

// Move note to collection
const moveResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "move_note",
    collection_id: collection.id,
    note_id: alpha_note_id
  }
});
```

**Expected**: Tags updated and note added to collection
**Pass Criteria**:
- Tag count increases to 5
- Collection created successfully
- Note appears in collection

**Store**: `alpha_collection_id`

---

### CHAIN-003: Search Discovers Note
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "Project Alpha launch plan Q2",
    required_tags: ["uat/chain-1"]
  }
});
```

**Expected**: Search returns the Project Alpha note
**Pass Criteria**:
- Results array contains `alpha_note_id`
- Note appears in top 3 results
- Relevance score > 0.5

---

### CHAIN-004: Export Organized Note
**MCP Tool**: `export_note`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "export_note",
  arguments: {
    id: alpha_note_id,
    format: "markdown"
  }
});
```

**Expected**: Markdown export with YAML frontmatter
**Pass Criteria**:
- Response contains markdown content
- YAML frontmatter includes all 5 tags
- Frontmatter includes collection reference
- Content matches original note

---

### CHAIN-005: View in Graph
**MCP Tool**: `explore_graph`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: alpha_note_id,
    depth: 2
  }
});
```

**Expected**: Graph exploration returns node data
**Pass Criteria**:
- Response contains `nodes` array
- Alpha note appears as root node
- Graph structure is valid JSON

---

### CHAIN-006: Check Knowledge Health
**MCP Tool**: `get_knowledge_health`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_knowledge_health",
  arguments: {}
});
```

**Expected**: Health metrics reflect the new note and collection
**Pass Criteria**:
- `total_notes` count includes alpha note
- `total_collections` includes alpha collection
- `total_tags` reflects new tag count
- No critical health issues reported

---

## Chain 2: Provenance → Spatial Discovery

### CHAIN-007: Create Note
**MCP Tool**: `capture_knowledge`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Site visit to downtown office - construction progress looks good",
    tags: ["uat/chain-2", "uat/site-visit", "uat/downtown"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created successfully
**Pass Criteria**: Response contains `note_id`

**Store**: `site_visit_note_id`

---

### CHAIN-008: Record Location + Device + Note Provenance (3-Step Chain)
**MCP Tool**: `record_provenance`

> **Critical**: Spatial search requires a 3-step provenance chain:
> 1. Create a standalone **location** record (returns `location_id`)
> 2. Create a **device** record (returns `device_id`)
> 3. Create **note** provenance linking the note to the location (and optionally device)
>
> Skipping step 3 means the note has no spatial provenance and will NOT appear in spatial search results.

```javascript
// Step 1: Create standalone location record
const locationProv = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "location",
    latitude: 40.7589,
    longitude: -73.9851,
    horizontal_accuracy_m: 10.0,
    source: "user_manual",
    confidence: "high"
  }
});
// Store: location_id = locationProv.id

// Step 2: Create device record
const deviceProv = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "device",
    device_make: "Apple",
    device_model: "iPhone 15 Pro",
    device_os: "iOS",
    device_os_version: "17.2",
    software: "Fortemi",
    software_version: "1.0.0"
  }
});
// Store: device_id = deviceProv.id

// Step 3: Link note to location + device via note provenance
const noteProv = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "record_provenance",
  arguments: {
    action: "note",
    note_id: site_visit_note_id,
    location_id: locationProv.id,
    device_id: deviceProv.id,
    capture_time_start: new Date().toISOString(),
    time_source: "user_manual",
    time_confidence: "high"
  }
});
```

**Expected**: All three provenance records created and linked
**Pass Criteria**:
- Location record returns `id` with coordinates
- Device record returns `id` with make/model
- Note provenance links note to location_id and device_id
- Note now discoverable via spatial search (CHAIN-009)

---

### CHAIN-009: Spatial Search Finds Provenance Note
**MCP Tool**: `search`

> **Prerequisite**: CHAIN-008 must have completed the full 3-step provenance chain (location → note provenance linking). Without the `record_provenance(action=note)` step, the note will NOT appear in spatial results.

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "spatial",
    lat: 40.7589,
    lon: -73.9851,
    radius: 100,
    required_tags: ["uat/chain-2"]
  }
});
```

**Expected**: Search returns the site visit note
**Pass Criteria**:
- Results array contains `site_visit_note_id`
- Distance calculation < 100 meters
- Location metadata included

> **Note**: Parameter names are `lat`/`lon`/`radius` (not `latitude`/`longitude`/`radius_meters`). Tag filtering uses `required_tags` (AND logic).

---

### CHAIN-010: Temporal Search Finds Provenance Note
**MCP Tool**: `search`

```javascript
const now = new Date();
const oneHourAgo = new Date(now.getTime() - 3600000);
const oneHourFromNow = new Date(now.getTime() + 3600000);

const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "temporal",
    start: oneHourAgo.toISOString(),
    end: oneHourFromNow.toISOString(),
    required_tags: ["uat/chain-2"]
  }
});
```

**Expected**: Search returns recently created note
**Pass Criteria**:
- Results array contains `site_visit_note_id`
- Timestamp falls within specified range
- Temporal metadata accurate

> **Note**: Parameter names are `start`/`end` (not `start_time`/`end_time`). Tag filtering uses `required_tags` (AND logic).

---

## Chain 3: Multi-Memory Isolation

### CHAIN-011: Create Note in Default Memory
**MCP Tool**: `select_memory`, `capture_knowledge`

```javascript
// Ensure default memory active
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "public"
  }
});

const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "This note belongs to the default memory",
    tags: ["uat/chain-3", "uat/default-memory"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created in public/default memory
**Pass Criteria**: Response contains `note_id`

**Store**: `default_memory_note_id`

---

### CHAIN-012: Provision Test Archive, Switch Memory, Create Note
**MCP Tool**: `manage_archives`, `select_memory`, `capture_knowledge`

> **Archive Provisioning**: The test archive must be created before it can be selected.
> Use `manage_archives` to create it. If the archive already exists (e.g., from PF-006), the error is safe to ignore.

```javascript
// Step 0: Provision the test archive via MCP
const archiveResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_archives",
  arguments: {
    action: "create",
    name: "uat-test-memory",
    description: "UAT test memory for multi-memory validation"
  }
});
// Success or "already exists" error — both are OK

// Switch to UAT test memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: {
    name: "uat-test-memory"
  }
});

const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "This note belongs to the UAT test memory",
    tags: ["uat/chain-3", "uat/test-memory"],
    revision_mode: "none"
  }
});
```

**Expected**: Note created in separate memory archive
**Pass Criteria**: Response contains `note_id` (different from default)

**Store**: `test_memory_note_id`

---

### CHAIN-013: Search in Each Memory Returns Only Its Notes
**MCP Tool**: `search`, `select_memory`

```javascript
// Search in default memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "public" }
});

const defaultResults = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "memory",
    required_tags: ["uat/chain-3"]
  }
});

// Search in test memory
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "select_memory",
  arguments: { name: "uat-test-memory" }
});

const testResults = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "memory",
    required_tags: ["uat/chain-3"]
  }
});
```

**Expected**: Each search returns only its memory's notes
**Pass Criteria**:
- Default search finds `default_memory_note_id` only
- Test search finds `test_memory_note_id` only
- No cross-memory contamination

---

### CHAIN-014: Federated Search Finds Both
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "federated",
    query: "memory isolation test",
    memories: ["public", "uat-test-memory"],
    required_tags: ["uat/chain-3"]
  }
});
```

**Expected**: Federated search returns notes from both memories
**Pass Criteria**:
- Results include both `default_memory_note_id` and `test_memory_note_id`
- Each result tagged with source memory
- Relevance ranking works across memories

---

## Chain 4: Collection Lifecycle

### CHAIN-015: Create Collection with Multiple Notes
**MCP Tool**: `manage_collection`, `capture_knowledge`

```javascript
// Create collection
const collection = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "create",
    name: "UAT Chain 4 Collection",
    description: "Multi-note collection lifecycle test"
  }
});

// Create first note
const note1 = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "First note in collection lifecycle test",
    tags: ["uat/chain-4", "uat/collection-test"],
    revision_mode: "none"
  }
});

// Create second note
const note2 = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Second note in collection lifecycle test",
    tags: ["uat/chain-4", "uat/collection-test"],
    revision_mode: "none"
  }
});

// Move both notes to collection
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "move_note",
    collection_id: collection.id,
    note_id: note1.note_id
  }
});

await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "move_note",
    collection_id: collection.id,
    note_id: note2.note_id
  }
});
```

**Expected**: Collection created with 2 notes
**Pass Criteria**:
- Collection exists
- Both notes successfully added
- Collection note count = 2

**Store**: `lifecycle_collection_id`, `lifecycle_note1_id`, `lifecycle_note2_id`

---

### CHAIN-016: Export Collection
**MCP Tool**: `manage_collection`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "export",
    collection_id: lifecycle_collection_id,
    format: "markdown"
  }
});
```

**Expected**: Markdown export containing both notes
**Pass Criteria**:
- Export includes collection metadata
- Both note contents present
- Proper markdown formatting

---

### CHAIN-017: Delete Collection, Notes Survive
**MCP Tool**: `manage_collection`, `get_note`

```javascript
// Delete collection
await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "manage_collection",
  arguments: {
    action: "delete",
    collection_id: lifecycle_collection_id
  }
});

// Verify notes still exist
const note1Check = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_note",
  arguments: {
    id: lifecycle_note1_id
  }
});

const note2Check = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_note",
  arguments: {
    id: lifecycle_note2_id
  }
});
```

**Expected**: Collection deleted but notes remain accessible
**Pass Criteria**:
- Collection no longer exists
- Both notes retrieved successfully
- Note content unchanged

---

## Chain 5: Bulk Operations

### CHAIN-018: Create 5 Notes, Bulk Reprocess
**MCP Tool**: `capture_knowledge`, `bulk_reprocess_notes`

```javascript
// Bulk create 5 notes
const bulkResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "bulk_create",
    notes: [
      {
        content: "Bulk note 1: AI research findings",
        tags: ["uat/chain-5", "uat/bulk-test", "uat/ai"],
        revision_mode: "none"
      },
      {
        content: "Bulk note 2: Machine learning applications",
        tags: ["uat/chain-5", "uat/bulk-test", "uat/ml"],
        revision_mode: "none"
      },
      {
        content: "Bulk note 3: Natural language processing",
        tags: ["uat/chain-5", "uat/bulk-test", "uat/nlp"],
        revision_mode: "none"
      },
      {
        content: "Bulk note 4: Computer vision techniques",
        tags: ["uat/chain-5", "uat/bulk-test", "uat/vision"],
        revision_mode: "none"
      },
      {
        content: "Bulk note 5: Deep learning frameworks",
        tags: ["uat/chain-5", "uat/bulk-test", "uat/deep-learning"],
        revision_mode: "none"
      }
    ]
  }
});

// Bulk reprocess all created notes
const noteIds = bulkResult.notes.map(n => n.note_id);
const reprocessResult = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "bulk_reprocess_notes",
  arguments: {
    note_ids: noteIds
  }
});
```

**Expected**: 5 notes created and reprocessed
**Pass Criteria**:
- Bulk create returns 5 note IDs
- Reprocess completes without errors
- All notes have updated embeddings

**Store**: `bulk_note_ids`

---

### CHAIN-019: Monitor with Health Check
**MCP Tool**: `get_knowledge_health`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_knowledge_health",
  arguments: {}
});
```

**Expected**: Health metrics show increased note count
**Pass Criteria**:
- `total_notes` increased by 5
- No orphaned notes
- Embedding coverage maintained

---

### CHAIN-020: Search Finds Reprocessed Notes
**MCP Tool**: `search`

```javascript
const result = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "search",
  arguments: {
    action: "text",
    query: "machine learning AI",
    required_tags: ["uat/chain-5"]
  }
});
```

**Expected**: Search returns bulk-created notes with good relevance
**Pass Criteria**:
- Results include bulk notes
- Relevance scores > 0.3
- Embeddings working correctly

---

## Phase Summary

| Chain | Pass | Fail | Skip | Total |
|-------|------|------|------|-------|
| Chain 1: Capture → Organization → Discovery | 0 | 0 | 0 | 6 |
| Chain 2: Provenance → Spatial Discovery | 0 | 0 | 0 | 4 |
| Chain 3: Multi-Memory Isolation | 0 | 0 | 0 | 4 |
| Chain 4: Collection Lifecycle | 0 | 0 | 0 | 3 |
| Chain 5: Bulk Operations | 0 | 0 | 0 | 3 |
| **Total** | **0** | **0** | **0** | **20** |

## Phase Result
- [ ] **Phase 12 PASSED** - All feature chains completed successfully
- [ ] **Phase 12 FAILED** - See failure details above
- [ ] **Phase 12 SKIPPED** - Reason: _______________

## Notes
- Switch back to default memory after Chain 3 completion
- Bulk operations may take 30-60 seconds to complete
- Federated search requires multi-memory feature enabled
