# UAT Phase 6: Graph & Links

**Purpose**: Validate graph exploration and semantic link discovery capabilities through MCP tools.

**Duration**: ~5 minutes

**Prerequisites**:
- Phase 1 completed (seed notes with content that may have generated semantic links)
- At least 3 notes created with related content to enable link generation

> **Graph Topology Setup**: This phase creates its own chain-topology notes (A→B→C) to ensure depth=2 traversal reliably discovers nodes beyond depth=1. While the new HNSW Algorithm 4 linking strategy (default) produces diverse neighbor connections that mitigate star topology issues, chain notes provide deterministic depth testing regardless of linking algorithm.

**Tools Tested**:
- `explore_graph` (graph traversal with configurable depth)
- `get_note_links` (direct link retrieval)
- `get_topology_stats` (graph health and linking strategy metrics)

> **MCP-First Requirement**: Every test in this phase MUST use MCP tool calls exclusively. No direct HTTP requests, no curl commands. This validates the agent-first workflow that real AI assistants will experience.

---

## Test Cases

### GRAPH-SETUP: Create Chain Topology Notes

**Purpose**: Create three notes with content designed to form a chain (A→B→C) where A links to B, B links to C, but A does NOT directly link to C. This guarantees depth=2 traversal discovers nodes that depth=1 cannot reach.

**MCP Tool**: `capture_knowledge` (action: create) — 3 calls

```javascript
// Note A: Machine learning topic
const noteA = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# Neural Network Architectures\n\nConvolutional neural networks (CNNs) use learnable filters to extract spatial features from input data. Key architectures include ResNet, VGG, and EfficientNet. Training involves backpropagation with gradient descent optimizers like Adam and SGD with momentum.",
  tags: ["uat/graph", "uat/graph-chain"],
  revision_mode: "none"
});

// Note B: Bridges ML and hardware — links to both A and C
const noteB = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# GPU Computing for Deep Learning\n\nGraphics processing units accelerate neural network training through massive parallelism. CUDA cores execute matrix multiplications for backpropagation. Modern GPUs like NVIDIA A100 provide tensor cores optimized for mixed-precision training of convolutional and transformer architectures.",
  tags: ["uat/graph", "uat/graph-chain"],
  revision_mode: "none"
});

// Note C: Hardware topic — links to B but not A
const noteC = await mcp.call_tool("capture_knowledge", {
  action: "create",
  content: "# Semiconductor Manufacturing Process\n\nModern chip fabrication uses extreme ultraviolet (EUV) lithography at 3nm and 5nm process nodes. TSMC and Samsung foundries produce processors and graphics processing units using silicon wafers. Yield optimization and thermal management are critical challenges in advanced semiconductor packaging.",
  tags: ["uat/graph", "uat/graph-chain"],
  revision_mode: "none"
});
```

**Pass Criteria**:
- [ ] All three notes created successfully with UUIDs
- [ ] Notes tagged with `uat/graph-chain` for cleanup

**Store**: `chain_note_a_id`, `chain_note_b_id`, `chain_note_c_id`

> **Wait**: Allow 5-10 seconds for the auto-linking pipeline to process embeddings and generate semantic links before proceeding to GRAPH-001.

---

### GRAPH-001: Explore Graph from Note (Depth 1)

**MCP Tool**: `explore_graph`

```javascript
// Use note A as the center — at depth 1, should find B but not C
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "explore_graph",
  arguments: {
    note_id: chain_note_a_id,
    depth: 1
  }
});
```

**Expected Response**:
- Graph structure with nodes and edges
- Central node is note A (neural networks)
- Connected nodes at depth 1: note B (GPU computing) — semantically similar via ML terms
- Note C (semiconductors) should NOT appear — no direct ML link
- Each node includes: id, title, snippet
- Each edge includes: source, target, similarity score

**Pass Criteria**:
- [ ] Response contains `nodes` array
- [ ] Response contains `edges` array
- [ ] Central note A appears in nodes
- [ ] Note B (GPU computing) appears as depth-1 neighbor
- [ ] Depth respected (only 1-hop neighbors)

**Store**: `graph_center_note_id = chain_note_a_id`, `depth1_node_count` (number of nodes returned)

> **Note**: If note C also appears at depth 1, the content overlap is too high. The chain topology relies on B being the semantic bridge. If this happens, the test still passes structurally but GRAPH-002 may not show additional depth-2 nodes.

---

### GRAPH-002: Explore Graph with Greater Depth

**MCP Tool**: `explore_graph`

