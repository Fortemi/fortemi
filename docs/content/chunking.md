# Intelligent Document Chunking

Fortémi uses intelligent chunking strategies to split documents into semantically meaningful chunks for optimal embedding quality. This document covers the chunking module, available strategies, and usage guidelines.

## Overview

Effective chunking is critical for embedding quality. Poor chunking leads to:

- **Lost context** - Related information split across chunks
- **Noisy embeddings** - Mixed concepts in single chunks
- **Poor retrieval** - Relevant content missed in searches

The chunking module (`matric-db::chunking`) provides 5 strategies optimized for different content types.

## Chunking Strategies

### 1. SentenceChunker

Splits text at sentence boundaries using punctuation patterns.

**Best For:**
- Narrative content
- Prose and articles
- Content where sentence boundaries are meaningful

**Features:**
- Handles abbreviations (Dr., Mr., etc.)
- Avoids splitting on decimal numbers (3.14)
- Respects question marks and exclamation points

```rust
use matric_db::chunking::{Chunker, SentenceChunker, ChunkerConfig};

let config = ChunkerConfig {
    max_chunk_size: 1000,  // Max chars per chunk
    min_chunk_size: 100,   // Min chars (smaller merged)
    overlap: 50,           // Chars overlap for context
};

let chunker = SentenceChunker::new(config);
let chunks = chunker.chunk("Your text here.");
```

### 2. ParagraphChunker

Splits text at paragraph boundaries (double newlines).

**Best For:**
- Structured documents
- Blog posts
- Documentation with clear sections

**Features:**
- Respects natural paragraph breaks
- Groups related sentences together
- Maintains structural context

```rust
let chunker = ParagraphChunker::new(config);
let chunks = chunker.chunk(document_text);
```

### 3. SemanticChunker ⭐ Recommended

Splits at natural semantic boundaries: headings, lists, code blocks, horizontal rules.

**Best For:**
- Markdown documents
- Technical documentation
- Mixed content (code + prose)

**Features:**
- Recognizes markdown headings (#, ##, etc.)
- Preserves code blocks intact
- Keeps list items together
- Respects horizontal rules as section breaks

```rust
let chunker = SemanticChunker::new(config);
let chunks = chunker.chunk(markdown_content);

// Chunks include metadata about their type
for chunk in chunks {
    if let Some(chunk_type) = chunk.metadata.get("chunk_type") {
        println!("Type: {}, Text: {}", chunk_type, chunk.text);
    }
}
```

**Metadata Tags:**
- `heading` - Section header
- `code_block` - Code fence content
- `list` - Bullet or numbered list
- `paragraph` - Regular text
- `horizontal_rule` - Section divider

### 4. SlidingWindowChunker

Fixed-size chunks with configurable overlap.

**Best For:**
- Long-form content without clear structure
- Dense technical text
- When consistent chunk sizes needed

**Features:**
- Predictable chunk sizes
- Configurable overlap for context preservation
- UTF-8 safe boundary handling

```rust
let config = ChunkerConfig {
    max_chunk_size: 500,
    min_chunk_size: 100,
    overlap: 100,  // 100 chars overlap between chunks
};

let chunker = SlidingWindowChunker::new(config);
let chunks = chunker.chunk(long_text);
```

### 5. RecursiveChunker

Hierarchical splitting: paragraphs → sentences → characters.

**Best For:**
- Mixed content with variable structure
- Content where optimal boundary varies
- Fallback when other strategies fail

**Features:**
- Tries paragraph boundaries first
- Falls back to sentence boundaries
- Last resort: character boundaries
- Ensures all chunks meet size constraints

```rust
let chunker = RecursiveChunker::new(config);
let chunks = chunker.chunk(mixed_content);
```

## Configuration

All chunkers use `ChunkerConfig`:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_chunk_size` | usize | 1000 | Maximum characters per chunk |
| `min_chunk_size` | usize | 100 | Minimum chunk size (smaller merged) |
| `overlap` | usize | 100 | Characters to overlap between chunks |

**Guidelines:**

| Embedding Model | Recommended max_chunk_size |
|-----------------|---------------------------|
| OpenAI text-embedding-3-small | 1000-2000 |
| Ollama nomic-embed-text | 500-1000 |
| Sentence Transformers | 256-512 |

## Chunk Structure

Each chunk contains:

```rust
pub struct Chunk {
    pub text: String,           // The chunk content
    pub start_offset: usize,    // Byte offset in original
    pub end_offset: usize,      // Ending byte offset
    pub metadata: HashMap<String, String>,  // Type info, etc.
}
```

**Offset Tracking:**

Offsets enable mapping chunks back to source positions:

```rust
let chunks = chunker.chunk(document);
for chunk in chunks {
    println!("Position {}-{}: {}",
        chunk.start_offset,
        chunk.end_offset,
        &document[chunk.start_offset..chunk.end_offset]
    );
}
```

## Strategy Selection Guide

| Content Type | Recommended Strategy | Reason |
|--------------|---------------------|--------|
| Markdown docs | SemanticChunker | Respects headings/code/lists |
| Blog posts | ParagraphChunker | Natural paragraph breaks |
| Technical articles | SemanticChunker | Handles code blocks |
| Prose/narrative | SentenceChunker | Sentence boundaries matter |
| Dense text | SlidingWindowChunker | Consistent chunk sizes |
| Mixed/unknown | RecursiveChunker | Adaptive fallback |

## Performance Considerations

### Memory Usage

- Chunks are owned strings (cloned from input)
- Metadata is minimal (HashMap<String, String>)
- Process large documents in streaming fashion if memory constrained

### Processing Speed

| Strategy | Relative Speed | Notes |
|----------|---------------|-------|
| SlidingWindowChunker | Fastest | Simple byte slicing |
| ParagraphChunker | Fast | Regex on newlines |
| SentenceChunker | Medium | Sentence detection regex |
| SemanticChunker | Medium | Multiple regex patterns |
| RecursiveChunker | Slowest | Multi-pass processing |

### UTF-8 Safety

All chunkers handle UTF-8 correctly:

- Never split in middle of multi-byte characters
- `find_char_boundary_before/after` helpers ensure valid boundaries
- Safe with emoji, CJK, RTL text

## Integration with Embeddings

The chunking module integrates with the embedding pipeline:

```rust
// In the NLP pipeline:
let chunker = SemanticChunker::new(config);
let chunks = chunker.chunk(&note.content);

for chunk in chunks {
    let embedding = embedding_service.embed(&chunk.text).await?;
    store_chunk_embedding(note_id, chunk, embedding).await?;
}
```

**Chunk Embeddings Table:**

```sql
CREATE TABLE chunk_embeddings (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES notes(id),
    chunk_index INTEGER,
    chunk_text TEXT,
    start_offset INTEGER,
    end_offset INTEGER,
    embedding vector(384)
);
```

## API Usage

The chunking module is used internally by the embedding job. Direct API access is not exposed, but understanding chunking helps with:

1. **Content formatting** - Write markdown with clear structure for better chunks
2. **Search tuning** - Understand why certain content matches/doesn't match
3. **Debugging** - Investigate retrieval issues related to chunking

## Testing

The module includes 54 unit tests covering:

- Empty and single-char inputs
- UTF-8 boundary handling
- Overlap mechanics
- Metadata preservation
- All chunker strategies

Run tests:

```bash
cargo test --package matric-db chunking
```

## Related Documentation

- [Architecture](./architecture.md) - System overview
- [Embedding Sets](./embedding-sets.md) - Focused search contexts
- [MCP Tools](./mcp.md) - Agent integration
