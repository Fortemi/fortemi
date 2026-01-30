# Chunk-Aware MCP Tools Implementation (Ticket #113)

## Overview

Implemented three new MCP tools for chunk-aware document handling in the Matric Memory MCP server. These tools provide explicit support for working with chunked documents that were split during ingestion.

## Tools Implemented

### 1. get_full_document

**Purpose**: Reconstruct chunked documents by stitching all chunks back together.

**API Endpoint**: `GET /api/v1/notes/{id}/full`

**Parameters**:
- `id` (required): UUID of the note or chain ID

**Returns**:
- `id`: Note ID or chain ID
- `title`: Original document title (chunk suffixes removed)
- `content`: Full reconstructed content
- `is_chunked`: Boolean indicating if this is a chunked document
- `chunks`: Array of chunk metadata (null for regular notes)
  - `id`: Chunk note ID
  - `sequence`: Chunk number in sequence
  - `title`: Chunk title
  - `byte_range`: [start, end] byte positions
- `total_chunks`: Number of chunks (null for regular notes)
- `tags`: Deduplicated tags from all chunks
- `created_at`, `updated_at`: Timestamps

**Use Cases**:
- Downloading complete documents that were split during ingestion
- Viewing full original content before chunking
- Exporting documents with chunk metadata

### 2. search_with_dedup

**Purpose**: Search with explicit deduplication to avoid duplicate results from chunked documents.

**API Endpoint**: `GET /api/v1/search?q=...`

**Parameters**:
- `query` (required): Search query
- `limit` (optional): Maximum results (default: 20)
- `mode` (optional): Search mode - "hybrid" (default), "fts", or "semantic"
- `set` (optional): Embedding set slug to restrict search

**Returns**:
- `results`: Array of deduplicated search hits
  - `note_id`: Best matching chunk ID
  - `score`: Relevance score
  - `snippet`: Text excerpt
  - `title`: Note title
  - `tags`: Associated tags
  - `chain_info`: Chunk metadata (if chunked)
    - `chain_id`: Document chain ID
    - `total_chunks`: Total chunks in document
    - `chunks_matched`: How many chunks matched
- `query`: Original search query
- `total`: Number of results

**Note**: Deduplication is already the default behavior in the search API. This tool makes it explicit for clarity.

**Use Cases**:
- Search large documents without duplicate results
- Understand which chunks matched from chunked documents
- Get document-level results rather than chunk-level

### 3. get_chunk_chain

**Purpose**: Get all chunks in a document chain with detailed metadata.

**API Endpoint**: `GET /api/v1/notes/{chain_id}/full?include_content=...`

**Parameters**:
- `chain_id` (required): UUID of the chain (first chunk ID or any chunk in chain)
- `include_content` (optional): Include full reconstructed content (default: true)

**Returns**: Same structure as `get_full_document`:
- `id`: Chain ID
- `title`: Original document title
- `content`: Full reconstructed content (if include_content=true)
- `is_chunked`: true for chunked documents
- `chunks`: Array of all chunks with sequence, titles, and byte ranges
- `total_chunks`: Number of chunks
- `tags`: Deduplicated tags from all chunks
- `created_at`, `updated_at`: Timestamps

**Use Cases**:
- Inspecting how a document was chunked
- Getting individual chunk IDs for targeted retrieval
- Understanding chunk boundaries and overlap
- Debugging chunking strategy

## Implementation Details

### File Changes

**File**: `/home/roctinam/dev/matric-memory/mcp-server/index.js`

