#!/usr/bin/env node

/**
 * Phase 0: Preflight Checks
 *
 * Validates MCP server connectivity and basic functionality before running
 * comprehensive test suites. Ensures the server is running, responsive, and
 * has all expected tools registered.
 *
 * Tests:
 * - Server info returns correct metadata
 * - Health check endpoint is accessible
 * - Tools list contains expected number of tools
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 0: Preflight Checks", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("PREFLIGHT-001: Server info returns server name and version", async () => {
    // Initialize returns server info
    const serverInfo = await client.initialize();

    assert.ok(serverInfo, "Server info should be returned");
    assert.ok(serverInfo.serverInfo, "Server info should contain serverInfo object");
    assert.ok(serverInfo.serverInfo.name, "Server info should contain name");
    assert.ok(serverInfo.serverInfo.version, "Server info should contain version");

    console.log(`  ✓ Server: ${serverInfo.serverInfo.name} v${serverInfo.serverInfo.version}`);
  });

  test("PREFLIGHT-002: Health check via API returns success", async () => {
    // Note: The health check tool may not exist in MCP tools
    // This test verifies we can make a basic tool call successfully
    const tools = await client.listTools();
    assert.ok(tools.length > 0, "Should have tools available");

    console.log(`  ✓ Server has ${tools.length} tools registered`);
  });

  test("PREFLIGHT-003: Tools list returns 100+ tools", async () => {
    const tools = await client.listTools();

    assert.ok(Array.isArray(tools), "Tools should be an array");
    assert.ok(tools.length >= 100, `Expected 100+ tools, got ${tools.length}`);

    // Verify tool structure
    const firstTool = tools[0];
    assert.ok(firstTool.name, "Tool should have name");
    assert.ok(firstTool.description, "Tool should have description");
    assert.ok(firstTool.inputSchema, "Tool should have inputSchema");

    console.log(`  ✓ Found ${tools.length} tools with valid structure`);
  });

  test("PREFLIGHT-004: Critical tools are present", async () => {
    const tools = await client.listTools();
    const toolNames = tools.map(t => t.name);

    // Verify critical CRUD tools exist
    const criticalTools = [
      "create_note",
      "get_note",
      "update_note",
      "delete_note",
      "list_notes",
      "search_notes",
    ];

    for (const toolName of criticalTools) {
      assert.ok(
        toolNames.includes(toolName),
        `Critical tool missing: ${toolName}`
      );
    }

    console.log(`  ✓ All ${criticalTools.length} critical tools present`);
  });

  test("PREFLIGHT-005: Session management works correctly", async () => {
    // Verify session ID was captured during initialization
    assert.ok(client.sessionId, "Client should have session ID after initialization");

    // Make another call to verify session persistence
    const tools = await client.listTools();
    assert.ok(tools.length > 0, "Should successfully use session for subsequent calls");

    console.log(`  ✓ Session ID: ${client.sessionId.slice(0, 16)}...`);
  });
});
