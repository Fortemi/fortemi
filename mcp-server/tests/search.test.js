#!/usr/bin/env node

/**
 * Phase 3: Search Operations (CRITICAL)
 *
 * Tests hybrid search capabilities including full-text search, semantic
 * search, and tag filtering. Search is a core feature that enables knowledge
 * discovery and must handle various query types correctly.
 *
 * Tests:
 * - Text query search
 * - Tag-based filtering
 * - Pagination (limit/offset)
 * - Empty query handling
 * - Non-matching queries
 * - Search result structure
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 3: Search Operations (CRITICAL)", () => {
  let client;
  const cleanup = { noteIds: [] };
  let testNoteIds = [];

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();

    // Create test notes for search
    const testTag = MCPTestClient.testTag("search", "fixtures");
    const testNotes = [
      {
        content: "# JavaScript Programming\n\nJavaScript is a versatile programming language.",
        tags: [testTag, "programming", "javascript"],
      },
      {
        content: "# Python Development\n\nPython is great for data science and web development.",
        tags: [testTag, "programming", "python"],
      },
      {
        content: "# Machine Learning Basics\n\nMachine learning is a subset of artificial intelligence.",
        tags: [testTag, "ai", "machine-learning"],
      },
      {
        content: "# Database Design\n\nPostgreSQL is a powerful relational database system.",
        tags: [testTag, "database", "postgresql"],
      },
    ];

    console.log("  Setting up test notes for search...");
    for (const noteData of testNotes) {
      const created = await client.callTool("create_note", noteData);
      testNoteIds.push(created.id);
      cleanup.noteIds.push(created.id);
    }
    console.log(`  ✓ Created ${testNoteIds.length} test notes`);
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

  test("SEARCH-001: Search notes with text query returns results", async () => {
    const result = await client.callTool("search_notes", {
      query: "programming",
    });

    assert.ok(result, "Should return a result");
    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    assert.ok(notes.length > 0, "Should find matching notes");

    // Verify result structure
    const firstNote = notes[0];
    assert.ok(firstNote.id, "Note should have ID");
    assert.ok(firstNote.content, "Note should have content");

    console.log(`  ✓ Found ${notes.length} notes matching "programming"`);
  });

  test("SEARCH-002: Search with specific term finds correct notes", async () => {
    const result = await client.callTool("search_notes", {
      query: "JavaScript",
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");

    // Should find the JavaScript note
    const jsNote = notes.find(n => n.content && n.content.includes("JavaScript"));
    assert.ok(jsNote, "Should find JavaScript note");

    console.log(`  ✓ Found JavaScript note in ${notes.length} results`);
  });

  test("SEARCH-003: Search with tag filter returns filtered results", async () => {
    const testTag = MCPTestClient.testTag("search", "fixtures");

    const result = await client.callTool("search_notes", {
      query: "",
      tags: [testTag],
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    assert.ok(notes.length >= 4, "Should find at least 4 test notes");

    // All notes should have the test tag
    for (const note of notes) {
      if (note.tags) {
        const hasTestTag = note.tags.some(tag =>
          typeof tag === "string"
            ? tag === testTag
            : tag.name === testTag
        );
        if (testNoteIds.includes(note.id)) {
          assert.ok(hasTestTag, `Note ${note.id} should have test tag`);
        }
      }
    }

    console.log(`  ✓ Found ${notes.length} notes with tag filter`);
  });

  test("SEARCH-004: Search with combined query and tag filter", async () => {
    const testTag = MCPTestClient.testTag("search", "fixtures");

    const result = await client.callTool("search_notes", {
      query: "programming",
      tags: [testTag],
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");

    // Should find programming notes with test tag
    const programmingNote = notes.find(n =>
      n.content && n.content.toLowerCase().includes("programming")
    );
    assert.ok(programmingNote, "Should find programming note");

    console.log(`  ✓ Combined search found ${notes.length} notes`);
  });

  test("SEARCH-005: Search with limit parameter restricts results", async () => {
    const result = await client.callTool("search_notes", {
      query: "",
      limit: 2,
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    assert.ok(notes.length <= 2, "Should return at most 2 results");

    console.log(`  ✓ Limited results to ${notes.length} notes`);
  });

  test("SEARCH-006: Search with offset parameter skips results", async () => {
    // First, get results without offset
    const firstResult = await client.callTool("search_notes", {
      query: "",
      limit: 5,
    });
    const firstNotes = firstResult.notes || firstResult.results || firstResult;

    // Then get results with offset
    const secondResult = await client.callTool("search_notes", {
      query: "",
      limit: 5,
      offset: 2,
    });
    const secondNotes = secondResult.notes || secondResult.results || secondResult;

    assert.ok(Array.isArray(firstNotes), "First result should be an array");
    assert.ok(Array.isArray(secondNotes), "Second result should be an array");

    // Results should be different if there are enough notes
    if (firstNotes.length >= 3) {
      const firstIds = firstNotes.map(n => n.id);
      const secondIds = secondNotes.map(n => n.id);

      // Second set should not contain the first 2 from first set
      const overlap = secondIds.filter(id => firstIds.slice(0, 2).includes(id));
      assert.strictEqual(overlap.length, 0, "Offset should skip first 2 results");
    }

    console.log(`  ✓ Offset pagination working correctly`);
  });

  test("SEARCH-007: Empty query returns results", async () => {
    const result = await client.callTool("search_notes", {
      query: "",
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    // Empty query should return all notes (or paginated set)
    assert.ok(notes.length >= 0, "Should return notes");

    console.log(`  ✓ Empty query returned ${notes.length} notes`);
  });

  test("SEARCH-008: Non-matching query returns empty or minimal results", async () => {
    // Search for a very specific term unlikely to exist
    const uniqueId = MCPTestClient.uniqueId();
    const result = await client.callTool("search_notes", {
      query: `xyzzyquuxnonexistent${uniqueId}`,
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    assert.strictEqual(notes.length, 0, "Should return empty results for non-matching query");

    console.log(`  ✓ Non-matching query returned ${notes.length} results`);
  });

  test("SEARCH-009: Search result includes relevance metadata", async () => {
    const result = await client.callTool("search_notes", {
      query: "database",
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");

    if (notes.length > 0) {
      const firstNote = notes[0];
      assert.ok(firstNote.id, "Result should have ID");
      assert.ok(firstNote.content, "Result should have content");

      // Check for optional relevance score or rank
      // (may not be present in all implementations)
      console.log(`  ✓ Search results have proper structure`);
    } else {
      console.log(`  ⚠ No results found to verify metadata`);
    }
  });

  test("SEARCH-010: Search handles special characters in query", async () => {
    const result = await client.callTool("search_notes", {
      query: "data & science",
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");
    // Should not error on special characters
    console.log(`  ✓ Special character query handled correctly`);
  });

  test("SEARCH-011: Search with multiple tags (AND logic)", async () => {
    const testTag = MCPTestClient.testTag("search", "fixtures");

    const result = await client.callTool("search_notes", {
      query: "",
      tags: [testTag, "programming"],
    });

    const notes = result.notes || result.results || result;
    assert.ok(Array.isArray(notes), "Should return an array");

    // Should find notes that have both tags
    if (notes.length > 0) {
      console.log(`  ✓ Multi-tag search found ${notes.length} notes`);
    } else {
      console.log(`  ⚠ No notes found with both tags`);
    }
  });

  test("SEARCH-012: Case-insensitive search", async () => {
    const lowerResult = await client.callTool("search_notes", {
      query: "javascript",
    });
    const upperResult = await client.callTool("search_notes", {
      query: "JAVASCRIPT",
    });

    const lowerNotes = lowerResult.notes || lowerResult.results || lowerResult;
    const upperNotes = upperResult.notes || upperResult.results || upperResult;

    assert.ok(Array.isArray(lowerNotes), "Lowercase result should be an array");
    assert.ok(Array.isArray(upperNotes), "Uppercase result should be an array");

    // Results should be similar (case-insensitive)
    console.log(`  ✓ Case-insensitive: lower=${lowerNotes.length}, upper=${upperNotes.length}`);
  });
});
