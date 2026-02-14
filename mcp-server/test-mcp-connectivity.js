#!/usr/bin/env node
/**
 * MCP Connectivity Integration Tests
 *
 * End-to-end tests against the real matric-api.
 * Validates full MCP connectivity including:
 * - API reachability
 * - HTTP transport (StreamableHTTP + SSE)
 * - Stdio transport
 * - Token validation
 * - Tool execution
 *
 * Prerequisites:
 *   - matric-api running at MATRIC_MEMORY_URL (default: http://localhost:3000)
 *   - Valid API key in MATRIC_MEMORY_API_KEY
 *
 * Run: node test-mcp-connectivity.js
 * Run with custom API: MATRIC_MEMORY_URL=http://localhost:3000 node test-mcp-connectivity.js
 */

import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Configuration from environment
const API_BASE = process.env.MATRIC_MEMORY_URL || 'http://localhost:3000';
let API_KEY = process.env.MATRIC_MEMORY_API_KEY;
const MCP_HTTP_PORT = parseInt(process.env.MCP_TEST_PORT || '3098', 10);

// OAuth client credentials (for automatic token generation if no API_KEY)
const OAUTH_CLIENT_ID = process.env.MCP_CLIENT_ID;
const OAUTH_CLIENT_SECRET = process.env.MCP_CLIENT_SECRET;

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
 * Get or generate OAuth token for testing
 */
