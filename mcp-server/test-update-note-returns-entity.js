#!/usr/bin/env node

/**
 * Test for issue #203: update_note should return the updated entity
 *
 * This test verifies that:
 * 1. update_note returns a note object in the response
 * 2. The returned note reflects the updated values
 */

const API_BASE = process.env.API_BASE || 'http://localhost:3000';

async function apiRequest(method, path, body = null) {
  const config = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body) config.body = JSON.stringify(body);

  const response = await fetch(`${API_BASE}${path}`, config);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }

  // Handle 204 No Content
  if (response.status === 204) {
    return null;
  }

  return await response.json();
}

async function testUpdateNoteReturnsEntity() {
  console.log('Testing update_note returns updated entity...\n');

  try {
    // Step 1: Create a test note
    console.log('1. Creating test note...');
    const createResult = await apiRequest('POST', '/api/v1/notes', {
      content: 'Initial content for update test',
      revision_mode: 'none'
    });
    const noteId = createResult.id;
    console.log(`   Created note: ${noteId}`);

    // Wait a moment for the note to be fully created
    await new Promise(resolve => setTimeout(resolve, 100));

    // Step 2: Update the note and verify response structure
    console.log('\n2. Updating note content...');
    const updateResult = await apiRequest('PATCH', `/api/v1/notes/${noteId}`, {
      content: 'Updated content for test',
      starred: true
    });

    console.log('   Update response structure:');
    console.log(`   - Has 'note' field: ${!!updateResult.note}`);
    console.log(`   - Has 'original' field: ${!!updateResult.original}`);
    console.log(`   - Has 'revised' field: ${!!updateResult.revised}`);
    console.log(`   - Has 'tags' field: ${!!updateResult.tags}`);
    console.log(`   - Has 'links' field: ${!!updateResult.links}`);

    // Verify the note object exists
    if (!updateResult.note) {
      throw new Error('FAIL: update_note did not return a note object');
    }

    // Verify the note ID matches
    if (updateResult.note.id !== noteId) {
      throw new Error(`FAIL: Note ID mismatch. Expected ${noteId}, got ${updateResult.note.id}`);
    }

    // Verify starred flag is updated
    if (updateResult.note.starred !== true) {
      throw new Error(`FAIL: Starred flag not updated. Expected true, got ${updateResult.note.starred}`);
    }

    console.log(`\n   Verified note fields:`);
    console.log(`   - ID: ${updateResult.note.id}`);
    console.log(`   - Starred: ${updateResult.note.starred}`);
    console.log(`   - Title: ${updateResult.note.title || '(null)'}`);

    // Step 3: Verify the update persisted
    console.log('\n3. Fetching note to verify persistence...');
    const fetchedNote = await apiRequest('GET', `/api/v1/notes/${noteId}`);

    if (fetchedNote.note.starred !== true) {
      throw new Error('FAIL: Starred flag not persisted');
    }

    console.log('   Verified update persisted correctly');

    // Step 4: Test updating just the starred flag (no content change)
    console.log('\n4. Testing status-only update...');
    const statusUpdate = await apiRequest('PATCH', `/api/v1/notes/${noteId}`, {
      archived: true
    });

    if (!statusUpdate.note) {
      throw new Error('FAIL: Status-only update did not return note object');
    }

    if (statusUpdate.note.archived !== true) {
      throw new Error('FAIL: Archived flag not updated');
    }

    console.log('   Status-only update works correctly');

    // Cleanup
    console.log('\n5. Cleaning up test note...');
    await apiRequest('DELETE', `/api/v1/notes/${noteId}`);
    console.log('   Test note deleted');

    console.log('\n✓ ALL TESTS PASSED');
    console.log('\nSummary:');
    console.log('- update_note returns full NoteFull object');
    console.log('- Returned note reflects updated values');
    console.log('- Updates persist correctly');
    console.log('- Status-only updates work correctly');

  } catch (error) {
    console.error('\n✗ TEST FAILED');
    console.error(error.message || error);
    process.exit(1);
  }
}

// Run the test
testUpdateNoteReturnsEntity();
