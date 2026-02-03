# Full-Text Search Operators and Query Syntax

This document explains the query operators and advanced syntax supported by Fortémi's full-text search system.

## Overview

Fortémi uses PostgreSQL's `websearch_to_tsquery` function to provide a user-friendly query syntax with boolean operators, phrase matching, and exclusion capabilities. This allows powerful search queries without requiring complex syntax.

## Supported Operators

### AND (Implicit)

Multiple words are automatically combined with AND logic. All words must appear in matching documents.

**Syntax:** Space-separated words (default behavior)

```bash
# Find documents containing both "machine" and "learning"
curl "http://localhost:3000/api/v1/search?q=machine+learning"
```

**SQL equivalent:**
```sql
to_tsvector('matric_english', content) @@
  websearch_to_tsquery('matric_english', 'machine learning')
```

**Examples:**
- `rust async` - Documents with both "rust" AND "async"
- `neural network training` - All three words required
- `API documentation guide` - All three words must appear

### OR

Match documents containing either term (or both). Useful for synonyms or related concepts.

**Syntax:** `term1 OR term2` (uppercase OR required)

```bash
# Find documents with either "Python" or "Ruby"
curl "http://localhost:3000/api/v1/search?q=Python+OR+Ruby"

# Multiple OR terms
curl "http://localhost:3000/api/v1/search?q=API+OR+endpoint+OR+route"
```

**Examples:**
- `apple OR orange` - Documents with either fruit
- `database OR db OR datastore` - Synonym matching
- `authentication OR authorization` - Related concepts

**Combining with AND:**
```bash
# (rust AND async) OR (go AND concurrency)
curl "http://localhost:3000/api/v1/search?q=rust+async+OR+go+concurrency"
```

### NOT (Exclusion)

Exclude documents containing specific terms. Use minus sign (-) before the term to exclude.

**Syntax:** `-term` (minus sign directly attached to word)

```bash
# Find Python content, but exclude Django-specific docs
curl "http://localhost:3000/api/v1/search?q=python+-django"

# Exclude multiple terms
curl "http://localhost:3000/api/v1/search?q=javascript+-node+-browser"
```

**Examples:**
- `rust -unsafe` - Rust content without unsafe code
- `apple -fruit` - "Apple" (company) not the fruit
- `java -javascript` - Java language, not JavaScript

**Caution:** Exclusion operators may reduce recall if overused. Documents that mention the excluded term even briefly will be filtered out.

### Phrase Matching

Match exact word sequences using double quotes.

**Syntax:** `"exact phrase"` (double quotes required)

```bash
# Find exact phrase "machine learning"
curl "http://localhost:3000/api/v1/search?q=%22machine+learning%22"

# Combine phrases with operators
curl "http://localhost:3000/api/v1/search?q=%22retrieval+augmented%22+OR+%22semantic+search%22"
```

**Examples:**
- `"hello world"` - Exact phrase match
- `"neural network architecture"` - Multi-word exact match
- `"API v2.0"` - Phrase with version numbers

**When to use:**
- Technical terms (e.g., `"machine learning"` vs individual words)
- Code identifiers (e.g., `"getUserById"`)
- Version strings (e.g., `"PostgreSQL 16"`)
- Proper names (e.g., `"Claude Opus"`)

## Operator Precedence and Grouping

Operators are evaluated in this order:

1. **Phrase matching** (highest precedence)
2. **NOT (exclusion)**
3. **OR**
4. **AND (implicit)** (lowest precedence)

**Examples:**

```bash
# Phrase + OR + exclusion
curl "http://localhost:3000/api/v1/search?q=%22machine+learning%22+OR+AI+-deep+learning"
# Matches: ("machine learning" OR AI) AND NOT "deep" AND NOT "learning"

# Multiple phrases with OR
curl "http://localhost:3000/api/v1/search?q=%22semantic+search%22+OR+%22vector+similarity%22"
# Matches either phrase

# Complex query
curl "http://localhost:3000/api/v1/search?q=rust+async+OR+tokio+-blocking"
# Matches: (rust AND async) OR tokio, excluding "blocking"
```

## Adaptive Weighting Based on Query Type

The hybrid search system automatically adjusts FTS/semantic weights based on query characteristics:

| Query Type | FTS Weight | Semantic Weight | Detection |
|------------|------------|-----------------|-----------|
| Quoted phrase | 0.7 | 0.3 | Contains `"..."` |
| Short keywords | 0.6 | 0.4 | 1-2 tokens |
| Balanced | 0.5 | 0.5 | 3-5 tokens |
| Conceptual | 0.35 | 0.65 | 6+ tokens |

**Example behavior:**
- `"machine learning"` - 70% weight on exact phrase matching
- `rust` - 60% weight on keyword matching
- `rust async await` - 50/50 balanced
- `how do I implement semantic search with embeddings` - 65% semantic weight

This ensures quoted phrases get precise matching while natural language queries benefit from semantic understanding.

## What is NOT Supported

The following features are **not available** in Fortémi's FTS:

### Wildcards and Partial Matching

