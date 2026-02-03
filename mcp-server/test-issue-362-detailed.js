#!/usr/bin/env node

/**
 * Test for issue #362: Detailed error debugging
 */

const API_BASE = process.env.API_BASE || 'http://localhost:3000';

async function apiRequest(method, path, body = null) {
  const config = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body) config.body = JSON.stringify(body);

  const response = await fetch(`${API_BASE}${path}`, config);

  // Get response body as text first
  const text = await response.text();

  return {
    ok: response.ok,
    status: response.status,
    statusText: response.statusText,
    body: text,
    headers: Object.fromEntries(response.headers.entries())
  };
}

async function testUpdateNonExistentNote() {
  console.log('Testing update_note error response details...\n');

  try {
    // Test 1: Update with content (triggers update_original)
    const fakeId1 = '00000000-0000-0000-0000-000000000001';
    console.log(`Test 1: Update non-existent note ${fakeId1} WITH content`);

    const result1 = await apiRequest('PATCH', `/api/v1/notes/${fakeId1}`, {
      content: 'This should fail',
      starred: true
    });

    console.log(`  Status: ${result1.status} ${result1.statusText}`);
    console.log(`  Body: ${result1.body}`);
    console.log(`  Content-Type: ${result1.headers['content-type']}`);

    // Test 2: Update without content (only status)
    const fakeId2 = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
    console.log(`\nTest 2: Update non-existent note ${fakeId2} WITHOUT content`);

    const result2 = await apiRequest('PATCH', `/api/v1/notes/${fakeId2}`, {
      starred: true
    });

    console.log(`  Status: ${result2.status} ${result2.statusText}`);
    console.log(`  Body: ${result2.body}`);
    console.log(`  Content-Type: ${result2.headers['content-type']}`);

    // Test 3: Check if update_status has different behavior
    console.log(`\nTest 3: Update note status endpoint ${fakeId2}`);

    const result3 = await apiRequest('PATCH', `/api/v1/notes/${fakeId2}/status`, {
      starred: true
    });

    console.log(`  Status: ${result3.status} ${result3.statusText}`);
    console.log(`  Body: ${result3.body}`);
    console.log(`  Content-Type: ${result3.headers['content-type']}`);

    console.log('\n=== SUMMARY ===');
    console.log(`With content:    ${result1.status}`);
    console.log(`Without content: ${result2.status}`);
    console.log(`/status route:   ${result3.status}`);

  } catch (error) {
    console.error('\n✗ TEST ERROR');
    console.error(error);
    process.exit(1);
  }
}

testUpdateNonExistentNote();
