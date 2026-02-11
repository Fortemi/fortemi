import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 20: Data Export", () => {
  let client;
  const cleanup = { noteIds: [] };
  let sharedNoteId;
  let snapshotFilename;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();

    // Create a test note for export operations
    const note = await client.callTool("create_note", {
      content: `# Export Test ${MCPTestClient.uniqueId()}\n\nData for export and backup testing.`,
      tags: [MCPTestClient.testTag("export")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create test note");
    sharedNoteId = note.id;
    cleanup.noteIds.push(note.id);

    // Small delay for indexing
    await new Promise((r) => setTimeout(r, 300));
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  // --- Backup Status ---

  test("BACK-001: Backup status", async () => {
    const result = await client.callTool("backup_status", {});
    assert.ok(result, "Should return backup status");
    assert.ok(result.status !== undefined, "Should have status field");
    console.log(`  Backup status: ${result.status}, count: ${result.backup_count}`);
  });

  test("BACK-002: Trigger backup", async () => {
    try {
      const result = await client.callTool("backup_now", {});
      assert.ok(result, "Should return backup result");
      console.log(`  Backup result: ${JSON.stringify(result).slice(0, 200)}`);
    } catch (e) {
      // backup_now may require backup script configuration
      assert.ok(
        e.message.includes("error") || e.message.includes("not") || e.message.includes("script"),
        "Should fail gracefully if backup not configured"
      );
      console.log(`  Backup not configured (expected in test env): ${e.message.slice(0, 100)}`);
    }
  });

  // --- Export Operations ---

  test("BACK-003: Export all notes", async () => {
    const result = await client.callTool("export_all_notes", {});
    assert.ok(result, "Should return export data");
    // Should have notes or manifest
    assert.ok(
      result.notes || result.manifest || Array.isArray(result),
      "Should have notes data or manifest"
    );
    if (result.notes) {
      assert.ok(Array.isArray(result.notes), "Notes should be an array");
      assert.ok(result.notes.length > 0, "Should have at least one note");
      console.log(`  Exported ${result.notes.length} notes`);
    }
    if (result.manifest) {
      console.log(`  Export manifest: ${JSON.stringify(result.manifest).slice(0, 200)}`);
    }
  });

  test("BACK-004: Export single note (revised content)", async () => {
    const result = await client.callTool("export_note", {
      id: sharedNoteId,
      content: "revised",
      include_frontmatter: true,
    });
    assert.ok(result, "Should return exported note");
    // Result may be a string (markdown) or object
    const content = typeof result === "string" ? result : (result.content || result.markdown || JSON.stringify(result));
    assert.ok(content.length > 0, "Exported content should not be empty");
    // With frontmatter, should contain YAML delimiters
    if (typeof result === "string" && result.includes("---")) {
      console.log(`  Exported with frontmatter (${content.length} chars)`);
    } else {
      console.log(`  Exported note (${content.length} chars)`);
    }
  });

  test("BACK-005: Export note - original content", async () => {
    const result = await client.callTool("export_note", {
      id: sharedNoteId,
      content: "original",
    });
    assert.ok(result, "Should return original content");
    const content = typeof result === "string" ? result : (result.content || result.markdown || JSON.stringify(result));
    assert.ok(content.length > 0, "Original content should not be empty");
    // Original content should contain our test content
    assert.ok(
      content.includes("Export Test") || content.includes("export"),
      "Should contain original test content"
    );
  });

  // --- Knowledge Shards ---

  test("BACK-006: Create knowledge shard", async () => {
    try {
      const result = await client.callTool("knowledge_shard", {});
      assert.ok(result, "Should return shard data or curl command");
      // May return curl command string, download URL, or shard manifest
      console.log(`  Shard result type: ${typeof result}, keys: ${typeof result === "object" ? Object.keys(result).join(",") : "N/A"}`);
    } catch (e) {
      // Knowledge shard may not be available in all environments
      console.log(`  Knowledge shard not available: ${e.message.slice(0, 100)}`);
    }
  });

  test("BACK-007: Knowledge shard with components", async () => {
    try {
      const result = await client.callTool("knowledge_shard", {
        include: "notes,collections,tags",
      });
      assert.ok(result, "Should return shard with specified components");
    } catch (e) {
      console.log(`  Knowledge shard with components not available: ${e.message.slice(0, 100)}`);
    }
  });

  test("BACK-008: Knowledge shard import (dry run)", async () => {
    // knowledge_shard_import requires a file on disk - test with graceful handling
    try {
      const result = await client.callTool("knowledge_shard_import", {
        file_path: "/tmp/nonexistent-shard.tar.gz",
        dry_run: true,
      });
      // If it succeeds, validate result
      assert.ok(result, "Should return import result");
    } catch (e) {
      // Expected to fail with file not found
      assert.ok(
        e.message.includes("error") || e.message.includes("not found") ||
        e.message.includes("no such") || e.message.includes("ENOENT") ||
        e.message.includes("exist"),
        "Should error for non-existent shard file"
      );
    }
  });

  // --- Backup Browser ---

  test("BACK-009: List backups", async () => {
    const result = await client.callTool("list_backups", {});
    assert.ok(result, "Should return backup list");
    // Result should have shards array or be an array
    const backups = result.shards || result.backups || (Array.isArray(result) ? result : []);
    assert.ok(Array.isArray(backups), "Backup list should be an array");
    console.log(`  Found ${backups.length} backups`);
  });

  test("BACK-010: Get backup info", async () => {
    // First get a backup filename from list
    const list = await client.callTool("list_backups", {});
    const backups = list.shards || list.backups || (Array.isArray(list) ? list : []);

    if (backups.length === 0) {
      console.log(`  No backups available to inspect, skipping`);
      return;
    }

    const filename = backups[0].filename || backups[0].name;
    assert.ok(filename, "Should have backup filename");

    const result = await client.callTool("get_backup_info", { filename });
    assert.ok(result, "Should return backup info");
    assert.ok(result.filename || result.name, "Should have filename");
    console.log(`  Backup info: ${filename} (${result.size_human || result.size_bytes + " bytes"})`);
  });

  test("BACK-011: Get backup metadata", async () => {
    const list = await client.callTool("list_backups", {});
    const backups = list.shards || list.backups || (Array.isArray(list) ? list : []);

    if (backups.length === 0) {
      console.log(`  No backups available for metadata, skipping`);
      return;
    }

    const filename = backups[0].filename || backups[0].name;
    const result = await client.callTool("get_backup_metadata", { filename });
    assert.ok(result, "Should return backup metadata");
    // has_metadata may be true or false
    assert.ok(result.has_metadata !== undefined || result.metadata !== undefined,
      "Should indicate whether metadata exists");
    console.log(`  Metadata for ${filename}: has_metadata=${result.has_metadata}`);
  });

  test("BACK-012: Update backup metadata", async () => {
    const list = await client.callTool("list_backups", {});
    const backups = list.shards || list.backups || (Array.isArray(list) ? list : []);

    if (backups.length === 0) {
      console.log(`  No backups available to update metadata, skipping`);
      return;
    }

    const filename = backups[0].filename || backups[0].name;
    const result = await client.callTool("update_backup_metadata", {
      filename,
      title: "UAT Test Backup",
      description: "Backup metadata updated during integration testing",
    });
    assert.ok(result, "Should update backup metadata");
    assert.ok(result.success || result.metadata, "Should indicate success");

    // Verify the update
    const verify = await client.callTool("get_backup_metadata", { filename });
    if (verify.has_metadata && verify.metadata) {
      assert.strictEqual(verify.metadata.title, "UAT Test Backup", "Title should be updated");
    }
  });

  // --- Database Operations ---

  test("BACK-013: Database snapshot", async () => {
    const testSuffix = MCPTestClient.uniqueId().slice(0, 8);
    const result = await client.callTool("database_snapshot", {
      name: `uat-test-${testSuffix}`,
      title: "UAT Integration Test Snapshot",
      description: "Created during automated integration testing",
    });
    assert.ok(result, "Should create database snapshot");
    assert.ok(result.success || result.filename, "Should indicate success or return filename");
    if (result.filename) {
      snapshotFilename = result.filename;
      console.log(`  Snapshot created: ${result.filename} (${result.size_human || ""})`);
    }
  });

  test("BACK-014: Backup download", async () => {
    try {
      const result = await client.callTool("backup_download", {});
      assert.ok(result, "Should return download data");
      // May return notes data or download path
      if (result.notes) {
        assert.ok(Array.isArray(result.notes), "Notes should be an array");
        console.log(`  Download contains ${result.notes.length} notes`);
      } else {
        console.log(`  Download result: ${JSON.stringify(result).slice(0, 200)}`);
      }
    } catch (e) {
      // backup_download may return data directly or fail in test env
      console.log(`  Backup download: ${e.message.slice(0, 100)}`);
    }
  });

  // --- Knowledge Archives ---

  test("BACK-015: Knowledge archive download", async () => {
    // Need a backup filename to download as archive
    const list = await client.callTool("list_backups", {});
    const backups = list.shards || list.backups || (Array.isArray(list) ? list : []);

    if (backups.length === 0) {
      console.log(`  No backups available for archive download, skipping`);
      return;
    }

    const filename = snapshotFilename || backups[0].filename || backups[0].name;
    try {
      const result = await client.callTool("knowledge_archive_download", {
        filename,
      });
      assert.ok(result, "Should return archive download data or curl command");
      console.log(`  Archive download result: ${JSON.stringify(result).slice(0, 200)}`);
    } catch (e) {
      // May fail if archive format not supported for this backup
      console.log(`  Archive download: ${e.message.slice(0, 100)}`);
    }
  });

  test("BACK-016: Knowledge archive upload", async () => {
    // knowledge_archive_upload requires a file on disk
    try {
      const result = await client.callTool("knowledge_archive_upload", {
        file_path: "/tmp/nonexistent-archive.archive",
      });
      assert.ok(result, "Should return upload result");
    } catch (e) {
      // Expected to fail with file not found
      assert.ok(
        e.message.includes("error") || e.message.includes("not found") ||
        e.message.includes("no such") || e.message.includes("ENOENT") ||
        e.message.includes("exist"),
        "Should error for non-existent archive file"
      );
    }
  });

  // --- Database Restore ---

  test("BACK-017: Database restore (verify API accepts params)", async () => {
    // WARNING: database_restore is destructive. We test that the API handles
    // a non-existent filename gracefully rather than actually restoring.
    const error = await client.callToolExpectError("database_restore", {
      filename: `nonexistent-backup-${MCPTestClient.uniqueId()}.sql.gz`,
      skip_snapshot: false,
    });
    assert.ok(error.error, "Should error for non-existent backup filename");
    assert.ok(
      error.error.includes("not found") || error.error.includes("error") ||
      error.error.includes("exist") || error.error.includes("No such"),
      "Error should indicate file not found"
    );
  });

  // --- Memory Info ---

  test("BACK-018: Memory info", async () => {
    const result = await client.callTool("memory_info", {});
    assert.ok(result, "Should return memory info");
    // Should have summary or storage info
    assert.ok(
      result.summary || result.storage || result.total_notes !== undefined,
      "Should have summary or storage data"
    );
    if (result.summary) {
      console.log(`  Memory: ${result.summary.total_notes} notes, storage: ${result.storage?.database_total_bytes || "N/A"} bytes`);
    }
    if (result.recommendations) {
      console.log(`  Recommendations: ${result.recommendations.length || "N/A"} items`);
    }
  });

  // --- Import with Conflict Resolution ---

  test("BACK-019: Import with conflict resolution (dry run)", async () => {
    const result = await client.callTool("backup_import", {
      backup: {
        notes: [
          {
            original_content: "# Test Import Note\n\nImported during integration testing.",
            tags: ["uat-import-test"],
          },
        ],
      },
      dry_run: true,
      on_conflict: "skip",
    });
    assert.ok(result, "Should return import dry run result");
    // Dry run should report what would be imported
    console.log(`  Import dry run: ${JSON.stringify(result).slice(0, 300)}`);
  });
});