async function getOrCreateToken() {
  // If API_KEY is already set, use it
  if (API_KEY) return true;

  // If OAuth credentials are set, use client_credentials flow
  if (OAUTH_CLIENT_ID && OAUTH_CLIENT_SECRET) {
    log('  Obtaining OAuth token via client_credentials...');
    try {
      const response = await fetch(`${API_BASE}/oauth/token`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: `grant_type=client_credentials&client_id=${encodeURIComponent(OAUTH_CLIENT_ID)}&client_secret=${encodeURIComponent(OAUTH_CLIENT_SECRET)}&scope=mcp%20read%20write`,
        signal: AbortSignal.timeout(10000),
      });

      if (response.ok) {
        const data = await response.json();
        API_KEY = data.access_token;
        log(`  ✓ OAuth token obtained (expires in ${data.expires_in}s)`);
        return true;
      } else {
        const err = await response.json();
        log(`  ✗ OAuth token failed: ${err.error} - ${err.error_description || ''}`);
        return false;
      }
    } catch (error) {
      log(`  ✗ OAuth token request failed: ${error.message}`);
      return false;
    }
  }

  // Try dynamic client registration as last resort
  log('  Attempting dynamic client registration...');
  try {
    const regResponse = await fetch(`${API_BASE}/oauth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_name: `MCP Test ${Date.now()}`,
        grant_types: ['client_credentials'],
        scope: 'mcp read write',
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!regResponse.ok) {
      const err = await regResponse.text();
      log(`  ✗ Client registration failed: ${err.slice(0, 100)}`);
      return false;
    }

    const client = await regResponse.json();
    log(`  ✓ Registered client: ${client.client_id}`);

    // Wait a moment to avoid rate limiting
    await delay(1000);

    // Get token
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
    log(`  ✗ Dynamic registration failed: ${error.message}`);
    return false;
  }
}

/**
 * Test API reachability directly
 */
async function testApiReachability() {
  log('\n📡 Testing API Reachability...');

  try {
    // Test health endpoint
    const healthResponse = await fetch(`${API_BASE}/health`, {
      signal: AbortSignal.timeout(10000),
    });
    assert(healthResponse.ok, 'API health endpoint reachable', `Status: ${healthResponse.status}`);

    // Get or create token
    const hasToken = await getOrCreateToken();
    if (!hasToken) {
      assert(false, 'API key or OAuth token available', 'Set MATRIC_MEMORY_API_KEY or MCP_CLIENT_ID/MCP_CLIENT_SECRET');
      return false;
    }

    const authResponse = await fetch(`${API_BASE}/api/v1/notes?limit=1`, {
      headers: { 'Authorization': `Bearer ${API_KEY}` },
      signal: AbortSignal.timeout(10000),
    });

    if (!assert(authResponse.ok, 'API accepts authentication', `Status: ${authResponse.status}`)) {
      const error = await authResponse.text();
      log(`    API error: ${error.slice(0, 200)}`);
      return false;
    }

    return true;

  } catch (error) {
    assert(false, 'API reachable', error.message);
    return false;
  }
}

/**
 * Start MCP server in HTTP mode
 */
function startMcpHttpServer(mcpClientId, mcpClientSecret) {
  return new Promise((resolve, reject) => {
    const env = {
      ...process.env,
      MCP_TRANSPORT: 'http',
      MCP_PORT: String(MCP_HTTP_PORT),
      MCP_BASE_URL: `http://localhost:${MCP_HTTP_PORT}`,
      MATRIC_MEMORY_URL: API_BASE,
      MATRIC_MEMORY_API_KEY: API_KEY,
      // OAuth client credentials for token introspection
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
        reject(new Error(`MCP server exited with code ${code} before starting. Output: ${startupOutput}`));
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
 * Test MCP HTTP server health
 */
async function testMcpHealth() {
  log('\n🏥 Testing MCP Server Health...');

  try {
    const response = await fetch(`http://localhost:${MCP_HTTP_PORT}/health`, {
      signal: AbortSignal.timeout(5000),
    });

    const data = await response.json();
    assert(response.ok, 'MCP health endpoint responds');
    assert(data.status === 'ok', 'MCP status is ok');
    assert(data.transport === 'http', 'MCP transport is http');
    return true;

  } catch (error) {
    assert(false, 'MCP health check', error.message);
    return false;
  }
}

/**
 * Test MCP StreamableHTTP transport with real API
 */
async function testStreamableHttpTransport() {
  log('\n🌊 Testing StreamableHTTP Transport...');

  try {
    // Initialize MCP session
    const initRequest = {
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2024-11-05',
        capabilities: {},
        clientInfo: { name: 'integration-test', version: '1.0.0' },
      },
    };

    const initResponse = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json, text/event-stream',
        'Authorization': `Bearer ${API_KEY}`,
      },
      body: JSON.stringify(initRequest),
      signal: AbortSignal.timeout(10000),
    });

    if (!assert(initResponse.ok, 'MCP initialize succeeds', `Status: ${initResponse.status}`)) {
      const err = await initResponse.text();
      log(`    Error: ${err.slice(0, 200)}`);
      return false;
    }

    const sessionId = initResponse.headers.get('MCP-Session-Id');
    assert(!!sessionId, 'Session ID returned');

    // Parse SSE response
    const initData = await initResponse.text();
    const events = initData.split('\n').filter(l => l.startsWith('data:'));

    if (events.length > 0) {
      try {
        const firstEvent = JSON.parse(events[0].slice(5));
        assert(!!firstEvent.result?.serverInfo, 'Server info in response');
        assert(!!firstEvent.result?.protocolVersion, 'Protocol version in response');
      } catch (e) {
        assert(false, 'Parse initialize response', e.message);
      }
    }

    // List tools
    const toolsResponse = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json, text/event-stream',
        'Authorization': `Bearer ${API_KEY}`,
        'MCP-Session-Id': sessionId,
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'tools/list',
      }),
      signal: AbortSignal.timeout(10000),
    });

    assert(toolsResponse.ok, 'List tools succeeds');

    const toolsData = await toolsResponse.text();
    const toolsEvents = toolsData.split('\n').filter(l => l.startsWith('data:'));

    if (toolsEvents.length > 0) {
      const toolsResult = JSON.parse(toolsEvents[0].slice(5));
      const tools = toolsResult.result?.tools || [];
      assert(Array.isArray(tools), 'Tools is array');
      assert(tools.length > 0, `Tools available (${tools.length} tools)`);

      // Check essential tools
      const toolNames = tools.map(t => t.name);
      assert(toolNames.includes('list_notes'), 'list_notes tool present');
      assert(toolNames.includes('search_notes'), 'search_notes tool present');
      assert(toolNames.includes('create_note'), 'create_note tool present');
      assert(toolNames.includes('get_note'), 'get_note tool present');
    }

    // Clean up session
    await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'DELETE',
      headers: {
        'Authorization': `Bearer ${API_KEY}`,
        'MCP-Session-Id': sessionId,
      },
    });

    return true;

  } catch (error) {
    assert(false, 'StreamableHTTP transport', error.message);
    return false;
  }
}

/**
 * Test tool execution against real API
 */
