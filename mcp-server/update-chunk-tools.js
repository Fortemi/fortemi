#!/usr/bin/env node
/**
 * Script to update MCP tools for chunk-aware document handling
 * Issue #113
 */

const fs = require('fs');
const path = require('path');

const indexPath = path.join(__dirname, 'index.js');
const backupPath = path.join(__dirname, `index.js.backup-${Date.now()}`);

// Read the file
let content = fs.readFileSync(indexPath, 'utf8');

// Create backup
fs.copyFileSync(indexPath, backupPath);
console.log(`Backup created: ${backupPath}`);

// 1. Update get_note handler
const getNoteHandler = `        case "get_note": {
          const params = new URLSearchParams();
          if (args.full_document) params.set("full_document", "true");
          const query = params.toString() ? \`?\${params}\` : "";
          result = await apiRequest("GET", \`/api/v1/notes/\${args.id}\${query}\`);
          break;
        }`;

content = content.replace(
  /case "get_note":\s*result = await apiRequest\("GET", `\/api\/v1\/notes\/\$\{args\.id\}`\);\s*break;/,
  getNoteHandler
);

// 2. Update search_notes handler to add new parameters
const searchNotesOld = /case "search_notes": \{[\s\S]*?const params = new URLSearchParams\(\{ q: args\.query \}\);[\s\S]*?if \(args\.set\) params\.set\("set", args\.set\);/;
const searchNotesReplacement = `case "search_notes": {
          const params = new URLSearchParams({ q: args.query });
          if (args.limit) params.set("limit", args.limit);
          if (args.mode) params.set("mode", args.mode);
          if (args.set) params.set("set", args.set);
          if (args.deduplicate_chains !== undefined) params.set("deduplicate_chains", args.deduplicate_chains);
          if (args.expand_chains) params.set("expand_chains", "true");`;

content = content.replace(searchNotesOld, searchNotesReplacement);

// 3. Add get_document_chain handler after get_note_links
const getDocumentChainHandler = `
        case "get_document_chain": {
          const params = new URLSearchParams();
          if (args.include_content !== undefined) params.set("include_content", args.include_content);
          const query = params.toString() ? \`?\${params}\` : "";
          result = await apiRequest("GET", \`/api/v1/notes/\${args.chain_id}/chain\${query}\`);
          break;
        }

        case "get_note_links":`;

content = content.replace(
  /case "get_note_links":/,
  getDocumentChainHandler
);

// 4. Update get_note tool schema
const getNoteToolOld = /\{\s*name: "get_note",[\s\S]*?inputSchema: \{[\s\S]*?properties: \{[\s\S]*?id: \{ type: "string", description: "UUID of the note" \},[\s\S]*?\},[\s\S]*?required: \["id"\],[\s\S]*?\},[\s\S]*?\},/;
const getNoteToolNew = `{
    name: "get_note",
    description: \`Get complete details for a specific note.

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

Use this to retrieve the full context of a note for analysis or reference.\`,
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
  },`;

content = content.replace(getNoteToolOld, getNoteToolNew);

// 5. Update search_notes tool schema
const searchNotesToolOld = /\{\s*name: "search_notes",[\s\S]*?inputSchema: \{[\s\S]*?properties: \{[\s\S]*?query:[\s\S]*?limit:[\s\S]*?mode:[\s\S]*?set: \{ type: "string", description: "Embedding set slug to restrict semantic search \(optional\)" \},[\s\S]*?\},[\s\S]*?required: \["query"\],[\s\S]*?\},[\s\S]*?\},/;
const searchNotesToolNew = `{
    name: "search_notes",
    description: \`Search notes using hybrid full-text and semantic search.

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

Use semantic mode when looking for conceptually related content even if exact keywords don't match.\`,
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
  },`;

content = content.replace(searchNotesToolOld, searchNotesToolNew);

// 6. Add get_document_chain tool after get_note_links
const getDocumentChainTool = `,
  {
    name: "get_document_chain",
    description: \`Get all chunks in a document chain for navigation.

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
- Assembling a partial document from specific chunks\`,
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
  }`;

content = content.replace(
  /(\{\s*name: "get_note_links",[\s\S]*?\},)\s*\{/,
  `$1${getDocumentChainTool},\n  {`
);

// Write updated content
fs.writeFileSync(indexPath, content, 'utf8');

console.log('Successfully updated MCP tools for chunk-aware handling:');
console.log('✓ Updated get_note tool schema (added full_document parameter)');
console.log('✓ Updated get_note handler');
console.log('✓ Updated search_notes tool schema (added deduplicate_chains, expand_chains)');
console.log('✓ Updated search_notes handler');
console.log('✓ Added get_document_chain tool');
console.log('✓ Added get_document_chain handler');
console.log('');
console.log(`Backup saved: ${backupPath}`);
