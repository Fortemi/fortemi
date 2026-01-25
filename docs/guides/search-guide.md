# Search Guide

This guide explains how to use matric-memory's hybrid search system effectively.

## Search Modes

matric-memory offers three search modes, each optimized for different use cases:

### 1. Hybrid Search (Default)

Combines lexical (BM25) and semantic (dense retrieval) results using Reciprocal Rank Fusion.

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=retrieval+augmented+generation"
```

**Best for:** Most queries. Finds both exact matches and semantically related content.

### 2. Lexical Search (FTS)

Pure keyword matching using BM25 ranking via PostgreSQL full-text search.

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=API+documentation&mode=fts"
```

**Best for:** Finding exact phrases, code snippets, or when you know the precise terminology.

### 3. Semantic Search

Pure embedding similarity using dense retrieval.

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=how+to+build+neural+networks&mode=semantic"
```

**Best for:** Conceptual queries, finding related content with different terminology.

## Understanding Results

### Score Interpretation

| Score Range | Meaning |
|-------------|---------|
| 0.8 - 1.0 | Highly relevant |
| 0.6 - 0.8 | Moderately relevant |
| 0.4 - 0.6 | Somewhat relevant |
| < 0.4 | Tangentially related |

### RRF Fusion

In hybrid mode, results are ranked using Reciprocal Rank Fusion:

```
score(d) = 1/(60 + rank_fts) + 1/(60 + rank_semantic)
```

Documents appearing high in both rankings score best. The k=60 constant prevents any single ranking from dominating.

## Filtering

### By Tags (SKOS Concepts)

Filter results to specific categories:

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=machine+learning&tags=research,ai"
```

### Strict vs. Soft Filtering

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Strict** | 100% isolation, applied before search | Multi-tenancy, access control |
| **Soft** | Combined with relevance scoring | Preference-based filtering |

### Date Ranges

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=meeting+notes&created_after=2024-01-01"
```

## Query Tips

### Natural Language Works

The semantic component understands natural language:
- "how do I connect to the database" works as well as "database connection"
- "problems with authentication" finds notes about "auth errors" or "login issues"

### Combining Approaches

Start with hybrid search. If results are too broad, switch to FTS for precision. If missing related content, try semantic mode.

### Phrase Matching

Use quotes for exact phrases in FTS mode:

```bash
curl "https://memory.integrolabs.net/api/v1/search?q=\"retrieval+augmented+generation\"&mode=fts"
```

## Performance Characteristics

| Collection Size | Hybrid p95 | Notes |
|-----------------|------------|-------|
| 1,000 docs | <50ms | No HNSW needed |
| 10,000 docs | <200ms | HNSW kicks in |
| 100,000 docs | <500ms | O(log N) scaling |

The HNSW vector index provides logarithmic query complexity, so performance degrades slowly as the knowledge base grows.

## API Reference

### Search Endpoint

```
GET /api/v1/search
```

**Parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `q` | string | required | Search query |
| `mode` | string | `hybrid` | `hybrid`, `fts`, or `semantic` |
| `limit` | integer | 20 | Max results (1-100) |
| `offset` | integer | 0 | Pagination offset |
| `tags` | string | - | Comma-separated tag filter |
| `created_after` | datetime | - | Filter by creation date |
| `created_before` | datetime | - | Filter by creation date |

**Response:**

```json
{
  "results": [
    {
      "note_id": "uuid",
      "score": 0.85,
      "snippet": "...matching text...",
      "title": "Note Title",
      "tags": ["tag1", "tag2"]
    }
  ],
  "total": 42,
  "mode": "hybrid"
}
```

---

*See also: [Architecture](../architecture.md) | [Glossary](../glossary.md)*
