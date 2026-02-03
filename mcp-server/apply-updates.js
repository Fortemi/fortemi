#!/usr/bin/env node
/**
 * Apply MCP chunk-aware document handling updates
 * Issue #113
 */

const fs = require('fs');
const path = require('path');

const indexPath = path.join(__dirname, 'index.js');
const backupPath = path.join(__dirname, `index.js.backup-${Date.now()}`);

console.log('Reading index.js...');
let lines = fs.readFileSync(indexPath, 'utf8').split('\n');

// Create backup
fs.writeFileSync(backupPath, lines.join('\n'));
console.log(`✓ Backup created: ${backupPath}`);

// Track changes
let changes = [];

// 1. Update get_note handler (line 94-96)
if (lines[93] && lines[93].includes('case "get_note":')) {
  const newHandler = [
    '        case "get_note": {',
    '          const params = new URLSearchParams();',
    '          if (args.full_document) params.set("full_document", "true");',
    '          const query = params.toString() ? `?${params}` : "";',
    '          result = await apiRequest("GET", `/api/v1/notes/${args.id}${query}`);',
    '          break;',
    '        }'
  ];
  lines.splice(93, 3, ...newHandler);
  changes.push('Updated get_note handler');
}

// Adjust line numbers after first change (+4 lines)
let offset = 4;

// 2. Update search_notes handler (line 128+offset)
const searchIdx = 128 + offset - 1;
for (let i = searchIdx; i < searchIdx + 10; i++) {
  if (lines[i] && lines[i].includes('if (args.set) params.set("set", args.set);')) {
    lines.splice(i + 1, 0,
      '          if (args.deduplicate_chains !== undefined) params.set("deduplicate_chains", args.deduplicate_chains);',
      '          if (args.expand_chains) params.set("expand_chains", "true");'
    );
    changes.push('Updated search_notes handler');
    offset += 2;
    break;
  }
}

// 3. Add get_document_chain handler (line 146+offset)
const linksIdx = 146 + offset - 1;
for (let i = linksIdx; i < linksIdx + 5; i++) {
  if (lines[i] && lines[i].includes('case "get_note_links":')) {
    const newHandler = [
      '',
      '        case "get_document_chain": {',
      '          const params = new URLSearchParams();',
      '          if (args.include_content !== undefined) params.set("include_content", args.include_content);',
      '          const query = params.toString() ? `?${params}` : "";',
      '          result = await apiRequest("GET", `/api/v1/notes/${args.chain_id}/chain${query}`);',
      '          break;',
      '        }',
      ''
    ];
    lines.splice(i, 0, ...newHandler);
    changes.push('Added get_document_chain handler');
    offset += 9;
    break;
  }
}

// Reset offset for tools section (different part of file)
offset = 0;

// 4. Update get_note tool schema (line ~818)
for (let i = 817; i < 900; i++) {
  if (lines[i] && lines[i].includes('name: "get_note",')) {
    // Find description and update it
    for (let j = i; j < i + 20; j++) {
      if (lines[j] && lines[j].includes('Use this to retrieve the full context')) {
        // Add chunk handling section before this line
        lines.splice(j, 0,
          '',
          'CHUNK HANDLING:',
          '- If the note is part of a chunked document, you\'ll receive the individual chunk by default',
          '- Set full_document=true to get the complete stitched document instead',
          '- Use get_document_chain to explore all chunks in a document',
          ''
        );
        break;
      }
    }
    // Find properties section and update
    for (let j = i; j < i + 30; j++) {
      if (lines[j] && lines[j].includes('id: { type: "string", description: "UUID of the note" },')) {
        lines[j] = '        id: { type: "string", description: "UUID of the note (chunk ID or chain ID)" },';
        lines.splice(j + 1, 0,
          '        full_document: { type: "boolean", default: false, description: "If true and note is chunked, return full stitched document" },'
        );
        break;
      }
    }
    changes.push('Updated get_note tool schema');
    break;
  }
}

// 5. Update search_notes tool schema (line ~839)
for (let i = 830; i < 920; i++) {
  if (lines[i] && lines[i].includes('name: "search_notes",')) {
    // Find and add chunk handling section
    for (let j = i; j < i + 30; j++) {
      if (lines[j] && lines[j].includes('Returns ranked results with:')) {
        lines.splice(j, 0,
          '',
          'CHUNK HANDLING:',
          '- By default, chunks from the same document are deduplicated (deduplicate_chains=true)',
          '- Set expand_chains=true to get full document content for matches',
          '- Search operates on chunk level but can return document-level results',
          ''
        );
        break;
      }
    }
    // Add new properties
    for (let j = i; j < i + 40; j++) {
      if (lines[j] && lines[j].includes('set: { type: "string", description: "Embedding set slug')) {
        lines.splice(j + 1, 0,
          '        deduplicate_chains: { type: "boolean", default: true, description: "Group chunk matches from same document (default: true)" },',
          '        expand_chains: { type: "boolean", default: false, description: "Return full document content for matches (default: false)" },'
        );
        break;
      }
    }
    changes.push('Updated search_notes tool schema');
    break;
  }
}

// 6. Add get_document_chain tool (after get_note_links at ~903)
for (let i = 900; i < 1000; i++) {
  if (lines[i] && lines[i].includes('name: "get_note_links",')) {
    // Find the end of get_note_links tool
    for (let j = i; j < i + 30; j++) {
      if (lines[j] && lines[j].trim() === '},') {
        // Check if next non-empty line starts a new tool
        let k = j + 1;
        while (k < j + 5 && lines[k] && lines[k].trim() === '') k++;
        if (lines[k] && lines[k].trim() === '{') {
          // Insert new tool here
          const newTool = [
            '  {',
            '    name: "get_document_chain",',
            '    description: `Get all chunks in a document chain for navigation.',
            '',
            'When a large document is chunked for embedding, all chunks share a common chain_id.',
            'This tool retrieves metadata about all chunks in the chain, allowing you to:',
            '- Navigate between chunks sequentially',
            '- Understand document structure',
            '- Fetch specific chunks by index',
            '- Optionally include chunk content',
            '',
            'RETURNS:',
            '- chain_id: The UUID linking all chunks',
            '- chunks: Array of chunk metadata (id, index, start/end offsets)',
            '- If include_content=true, each chunk includes its text content',
            '',
            'USE WHEN:',
            '- You need to understand document structure',
            '- Navigating through long documents chunk by chunk',
            '- Assembling a partial document from specific chunks`,',
            '    inputSchema: {',
            '      type: "object",',
            '      properties: {',
            '        chain_id: { type: "string", description: "Chain UUID or any chunk ID in the chain" },',
            '        include_content: { type: "boolean", default: false, description: "Include full text content of each chunk (default: false)" },',
            '      },',
            '      required: ["chain_id"],',
            '    },',
            '  },'
          ];
          lines.splice(k, 0, ...newTool);
          changes.push('Added get_document_chain tool');
          break;
        }
      }
    }
    break;
  }
}

// Write updated file
fs.writeFileSync(indexPath, lines.join('\n'));

console.log('\n✓ Successfully updated index.js\n');
console.log('Changes applied:');
changes.forEach((change, i) => console.log(`  ${i + 1}. ${change}`));
console.log(`\nBackup: ${backupPath}`);
console.log('\nTo test: npx @modelcontextprotocol/inspector node index.js');
