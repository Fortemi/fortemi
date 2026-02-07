# UAT Phase 6: Semantic Links

**Purpose**: Verify bidirectional semantic linking and graph exploration
**Duration**: ~5 minutes
**Prerequisites**: Phase 1 seed data with embeddings generated
**Tools Tested**: `get_note_links`, `explore_graph`, `get_full_document`, `get_chunk_chain`, `search_with_dedup`, `get_note_backlinks`, `get_note_provenance`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Semantic Links

### LINK-001: Get Note Links

**MCP Tool**: `get_note_links`

```javascript
get_note_links({ id: "<ml_note_id>" })
```

**Expected Response**:
```json
{
  "outgoing": [
    { "to_note_id": "...", "score": 0.82, "kind": "semantic" }
  ],
  "incoming": [
    { "from_note_id": "...", "score": 0.78, "kind": "semantic" }
  ]
}
```

**Pass Criteria**: Returns `outgoing` and `incoming` arrays

---

### LINK-002: Verify Bidirectional Links

**MCP Tool**: `get_note_links`

```javascript
// Get links from note A
const linksA = get_note_links({ id: "<note_a_id>" })

// If A links to B, B should link back to A
const linkedNoteId = linksA.outgoing[0].to_note_id
const linksB = get_note_links({ id: linkedNoteId })
```

**Pass Criteria**: Note B has Note A in its `incoming` links

---

### LINK-003: Link Score Threshold

**MCP Tool**: `get_note_links`

```javascript
get_note_links({ id: "<note_id>" })
```

**Pass Criteria**: All links have `score >= 0.7` (default threshold)

---

## Graph Exploration

### LINK-004: Explore Graph - Depth 1

**MCP Tool**: `explore_graph`

```javascript
explore_graph({
  id: "<ml_note_id>",
  depth: 1,
  max_nodes: 10
})
```

**Expected Response**:
```json
{
  "nodes": [
    { "id": "...", "title": "Root Note", "depth": 0 },
    { "id": "...", "title": "Connected Note", "depth": 1 }
  ],
  "edges": [
    { "from": "...", "to": "...", "score": 0.82 }
  ]
}
```

**Pass Criteria**: Returns nodes and edges arrays

---

### LINK-005: Explore Graph - Depth 2

**MCP Tool**: `explore_graph`

```javascript
explore_graph({
  id: "<ml_note_id>",
  depth: 2,
  max_nodes: 20
})
```

**Pass Criteria**: Includes depth-2 connections (friends of friends)

---

### LINK-006: Graph Max Nodes Limit

**MCP Tool**: `explore_graph`

```javascript
explore_graph({
  id: "<note_id>",
  depth: 3,
  max_nodes: 5
})
```

**Pass Criteria**: Returns at most 5 nodes despite deeper exploration

---

## Knowledge Discovery

### LINK-007: Cross-Topic Links

**MCP Tool**: `get_note_links`

```javascript
// ML note should link to related programming concepts
const mlLinks = get_note_links({ id: "<backpropagation_note_id>" })
```

**Pass Criteria**: Links to other neural network notes exist

---

### LINK-008: No Self-Links

**MCP Tool**: `get_note_links`

```javascript
const links = get_note_links({ id: "<any_note_id>" })
```

**Pass Criteria**: Note does not appear in its own outgoing or incoming links

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| LINK-001 | Get Note Links | `get_note_links` | |
| LINK-002 | Verify Bidirectional | `get_note_links` | |
| LINK-003 | Link Score Threshold | `get_note_links` | |
| LINK-004 | Explore Graph Depth 1 | `explore_graph` | |
| LINK-005 | Explore Graph Depth 2 | `explore_graph` | |
| LINK-006 | Graph Max Nodes | `explore_graph` | |
| LINK-007 | Cross-Topic Links | `get_note_links` | |
| LINK-008 | No Self-Links | `get_note_links` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:

**Important**: Semantic links require embeddings to be generated. If links are empty, verify embeddings exist using `list_jobs({ note_id: "<id>" })` and wait for completion.

---

## Chunked Document Handling

### LINK-009: Get Full Document

**MCP Tool**: `get_full_document`

```javascript
// For a chunked note, get the full reconstructed document
get_full_document({ id: "<chunked_note_id>" })
```

