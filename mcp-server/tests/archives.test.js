import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 12: Archives", () => {
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

  test("ARC-001: Archive a note", async () => {
    const note = await client.callTool("create_note", {
      content: `# Archive Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("archives")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Archive the note by updating status
    const result = await client.callTool("update_note", {
      id: note.id,
      status: "archived",
    });
    assert.ok(result, "Should archive successfully");
  });

  test("ARC-002: Unarchive a note", async () => {
    const note = await client.callTool("create_note", {
      content: `# Unarchive Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("archives")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Archive then unarchive
    await client.callTool("update_note", {
      id: note.id,
      status: "archived",
    });

    const result = await client.callTool("update_note", {
      id: note.id,
      status: "active",
    });
    assert.ok(result, "Should unarchive successfully");
  });

  test("ARC-003: List archived notes", async () => {
    const result = await client.callTool("list_notes", {
      filter: "archived",
    });
    assert.ok(result, "Should return archived notes list");
  });
});
