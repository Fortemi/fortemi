# Search Guide

This guide explains how to use Fortémi's hybrid search system effectively.

## When to Use Which Mode

| Your Query | Recommended Mode | Why |
|------------|-----------------|-----|
| Known keywords, code snippets, exact phrases | **FTS** | Lexical matching finds precise terms |
| Conceptual questions, "how to..." | **Semantic** | Embedding similarity captures meaning |
| General search, don't know what to expect | **Hybrid** (default) | Best of both worlds via RRF fusion |
| CJK characters, emoji | **Hybrid** with language hint | Combines bigram/trigram with semantic |
| Large result set, need precision | **FTS** with strict filters | Fastest, most precise for known terms |

**Quick rule:** If unsure, use hybrid (the default). Switch to FTS for precision or semantic for discovery.

## Search Modes

Fortémi offers three search modes, each optimized for different use cases:

### 1. Hybrid Search (Default)

Combines lexical (BM25) and semantic (dense retrieval) results using Reciprocal Rank Fusion.

```bash
curl "http://localhost:3000/api/v1/search?q=retrieval+augmented+generation"
```

**Best for:** Most queries. Finds both exact matches and semantically related content.

### 2. Lexical Search (FTS)

Pure keyword matching using BM25 ranking via PostgreSQL full-text search.

```bash
curl "http://localhost:3000/api/v1/search?q=API+documentation&mode=fts"
```

**Best for:** Finding exact phrases, code snippets, or when you know the precise terminology.

### 3. Semantic Search

Pure embedding similarity using dense retrieval.

```bash
curl "http://localhost:3000/api/v1/search?q=how+to+build+neural+networks&mode=semantic"
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
curl "http://localhost:3000/api/v1/search?q=machine+learning&tags=research,ai"
```

### Strict vs. Soft Filtering

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Strict** | 100% isolation, applied before search | Multi-tenancy, access control |
| **Soft** | Combined with relevance scoring | Preference-based filtering |

### Date Ranges

```bash
curl "http://localhost:3000/api/v1/search?q=meeting+notes&created_after=2024-01-01"
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
curl "http://localhost:3000/api/v1/search?q=\"retrieval+augmented+generation\"&mode=fts"
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

For complete search API documentation including all parameters, request/response schemas, and examples, see:

- **Interactive docs**: [Swagger UI](/docs)
- **OpenAPI spec**: [openapi.yaml](/openapi.yaml)
- **Configuration**: [Configuration Reference](./configuration.md)

## Multilingual Search

Fortémi supports full-text search across multiple languages and scripts.

### Supported Languages

| Language/Script | Support Level | Configuration |
|-----------------|---------------|---------------|
| **English** | Full stemming | `matric_english` (default) |
| **German** | Full stemming | `matric_german` |
| **French** | Full stemming | `matric_french` |
| **Spanish** | Full stemming | `matric_spanish` |
| **Portuguese** | Full stemming | `matric_portuguese` |
| **Russian** | Full stemming | `matric_russian` |
| **Chinese, Japanese, Korean** | Bigram/Trigram | `matric_simple` + pg_bigm |
| **Emoji & Symbols** | Trigram matching | pg_trgm |
| **Other scripts** | Basic tokenization | `matric_simple` |

### Query Syntax

The search system supports boolean operators via `websearch_to_tsquery`:

| Syntax | Example | Description |
|--------|---------|-------------|
| Simple | `hello world` | Match all words (AND) |
| OR | `apple OR orange` | Match either word |
| NOT | `apple -orange` | Exclude word |
| Phrase | `"hello world"` | Match exact phrase |
| Combined | `"machine learning" OR AI` | Phrase OR single word |

```bash
# OR operator
curl "http://localhost:3000/api/v1/search?q=apple+OR+orange"

# NOT operator
curl "http://localhost:3000/api/v1/search?q=python+-snake"

# Phrase search
curl "http://localhost:3000/api/v1/search?q=%22machine+learning%22"
```

### Language Hints

Specify language for better stemming results:

```bash
# German search
curl "http://localhost:3000/api/v1/search?q=Haus&lang=de"

# Chinese search
curl "http://localhost:3000/api/v1/search?q=人工智能&lang=zh"

# Japanese search
curl "http://localhost:3000/api/v1/search?q=プログラミング&lang=ja"
```

### Script Detection

The system automatically detects query script and routes to the appropriate search strategy:

| Detected Script | Search Strategy |
|-----------------|-----------------|
| Latin | FTS with matric_english |
| CJK (Han, Hiragana, Katakana, Hangul) | Bigram (pg_bigm) or Trigram fallback |
| Cyrillic | FTS with matric_russian |
| Arabic, Hebrew, Greek | FTS with matric_simple |
| Emoji | Trigram matching (pg_trgm) |
| Mixed scripts | Multi-strategy search |

### Emoji Search

Emoji search uses pg_trgm trigram matching with ILIKE substring fallback.

**Supported emoji patterns:**

| Pattern | Example | Result |
|---------|---------|--------|
| Single emoji | 🚀 🔥 ⭐ | ✅ Found |
| Repeated same | 🔥🔥 | ✅ Found |
| Adjacent different | 🚀🎉 | ✅ Found |
| Emoji + text | meeting 📝 | ✅ Found |
| Emoji with variation selector | ❤️ | ✅ Found |

**Supported Unicode ranges:**
- Emoticons (😀-🙏)
- Misc Symbols and Pictographs (🌀-🗿)
- Transport and Map (🚀-🛿)
- Misc Symbols (☀️, ⚡, ☔)
- Dingbats (✅, ✨, ✔️)
- Misc Symbols and Arrows (⭐, ⬆️, ⬇️)

```bash
# Single emoji
curl "http://localhost:3000/api/v1/search?q=🎉"