**Not supported:**
- `*` wildcard: `rust*` (won't match "rustc", "rustup")
- `?` single-char wildcard: `test?ng` (won't match "testing")
- Prefix matching: `embed*` (won't match "embedding", "embeddings")

**Alternative:** Use trigram search for substring matching (see Multilingual Features below).

### Proximity Operators

**Not supported:**
- NEAR: `machine NEAR/5 learning` (within 5 words)
- Distance operators: `term1 <-> term2`
- Ordered proximity: `"neural network" <2> "architecture"`

**Alternative:** Use phrase matching for exact sequences, or rely on semantic search for conceptual proximity.

### Field-Specific Search

**Not supported:**
- Field selectors: `title:rust` or `content:async`
- Scoped search: `author:smith`

**Why:** Fortémi uses weighted field search internally. Title matches automatically receive higher ranking (weight A=1.0 vs content weight C=0.2).

**Alternative:** Use strict tag filtering to narrow results by metadata.

### Regular Expressions

**Not supported:**
- Regex patterns: `/[A-Z]{3}/` or `\d{3}-\d{4}`
- LIKE patterns: `%test%`

**Alternative:** For pattern matching, extract data at ingestion time and use tag filtering.

### Fuzzy Matching

**Not supported:**
- Edit distance: `search~2` (Levenshtein distance)
- Fuzziness operators: `neural~`

**Alternative:** Semantic search handles conceptual similarity and typo tolerance through embeddings.

## BM25F Field-Weighted Scoring

Fortémi uses BM25F-style scoring with field weighting:

```sql
ts_rank(
  setweight(to_tsvector('matric_english', title), 'A') ||          -- Weight 1.0
  setweight(to_tsvector('matric_english', tags), 'B') ||           -- Weight 0.4
  setweight(tsv, 'C'),                                              -- Weight 0.2
  websearch_to_tsquery('matric_english', query),
  32  -- Normalization flag (divides by rank + 1)
)
```

**Field weights:**
- **Title (A):** 1.0 - Highest priority
- **Tags (B):** 0.4 - Medium priority
- **Content (C):** 0.2 - Base priority

Matches in the title score 5× higher than content matches, while tag matches score 2× higher. This ensures relevant results surface even with keyword queries.

## Multilingual Query Syntax

### Script Detection and Routing

When script detection is enabled (`FTS_SCRIPT_DETECTION=true`), queries are automatically routed to the appropriate search strategy:

| Script | Search Method | Example |
|--------|---------------|---------|
| Latin | FTS with stemming | `programming languages` |
| CJK (Han, Hiragana, Katakana, Hangul) | Bigram or trigram | `人工智能`, `プログラミング`, `안녕하세요` |
| Cyrillic | FTS (matric_russian) | `программирование` |
| Arabic, Hebrew, Greek | FTS (matric_simple) | `برمجة` |
| Emoji | Trigram matching | `🎉📝` |

**Examples:**

```bash
# Automatic CJK detection
curl "http://localhost:3000/api/v1/search?q=人工智能"

# Emoji search
curl "http://localhost:3000/api/v1/search?q=🎉"

# Mixed script query
curl "http://localhost:3000/api/v1/search?q=Rust+編程"
```

### Language Hints

Override automatic detection with explicit language hints:

```bash
# German stemming
curl "http://localhost:3000/api/v1/search?q=Häuser&lang=de"

# French stemming
curl "http://localhost:3000/api/v1/search?q=maisons&lang=fr"

# Spanish stemming
curl "http://localhost:3000/api/v1/search?q=programación&lang=es"
```

### Supported Operators by Language

All operators (OR, NOT, phrase) work across all languages:

```bash
# German with operators
curl "http://localhost:3000/api/v1/search?q=Rust+OR+Python&lang=de"

# Chinese phrase search
curl "http://localhost:3000/api/v1/search?q=%22机器学习%22&lang=zh"

# Japanese with exclusion
curl "http://localhost:3000/api/v1/search?q=プログラミング+-JavaScript&lang=ja"
```

## Performance Characteristics

### Query Complexity

| Query Type | Complexity | Notes |
|------------|------------|-------|
| Single word | O(log N) | GIN index lookup |
| AND (2-3 terms) | O(log N × k) | Intersect postings |
| OR (2-3 terms) | O(log N × k) | Union postings |
| Phrase (2-3 words) | O(log N × k) | Position verification |
| Complex (5+ operators) | O(log N × k²) | Multiple merges |

Where N = corpus size, k = average hits per term.

### Index Requirements

**GIN Index on tsvector:**
```sql
CREATE INDEX idx_note_revised_current_tsv_gin
  ON note_revised_current
  USING gin(tsv);
```

This enables fast full-text searches. Without the index, queries degrade to sequential scans (O(N)).

### Optimization Tips

**Avoid:**
- Many OR clauses (>10) - Degrades performance
- Very common words without exclusions - Returns too many results
- Overlapping phrases - Redundant matches

**Prefer:**
- Specific terms over general ones
- 2-4 word queries for best balance
- Hybrid mode for complex queries (combines FTS + semantic)

## Query Examples by Use Case

### Code Search

```bash
# Find Rust async functions
curl "http://localhost:3000/api/v1/search?q=rust+async+fn"

# API endpoints
curl "http://localhost:3000/api/v1/search?q=endpoint+OR+route+OR+handler"

# Error handling, exclude specific types
curl "http://localhost:3000/api/v1/search?q=error+handling+-panic"
```

### Documentation Search

```bash
# Installation guides
curl "http://localhost:3000/api/v1/search?q=%22installation+guide%22+OR+%22getting+started%22"

# Configuration examples
curl "http://localhost:3000/api/v1/search?q=config+OR+configuration+example"

# Tutorials excluding advanced topics
curl "http://localhost:3000/api/v1/search?q=tutorial+-advanced+-expert"
```

### Research Search

```bash
# Papers on specific topic
curl "http://localhost:3000/api/v1/search?q=%22neural+network%22+architecture"

# Authors or citations
curl "http://localhost:3000/api/v1/search?q=%22Vaswani+et+al%22+OR+%22Attention+is+All+You+Need%22"

# Related concepts
curl "http://localhost:3000/api/v1/search?q=transformer+OR+attention+OR+BERT"
```

### Meeting Notes

```bash
# Recent decisions
curl "http://localhost:3000/api/v1/search?q=decision+OR+agreed+OR+approved&created_after=2024-01-01"

# Action items
curl "http://localhost:3000/api/v1/search?q=%22action+item%22+OR+TODO+OR+follow-up"

# Specific project meetings
curl "http://localhost:3000/api/v1/search?q=project+alpha+meeting+-canceled"
```

## Debugging Search Queries

### Enable Search Metadata

Search responses include metadata showing how the query was processed:

```json
{
  "results": [...],
  "metadata": {
    "detected_script": "latin",
    "search_strategy": "fts_english",
    "fts_config": "matric_english",
    "search_time_ms": 45,
    "fts_results": 12,
    "semantic_results": 8,
    "fused_results": 15
  }
}
```

**Key fields:**
- `search_strategy` - Which search method was used
- `fts_config` - PostgreSQL text search configuration
- `detected_script` - Auto-detected script/language
- Result counts show fusion behavior

### Common Issues

**No results with quoted phrases:**
- Ensure exact spelling and word order
- Try removing quotes for fuzzy matching
- Check for stemming differences (searching `"running"` may not match `"run"`)

**Too many results:**
- Add exclusion terms (`-common -term`)
- Use phrase matching for precision (`"exact phrase"`)
- Switch to FTS-only mode (`mode=fts`)

**Missing expected results:**
- Check for typos in query
- Try OR operator for synonyms
- Use semantic search mode for conceptual matches
- Verify content isn't archived (archived notes excluded by default)

### Query Testing Workflow

1. **Start broad:** `rust async`
2. **Add exclusions:** `rust async -tokio`
3. **Use phrases:** `rust "async await"`
4. **Try operators:** `rust async OR futures`
5. **Check metadata:** Review search strategy and script detection
6. **Switch modes:** Try `mode=semantic` if FTS misses conceptual matches

## API Reference

### Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `q` | string | Search query (URL-encoded) |
| `mode` | string | `hybrid`, `fts`, or `semantic` (default: `hybrid`) |
| `lang` | string | ISO 639-1 language code (e.g., `en`, `de`, `zh`) |
| `script` | string | Script hint (e.g., `latin`, `han`, `cyrillic`) |
| `limit` | integer | Max results (1-100, default: 20) |
| `offset` | integer | Pagination offset (default: 0) |

### Example Requests

```bash
# Basic query
curl "http://localhost:3000/api/v1/search?q=machine+learning"

# With operators
curl "http://localhost:3000/api/v1/search?q=Python+OR+Ruby+-Django"

# FTS-only mode
curl "http://localhost:3000/api/v1/search?q=%22exact+phrase%22&mode=fts"

# With language hint
curl "http://localhost:3000/api/v1/search?q=Programmierung&lang=de"
```

### Response Format

```json
{
  "results": [
    {
      "note_id": "018d1234-5678-7abc-def0-123456789abc",
      "score": 0.85,
      "snippet": "...matching content snippet...",
      "title": "Document Title",
      "tags": ["machine-learning", "tutorial"],
      "chain_info": {
        "chain_id": "018d1234-...",
        "original_title": "Document Title",
        "chunks_matched": 2,
        "best_chunk_sequence": 1,
        "total_chunks": 5
      }
    }
  ],
  "total": 42,
  "mode": "hybrid",
  "metadata": {
    "detected_script": "latin",
    "search_strategy": "fts_english",
    "fts_config": "matric_english",
    "search_time_ms": 45
  }
}
```

## Related Documentation

- [Search Guide](./search-guide.md) - General search usage and tips
- [Chunking Workflow](./chunking-workflow.md) - Document chunking and deduplication
- [Embedding Pipeline](./embedding-pipeline.md) - Semantic search architecture
- [Tags Guide](./tags.md) - SKOS-based filtering
- [Glossary](./glossary.md) - Term definitions
