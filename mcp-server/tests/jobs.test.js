import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 15: Background Jobs", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("JOB-001: List jobs", async () => {
    const result = await client.callTool("list_jobs", {});
    assert.ok(result, "Should return jobs list");
    // Should be an array or object with jobs
    if (Array.isArray(result)) {
      assert.ok(Array.isArray(result), "Jobs should be an array");
    } else {
      assert.ok(result.jobs !== undefined || result.items !== undefined, "Should have jobs");
    }
  });

  test("JOB-002: Get job stats", async () => {
    const result = await client.callTool("get_queue_stats", {});
    assert.ok(result, "Should return queue stats");
  });

  test("JOB-003: Get pending jobs count", async () => {
    const result = await client.callTool("get_pending_jobs_count", {});
    assert.ok(result !== undefined && result !== null, "Should return pending count");
  });
});
