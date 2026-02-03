# Chunked Document Search and Deduplication Workflow

This guide explains how Fortémi handles large documents through intelligent chunking, embedding, and search deduplication to provide clean, relevant search results.

## Overview

Large documents cannot be embedded as a single unit due to:

- **Token limits** - Embedding models have maximum input lengths (256-8192 tokens)
- **Context dilution** - Mixing unrelated sections reduces embedding quality
- **Poor retrieval** - Relevant sections buried in large embeddings score lower

Fortémi solves this by:

1. **Chunking** documents into semantically meaningful sections
2. **Embedding** each chunk independently for precise matching
3. **Deduplicating** search results to show one entry per document
4. **Preserving** chunk metadata for context and navigation

## Document Ingestion Workflow

### Step 1: Document Type Detection

When a document is created or updated, the system detects its type:

```
User creates note → Detect document type → Select chunking strategy
                      |                      |
                      v                      v
                  Filename pattern       Semantic (markdown)
                  MIME type              Syntactic (code)
                  Magic bytes            Paragraph (prose)
                  Content analysis       Sentence (narrative)
```

**Example detection rules:**

| File Extension | Document Type | Chunking Strategy |
|----------------|---------------|-------------------|
| `.md`, `.markdown` | Markdown | Semantic |
| `.rs`, `.py`, `.js` | Code | Syntactic (Tree-sitter) |
| `.txt`, `.log` | Plain text | Paragraph |
| `.pdf`, `.docx` | Document | Paragraph |

See [Document Types Guide](./document-types-guide.md) for full registry.

### Step 2: Chunking Strategy Selection

The system selects a chunking strategy based on document type configuration:

```rust
pub enum ChunkingStrategy {
    Semantic,      // Markdown headings, code blocks, lists
    Syntactic,     // Code syntax via Tree-sitter
    Paragraph,     // Double newlines
    Sentence,      // Sentence boundaries
    SlidingWindow, // Fixed-size overlapping chunks
    Recursive,     // Hierarchical fallback
}
```

**Configuration example:**

```sql
SELECT name, chunking_strategy, chunk_size_default, chunk_overlap_default
FROM document_type
WHERE name = 'markdown';

--  name     | chunking_strategy | chunk_size_default | chunk_overlap_default
-- ----------|-------------------|--------------------|----------------------
--  markdown | semantic          | 1000               | 100
```

### Step 3: Chunk Generation

The chunker processes content into chunks with metadata:

```rust
pub struct Chunk {
    pub text: String,           // Chunk content
    pub start_offset: usize,    // Byte position in original
    pub end_offset: usize,      // Ending byte position
    pub metadata: HashMap<String, String>,  // Type, hierarchy, etc.
}
```

**Example: Semantic chunking of markdown**

Input document:
```markdown
# Introduction to Neural Networks

Neural networks are computing systems inspired by biological neural networks.

## Architecture

The basic building blocks are neurons arranged in layers:
- Input layer
- Hidden layers
- Output layer

## Training

Training involves forward propagation and backpropagation.
```

Output chunks:
```
Chunk 1 (offset 0-73):
  Type: heading
  Text: "# Introduction to Neural Networks\n\nNeural networks are computing systems..."

Chunk 2 (offset 74-180):
  Type: heading
  Text: "## Architecture\n\nThe basic building blocks are neurons arranged in layers:\n- Input layer\n- Hidden layers\n- Output layer"

Chunk 3 (offset 181-250):
  Type: heading
  Text: "## Training\n\nTraining involves forward propagation and backpropagation."
```

### Step 4: Chunk Storage and Linking

Chunks are stored with relationship metadata:

```sql
-- Simplified schema
CREATE TABLE note_chunk (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    chunk_sequence INTEGER,    -- Position in document (0, 1, 2...)
    chunk_text TEXT,
    start_offset INTEGER,
    end_offset INTEGER,
    chunk_metadata JSONB,
    embedding vector(768),     -- Chunk embedding
    embedding_set_id UUID,
    created_at TIMESTAMPTZ
);

-- Chain metadata
CREATE TABLE note_chunk_chain (
    chain_id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    original_title TEXT,
    total_chunks INTEGER,
    chunking_strategy TEXT,
    created_at TIMESTAMPTZ
);
```

**Chunk linking:**

Each chunk belongs to a "chain" (the original document), allowing:
- Reconstruction of full document from chunks
- Navigation between chunks (previous/next)
- Tracking which chunks matched a query

