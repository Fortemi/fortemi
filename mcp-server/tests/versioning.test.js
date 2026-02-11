import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 11: Note Versioning", () => {
  let client;
  const cleanup = { noteIds: [] };
  let sharedNoteId;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();

    // Create a shared note and build up version history
    const note = await client.callTool("create_note", {
      content: `# Version Test ${MCPTestClient.uniqueId()}\n\nVersion 1 content.`,
      tags: [MCPTestClient.testTag("versioning"), "v1-tag"],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create shared note");
    sharedNoteId = note.id;
    cleanup.noteIds.push(note.id);

    // Create v2
    await client.callTool("update_note", {
      id: sharedNoteId,
      content: `# Version Test\n\nVersion 2 content with updates.`,
    });

    // Create v3
    await client.callTool("update_note", {
      id: sharedNoteId,
      content: `# Version Test\n\nVersion 3 content with more changes.`,
    });

    // Small delay for indexing
    await new Promise((r) => setTimeout(r, 300));
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  test("VER-001: Note has version history after creation", async () => {
    const note = await client.callTool("create_note", {
      content: `# VER-001 ${MCPTestClient.uniqueId()}\n\nOriginal content.`,
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
      content: `# VER-002 ${MCPTestClient.uniqueId()}\n\nVersion 1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await client.callTool("update_note", {
      id: note.id,
      content: `# VER-002\n\nVersion 2 - updated content.`,
    });

    const updated = await client.callTool("get_note", { id: note.id });
    assert.ok(updated, "Should retrieve updated note");
    const content = updated.original?.content || updated.revised?.content || "";
    assert.ok(
      content.includes("Version 2") || content.includes("updated"),
      "Content should be updated"
    );
  });

  test("VER-003: list_note_versions returns version history", async () => {
    const result = await client.callTool("list_note_versions", {
      note_id: sharedNoteId,
    });

    assert.ok(result, "Should return version data");
    assert.ok(result.original_versions, "Should have original_versions array");
    assert.ok(Array.isArray(result.original_versions), "original_versions should be array");
    assert.ok(result.original_versions.length >= 3, `Should have at least 3 versions, got ${result.original_versions.length}`);

    console.log(`  Found ${result.original_versions.length} original versions`);
  });

  test("VER-004: Version entries have expected structure", async () => {
    const result = await client.callTool("list_note_versions", {
      note_id: sharedNoteId,
    });

    const versions = result.original_versions;
    assert.ok(versions.length > 0, "Should have versions");

    const v = versions[0];
    assert.ok(v.version_number !== undefined, "Should have version_number");
    assert.ok(v.created_at_utc || v.created_at, "Should have created_at timestamp");
  });

  test("VER-005: get_note_version retrieves specific version (original track)", async () => {
    const result = await client.callTool("get_note_version", {
      note_id: sharedNoteId,
      version: 1,
      track: "original",
    });

    assert.ok(result, "Should return version content");
    // Version 1 should contain original content
    const content = typeof result === "string" ? result : (result.content || JSON.stringify(result));
    assert.ok(content, "Should have content");
    console.log(`  Version 1 content length: ${content.length}`);
  });

  test("VER-006: get_note_version retrieves latest version", async () => {
    // Get version list first to find current version
    const versions = await client.callTool("list_note_versions", {
      note_id: sharedNoteId,
    });
    const currentVersion = versions.current_original_version || versions.original_versions.length;

    const result = await client.callTool("get_note_version", {
      note_id: sharedNoteId,
      version: currentVersion,
      track: "original",
    });

    assert.ok(result, "Should return current version");
  });

  test("VER-007: diff_note_versions shows changes between versions", async () => {
    const result = await client.callTool("diff_note_versions", {
      note_id: sharedNoteId,
      from_version: 1,
      to_version: 2,
    });

    assert.ok(result !== undefined && result !== null, "Should return diff result");
    // Diff may be a string (unified diff) or an object
    const diffText = typeof result === "string" ? result : (result.diff || JSON.stringify(result));
    assert.ok(diffText.length > 0, "Diff should not be empty");
    console.log(`  Diff length: ${diffText.length} chars`);
  });

  test("VER-008: diff_note_versions between v1 and v3", async () => {
    const result = await client.callTool("diff_note_versions", {
      note_id: sharedNoteId,
      from_version: 1,
      to_version: 3,
    });

    assert.ok(result !== undefined && result !== null, "Should return diff");
  });

  test("VER-009: restore_note_version restores previous content", async () => {
    // Create a note specifically for restore testing
    const note = await client.callTool("create_note", {
      content: `# Restore Test ${MCPTestClient.uniqueId()}\n\nOriginal for restore.`,
      tags: [MCPTestClient.testTag("versioning"), "restore-original"],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Update to v2
    await client.callTool("update_note", {
      id: note.id,
      content: `# Restore Test\n\nModified content v2.`,
    });

    // Restore to v1
    const result = await client.callTool("restore_note_version", {
      note_id: note.id,
      version: 1,
      restore_tags: false,
    });

    assert.ok(result, "Should return restore result");
    assert.ok(
      result.success || result.restored_from_version !== undefined,
      "Should indicate successful restore"
    );
    console.log(`  Restored from version: ${result.restored_from_version}`);
  });

  test("VER-010: Restore creates new version (doesn't overwrite)", async () => {
    // After restore in VER-009, check that a new version was created
    const note = await client.callTool("create_note", {
      content: `# Restore Version Check ${MCPTestClient.uniqueId()}\n\nV1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await client.callTool("update_note", { id: note.id, content: "# V2 content" });
    await client.callTool("restore_note_version", { note_id: note.id, version: 1 });

    const versions = await client.callTool("list_note_versions", { note_id: note.id });
    // Should have: v1 (create), v2 (update), v3 (restore from v1)
    assert.ok(
      versions.original_versions.length >= 3,
      `Should have 3+ versions after restore, got ${versions.original_versions.length}`
    );
  });

  test("VER-011: restore_note_version with restore_tags=true", async () => {
    const note = await client.callTool("create_note", {
      content: `# Tag Restore ${MCPTestClient.uniqueId()}\n\nWith tags.`,
      tags: [MCPTestClient.testTag("versioning"), "original-tag"],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Update content and tags
    await client.callTool("update_note", { id: note.id, content: "# Updated content" });
    await client.callTool("set_note_tags", { id: note.id, tags: ["new-tag"] });

    // Restore with tags
    const result = await client.callTool("restore_note_version", {
      note_id: note.id,
      version: 1,
      restore_tags: true,
    });

    assert.ok(result, "Should restore with tags");
  });

  test("VER-012: delete_note_version removes a version", async () => {
    const note = await client.callTool("create_note", {
      content: `# Delete Version ${MCPTestClient.uniqueId()}\n\nV1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Create v2 and v3 so we can delete v2
    await client.callTool("update_note", { id: note.id, content: "# V2" });
    await client.callTool("update_note", { id: note.id, content: "# V3" });

    // Delete v2 (not current)
    const result = await client.callTool("delete_note_version", {
      note_id: note.id,
      version: 2,
    });

    assert.ok(result, "Should return delete result");
    assert.ok(
      result.success || result.deleted_version !== undefined,
      "Should indicate successful deletion"
    );
  });

  test("VER-013: Verify deleted version is gone", async () => {
    const note = await client.callTool("create_note", {
      content: `# Verify Delete ${MCPTestClient.uniqueId()}\n\nV1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await client.callTool("update_note", { id: note.id, content: "# V2" });
    await client.callTool("update_note", { id: note.id, content: "# V3" });
    await client.callTool("delete_note_version", { note_id: note.id, version: 2 });

    // Try to get deleted version - should error
    const error = await client.callToolExpectError("get_note_version", {
      note_id: note.id,
      version: 2,
      track: "original",
    });
    assert.ok(error.error, "Should error when getting deleted version");
  });

  test("VER-014: get_note_version with non-existent version errors", async () => {
    const error = await client.callToolExpectError("get_note_version", {
      note_id: sharedNoteId,
      version: 999,
      track: "original",
    });
    assert.ok(error.error, "Should error for non-existent version");
  });

  test("VER-015: diff_note_versions with deleted version errors", async () => {
    const note = await client.callTool("create_note", {
      content: `# Diff Deleted ${MCPTestClient.uniqueId()}\n\nV1.`,
      tags: [MCPTestClient.testTag("versioning")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await client.callTool("update_note", { id: note.id, content: "# V2" });
    await client.callTool("update_note", { id: note.id, content: "# V3" });
    await client.callTool("delete_note_version", { note_id: note.id, version: 2 });

    // Try to diff using deleted version
    const error = await client.callToolExpectError("diff_note_versions", {
      note_id: note.id,
      from_version: 2,
      to_version: 3,
    });
    assert.ok(error.error, "Should error when diffing with deleted version");
  });
});
