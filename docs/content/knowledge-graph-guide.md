# Knowledge Graph Guide

This guide explains how Fortémi automatically constructs and traverses knowledge graphs.

## How It Works

### Automatic Link Creation

When you create a note, Fortémi's NLP pipeline automatically constructs the knowledge graph across three phases:

**Phase 1** (parallel):
1. **AI concept tagging** — Generates 8-15 SKOS concept tags across 6 dimensions (domain, topic, methodology, application, technique, content-type)

**Phase 2** (after tagging completes):
2. **Related concept inference** — Uses the LLM to identify cross-dimensional associative relationships (e.g., `technique/attention-mechanism` related to `domain/machine-learning`) and creates `skos:related` edges with confidence scores. Skips notes with fewer than 3 leaf concepts.
3. **Tag-enriched embeddings** — Converts content + SKOS concept labels and their hierarchical relationships (broader, narrower, related) into vectors, producing semantically richer embeddings than content alone

**Phase 3** (after embedding completes):
4. **Tag-boosted similarity** — Blends embedding cosine similarity with SKOS tag overlap using a configurable weight formula:
   ```
   final_score = (embedding_sim × (1 - tag_weight)) + (tag_overlap × tag_weight)
   ```
5. **HNSW diverse neighbor selection** — Uses Algorithm 4 (Malkov & Yashunin 2018) to select up to k neighbors that are closer to the source than to already-selected neighbors, preventing star topology on clustered data
6. **Creates reciprocal links** — Bidirectional links with metadata (strategy, k, rank, tag_weight)

This ordering is critical: concept relationships are inferred before embeddings are generated, so embeddings incorporate the full concept graph context. Tags, relationships, and embeddings all inform linking. The result is significantly higher-quality connections than pure embedding similarity alone.

### Link Types

| Type | Creation | Directionality |
|------|----------|----------------|
| **Semantic** | Automatic (embedding similarity + tag overlap) | Bidirectional |
| **Explicit** | Manual (user-defined or `[[wiki-style]]` links) | Directional |

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

| Score | Relationship |
|-------|--------------|
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

### Tag-Boosted Scoring

When SKOS tags are available (the default for all notes processed by the NLP pipeline), the linking system blends embedding similarity with tag overlap. This means two notes about "machine learning" that share SKOS concepts like `domain/ai/machine-learning` will score higher than their raw embedding similarity suggests.

The tag weight is configurable per linking strategy. A fallback ensures that even if the tag-boosted heuristic selects nothing, the single best embedding match is still linked to prevent note isolation.

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

## The Automation Pipeline

Understanding how the knowledge graph builds itself helps you get the most from it:

1. **Create a note** via API or MCP `capture_knowledge`
2. **Phase 1 NLP jobs run in parallel**: AI revision, title generation, concept tagging, metadata extraction, document type inference
3. **Concept tagging completes** — the note now has 8-15 hierarchical SKOS tags
4. **Related concept inference runs** — the LLM identifies associative relationships between the note's concepts and creates `skos:related` edges
5. **Tag-enriched embedding is queued** — embedding generation uses concept labels and their relationships (broader, narrower, related) for richer vectors
6. **Tag-boosted linking is queued** — new connections appear using both embeddings and tag overlap
7. **The knowledge graph grows** — new connections appear automatically

You don't need to trigger any of these steps manually. The entire pipeline runs in the background via the job queue.

To re-run the pipeline (e.g., after a model upgrade), use `bulk_reprocess_notes` via MCP or the REST API.

## Limitations

### Embedding Drift

As your knowledge base evolves, older embeddings may become less representative. Consider periodic re-embedding for large, long-lived collections using `bulk_reprocess_notes`.

### Cold Start

New knowledge bases have sparse graphs until sufficient content accumulates. Minimum ~10 notes for meaningful connections.

### Topic Isolation

Notes on completely different topics won't link semantically, even if you want to connect them. Use explicit `[[wiki-style]]` links in note content for cross-domain connections.

---

*See also: [Search Guide](./search-guide.md) | [Tags Guide](./tags.md) | [Research Background](./research-background.md)*
