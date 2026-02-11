#!/usr/bin/env node

/**
 * Phase 5: Collections
 *
 * Tests collection management functionality including creating collections,
 * adding/removing notes, and listing collections. Collections enable
 * organizing notes into logical groups beyond tag-based categorization.
 *
 * Tests:
 * - Create collections with name and description
 * - List all collections
 * - Add notes to collections
 * - List notes within a collection
 * - Remove notes from collections
 * - Delete collections
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 5: Collections", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Cleanup collections first (they may reference notes)
    console.log(`  Cleaning up ${cleanup.collectionIds.length} collections...`);
    for (const id of cleanup.collectionIds) {
      try {
        await client.callTool("delete_collection", { id });
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    // Then cleanup notes
    console.log(`  Cleaning up ${cleanup.noteIds.length} notes...`);
    for (const id of cleanup.noteIds) {
      try {
        await client.callTool("delete_note", { id });
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    await client.close();
  });

  test("COLL-001: Create collection with name and description", async () => {
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collectionName = `Test Collection ${uniqueId}`;
    const description = "A test collection for automated testing";

    const result = await client.callTool("create_collection", {
      name: collectionName,
      description: description,
    });

    assert.ok(result, "Should return a result");
    assert.ok(result.id, "Result should contain collection ID");
    assert.match(
      result.id,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/,
      "ID should be a valid UUID"
    );

    cleanup.collectionIds.push(result.id);
    console.log(`  ✓ Created collection: ${result.id}`);
  });

  test("COLL-002: List collections returns array", async () => {
    const result = await client.callTool("list_collections", {});

    assert.ok(result, "Should return a result");
    const collections = result.collections || result;
    assert.ok(Array.isArray(collections), "Should return an array");

    console.log(`  ✓ Found ${collections.length} collections`);
  });

  test("COLL-003: Created collection appears in list", async () => {
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collectionName = `Unique Collection ${uniqueId}`;

    // Create collection
    const created = await client.callTool("create_collection", {
      name: collectionName,
      description: "For list verification",
    });
    cleanup.collectionIds.push(created.id);

    // List collections
    const result = await client.callTool("list_collections", {});
    const collections = result.collections || result;

    // Find our collection
    const found = collections.find(c => c.id === created.id);
    assert.ok(found, "Created collection should appear in list");
    assert.strictEqual(found.name, collectionName, "Collection name should match");

    console.log(`  ✓ Collection found in list of ${collections.length}`);
  });

  test("COLL-004: Add note to collection", async () => {
    // Create a collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Collection for Note ${uniqueId}`,
      description: "Collection for add note test",
    });
    cleanup.collectionIds.push(collection.id);

    // Create a note
    const testTag = MCPTestClient.testTag("collection", "add");
    const note = await client.callTool("create_note", {
      content: "Note to add to collection",
      tags: [testTag],
    });
    cleanup.noteIds.push(note.id);

    // Add note to collection
    const result = await client.callTool("move_note_to_collection", {
      collection_id: collection.id,
      note_id: note.id,
    });

    assert.ok(result, "Should return a result");
    console.log(`  ✓ Added note ${note.id} to collection ${collection.id}`);
  });

  test("COLL-005: List notes in collection", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Collection with Notes ${uniqueId}`,
      description: "For listing notes",
    });
    cleanup.collectionIds.push(collection.id);

    // Create and add multiple notes
    const testTag = MCPTestClient.testTag("collection", "list");
    const noteIds = [];

    for (let i = 0; i < 3; i++) {
      const note = await client.callTool("create_note", {
        content: `Note ${i} in collection`,
        tags: [testTag],
      });
      cleanup.noteIds.push(note.id);
      noteIds.push(note.id);

      await client.callTool("move_note_to_collection", {
        collection_id: collection.id,
        note_id: note.id,
      });
    }

    // List notes in collection
    const result = await client.callTool("get_collection_notes", {
      id: collection.id,
    });

    const notes = result.notes || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    assert.ok(notes.length >= 3, `Should have at least 3 notes, got ${notes.length}`);

    // Verify our notes are in the collection
    const foundIds = notes.map(n => n.id);
    for (const noteId of noteIds) {
      assert.ok(
        foundIds.includes(noteId),
        `Note ${noteId} should be in collection`
      );
    }

    console.log(`  ✓ Collection contains ${notes.length} notes`);
  });

  test("COLL-006: Remove note from collection", async () => {
    // Create collection and note
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Collection Remove Test ${uniqueId}`,
      description: "For remove note test",
    });
    cleanup.collectionIds.push(collection.id);

    const testTag = MCPTestClient.testTag("collection", "remove");
    const note = await client.callTool("create_note", {
      content: "Note to remove from collection",
      tags: [testTag],
    });
    cleanup.noteIds.push(note.id);

    // Add note to collection
    await client.callTool("move_note_to_collection", {
      collection_id: collection.id,
      note_id: note.id,
    });

    // Remove note from collection by moving to uncategorized (omit collection_id)
    const result = await client.callTool("move_note_to_collection", {
      note_id: note.id,
    });

    assert.ok(result, "Should return a result");

    // Verify note is removed
    const listResult = await client.callTool("get_collection_notes", {
      id: collection.id,
    });
    const notes = listResult.notes || listResult;
    const foundIds = notes.map(n => n.id);

    assert.ok(
      !foundIds.includes(note.id),
      "Note should be removed from collection"
    );

    console.log(`  ✓ Removed note from collection`);
  });

  test("COLL-007: Delete collection", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Collection to Delete ${uniqueId}`,
      description: "For deletion test",
    });
    cleanup.collectionIds.push(collection.id);

    // Delete collection
    const result = await client.callTool("delete_collection", {
      id: collection.id,
    });

    assert.ok(result, "Delete should return a result");

    // Verify collection is deleted
    const listResult = await client.callTool("list_collections", {});
    const collections = listResult.collections || listResult;
    const found = collections.find(c => c.id === collection.id);

    assert.ok(!found, "Deleted collection should not appear in list");

    console.log(`  ✓ Deleted collection successfully`);
  });

  test("COLL-007b: Delete non-empty collection without force flag fails", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Non-Empty Collection ${uniqueId}`,
      description: "For force delete test",
    });
    cleanup.collectionIds.push(collection.id);

    // Create note in collection
    const note = await client.callTool("create_note", {
      content: "Test note for force delete",
      collection_id: collection.id,
    });
    cleanup.noteIds.push(note.id);

    // Try to delete collection without force flag - should fail
    let errorThrown = false;
    try {
      await client.callTool("delete_collection", {
        id: collection.id,
      });
    } catch (error) {
      errorThrown = true;
      assert.ok(
        error.message.includes("force=true") || error.message.includes("not empty"),
        "Error should mention force flag or non-empty collection"
      );
    }

    assert.ok(errorThrown, "Should throw error when deleting non-empty collection without force");

    // Verify collection still exists
    const listResult = await client.callTool("list_collections", {});
    const collections = listResult.collections || listResult;
    const found = collections.find(c => c.id === collection.id);
    assert.ok(found, "Collection should still exist after failed delete");

    console.log(`  ✓ Non-empty collection delete properly rejected`);
  });

  test("COLL-007c: Delete non-empty collection with force flag succeeds", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Force Delete Collection ${uniqueId}`,
      description: "For force delete success test",
    });
    cleanup.collectionIds.push(collection.id);

    // Create note in collection
    const note = await client.callTool("create_note", {
      content: "Test note for force delete",
      collection_id: collection.id,
    });
    cleanup.noteIds.push(note.id);

    // Delete collection with force flag - should succeed
    const result = await client.callTool("delete_collection", {
      id: collection.id,
      force: true,
    });

    assert.ok(result, "Delete should return a result");

    // Verify collection is deleted
    const listResult = await client.callTool("list_collections", {});
    const collections = listResult.collections || listResult;
    const found = collections.find(c => c.id === collection.id);
    assert.ok(!found, "Collection should be deleted with force flag");

    // Verify note was moved to uncategorized (collection_id = null)
    const noteResult = await client.callTool("get_note", { id: note.id });
    assert.strictEqual(
      noteResult.collection_id,
      null,
      "Note should be moved to uncategorized"
    );

    console.log(`  ✓ Force delete of non-empty collection succeeded`);
  });

  test("COLL-008: Get collection details", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collectionName = `Collection Details ${uniqueId}`;
    const description = "Detailed collection information";

    const created = await client.callTool("create_collection", {
      name: collectionName,
      description: description,
    });
    cleanup.collectionIds.push(created.id);

    // Get collection details
    const result = await client.callTool("get_collection", {
      id: created.id,
    });

    assert.ok(result, "Should return collection details");
    assert.strictEqual(result.id, created.id, "ID should match");
    assert.strictEqual(result.name, collectionName, "Name should match");
    assert.ok(result.description, "Should have description");

    console.log(`  ✓ Retrieved collection details`);
  });

  test("COLL-009: Collection with empty description", async () => {
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const created = await client.callTool("create_collection", {
      name: `Minimal Collection ${uniqueId}`,
      description: "",
    });
    cleanup.collectionIds.push(created.id);

    assert.ok(created.id, "Should create collection with empty description");
    console.log(`  ✓ Created collection with empty description`);
  });

  test("COLL-010: Move note between collections", async () => {
    // Create two collections
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const coll1 = await client.callTool("create_collection", {
      name: `Collection A ${uniqueId}`,
      description: "First collection",
    });
    cleanup.collectionIds.push(coll1.id);

    const coll2 = await client.callTool("create_collection", {
      name: `Collection B ${uniqueId}`,
      description: "Second collection",
    });
    cleanup.collectionIds.push(coll2.id);

    // Create one note
    const testTag = MCPTestClient.testTag("collection", "move");
    const note = await client.callTool("create_note", {
      content: "Note to move between collections",
      tags: [testTag],
    });
    cleanup.noteIds.push(note.id);

    // Move note to first collection
    await client.callTool("move_note_to_collection", {
      collection_id: coll1.id,
      note_id: note.id,
    });

    // Verify note is in first collection
    const list1 = await client.callTool("get_collection_notes", {
      id: coll1.id,
    });
    const notes1 = list1.notes || list1;
    assert.ok(notes1.find(n => n.id === note.id), "Note should be in first collection");

    // Move note to second collection
    await client.callTool("move_note_to_collection", {
      collection_id: coll2.id,
      note_id: note.id,
    });

    // Verify note is now in second collection
    const list2 = await client.callTool("get_collection_notes", {
      id: coll2.id,
    });
    const notes2 = list2.notes || list2;
    assert.ok(notes2.find(n => n.id === note.id), "Note should be in second collection");

    console.log(`  ✓ Note successfully moved between collections`);
  });

  test("COLL-011: Delete collection with notes does not delete notes", async () => {
    // Create collection and note
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const collection = await client.callTool("create_collection", {
      name: `Collection Delete Test ${uniqueId}`,
      description: "For cascade delete verification",
    });
    cleanup.collectionIds.push(collection.id);

    const testTag = MCPTestClient.testTag("collection", "cascade");
    const note = await client.callTool("create_note", {
      content: "Note in collection to delete",
      tags: [testTag],
    });
    cleanup.noteIds.push(note.id);

    // Add note to collection
    await client.callTool("move_note_to_collection", {
      collection_id: collection.id,
      note_id: note.id,
    });

    // Delete collection
    await client.callTool("delete_collection", {
      id: collection.id,
    });

    // Verify note still exists
    const retrieved = await client.callTool("get_note", { id: note.id });
    assert.ok(retrieved, "Note should still exist after collection deletion");
    const noteId = retrieved.note?.id || retrieved.id;
    assert.strictEqual(noteId, note.id, "Note ID should match");

    console.log(`  ✓ Note survived collection deletion`);
  });

  test("COLL-012: Update collection name and description", async () => {
    // Create collection
    const uniqueId = MCPTestClient.uniqueId().slice(0, 8);
    const originalName = `Original Name ${uniqueId}`;
    const created = await client.callTool("create_collection", {
      name: originalName,
      description: "Original description",
    });
    cleanup.collectionIds.push(created.id);

    // Update collection
    const updatedName = `Updated Name ${uniqueId}`;
    const updatedDescription = "Updated description";
    await client.callTool("update_collection", {
      id: created.id,
      name: updatedName,
      description: updatedDescription,
    });

    // Verify update
    const retrieved = await client.callTool("get_collection", {
      id: created.id,
    });

    assert.strictEqual(retrieved.name, updatedName, "Name should be updated");
    assert.strictEqual(
      retrieved.description,
      updatedDescription,
      "Description should be updated"
    );

    console.log(`  ✓ Collection updated successfully`);
  });
});
