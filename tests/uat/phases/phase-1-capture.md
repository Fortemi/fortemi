# UAT Phase 1: Knowledge Capture

**Purpose**: Verify all knowledge capture workflows including single note creation, bulk operations, template instantiation, and file upload guidance.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 0 (Preflight & System) completed successfully
- System health verified
- MCP tools available

**Tools Tested**: `capture_knowledge` (actions: create, bulk_create, from_template, upload)

---

> **MCP-First Requirement**
>
> All tests use the consolidated `capture_knowledge` tool with action-specific parameters.
> No legacy create_note or upload_file tools. All created notes use `revision_mode: "none"` to prevent auto-queuing background jobs during testing.

---

## Test Cases

### CK-001: Create Basic Note
**MCP Tool**: `capture_knowledge` (action: create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# UAT Test Note\n\nThis is a basic test note created during Phase 1.",
  tags: ["uat/capture"],
  revision_mode: "none"
});
```

**Expected Response**:
```json
{
  "id": "uuid-here",
  "content": "# UAT Test Note...",
  "tags": ["uat/capture"],
  "created_at": "2026-02-14T...",
  "updated_at": "2026-02-14T..."
}
```

**Pass Criteria**:
- [ ] Response contains valid UUID `id`
- [ ] Content matches input
- [ ] Tags array includes "uat/capture"
- [ ] Timestamps are present and valid ISO 8601 format

**Store**: `basic_note_id` (for CRUD phase)

---

### CK-002: Create Note with Metadata
**MCP Tool**: `capture_knowledge` (action: create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# Metadata Test\n\nNote with custom metadata fields.",
  tags: ["uat/capture", "uat/metadata"],
  metadata: {
    test_phase: "phase-1",
    priority: "high",
    custom_field: "example_value"
  },
  revision_mode: "none"
});
```

**Expected Response**:
```json
{
  "id": "uuid-here",
  "content": "# Metadata Test...",
  "tags": ["uat/capture", "uat/metadata"],
  "metadata": {
    "test_phase": "phase-1",
    "priority": "high",
    "custom_field": "example_value"
  }
}
```

**Pass Criteria**:
- [ ] Metadata object preserved exactly as provided
- [ ] All three custom fields present in response
- [ ] Note created successfully with tags

**Store**: `metadata_note_id`

---

### CK-003: Create with Hierarchical Tags
**MCP Tool**: `capture_knowledge` (action: create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# Hierarchical Tag Test\n\nTesting deep tag hierarchy.",
  tags: ["uat/capture/hierarchy/deep/nested"],
  revision_mode: "none"
});
```

**Expected Response**:
```json
{
  "id": "uuid-here",
  "tags": ["uat/capture/hierarchy/deep/nested"]
}
```

**Pass Criteria**:
- [ ] Full hierarchical tag path preserved
- [ ] Tag hierarchy uses forward slashes correctly
- [ ] Note retrievable via parent tag prefix filters (verified in Phase 2)

**Store**: `hierarchy_note_id`

---

### CK-004: Create with Explicit Revision Mode None
**MCP Tool**: `capture_knowledge` (action: create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# No Revision Test\n\nExplicitly disabling AI revision.",
  tags: ["uat/capture"],
  revision_mode: "none"
});
```

**Expected Response**:
```json
{
  "id": "uuid-here",
  "content": "# No Revision Test...",
  "revision_mode": "none"
}
```

**Pass Criteria**:
- [ ] Note created without queuing background revision job
- [ ] Response confirms `revision_mode: "none"` if field is returned
- [ ] No job worker activity triggered

---

### CK-005: Bulk Create Notes
**MCP Tool**: `capture_knowledge` (action: bulk_create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "bulk_create",
  notes: [
    {
      content: "# Bulk Note 1\n\nFirst note in batch.",
      tags: ["uat/capture", "uat/bulk"],
      revision_mode: "none"
    },
    {
      content: "# Bulk Note 2\n\nSecond note in batch.",
      tags: ["uat/capture", "uat/bulk"],
      revision_mode: "none"
    },
    {
      content: "# Bulk Note 3\n\nThird note in batch.",
      tags: ["uat/capture", "uat/bulk"],
      revision_mode: "none"
    }
  ]
});
```

**Expected Response**:
```json
{
  "created": 3,
  "ids": ["uuid-1", "uuid-2", "uuid-3"],
  "notes": [
    { "id": "uuid-1", "content": "# Bulk Note 1..." },
    { "id": "uuid-2", "content": "# Bulk Note 2..." },
    { "id": "uuid-3", "content": "# Bulk Note 3..." }
  ]
}
```

**Pass Criteria**:
- [ ] Response indicates 3 notes created
- [ ] `ids` array contains 3 valid UUIDs
- [ ] `notes` array contains full note objects
- [ ] All notes have correct tags

**Store**: `bulk_note_ids` (array for cleanup)

---

### CK-006: Bulk Create Validates Results
**MCP Tool**: `capture_knowledge` (action: bulk_create)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "bulk_create",
  notes: [
    { content: "Note A", tags: ["uat/validation"], revision_mode: "none" },
    { content: "Note B", tags: ["uat/validation"], revision_mode: "none" }
  ]
});

// Verify count matches
console.assert(result.created === 2, "Created count mismatch");
console.assert(result.ids.length === 2, "IDs array length mismatch");
console.assert(result.notes.length === 2, "Notes array length mismatch");
```