**Handler Additions** (lines ~757-785):
```javascript
case "get_full_document":
  result = await apiRequest("GET", `/api/v1/notes/${args.id}/full`);
  break;

case "search_with_dedup": {
  const dedupParams = new URLSearchParams({ q: args.query });
  if (args.limit) dedupParams.set("limit", args.limit);
  if (args.mode) dedupParams.set("mode", args.mode);
  if (args.set) dedupParams.set("set", args.set);
  result = await apiRequest("GET", `/api/v1/search?${dedupParams}`);
  break;
}

case "get_chunk_chain": {
  const chainParams = new URLSearchParams();
  if (args.include_content !== undefined) {
    chainParams.set("include_content", args.include_content.toString());
  }
  result = await apiRequest("GET", `/api/v1/notes/${args.chain_id}/full?${chainParams}`);
  break;
}
```

**Tool Definitions** (lines ~2421-2550): Added comprehensive tool schemas with descriptions, parameters, and return value documentation.

### Testing

**Test File**: `/home/roctinam/dev/matric-memory/mcp-server/test-new-chunk-tools.js`

Tests verify:
1. Tool schemas exist in the tools array
2. Required parameters are defined
3. Handlers are implemented in the switch statement
4. Correct API endpoints are called
5. Documentation mentions chunk handling

**Test Results**: All 13 tests pass successfully.

```bash
cd mcp-server
node test-new-chunk-tools.js
```

### Syntax Validation

The updated index.js file passes Node.js syntax validation:

```bash
node --check index.js  # No errors
```

## API Backend Requirements

These MCP tools are implemented and ready to use. The backend API already provides:

- **GET /api/v1/notes/{id}/full** - Implemented in `crates/matric-api/src/main.rs` (line 2576)
  - Uses `ReconstructionService` to stitch chunks together
  - Returns `FullDocumentResponse` with chunk metadata

- **GET /api/v1/search** - Existing search endpoint
  - Deduplication is enabled by default via `DeduplicationConfig`
  - Returns `EnhancedSearchHit` with chain_info

## Usage Examples

### Get Full Document

```javascript
// Reconstruct a chunked document
{
  "name": "get_full_document",
  "arguments": {
    "id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

### Search with Deduplication

```javascript
// Search with explicit deduplication
{
  "name": "search_with_dedup",
  "arguments": {
    "query": "machine learning embeddings",
    "limit": 10,
    "mode": "hybrid"
  }
}
```

### Get Chunk Chain

```javascript
// Inspect chunk structure
{
  "name": "get_chunk_chain",
  "arguments": {
    "chain_id": "550e8400-e29b-41d4-a716-446655440000",
    "include_content": false  // Just metadata, not full content
  }
}
```

## Testing with MCP Inspector

To test the tools interactively:

```bash
cd mcp-server
npx @modelcontextprotocol/inspector node index.js
```

This will open a browser interface where you can:
1. See all available tools including the new chunk-aware tools
2. Test tool calls with different parameters
3. Inspect responses and error handling

## Integration Notes

- **Graceful Handling**: All tools handle both chunked and non-chunked documents gracefully
- **Metadata**: Chunk metadata is only present for chunked documents (null for regular notes)
- **API Compatibility**: Tools use existing API endpoints that are already implemented
- **Error Handling**: Standard MCP error handling applies (network errors, auth errors, etc.)

## Related Files

- `/home/roctinam/dev/matric-memory/crates/matric-api/src/main.rs` - API endpoint handlers
- `/home/roctinam/dev/matric-memory/crates/matric-api/src/openapi.yaml` - API specification
- `/home/roctinam/dev/matric-memory/crates/matric-search/src/deduplication.rs` - Deduplication logic
- `/home/roctinam/dev/matric-memory/crates/matric-api/src/services/reconstruction.rs` - Document reconstruction

## Completion Status

- [x] Tests written (TDD - Red phase)
- [x] Tools implemented (TDD - Green phase)
- [x] All tests passing
- [x] Syntax validation passing
- [x] Documentation complete
- [x] Ready for integration testing with backend API

## Next Steps

1. Test tools with actual API backend using MCP inspector
2. Verify reconstruction works correctly for multi-chunk documents
3. Confirm deduplication metadata is returned as expected
4. Add integration tests once backend testing is complete
