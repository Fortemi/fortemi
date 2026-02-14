# UAT Phase 10: Export, Health & Bulk Operations

## Purpose
Validate note export functionality, knowledge health monitoring, and bulk reprocessing operations. Tests verify markdown export with frontmatter, health metrics collection, and bulk note pipeline execution.

## Duration
~5 minutes

## Prerequisites
- Phase 1 completed (notes exist for export and reprocessing)
- At least one note with ID stored from previous phases

## Tools Tested
- `export_note`
- `get_knowledge_health`
- `bulk_reprocess_notes`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls through the matric-memory MCP server. Direct HTTP API calls are NOT permitted. Use `mcp.request({ method: "tools/call", params: { name: "tool_name", arguments: {...} }})`.

---

## Test Cases

### EXP-001: Export Note Markdown
**MCP Tool**: `export_note`

Export a note in markdown format.

```javascript
const exported = await mcp.request({
  method: "tools/call",
  params: {
    name: "export_note",
    arguments: {
      id: NOTE_ID_FROM_PHASE1,
      format: "markdown"
    }
  }
});

console.log("Exported content length:", exported.content.length);
console.log("First 200 chars:", exported.content.substring(0, 200));
```

**Expected**:
- Returns markdown content string
- Content contains note title and body
- Format is valid markdown

**Pass Criteria**:
- Response contains `content` field
- Content length > 0
- Content includes note title from Phase 1

**Store**: `EXPORTED_MARKDOWN`

---

### EXP-002: Export Note with Frontmatter
**MCP Tool**: `export_note`

Verify exported markdown includes YAML frontmatter with metadata.

```javascript
const exported = await mcp.request({
  method: "tools/call",
  params: {
    name: "export_note",
    arguments: {
      id: NOTE_ID_FROM_PHASE1,
      format: "markdown"
    }
  }
});

// Check for YAML frontmatter
const hasFrontmatter = exported.content.startsWith("---\n");
const frontmatterEnd = exported.content.indexOf("\n---\n", 4);

console.log("Has frontmatter:", hasFrontmatter);
if (hasFrontmatter && frontmatterEnd > 0) {
  const frontmatter = exported.content.substring(0, frontmatterEnd + 5);
  console.log("Frontmatter:\n", frontmatter);
}
```

**Expected**:
- Exported markdown begins with `---\n`
- Contains YAML frontmatter section
- Frontmatter includes fields like `title`, `created_at`, `tags`
- Frontmatter is properly terminated with `---`

**Pass Criteria**:
- Content starts with `---\n`
- Second `---` marker found after first
- Frontmatter section contains at least `title:` field
- Content after frontmatter is note body

---

### EXP-003: Export Non-Existent Note
**MCP Tool**: `export_note`

**Isolation**: Required

Attempt to export note that doesn't exist.

```javascript
try {
  await mcp.request({
    method: "tools/call",
    params: {
      name: "export_note",
      arguments: {
        id: "00000000-0000-0000-0000-000000000000",
        format: "markdown"
      }
    }
  });
  console.error("FAIL: Should have thrown 404 for missing note");
} catch (error) {
  console.log("Correctly rejected missing note:", error.message);
}
```

**Expected**:
- Request fails with 404 error
- Error indicates note not found
- Error message references the note ID

**Pass Criteria**:
- Request throws error
- Error message contains "not found" or "404"
- No export content returned

---

### HEALTH-001: Knowledge Health Dashboard
**MCP Tool**: `get_knowledge_health`

Retrieve knowledge health metrics for the active memory.

```javascript
const health = await mcp.request({
  method: "tools/call",
  params: {
    name: "get_knowledge_health",
    arguments: {}
  }
});

console.log("Health metrics:", JSON.stringify(health, null, 2));
```

**Expected**:
- Returns health metrics object
- Contains fields for various health indicators
- Typical fields: `orphan_tags`, `stale_notes`, `unlinked_notes`, `total_notes`, `total_tags`
- All metrics are numeric

**Pass Criteria**:
- Response is object (not array)
- Contains at least 3 health metric fields
- All metric values are numbers
- No negative values

**Store**: `HEALTH_METRICS`

---

### HEALTH-002: Health Metrics Are Numeric
**MCP Tool**: `get_knowledge_health`

Verify all health metrics return valid numeric values.

```javascript
const health = await mcp.request({
  method: "tools/call",
  params: {
    name: "get_knowledge_health",
    arguments: {}
  }
});

const allNumeric = Object.entries(health).every(([key, value]) => {
  const isNum = typeof value === "number" && value >= 0;
  console.log(`${key}: ${value} (${typeof value}) - ${isNum ? "OK" : "FAIL"}`);
  return isNum;
});

console.log("All metrics numeric and non-negative:", allNumeric);
```

**Expected**:
- All health metric values are type `number`
- All values are >= 0
- No `null`, `undefined`, or string values
- Integer values for count-based metrics

**Pass Criteria**:
- Every field in health response is `typeof number`
- Every field value >= 0
- At least 3 fields present
- `allNumeric` evaluates to `true`

---

### BULK-001: Bulk Reprocess Specific Notes
**MCP Tool**: `bulk_reprocess_notes`

Reprocess specific notes with targeted pipeline steps.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "bulk_reprocess_notes",
    arguments: {
      note_ids: [NOTE_ID_FROM_PHASE1],
      revision_mode: "light",
      steps: ["embedding"]
    }
  }
});

console.log("Reprocessed:", result.processed_count);
console.log("Failed:", result.failed_count);
console.log("Job ID:", result.job_id);
```

**Expected**:
- Returns job status with counts
- `processed_count` >= 1 (at least the specified note)
- `failed_count` = 0 (assuming valid note)
- `job_id` present for tracking
- Embedding regeneration completes

**Pass Criteria**:
- Response contains `processed_count`, `failed_count`, `job_id`
- `processed_count` >= 1
- `failed_count` = 0
- Job ID is UUID format

**Store**: `BULK_JOB_ID`

---

### BULK-002: Bulk Reprocess with Limit
**MCP Tool**: `bulk_reprocess_notes`

Reprocess notes with limit constraint (no specific note IDs).

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "bulk_reprocess_notes",
    arguments: {
      revision_mode: "light",
      limit: 3
    }
  }
});

console.log("Reprocessed count:", result.processed_count);
console.log("Limit respected:", result.processed_count <= 3);
```

**Expected**:
- Returns job status
- `processed_count` <= 3 (respects limit)
- Processes most recently updated notes (default ordering)
- `failed_count` reported

**Pass Criteria**:
- Response contains `processed_count`
- `processed_count` <= 3
- `processed_count` >= 0 (could be 0 if no notes)
- Limit constraint honored

---

### BULK-003: Bulk Reprocess All Steps
**MCP Tool**: `bulk_reprocess_notes`

Reprocess note with all pipeline steps enabled.

```javascript
const result = await mcp.request({
  method: "tools/call",
  params: {
    name: "bulk_reprocess_notes",
    arguments: {
      note_ids: [NOTE_ID_FROM_PHASE1],
      steps: ["all"]
    }
  }
});

console.log("Full pipeline reprocess:", result.processed_count);
console.log("Job ID:", result.job_id);
```

**Expected**:
- Returns job status
- All pipeline steps execute (embedding, linking, extraction, etc.)
- `processed_count` >= 1
- No failures for valid note
- Job completes successfully

**Pass Criteria**:
- Response contains `processed_count`, `job_id`
- `processed_count` >= 1
- `failed_count` = 0
- Job ID present

---

## Phase Summary

| Test ID | Tool | Status | Notes |
|---------|------|--------|-------|
| EXP-001 | export_note | [ ] | Basic markdown export |
| EXP-002 | export_note | [ ] | YAML frontmatter validation |
| EXP-003 | export_note | [ ] | Missing note error handling |
| HEALTH-001 | get_knowledge_health | [ ] | Health metrics retrieval |
| HEALTH-002 | get_knowledge_health | [ ] | Numeric validation |
| BULK-001 | bulk_reprocess_notes | [ ] | Targeted reprocessing |
| BULK-002 | bulk_reprocess_notes | [ ] | Limit constraint |
| BULK-003 | bulk_reprocess_notes | [ ] | Full pipeline |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Export tests validate markdown format and YAML frontmatter structure
- Health metrics provide observability into knowledge base quality
- Bulk reprocessing supports note_ids (specific), limit (batch size), and steps (pipeline control)
- All bulk operations return job IDs for async tracking
