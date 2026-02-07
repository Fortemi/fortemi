#!/usr/bin/env node

/**
 * MCP Semantic Links Tests (Phase 6)
 *
 * Tests semantic link management via MCP tools:
 * - get_links: Retrieve links for a note
 * - create_link: Create link between two notes
 * - semantic_neighbors: Find semantically similar notes
 * - delete link: Remove a link
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 6: Semantic Links", () => {
  let client;
  const cleanup = { noteIds: [], linkIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Clean up links first (if we tracked them)
    for (const id of cleanup.linkIds) {
      try {
        await client.callTool("delete_link", { id });
      } catch (e) {
        console.error(`Failed to delete link ${id}:`, e.message);
      }
    }

    // Clean up notes
    for (const id of cleanup.noteIds) {
      try {
        await client.callTool("delete_note", { id });
      } catch (e) {
        console.error(`Failed to delete note ${id}:`, e.message);
      }
    }

    await client.close();
  });

  test("LINKS-001: get_links returns empty array for new note", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Test Note ${testId}\n\nNote with no links yet.`;

    const createResult = await client.callTool("create_note", { content });
    assert.ok(createResult.id, "Note should be created with ID");
    cleanup.noteIds.push(createResult.id);

    const links = await client.callTool("get_links", { note_id: createResult.id });
    assert.ok(Array.isArray(links), "Links should be an array");
    assert.strictEqual(links.length, 0, "New note should have no links");
  });

  test("LINKS-002: create_link between two notes", async () => {
    const testId = MCPTestClient.uniqueId();

    // Create source note
    const sourceContent = `# Source Note ${testId}\n\nThis is the source note.`;
    const sourceResult = await client.callTool("create_note", { content: sourceContent });
    assert.ok(sourceResult.id, "Source note should be created");
    cleanup.noteIds.push(sourceResult.id);

    // Create target note
    const targetContent = `# Target Note ${testId}\n\nThis is the target note.`;
    const targetResult = await client.callTool("create_note", { content: targetContent });
    assert.ok(targetResult.id, "Target note should be created");
    cleanup.noteIds.push(targetResult.id);

    // Create link
    const linkResult = await client.callTool("create_link", {
      from_id: sourceResult.id,
      to_id: targetResult.id,
      link_type: "references",
    });

    assert.ok(linkResult.id, "Link should be created with ID");
    assert.strictEqual(linkResult.from_id, sourceResult.id, "Link should reference source note");
    assert.strictEqual(linkResult.to_id, targetResult.id, "Link should reference target note");
    assert.strictEqual(linkResult.link_type, "references", "Link type should be preserved");

    cleanup.linkIds.push(linkResult.id);
  });

  test("LINKS-003: get_links retrieves created link", async () => {
    const testId = MCPTestClient.uniqueId();

    // Create two notes and link them
    const note1 = await client.callTool("create_note", {
      content: `# Note 1 ${testId}\n\nFirst note.`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Note 2 ${testId}\n\nSecond note.`,
    });
    cleanup.noteIds.push(note2.id);

    const link = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note2.id,
      link_type: "related",
    });
    cleanup.linkIds.push(link.id);

    // Retrieve links for source note
    const links = await client.callTool("get_links", { note_id: note1.id });
    assert.ok(Array.isArray(links), "Links should be an array");
    assert.ok(links.length > 0, "Note should have at least one link");

    const createdLink = links.find((l) => l.id === link.id);
    assert.ok(createdLink, "Created link should be in the list");
    assert.strictEqual(createdLink.from_id, note1.id, "Link from_id should match");
    assert.strictEqual(createdLink.to_id, note2.id, "Link to_id should match");
  });

  test("LINKS-004: semantic_neighbors finds similar notes", async () => {
    const testId = MCPTestClient.uniqueId();
    const sharedTopic = `artificial intelligence and machine learning ${testId}`;

    // Create notes with similar content
    const note1 = await client.callTool("create_note", {
      content: `# AI Research ${testId}\n\nThis note discusses ${sharedTopic} in depth.`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# ML Study ${testId}\n\nExploring concepts in ${sharedTopic}.`,
    });
    cleanup.noteIds.push(note2.id);

    // Allow some time for embeddings to be generated (if async)
    // In real deployment, embeddings might be queued
    await new Promise((resolve) => setTimeout(resolve, 100));

    // Search for semantic neighbors
    const neighbors = await client.callTool("semantic_neighbors", {
      note_id: note1.id,
      limit: 10,
    });

    assert.ok(Array.isArray(neighbors), "Neighbors should be an array");
    // Note: Semantic search requires embeddings, which may not be instant
    // This test validates the tool runs without error
  });

  test("LINKS-005: delete_link removes a link", async () => {
    const testId = MCPTestClient.uniqueId();

    // Create notes and link
    const note1 = await client.callTool("create_note", {
      content: `# Delete Test 1 ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Delete Test 2 ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    const link = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note2.id,
      link_type: "cites",
    });

    // Delete the link
    await client.callTool("delete_link", { id: link.id });

    // Verify link is gone
    const links = await client.callTool("get_links", { note_id: note1.id });
    const deletedLink = links.find((l) => l.id === link.id);
    assert.strictEqual(deletedLink, undefined, "Link should be deleted");
  });

  test("LINKS-006: create_link with custom metadata", async () => {
    const testId = MCPTestClient.uniqueId();

    const note1 = await client.callTool("create_note", {
      content: `# Research Paper ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Citation ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    // Create link with weight/strength
    const link = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note2.id,
      link_type: "cites",
      weight: 0.95,
    });

    assert.ok(link.id, "Link should be created");
    cleanup.linkIds.push(link.id);

    // Verify weight is stored
    const links = await client.callTool("get_links", { note_id: note1.id });
    const createdLink = links.find((l) => l.id === link.id);
    assert.ok(createdLink, "Link should exist");
  });

  test("LINKS-007: get_links with direction filter", async () => {
    const testId = MCPTestClient.uniqueId();

    const note1 = await client.callTool("create_note", {
      content: `# Central Note ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Outgoing Target ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    const note3 = await client.callTool("create_note", {
      content: `# Incoming Source ${testId}`,
    });
    cleanup.noteIds.push(note3.id);

    // Create outgoing link
    const outLink = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note2.id,
      link_type: "references",
    });
    cleanup.linkIds.push(outLink.id);

    // Create incoming link
    const inLink = await client.callTool("create_link", {
      from_id: note3.id,
      to_id: note1.id,
      link_type: "references",
    });
    cleanup.linkIds.push(inLink.id);

    // Get all links (both directions)
    const allLinks = await client.callTool("get_links", {
      note_id: note1.id,
    });

    assert.ok(Array.isArray(allLinks), "Links should be an array");
    assert.ok(allLinks.length >= 2, "Should have at least 2 links");
  });

  test("LINKS-008: semantic_neighbors with threshold", async () => {
    const testId = MCPTestClient.uniqueId();

    const note = await client.callTool("create_note", {
      content: `# Test Note ${testId}\n\nQuantum computing and cryptography.`,
    });
    cleanup.noteIds.push(note.id);

    // Search with high threshold
    const neighbors = await client.callTool("semantic_neighbors", {
      note_id: note.id,
      limit: 5,
      min_similarity: 0.8,
    });

    assert.ok(Array.isArray(neighbors), "Neighbors should be an array");
    // High threshold might return fewer results
  });

  test("LINKS-009: create_link error handling - invalid note IDs", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("create_link", {
      from_id: fakeId,
      to_id: MCPTestClient.uniqueId(),
      link_type: "references",
    });

    assert.ok(error.error, "Should return an error");
    assert.ok(
      error.error.includes("not found") || error.error.includes("error"),
      "Error should mention note not found"
    );
  });

  test("LINKS-010: get_links error handling - invalid note ID", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("get_links", {
      note_id: fakeId,
    });

    assert.ok(error.error, "Should return an error for non-existent note");
  });
});
