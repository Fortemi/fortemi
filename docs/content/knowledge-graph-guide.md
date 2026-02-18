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

### Graph Exploration (v1 Contract)

Traverse the knowledge graph starting from any note. Returns a versioned payload with nodes, edges, and truncation/guardrail metadata.

```bash
curl "http://localhost:3000/api/v1/graph/{id}?depth=2&max_nodes=50"
```

**Parameters:**

| Param | Default | Range | Description |
|-------|---------|-------|-------------|
| `depth` | 2 | 0-10 | Maximum hops to traverse |
| `max_nodes` | 50 | 1-1000 | Limit total nodes returned |
| `min_score` | 0.0 | 0.0-1.0 | Minimum edge score threshold |
| `max_edges_per_node` | unlimited | 1-1000 | Cap edges per hub node |

**Response (v1):**

```json
{
  "graph_version": "v1",
  "nodes": [
    {
      "id": "uuid",
      "title": "Note Title",
      "depth": 0,
      "collection_id": "uuid-or-null",
      "archived": false,
      "created_at_utc": "2026-01-15T10:00:00Z",
      "updated_at_utc": "2026-02-01T14:30:00Z"
    },
    {
      "id": "uuid",
      "title": "Related Note",
      "depth": 1,
      "collection_id": null,
      "archived": false,
      "created_at_utc": "2026-01-20T08:00:00Z",
      "updated_at_utc": "2026-01-20T08:00:00Z"
    }
  ],
  "edges": [
    {
      "source": "uuid",
      "target": "uuid",
      "edge_type": "semantic",
      "score": 0.85,
      "rank": 1,
      "computed_at": "2026-01-20T08:01:00Z"
    },
    {
      "source": "uuid",
      "target": "uuid",
      "edge_type": "explicit",
      "score": 1.0,
      "rank": 1,
      "computed_at": "2026-02-01T14:30:00Z"
    }
  ],
  "meta": {
    "total_nodes": 127,
    "total_edges": 342,
    "truncated_nodes": 77,
    "truncated_edges": 0,
    "effective_depth": 2,
    "effective_max_nodes": 50,
    "effective_min_score": 0.0,
    "truncation_reasons": [
      "max_nodes limit: 50 of 127 nodes returned"
    ]
  }
}
```

**Node fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Note identifier |
| `title` | string/null | Note title |
| `depth` | integer | Hops from starting node (0 = start) |
| `collection_id` | UUID/null | Parent collection |
| `archived` | boolean | Archive status |
| `created_at_utc` | datetime | Creation timestamp |
| `updated_at_utc` | datetime | Last update timestamp |
| `community_id` | integer/null | Community cluster ID (when available) |
| `community_label` | string/null | Community label (when available) |
| `community_confidence` | float/null | Community assignment confidence |

**Edge fields:**

| Field | Type | Description |
|-------|------|-------------|
| `source` | UUID | Origin note ID |
| `target` | UUID | Destination note ID |
| `edge_type` | string | `"semantic"` or `"explicit"` |
| `score` | float | Similarity score (0.0-1.0) |
| `rank` | integer/null | Rank among edges from source node |
| `embedding_set` | string/null | Embedding set used (provenance) |
| `model` | string/null | Model used for embedding (provenance) |
| `computed_at` | datetime/null | When the link was computed |

**Meta fields (guardrails):**

| Field | Type | Description |
|-------|------|-------------|
| `total_nodes` | integer | Total reachable nodes before truncation |
| `total_edges` | integer | Total qualifying edges before truncation |
| `truncated_nodes` | integer | Nodes omitted due to limits |
| `truncated_edges` | integer | Edges omitted due to limits |
| `effective_depth` | integer | Depth actually applied (after server clamping) |
| `effective_max_nodes` | integer | Max nodes actually applied |
| `effective_min_score` | float | Score threshold actually applied |
| `effective_max_edges_per_node` | integer/null | Per-node edge limit (null = unlimited) |
| `truncation_reasons` | string[] | Human-readable truncation explanations |

**Legacy compatibility:** The v1 contract replaces the previous unversioned response. Clients should check for `graph_version` to detect the payload format. Edge fields changed from `from_id`/`to_id`/`kind` to `source`/`target`/`edge_type`.

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
5. **Tag-enriched embedding is queued** — embedding generation uses concept labels (prefixed `clustering:`) and their relationships (broader, narrower, related) for richer vectors; high-frequency concepts are filtered via TF-IDF to prevent generic terms from dominating
6. **Tag-boosted linking is queued** — new connections appear using both embeddings and tag overlap
7. **The knowledge graph grows** — new connections appear automatically
8. **Graph maintenance runs periodically** — a `GraphMaintenance` job applies the full quality pipeline: normalization → SNN → PFNET sparsification → Louvain community detection → diagnostics snapshot

You don't need to trigger any of these steps manually. The entire pipeline runs in the background via the job queue.

To re-run the note pipeline (e.g., after a model upgrade), use `bulk_reprocess_notes` via MCP or the REST API. To trigger graph maintenance immediately, use `POST /api/v1/graph/maintenance` or the `trigger_graph_maintenance` MCP tool.

## Graph Quality Pipeline

The graph maintenance job applies a four-step quality pipeline to the entire knowledge graph.

### Step 1: Normalization

