# MCP Tools Update for Chunk-Aware Document Handling (Issue #113)

## Overview
This guide provides the exact changes needed to update MCP tools for chunk-aware document handling.

## Changes Required

### 1. Update `get_note` Handler (Line ~94-96)

**Before:**
```javascript
        case "get_note":
          result = await apiRequest("GET", `/api/v1/notes/${args.id}`);
          break;
```

**After:**
```javascript
        case "get_note": {
          const params = new URLSearchParams();
          if (args.full_document) params.set("full_document", "true");
          const query = params.toString() ? `?${params}` : "";
          result = await apiRequest("GET", `/api/v1/notes/${args.id}${query}`);
          break;
        }
```

### 2. Update `search_notes` Handler (Line ~128-135)

**Before:**
```javascript
        case "search_notes": {
          const params = new URLSearchParams({ q: args.query });
          if (args.limit) params.set("limit", args.limit);
          if (args.mode) params.set("mode", args.mode);
          if (args.set) params.set("set", args.set);
          result = await apiRequest("GET", `/api/v1/search?${params}`);
          break;
        }
```

**After:**
```javascript
        case "search_notes": {
          const params = new URLSearchParams({ q: args.query });
          if (args.limit) params.set("limit", args.limit);
          if (args.mode) params.set("mode", args.mode);
          if (args.set) params.set("set", args.set);
          if (args.deduplicate_chains !== undefined) params.set("deduplicate_chains", args.deduplicate_chains);
          if (args.expand_chains) params.set("expand_chains", "true");
          result = await apiRequest("GET", `/api/v1/search?${params}`);
          break;
        }
```

### 3. Add `get_document_chain` Handler (After `get_note_links` handler, ~line 146)

**Insert after `case "get_note_links":`:**
```javascript
        case "get_document_chain": {
          const params = new URLSearchParams();
          if (args.include_content !== undefined) params.set("include_content", args.include_content);
          const query = params.toString() ? `?${params}` : "";
          result = await apiRequest("GET", `/api/v1/notes/${args.chain_id}/chain${query}`);
          break;
        }
```

### 4. Update `get_note` Tool Schema (Line ~817-837)

**Before:**
```javascript
  {
    name: "get_note",
    description: `Get complete details for a specific note.

Returns the full note including:
- Original content (as submitted)
- AI-enhanced revision (structured, contextual)
- Generated title
- Tags (user + AI-generated)
- Semantic links to related notes
- Metadata and timestamps

Use this to retrieve the full context of a note for analysis or reference.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
      },
      required: ["id"],
    },
  },
```

**After:**
```javascript
  {
    name: "get_note",
    description: `Get complete details for a specific note.

Returns the full note including:
- Original content (as submitted)
- AI-enhanced revision (structured, contextual)
- Generated title
- Tags (user + AI-generated)
- Semantic links to related notes
- Metadata and timestamps

CHUNK HANDLING:
- If the note is part of a chunked document, you'll receive the individual chunk by default
- Set full_document=true to get the complete stitched document instead
- Use get_document_chain to explore all chunks in a document

Use this to retrieve the full context of a note for analysis or reference.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note (chunk ID or chain ID)" },
        full_document: {
          type: "boolean",
          default: false,
          description: "If true and note is chunked, return full stitched document"
        },
      },
      required: ["id"],
    },
  },
```

### 5. Update `search_notes` Tool Schema (Line ~838-870)

**Before:**
```javascript
  {
    name: "search_notes",
    description: `Search notes using hybrid full-text and semantic search.

Search modes:
- 'hybrid' (default): Combines keyword matching with semantic similarity for best results
- 'fts': Full-text search only - exact keyword matching
- 'semantic': Vector similarity only - finds conceptually related content

Embedding sets:
- Use 'set' parameter to restrict semantic search to a specific embedding set
- Omit 'set' to search across all embeddings (default behavior)
- Use list_embedding_sets to discover available sets

Returns ranked results with:
- note_id: UUID of the matching note
- score: Relevance score (0.0-1.0)
- snippet: Text excerpt showing matching content
- title: Note title (for quick identification)
- tags: Associated tags (for context)

Use semantic mode when looking for conceptually related content even if exact keywords don't match.`,
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query (natural language or keywords)" },
        limit: { type: "number", description: "Maximum results (default: 20)", default: 20 },
        mode: { type: "string", enum: ["hybrid", "fts", "semantic"], description: "Search mode", default: "hybrid" },
        set: { type: "string", description: "Embedding set slug to restrict semantic search (optional)" },
      },
      required: ["query"],
    },
  },
