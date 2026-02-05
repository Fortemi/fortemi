#!/usr/bin/env node
/**
 * Test cases for document type MCP tools
 * Issue #421
 *
 * These tests verify the tool schemas and handler logic for document type management.
 */

const assert = require('assert');
const fs = require('fs');
const path = require('path');

// Load the tools from index.js
const indexContent = fs.readFileSync(path.join(__dirname, 'index.js'), 'utf8');

console.log('Testing MCP Document Type Tools...\n');

// Test 1: Verify list_document_types tool exists
console.log('Test 1: list_document_types tool schema');
const listToolMatch = indexContent.match(/name: "list_document_types"/);
assert(listToolMatch, 'list_document_types tool should exist');
console.log('✓ list_document_types tool exists');

// Test 2: Verify list_document_types has category parameter
const listCategoryMatch = indexContent.match(/name: "list_document_types"[\s\S]*?properties:[\s\S]*?category/);
assert(listCategoryMatch, 'list_document_types should have category parameter');
console.log('✓ list_document_types has category parameter');

// Test 3: Verify get_document_type tool exists
console.log('\nTest 2: get_document_type tool schema');
const getToolMatch = indexContent.match(/name: "get_document_type"/);
assert(getToolMatch, 'get_document_type tool should exist');
console.log('✓ get_document_type tool exists');

// Test 4: Verify get_document_type has name parameter
const getNameMatch = indexContent.match(/name: "get_document_type"[\s\S]*?properties:[\s\S]*?name:[\s\S]*?type: "string"/);
assert(getNameMatch, 'get_document_type should have name parameter');
console.log('✓ get_document_type has name parameter in schema');

// Test 5: Verify create_document_type tool exists
console.log('\nTest 3: create_document_type tool schema');
const createToolMatch = indexContent.match(/name: "create_document_type"/);
assert(createToolMatch, 'create_document_type tool should exist');
console.log('✓ create_document_type tool exists');

// Test 6: Verify create_document_type has required properties
const createNameMatch = indexContent.match(/name: "create_document_type"[\s\S]*?properties:[\s\S]*?name:/);
const createDisplayNameMatch = indexContent.match(/name: "create_document_type"[\s\S]*?properties:[\s\S]*?display_name:/);
const createCategoryMatch = indexContent.match(/name: "create_document_type"[\s\S]*?properties:[\s\S]*?category:/);
const createChunkingMatch = indexContent.match(/name: "create_document_type"[\s\S]*?properties:[\s\S]*?chunking_strategy:/);
assert(createNameMatch, 'create_document_type should have name property');
assert(createDisplayNameMatch, 'create_document_type should have display_name property');
assert(createCategoryMatch, 'create_document_type should have category property');
assert(createChunkingMatch, 'create_document_type should have chunking_strategy property');
console.log('✓ create_document_type has required properties');

// Test 7: Verify update_document_type tool exists
console.log('\nTest 4: update_document_type tool schema');
const updateToolMatch = indexContent.match(/name: "update_document_type"/);
assert(updateToolMatch, 'update_document_type tool should exist');
console.log('✓ update_document_type tool exists');

// Test 8: Verify delete_document_type tool exists
console.log('\nTest 5: delete_document_type tool schema');
const deleteToolMatch = indexContent.match(/name: "delete_document_type"/);
assert(deleteToolMatch, 'delete_document_type tool should exist');
console.log('✓ delete_document_type tool exists');

// Test 9: Verify detect_document_type tool exists
console.log('\nTest 6: detect_document_type tool schema');
const detectToolMatch = indexContent.match(/name: "detect_document_type"/);
assert(detectToolMatch, 'detect_document_type tool should exist');
console.log('✓ detect_document_type tool exists');

// Test 10: Verify detect_document_type has filename and content parameters
const detectFilenameMatch = indexContent.match(/name: "detect_document_type"[\s\S]*?properties:[\s\S]*?filename:/);
const detectContentMatch = indexContent.match(/name: "detect_document_type"[\s\S]*?properties:[\s\S]*?content:/);
assert(detectFilenameMatch, 'detect_document_type should have filename parameter');
assert(detectContentMatch, 'detect_document_type should have content parameter');
console.log('✓ detect_document_type has filename and content parameters');

// Test 11: Verify list_document_types handler exists
console.log('\nTest 7: Handler implementations');
const listHandlerMatch = indexContent.match(/case "list_document_types":[\s\S]*?apiRequest/);
assert(listHandlerMatch, 'list_document_types handler should exist and call API');
console.log('✓ list_document_types handler implemented');