async function testToolExecution() {
  log('\n🔧 Testing Tool Execution (Real API)...');

  try {
    // Initialize session
    const initResponse = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
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
          clientInfo: { name: 'integration-test', version: '1.0.0' },
        },
      }),
      signal: AbortSignal.timeout(10000),
    });

    const sessionId = initResponse.headers.get('MCP-Session-Id');
    await initResponse.text();

    // Helper to call tool and parse response
    async function callTool(name, args, requestId) {
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
        return { ok: response.ok, result };
      }

      return { ok: response.ok, result: null };
    }

    // Test list_notes
    const listResult = await callTool('list_notes', { limit: 5 }, 2);
    if (assert(listResult.ok, 'list_notes call succeeds')) {
      const content = listResult.result?.result?.content?.[0]?.text;
      if (content) {
        try {
          const parsed = JSON.parse(content);
          assert('notes' in parsed || Array.isArray(parsed), 'list_notes returns notes data');
        } catch (e) {
          assert(content.includes('notes') || content.includes('[]'), 'list_notes returns valid response');
        }
      }
    }

    // Test list_tags
    const tagsResult = await callTool('list_tags', {}, 3);
    assert(tagsResult.ok, 'list_tags call succeeds');

    // Test search_notes
    const searchResult = await callTool('search_notes', { query: 'test', limit: 3 }, 4);
    if (assert(searchResult.ok, 'search_notes call succeeds')) {
      const content = searchResult.result?.result?.content?.[0]?.text;
      if (content) {
        assert(content.length > 0, 'search_notes returns content');
      }
    }

    // Clean up session
    await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'DELETE',
      headers: {
        'Authorization': `Bearer ${API_KEY}`,
        'MCP-Session-Id': sessionId,
      },
    });

    return true;

  } catch (error) {
    assert(false, 'Tool execution', error.message);
    return false;
  }
}

/**
 * Test SSE transport endpoint
 */
async function testSseTransport() {
  log('\n📺 Testing SSE Transport...');

  try {
    // Test auth required
    const noAuthResponse = await fetch(`http://localhost:${MCP_HTTP_PORT}/sse`, {
      signal: AbortSignal.timeout(5000),
    });
    assert(noAuthResponse.status === 401, 'SSE requires authentication');

    // Test SSE connection opens
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 3000);

    try {
      const sseResponse = await fetch(`http://localhost:${MCP_HTTP_PORT}/sse`, {
        headers: { 'Authorization': `Bearer ${API_KEY}` },
        signal: controller.signal,
      });

      assert(sseResponse.ok, 'SSE endpoint accepts token');
      const contentType = sseResponse.headers.get('content-type');
      assert(contentType?.includes('text/event-stream'), 'SSE returns event-stream');

    } catch (error) {
      if (error.name === 'AbortError') {
        assert(true, 'SSE connection established');
      } else {
        throw error;
      }
    } finally {
      clearTimeout(timeout);
    }

    return true;

  } catch (error) {
    assert(false, 'SSE transport', error.message);
    return false;
  }
}

/**
 * Test stdio transport
 */
async function testStdioTransport() {
  log('\n📟 Testing Stdio Transport...');

  return new Promise((resolve) => {
    const env = {
      ...process.env,
      MCP_TRANSPORT: 'stdio',
      MATRIC_MEMORY_URL: API_BASE,
      MATRIC_MEMORY_API_KEY: API_KEY,
    };

    const child = spawn('node', [join(__dirname, 'index.js')], {
      env,
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    let outputBuffer = '';
    let gotServerInfo = false;
    let gotTools = false;
    let gotToolResult = false;

    child.stdout.on('data', (data) => {
      outputBuffer += data.toString();

      // Parse line-delimited JSON responses
      const lines = outputBuffer.split('\n');
      outputBuffer = lines.pop(); // Keep incomplete line

      for (const line of lines) {
        if (!line.trim()) continue;

        try {
          const response = JSON.parse(line);

          if (response.result?.serverInfo) {
            gotServerInfo = true;
            assert(true, 'Stdio initialize returns serverInfo');
          }

          if (response.result?.tools) {
            gotTools = true;
            assert(Array.isArray(response.result.tools), 'Stdio tools/list returns array');
            assert(response.result.tools.length > 0, `Stdio has ${response.result.tools.length} tools`);
          }

          if (response.id === 3 && response.result?.content) {
            gotToolResult = true;
            assert(true, 'Stdio tool call returns content');
          }

        } catch (e) {
          // Not JSON - ignore
        }
      }

      // Complete test after getting tool result
      if (gotServerInfo && gotTools && gotToolResult) {
        child.kill();
        resolve(true);
      }
    });

    child.stderr.on('data', () => {
      // Ignore stderr
    });

    child.on('error', (error) => {
      assert(false, 'Stdio process starts', error.message);
      resolve(false);
    });

    // Send requests
    setTimeout(() => {
      // Initialize
      child.stdin.write(JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: {
          protocolVersion: '2024-11-05',
          capabilities: {},
          clientInfo: { name: 'stdio-test', version: '1.0.0' },
        },
      }) + '\n');
    }, 500);

    setTimeout(() => {
      // Initialized notification
      child.stdin.write(JSON.stringify({
        jsonrpc: '2.0',
        method: 'notifications/initialized',
      }) + '\n');
    }, 1000);

    setTimeout(() => {
      // List tools
      child.stdin.write(JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'tools/list',
      }) + '\n');
    }, 1500);

    setTimeout(() => {
      // Call tool
      child.stdin.write(JSON.stringify({
        jsonrpc: '2.0',
        id: 3,
        method: 'tools/call',
        params: { name: 'list_tags', arguments: {} },
      }) + '\n');
    }, 2000);

    // Timeout
    setTimeout(() => {
      if (!gotServerInfo) assert(false, 'Stdio returns serverInfo', 'Timeout');
      if (!gotTools) assert(false, 'Stdio returns tools', 'Timeout');
      if (!gotToolResult) assert(false, 'Stdio tool call works', 'Timeout');
      child.kill();
      resolve(gotServerInfo && gotTools && gotToolResult);
    }, 15000);
  });
}

