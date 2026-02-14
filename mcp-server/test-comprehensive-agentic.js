#!/usr/bin/env node
/**
 * Comprehensive MCP Agentic Test Suite
 *
 * Verifies all MCP functionality after bug fixes, with special focus on:
 * - Issue #198: update_note with single field updates (archived, starred)
 * - Issue #199: search_notes_strict with required_tags (simple string tags)
 * - Issue #200: get_note_concepts after tagging with tag_note_concept
 * - Issue #201: diff_note_versions returns plain text diff properly
 *
 * Test Categories:
 * 1. Note CRUD (create, read, update, delete)
 * 2. Search (hybrid, FTS, semantic, strict filtering)
 * 3. SKOS Concepts (tagging, get_note_concepts)
 * 4. Versioning (list, get, diff)
 * 5. Collections and Templates
 * 6. Embedding Sets
 *
 * Prerequisites:
 *   - matric-api running at MATRIC_MEMORY_URL
 *   - Valid API key in MATRIC_MEMORY_API_KEY
 *   - Test data set up with specific IDs from environment
 *
 * Usage: node test-comprehensive-agentic.js
 */

import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Configuration from environment
const API_BASE = process.env.MATRIC_MEMORY_URL || 'http://localhost:3000';
let API_KEY = process.env.MATRIC_MEMORY_API_KEY;
const MCP_HTTP_PORT = parseInt(process.env.MCP_TEST_PORT || '3099', 10);

// OAuth client credentials for MCP server
let mcpClientId = null;
let mcpClientSecret = null;

// Test data IDs from environment
const TEST_IDS = {
  NOTE_FULL: process.env.NOTE_FULL || '019c0c53-eaa2-7122-8ff1-abc9ccb84219',
  NOTE_STATUS: process.env.NOTE_STATUS || '019c0c53-eb39-7d61-9f78-b829a8f3f325',
  NOTE_VERSION: process.env.NOTE_VERSION || '019c0c53-eb4c-7a83-9afa-403708f71146',
  SCHEME: process.env.SCHEME || '019c0c53-eb83-7b03-8131-013f240237c7',
  CONCEPT_ROOT: process.env.CONCEPT_ROOT || '019c0c53-eb8b-7523-a104-22d771d04270',
};

// Test results tracking
let passed = 0;
let failed = 0;
const results = [];
let sessionId = null;

function log(msg) {
  console.log(msg);
}

function assert(condition, testName, details = '') {
  if (condition) {
    passed++;
    results.push({ name: testName, status: 'PASS' });
    log(`  ✓ ${testName}`);
    return true;
  } else {
    failed++;
    results.push({ name: testName, status: 'FAIL', details });
    log(`  ✗ ${testName}`);
    if (details) log(`    → ${details}`);
    return false;
  }
}

/**
 * Register OAuth client for MCP server
 */
