# Knowledge Graph Guide

This guide explains how Fortémi automatically constructs and traverses knowledge graphs.

## How It Works

### Automatic Link Creation

When you create a note, Fortémi:

1. **Generates embeddings** - Converts text to 768-dimensional sentence embeddings
2. **Computes similarity** - Compares with all existing note embeddings via cosine similarity
3. **Creates links** - Automatically links notes above 70% similarity threshold
4. **Stores weights** - Saves similarity scores as edge weights

This happens in the background via the job queue—no manual linking required.

### Link Types

| Type | Creation | Directionality |
|------|----------|----------------|
| **Semantic** | Automatic (embedding similarity) | Bidirectional |
| **Explicit** | Manual (user-defined) | Directional |

## Exploring the Graph

### Get Note Links

Retrieve all links for a specific note:

```bash
curl "http://localhost:3000/api/v1/notes/{id}/links"
```

**Response:**

```json
{
  "outgoing": [
    {
      "to_note_id": "uuid",
      "score": 0.85,
      "kind": "semantic"
    }
  ],
  "incoming": [
    {
      "from_note_id": "uuid",
      "score": 0.78,
      "kind": "semantic"
    }
  ]
}
```

### Graph Exploration

Traverse the knowledge graph starting from any note:

```bash
curl "http://localhost:3000/api/v1/graph/{id}/explore?depth=2&max_nodes=50"
```

**Parameters:**

| Param | Default | Description |
|-------|---------|-------------|
| `depth` | 2 | Maximum hops to traverse |
| `max_nodes` | 50 | Limit total nodes returned |

**Response:**

```json
{
  "nodes": [
    {
      "id": "uuid",
      "title": "Note Title",
      "depth": 0
    },
    {
      "id": "uuid",
      "title": "Related Note",
      "depth": 1
    }
  ],
  "edges": [
    {
      "from": "uuid",
      "to": "uuid",
      "score": 0.85,
      "kind": "semantic"
    }
  ]
}
```

## Understanding Similarity Scores

### Threshold Interpretation

| Similarity | Relationship |
|------------|--------------|
| 90%+ | Nearly identical topics |
| 80-90% | Strongly related |
| 70-80% | Related (link threshold) |
| 60-70% | Tangentially related (no link) |
| <60% | Different topics |

### Why 70%?

The 70% threshold was empirically chosen to balance:
- **Precision** - Avoiding spurious connections
- **Recall** - Discovering meaningful relationships

Higher thresholds miss valid relationships; lower thresholds create noise.

## Use Cases

### 1. Context Discovery

Find related context when writing:

```bash
# Get notes related to what you're writing
curl "http://localhost:3000/api/v1/notes/{draft_id}/links"
```

### 2. Knowledge Clusters

Explore topic clusters via graph traversal:

```bash
# Find all notes within 2 hops of a seed note
curl "http://localhost:3000/api/v1/graph/{seed_id}/explore?depth=2"
```

### 3. Gap Analysis

Notes with few links may indicate:
- Novel topics (good)
- Poorly integrated knowledge (needs attention)

### 4. Navigation

Build breadcrumb trails through related content for users exploring the knowledge base.

## Backlinks

Every link is bidirectional. The "incoming" links show what notes reference the current note—useful for understanding how concepts connect.

```json
{
  "incoming": [
    {
      "from_note_id": "uuid",
      "from_note_title": "Machine Learning Basics",
      "score": 0.82
    }
  ]
}
```

## Performance

### Link Generation

- Embedding generation: ~500ms per note (GPU) / ~2s (CPU)
- Similarity computation: O(N) against existing notes
- Link creation: Batched, async via job queue

### Graph Traversal

- Single-hop: <10ms
- Multi-hop (depth=3): <50ms
- Uses recursive CTE for efficiency

## Limitations

### Embedding Drift

As your knowledge base evolves, older embeddings may become less representative. Consider periodic re-embedding for large, long-lived collections.

### Cold Start

New knowledge bases have sparse graphs until sufficient content accumulates. Minimum ~10 notes for meaningful connections.

### Topic Isolation

Notes on completely different topics won't link, even if you want to connect them. Use explicit links for cross-domain connections.

---

*See also: [Search Guide](./search-guide.md) | [Research Background](./research-background.md)*