**Expected Response**:
```json
{
  "id": "<uuid>",
  "title": "Full Document Title",
  "content": "Reconstructed full content...",
  "is_chunked": true,
  "chunks": [
    { "id": "...", "position": 0, "content": "..." },
    { "id": "...", "position": 1, "content": "..." }
  ],
  "total_chunks": 2,
  "tags": [...],
  "timestamps": {...}
}
```

**Pass Criteria**: Returns reconstructed document with all chunks

**Note**: If no chunked documents exist, create one by ingesting a long document (>4000 words)

---

### LINK-010: Get Chunk Chain

**MCP Tool**: `get_chunk_chain`

```javascript
get_chunk_chain({
  chain_id: "<chain_id>",
  include_content: true
})
```

**Pass Criteria**: Returns ordered chunks for the document chain

---

### LINK-011: Search With Deduplication

**MCP Tool**: `search_with_dedup`

```javascript
search_with_dedup({
  query: "specific topic",
  limit: 10,
  mode: "hybrid"
})
```

**Expected Response**:
```json
{
  "results": [
    {
      "note_id": "...",
      "title": "...",
      "chunk_id": "...",
      "is_deduplicated": true,
      "original_chunks": 3
    }
  ]
}
```

**Pass Criteria**: Results deduplicated to one entry per source document

---

## Updated Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| LINK-001 | Get Note Links | `get_note_links` | |
| LINK-002 | Verify Bidirectional | `get_note_links` | |
| LINK-003 | Link Score Threshold | `get_note_links` | |
| LINK-004 | Explore Graph Depth 1 | `explore_graph` | |
| LINK-005 | Explore Graph Depth 2 | `explore_graph` | |
| LINK-006 | Graph Max Nodes | `explore_graph` | |
| LINK-007 | Cross-Topic Links | `get_note_links` | |
| LINK-008 | No Self-Links | `get_note_links` | |
| LINK-009 | Get Full Document | `get_full_document` | |
| LINK-010 | Get Chunk Chain | `get_chunk_chain` | |
| LINK-011 | Search With Dedup | `search_with_dedup` | |

---

## Note Backlinks & Provenance

### LINK-012: Get Note Backlinks

**MCP Tool**: `get_note_backlinks`

```javascript
get_note_backlinks({ id: "<ml_note_id>" })
```

**Expected Response**:
```json
{
  "backlinks": [
    {
      "from_note_id": "...",
      "from_note_title": "...",
      "link_type": "semantic",
      "score": 0.82,
      "created_at": "<timestamp>"
    }
  ],
  "total": 5
}
```

**Pass Criteria**: Returns notes that link TO this note

**Note**: Backlinks are the inverse of outgoing links - they show what references this note

---

### LINK-013: Get Note Provenance

**MCP Tool**: `get_note_provenance`

```javascript
get_note_provenance({ id: "<ml_note_id>" })
```

**Expected Response**:
```json
{
  "note_id": "<uuid>",
  "created_at": "<timestamp>",
  "created_by": "api",
  "derivations": [
    {
      "event": "ai_revision",
      "timestamp": "<timestamp>",
      "source_notes": ["<uuid>", "<uuid>"],
      "model": "llama3.2"
    }
  ],
  "sources": [
    {
      "note_id": "<uuid>",
      "relationship": "used_for_context"
    }
  ]
}
```

**Pass Criteria**: Returns W3C PROV-style provenance information

**Note**: Provenance tracks how notes were created/derived and what sources influenced them

---

## Updated Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| LINK-001 | Get Note Links | `get_note_links` | |
| LINK-002 | Verify Bidirectional | `get_note_links` | |
| LINK-003 | Link Score Threshold | `get_note_links` | |
| LINK-004 | Explore Graph Depth 1 | `explore_graph` | |
| LINK-005 | Explore Graph Depth 2 | `explore_graph` | |
| LINK-006 | Graph Max Nodes | `explore_graph` | |
| LINK-007 | Cross-Topic Links | `get_note_links` | |
| LINK-008 | No Self-Links | `get_note_links` | |
| LINK-009 | Get Full Document | `get_full_document` | |
| LINK-010 | Get Chunk Chain | `get_chunk_chain` | |
| LINK-011 | Search With Dedup | `search_with_dedup` | |
| LINK-012 | Get Note Backlinks | `get_note_backlinks` | |
| LINK-013 | Get Note Provenance | `get_note_provenance` | |

**MCP Tools Covered**: `get_note_links`, `explore_graph`, `get_full_document`, `get_chunk_chain`, `search_with_dedup`, `get_note_backlinks`, `get_note_provenance`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
