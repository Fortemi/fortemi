#!/usr/bin/env node
/**
 * PKE Keyset Management Tools Integration Tests
 *
 * Tests for PKE keyset management MCP tools:
 * - pke_list_keysets
 * - pke_create_keyset
 * - pke_get_active_keyset
 * - pke_set_active_keyset
 *
 * Prerequisites:
 *   - matric-pke CLI available in PATH
 *   - Write access to ~/.matric/keys/ directory
 *
 * Run: node test-pke-keyset-tools.js
 */

import { spawn, execSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdirSync, rmSync, existsSync, readdirSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Test configuration
const TEST_KEYS_DIR = join(homedir(), '.matric', 'keys');
const BACKUP_DIR = join(homedir(), '.matric', 'keys-backup-test');

// Test results tracking
let passed = 0;
let failed = 0;
const results = [];

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
 * Backup existing keys directory if it exists
 */
function backupKeysDir() {
  if (existsSync(TEST_KEYS_DIR)) {
    log('  Backing up existing keys directory...');
    if (existsSync(BACKUP_DIR)) {
      rmSync(BACKUP_DIR, { recursive: true, force: true });
    }
    mkdirSync(dirname(BACKUP_DIR), { recursive: true });
    // Copy directory using execSync
    try {
      execSync(`cp -r "${TEST_KEYS_DIR}" "${BACKUP_DIR}"`);
      log(`  ✓ Backed up to ${BACKUP_DIR}`);
    } catch (e) {
      log(`  ⚠ Backup failed: ${e.message}`);
    }
  }
}

/**
 * Restore keys directory from backup
 */
function restoreKeysDir() {
  if (existsSync(BACKUP_DIR)) {
    log('  Restoring keys directory from backup...');
    if (existsSync(TEST_KEYS_DIR)) {
      rmSync(TEST_KEYS_DIR, { recursive: true, force: true });
    }
    try {
      execSync(`cp -r "${BACKUP_DIR}" "${TEST_KEYS_DIR}"`);
      rmSync(BACKUP_DIR, { recursive: true, force: true });
      log('  ✓ Restored from backup');
    } catch (e) {
      log(`  ⚠ Restore failed: ${e.message}`);
    }
  }
}

/**
 * Start MCP server in stdio mode for testing
 */
function startMcpStdioServer() {
  const child = spawn('node', [join(__dirname, 'index.js')], {
    env: {
      ...process.env,
      MCP_TRANSPORT: 'stdio',
    },
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  let outputBuffer = '';
  let requestId = 1;

  const responseHandlers = new Map();

  child.stdout.on('data', (data) => {
    outputBuffer += data.toString();
    const lines = outputBuffer.split('\n');
    outputBuffer = lines.pop(); // Keep incomplete line

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response = JSON.parse(line);
        if (response.id && responseHandlers.has(response.id)) {
          const handler = responseHandlers.get(response.id);
          responseHandlers.delete(response.id);
          handler(null, response);
        }
      } catch (e) {
        // Not JSON - ignore
      }
    }
  });

  child.stderr.on('data', () => {
    // Ignore stderr
  });

  // Helper to call tool
  async function callTool(name, args) {
    return new Promise((resolve, reject) => {
      const id = requestId++;
      const timeout = setTimeout(() => {
        responseHandlers.delete(id);
        reject(new Error(`Timeout calling ${name}`));
      }, 10000);

      responseHandlers.set(id, (error, response) => {
        clearTimeout(timeout);
        if (error) {
          reject(error);
        } else {
          resolve(response);
        }
      });

      child.stdin.write(JSON.stringify({
        jsonrpc: '2.0',
        id,
        method: 'tools/call',
        params: { name, arguments: args },
      }) + '\n');
    });
  }

  // Initialize
  child.stdin.write(JSON.stringify({
    jsonrpc: '2.0',
    id: 0,
    method: 'initialize',
    params: {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'pke-keyset-test', version: '1.0.0' },
    },
  }) + '\n');

  child.stdin.write(JSON.stringify({
    jsonrpc: '2.0',
    method: 'notifications/initialized',
  }) + '\n');

  return { child, callTool };
}

/**
 * Test pke_list_keysets with empty directory
 */
