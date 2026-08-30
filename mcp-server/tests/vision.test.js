#!/usr/bin/env node

/**
 * Vision capability UAT
 *
 * Vision inference is optional and model-dependent, so the release gate verifies
 * the discoverability and schema contract without requiring a large model pull.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Vision capability contract (UAT)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    await client.close();
  });

  test("VISION-001: system info exposes a typed vision capability", async () => {
    const info = await client.callTool("get_system_info", {});
    const vision = info?.infrastructure?.extraction?.vision;
    assert.ok(vision, "System info should expose infrastructure.extraction.vision");
    assert.equal(typeof vision.enabled, "boolean", "Vision enabled should be boolean");
    if (vision.enabled) {
      assert.ok(vision.provider, "Enabled vision should identify its provider");
    }
  });

  test("VISION-002: vision setup and degraded mode are documented", async () => {
    const result = await client.callTool("get_documentation", { topic: "vision" });
    const content = typeof result === "string" ? result : result.content;
    assert.match(content, /vision/i);
    assert.match(content, /OLLAMA_VISION_MODEL/);
    assert.match(content, /disabled gracefully/i);
  });

  test("VISION-003: capture tool publishes the vision_mode input contract", async () => {
    const tools = await client.listTools();
    const capture = tools.find((tool) => tool.name === "capture_knowledge");
    assert.ok(capture, "capture_knowledge should be registered");
    assert.ok(
      capture.inputSchema?.properties?.vision_mode,
      "capture_knowledge should publish vision_mode"
    );
  });
});
