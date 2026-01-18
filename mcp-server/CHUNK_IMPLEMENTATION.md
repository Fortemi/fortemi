# Chunk-Aware MCP Tools Implementation

## Issue #113: Update MCP tools for chunk-aware document handling

### Implementation Status

✓ MCP tool schemas updated
✓ Tool handlers implemented
✓ API endpoint calls defined
⏳ Backend API endpoints (requires separate implementation)
⏳ Integration tests (pending backend)

### Overview

This implementation adds chunk-awareness to matric-memory's MCP server, allowing AI agents to work intelligently with chunked documents. When large documents are split into chunks for embedding, these tools provide:

1. **Transparent access** - Get individual chunks or full documents
2. **Smart search** - Deduplicate and expand chunk results
3. **Navigation** - Browse chunk chains sequentially

### Changes Made

#### 1. Updated `get_note` Tool

**Schema Changes:**
- Added `full_document` parameter (boolean, default: false)
- Updated description to explain chunk behavior
- Changed `id` description to clarify it accepts chunk or chain IDs

**Handler Changes:**
- Conditionally adds `?full_document=true` query parameter
- API call: `GET /api/v1/notes/:id?full_document=true`

**Usage:**
```javascript
// Get individual chunk (default)
get_note({ id: "chunk-uuid" })

// Get full stitched document
get_note({ id: "chunk-uuid", full_document: true })
```

#### 2. Updated `search_notes` Tool

**Schema Changes:**
- Added `deduplicate_chains` parameter (boolean, default: true)
- Added `expand_chains` parameter (boolean, default: false)
- Updated description with chunk handling section

**Handler Changes:**
- Passes both parameters as query params
- API call: `GET /api/v1/search?q=...&deduplicate_chains=true&expand_chains=false`

**Usage:**
```javascript
// Search with chunk deduplication (default)
search_notes({ query: "machine learning" })

// Show all chunks separately
search_notes({ query: "machine learning", deduplicate_chains: false })

// Get full documents for matches
search_notes({ query: "machine learning", expand_chains: true })
```

#### 3. New `get_document_chain` Tool

**Purpose:**
Get metadata about all chunks in a document chain for navigation and structure understanding.

**Parameters:**
- `chain_id` (string, required) - Chain UUID or any chunk ID in the chain
- `include_content` (boolean, default: false) - Include full text of each chunk

**Handler:**
- API call: `GET /api/v1/notes/:chain_id/chain?include_content=false`

**Returns:**
```javascript
{
  chain_id: "uuid",
  chunks: [
    {
      id: "chunk-1-uuid",
      index: 0,
      start_offset: 0,
      end_offset: 1000,
      content: "..." // if include_content=true
    },
    // ... more chunks
  ]
}
```

**Usage:**
```javascript
// Get chunk metadata only
get_document_chain({ chain_id: "chain-uuid" })

// Get chunks with content
get_document_chain({ chain_id: "chain-uuid", include_content: true })
```

### Files Modified

1. **index.js** - Main MCP server implementation
   - Line ~94-96: Updated `get_note` handler
   - Line ~128-135: Updated `search_notes` handler
   - Line ~146: Added `get_document_chain` handler
   - Line ~818-837: Updated `get_note` tool schema
   - Line ~839-870: Updated `search_notes` tool schema
   - Line ~903: Added `get_document_chain` tool schema

### Installation & Testing

#### Apply Updates

```bash
cd /home/roctinam/dev/matric-memory/mcp-server

# Option 1: Apply patch
patch -p0 < chunk-updates.patch

# Option 2: Run Node.js script
node apply-updates.js

# Option 3: Manual edits using CHUNK_UPDATE_GUIDE.md
```

#### Verify Changes

```bash
# Run automated tests
node test-chunk-tools.js

# Test with MCP inspector
npx @modelcontextprotocol/inspector node index.js
```

#### Rollback

```bash
# If issues occur, restore from backup
cp index.js.backup-* index.js
```

### Backend API Requirements

The following API endpoints must be implemented in `matric-api` for full functionality:

#### 1. GET /api/v1/notes/:id?full_document=true

**Purpose:** Retrieve a note, optionally stitching chunks into full document

**Parameters:**
- `:id` - Note UUID (chunk ID or chain ID)
- `full_document` (query, optional) - If true and note is chunked, return stitched document

