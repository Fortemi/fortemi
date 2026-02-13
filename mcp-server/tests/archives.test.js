import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 12: Memory Archives", () => {
  let client;
  const cleanup = { archiveNames: [], noteIds: [] };
  const testSuffix = MCPTestClient.uniqueId().slice(0, 8);

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Restore default archive to "public" to prevent state leakage to other test suites
    try {
      await client.callTool("set_default_archive", { name: "public" });
      await client.callTool("select_memory", { name: "public" });
    } catch {}

    // Clean up notes first
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    // Clean up archives (reverse order to handle dependencies)
    for (const name of cleanup.archiveNames.reverse()) {
      try { await client.callTool("delete_archive", { name }); } catch {}
    }
    await client.close();
  });

  test("ARCH-001: list_archives returns at least default archive", async () => {
    const result = await client.callTool("list_archives", {});
    assert.ok(Array.isArray(result), "Should return array");
    assert.ok(result.length >= 1, "Should have at least one archive (default)");

    const defaultArchive = result.find((a) => a.is_default);
    assert.ok(defaultArchive, "Should have a default archive");
    console.log(`  Found ${result.length} archives, default: ${defaultArchive.name}`);
  });

  test("ARCH-002: create_archive with name", async () => {
    const name = `test-arch-${testSuffix}-a`;
    const result = await client.callTool("create_archive", {
      name,
      description: "Test archive A",
    });

    assert.ok(result, "Should return created archive");
    assert.ok(result.id || result.name, "Should have id or name");
    cleanup.archiveNames.push(name);
    console.log(`  Created archive: ${name}`);
  });

  test("ARCH-003: create_archive with description", async () => {
    const name = `test-arch-${testSuffix}-b`;
    const description = "Second test archive with description";
    const result = await client.callTool("create_archive", {
      name,
      description,
    });

    assert.ok(result, "Should create archive with description");
    cleanup.archiveNames.push(name);
  });

  test("ARCH-004: list_archives shows created archives", async () => {
    const result = await client.callTool("list_archives", {});
    assert.ok(result.length >= 3, `Should have 3+ archives (default + 2 created), got ${result.length}`);

    const nameA = `test-arch-${testSuffix}-a`;
    const found = result.find((a) => a.name === nameA);
    assert.ok(found, `Should find archive '${nameA}' in list`);
  });

  test("ARCH-005: get_archive returns archive details", async () => {
    const name = `test-arch-${testSuffix}-a`;
    const result = await client.callTool("get_archive", { name });

    assert.ok(result, "Should return archive details");
    assert.strictEqual(result.name, name, "Name should match");
    assert.ok(result.description !== undefined, "Should have description field");
  });

  test("ARCH-006: get_archive_stats returns statistics", async () => {
    const name = `test-arch-${testSuffix}-a`;
    const result = await client.callTool("get_archive_stats", { name });

    assert.ok(result, "Should return archive stats");
    assert.ok(result.note_count !== undefined, "Should have note_count");
    assert.ok(result.size_bytes !== undefined, "Should have size_bytes");
    console.log(`  Archive stats: ${result.note_count} notes, ${result.size_bytes} bytes`);
  });

  test("ARCH-007: update_archive changes description", async () => {
    const name = `test-arch-${testSuffix}-a`;
    const newDesc = "Updated description for archive A";
    const result = await client.callTool("update_archive", {
      name,
      description: newDesc,
    });

    assert.ok(result, "Should update archive");

    // Verify update
    const archive = await client.callTool("get_archive", { name });
    assert.strictEqual(archive.description, newDesc, "Description should be updated");
  });

  test("ARCH-008: set_default_archive changes default", async () => {
    const name = `test-arch-${testSuffix}-a`;

    // Get current default first
    const before = await client.callTool("list_archives", {});
    const originalDefault = before.find((a) => a.is_default);

    const result = await client.callTool("set_default_archive", { name });
    assert.ok(result, "Should set default archive");

    // Verify
    const after = await client.callTool("list_archives", {});
    const newDefault = after.find((a) => a.is_default);
    assert.strictEqual(newDefault.name, name, "New default should be our archive");

    // Restore original default
    if (originalDefault && originalDefault.name !== name) {
      await client.callTool("set_default_archive", { name: originalDefault.name });
    }
  });

  test("ARCH-009: Verify default switch is reflected in list", async () => {
    const name = `test-arch-${testSuffix}-b`;
    const before = await client.callTool("list_archives", {});
    const originalDefault = before.find((a) => a.is_default);

    await client.callTool("set_default_archive", { name });
    const after = await client.callTool("list_archives", {});

    const newDefault = after.find((a) => a.is_default);
    assert.strictEqual(newDefault.name, name, "Default should be switched");

    // Only one default allowed
    const defaults = after.filter((a) => a.is_default);
    assert.strictEqual(defaults.length, 1, "Should have exactly one default");

    // Restore
    if (originalDefault) {
      await client.callTool("set_default_archive", { name: originalDefault.name });
    }
  });

  test("ARCH-010: Create note in specific archive context", async () => {
    // This tests that notes can be created (the API uses X-Fortemi-Memory header)
    // MCP may not support archive-scoped operations directly, so we test what's available
    const note = await client.callTool("create_note", {
      content: `# Archive Note ${MCPTestClient.uniqueId()}\n\nNote in default archive.`,
      tags: [MCPTestClient.testTag("archives", "note")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create note");
    cleanup.noteIds.push(note.id);
  });

  test("ARCH-011: Get archive stats after adding notes", async () => {
    // Get stats for default archive (where we created notes)
    const archives = await client.callTool("list_archives", {});
    const defaultArchive = archives.find((a) => a.is_default);

    const stats = await client.callTool("get_archive_stats", { name: defaultArchive.name });
    assert.ok(stats.note_count >= 0, "Should have note count");
  });

  test("ARCH-012: create_archive with duplicate name errors", async () => {
    const name = `test-arch-${testSuffix}-a`; // Already exists
    const error = await client.callToolExpectError("create_archive", { name });

    assert.ok(error.error, "Should error for duplicate name");
    assert.ok(
      error.error.includes("exists") || error.error.includes("duplicate") ||
      error.error.includes("already") || error.error.includes("conflict"),
      "Error should indicate duplicate"
    );
  });

  test("ARCH-013: delete_archive removes archive", async () => {
    // Create a throwaway archive to delete
    const name = `test-arch-${testSuffix}-del`;
    await client.callTool("create_archive", { name, description: "To be deleted" });

    const result = await client.callTool("delete_archive", { name });
    assert.ok(result, "Should delete archive");

    // Verify it's gone
    const error = await client.callToolExpectError("get_archive", { name });
    assert.ok(error.error, "Should error for deleted archive");
  });

  test("ARCH-014: delete_archive for non-existent name errors", async () => {
    const error = await client.callToolExpectError("delete_archive", {
      name: `nonexistent-${MCPTestClient.uniqueId()}`,
    });
    assert.ok(error.error, "Should error for non-existent archive");
  });

  test("ARCH-015: Cannot delete default archive", async () => {
    const archives = await client.callTool("list_archives", {});
    const defaultArchive = archives.find((a) => a.is_default);

    const error = await client.callToolExpectError("delete_archive", {
      name: defaultArchive.name,
    });
    assert.ok(error.error, "Should error when deleting default archive");
  });

  test("ARCH-016: search_memories_federated searches across archives", async () => {
    const result = await client.callTool("search_memories_federated", {
      q: "test",
      memories: ["all"],
      limit: 5,
    });
    assert.ok(result !== undefined, "Should return federated search results");
    assert.ok(Array.isArray(result.results), "Should have results array");
    assert.ok(result.memories_searched !== undefined, "Should report memories searched");
    console.log(`  ✓ Federated search returned ${result.results.length} results across ${result.memories_searched} memories`);
  });

  test("ARCH-017: get_archive with non-existent name errors", async () => {
    const error = await client.callToolExpectError("get_archive", {
      name: `nonexistent-${MCPTestClient.uniqueId()}`,
    });
    assert.ok(error.error, "Should error for non-existent archive");
  });

  test("ARCH-018: update_archive clears description with null", async () => {
    const name = `test-arch-${testSuffix}-b`;
    await client.callTool("update_archive", { name, description: null });

    const archive = await client.callTool("get_archive", { name });
    assert.ok(
      archive.description === null || archive.description === "" || archive.description === undefined,
      "Description should be cleared"
    );
  });

  test("ARCH-019: Archive listing structure is complete", async () => {
    const result = await client.callTool("list_archives", {});
    assert.ok(result.length > 0, "Should have archives");

    const archive = result[0];
    assert.ok(archive.name, "Should have name");
    assert.ok(archive.is_default !== undefined, "Should have is_default field");
    // Other fields may be present: id, schema_name, description, created_at, note_count, size_bytes
  });

  test("ARCH-020: set_default_archive syncs session memory (Issue #316)", async () => {
    // Create a dedicated archive for this test
    const archiveName = `arch316-${MCPTestClient.uniqueId()}`;
    await client.callTool("create_archive", {
      name: archiveName,
      description: "Issue #316 isolation test",
    });

    // Save original default to restore later
    const before = await client.callTool("list_archives", {});
    const originalDefault = before.find((a) => a.is_default);

    try {
      // Set the test archive as default - this should ALSO set session memory
      await client.callTool("set_default_archive", { name: archiveName });

      // Create a note - should go into the test archive, not public
      const note = await client.callTool("create_note", {
        content: `Issue 316 test note ${MCPTestClient.uniqueId()}`,
        revision_mode: "none",
      });
      assert.ok(note.id, "Should create note");

      // Switch back to public as default
      await client.callTool("set_default_archive", { name: "public" });

      // List notes in public - the test note should NOT appear
      const publicNotes = await client.callTool("list_notes", { limit: 100 });
      const foundInPublic = publicNotes.notes.some((n) => n.id === note.id);
      assert.strictEqual(foundInPublic, false, "Note should NOT appear in public after switching default");

      // Switch to test archive and verify the note IS there
      await client.callTool("select_memory", { name: archiveName });
      const archiveNotes = await client.callTool("list_notes", { limit: 100 });
      const foundInArchive = archiveNotes.notes.some((n) => n.id === note.id);
      assert.strictEqual(foundInArchive, true, "Note should be in the archive where it was created");

      // Clean up - delete the note
      await client.callTool("delete_note", { id: note.id });
    } finally {
      // Restore original default
      if (originalDefault) {
        await client.callTool("set_default_archive", { name: originalDefault.name });
      }
      // Switch back to public for other tests
      await client.callTool("select_memory", { name: "public" });
      // Delete test archive
      try {
        await client.callTool("delete_archive", { name: archiveName });
      } catch {
        // Ignore if can't delete
      }
    }
  });
});