## Search Workflow with Chunks

### Step 1: Query Processing

User query is processed for both FTS and semantic search:

```
User query: "neural network training"
    |
    ├─> FTS query: websearch_to_tsquery('neural network training')
    |
    └─> Embedding: embed("query: neural network training")
```

### Step 2: Chunk-Level Search

Search operates at chunk granularity:

```sql
-- Simplified hybrid search query
SELECT
    nc.id as note_id,              -- Chunk ID (presented as note_id)
    nc.note_id as source_note_id,  -- Original document ID
    nc.chunk_sequence,
    similarity_score,
    nc.chunk_text as snippet,
    n.title
FROM note_chunk nc
JOIN note n ON nc.note_id = n.id
WHERE
    -- FTS on chunk text
    to_tsvector('matric_english', nc.chunk_text) @@
      websearch_to_tsquery('matric_english', $1)
    OR
    -- Semantic on chunk embedding
    nc.embedding <-> $2 < 0.5
ORDER BY combined_score DESC
LIMIT 100;  -- Over-fetch for deduplication
```

**Multiple chunks from same document may match:**

```
Results before deduplication:
  1. Chunk 3 from "Neural Networks Guide" (score: 0.92)
  2. Chunk 1 from "Neural Networks Guide" (score: 0.85)
  3. Chunk 2 from "Deep Learning Book" (score: 0.80)
  4. Chunk 5 from "Neural Networks Guide" (score: 0.75)
```

### Step 3: Deduplication

The deduplication algorithm consolidates chunks by document:

```rust
pub struct DeduplicationConfig {
    /// Deduplicate chunks from same document (keep best chunk only)
    pub deduplicate_chains: bool,  // default: true
    /// Include full chunk chain info in results
    pub expand_chains: bool,       // default: false
}
```

**Deduplication process:**

1. **Group** search results by `source_note_id` (original document)
2. **Select** highest-scoring chunk per document
3. **Collect** metadata about all matching chunks
4. **Re-sort** results by best chunk score

**Example:**

```rust
// Before deduplication: 4 results (2 from doc A, 2 from doc B)
[
    { note_id: "doc-A-chunk-3", score: 0.92 },
    { note_id: "doc-A-chunk-1", score: 0.85 },
    { note_id: "doc-B-chunk-2", score: 0.80 },
    { note_id: "doc-A-chunk-5", score: 0.75 },
]

// After deduplication: 2 results (1 per document)
[
    {
        note_id: "doc-A",  // Original document ID
        score: 0.92,       // Best chunk score
        snippet: "...text from chunk 3...",
        title: "Neural Networks Guide",
        chain_info: {
            chain_id: "doc-A",
            original_title: "Neural Networks Guide",
            chunks_matched: 3,        // Total chunks that matched
            best_chunk_sequence: 3,   // Which chunk scored best
            total_chunks: 8           // Total chunks in document
        }
    },
    {
        note_id: "doc-B",
        score: 0.80,
        snippet: "...text from chunk 2...",
        title: "Deep Learning Book",
        chain_info: {
            chain_id: "doc-B",
            chunks_matched: 1,
            best_chunk_sequence: 2,
            total_chunks: 15
        }
    }
]
```

### Step 4: Result Enhancement

Each deduplicated result includes rich metadata:

```typescript
interface EnhancedSearchHit {
    hit: SearchHit;              // Core search result
    chain_info?: ChainInfo;      // Chunk chain metadata
}

interface ChainInfo {
    chain_id: string;            // Original document ID
    original_title: string;      // Document title
    chunks_matched: number;      // How many chunks matched query
    best_chunk_sequence: number; // Which chunk scored highest
    total_chunks: number;        // Total chunks in document
}
```

**Uses:**

- **Navigation**: Click result to view document, highlight best chunk
- **Coverage**: See how many sections matched query
- **Context**: Understand chunk position in document (e.g., "Section 3 of 8")
- **Relevance**: Multiple matching chunks indicate strong document relevance

## Using Search with Chunks

### Basic Search (Deduplication Enabled)

Default behavior provides clean, deduplicated results:

```bash
curl "http://localhost:3000/api/v1/search?q=neural+network+training"
```

Response:
```json
{
  "results": [
    {
      "note_id": "018d1234-5678-7abc-def0-123456789abc",
      "score": 0.92,
      "snippet": "Training involves forward propagation and backpropagation...",
      "title": "Neural Networks Guide",
      "chain_info": {
        "chain_id": "018d1234-5678-7abc-def0-123456789abc",
        "chunks_matched": 3,
        "best_chunk_sequence": 5,
        "total_chunks": 8
      }
    }
  ]
}
```