// Test 12: Verify get_document_type handler exists
const getHandlerMatch = indexContent.match(/case "get_document_type":[\s\S]*?args\.name[\s\S]*?apiRequest/);
assert(getHandlerMatch, 'get_document_type handler should exist and call API');
console.log('✓ get_document_type handler implemented');

// Test 13: Verify create_document_type handler exists
const createHandlerMatch = indexContent.match(/case "create_document_type":[\s\S]*?apiRequest[\s\S]*?"POST"/);
assert(createHandlerMatch, 'create_document_type handler should exist with POST method');
console.log('✓ create_document_type handler implemented');

// Test 14: Verify update_document_type handler exists
const updateHandlerMatch = indexContent.match(/case "update_document_type":[\s\S]*?apiRequest[\s\S]*?"PATCH"/);
assert(updateHandlerMatch, 'update_document_type handler should exist with PATCH method');
console.log('✓ update_document_type handler implemented');

// Test 15: Verify delete_document_type handler exists
const deleteHandlerMatch = indexContent.match(/case "delete_document_type":[\s\S]*?apiRequest[\s\S]*?"DELETE"/);
assert(deleteHandlerMatch, 'delete_document_type handler should exist with DELETE method');
console.log('✓ delete_document_type handler implemented');

// Test 16: Verify detect_document_type handler exists
const detectHandlerMatch = indexContent.match(/case "detect_document_type":[\s\S]*?apiRequest[\s\S]*?"POST"/);
assert(detectHandlerMatch, 'detect_document_type handler should exist with POST method');
console.log('✓ detect_document_type handler implemented');

// Test 17: Verify API endpoint patterns
console.log('\nTest 8: API endpoint patterns');
const listEndpoint = indexContent.match(/\/api\/v1\/document-types/);
assert(listEndpoint, 'Should reference /api/v1/document-types endpoint');
console.log('✓ API endpoints reference correct paths');

// Test 18: Verify chunking_strategy enum values
console.log('\nTest 9: Enum validation');
const chunkingEnumMatch = indexContent.match(/name: "create_document_type"[\s\S]*?chunking_strategy:[\s\S]*?enum:[\s\S]*?\["semantic", "syntactic", "fixed", "hybrid", "per_section", "per_unit", "whole"\]/);
assert(chunkingEnumMatch, 'chunking_strategy should have correct enum values');
console.log('✓ chunking_strategy enum includes all expected values');

// Test 19: Verify category description mentions valid categories
console.log('\nTest 10: Documentation');
const categoryDocsMatch = indexContent.match(/name: "list_document_types"[\s\S]*?description:[\s\S]*?code, prose, config/);
assert(categoryDocsMatch, 'list_document_types should document available categories');
console.log('✓ Category filter documentation includes examples');

// Test 20: Verify tools are read-only/destructive annotated appropriately
console.log('\nTest 11: Tool annotations');
const listAnnotationMatch = indexContent.match(/name: "list_document_types"[\s\S]{1,2000}?annotations:[\s\S]*?readOnlyHint: true/);
const getAnnotationMatch = indexContent.match(/name: "get_document_type"[\s\S]{1,2000}?annotations:[\s\S]*?readOnlyHint: true/);
const detectAnnotationMatch = indexContent.match(/name: "detect_document_type"[\s\S]{1,3000}?annotations:[\s\S]*?readOnlyHint: true/);
assert(listAnnotationMatch, 'list_document_types should be marked read-only');
assert(getAnnotationMatch, 'get_document_type should be marked read-only');
assert(detectAnnotationMatch, 'detect_document_type should be marked read-only');
console.log('✓ Read-only tools properly annotated');

console.log('\n====================================');
console.log('All tests passed! ✓');
console.log('====================================\n');

console.log('Next steps:');
console.log('1. Verify backend API endpoints are implemented:');
console.log('   - GET /api/v1/document-types');
console.log('   - GET /api/v1/document-types?category=code');
console.log('   - GET /api/v1/document-types/:name');
console.log('   - POST /api/v1/document-types');
console.log('   - PATCH /api/v1/document-types/:name');
console.log('   - DELETE /api/v1/document-types/:name');
console.log('   - POST /api/v1/document-types/detect');
console.log('');
console.log('2. Test with MCP inspector:');
console.log('   npx @modelcontextprotocol/inspector node index.js');
console.log('');
console.log('3. Integration test with actual API once endpoints are verified');