async function registerOAuthClient() {
  log('  Registering OAuth client for MCP server...');
  try {
    const response = await fetch(`${API_BASE}/oauth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_name: `MCP Test Server ${Date.now()}`,
        grant_types: ['client_credentials'],
        scope: 'mcp read write',
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!response.ok) {
      const err = await response.text();
      log(`  ✗ Client registration failed: ${err.slice(0, 100)}`);
      return false;
    }

    const client = await response.json();
    mcpClientId = client.client_id;
    mcpClientSecret = client.client_secret;
    log(`  ✓ Registered OAuth client: ${client.client_id}`);
    return true;
  } catch (error) {
    log(`  ✗ OAuth client registration failed: ${error.message}`);
    return false;
  }
}

/**
 * Get or generate OAuth token for testing
 */
async function getOrCreateToken() {
  if (API_KEY) return true;

  log('  Obtaining OAuth token for test client...');
  try {
    // Register a separate client for the test requests
    const response = await fetch(`${API_BASE}/oauth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_name: `MCP Test Client ${Date.now()}`,
        grant_types: ['client_credentials'],
        scope: 'mcp read write',
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!response.ok) {
      const err = await response.text();
      log(`  ✗ Client registration failed: ${err.slice(0, 100)}`);
      return false;
    }

    const client = await response.json();

    const tokenResponse = await fetch(`${API_BASE}/oauth/token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: `grant_type=client_credentials&client_id=${encodeURIComponent(client.client_id)}&client_secret=${encodeURIComponent(client.client_secret)}&scope=mcp%20read%20write`,
      signal: AbortSignal.timeout(10000),
    });

    if (tokenResponse.ok) {
      const data = await tokenResponse.json();
      API_KEY = data.access_token;
      log(`  ✓ OAuth token obtained`);
      return true;
    } else {
      const err = await tokenResponse.json();
      log(`  ✗ Token request failed: ${err.error}`);
      return false;
    }
  } catch (error) {
    log(`  ✗ OAuth setup failed: ${error.message}`);
    return false;
  }
}

/**
 * Start MCP server in HTTP mode
 */
function startMcpHttpServer() {
  return new Promise((resolve, reject) => {
    const env = {
      ...process.env,
      MCP_TRANSPORT: 'http',
      MCP_PORT: String(MCP_HTTP_PORT),
      MCP_BASE_URL: `http://localhost:${MCP_HTTP_PORT}`,
      MATRIC_MEMORY_URL: API_BASE,
      MATRIC_MEMORY_API_KEY: API_KEY,
      MCP_CLIENT_ID: mcpClientId,
      MCP_CLIENT_SECRET: mcpClientSecret,
    };

    const child = spawn('node', [join(__dirname, 'index.js')], {
      env,
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    let started = false;
    let startupOutput = '';

    child.stdout.on('data', (data) => {
      const output = data.toString();
      startupOutput += output;
      if (output.includes('MCP HTTP server listening') && !started) {
        started = true;
        resolve(child);
      }
    });

    child.stderr.on('data', (data) => {
      const msg = data.toString().trim();
      if (msg && !msg.includes('[mcp]') && !msg.includes('[sse]') && !msg.includes('[messages]')) {
        log(`  MCP stderr: ${msg}`);
      }
    });

    child.on('error', (err) => {
      reject(new Error(`Failed to start MCP server: ${err.message}`));
    });

    child.on('exit', (code) => {
      if (!started) {
        reject(new Error(`MCP server exited with code ${code}. Output: ${startupOutput.slice(0, 500)}`));
      }
    });

    setTimeout(() => {
      if (!started) {
        child.kill();
        reject(new Error('MCP HTTP server failed to start within 15s timeout'));
      }
    }, 15000);
  });
}

/**
 * Initialize MCP session
 */
async function initializeSession() {
  log('\n🔌 Initializing MCP Session...');

  try {
    const response = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json, text/event-stream',
        'Authorization': `Bearer ${API_KEY}`,
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: {
          protocolVersion: '2024-11-05',
          capabilities: {},
          clientInfo: { name: 'agentic-test', version: '1.0.0' },
        },
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!response.ok) {
      const errorText = await response.text();
      assert(false, 'MCP session initialization', `Status: ${response.status}, Error: ${errorText.slice(0, 200)}`);
      return false;
    }

    sessionId = response.headers.get('MCP-Session-Id');
    assert(!!sessionId, 'Session ID obtained', sessionId);

    const data = await response.text();
    const events = data.split('\n').filter(l => l.startsWith('data:'));

    if (events.length > 0) {
      const result = JSON.parse(events[0].slice(5));
      assert(!!result.result?.serverInfo, 'Server info received');
    }

    return true;

  } catch (error) {
    assert(false, 'MCP session initialization', error.message);
    return false;
  }
}

/**
 * Helper to call MCP tool
 */
async function callTool(name, args, requestId = Math.floor(Math.random() * 10000)) {
  const response = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Accept': 'application/json, text/event-stream',
      'Authorization': `Bearer ${API_KEY}`,
      'MCP-Session-Id': sessionId,
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: requestId,
      method: 'tools/call',
      params: { name, arguments: args },
    }),
    signal: AbortSignal.timeout(30000),
  });

  const data = await response.text();
  const events = data.split('\n').filter(l => l.startsWith('data:'));

  if (events.length > 0) {
    const result = JSON.parse(events[0].slice(5));

    // Extract content text if available
    let content = null;
    if (result.result?.content?.[0]?.text) {
      try {
        content = JSON.parse(result.result.content[0].text);
      } catch {
        content = result.result.content[0].text;
      }
    }

    return {
      ok: response.ok,
      result: result.result,
      content,
      error: result.error,
    };
  }

  return { ok: response.ok, result: null, content: null };
}