# Adjacent emojis
curl "http://localhost:3000/api/v1/search?q=🚀🎉"

# Emoji with text
curl "http://localhost:3000/api/v1/search?q=meeting+📝"
```

**How it works:** When the query contains emoji, the system uses two strategies:
1. `similarity()` function for fuzzy trigram matching
2. `ILIKE '%emoji%'` for exact substring matching (fallback)

The ILIKE fallback ensures emoji sequences are found even when trigram similarity is low.

### CJK Search Requirements

**Minimum 2 characters required** for CJK (Chinese, Japanese, Korean) queries.

| Query Length | Result | Why |
|--------------|--------|-----|
| 1 character (中) | 0 results | Below n-gram minimum |
| 2+ characters (中文) | ✅ Found | Meets bigram/trigram threshold |

This is an industry-standard limitation shared by all major search engines:
- PostgreSQL pg_trgm requires 3 characters (trigrams)
- PostgreSQL pg_bigm requires 2 characters (bigrams)
- Elasticsearch CJK analyzers recommend 2+ characters
- Google, Baidu, Naver all require 2+ characters for meaningful results

**Why single characters don't work:** N-gram indexes create searchable tokens from character sequences. A single CJK character doesn't generate enough tokens for reliable matching against document content.

```bash
# Chinese: 2+ characters required
curl "http://localhost:3000/api/v1/search?q=中文"      # ✅ Works
curl "http://localhost:3000/api/v1/search?q=人工智能"  # ✅ Works

# Japanese with hiragana
curl "http://localhost:3000/api/v1/search?q=日本語"    # ✅ Works

# Korean
curl "http://localhost:3000/api/v1/search?q=한국어"    # ✅ Works
```

### Feature Flags

Multilingual features can be enabled via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `FTS_WEBSEARCH_TO_TSQUERY` | true | OR/NOT/phrase operators |
| `FTS_SCRIPT_DETECTION` | false | Automatic script routing |
| `FTS_TRIGRAM_FALLBACK` | false | Emoji/symbol search |
| `FTS_BIGRAM_CJK` | false | Optimized CJK search |
| `FTS_MULTILINGUAL_CONFIGS` | false | Language-specific stemming |

Enable all multilingual features:
```bash
export FTS_SCRIPT_DETECTION=true
export FTS_TRIGRAM_FALLBACK=true
export FTS_BIGRAM_CJK=true
export FTS_MULTILINGUAL_CONFIGS=true
```

## How Search Indexing Works

Understanding the underlying technology helps set appropriate expectations for search behavior.

### PostgreSQL Extensions

Fortémi uses three PostgreSQL extensions for full-text search:

| Extension | Purpose | Minimum Query Length |
|-----------|---------|---------------------|
| **tsvector/tsquery** | Standard FTS with stemming | 1+ characters (Latin scripts) |
| **pg_trgm** | Trigram similarity matching | 3 characters |
| **pg_bigm** | Bigram matching (CJK-optimized) | 2 characters |

### N-gram Tokenization Explained

N-gram indexes work by breaking text into overlapping character sequences:

**Trigrams (3-character sequences):**
```
"hello" → {"  h", " he", "hel", "ell", "llo", "lo ", "o  "}
```

**Bigrams (2-character sequences):**
```
"日本語" → {"日本", "本語"}
```

The search query is also tokenized, and matching occurs when enough n-grams overlap between query and document. This is why minimum character requirements exist—short queries don't generate enough tokens for reliable matching.

### Script-Specific Search Strategies

The system automatically selects the optimal strategy based on detected script:

```
┌─────────────────┐     ┌──────────────────────────────────────┐
│   User Query    │────▶│         Script Detection             │
└─────────────────┘     └──────────────────────────────────────┘
                                        │
        ┌───────────────┬───────────────┼───────────────┬───────────────┐
        ▼               ▼               ▼               ▼               ▼
   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
   │  Latin  │    │   CJK   │    │ Cyrillic│    │  Emoji  │    │  Mixed  │
   └────┬────┘    └────┬────┘    └────┬────┘    └────┬────┘    └────┬────┘
        │              │              │              │              │
        ▼              ▼              ▼              ▼              ▼
   FTS English    pg_bigm or     FTS Russian    pg_trgm +      pg_trgm
   (stemming)     pg_trgm        (stemming)      ILIKE         fallback
