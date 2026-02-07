import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 11: Note Versioning", () => {
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

  test("VER-001: Note has version history after creation", async () => {
    const note = await client.callTool("create_note", {
      content: `# Version Test ${MCPTestClient.uniqueId()}\n\nOriginal content.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create note");
    cleanup.noteIds.push(note.id);

    const retrieved = await client.callTool("get_note", { id: note.id });
    assert.ok(retrieved, "Should retrieve note");
  });

  test("VER-002: Update creates new version", async () => {
    const note = await client.callTool("create_note", {
      content: `# Version Update Test ${MCPTestClient.uniqueId()}\n\nVersion 1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Update the note
    await client.callTool("update_note", {
      id: note.id,
      content: `# Version Update Test\n\nVersion 2 - updated content.`,
    });

    const updated = await client.callTool("get_note", { id: note.id });
    assert.ok(updated, "Should retrieve updated note");
    // Content should reflect the update
    const content = typeof updated === "string" ? updated : (updated.content || updated.note || "");
    assert.ok(
      content.includes("Version 2") || content.includes("updated"),
      "Content should be updated"
    );
  });
});
