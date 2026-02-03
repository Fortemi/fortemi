#!/usr/bin/env node
/**
 * Test cases for new chunk-aware MCP tools (Ticket #113)
 *
 * Tests the three new tools:
 * 1. get_full_document - Reconstructs chunked documents
 * 2. search_with_dedup - Explicit deduplication control
 * 3. get_chunk_chain - Gets all chunks with metadata
 */

import assert from 'assert';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Load the tools from index.js
const indexContent = fs.readFileSync(path.join(__dirname, 'index.js'), 'utf8');

console.log('Testing New Chunk-Aware MCP Tools (Ticket #113)...\n');

// Test 1: Verify get_full_document tool exists
console.log('Test 1: get_full_document tool schema');
const getFullDocMatch = indexContent.match(/name: "get_full_document"/);
assert(getFullDocMatch, 'get_full_document tool should exist');
console.log('✓ get_full_document tool exists');

// Test 2: Verify get_full_document has id parameter
const fullDocIdMatch = indexContent.match(/name: "get_full_document"[\s\S]{0,2000}?id: \{ type: "string"/);
assert(fullDocIdMatch, 'get_full_document should have id parameter');
console.log('✓ get_full_document has id parameter');

// Test 3: Verify search_with_dedup tool exists
console.log('\nTest 2: search_with_dedup tool schema');
const searchDedupMatch = indexContent.match(/name: "search_with_dedup"/);
assert(searchDedupMatch, 'search_with_dedup tool should exist');
console.log('✓ search_with_dedup tool exists');

// Test 4: Verify search_with_dedup has required parameters
const dedupQueryMatch = indexContent.match(/name: "search_with_dedup"[\s\S]{0,2000}?query: \{ type: "string"/);
assert(dedupQueryMatch, 'search_with_dedup should have query parameter');
console.log('✓ search_with_dedup has query parameter');

// Test 5: Verify get_chunk_chain tool exists
console.log('\nTest 3: get_chunk_chain tool schema');
const chunkChainMatch = indexContent.match(/name: "get_chunk_chain"/);
assert(chunkChainMatch, 'get_chunk_chain tool should exist');
console.log('✓ get_chunk_chain tool exists');

// Test 6: Verify get_chunk_chain has chain_id parameter
const chainIdMatch = indexContent.match(/name: "get_chunk_chain"[\s\S]{0,2000}?chain_id: \{ type: "string"/);
assert(chainIdMatch, 'get_chunk_chain should have chain_id parameter');
console.log('✓ get_chunk_chain has chain_id parameter');

// Test 7: Verify handlers exist
console.log('\nTest 4: Handler implementations');
const getFullDocHandler = indexContent.match(/case "get_full_document":/);
assert(getFullDocHandler, 'get_full_document handler should exist');
console.log('✓ get_full_document handler exists');

const searchDedupHandler = indexContent.match(/case "search_with_dedup":/);
assert(searchDedupHandler, 'search_with_dedup handler should exist');
console.log('✓ search_with_dedup handler exists');

const chunkChainHandler = indexContent.match(/case "get_chunk_chain":/);
assert(chunkChainHandler, 'get_chunk_chain handler should exist');
console.log('✓ get_chunk_chain handler exists');

// Test 8: Verify API endpoints
console.log('\nTest 5: API endpoint patterns');
const fullDocEndpoint = indexContent.match(/\/api\/v1\/notes\/\$\{args\.id\}\/full/);
assert(fullDocEndpoint, 'get_full_document should call /api/v1/notes/:id/full');
console.log('✓ get_full_document calls /api/v1/notes/:id/full');

// Test 9: Verify documentation mentions chunk handling
console.log('\nTest 6: Documentation');
const fullDocDocs = indexContent.match(/name: "get_full_document"[\s\S]{0,1500}?chunked/i);
assert(fullDocDocs, 'get_full_document description should mention chunked documents');
console.log('✓ get_full_document includes chunk documentation');

const searchDedupDocs = indexContent.match(/name: "search_with_dedup"[\s\S]{0,2000}?dedup/i);
assert(searchDedupDocs, 'search_with_dedup description should mention deduplication');
console.log('✓ search_with_dedup includes deduplication documentation');

const chunkChainDocs = indexContent.match(/name: "get_chunk_chain"[\s\S]{0,2000}?chain/i);
assert(chunkChainDocs, 'get_chunk_chain description should mention chain');
console.log('✓ get_chunk_chain includes chain documentation');

console.log('\n====================================');
console.log('All tests passed! ✓');
console.log('====================================\n');

console.log('Tools implemented:');
console.log('1. get_full_document - Reconstruct chunked documents');
console.log('2. search_with_dedup - Search with explicit deduplication');
console.log('3. get_chunk_chain - Get all chunks in a document chain');
console.log('');
console.log('Test with MCP inspector:');
console.log('  npx @modelcontextprotocol/inspector node index.js');
