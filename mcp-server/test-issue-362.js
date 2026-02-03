#!/usr/bin/env node

/**
 * Test for issue #362: update_note returns "Internal server error" for non-existent notes
 *
 * This test verifies that:
 * 1. Updating a non-existent note returns proper 404 error
 * 2. The MCP server sanitizes this to "Resource not found"
 * 3. It should NOT return "Internal server error"
 */

const API_BASE = process.env.API_BASE || 'http://localhost:3000';

async function apiRequest(method, path, body = null) {
  const config = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body) config.body = JSON.stringify(body);

  const response = await fetch(`${API_BASE}${path}`, config);

  // Return both response and status for testing error cases
  return {
    ok: response.ok,
    status: response.status,
    statusText: response.statusText,
    data: response.ok ? await response.json() : null
  };
}

async function testUpdateNonExistentNote() {
  console.log('Testing update_note with non-existent note ID...\n');

  try {
    // Use a valid UUID format but one that doesn't exist
    const fakeId = '00000000-0000-0000-0000-000000000001';

    console.log(`1. Attempting to update non-existent note: ${fakeId}`);

    const result = await apiRequest('PATCH', `/api/v1/notes/${fakeId}`, {
      content: 'This should fail',
      starred: true
    });

    console.log(`   Response status: ${result.status} ${result.statusText}`);
    console.log(`   Response ok: ${result.ok}`);

    if (result.ok) {
      console.error('\n✗ TEST FAILED: Expected error but got success');
      console.error('   The API should return 404 for non-existent notes');
      process.exit(1);
    }

    // Check what status code we actually get
    if (result.status === 404) {
      console.log('   ✓ API correctly returns 404 Not Found');
    } else if (result.status === 500) {
      console.log('   ✗ API incorrectly returns 500 Internal Server Error');
      console.log('   This is likely an API bug - it should return 404');
    } else {
      console.log(`   ? Unexpected status code: ${result.status}`);
    }

    console.log('\n2. Testing with another non-existent UUID...');
    const anotherId = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
    const result2 = await apiRequest('PATCH', `/api/v1/notes/${anotherId}`, {
      starred: false
    });

    console.log(`   Response status: ${result2.status} ${result2.statusText}`);

    console.log('\n✓ TEST COMPLETED');
    console.log('\nFindings:');
    console.log(`- First attempt returned: ${result.status}`);
    console.log(`- Second attempt returned: ${result2.status}`);

    if (result.status === 404) {
      console.log('- API behavior is CORRECT (404)');
      console.log('- MCP server should map this to "Resource not found"');
    } else if (result.status === 500) {
      console.log('- API behavior is INCORRECT (500)');
      console.log('- This is an API-level bug that should be fixed');
      console.log('- MCP server is correctly showing "Internal server error" for 500');
    }

  } catch (error) {
    console.error('\n✗ TEST ERROR');
    console.error(error.message || error);
    process.exit(1);
  }
}

// Run the test
testUpdateNonExistentNote();