```

### Index Types and Their Characteristics

| Index Type | Best For | Limitations |
|------------|----------|-------------|
| **GIN tsvector** | Word-based FTS | No substring matching |
| **GIN pg_trgm** | Similarity search, LIKE/ILIKE | 3-char minimum |
| **GIN pg_bigm** | CJK, short strings | 2-char minimum |
| **HNSW (pgvector)** | Semantic similarity | Requires embeddings |

### Default Configuration

The Docker bundle enables these features by default:

```yaml
# docker-compose.bundle.yml
environment:
  - FTS_SCRIPT_DETECTION=true
  - FTS_TRIGRAM_FALLBACK=true
  - FTS_BIGRAM_CJK=true
  - FTS_MULTILINGUAL_CONFIGS=true
```

### Performance Implications

| Query Type | Index Used | Complexity | Typical Latency |
|------------|------------|------------|-----------------|
| English keywords | GIN tsvector | O(log n) | <10ms |
| CJK 2+ chars | GIN bigm/trgm | O(log n) | <20ms |
| Emoji | GIN trgm + ILIKE | O(n) for ILIKE | <50ms |
| Semantic | HNSW | O(log n) | <100ms |

The ILIKE fallback for emoji is slower (linear scan) but ensures correctness. For large collections with heavy emoji usage, consider pre-filtering with tags.

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

## Federated Search

Search across multiple memories simultaneously with unified result ranking.

### Search All Memories

```bash
curl -X POST http://localhost:3000/api/v1/search/federated \
  -H "Content-Type: application/json" \
  -d '{
    "query": "machine learning",
    "memories": ["all"]
  }'
```

### Search Specific Memories

```bash
curl -X POST http://localhost:3000/api/v1/search/federated \
  -H "Content-Type: application/json" \
  -d '{
    "query": "project documentation",
    "memories": ["default", "work-notes", "research"]
  }'
```

### Federated Search Response

```json
{
  "results": [
    {
      "note_id": "550e8400-...",
      "memory": "work-notes",
      "score": 0.92,
      "title": "Project Documentation",
      "snippet": "...machine learning algorithms...",
      "tags": ["project", "ml"]
    },
    {
      "note_id": "660e8400-...",
      "memory": "research",
      "score": 0.85,
      "title": "ML Research Papers",
      "snippet": "...deep learning techniques...",
      "tags": ["research", "ml"]
    }
  ],
  "total": 2,
  "memories_searched": ["work-notes", "research"]
}
```

### How It Works

1. **Parallel Execution**: Searches run concurrently across all specified memories
2. **Score Normalization**: Each memory's scores are normalized to [0,1] range
3. **Unified Ranking**: Results are merged and re-sorted by normalized score
4. **Memory Attribution**: Each result includes `memory` field showing source

### Performance Considerations

- Federated search latency = slowest memory search time
- Use specific memory names instead of `["all"]` when possible
- Consider memory size when searching many memories (large memories slow down federation)

### Use Cases

- **Cross-project search**: Find related work across all project memories
- **Multi-client search**: Search across client memories for patterns
- **Comprehensive research**: Discover connections across research and work notes

See the [Multi-Memory Guide](./multi-memory.md) for comprehensive documentation.

## Troubleshooting Poor Results

### No Results Returned

| Possible Cause | Diagnosis | Fix |
|----------------|-----------|-----|
| No embeddings generated | Check `/api/v1/jobs` for pending embed jobs | Wait for jobs or trigger via `/api/v1/jobs` |
| Wrong search mode | FTS won't find semantic matches | Try `mode=hybrid` or `mode=semantic` |
| Strict filter too narrow | Tag filter excludes all notes | Broaden filter or check tag spelling |
| Language mismatch | Non-English content with English stemmer | Add `lang` parameter or enable `FTS_SCRIPT_DETECTION` |
| **CJK query too short** | Single-character CJK query | Use 2+ characters (e.g., 中文 not 中) |
| **Features not enabled** | Script detection disabled | Enable `FTS_SCRIPT_DETECTION=true` |

### Irrelevant Results

| Possible Cause | Diagnosis | Fix |
|----------------|-----------|-----|
| Too many unrelated notes | Check if embedding set is too broad | Use tag-filtered embedding set |
| Short query, broad matches | 1-2 word queries match everything | Add more context words or use FTS mode |
| Stale embeddings | Notes updated but not re-embedded | Trigger re-embedding via job queue |

### Slow Search Performance

| Possible Cause | Diagnosis | Fix |
|----------------|-----------|-----|
| Missing HNSW index | Check `pg_indexes` for embedding index | Run migrations to create index |
| High ef_search | Query accuracy too high for your needs | Lower `hnsw.ef_search` (default: 64) |
| Large corpus without MRL | Full-dimension search on 100K+ docs | Use MRL truncation (256-dim) |

See [Troubleshooting Guide](./troubleshooting.md) for comprehensive diagnostics.

---

*See also: [Architecture](./architecture.md) | [Best Practices](./best-practices.md) | [Configuration](./configuration.md) | [Multi-Memory Guide](./multi-memory.md) | [Glossary](./glossary.md)*