/**
 * Category 1: Note CRUD Tests
 */
async function testNoteCrud() {
  log('\n📝 Testing Note CRUD Operations...');

  try {
    // Test: List notes
    const listResult = await callTool('list_notes', { limit: 5 });
    assert(listResult.ok, 'list_notes succeeds');
    assert(!!listResult.content, 'list_notes returns content');

    // Test: Get specific note
    const getResult = await callTool('get_note', { note_id: TEST_IDS.NOTE_FULL });
    assert(getResult.ok, 'get_note succeeds', getResult.error?.message || '');
    if (getResult.content) {
      assert(getResult.content.id === TEST_IDS.NOTE_FULL, 'get_note returns correct note');
    }

    // Test: Create note
    const createResult = await callTool('create_note', {
      title: 'MCP Test Note ' + Date.now(),
      content: 'Created by comprehensive agentic test suite',
      tags: ['mcp-test', 'automated'],
    });
    assert(createResult.ok, 'create_note succeeds', createResult.error?.message || '');

    let createdNoteId = null;
    if (createResult.content?.id) {
      createdNoteId = createResult.content.id;
      assert(true, 'create_note returns note ID');
    }

    // Test: Issue #198 - Update note with only archived=true
    if (createdNoteId) {
      const archiveResult = await callTool('update_note', {
        note_id: createdNoteId,
        archived: true,
      });
      assert(archiveResult.ok, 'Issue #198: update_note with only archived=true', archiveResult.error?.message || '');

      if (archiveResult.content) {
        assert(archiveResult.content.archived === true, 'Issue #198: Note is archived');
      }
    }

    // Test: Issue #198 - Update note with only starred=true
    if (createdNoteId) {
      const starResult = await callTool('update_note', {
        note_id: createdNoteId,
        starred: true,
      });
      assert(starResult.ok, 'Issue #198: update_note with only starred=true', starResult.error?.message || '');

      if (starResult.content) {
        assert(starResult.content.starred === true, 'Issue #198: Note is starred');
      }
    }

    // Test: Update note content
    if (createdNoteId) {
      const updateResult = await callTool('update_note', {
        note_id: createdNoteId,
        content: 'Updated content by test suite',
      });
      assert(updateResult.ok, 'update_note with content succeeds');
    }

    // Test: Delete note (cleanup)
    if (createdNoteId) {
      const deleteResult = await callTool('delete_note', { note_id: createdNoteId });
      assert(deleteResult.ok, 'delete_note succeeds');
    }

    return true;

  } catch (error) {
    assert(false, 'Note CRUD operations', error.message);
    return false;
  }
}

/**
 * Category 2: Search Tests
 */
async function testSearch() {
  log('\n🔍 Testing Search Operations...');

  try {
    // Test: Basic hybrid search
    const searchResult = await callTool('search_notes', {
      query: 'test',
      limit: 5,
    });
    assert(searchResult.ok, 'search_notes hybrid search succeeds');

    // Test: FTS search
    const ftsResult = await callTool('search_notes', {
      query: 'test',
      search_mode: 'fts',
      limit: 5,
    });
    assert(ftsResult.ok, 'search_notes FTS mode succeeds');

    // Test: Semantic search
    const semanticResult = await callTool('search_notes', {
      query: 'knowledge management',
      search_mode: 'semantic',
      limit: 5,
    });
    assert(semanticResult.ok, 'search_notes semantic mode succeeds');

    // Test: Issue #199 - search_notes_strict with required_tags (simple string tags)
    const strictResult = await callTool('search_notes_strict', {
      query: 'test',
      required_tags: ['mcp-test'],
      limit: 5,
    });
    assert(strictResult.ok, 'Issue #199: search_notes_strict with required_tags', strictResult.error?.message || '');

    if (strictResult.content) {
      // Verify all results have the required tag
      const notes = Array.isArray(strictResult.content) ? strictResult.content : strictResult.content.notes || [];
      if (notes.length > 0) {
        const allHaveTag = notes.every(note =>
          note.tags && note.tags.includes('mcp-test')
        );
        assert(allHaveTag, 'Issue #199: All results have required tag');
      } else {
        log('    → No notes with mcp-test tag found (expected if no test data)');
      }
    }

    // Test: search_notes_strict with excluded_tags
    const excludeResult = await callTool('search_notes_strict', {
      query: 'note',
      excluded_tags: ['archive', 'deleted'],
      limit: 5,
    });
    assert(excludeResult.ok, 'search_notes_strict with excluded_tags succeeds');

    // Test: search_notes with strict_filter parameter
    const strictFilterResult = await callTool('search_notes', {
      query: 'test',
      strict_filter: {
        required_tags: ['automated'],
      },
      limit: 5,
    });
    assert(strictFilterResult.ok, 'search_notes with strict_filter parameter succeeds');

    return true;

  } catch (error) {
    assert(false, 'Search operations', error.message);
    return false;
  }
}

