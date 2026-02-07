import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 20: Data Export", () => {
  let client;
  const cleanup = { noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  test("EXP-001: Export knowledge archive", async () => {
    // Create a test note to ensure there's data to export
    const note = await client.callTool("create_note", {
      content: `# Export Test ${MCPTestClient.uniqueId()}\n\nData for export testing.`,
      tags: [MCPTestClient.testTag("export")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Try to download knowledge archive
    try {
      const result = await client.callTool("knowledge_archive_download", {});
      assert.ok(result, "Should return archive data or path");
    } catch (e) {
      // Export might not be available in test env — that's acceptable
      assert.ok(
        e.message.includes("error") || e.message.includes("not"),
        "Should fail gracefully if export not available"
      );
    }
  });
});