async function testListKeysetsEmpty(callTool) {
  log('\n📋 Testing pke_list_keysets (empty directory)...');

  try {
    // Ensure directory doesn't exist
    if (existsSync(TEST_KEYS_DIR)) {
      rmSync(TEST_KEYS_DIR, { recursive: true, force: true });
    }

    const response = await callTool('pke_list_keysets', {});

    assert(response.result, 'Response has result');

    const content = response.result.content?.[0]?.text;
    assert(content !== undefined, 'Response has content');

    const data = JSON.parse(content);
    assert(Array.isArray(data), 'Returns array');
    assert(data.length === 0, 'Returns empty array when directory does not exist');

    return true;
  } catch (error) {
    assert(false, 'pke_list_keysets with empty directory', error.message);
    return false;
  }
}

/**
 * Test pke_create_keyset
 */
async function testCreateKeyset(callTool) {
  log('\n🔐 Testing pke_create_keyset...');

  try {
    const response = await callTool('pke_create_keyset', {
      name: 'test-keyset-1',
      passphrase: 'TestPassphrase123!',
    });

    assert(response.result, 'Response has result');

    const content = response.result.content?.[0]?.text;
    assert(content !== undefined, 'Response has content');

    const data = JSON.parse(content);
    assert(data.name === 'test-keyset-1', 'Keyset name matches');
    assert(data.address !== undefined, 'Keyset has address');
    assert(data.address.startsWith('mm:'), 'Address has mm: prefix');
    assert(data.public_key_path !== undefined, 'Has public key path');
    assert(data.private_key_path !== undefined, 'Has private key path');

    // Verify files exist
    const keysetDir = join(TEST_KEYS_DIR, 'test-keyset-1');
    assert(existsSync(keysetDir), 'Keyset directory created');
    assert(existsSync(data.public_key_path), 'Public key file created');
    assert(existsSync(data.private_key_path), 'Private key file created');

    return true;
  } catch (error) {
    assert(false, 'pke_create_keyset', error.message);
    return false;
  }
}

/**
 * Test pke_create_keyset with weak passphrase
 */
async function testCreateKeysetWeakPassphrase(callTool) {
  log('\n🔐 Testing pke_create_keyset (weak passphrase)...');

  try {
    const response = await callTool('pke_create_keyset', {
      name: 'test-weak',
      passphrase: 'weak',
    });

    const content = response.result.content?.[0]?.text;
    assert(content.includes('Error') || content.includes('error'), 'Rejects weak passphrase');

    return true;
  } catch (error) {
    // Expected to fail
    assert(true, 'pke_create_keyset rejects weak passphrase');
    return true;
  }
}

/**
 * Test pke_list_keysets with keysets
 */
async function testListKeysetsWithData(callTool) {
  log('\n📋 Testing pke_list_keysets (with keysets)...');

  try {
    // Create another keyset
    await callTool('pke_create_keyset', {
      name: 'test-keyset-2',
      passphrase: 'AnotherTestPass456!',
    });

    const response = await callTool('pke_list_keysets', {});

    const content = response.result.content?.[0]?.text;
    const data = JSON.parse(content);

    assert(Array.isArray(data), 'Returns array');
    assert(data.length === 2, `Returns two keysets (got ${data.length})`);

    const names = data.map(k => k.name).sort();
    assert(names[0] === 'test-keyset-1', 'First keyset present');
    assert(names[1] === 'test-keyset-2', 'Second keyset present');

    // Verify each keyset has required fields
    data.forEach(keyset => {
      assert(keyset.name !== undefined, `Keyset ${keyset.name} has name`);
      assert(keyset.address !== undefined, `Keyset ${keyset.name} has address`);
      assert(keyset.public_key_path !== undefined, `Keyset ${keyset.name} has public_key_path`);
      assert(keyset.private_key_path !== undefined, `Keyset ${keyset.name} has private_key_path`);
      assert(keyset.created !== undefined, `Keyset ${keyset.name} has created timestamp`);
    });

    return true;
  } catch (error) {
    assert(false, 'pke_list_keysets with data', error.message);
    return false;
  }
}

/**
 * Test pke_get_active_keyset (no active)
 */
async function testGetActiveKeysetNone(callTool) {
  log('\n📌 Testing pke_get_active_keyset (no active)...');

  try {
    const response = await callTool('pke_get_active_keyset', {});

    const content = response.result.content?.[0]?.text;
    const data = JSON.parse(content);

    assert(data === null, 'Returns null when no active keyset');

    return true;
  } catch (error) {
    assert(false, 'pke_get_active_keyset (no active)', error.message);
    return false;
  }
}

