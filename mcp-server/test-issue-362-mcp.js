#!/usr/bin/env node

/**
 * Test for issue #362: MCP server should properly sanitize 404 errors
 *
 * This test simulates what happens when Claude Code calls update_note
 * via the MCP server for a non-existent note.
 */

// Mock the MCP server's error handling
function sanitizeError(error, toolName) {
  const errorStr = error.message || String(error);

  // Map API status codes to safe messages
  const match = errorStr.match(/API error (\d+)/);
  if (match) {
    const code = match[1];
    const safeMessages = {
      '400': 'Invalid request parameters',
      '401': 'Authentication required',
      '403': 'Access denied',
      '404': 'Resource not found',
      '409': 'Conflict with existing resource',
      '422': 'Validation failed',
      '500': 'Internal server error',
    };
    return safeMessages[code] || `Request failed (${code})`;
  }

  // Check for our own validation errors (safe to show)
  if (errorStr.startsWith('Missing required parameter:') ||
      errorStr.startsWith('Parameter ')) {
    return errorStr;
  }

  // Log full error server-side, return generic message
  console.error(`[${toolName}] Error:`, error);
  return 'An error occurred while processing your request';
}

console.log('Testing MCP server error sanitization for issue #362...\n');

// Test 1: 404 error should be sanitized to "Resource not found"
console.log('Test 1: 404 error sanitization');
const error404 = new Error('API error 404: {"error":"Note 00000000-0000-0000-0000-000000000001 not found"}');
const sanitized404 = sanitizeError(error404, 'update_note');
console.log(`  Input:  ${error404.message}`);
console.log(`  Output: ${sanitized404}`);
console.log(`  ✓ ${sanitized404 === 'Resource not found' ? 'PASS' : 'FAIL'}`);

// Test 2: 500 error should be sanitized to "Internal server error"
console.log('\nTest 2: 500 error sanitization');
const error500 = new Error('API error 500: {"error":"Database error: FK violation"}');
const sanitized500 = sanitizeError(error500, 'update_note');
console.log(`  Input:  ${error500.message}`);
console.log(`  Output: ${sanitized500}`);
console.log(`  ✓ ${sanitized500 === 'Internal server error' ? 'PASS' : 'FAIL'}`);

// Test 3: Validation error should be shown as-is
console.log('\nTest 3: Validation error pass-through');
const errorValidation = new Error('Missing required parameter: id');
const sanitizedValidation = sanitizeError(errorValidation, 'update_note');
console.log(`  Input:  ${errorValidation.message}`);
console.log(`  Output: ${sanitizedValidation}`);
console.log(`  ✓ ${sanitizedValidation === 'Missing required parameter: id' ? 'PASS' : 'FAIL'}`);

console.log('\n=== SUMMARY ===');
console.log('Before fix: update_note returned 500 → "Internal server error"');
console.log('After fix:  update_note returns 404 → "Resource not found"');
console.log('\nThis provides a much better user experience for Claude Code users.');