### Viewing All Matching Chunks

Disable deduplication to see all chunks that matched:

```bash
curl "http://localhost:3000/api/v1/search?q=neural+network&deduplicate=false"
```

Response shows every matching chunk:
```json
{
  "results": [
    {
      "note_id": "doc-A-chunk-5",
      "score": 0.92,
      "snippet": "...from chunk 5...",
      "title": "Neural Networks Guide (Part 6/8)"
    },
    {
      "note_id": "doc-A-chunk-3",
      "score": 0.85,
      "snippet": "...from chunk 3...",
      "title": "Neural Networks Guide (Part 4/8)"
    },
    {
      "note_id": "doc-A-chunk-1",
      "score": 0.75,
      "snippet": "...from chunk 1...",
      "title": "Neural Networks Guide (Part 2/8)"
    }
  ]
}
```

Use case: Understanding which sections of a document are most relevant.

### Retrieving Full Chunk Chain

Get all chunks for a document to reconstruct the full text:

```bash
# API endpoint (example - check actual API)
curl "http://localhost:3000/api/v1/notes/{note_id}/chunks"
```

Response:
```json
{
  "chain_id": "018d1234-5678-7abc-def0-123456789abc",
  "original_title": "Neural Networks Guide",
  "total_chunks": 8,
  "chunks": [
    {
      "chunk_sequence": 0,
      "text": "# Introduction to Neural Networks...",
      "start_offset": 0,
      "end_offset": 150,
      "metadata": { "type": "heading" }
    },
    {
      "chunk_sequence": 1,
      "text": "## Architecture...",
      "start_offset": 151,
      "end_offset": 350,
      "metadata": { "type": "heading" }
    },
    // ... remaining chunks
  ]
}
```

### Getting Full Document from Chunk Result

When you receive a chunk in search results, retrieve the full document:

```bash
# Search returns chunk
curl "http://localhost:3000/api/v1/search?q=backpropagation"
# Response: note_id = "chunk-id", chain_info.chain_id = "document-id"

# Retrieve full document
curl "http://localhost:3000/api/v1/notes/{chain_id}"
```

The `chain_id` in `chain_info` is the original document ID.

## Optimizing Chunk Size

### Chunk Size Guidelines

| Content Type | Recommended Size | Overlap | Reason |
|--------------|------------------|---------|--------|
| **Code** | 500-1000 chars | 50-100 | Preserve function/class context |
| **Markdown docs** | 800-1500 chars | 100-200 | Keep section coherence |
| **Technical prose** | 1000-2000 chars | 150-300 | Balance detail and context |
| **Meeting notes** | 500-1000 chars | 50-100 | Topic-level granularity |
| **Research papers** | 1500-2500 chars | 200-400 | Paragraph to section level |

### Tuning for Embedding Models

Different models have different optimal chunk sizes:

| Model | Max Tokens | Optimal Chunk Size | Notes |
|-------|------------|-------------------|-------|
| `nomic-embed-text` | 8192 tokens | 1000-2000 chars | Long context capable |
| `e5-base-v2` | 512 tokens | 800-1200 chars | Standard context |
| `all-minilm` | 256 tokens | 400-800 chars | Short context only |
| `bge-large-en-v1.5` | 512 tokens | 800-1200 chars | Balanced |

**Rule of thumb:** 1 token ≈ 4 characters for English text.

### Overlap Strategy

Overlap preserves context across chunk boundaries:

```
Document: "The neural network architecture consists of three layers..."

Chunk 1 (size=100, overlap=20):
"The neural network architecture consists of three layers. The input layer receives data and..."

Chunk 2 (size=100, overlap=20, starts at offset 80):
"receives data and passes it to hidden layers. The hidden layers perform transformations..."
                  ^^^^^^^^^^^^^^^^^^^^^^^^ Overlap region
```

**Benefits:**
- Sentences split across chunks remain searchable
- Context preserved at boundaries
- Reduces risk of missing relevant passages

**Costs:**
- Increased storage (20-30% more chunks)
- Slightly slower embedding generation
- Potential duplicate results (mitigated by deduplication)

**Recommended overlap:**
- 10-15% of chunk size for most content
- 20-30% for code where context is critical
- 5-10% for structured content (markdown with clear sections)

## Chunk-Aware Search Patterns

