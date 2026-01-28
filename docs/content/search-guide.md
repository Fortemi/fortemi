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
score(d) = 1/(20 + rank_fts) + 1/(20 + rank_semantic)
```

Documents appearing high in both rankings score best. The k=20 constant (optimized from the original k=60 based on Elasticsearch BEIR benchmark analysis) emphasizes top-ranked results while preventing any single ranking from dominating.

## Advanced Search Features

### Adaptive RRF

The RRF k parameter automatically adapts based on query characteristics:

- **Short queries (1-2 tokens):** k *= 0.7 (tighter fusion, more emphasis on top results)
- **Long queries (6+ tokens):** k *= 1.3 (looser fusion, considers more results)
- **Quoted queries:** k *= 0.6 (precision-focused, exact match emphasis)
- **Default:** k=20 for balanced queries

This adaptive approach improves relevance by tailoring the fusion algorithm to query type.

### Adaptive Weights

FTS and semantic weights automatically adjust based on query characteristics:

| Query Type | FTS Weight | Semantic Weight | When to Use |
|------------|------------|-----------------|-------------|
| **Quoted phrases** | 0.7 | 0.3 | "machine learning" - Exact phrase matching |
| **Keywords (1-2 tokens)** | 0.6 | 0.4 | rust, API - Short keyword queries |
| **Balanced (3-5 tokens)** | 0.5 | 0.5 | rust async programming - Medium queries |
| **Conceptual (6+ tokens)** | 0.35 | 0.65 | how do I implement semantic search - Natural language |

**Why this matters:** Keyword queries benefit from lexical precision, while conceptual queries benefit from semantic understanding. The system automatically chooses the best balance.

### Relative Score Fusion (RSF)

Alternative fusion algorithm that preserves score magnitude:

```
normalized_score = (score - min) / (max - min)
final_score = w_fts * norm_fts + w_sem * norm_sem
```

**Differences from RRF:**
- RRF uses only rank position (1st, 2nd, 3rd...)
- RSF preserves actual score values
- RSF better captures large score differences
- Weaviate reports +6% recall on FIQA benchmark vs RRF

**When to use RSF:** When score magnitudes matter (e.g., large quality gaps between results).

### Result Deduplication

When documents are chunked for embedding, multiple chunks from the same document may appear in results. The system automatically:

1. Groups chunks by document ID
2. Keeps the best-scoring chunk per document
3. Adds metadata showing how many chunks matched
4. Re-sorts results after deduplication

**Example response with chain info:**
```json
{
  "note_id": "uuid",
  "score": 0.85,
  "snippet": "...matching text...",
  "title": "Original Document Title",
  "chain_info": {
    "chain_id": "uuid",
    "original_title": "Original Document Title",
    "chunks_matched": 3,
    "best_chunk_sequence": 2,
    "total_chunks": 5
  }
}
```

This ensures clean results without duplicate entries for the same document.

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

### Query Length Strategy

- **1-2 words:** System favors FTS (60/40) - good for precise terms
- **3-5 words:** Balanced (50/50) - hybrid works well
- **6+ words:** System favors semantic (35/65) - captures intent

The system handles this automatically, but you can override by selecting a specific mode.

## Performance Characteristics

| Collection Size | Hybrid p95 | Notes |
|-----------------|------------|-------|
| 1,000 docs | <50ms | No HNSW needed |
| 10,000 docs | <200ms | HNSW kicks in |
| 100,000 docs | <500ms | O(log N) scaling |

The HNSW vector index provides logarithmic query complexity, so performance degrades slowly as the knowledge base grows.

### HNSW Tuning

The system dynamically adjusts the HNSW `ef_search` parameter based on:
- **Corpus size:** Larger collections get higher ef_search
- **Recall target:** Choose between Fast (85%), Balanced (92%), High (96%), or Exhaustive (99%)

Formula: `ef = base_ef * max(1.0, log2(corpus_size / 10000) * scale_factor)`

This balances recall and latency based on your collection size.

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
      "tags": ["tag1", "tag2"],
      "chain_info": {
        "chunks_matched": 2,
        "original_title": "Document Title"
      }
    }
  ],
  "total": 42,
  "mode": "hybrid"
}
```

## Advanced Topics

### Understanding Score Components

In hybrid mode with adaptive weights, the final score combines:

1. **FTS score** - BM25 relevance (term frequency, document length normalization)
2. **Semantic score** - Cosine similarity of embeddings (0-1 range)
3. **Fusion** - RRF or RSF combines the two rankings
4. **Adaptive weighting** - Query-dependent balance between FTS and semantic

### Choosing Between RRF and RSF

**Use RRF when:**
- You want proven, unsupervised fusion
- Rank position matters more than score magnitude
- You need consistent behavior across query types

**Use RSF when:**
- Score differences are meaningful (e.g., 0.9 vs 0.3)
- You want to preserve quality gaps between results
- You need slightly better recall (Weaviate FIQA: +6%)

### Chunked Document Handling

Large documents are automatically chunked for embedding. The search system:
1. Searches across all chunks
2. Finds the most relevant chunk per document
3. Returns deduplicated results with chunk metadata
4. Preserves the best snippet from the highest-scoring chunk

This ensures comprehensive coverage while maintaining clean results.

---

*See also: [Architecture](./architecture.md) | [Glossary](./glossary.md)*
