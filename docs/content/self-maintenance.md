# Self-Maintenance: Indexing Fortémi

Matric-memory can index and search its own codebase, demonstrating the power of content-aware document types, intelligent chunking, and semantic code search.

## Overview

This capability showcases:

- **Code Document Types** - Format-specific handling for Rust, TypeScript, SQL
- **Syntactic Chunking** - Breaking code into meaningful units (functions, modules, statements)
- **Semantic Code Search** - Find relevant code by describing functionality, not exact keywords
- **Multilingual Codebase** - Index mixed Rust, TypeScript, SQL, and Markdown in a single collection

## Quick Start

Run the self-indexing demo:

```bash
./scripts/self-index-demo.sh
```

This script will:

1. Create a collection named `Fortémi-codebase`
2. Index key Rust source files from `crates/matric-core` and `crates/matric-db`
3. Index TypeScript MCP server files
4. Index SQL migration files
5. Index core documentation files
6. Demonstrate semantic search queries

**Prerequisites:**

- Fortémi API server running on `http://localhost:3000` (or set `MATRIC_API_URL`)
- `curl` and `jq` installed
- Embedding service configured and running

## Use Cases

### 1. Code Discovery

Find relevant functions by describing what you need, rather than knowing exact function names:

```bash
# Find code related to embedding generation
curl "http://localhost:3000/api/v1/search?q=embedding+generation+trait"

# Find database repository implementations
curl "http://localhost:3000/api/v1/search?q=postgresql+repository+implementation"
```

### 2. Dependency Tracking

Search for imports, usages, and dependencies across the codebase:

```bash
# Find uses of specific traits
curl "http://localhost:3000/api/v1/search?q=EmbeddingProvider+trait"

# Find database migrations affecting specific tables
curl "http://localhost:3000/api/v1/search?q=notes+table+schema+migration"
```

### 3. Documentation Search

Find documentation alongside relevant code:

```bash
# Find docs about chunking strategies
curl "http://localhost:3000/api/v1/search?q=chunking+strategies"

# Find API endpoint documentation
curl "http://localhost:3000/api/v1/search?q=REST+API+endpoints"
```

### 4. Architecture Understanding

Query for patterns, design decisions, and architectural components:

```bash
# Find repository pattern implementations
curl "http://localhost:3000/api/v1/search?q=repository+pattern+database"

# Find error handling patterns
curl "http://localhost:3000/api/v1/search?q=error+handling+Result"
```

## Document Types Used

The demo indexes files with appropriate document type hints:

| File Type | Format | Chunking Strategy | Examples |
|-----------|--------|------------------|----------|
| Rust source | `rust` | Syntactic (Tree-sitter) | `*.rs` files |
| TypeScript | `typescript` | Syntactic (Tree-sitter) | `*.ts` files |
| SQL migrations | `sql` | Statement-level | `*.sql` files |
| Markdown docs | `markdown` | Semantic (headings, code blocks) | `*.md` files |

### Format Field

When creating notes programmatically, specify the `format` field to enable content-aware processing:

```bash
curl -X POST "http://localhost:3000/api/v1/notes" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "pub fn example() { ... }",
    "format": "rust",
    "source": "self-index",
    "tags": ["rust", "source-code"]
  }'
```

**Available formats:**

- `markdown` (default) - Semantic chunking by headings, lists, code blocks
- `rust` - Syntactic chunking for Rust code (functions, structs, impl blocks)
- `typescript` - Syntactic chunking for TypeScript/JavaScript
- `sql` - Statement-level chunking for SQL
- `plaintext` - Basic sentence/paragraph chunking

## How It Works

### 1. Document Ingestion

When a note is created with a specific `format`:

1. Content is stored in the `notes` table with the format hint
2. Background job picks up the note for processing
3. Format-specific chunker splits content into semantic units

### 2. Intelligent Chunking

For code files, Tree-sitter-based syntactic chunking:

- **Rust**: Functions, structs, impl blocks, modules
- **TypeScript**: Functions, classes, methods, interfaces
- **SQL**: CREATE/ALTER/INSERT statements

For documentation:

- **Markdown**: Sections by headings, code blocks, lists
- **Plaintext**: Sentences and paragraphs

### 3. Embedding Generation

Each chunk is embedded using the configured model:

- Default: `nomic-embed-text` (768-dim, Matryoshka-capable)
- Stores both full and truncated MRL embeddings for two-stage retrieval

### 4. Semantic Search

Search queries:

1. Query is embedded using the same model
2. Coarse retrieval uses low-dim MRL embeddings (e.g., 64-dim)
3. Fine ranking uses full-dim embeddings (768-dim)
4. Results ranked by cosine similarity

## Example Workflow

### Self-Index the Codebase

```bash
# Run the demo script
export MATRIC_API_URL="http://localhost:3000"
./scripts/self-index-demo.sh
```

**Output:**

```
=== Fortémi Self-Maintenance Demo ===
Indexing Fortémi codebase into itself...

Creating collection for codebase...
Created collection: 550e8400-e29b-41d4-a716-446655440000

Indexing Rust source files...
  Indexed: crates/matric-core/src/lib.rs
  Indexed: crates/matric-core/src/models.rs
  ...

Indexed 15 Rust files

=== Semantic Code Search Demo ===

Query 1: 'embedding repository trait'
---------------------------------------
  - embedding_provider.rs (score: 0.89)
    Tags: rust, source-code
  - traits.rs (score: 0.85)
    Tags: rust, source-code
  ...
```

### Search for Specific Functionality

```bash
# Find code related to search ranking
curl "http://localhost:3000/api/v1/search?q=search+ranking+algorithm&collection_id=550e8400-e29b-41d4-a716-446655440000"
```

### Explore Related Code

