# UAT Phase 6: Graph & Links

**Purpose**: Validate graph exploration and semantic link discovery capabilities through MCP tools.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 1 completed (seed notes with content that may have generated semantic links)
- At least 3 notes created with related content to enable link generation

**Tools Tested**:
- `explore_graph` (graph traversal with configurable depth)
- `get_note_links` (direct link retrieval)

> **MCP-First Requirement**: Every test in this phase MUST use MCP tool calls exclusively. No direct HTTP requests, no curl commands. This validates the agent-first workflow that real AI assistants will experience.

---

## Test Cases

### GRAPH-001: Explore Graph from Note (Depth 1)

**MCP Tool**: `explore_graph`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: "<note_id_from_phase_1>",
    depth: 1
  }
});
```

**Expected Response**:
- Graph structure with nodes and edges
- Central node is the queried note
- Connected nodes at depth 1 (direct links only)
- Each node includes: id, title, snippet
- Each edge includes: source, target, similarity score

**Pass Criteria**:
- [ ] Response contains `nodes` array
- [ ] Response contains `edges` array
- [ ] Central note appears in nodes
- [ ] Depth respected (only 1-hop neighbors)

**Store**: `graph_center_note_id` for subsequent tests

---

### GRAPH-002: Explore Graph with Greater Depth

**MCP Tool**: `explore_graph`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: graph_center_note_id,
    depth: 2
  }
});
```

**Expected Response**:
- Graph structure with nodes at depth 1 and depth 2
- More nodes than GRAPH-001 (if 2-hop neighbors exist)
- Edges connecting depth-1 nodes to depth-2 nodes

**Pass Criteria**:
- [ ] Response contains nodes and edges
- [ ] Node count >= GRAPH-001 node count
- [ ] Graph includes 2-hop connections
- [ ] No nodes beyond depth 2

---

### GRAPH-003: Explore Graph with Result Limit

**MCP Tool**: `explore_graph`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: graph_center_note_id,
    depth: 1,
    limit: 5
  }
});
```

**Expected Response**:
- Graph structure limited to top 5 most relevant connections
- Nodes array length <= 6 (center + 5 neighbors)
- Highest similarity edges included first

**Pass Criteria**:
- [ ] Total nodes <= 6 (5 limit + 1 center)
- [ ] Response respects limit parameter
- [ ] Edges sorted by relevance/similarity

---

### GRAPH-004: Get Direct Note Links

**MCP Tool**: `get_note_links`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_note_links",
  arguments: {
    note_id: graph_center_note_id
  }
});
```

**Expected Response**:
- Array of link objects
- Each link includes: linked_note_id, title, similarity, created_at
- Links in both directions (incoming and outgoing)

**Pass Criteria**:
- [ ] Response is array
- [ ] Each link has required fields (linked_note_id, title, similarity)
- [ ] Similarity scores are between 0.0 and 1.0
- [ ] Links match those from GRAPH-001

**Store**: `linked_note_id` (any linked note for future tests)

---

### GRAPH-005: Get Links for Note with No Links

**MCP Tool**: `get_note_links`

```javascript
// Create isolated note first
const isolated = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "capture_knowledge",
  arguments: {
    action: "create",
    content: "Isolated note with unique content xyz123",
    tags: ["uat/graph", "uat/isolated"],
    revision_mode: "none"
  }
});

const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_note_links",
  arguments: {
    note_id: isolated.id
  }
});
```

**Expected Response**:
- Empty array `[]`
- No error thrown

**Pass Criteria**:
- [ ] Response is empty array
- [ ] No errors or null response
- [ ] Handles notes without links gracefully

---

### GRAPH-006: Explore Graph for Non-Existent Note

**Isolation**: Required

**MCP Tool**: `explore_graph`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: "00000000-0000-0000-0000-000000000000",
    depth: 1
  }
});
```

**Expected Response**:
- Error response indicating note not found
- Clear error message

**Pass Criteria**:
- [ ] Tool call fails with appropriate error
- [ ] Error message mentions note not found or invalid ID
- [ ] No partial graph returned

---

### GRAPH-007: Get Links for Non-Existent Note

**Isolation**: Required

**MCP Tool**: `get_note_links`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_note_links",
  arguments: {
    note_id: "00000000-0000-0000-0000-000000000000"
  }
});
```

**Expected Response**:
- Error response indicating note not found
- Clear error message

**Pass Criteria**:
- [ ] Tool call fails with appropriate error
- [ ] Error message mentions note not found or invalid ID
- [ ] No empty array (should error, not return empty)

---

### GRAPH-008: Explore Graph with Default Depth

**MCP Tool**: `explore_graph`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: graph_center_note_id
    // No depth parameter - test default behavior
  }
});
```

**Expected Response**:
- Graph structure with default depth applied (typically depth 1 or 2)
- Consistent behavior across calls with same parameters

**Pass Criteria**:
- [ ] Response contains valid graph structure
- [ ] Default depth is applied consistently
- [ ] Behavior matches documented default
- [ ] No errors from omitted depth parameter

---

## Phase Summary

| Metric | Target | Actual |
|--------|--------|--------|
| Tests Executed | 8 | ___ |
| Tests Passed | 8 | ___ |
| Tests Failed | 0 | ___ |
| Duration | ~5 min | ___ |
| Graph Depth Validated | ✓ | ___ |
| Link Discovery Validated | ✓ | ___ |
| Error Handling Validated | ✓ | ___ |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- Graph exploration requires pre-existing notes with semantic relationships
- Link generation is automatic based on content similarity (typically >70% threshold)
- If no links exist, verify Phase 1 created sufficiently related content
- Depth limits prevent performance issues on large graphs