**Expected Response**:
```json
{
  "created": 2,
  "ids": ["uuid-a", "uuid-b"],
  "notes": [...]
}
```

**Pass Criteria**:
- [ ] `created` count equals input array length
- [ ] `ids` array length matches `created` count
- [ ] `notes` array length matches `created` count
- [ ] All assertions pass

---

### CK-007: Create from Template
**MCP Tool**: `capture_knowledge` (action: from_template)

```javascript
// Assumes a template "meeting-notes" exists in the system
// Or uses a system default template
const result = await mcp.call_tool("capture_knowledge", {
  action: "from_template",
  template_name: "meeting-notes",
  variables: {
    title: "UAT Planning Meeting",
    date: "2026-02-14",
    attendees: "QA Team"
  },
  tags: ["uat/capture", "uat/template"],
  revision_mode: "none"
});
```

**Expected Response**:
```json
{
  "id": "uuid-here",
  "content": "# UAT Planning Meeting\n\nDate: 2026-02-14\nAttendees: QA Team\n\n...",
  "tags": ["uat/capture", "uat/template"]
}
```

**Pass Criteria**:
- [ ] Note created with template structure
- [ ] Variables substituted correctly in content
- [ ] Tags applied as specified
- [ ] If template doesn't exist, error message is clear

**Note**: If no "meeting-notes" template exists, this test may need to use a known system template or create one first. Adjust template_name based on system state.

**Store**: `template_note_id`

---

### CK-008: Upload Guidance
**MCP Tool**: `capture_knowledge` (action: upload)

```javascript
const result = await mcp.call_tool("capture_knowledge", {
  action: "upload"
});
```

**Expected Response**:
```json
{
  "instructions": "Use the following curl command to upload files...",
  "endpoint": "https://your-domain.com/api/v1/notes/upload",
  "method": "POST",
  "example": "curl -X POST https://... -F 'file=@document.pdf' -F 'tags=uat/upload'"
}
```

**Pass Criteria**:
- [ ] Response provides upload instructions
- [ ] Endpoint URL is included
- [ ] Example curl command or equivalent guidance provided
- [ ] No actual upload occurs (guidance only)

---

### CK-009: Invalid Action Error
**MCP Tool**: `capture_knowledge` (action: invalid)

**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
try {
  await mcp.call_tool("capture_knowledge", {
    action: "invalid_action",
    content: "This should fail"
  });
  console.error("FAIL: Expected error for invalid action");
} catch (error) {
  console.log("PASS: Error caught as expected");
  console.log("Error message:", error.message);
}
```

**Expected Response**:
```json
{
  "error": "Invalid action: invalid_action. Available actions: create, bulk_create, from_template, upload"
}
```

**Pass Criteria**:
- [ ] Tool call throws or returns error
- [ ] Error message clearly indicates invalid action
- [ ] Error lists valid actions
- [ ] No note created

---

### CK-010: Create Without Content Error
**MCP Tool**: `capture_knowledge` (action: create)

**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
try {
  await mcp.call_tool("capture_knowledge", {
    action: "create",
    tags: ["uat/capture"]
    // Missing required 'content' field
  });
  console.error("FAIL: Expected error for missing content");
} catch (error) {
  console.log("PASS: Error caught as expected");
  console.log("Error message:", error.message);
}
```

**Expected Response**:
```json
{
  "error": "Missing required field: content"
}
```

**Pass Criteria**:
- [ ] Tool call throws or returns error
- [ ] Error message indicates missing required field
- [ ] Error specifically mentions "content"
- [ ] No note created

---

## Phase Summary

| Test ID | Tool | Action | Focus | Result |
|---------|------|--------|-------|--------|
| CK-001  | capture_knowledge | create | Basic note creation | ⬜ |
| CK-002  | capture_knowledge | create | Metadata preservation | ⬜ |
| CK-003  | capture_knowledge | create | Hierarchical tags | ⬜ |
| CK-004  | capture_knowledge | create | Revision mode control | ⬜ |
| CK-005  | capture_knowledge | bulk_create | Batch creation | ⬜ |
| CK-006  | capture_knowledge | bulk_create | Result validation | ⬜ |
| CK-007  | capture_knowledge | from_template | Template instantiation | ⬜ |
| CK-008  | capture_knowledge | upload | Upload guidance | ⬜ |
| CK-009  | capture_knowledge | (invalid) | Error handling | ⬜ |
| CK-010  | capture_knowledge | create | Validation error | ⬜ |

**Phase Result**: ⬜ PASS / ⬜ FAIL

**Notes**:
