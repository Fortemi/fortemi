#!/usr/bin/env python3
"""
Apply chunk-aware document handling updates to MCP server index.js
Issue #113
"""

import re
import sys
from datetime import datetime

def main():
    input_file = 'index.js'
    backup_file = f'index.js.backup-{datetime.now().strftime("%Y%m%d%H%M%S")}'

    # Read the file
    with open(input_file, 'r') as f:
        content = f.read()

    # Create backup
    with open(backup_file, 'w') as f:
        f.write(content)
    print(f"✓ Created backup: {backup_file}")

    original_content = content

    # 1. Update get_note handler
    get_note_handler_old = r'        case "get_note":\s+result = await apiRequest\("GET", `/api/v1/notes/\$\{args\.id\}`\);\s+break;'
    get_note_handler_new = '''        case "get_note": {
          const params = new URLSearchParams();
          if (args.full_document) params.set("full_document", "true");
          const query = params.toString() ? `?${params}` : "";
          result = await apiRequest("GET", `/api/v1/notes/${args.id}${query}`);
          break;
        }'''

    content = re.sub(get_note_handler_old, get_note_handler_new, content)
    if content != original_content:
        print("✓ Updated get_note handler")
        original_content = content

    # 2. Update search_notes handler
    search_handler_find = r'(case "search_notes": \{.*?if \(args\.set\) params\.set\("set", args\.set\);)'
    search_handler_replace = r'\1\n          if (args.deduplicate_chains !== undefined) params.set("deduplicate_chains", args.deduplicate_chains);\n          if (args.expand_chains) params.set("expand_chains", "true");'

    content = re.sub(search_handler_find, search_handler_replace, content, flags=re.DOTALL)
    if content != original_content:
        print("✓ Updated search_notes handler")
        original_content = content

    # 3. Add get_document_chain handler
    doc_chain_handler = '''
        case "get_document_chain": {
          const params = new URLSearchParams();
          if (args.include_content !== undefined) params.set("include_content", args.include_content);
          const query = params.toString() ? `?${params}` : "";
          result = await apiRequest("GET", `/api/v1/notes/${args.chain_id}/chain${query}`);
          break;
        }

        '''

    content = re.sub(r'(\s+case "get_note_links":)', doc_chain_handler + r'\1', content, count=1)
    if content != original_content:
        print("✓ Added get_document_chain handler")
        original_content = content

    # 4. Update get_note tool schema
    get_note_tool_old = r'''  \{
    name: "get_note",
    description: `Get complete details for a specific note\.

Returns the full note including:
- Original content \(as submitted\)
- AI-enhanced revision \(structured, contextual\)
- Generated title
- Tags \(user \+ AI-generated\)
- Semantic links to related notes
- Metadata and timestamps

Use this to retrieve the full context of a note for analysis or reference\.`,
    inputSchema: \{
      type: "object",
      properties: \{
        id: \{ type: "string", description: "UUID of the note" \},
      \},
      required: \["id"\],
    \},
  \},'''

    get_note_tool_new = '''  {
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
  },'''

    content = re.sub(get_note_tool_old, get_note_tool_new, content)
    if content != original_content:
        print("✓ Updated get_note tool schema")
        original_content = content

    # 5. Update search_notes tool schema
    search_tool_pattern = r'''  \{
    name: "search_notes",
    description: `Search notes using hybrid full-text and semantic search\.

Search modes:
- 'hybrid' \(default\): Combines keyword matching with semantic similarity for best results
- 'fts': Full-text search only - exact keyword matching
- 'semantic': Vector similarity only - finds conceptually related content

Embedding sets:
- Use 'set' parameter to restrict semantic search to a specific embedding set
- Omit 'set' to search across all embeddings \(default behavior\)
- Use list_embedding_sets to discover available sets

Returns ranked results with:
- note_id: UUID of the matching note
- score: Relevance score \(0\.0-1\.0\)
- snippet: Text excerpt showing matching content
- title: Note title \(for quick identification\)
- tags: Associated tags \(for context\)

Use semantic mode when looking for conceptually related content even if exact keywords don't match\.`,
    inputSchema: \{
      type: "object",
      properties: \{
        query: \{ type: "string", description: "Search query \(natural language or keywords\)" \},
        limit: \{ type: "number", description: "Maximum results \(default: 20\)", default: 20 \},
        mode: \{ type: "string", enum: \["hybrid", "fts", "semantic"\], description: "Search mode", default: "hybrid" \},
        set: \{ type: "string", description: "Embedding set slug to restrict semantic search \(optional\)" \},
      \},
      required: \["query"\],
    \},
  \},'''

    search_tool_new = '''  {
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
  },'''

    content = re.sub(search_tool_pattern, search_tool_new, content)
    if content != original_content:
        print("✓ Updated search_notes tool schema")
        original_content = content

    # 6. Add get_document_chain tool after get_note_links
    doc_chain_tool = ''',
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
  }'''

    # Find get_note_links tool and add after it
    pattern = r'(  \{\s+name: "get_note_links",.*?\},)\s+(  \{)'
    replacement = r'\1' + doc_chain_tool + r'\n\2'
    content = re.sub(pattern, replacement, content, flags=re.DOTALL)
    if content != original_content:
        print("✓ Added get_document_chain tool")
        original_content = content

    # Write updated content
    with open(input_file, 'w') as f:
        f.write(content)

    print(f"\n✓ Successfully updated {input_file}")
    print(f"✓ Backup saved to: {backup_file}")
    print("\nChanges applied:")
    print("  1. Updated get_note handler with full_document parameter")
    print("  2. Updated search_notes handler with deduplicate_chains and expand_chains")
    print("  3. Added get_document_chain handler")
    print("  4. Updated get_note tool schema")
    print("  5. Updated search_notes tool schema")
    print("  6. Added get_document_chain tool")

if __name__ == '__main__':
    main()