```

**After:**
```javascript
  {
    name: "search_notes",
    description: `Search notes using hybrid full-text and semantic search.

Search modes:
- 'hybrid' (default): Combines keyword matching with semantic similarity for best results
- 'fts': Full-text search only - exact keyword matching
- 'semantic': Vector similarity only - finds conceptually related content

Embedding sets:
- Use 'set' parameter to restrict semantic search to a specific embedding set
- Omit 'set' to search across all embeddings (default behavior)
- Use list_embedding_sets to discover available sets

CHUNK HANDLING:
- By default, chunks from the same document are deduplicated (deduplicate_chains=true)
- Set expand_chains=true to get full document content for matches
- Search operates on chunk level but can return document-level results

Returns ranked results with:
- note_id: UUID of the matching note
- score: Relevance score (0.0-1.0)
- snippet: Text excerpt showing matching content
- title: Note title (for quick identification)
- tags: Associated tags (for context)

Use semantic mode when looking for conceptually related content even if exact keywords don't match.`,
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query (natural language or keywords)" },
        limit: { type: "number", description: "Maximum results (default: 20)", default: 20 },
        mode: { type: "string", enum: ["hybrid", "fts", "semantic"], description: "Search mode", default: "hybrid" },
        set: { type: "string", description: "Embedding set slug to restrict semantic search (optional)" },
        deduplicate_chains: {
          type: "boolean",
          default: true,
          description: "Group chunk matches from same document (default: true)"
        },
        expand_chains: {
          type: "boolean",
          default: false,
          description: "Return full document content for matches (default: false)"
        },
      },
      required: ["query"],
    },
  },
```

### 6. Add `get_document_chain` Tool (After `get_note_links` tool, ~line 903)

**Insert after the `get_note_links` tool definition:**
```javascript
  {
    name: "get_document_chain",
    description: `Get all chunks in a document chain for navigation.

When a large document is chunked for embedding, all chunks share a common chain_id.
This tool retrieves metadata about all chunks in the chain, allowing you to:
- Navigate between chunks sequentially
- Understand document structure
- Fetch specific chunks by index
- Optionally include chunk content

RETURNS:
- chain_id: The UUID linking all chunks
- chunks: Array of chunk metadata (id, index, start/end offsets)
- If include_content=true, each chunk includes its text content

USE WHEN:
- You need to understand document structure
- Navigating through long documents chunk by chunk
- Assembling a partial document from specific chunks`,
    inputSchema: {
      type: "object",
      properties: {
        chain_id: {
          type: "string",
          description: "Chain UUID or any chunk ID in the chain"
        },
        include_content: {
          type: "boolean",
          default: false,
          description: "Include full text content of each chunk (default: false)"
        },
      },
      required: ["chain_id"],
    },
  },
```

## API Endpoints

The tools will call these API endpoints (to be implemented in matric-api):

1. **GET /api/v1/notes/:id?full_document=true**
   - Returns stitched document if note is part of a chain

2. **GET /api/v1/search?deduplicate_chains=true&expand_chains=false**
   - Search with chunk deduplication and optional expansion

3. **GET /api/v1/notes/:chain_id/chain?include_content=false**
   - Get all chunks in a document chain

## Testing

After implementing changes, test with:

```bash
# Test get_note with full_document
node -e "const mcp = require('./index.js'); ..."

# Or use MCP inspector
npx @modelcontextprotocol/inspector node index.js
```

## Rollback

If issues occur, restore from backup:
```bash
cp index.js.backup-* index.js
```
