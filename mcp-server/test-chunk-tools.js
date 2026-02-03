#!/usr/bin/env node
/**
 * Test cases for chunk-aware MCP tools
 * Issue #113
 *
 * These tests verify the tool schemas and handler logic.
 * Actual API endpoint testing requires backend implementation.
 */

const assert = require('assert');
const fs = require('fs');
const path = require('path');

// Load the tools from index.js
const indexContent = fs.readFileSync(path.join(__dirname, 'index.js'), 'utf8');

console.log('Testing MCP Chunk-Aware Tools...\n');

// Test 1: Verify get_note tool has full_document parameter
console.log('Test 1: get_note tool schema');
const getNoteToolMatch = indexContent.match(/name: "get_note",[\s\S]*?inputSchema:[\s\S]*?properties:[\s\S]*?full_document/);
assert(getNoteToolMatch, 'get_note tool should have full_document parameter');
console.log('✓ get_note has full_document parameter');

// Test 2: Verify search_notes has deduplicate_chains parameter
console.log('\nTest 2: search_notes tool schema');
const searchToolMatch = indexContent.match(/name: "search_notes",[\s\S]*?inputSchema:[\s\S]*?properties:[\s\S]*?deduplicate_chains/);
assert(searchToolMatch, 'search_notes tool should have deduplicate_chains parameter');
console.log('✓ search_notes has deduplicate_chains parameter');

// Test 3: Verify search_notes has expand_chains parameter
const expandChainsMatch = indexContent.match(/name: "search_notes",[\s\S]*?inputSchema:[\s\S]*?properties:[\s\S]*?expand_chains/);
assert(expandChainsMatch, 'search_notes tool should have expand_chains parameter');
console.log('✓ search_notes has expand_chains parameter');

// Test 4: Verify get_document_chain tool exists
console.log('\nTest 3: get_document_chain tool exists');
const docChainToolMatch = indexContent.match(/name: "get_document_chain"/);
assert(docChainToolMatch, 'get_document_chain tool should exist');
console.log('✓ get_document_chain tool exists');

// Test 5: Verify get_document_chain has required properties
const chainIdMatch = indexContent.match(/name: "get_document_chain"[\s\S]*?properties:[\s\S]*?chain_id/);
const includeContentMatch = indexContent.match(/name: "get_document_chain"[\s\S]*?properties:[\s\S]*?include_content/);
assert(chainIdMatch, 'get_document_chain should have chain_id property');
assert(includeContentMatch, 'get_document_chain should have include_content property');
console.log('✓ get_document_chain has chain_id and include_content parameters');

// Test 6: Verify get_note handler uses query parameters
console.log('\nTest 4: Handler implementations');
const getNoteHandlerMatch = indexContent.match(/case "get_note":[\s\S]*?full_document[\s\S]*?apiRequest/);
assert(getNoteHandlerMatch, 'get_note handler should check full_document parameter');
console.log('✓ get_note handler implements full_document parameter');

// Test 7: Verify search_notes handler uses new parameters
const searchHandlerMatch = indexContent.match(/case "search_notes":[\s\S]*?deduplicate_chains[\s\S]*?expand_chains/);
assert(searchHandlerMatch, 'search_notes handler should check deduplicate_chains and expand_chains');
console.log('✓ search_notes handler implements chunk parameters');

// Test 8: Verify get_document_chain handler exists
const docChainHandlerMatch = indexContent.match(/case "get_document_chain":[\s\S]*?chain_id[\s\S]*?apiRequest/);
assert(docChainHandlerMatch, 'get_document_chain handler should exist and call API');
console.log('✓ get_document_chain handler implemented');

// Test 9: Verify API endpoint patterns
console.log('\nTest 5: API endpoint patterns');
const getNoteEndpoint = indexContent.match(/\/api\/v1\/notes\/\$\{args\.id\}\$\{query\}/);
assert(getNoteEndpoint, 'get_note should call /api/v1/notes/:id with query params');
console.log('✓ get_note calls correct endpoint');

const searchEndpoint = indexContent.match(/\/api\/v1\/search\?\$\{params\}/);
assert(searchEndpoint, 'search_notes should call /api/v1/search with params');
console.log('✓ search_notes calls correct endpoint');

const chainEndpoint = indexContent.match(/\/api\/v1\/notes\/\$\{args\.chain_id\}\/chain/);
assert(chainEndpoint, 'get_document_chain should call /api/v1/notes/:chain_id/chain');
console.log('✓ get_document_chain calls correct endpoint');

// Test 10: Verify descriptions mention chunk handling
console.log('\nTest 6: Documentation');
const getNoteChunkDocs = indexContent.match(/name: "get_note"[\s\S]*?CHUNK HANDLING:/);
assert(getNoteChunkDocs, 'get_note description should explain chunk handling');
console.log('✓ get_note includes chunk handling documentation');

const searchChunkDocs = indexContent.match(/name: "search_notes"[\s\S]*?CHUNK HANDLING:/);
assert(searchChunkDocs, 'search_notes description should explain chunk handling');
console.log('✓ search_notes includes chunk handling documentation');

console.log('\n====================================');
console.log('All tests passed! ✓');
console.log('====================================\n');

console.log('Next steps:');
console.log('1. Implement backend API endpoints:');
console.log('   - GET /api/v1/notes/:id?full_document=true');
console.log('   - GET /api/v1/search?deduplicate_chains=true&expand_chains=false');
console.log('   - GET /api/v1/notes/:chain_id/chain?include_content=false');
console.log('');
console.log('2. Test with MCP inspector:');
console.log('   npx @modelcontextprotocol/inspector node index.js');
console.log('');
console.log('3. Integration test with actual API once endpoints are implemented');