/**
 * Test session isolation
 */
async function testSessionIsolation() {
  log('\n🔒 Testing Session Isolation...');

  try {
    // Create two sessions
    const init1 = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
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
          clientInfo: { name: 'client-1', version: '1.0.0' },
        },
      }),
    });

    const session1 = init1.headers.get('MCP-Session-Id');
    await init1.text();

    const init2 = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
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
          clientInfo: { name: 'client-2', version: '1.0.0' },
        },
      }),
    });

    const session2 = init2.headers.get('MCP-Session-Id');
    await init2.text();

    assert(session1 !== session2, 'Sessions have unique IDs');

    // Both can call tools
    const call1 = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json, text/event-stream',
        'Authorization': `Bearer ${API_KEY}`,
        'MCP-Session-Id': session1,
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'tools/list',
      }),
    });

    const call2 = await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'application/json, text/event-stream',
        'Authorization': `Bearer ${API_KEY}`,
        'MCP-Session-Id': session2,
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 2,
        method: 'tools/list',
      }),
    });

    assert(call1.ok && call2.ok, 'Both sessions can list tools');

    // Clean up
    await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${API_KEY}`, 'MCP-Session-Id': session1 },
    });
    await fetch(`http://localhost:${MCP_HTTP_PORT}/`, {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${API_KEY}`, 'MCP-Session-Id': session2 },
    });

    return true;

  } catch (error) {
    assert(false, 'Session isolation', error.message);
    return false;
  }
}

/**
 * Register OAuth client for MCP HTTP testing
 */
async function registerMcpClient() {
  // If credentials already provided, use them
  if (OAUTH_CLIENT_ID && OAUTH_CLIENT_SECRET) {
    return { client_id: OAUTH_CLIENT_ID, client_secret: OAUTH_CLIENT_SECRET };
  }

  log('  Registering OAuth client for MCP HTTP transport...');
  try {
    const response = await fetch(`${API_BASE}/oauth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client_name: `MCP Test HTTP ${Date.now()}`,
        grant_types: ['client_credentials'],
        scope: 'mcp read write',
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!response.ok) {
      const err = await response.text();
      log(`  ✗ Client registration failed: ${err.slice(0, 100)}`);
      return null;
    }

    const client = await response.json();
    log(`  ✓ Registered OAuth client: ${client.client_id}`);
    return { client_id: client.client_id, client_secret: client.client_secret };
  } catch (error) {
    log(`  ✗ OAuth client registration failed: ${error.message}`);
    return null;
  }
}

/**
 * Main test runner
 */
async function runTests() {
  console.log('═══════════════════════════════════════════════════════════');
  console.log('       MCP Connectivity Integration Tests');
  console.log('═══════════════════════════════════════════════════════════');
  console.log(`  API: ${API_BASE}`);
  console.log(`  Key: ${API_KEY ? API_KEY.slice(0, 8) + '...' : '(not set)'}`);
  console.log('═══════════════════════════════════════════════════════════');

  let mcpServer;
  let mcpClient;

  try {
    // Test API first
    const apiOk = await testApiReachability();
    if (!apiOk) {
      log('\n⚠️  Cannot proceed - API not reachable or auth failed');
      log('   Ensure matric-api is running and MATRIC_MEMORY_API_KEY is set');
      process.exit(1);
    }

    // Register OAuth client for MCP HTTP transport
    mcpClient = await registerMcpClient();
    if (!mcpClient) {
      log('\n⚠️  Cannot register OAuth client - HTTP transport tests will fail');
      log('   Set MCP_CLIENT_ID and MCP_CLIENT_SECRET to use existing credentials');
    }

    // Start MCP HTTP server
    log('\n🚀 Starting MCP HTTP server...');
    mcpServer = await startMcpHttpServer(mcpClient?.client_id, mcpClient?.client_secret);
    log(`   MCP server running on port ${MCP_HTTP_PORT}`);
    await delay(1000);

    // Run tests
    await testMcpHealth();
    await testStreamableHttpTransport();
    await testSseTransport();
    await testToolExecution();
    await testSessionIsolation();
    await testStdioTransport();

  } catch (error) {
    console.error('\n❌ Test error:', error.message);
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
    }

    process.exit(failed > 0 ? 1 : 0);
  }
}

runTests();