/**
 * Test pke_set_active_keyset
 */
async function testSetActiveKeyset(callTool) {
  log('\n📌 Testing pke_set_active_keyset...');

  try {
    const response = await callTool('pke_set_active_keyset', {
      name: 'test-keyset-1',
    });

    assert(response.result, 'Response has result');

    const content = response.result.content?.[0]?.text;
    const data = JSON.parse(content);

    assert(data.success === true, 'Returns success');
    assert(data.active_keyset === 'test-keyset-1', 'Sets correct keyset');

    // Verify active file exists and has correct content
    const activeFile = join(TEST_KEYS_DIR, 'active');
    assert(existsSync(activeFile), 'Active file created');

    const activeContent = readFileSync(activeFile, 'utf8').trim();
    assert(activeContent === 'test-keyset-1', 'Active file contains correct keyset name');

    return true;
  } catch (error) {
    assert(false, 'pke_set_active_keyset', error.message);
    return false;
  }
}

/**
 * Test pke_get_active_keyset (with active)
 */
async function testGetActiveKeysetWithActive(callTool) {
  log('\n📌 Testing pke_get_active_keyset (with active)...');

  try {
    const response = await callTool('pke_get_active_keyset', {});

    const content = response.result.content?.[0]?.text;
    const data = JSON.parse(content);

    assert(data !== null, 'Returns keyset data');
    assert(data.name === 'test-keyset-1', 'Returns correct keyset');
    assert(data.address !== undefined, 'Has address');
    assert(data.public_key_path !== undefined, 'Has public_key_path');
    assert(data.private_key_path !== undefined, 'Has private_key_path');

    return true;
  } catch (error) {
    assert(false, 'pke_get_active_keyset (with active)', error.message);
    return false;
  }
}

/**
 * Test pke_set_active_keyset with non-existent keyset
 */
async function testSetActiveKeysetInvalid(callTool) {
  log('\n📌 Testing pke_set_active_keyset (invalid keyset)...');

  try {
    const response = await callTool('pke_set_active_keyset', {
      name: 'non-existent-keyset',
    });

    const content = response.result.content?.[0]?.text;
    assert(content.includes('Error') || content.includes('error') || content.includes('not found'),
           'Returns error for non-existent keyset');

    return true;
  } catch (error) {
    // Expected to fail
    assert(true, 'pke_set_active_keyset rejects invalid keyset');
    return true;
  }
}

/**
 * Main test runner
 */
async function runTests() {
  console.log('═══════════════════════════════════════════════════════════');
  console.log('       PKE Keyset Management Tools Tests');
  console.log('═══════════════════════════════════════════════════════════');
  console.log(`  Keys Directory: ${TEST_KEYS_DIR}`);
  console.log('═══════════════════════════════════════════════════════════');

  let mcpServer;

  try {
    // Backup existing keys directory
    backupKeysDir();

    // Clean test directory
    if (existsSync(TEST_KEYS_DIR)) {
      rmSync(TEST_KEYS_DIR, { recursive: true, force: true });
    }

    // Start MCP server
    log('\n🚀 Starting MCP server in stdio mode...');
    const server = startMcpStdioServer();
    mcpServer = server.child;
    const { callTool } = server;

    // Wait for server to initialize
    await delay(1000);
    log('  ✓ MCP server started');

    // Run tests in order
    await testListKeysetsEmpty(callTool);
    await testCreateKeyset(callTool);
    await testCreateKeysetWeakPassphrase(callTool);
    await testListKeysetsWithData(callTool);
    await testGetActiveKeysetNone(callTool);
    await testSetActiveKeyset(callTool);
    await testGetActiveKeysetWithActive(callTool);
    await testSetActiveKeysetInvalid(callTool);

  } catch (error) {
    console.error('\n❌ Test error:', error.message);
    console.error(error.stack);
    failed++;
  } finally {
    // Cleanup
    if (mcpServer) {
      mcpServer.kill();
    }

    // Clean up test directory
    if (existsSync(TEST_KEYS_DIR)) {
      rmSync(TEST_KEYS_DIR, { recursive: true, force: true });
    }

    // Restore backup
    restoreKeysDir();

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
    }

    process.exit(failed > 0 ? 1 : 0);
  }
}

runTests();