Using MCP tools (via Claude):

```
> Find all references to the EmbeddingProvider trait

I'll search for EmbeddingProvider references in the Fortémi-codebase collection.

Found 8 references across:
- crates/matric-core/src/traits.rs (trait definition)
- crates/matric-inference/src/ollama.rs (implementation)
- crates/matric-jobs/src/embed.rs (usage in background jobs)
...
```

## Advanced Usage

### Custom Indexing Script

Create your own indexing script for specific use cases:

```bash
#!/bin/bash
# Index only API-related code

API_URL="http://localhost:3000"
COLLECTION_ID="your-collection-id"

# Index API handlers
for file in crates/matric-api/src/handlers/*.rs; do
  content=$(cat "$file" | jq -Rs .)

  curl -X POST "$API_URL/api/v1/notes" \
    -H "Content-Type: application/json" \
    -d "{
      \"content\": $content,
      \"format\": \"rust\",
      \"source\": \"api-index\",
      \"collection_id\": \"$COLLECTION_ID\",
      \"tags\": [\"rust\", \"api-handler\"]
    }"
done
```

### Integration with MCP

Configure Claude Code to access the self-indexed codebase:

1. Run the self-index demo to create the collection
2. Use MCP search tools to query the codebase
3. Ask Claude to find implementations, explain code, or suggest refactorings

```
> Using the Fortémi-codebase collection, explain how the hybrid search algorithm works

Based on the indexed code, the hybrid search implementation combines:
1. Full-text search via PostgreSQL's tsquery
2. Semantic search via vector similarity
3. RRF (Reciprocal Rank Fusion) to merge results
...
```

## Performance Considerations

### Indexing Time

- Small codebase (50 files): ~30 seconds
- Medium codebase (500 files): ~5 minutes
- Large codebase (5000 files): ~50 minutes

**Factors:**

- Embedding model inference time
- File size and chunking complexity
- Database write throughput

### Search Performance

- Coarse retrieval (MRL 64-dim): <50ms for 10K chunks
- Fine ranking (full 768-dim): <200ms for top 100 candidates
- Total search latency: <300ms typical

### Storage Requirements

For 1000 code files (~500KB each):

- Raw content: ~500MB
- Chunks (avg 5/file): 5000 chunks
- Embeddings (768-dim + 64-dim MRL): ~25MB

## Limitations

### Current Scope

- Requires embedding service running (Ollama or compatible)
- No incremental updates (re-index on changes)
- No syntax highlighting in search results (planned)
- No cross-file symbol resolution (planned)

### Future Enhancements

See issues for planned improvements:

- **Incremental indexing** - Watch file system, re-index on changes
- **Syntax-aware snippets** - Return highlighted code in search results
- **Symbol graph** - Cross-reference functions, types, imports
- **Multi-repo support** - Index multiple codebases in separate collections

## Graph Self-Maintenance

Beyond indexing its own content, Fortémi also maintains the quality of its own knowledge graph through the `GraphMaintenance` job pipeline. This pipeline runs periodically and can be triggered on demand:

```bash
# Trigger the full graph quality pipeline
curl -X POST http://localhost:3000/api/v1/graph/maintenance

# Or via MCP
trigger_graph_maintenance({})
```

The pipeline applies four steps to the entire graph:

1. **Normalization** — Corrects score distribution bias (`GRAPH_NORMALIZATION_GAMMA`)
2. **SNN (Shared Nearest Neighbors)** — Strengthens genuine neighborhood connections, prunes isolated coincidental links; breaks the "seashell pattern"
3. **PFNET sparsification** — Removes transitive redundancy, producing a cleaner graph
4. **Louvain community detection** — Assigns community labels to all notes for cluster-aware navigation

After each run, a diagnostics snapshot is saved. Use `GET /api/v1/graph/diagnostics/history` to review graph health over time, and `GET /api/v1/graph/diagnostics/compare` to compare snapshots.

### Concept Embedding Quality

The `clustering:` prefix on SKOS concept labels in embeddings, combined with TF-IDF filtering via `EMBED_CONCEPT_MAX_DOC_FREQ`, improves the signal-to-noise ratio in semantic links. Generic concepts that appear in most notes are filtered out, so embeddings reflect genuinely distinctive content.

## Related Documentation

- [Chunking Strategies](./chunking.md) - Deep dive on content-aware chunking
- [Embedding Sets](./embedding-sets.md) - Focused search contexts for code vs docs
- [Knowledge Graph Guide](./knowledge-graph-guide.md) - Graph quality pipeline and maintenance
- [MCP Integration](./mcp.md) - Using MCP tools for code exploration
- [Search Guide](./search-guide.md) - Advanced search techniques

## Troubleshooting

### No search results

**Issue:** Search returns empty results after indexing

**Solutions:**

1. Check embedding job queue: `curl http://localhost:3000/api/v1/jobs/status`
2. Verify embeddings generated: Check `chunk_embeddings` table
3. Wait longer - embedding large codebases takes time
4. Check logs for errors in the embedding service

### Slow search

**Issue:** Search takes >5 seconds

**Solutions:**

1. Ensure MRL truncation is enabled (two-stage retrieval)
2. Check database indexes on `chunk_embeddings`
3. Reduce search limit parameter
4. Use collection filters to narrow scope

### Memory usage

**Issue:** High memory usage during indexing

**Solutions:**

1. Index files in smaller batches
2. Reduce chunking max_chunk_size
3. Use streaming API for large files
4. Increase database connection pool size

## Support

For questions or issues:

- GitHub Issues: https://github.com/integromat/Fortémi/issues
- Documentation: http://localhost:3000/docs
- MCP Troubleshooting: [mcp-troubleshooting.md](./mcp-troubleshooting.md)