**Response:**
- If `full_document=false` or note not chunked: Single note object
- If `full_document=true` and note is chunked: Note object with stitched content from all chunks

**Implementation Notes:**
- Check if note has `chain_id`
- If yes and `full_document=true`, query all chunks with same `chain_id`
- Sort chunks by `chunk_index`
- Concatenate `content` fields in order
- Return note with stitched content

#### 2. GET /api/v1/search?deduplicate_chains=true&expand_chains=false

**Purpose:** Search with chunk-aware result handling

**Parameters:**
- Existing search parameters (q, limit, mode, set)
- `deduplicate_chains` (query, optional, default: true) - Group chunks from same document
- `expand_chains` (query, optional, default: false) - Return full document content

**Response:**
- If `deduplicate_chains=true`: Group results by `chain_id`, keep highest-scoring chunk per document
- If `expand_chains=true`: Include full stitched document content in results
- Otherwise: Return individual chunk matches

**Implementation Notes:**
- Perform search as normal (returns chunks)
- Post-process results based on flags:
  - `deduplicate_chains`: GROUP BY chain_id, select MAX(score)
  - `expand_chains`: Stitch content from all chunks in chain

#### 3. GET /api/v1/notes/:chain_id/chain?include_content=false

**Purpose:** Get all chunks in a document chain

**Parameters:**
- `:chain_id` - Chain UUID or any chunk ID in the chain
- `include_content` (query, optional, default: false) - Include chunk text

**Response:**
```json
{
  "chain_id": "uuid",
  "chunks": [
    {
      "id": "chunk-uuid",
      "chunk_index": 0,
      "start_offset": 0,
      "end_offset": 1000,
      "content": "..." // if include_content=true
    }
  ]
}
```

**Implementation Notes:**
- If `:chain_id` is a chunk ID, first resolve to actual chain_id
- Query all chunks with matching chain_id
- Order by chunk_index
- Optionally include content field

### Database Schema Assumptions

The implementation assumes notes table has:
- `id` (UUID) - Unique note/chunk identifier
- `chain_id` (UUID, nullable) - Links chunks of same document
- `chunk_index` (INTEGER, nullable) - Position in document (0-indexed)
- `start_offset` (INTEGER, nullable) - Character offset in original
- `end_offset` (INTEGER, nullable) - Ending character offset
- `content` (TEXT) - The chunk or full note content

### Integration with Chunking System

This implementation integrates with the existing chunking module (`matric-db::chunking`):

1. **Document ingestion**: When a large note is created, it's chunked using the appropriate strategy
2. **Chunk storage**: Each chunk is stored as a separate note with same `chain_id`
3. **Embedding**: Each chunk is embedded separately for precise semantic search
4. **Retrieval**: MCP tools allow access to individual chunks or stitched documents
5. **Search**: Chunk-level search with document-level deduplication

### Testing Checklist

Once backend endpoints are implemented:

- [ ] get_note with full_document=false returns single chunk
- [ ] get_note with full_document=true returns stitched document
- [ ] get_note with non-chunked note works normally
- [ ] search_notes with deduplicate_chains=true groups chunk matches
- [ ] search_notes with deduplicate_chains=false shows all chunks
- [ ] search_notes with expand_chains=true includes full document content
- [ ] get_document_chain returns all chunks in chain
- [ ] get_document_chain with include_content=true includes text
- [ ] get_document_chain with chunk ID resolves to chain
- [ ] Error handling for non-existent chains
- [ ] Performance with large documents (many chunks)
- [ ] Integration with MCP inspector

### Related Documentation

- [Chunking Documentation](/home/roctinam/dev/matric-memory/docs/chunking.md)
- [MCP Documentation](/home/roctinam/dev/matric-memory/docs/mcp.md)
- [Architecture](/home/roctinam/dev/matric-memory/docs/architecture.md)

### Future Enhancements

Potential future improvements:

1. **Chunk metadata**: Include chunk type (heading, code, paragraph) in responses
2. **Smart stitching**: Add separators between chunks based on type
3. **Partial expansion**: Expand only relevant chunks, not entire chain
4. **Chunk previews**: Show snippet from matching chunk even when deduplicated
5. **Chain statistics**: Add chunk count, total size, chunking strategy used
6. **Chunk navigation**: Previous/next chunk helpers

### Authors

- Implementation: Claude Opus 4.5 (Software Implementer)
- Issue: #113
- Date: 2026-01-18