Raw similarity scores are normalized using a configurable gamma parameter. This corrects for embedding model bias and produces a more uniform score distribution.

```
normalized_score = score ^ GRAPH_NORMALIZATION_GAMMA
```

**Environment variable:** `GRAPH_NORMALIZATION_GAMMA` (default: `1.0`)

### Step 2: Shared Nearest Neighbors (SNN)

Normalizes per-note edge scores by comparing each note's neighborhood to its neighbors' neighborhoods. Notes that share many neighbors get stronger links; isolated connections are weakened.

- **k parameter** (`GRAPH_SNN_K`): Number of nearest neighbors considered per node
- **Prune threshold** (`GRAPH_SNN_PRUNE_THRESHOLD`): Minimum SNN score to retain an edge

SNN is effective at breaking the "seashell pattern" — a topology defect where one highly-connected hub pulls many notes into artificial proximity, producing a star-shaped cluster rather than a meaningful topic cluster.

### Step 3: PFNET Sparsification

Pathfinder Network (PFNET) pruning removes edges that are redundant given transitive paths. An edge (A→C) is removed if a path A→B→C exists where both segments are stronger than the direct connection.

**Environment variable:** `GRAPH_PFNET_Q` — controls the pathfinder metric space. Higher values preserve more edges.

PFNET produces a sparser, more interpretable graph where each retained edge represents a genuinely direct relationship.

### Step 4: Louvain Community Detection

Applies the Louvain algorithm to identify topic communities in the sparsified graph. Each note is assigned a `community_id` and `community_label`.

**Environment variable:** `GRAPH_COMMUNITY_RESOLUTION` — controls community granularity (higher = more, smaller communities)

For large knowledge bases, coarse community detection uses MRL 64-dimensional embeddings for efficiency via `POST /api/v1/graph/community/coarse`.

Community assignments appear on graph nodes in `GET /api/v1/graph/{id}` responses:

```json
{
  "id": "uuid",
  "title": "Note Title",
  "community_id": 3,
  "community_label": "machine-learning",
  "community_confidence": 0.87
}
```

### Diagnostics

After each maintenance run, a diagnostics snapshot is captured automatically. Snapshots record graph health metrics (node count, edge count, average degree, community count, isolated node count) and enable trend tracking over time.

## Graph API Endpoints

### Trigger Graph Maintenance

```bash
POST /api/v1/graph/maintenance
```

Queues a `GraphMaintenance` job that runs the full quality pipeline (normalize → SNN → PFNET → Louvain → diagnostics). Returns the job ID for status tracking.

### Recompute SNN Scores

```bash
POST /api/v1/graph/snn/recompute
```

Recomputes Shared Nearest Neighbor scores without running the full pipeline. Useful after bulk note imports.

### Run PFNET Sparsification

```bash
POST /api/v1/graph/pfnet/sparsify
```

Applies PFNET pruning to the current graph state.

### Coarse Community Detection

```bash
POST /api/v1/graph/community/coarse
```

Runs Louvain community detection using MRL 64-dimensional embeddings. Efficient for large knowledge bases.

### Diagnostics Endpoints

```bash
# Capture a diagnostics snapshot
POST /api/v1/graph/diagnostics/snapshot

# List all snapshots
GET /api/v1/graph/diagnostics/history

# Compare two snapshots
GET /api/v1/graph/diagnostics/compare?from={snapshot_id}&to={snapshot_id}
```

The compare endpoint highlights changes in graph health between two points in time, useful for validating that maintenance improved graph quality.

## Graph MCP Tools

Two MCP tools support graph maintenance workflows:

| Tool | Description |
|------|-------------|
| `trigger_graph_maintenance` | Queue the full 4-step maintenance pipeline |
| `coarse_community_detection` | Run community detection using MRL 64-dim embeddings |

These are part of the `manage_graphs` discriminated-union tool group. The MCP server now exposes 37 total core tools (was 35).

## Graph Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `GRAPH_NORMALIZATION_GAMMA` | `1.0` | Score normalization exponent |
| `GRAPH_SNN_K` | (system default) | Nearest neighbors for SNN computation |
| `GRAPH_SNN_PRUNE_THRESHOLD` | (system default) | Minimum SNN score to retain an edge |
| `GRAPH_PFNET_Q` | (system default) | PFNET pathfinder metric space parameter |
| `GRAPH_COMMUNITY_RESOLUTION` | (system default) | Louvain community granularity |
| `GRAPH_STRUCTURAL_SCORE` | (system default) | Weight for structural vs. similarity scores in linking |
| `EMBED_CONCEPT_MAX_DOC_FREQ` | (system default) | TF-IDF document frequency cutoff for concept filtering in embeddings |

## Limitations

### Embedding Drift

As your knowledge base evolves, older embeddings may become less representative. Consider periodic re-embedding for large, long-lived collections using `bulk_reprocess_notes`.

### Cold Start

New knowledge bases have sparse graphs until sufficient content accumulates. Minimum ~10 notes for meaningful connections.

### Topic Isolation

Notes on completely different topics won't link semantically, even if you want to connect them. Use explicit `[[wiki-style]]` links in note content for cross-domain connections.

---

*See also: [Search Guide](./search-guide.md) | [Tags Guide](./tags.md) | [Research Background](./research-background.md)*
