#!/usr/bin/env node

/**
 * MCP Semantic Links Tests (Phase 6)
 *
 * Tests semantic link management via MCP tools:
 * - get_note_links: Retrieve links for a note (outgoing and incoming)
 * - get_note_backlinks: Get backlinks pointing to a note
 * - explore_graph: Explore graph neighborhood around a note
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 *
 * NOTE: Links are created automatically by semantic analysis, not via manual API.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 6: Semantic Links", () => {
  let client;
  const cleanup = { noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
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

  test("LINKS-001: Create notes with related content and check link structure", async () => {
    const testId = MCPTestClient.uniqueId();
    const sharedTopic = `authentication and security ${testId}`;

    // Create two related notes
    const note1 = await client.callTool("create_note", {
      content: `# Security Note ${testId}\n\nThis note discusses ${sharedTopic} in web applications.`,
    });
    assert.ok(note1.id, "Note 1 should be created with ID");
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Auth Note ${testId}\n\nExploring ${sharedTopic} best practices.`,
    });
    assert.ok(note2.id, "Note 2 should be created with ID");
    cleanup.noteIds.push(note2.id);

    // Check link structure (may be empty as automatic linking takes time)
    const links = await client.callTool("get_note_links", { id: note1.id });
    assert.ok(links, "Links response should exist");
    assert.ok(Array.isArray(links.outgoing), "Outgoing links should be an array");
    assert.ok(Array.isArray(links.incoming), "Incoming links should be an array");
  });

  test("LINKS-002: get_note_links returns proper structure", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Test Note ${testId}\n\nNote for link structure validation.`;

    const createResult = await client.callTool("create_note", { content });
    assert.ok(createResult.id, "Note should be created with ID");
    cleanup.noteIds.push(createResult.id);

    const links = await client.callTool("get_note_links", { id: createResult.id });
    assert.ok(links, "Links response should exist");
    assert.ok(typeof links === "object", "Links should be an object");
    assert.ok(Array.isArray(links.outgoing), "Should have outgoing array");
    assert.ok(Array.isArray(links.incoming), "Should have incoming array");
  });

  test("LINKS-003: get_note_backlinks returns proper structure", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Backlink Test ${testId}\n\nNote for backlink testing.`;

    const note = await client.callTool("create_note", { content });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    const backlinks = await client.callTool("get_note_backlinks", { id: note.id });
    assert.ok(backlinks, "Backlinks response should exist");
    assert.ok(Array.isArray(backlinks.backlinks), "Should have backlinks array");
    assert.ok(typeof backlinks.count === "number", "Should have count");
    assert.strictEqual(backlinks.note_id, note.id, "Note ID should match");
  });

  test("LINKS-004: explore_graph returns nodes and edges", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Graph Test ${testId}\n\nNote for graph exploration.`;

    const note = await client.callTool("create_note", { content });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    const graph = await client.callTool("explore_graph", {
      id: note.id,
      depth: 2,
      max_nodes: 10,
    });

    assert.ok(graph, "Graph response should exist");
    assert.ok(Array.isArray(graph.nodes), "Should have nodes array");
    assert.ok(Array.isArray(graph.edges), "Should have edges array");

    // Should at least include the starting node
    assert.ok(graph.nodes.length >= 1, "Should have at least starting node");
    const startNode = graph.nodes.find((n) => n.id === note.id);
    assert.ok(startNode, "Starting node should be in graph");
  });

  test("LINKS-005: explore_graph with depth constraint", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Depth Test ${testId}\n\nNote for depth testing.`;

    const note = await client.callTool("create_note", { content });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    const graph = await client.callTool("explore_graph", {
      id: note.id,
      depth: 1,
      max_nodes: 20,
    });

    assert.ok(graph, "Graph response should exist");
    assert.ok(Array.isArray(graph.nodes), "Should have nodes array");

    // Verify all nodes respect depth constraint
    for (const node of graph.nodes) {
      assert.ok(node.depth !== undefined, "Each node should have depth");
      assert.ok(node.depth <= 1, "Node depth should not exceed requested depth");
    }
  });

  test("LINKS-006: get_note_links on nonexistent note returns error or empty", async () => {
    const fakeId = MCPTestClient.uniqueId();

    try {
      const result = await client.callTool("get_note_links", { id: fakeId });
      // API may return empty arrays for nonexistent notes
      assert.ok(result, "Should return a result");
      const outgoing = result.outgoing || [];
      const incoming = result.incoming || [];
      assert.strictEqual(outgoing.length, 0, "Should have no outgoing links");
      assert.strictEqual(incoming.length, 0, "Should have no incoming links");
    } catch (e) {
      // Or it may return an error
      assert.ok(
        e.message.includes("not found") || e.message.includes("error") || e.message.includes("404"),
        "Error should indicate note not found"
      );
    }
  });

  test("LINKS-007: explore_graph with max_nodes constraint", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Max Nodes Test ${testId}\n\nNote for node limit testing.`;

    const note = await client.callTool("create_note", { content });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    const maxNodes = 5;
    const graph = await client.callTool("explore_graph", {
      id: note.id,
      depth: 3,
      max_nodes: maxNodes,
    });

    assert.ok(graph, "Graph response should exist");
    assert.ok(Array.isArray(graph.nodes), "Should have nodes array");
    assert.ok(
      graph.nodes.length <= maxNodes,
      `Should not exceed max_nodes limit of ${maxNodes}`
    );
  });

  test("LINKS-008: get_note includes links array in response", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Get Note Links Test ${testId}\n\nVerify get_note includes links.`;

    const createResult = await client.callTool("create_note", { content });
    assert.ok(createResult.id, "Note should be created");
    cleanup.noteIds.push(createResult.id);

    const note = await client.callTool("get_note", { id: createResult.id });
    assert.ok(note, "Note response should exist");
    assert.ok(note.note, "Should have note object");
    assert.ok(Array.isArray(note.links), "Should have links array");
  });

  test("LINKS-009: get_note_backlinks on nonexistent note returns error or empty", async () => {
    const fakeId = MCPTestClient.uniqueId();

    try {
      const result = await client.callTool("get_note_backlinks", { id: fakeId });
      // API may return empty result for nonexistent notes
      assert.ok(result, "Should return a result");
      const backlinks = result.backlinks || [];
      assert.strictEqual(backlinks.length, 0, "Should have no backlinks");
    } catch (e) {
      // Or it may return an error
      assert.ok(
        e.message.includes("not found") || e.message.includes("error") || e.message.includes("404"),
        "Error should indicate note not found"
      );
    }
  });

  test("LINKS-010: explore_graph with default parameters", async () => {
    const testId = MCPTestClient.uniqueId();
    const content = `# Default Graph Test ${testId}\n\nTest with default graph params.`;

    const note = await client.callTool("create_note", { content });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Call with minimal params (defaults should apply)
    const graph = await client.callTool("explore_graph", {
      id: note.id,
    });

    assert.ok(graph, "Graph response should exist");
    assert.ok(Array.isArray(graph.nodes), "Should have nodes array");
    assert.ok(Array.isArray(graph.edges), "Should have edges array");

    // Basic structure validation
    for (const node of graph.nodes) {
      assert.ok(node.id, "Each node should have id");
      assert.ok(node.depth !== undefined, "Each node should have depth");
    }

    for (const edge of graph.edges) {
      if (graph.edges.length > 0) {
        assert.ok(edge.source, "Each edge should have source");
        assert.ok(edge.target, "Each edge should have target");
      }
    }
  });
});