### Pattern 1: Find Best Section

User wants the most relevant section of a document:

```bash
# Deduplication enabled (default)
curl "http://localhost:3000/api/v1/search?q=backpropagation+algorithm"
```

Result includes `best_chunk_sequence` showing which section matched.

### Pattern 2: Understand Document Coverage

User wants to see how comprehensively a document covers a topic:

```bash
# Check chunks_matched vs total_chunks ratio
curl "http://localhost:3000/api/v1/search?q=neural+networks"
```

```json
{
  "chain_info": {
    "chunks_matched": 6,
    "total_chunks": 8
  }
}
```

75% of sections match → Comprehensive coverage of topic.

### Pattern 3: Navigate Within Document

User wants to explore surrounding context:

1. **Search** returns chunk 5 as best match
2. **Retrieve** full chunk chain for document
3. **Display** chunks 4, 5, 6 with chunk 5 highlighted
4. **Allow** user to navigate previous/next chunks

### Pattern 4: Aggregate Multi-Chunk Evidence

For analytical queries, examine all matching chunks:

```bash
# Disable deduplication
curl "http://localhost:3000/api/v1/search?q=performance+metrics&deduplicate=false"
```

Aggregate results:
- Extract all mentioned metrics
- Compare definitions across chunks
- Identify contradictions or evolution of concepts

## Document Type-Specific Chunking

### Code Documents

**Strategy:** Syntactic chunking via Tree-sitter

```rust
// Example: Rust code chunked by functions
fn calculate_loss() {
    // Chunk 1: Full function
}

fn backpropagate() {
    // Chunk 2: Full function
}

impl NeuralNetwork {
    // Chunk 3: Impl block
    fn train() { ... }
    fn predict() { ... }
}
```

**Benefits:**
- Preserves syntactic units (functions, classes, methods)
- Maintains context within code blocks
- Better embedding quality for code semantics

**Configuration:**
```sql
SELECT name, chunking_strategy, tree_sitter_language
FROM document_type
WHERE category = 'code';

--  name | chunking_strategy | tree_sitter_language
-- ------|-------------------|---------------------
--  rust | syntactic         | rust
--  python | syntactic       | python
```

### Markdown Documents

**Strategy:** Semantic chunking by headings

```markdown
# Chapter 1: Introduction
Content for chapter 1...
## Section 1.1
Subsection content...

# Chapter 2: Methods
Content for chapter 2...
```

Chunks:
- Chunk 0: "# Chapter 1: Introduction\nContent for chapter 1..."
- Chunk 1: "## Section 1.1\nSubsection content..."
- Chunk 2: "# Chapter 2: Methods\nContent for chapter 2..."

**Benefits:**
- Respects document structure
- Preserves headings for context
- Natural semantic boundaries

### Plain Text / Prose

**Strategy:** Paragraph or sentence chunking

Paragraph chunking splits on double newlines:
```
Paragraph 1.

Paragraph 2.

Paragraph 3.
```

Sentence chunking for narrative content:
```
Sentence 1. Sentence 2. Sentence 3.
```

**Configuration:**
```sql
SELECT chunking_strategy, chunk_size_default
FROM document_type
WHERE name = 'plain_text';

--  chunking_strategy | chunk_size_default
-- -------------------|-------------------
--  paragraph         | 1200
```

## Performance and Scalability

### Storage Impact

Chunked documents require more storage:

| Document Size | Chunks (avg) | Storage Multiplier | Notes |
|---------------|--------------|-------------------|-------|
| 5 KB | 5 | 1.2× | Metadata overhead |
| 50 KB | 40 | 1.15× | Amortized overhead |
| 500 KB | 350 | 1.10× | Minimal overhead % |

**Calculation:**
```
Storage = original_size + (chunks × metadata_size) + (chunks × embedding_size)

For 50 KB document with 40 chunks:
  Original: 50,000 bytes
  Metadata: 40 × 200 bytes = 8,000 bytes
  Embeddings: 40 × (768 dims × 4 bytes) = 122,880 bytes
  Total: ~181 KB (3.6× original)
```

Most storage comes from embeddings, not chunking overhead.

### Search Performance

Chunk-level search is faster than full-document search:

| Corpus Size | Full-Doc Search | Chunk Search | Speedup |
|-------------|----------------|--------------|---------|
| 1,000 docs | 50 ms | 45 ms | 1.1× |
| 10,000 docs | 200 ms | 150 ms | 1.3× |
| 100,000 docs | 800 ms | 400 ms | 2× |