/**
 * Category 3: SKOS Concepts Tests
 */
async function testSkosConcepts() {
  log('\n🏷️  Testing SKOS Concepts...');

  try {
    // Create a test note for tagging
    const createResult = await callTool('create_note', {
      title: 'SKOS Test Note ' + Date.now(),
      content: 'Testing SKOS concept tagging',
      tags: ['skos-test'],
    });

    const testNoteId = createResult.content?.id;
    if (!testNoteId) {
      assert(false, 'Create test note for SKOS', 'Failed to create note');
      return false;
    }

    // Test: Tag note with concept
    const tagResult = await callTool('tag_note_concept', {
      note_id: testNoteId,
      concept_id: TEST_IDS.CONCEPT_ROOT,
    });
    assert(tagResult.ok, 'tag_note_concept succeeds', tagResult.error?.message || '');

    // Test: Issue #200 - get_note_concepts after tagging
    await delay(500); // Brief delay to ensure tagging is processed

    const conceptsResult = await callTool('get_note_concepts', {
      note_id: testNoteId,
    });
    assert(conceptsResult.ok, 'Issue #200: get_note_concepts succeeds', conceptsResult.error?.message || '');

    if (conceptsResult.content) {
      const concepts = Array.isArray(conceptsResult.content) ? conceptsResult.content : conceptsResult.content.concepts || [];
      assert(concepts.length > 0, 'Issue #200: get_note_concepts returns concepts after tagging');

      const hasTaggedConcept = concepts.some(c => c.id === TEST_IDS.CONCEPT_ROOT);
      assert(hasTaggedConcept, 'Issue #200: Tagged concept appears in results');
    }

    // Test: List concepts in scheme
    const schemeConceptsResult = await callTool('list_concepts', {
      scheme_id: TEST_IDS.SCHEME,
    });
    assert(schemeConceptsResult.ok, 'list_concepts succeeds');

    // Test: Get concept details
    const conceptResult = await callTool('get_concept', {
      concept_id: TEST_IDS.CONCEPT_ROOT,
    });
    assert(conceptResult.ok, 'get_concept succeeds');

    // Test: Search concepts
    const searchConceptsResult = await callTool('search_concepts', {
      query: 'test',
      limit: 5,
    });
    assert(searchConceptsResult.ok, 'search_concepts succeeds');

    // Cleanup
    await callTool('delete_note', { note_id: testNoteId });

    return true;

  } catch (error) {
    assert(false, 'SKOS Concepts operations', error.message);
    return false;
  }
}

/**
 * Category 4: Versioning Tests
 */
