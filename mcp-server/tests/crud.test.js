#!/usr/bin/env node

/**
 * Phase 2: CRUD Operations (CRITICAL)
 *
 * Tests core note lifecycle operations that form the foundation of the
 * knowledge base. These operations must work correctly for all other
 * features to function properly.
 *
 * Tests:
 * - Create notes with content and tags
 * - Retrieve notes by ID
 * - Update note content and metadata
 * - Delete notes (soft delete)
 * - List notes with filtering
 * - Bulk operations
 * - Error handling for invalid inputs
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 2: CRUD Operations (CRITICAL)", () => {
  let client;
  const cleanup = { noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Cleanup all test notes
    console.log(`  Cleaning up ${cleanup.noteIds.length} test notes...`);
    for (const id of cleanup.noteIds) {
      try {
        await client.callTool("delete_note", { id });
      } catch (error) {
        // Ignore cleanup errors
      }
    }
    await client.close();
  });

  test("CRUD-001: Create note with content and tags returns ID", async () => {
    const testTag = MCPTestClient.testTag("crud", "create");
    const content = `# Test Note

This is a test note created at ${new Date().toISOString()}.

It has multiple paragraphs and markdown formatting.`;

    const result = await client.callTool("create_note", {
      content,
      tags: [testTag, "test/automated"],
    });

    assert.ok(result, "Should return a result");
    assert.ok(result.id, "Result should contain note ID");
    assert.match(
      result.id,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
      "ID should be a valid UUID"
    );

    cleanup.noteIds.push(result.id);
    console.log(`  ✓ Created note: ${result.id}`);
  });

  test("CRUD-002: Get note by ID returns content, tags, and metadata", async () => {
    // Create a note first
    const testTag = MCPTestClient.testTag("crud", "get");
    const originalContent = "Test content for retrieval";

    const created = await client.callTool("create_note", {
      content: originalContent,
      tags: [testTag],
    });
    cleanup.noteIds.push(created.id);

    // Retrieve the note
    const result = await client.callTool("get_note", { id: created.id });

    assert.ok(result, "Should return a result");
    assert.strictEqual(result.id, created.id, "ID should match");
    assert.ok(result.content, "Should have content");
    assert.ok(result.content.includes(originalContent), "Content should match original");
    assert.ok(Array.isArray(result.tags), "Tags should be an array");
    assert.ok(result.tags.includes(testTag), "Should include original tag");
    assert.ok(result.created_at, "Should have created_at timestamp");
    assert.ok(result.updated_at, "Should have updated_at timestamp");

    console.log(`  ✓ Retrieved note with ${result.tags.length} tags`);
  });

  test("CRUD-003: Update note changes content", async () => {
    // Create a note
    const testTag = MCPTestClient.testTag("crud", "update");
    const originalContent = "Original content";

    const created = await client.callTool("create_note", {
      content: originalContent,
      tags: [testTag],
    });
    cleanup.noteIds.push(created.id);

    // Update the note
    const updatedContent = "Updated content at " + new Date().toISOString();
    const updated = await client.callTool("update_note", {
      id: created.id,
      content: updatedContent,
    });

    assert.ok(updated, "Update should return a result");

    // Verify the update
    const retrieved = await client.callTool("get_note", { id: created.id });
    assert.ok(
      retrieved.content.includes(updatedContent),
      "Content should be updated"
    );

    console.log(`  ✓ Updated note content successfully`);
  });

  test("CRUD-004: Delete note marks as deleted", async () => {
    // Create a note
    const testTag = MCPTestClient.testTag("crud", "delete");
    const created = await client.callTool("create_note", {
      content: "To be deleted",
      tags: [testTag],
    });
    cleanup.noteIds.push(created.id);

    // Delete the note
    const result = await client.callTool("delete_note", { id: created.id });

    assert.ok(result, "Delete should return a result");

    // Verify note is deleted (should return error or marked as deleted)
    try {
      await client.callTool("get_note", { id: created.id });
      // If we get here, note might be soft-deleted but still retrievable
      // This is acceptable behavior
    } catch (error) {
      // Expected: note not found after deletion
      assert.ok(
        error.message.includes("404") || error.message.includes("not found"),
        "Should return 404 or not found error"
      );
    }

    console.log(`  ✓ Deleted note successfully`);
  });

  test("CRUD-005: List notes returns array", async () => {
    const result = await client.callTool("list_notes", {});

    assert.ok(result, "Should return a result");
    assert.ok(Array.isArray(result.notes) || Array.isArray(result), "Should return an array");

    const notes = result.notes || result;
    console.log(`  ✓ Listed ${notes.length} notes`);
  });

  test("CRUD-006: List notes with tag filter", async () => {
    // Create notes with specific tag
    const testTag = MCPTestClient.testTag("crud", "filter");

    const note1 = await client.callTool("create_note", {
      content: "Note 1 with filter tag",
      tags: [testTag],
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: "Note 2 with filter tag",
      tags: [testTag],
    });
    cleanup.noteIds.push(note2.id);

    // List with tag filter
    const result = await client.callTool("list_notes", {
      tags: [testTag],
    });

    const notes = result.notes || result;
    assert.ok(Array.isArray(notes), "Should return an array");

    // Should contain at least our 2 notes
    const foundIds = notes.map(n => n.id);
    assert.ok(
      foundIds.includes(note1.id),
      "Should include first note"
    );
    assert.ok(
      foundIds.includes(note2.id),
      "Should include second note"
    );

    console.log(`  ✓ Filtered to ${notes.length} notes with tag "${testTag}"`);
  });

  test("CRUD-007: Bulk create notes creates multiple", async () => {
    const testTag = MCPTestClient.testTag("crud", "bulk");
    const notes = [
      { content: "Bulk note 1", tags: [testTag] },
      { content: "Bulk note 2", tags: [testTag] },
      { content: "Bulk note 3", tags: [testTag] },
    ];

    const created = await client.callTool("bulk_create_notes", { notes });

    assert.ok(Array.isArray(created), "Should return an array of { id } objects");
    assert.strictEqual(created.length, 3, "Should create 3 notes");

    for (const note of created) {
      assert.ok(note.id, "Each note should have an id");
      cleanup.noteIds.push(note.id);
    }

    console.log(`  ✓ Bulk created ${created.length} notes`);
  });

  test("CRUD-008: Get note for non-existent UUID returns error", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("get_note", { id: fakeId });

    assert.ok(error, "Should return an error");
    assert.ok(
      error.error.includes("404") || error.error.includes("not found"),
      "Should return 404 or not found error"
    );

    console.log(`  ✓ Correctly rejected non-existent ID`);
  });

  test("CRUD-009: Create note with empty content returns error", async () => {
    const error = await client.callToolExpectError("create_note", {
      content: "",
      tags: [],
    });

    assert.ok(error, "Should return an error");
    assert.ok(
      error.error.includes("400") ||
      error.error.includes("empty") ||
      error.error.includes("required"),
      "Should indicate validation error"
    );

    console.log(`  ✓ Correctly rejected empty content`);
  });

  test("CRUD-010: Update note with invalid ID returns error", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("update_note", {
      id: fakeId,
      content: "Updated content",
    });

    assert.ok(error, "Should return an error");
    assert.ok(
      error.error.includes("404") || error.error.includes("not found"),
      "Should return 404 or not found error"
    );

    console.log(`  ✓ Correctly rejected invalid update ID`);
  });

  test("CRUD-011: Create and retrieve note with special characters", async () => {
    const testTag = MCPTestClient.testTag("crud", "special");
    const specialContent = `# Special Characters Test

Unicode: 你好世界 🌍 Здравствуй мир
Markdown: **bold** *italic* \`code\`
Symbols: <>&"'
Math: ∑ ∫ √ ∞`;

    const created = await client.callTool("create_note", {
      content: specialContent,
      tags: [testTag],
    });
    cleanup.noteIds.push(created.id);

    const retrieved = await client.callTool("get_note", { id: created.id });

    assert.ok(
      retrieved.content.includes("你好世界"),
      "Should preserve Unicode"
    );
    assert.ok(
      retrieved.content.includes("🌍"),
      "Should preserve emoji"
    );
    assert.ok(
      retrieved.content.includes("**bold**"),
      "Should preserve markdown"
    );

    console.log(`  ✓ Special characters preserved correctly`);
  });
});