```javascript
// Same center note A, but depth 2 — should now discover C via B
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
- Graph structure with nodes at depth 1 AND depth 2
- Depth 1: note B (GPU computing) — direct link from A
- Depth 2: note C (semiconductors) — reached via B's link to C
- More nodes than GRAPH-001 result
- Edges connecting A→B and B→C

**Pass Criteria**:
- [ ] Response contains nodes and edges
- [ ] Node count >= `depth1_node_count` from GRAPH-001
- [ ] Graph includes 2-hop connections (nodes reachable only via intermediary)
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

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

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

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

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

### GRAPH-009: Get Topology Statistics

**MCP Tool**: `get_topology_stats`

```javascript
const response = await use_mcp_tool({
  server_name: "matric-memory",
  tool_name: "get_topology_stats",
  arguments: {}
});
```

**Expected Response**:
- `total_notes`: integer >= 0 (should be > 0 after earlier phases)
- `total_links`: integer >= 0
- `isolated_nodes`: integer >= 0
- `connected_components`: integer >= 0
- `avg_degree`: float >= 0
- `max_degree`: integer >= 0
- `min_degree_linked`: integer >= 0
- `median_degree`: float >= 0
- `linking_strategy`: string (e.g., "HnswHeuristic" or "Threshold")
- `effective_k`: integer >= 5

**Pass Criteria**:
- [ ] Response contains all topology fields
- [ ] `total_notes` matches known count from earlier phases
- [ ] `linking_strategy` is present and non-empty
- [ ] `effective_k` >= 5 (minimum k from adaptive formula)
- [ ] No errors

**Store**: `topology_stats` (full response for GRAPH-010)

---

### GRAPH-010: Verify Linking Strategy is HNSW Heuristic

**Purpose**: Confirm the default linking strategy is HNSW Algorithm 4 (diverse neighbor selection).

**Validation** (uses stored `topology_stats` from GRAPH-009):

**Pass Criteria**:
- [ ] `linking_strategy` is "HnswHeuristic" (default)
- [ ] `effective_k` is between 5 and 15 (adaptive range)
- [ ] `avg_degree` is reasonable (> 0 if links exist)
- [ ] If `total_links > 0`: `isolated_nodes < total_notes` (not all nodes are isolated)

---

### GRAPH-011: Verify Diverse Linking (Anti-Star Topology)

**Purpose**: After auto-linking runs on Phase 1 seed notes, verify the graph does NOT form a pure star topology. HNSW Algorithm 4 should produce diverse connections.

**Validation** (uses stored `topology_stats` from GRAPH-009):

**Pass Criteria**:
- [ ] If `total_links >= 10`: `max_degree < total_links` (not all links on one hub)
- [ ] If `total_notes >= 5`: `connected_components <= total_notes / 2` (reasonable connectivity)
- [ ] `avg_degree` is within reasonable range (1-15 for typical UAT corpus)

> **Note**: These thresholds are intentionally loose. The UAT corpus is small (~10-20 notes), so perfect mesh topology is not expected. The goal is to confirm links are distributed, not concentrated on a single hub.

---

## Phase Summary

| Metric | Target | Actual |
|--------|--------|--------|
| Tests Executed | 12 | ___ |
| Tests Passed | 12 | ___ |
| Tests Failed | 0 | ___ |
| Duration | ~5 min | ___ |
| Chain Topology Created | ✓ | ___ |
| Graph Depth Validated | ✓ | ___ |
| Link Discovery Validated | ✓ | ___ |
| Error Handling Validated | ✓ | ___ |
| Topology Stats Validated | ✓ | ___ |
| Linking Strategy Confirmed | ✓ | ___ |
| Diverse Linking Verified | ✓ | ___ |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
- GRAPH-SETUP creates chain-topology notes (A→B→C) for reliable depth testing
- Auto-linked notes from Phase 1 may form star topologies where depth>1 adds no new nodes
- Link generation is automatic based on content similarity (typically >70% threshold)
- Allow 5-10 seconds after GRAPH-SETUP for auto-linking pipeline to generate links
- Depth limits prevent performance issues on large graphs
- `get_topology_stats` returns graph-wide metrics — useful for monitoring linking health
- Default linking strategy is `HnswHeuristic` (HNSW Algorithm 4, Malkov & Yashunin 2018)
- Linking strategy is configurable via `GRAPH_LINKING_STRATEGY` env var (threshold fallback available)
- Adaptive k computes `log₂(N)` clamped to [5, 15] based on corpus size