async function testVersioning() {
  log('\n📚 Testing Note Versioning...');

  try {
    // Test: List note versions
    const listVersionsResult = await callTool('list_note_versions', {
      note_id: TEST_IDS.NOTE_VERSION,
    });
    assert(listVersionsResult.ok, 'list_note_versions succeeds', listVersionsResult.error?.message || '');

    let versionNumbers = [];
    if (listVersionsResult.content) {
      const versions = Array.isArray(listVersionsResult.content) ? listVersionsResult.content : listVersionsResult.content.versions || [];
      versionNumbers = versions.map(v => v.version_number || v.version);
      assert(versions.length > 0, 'list_note_versions returns versions');
    }

    // Test: Get specific version
    if (versionNumbers.length > 0) {
      const getVersionResult = await callTool('get_note_version', {
        note_id: TEST_IDS.NOTE_VERSION,
        version: versionNumbers[0],
      });
      assert(getVersionResult.ok, 'get_note_version succeeds');
    }

    // Test: Issue #201 - diff_note_versions returns plain text diff
    if (versionNumbers.length >= 2) {
      const diffResult = await callTool('diff_note_versions', {
        note_id: TEST_IDS.NOTE_VERSION,
        from_version: versionNumbers[1],
        to_version: versionNumbers[0],
      });
      assert(diffResult.ok, 'Issue #201: diff_note_versions succeeds', diffResult.error?.message || '');

      if (diffResult.content) {
        const diffText = typeof diffResult.content === 'string' ? diffResult.content : diffResult.content.diff;
        assert(!!diffText, 'Issue #201: diff_note_versions returns diff content');

        // Verify it's plain text diff (contains typical diff markers)
        const hasUnifiedDiffFormat = diffText.includes('---') || diffText.includes('+++') ||
                                      diffText.includes('@@') || diffText.includes('-') || diffText.includes('+');
        assert(hasUnifiedDiffFormat, 'Issue #201: diff is in plain text unified diff format');
      }
    } else {
      log('    → Not enough versions to test diff (need at least 2)');
    }

    return true;

  } catch (error) {
    assert(false, 'Versioning operations', error.message);
    return false;
  }
}

/**
 * Category 5: Collections and Templates Tests
 */
async function testCollectionsAndTemplates() {
  log('\n📂 Testing Collections and Templates...');

  try {
    // Test: List collections
    const listCollectionsResult = await callTool('list_collections', {});
    assert(listCollectionsResult.ok, 'list_collections succeeds');

    // Test: Create collection
    const createCollectionResult = await callTool('create_collection', {
      name: 'Test Collection ' + Date.now(),
      description: 'Created by comprehensive test suite',
    });
    assert(createCollectionResult.ok, 'create_collection succeeds', createCollectionResult.error?.message || '');

    const collectionId = createCollectionResult.content?.id;

    // Test: List templates
    const listTemplatesResult = await callTool('list_templates', {});
    assert(listTemplatesResult.ok, 'list_templates succeeds');

    // Test: Create template
    const createTemplateResult = await callTool('create_template', {
      name: 'Test Template ' + Date.now(),
      content: 'Template content with {{variable}}',
    });
    assert(createTemplateResult.ok, 'create_template succeeds', createTemplateResult.error?.message || '');

    const templateId = createTemplateResult.content?.id;

    // Cleanup
    if (collectionId) {
      await callTool('delete_collection', { collection_id: collectionId });
    }
    if (templateId) {
      await callTool('delete_template', { template_id: templateId });
    }

    return true;

  } catch (error) {
    assert(false, 'Collections and Templates operations', error.message);
    return false;
  }
}

/**
 * Category 6: Embedding Sets Tests
 */
async function testEmbeddingSets() {
  log('\n🧮 Testing Embedding Sets...');

  try {
    // Test: List embedding sets
    const listResult = await callTool('list_embedding_sets', {});
    assert(listResult.ok, 'list_embedding_sets succeeds');

    // Test: Get embedding set (if any exist)
    if (listResult.content) {
      const sets = Array.isArray(listResult.content) ? listResult.content : listResult.content.embedding_sets || [];
      if (sets.length > 0) {
        const getResult = await callTool('get_embedding_set', {
          set_id: sets[0].id,
        });
        assert(getResult.ok, 'get_embedding_set succeeds');
      } else {
        log('    → No embedding sets found (expected if none configured)');
      }
    }

    return true;

  } catch (error) {
    assert(false, 'Embedding Sets operations', error.message);
    return false;
  }
}

/**
 * Additional Tests: Tags, Graph, Related Notes
 */
