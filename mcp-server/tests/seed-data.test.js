import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 1: Seed Data", () => {
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

  test("SEED-001: Create single note for seeding", async () => {
    const uid = MCPTestClient.uniqueId();
    const result = await client.callTool("create_note", {
      content: `# Seed Note ${uid}\n\nThis is seed data for testing.`,
      tags: [MCPTestClient.testTag("seed"), "test/automated"],
      revision_mode: "none",
    });
    assert.ok(result.id, "Should return note ID");
    cleanup.noteIds.push(result.id);
  });

  test("SEED-002: Bulk create notes", async () => {
    const notes = Array.from({ length: 3 }, (_, i) => ({
      content: `# Bulk Seed ${i + 1} - ${MCPTestClient.uniqueId()}\n\nBulk note ${i + 1}.`,
      tags: [MCPTestClient.testTag("seed"), "test/bulk"],
      revision_mode: "none",
    }));

    const result = await client.callTool("bulk_create_notes", { notes });
    assert.ok(result, "Should return bulk create result");

    // Track created IDs for cleanup
    const ids = Array.isArray(result) ? result.map(n => n.id) : (result.ids || result.notes?.map(n => n.id) || []);
    cleanup.noteIds.push(...ids.filter(Boolean));
    assert.ok(ids.length > 0, "Should create multiple notes");
  });

  test("SEED-003: Create note with rich metadata", async () => {
    const result = await client.callTool("create_note", {
      content: `# Rich Metadata Seed ${MCPTestClient.uniqueId()}\n\n## Details\n\nNote with multiple tags and options.`,
      tags: [
        MCPTestClient.testTag("seed"),
        "domain/testing",
        "type/documentation",
      ],
      starred: true,
      revision_mode: "none",
    });
    assert.ok(result.id, "Should create note with metadata");
    cleanup.noteIds.push(result.id);
  });
});