**Why faster:**
- More granular HNSW index (better recall with lower `ef_search`)
- Smaller embedding vectors to compare
- Better L2 cache utilization

### Deduplication Cost

Deduplication adds minimal overhead:

```
Search time breakdown:
  Database query: 90-95%
  Deduplication: 3-5%
  Network/serialization: 2-5%
```

For typical searches (20 results, 2-3 chunks per doc), deduplication adds <5 ms.

## Troubleshooting

### Issue: Too Many Chunks

**Symptom:** Document split into >100 chunks

**Causes:**
- `chunk_size_default` too small
- Document has many short sections
- Recursive chunker falling back to character splits

**Solutions:**
1. Increase `chunk_size_default` for document type
2. Adjust `preserve_boundaries` setting
3. Switch to different chunking strategy

```sql
-- Increase chunk size for markdown documents
UPDATE document_type
SET chunk_size_default = 2000
WHERE name = 'markdown';
```

### Issue: Missing Context in Chunks

**Symptom:** Search returns chunks lacking surrounding context

**Causes:**
- Overlap too small
- Chunk boundaries split critical information
- Semantic chunker not preserving structures

**Solutions:**
1. Increase `chunk_overlap_default`
2. Use semantic chunker for structured content
3. Add preprocessing to normalize document structure

```sql
-- Increase overlap for code documents
UPDATE document_type
SET chunk_overlap_default = 200
WHERE category = 'code';
```

### Issue: Duplicate Results Despite Deduplication

**Symptom:** Same document appears multiple times in results

**Causes:**
- Deduplication disabled (`deduplicate=false`)
- Chunks from different embedding sets
- Chain metadata not properly linked

**Solutions:**
1. Enable deduplication: `deduplicate=true`
2. Check `note_chunk_chain` table for orphaned chunks
3. Re-run embedding jobs to rebuild chain metadata

```sql
-- Verify chain integrity
SELECT
    ncc.chain_id,
    ncc.total_chunks,
    COUNT(nc.id) as actual_chunks
FROM note_chunk_chain ncc
LEFT JOIN note_chunk nc ON ncc.chain_id = nc.note_id
GROUP BY ncc.chain_id, ncc.total_chunks
HAVING COUNT(nc.id) != ncc.total_chunks;
```

### Issue: Poor Search Relevance

**Symptom:** Irrelevant chunks rank high in results

**Causes:**
- Chunks too small (lack context)
- Chunks too large (mix unrelated topics)
- Wrong chunking strategy for content type

**Solutions:**
1. Tune chunk size for content type
2. Switch chunking strategy (e.g., semantic for markdown)
3. Adjust FTS vs semantic weights in hybrid search
4. Add document type detection rules

**Testing workflow:**
1. Disable deduplication to see all matching chunks
2. Review chunk boundaries and content
3. Adjust chunk size and overlap
4. Re-embed document
5. Test search again

## API Reference

### Search with Deduplication

```bash
GET /api/v1/search?q={query}&deduplicate={true|false}
```

**Parameters:**
- `q` - Search query (required)
- `deduplicate` - Enable deduplication (default: `true`)
- `expand` - Include full chain info (default: `false`)

**Response:**
```json
{
  "results": [
    {
      "note_id": "uuid",
      "score": 0.85,
      "snippet": "...",
      "title": "Document Title",
      "chain_info": {
        "chain_id": "uuid",
        "chunks_matched": 3,
        "best_chunk_sequence": 5,
        "total_chunks": 10
      }
    }
  ]
}
```

### Get Chunk Chain

```bash
GET /api/v1/notes/{note_id}/chunks
```

**Response:**
```json
{
  "chain_id": "uuid",
  "original_title": "Document Title",
  "total_chunks": 10,
  "chunks": [
    {
      "chunk_sequence": 0,
      "text": "...",
      "start_offset": 0,
      "end_offset": 1200,
      "metadata": {}
    }
  ]
}
```

### Get Full Document

```bash
GET /api/v1/notes/{chain_id}
```

Returns the complete original document with all metadata.

## Related Documentation

- [Chunking Guide](./chunking.md) - Chunking strategies and configuration
- [Search Guide](./search-guide.md) - General search usage
- [Search Operators](./search-operators.md) - Query syntax
- [Embedding Pipeline](./embedding-pipeline.md) - Embedding generation
- [Document Types Guide](./document-types-guide.md) - Document type registry
