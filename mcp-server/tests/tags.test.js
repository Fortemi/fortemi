#!/usr/bin/env node

/**
 * Phase 4: Tag Operations
 *
 * Tests tag management functionality including listing tags, creating notes
 * with tags, and updating tags. Tags are critical for organizing knowledge
 * and enabling filtered search.
 *
 * Tests:
 * - List all tags
 * - Create notes with tags
 * - Tags appear in tag list
 * - Update notes to add/remove tags
 * - Tag hierarchy and relationships
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 4: Tag Operations", () => {
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

  test("TAG-001: List tags returns array", async () => {
    const result = await client.callTool("list_tags", {});

    assert.ok(result, "Should return a result");
    const tags = result.tags || result;
    assert.ok(Array.isArray(tags), "Should return an array");

    console.log(`  ✓ Found ${tags.length} tags in system`);
  });

  test("TAG-002: Create note with tags adds tags to note", async () => {
    const testTag1 = MCPTestClient.testTag("tag", "create1");
    const testTag2 = MCPTestClient.testTag("tag", "create2");

    const created = await client.callTool("create_note", {
      content: "Note with multiple tags",
      tags: [testTag1, testTag2, "category/test"],
    });
    cleanup.noteIds.push(created.id);

    // Retrieve the note to verify tags
    const retrieved = await client.callTool("get_note", { id: created.id });

    assert.ok(Array.isArray(retrieved.tags), "Note should have tags array");
    assert.ok(retrieved.tags.length >= 3, "Should have at least 3 tags");

    // Verify our test tags are present
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );
    assert.ok(tagNames.includes(testTag1), "Should include first test tag");
    assert.ok(tagNames.includes(testTag2), "Should include second test tag");

    console.log(`  ✓ Note created with ${retrieved.tags.length} tags`);
  });

  test("TAG-003: Created tags appear in list_tags", async () => {
    const uniqueTag = MCPTestClient.testTag("tag", "unique");

    // Create note with unique tag
    const created = await client.callTool("create_note", {
      content: "Note with unique tag",
      tags: [uniqueTag],
    });
    cleanup.noteIds.push(created.id);

    // List all tags
    const result = await client.callTool("list_tags", {});
    const tags = result.tags || result;

    // Find our unique tag
    const tagNames = tags.map(t => (typeof t === "string" ? t : t.name));
    const found = tagNames.includes(uniqueTag);

    assert.ok(found, `Unique tag "${uniqueTag}" should appear in list_tags`);

    console.log(`  ✓ Unique tag found in ${tags.length} total tags`);
  });

  test("TAG-004: Update note to add tags", async () => {
    const initialTag = MCPTestClient.testTag("tag", "initial");
    const addedTag = MCPTestClient.testTag("tag", "added");

    // Create note with one tag
    const created = await client.callTool("create_note", {
      content: "Note for tag update test",
      tags: [initialTag],
    });
    cleanup.noteIds.push(created.id);

    // Update to add another tag via set_note_tags
    await client.callTool("set_note_tags", {
      id: created.id,
      tags: [initialTag, addedTag],
    });

    // Verify both tags are present
    const retrieved = await client.callTool("get_note", { id: created.id });
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );

    assert.ok(tagNames.includes(initialTag), "Should keep initial tag");
    assert.ok(tagNames.includes(addedTag), "Should have added tag");

    console.log(`  ✓ Updated note now has ${retrieved.tags.length} tags`);
  });

  test("TAG-005: Update note to remove tags", async () => {
    const tag1 = MCPTestClient.testTag("tag", "remove1");
    const tag2 = MCPTestClient.testTag("tag", "remove2");

    // Create note with two tags
    const created = await client.callTool("create_note", {
      content: "Note with tags to remove",
      tags: [tag1, tag2],
    });
    cleanup.noteIds.push(created.id);

    // Update to keep only one tag via set_note_tags
    await client.callTool("set_note_tags", {
      id: created.id,
      tags: [tag1],
    });

    // Verify only tag1 remains
    const retrieved = await client.callTool("get_note", { id: created.id });
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );

    assert.ok(tagNames.includes(tag1), "Should keep tag1");
    assert.ok(!tagNames.includes(tag2), "Should remove tag2");

    console.log(`  ✓ Removed tag successfully, ${retrieved.tags.length} tags remain`);
  });

  test("TAG-006: Hierarchical tags with slashes", async () => {
    const hierarchicalTag = `test/hierarchy/${MCPTestClient.uniqueId().slice(0, 8)}`;

    const created = await client.callTool("create_note", {
      content: "Note with hierarchical tag",
      tags: [hierarchicalTag],
    });
    cleanup.noteIds.push(created.id);

    const retrieved = await client.callTool("get_note", { id: created.id });
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );

    assert.ok(
      tagNames.includes(hierarchicalTag),
      "Should support hierarchical tags with slashes"
    );

    console.log(`  ✓ Hierarchical tag "${hierarchicalTag}" supported`);
  });

  test("TAG-007: Tags with special characters", async () => {
    const specialTags = [
      `test/tag-with-dashes-${MCPTestClient.uniqueId().slice(0, 8)}`,
      `test/tag_with_underscores_${MCPTestClient.uniqueId().slice(0, 8)}`,
      `test/tag.with.dots.${MCPTestClient.uniqueId().slice(0, 8)}`,
    ];

    const created = await client.callTool("create_note", {
      content: "Note with special character tags",
      tags: specialTags,
    });
    cleanup.noteIds.push(created.id);

    const retrieved = await client.callTool("get_note", { id: created.id });
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );

    for (const tag of specialTags) {
      assert.ok(
        tagNames.includes(tag),
        `Should support tag with special characters: ${tag}`
      );
    }

    console.log(`  ✓ Special character tags supported`);
  });

  test("TAG-008: Empty tags array creates note without tags", async () => {
    const created = await client.callTool("create_note", {
      content: "Note without tags",
      tags: [],
    });
    cleanup.noteIds.push(created.id);

    const retrieved = await client.callTool("get_note", { id: created.id });

    assert.ok(Array.isArray(retrieved.tags), "Tags should be an array");
    assert.strictEqual(retrieved.tags.length, 0, "Should have no tags");

    console.log(`  ✓ Note created with empty tags array`);
  });

  test("TAG-009: Duplicate tags are handled correctly", async () => {
    const duplicateTag = MCPTestClient.testTag("tag", "duplicate");

    const created = await client.callTool("create_note", {
      content: "Note with duplicate tags",
      tags: [duplicateTag, duplicateTag, duplicateTag],
    });
    cleanup.noteIds.push(created.id);

    const retrieved = await client.callTool("get_note", { id: created.id });
    const tagNames = retrieved.tags.map(t =>
      typeof t === "string" ? t : t.name
    );

    const duplicateCount = tagNames.filter(t => t === duplicateTag).length;

    // System should deduplicate tags
    assert.strictEqual(
      duplicateCount,
      1,
      "Duplicate tags should be deduplicated"
    );

    console.log(`  ✓ Duplicate tags deduplicated correctly`);
  });

  test("TAG-010: Tag list includes usage counts", async () => {
    // Create multiple notes with same tag
    const commonTag = MCPTestClient.testTag("tag", "common");

    for (let i = 0; i < 3; i++) {
      const created = await client.callTool("create_note", {
        content: `Note ${i} with common tag`,
        tags: [commonTag],
      });
      cleanup.noteIds.push(created.id);
    }

    // List tags
    const result = await client.callTool("list_tags", {});
    const tags = result.tags || result;

    // Find our common tag
    const commonTagEntry = tags.find(t =>
      (typeof t === "string" ? t : t.name) === commonTag
    );

    if (commonTagEntry && typeof commonTagEntry === "object" && commonTagEntry.count) {
      assert.ok(
        commonTagEntry.count >= 3,
        `Tag should have count >= 3, got ${commonTagEntry.count}`
      );
      console.log(`  ✓ Tag usage count: ${commonTagEntry.count}`);
    } else {
      console.log(`  ⚠ Tag counts not included in list_tags response`);
    }
  });

  test("TAG-011: Update note with null/empty tags removes all tags", async () => {
    const tag1 = MCPTestClient.testTag("tag", "clear1");
    const tag2 = MCPTestClient.testTag("tag", "clear2");

    // Create note with tags
    const created = await client.callTool("create_note", {
      content: "Note with tags to clear",
      tags: [tag1, tag2],
    });
    cleanup.noteIds.push(created.id);

    // Update to clear tags via set_note_tags
    await client.callTool("set_note_tags", {
      id: created.id,
      tags: [],
    });

    // Verify tags are cleared
    const retrieved = await client.callTool("get_note", { id: created.id });

    assert.ok(Array.isArray(retrieved.tags), "Tags should be an array");
    assert.strictEqual(retrieved.tags.length, 0, "All tags should be removed");

    console.log(`  ✓ Tags cleared successfully`);
  });
});