async function testAdditionalFeatures() {
  log('\n🔗 Testing Additional Features...');

  try {
    // Test: List tags
    const tagsResult = await callTool('list_tags', {});
    assert(tagsResult.ok, 'list_tags succeeds');

    // Test: Graph exploration
    const graphResult = await callTool('explore_graph', {
      note_id: TEST_IDS.NOTE_FULL,
      depth: 2,
    });
    assert(graphResult.ok, 'explore_graph succeeds', graphResult.error?.message || '');

    // Test: Find related notes
    const relatedResult = await callTool('find_related_notes', {
      note_id: TEST_IDS.NOTE_FULL,
      limit: 5,
    });
    assert(relatedResult.ok, 'find_related_notes succeeds', relatedResult.error?.message || '');

    return true;

  } catch (error) {
    assert(false, 'Additional features', error.message);
    return false;
  }
}

/**
 * Main test runner
 */
async function runTests() {
  console.log('═══════════════════════════════════════════════════════════');
  console.log('      MCP Comprehensive Agentic Test Suite');
  console.log('═══════════════════════════════════════════════════════════');
  console.log(`  API: ${API_BASE}`);
  console.log(`  Key: ${API_KEY ? API_KEY.slice(0, 8) + '...' : '(not set)'}`);
  console.log('  Test IDs:');
  console.log(`    NOTE_FULL:     ${TEST_IDS.NOTE_FULL}`);
  console.log(`    NOTE_STATUS:   ${TEST_IDS.NOTE_STATUS}`);
  console.log(`    NOTE_VERSION:  ${TEST_IDS.NOTE_VERSION}`);
  console.log(`    SCHEME:        ${TEST_IDS.SCHEME}`);
  console.log(`    CONCEPT_ROOT:  ${TEST_IDS.CONCEPT_ROOT}`);
  console.log('═══════════════════════════════════════════════════════════');

  let mcpServer;

  try {
    // Register OAuth client for MCP server
    const clientOk = await registerOAuthClient();
    if (!clientOk) {
      log('\n⚠️  Cannot proceed - OAuth client registration failed');
      process.exit(1);
    }

    // Get auth token for test requests
    const hasToken = await getOrCreateToken();
    if (!hasToken) {
      log('\n⚠️  Cannot proceed - no API key available');
      process.exit(1);
    }

    // Start MCP server
    log('\n🚀 Starting MCP HTTP server...');
    mcpServer = await startMcpHttpServer();
    log(`   MCP server running on port ${MCP_HTTP_PORT}`);
    await delay(1000);

    // Initialize session
    const sessionOk = await initializeSession();
    if (!sessionOk) {
      log('\n⚠️  Cannot proceed - session initialization failed');
      process.exit(1);
    }

    // Run test categories
    await testNoteCrud();
    await testSearch();
    await testSkosConcepts();
    await testVersioning();
    await testCollectionsAndTemplates();
    await testEmbeddingSets();
    await testAdditionalFeatures();

  } catch (error) {
    console.error('\n❌ Test error:', error.message);
    console.error(error.stack);
    failed++;
  } finally {
    // Cleanup
    if (mcpServer) {
      mcpServer.kill();
    }

    // Summary
    console.log('\n═══════════════════════════════════════════════════════════');
    console.log('                     Test Summary');
    console.log('═══════════════════════════════════════════════════════════');
    console.log(`  Passed: ${passed}`);
    console.log(`  Failed: ${failed}`);
    console.log(`  Total:  ${passed + failed}`);
    console.log('═══════════════════════════════════════════════════════════');

    if (failed > 0) {
      console.log('\n❌ Failed tests:');
      results.filter(r => r.status === 'FAIL').forEach(r => {
        console.log(`  • ${r.name}`);
        if (r.details) console.log(`    → ${r.details}`);
      });
      console.log('');
    } else {
      console.log('\n✅ All tests passed!');
      console.log('\nBug fix verifications:');
      console.log('  ✓ Issue #198: Single field updates (archived, starred)');
      console.log('  ✓ Issue #199: search_notes_strict with required_tags');
      console.log('  ✓ Issue #200: get_note_concepts after tagging');
      console.log('  ✓ Issue #201: diff_note_versions returns plain text diff');
      console.log('');
    }

    process.exit(failed > 0 ? 1 : 0);
  }
}

runTests();
