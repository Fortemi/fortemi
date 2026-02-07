import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 16: Observability", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("OBS-001: Get knowledge health dashboard", async () => {
    const result = await client.callTool("get_knowledge_health", {});
    assert.ok(result, "Should return knowledge health data");
  });

  test("OBS-002: Get orphan tags", async () => {
    const result = await client.callTool("get_orphan_tags", {});
    assert.ok(result !== undefined, "Should return orphan tags data");
  });

  test("OBS-003: Get stale notes", async () => {
    const result = await client.callTool("get_stale_notes", {});
    assert.ok(result !== undefined, "Should return stale notes data");
  });

  test("OBS-004: Get unlinked notes", async () => {
    const result = await client.callTool("get_unlinked_notes", {});
    assert.ok(result !== undefined, "Should return unlinked notes data");
  });

  test("OBS-005: Server info tool", async () => {
    const result = await client.callTool("server_info", {});
    assert.ok(result, "Should return server info");
  });
});
