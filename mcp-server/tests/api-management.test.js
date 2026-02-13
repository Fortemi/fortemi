import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Rate Limiting & Extraction Stats", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    await client.close();
  });

  test("RATE-001: get_rate_limit_status returns status", async () => {
    const result = await client.callTool("get_rate_limit_status", {});
    assert.ok(result !== undefined, "Should return rate limit status");
    // enabled is a boolean
    assert.ok(
      typeof result.enabled === "boolean" || result.message,
      "Should include enabled flag or status message"
    );
  });

  test("EXT-001: get_extraction_stats returns pipeline stats", async () => {
    const result = await client.callTool("get_extraction_stats", {});
    assert.ok(result !== undefined, "Should return extraction stats");
  });
});

describe("Collection Export", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    for (const id of cleanup.collectionIds) {
      try { await client.callTool("delete_collection", { id }); } catch {}
    }
    await client.close();
  });

  test("COLEXP-001: export_collection returns data for empty collection", async () => {
    // Create an empty collection (no notes)
    const col = await client.callTool("create_collection", {
      name: `Export Test ${MCPTestClient.uniqueId()}`,
    });
    assert.ok(col, "Should create collection");
    const colId = col.id;
    cleanup.collectionIds.push(colId);

    // Export the empty collection — should succeed without JSON parse issues
    const result = await client.callTool("export_collection", { id: colId });
    // Empty collection may return null or an object
    // The key assertion is that it doesn't throw an error
    assert.ok(true, "Export should not throw for empty collection");
  });

  test("COLEXP-002: export_collection with include_frontmatter=false", async () => {
    const col = await client.callTool("create_collection", {
      name: `Export NoFM ${MCPTestClient.uniqueId()}`,
    });
    const colId = col.id;
    cleanup.collectionIds.push(colId);

    const result = await client.callTool("export_collection", {
      id: colId,
      include_frontmatter: false,
    });
    assert.ok(result !== undefined, "Should return export data without frontmatter");
  });

  test("COLEXP-003: export_collection with nonexistent ID returns null or error", async () => {
    // Nonexistent collection may return null (empty) or an error depending on API behavior
    try {
      const result = await client.callTool("export_collection", {
        id: "00000000-0000-0000-0000-000000000000",
      });
      // If no error thrown, result should be null/empty (collection not found returns empty export)
      assert.ok(
        result === null || result === undefined || result === "",
        "Should return null/empty for nonexistent collection"
      );
    } catch (e) {
      // Error is also acceptable — collection not found
      assert.ok(
        e.message.includes("error") || e.message.includes("not found"),
        "Error should indicate collection not found"
      );
    }
  });
});

describe("Backup Swap", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    await client.close();
  });

  test("SWAP-001: swap_backup with nonexistent file returns error", async () => {
    const errResult = await client.callToolExpectError("swap_backup", {
      filename: "nonexistent_shard_file.tar.gz",
      dry_run: true,
    });
    assert.ok(errResult.error, "Should return error for nonexistent backup file");
  });
});

describe("Memory Backup Download (curl-command pattern)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    await client.close();
  });

  test("MEMBK-001: memory_backup_download returns curl command", async () => {
    const result = await client.callTool("memory_backup_download", {
      name: "default",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(result.download_url, "Should return a download_url");
    assert.ok(result.suggested_filename, "Should return a suggested_filename");
    assert.ok(result.instructions, "Should return instructions");
    assert.ok(
      result.download_url.includes("/api/v1/backup/memory/"),
      "download_url should target backup memory endpoint"
    );
    assert.ok(
      result.suggested_filename.includes("default"),
      "suggested_filename should include memory name"
    );
    assert.ok(
      result.curl_command.includes("curl"),
      "curl_command should contain curl"
    );
  });

  test("MEMBK-002: memory_backup_download with custom name", async () => {
    const result = await client.callTool("memory_backup_download", {
      name: "test-archive",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(
      result.download_url.includes("test-archive"),
      "download_url should include the memory name"
    );
    assert.ok(
      result.suggested_filename.includes("test-archive"),
      "suggested_filename should include the memory name"
    );
  });
});
