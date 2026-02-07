#!/usr/bin/env node

/**
 * MCP Feature Chains Tests (Phase 19 - CRITICAL)
 *
 * Tests multi-tool workflows that chain multiple MCP tools together:
 * - Create → Search → Get: Verify search finds created note
 * - Create → Collection → Add → List: Collection management workflow
 * - Create with tags → List tags → Verify: Tag creation workflow
 * - Bulk create → Search → Verify: Bulk operations workflow
 *
 * These tests validate end-to-end workflows that AI agents typically use.
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 19: Multi-Tool Workflows (CRITICAL)", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [], tagIds: [] };

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

    // Clean up collections
    for (const id of cleanup.collectionIds) {
      try {
        await client.callTool("delete_collection", { id });
      } catch (e) {
        console.error(`Failed to delete collection ${id}:`, e.message);
      }
    }

    await client.close();
  });

  test("CHAIN-001: create_note → search_notes → get_note workflow", async () => {
    const testId = MCPTestClient.uniqueId();
    const uniquePhrase = `workflow-test-${testId}`;
    const content = `# Workflow Test\n\nThis note contains ${uniquePhrase} for testing.`;

    // Step 1: Create note
    const created = await client.callTool("create_note", { content });
    assert.ok(created.id, "Note should be created");
    cleanup.noteIds.push(created.id);

    // Allow brief time for indexing (if async)
    await new Promise((resolve) => setTimeout(resolve, 100));

    // Step 2: Search for the note
    const searchResults = await client.callTool("search_notes", {
      query: uniquePhrase,
    });
    assert.ok(Array.isArray(searchResults), "Search should return array");
    assert.ok(searchResults.length > 0, "Search should find at least one note");

    const found = searchResults.find((n) => n.id === created.id);
    assert.ok(found, "Search should find the created note");

    // Step 3: Get the note by ID
    const retrieved = await client.callTool("get_note", { id: created.id });
    assert.ok(retrieved, "Note should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "Retrieved note ID should match");
    assert.ok(retrieved.content.includes(uniquePhrase), "Content should match");
  });

  test("CHAIN-002: create_note → create_collection → add_to_collection → list_notes workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const collectionName = `test-collection-${testId}`;

    // Step 1: Create notes
    const note1 = await client.callTool("create_note", {
      content: `# Collection Note 1 ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Collection Note 2 ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    // Step 2: Create collection
    const collection = await client.callTool("create_collection", {
      name: collectionName,
      description: "Test collection for workflow",
    });
    assert.ok(collection.id, "Collection should be created");
    cleanup.collectionIds.push(collection.id);

    // Step 3: Add notes to collection
    await client.callTool("add_to_collection", {
      collection_id: collection.id,
      note_id: note1.id,
    });

    await client.callTool("add_to_collection", {
      collection_id: collection.id,
      note_id: note2.id,
    });

    // Step 4: List notes in collection
    const collectionNotes = await client.callTool("list_notes", {
      collection_id: collection.id,
    });

    assert.ok(Array.isArray(collectionNotes), "Collection notes should be array");
    assert.ok(collectionNotes.length >= 2, "Collection should have at least 2 notes");

    const foundNote1 = collectionNotes.find((n) => n.id === note1.id);
    const foundNote2 = collectionNotes.find((n) => n.id === note2.id);
    assert.ok(foundNote1, "Note 1 should be in collection");
    assert.ok(foundNote2, "Note 2 should be in collection");
  });

  test("CHAIN-003: create_note with tags → list_tags → verify workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const tag = MCPTestClient.testTag("chain", testId);
    const content = `# Tagged Note ${testId}\n\nThis note has a test tag.`;

    // Step 1: Create note with tag
    const note = await client.callTool("create_note", {
      content,
      tags: [tag],
    });
    assert.ok(note.id, "Note should be created");
    assert.ok(note.tags.includes(tag), "Note should have tag");
    cleanup.noteIds.push(note.id);

    // Step 2: List all tags
    const allTags = await client.callTool("list_tags");
    assert.ok(Array.isArray(allTags), "Tags should be array");

    // Step 3: Verify tag exists
    const foundTag = allTags.find((t) => t.name === tag);
    assert.ok(foundTag, "Tag should exist in list");
    assert.ok(foundTag.count >= 1, "Tag should have count >= 1");

    // Step 4: Search notes by tag
    const taggedNotes = await client.callTool("list_notes", {
      tags: tag,
    });
    assert.ok(Array.isArray(taggedNotes), "Tagged notes should be array");

    const foundNote = taggedNotes.find((n) => n.id === note.id);
    assert.ok(foundNote, "Created note should be found by tag filter");
  });

  test("CHAIN-004: bulk_create_notes → search_notes → verify workflow", async () => {
    const testId = MCPTestClient.uniqueId();
    const sharedKeyword = `bulk-chain-${testId}`;

    const notes = [
      { content: `# Bulk Note 1\n\nContains ${sharedKeyword}` },
      { content: `# Bulk Note 2\n\nAlso has ${sharedKeyword}` },
      { content: `# Bulk Note 3\n\nIncludes ${sharedKeyword} too` },
    ];

    // Step 1: Bulk create notes
    const created = await client.callTool("bulk_create_notes", { notes });
    assert.ok(Array.isArray(created), "Bulk create should return array");
    assert.strictEqual(created.length, 3, "Should create 3 notes");

    created.forEach((note) => {
      assert.ok(note.id, "Each note should have ID");
      cleanup.noteIds.push(note.id);
    });

    // Allow time for indexing
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Step 2: Search for all created notes
    const searchResults = await client.callTool("search_notes", {
      query: sharedKeyword,
    });

    assert.ok(Array.isArray(searchResults), "Search should return array");
    assert.ok(searchResults.length >= 3, "Search should find at least 3 notes");

    // Step 3: Verify all notes are found
    const createdIds = new Set(created.map((n) => n.id));
    const foundIds = new Set(searchResults.map((n) => n.id));

    created.forEach((note) => {
      assert.ok(foundIds.has(note.id), `Note ${note.id} should be found in search`);
    });
  });

  test("CHAIN-005: create_note → update_note → get_note → verify changes", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const originalContent = `# Original Content ${testId}`;
    const updatedContent = `# Updated Content ${testId}\n\nThis has been modified.`;

    // Step 1: Create note
    const created = await client.callTool("create_note", {
      content: originalContent,
    });
    cleanup.noteIds.push(created.id);

    // Step 2: Update note
    const updated = await client.callTool("update_note", {
      id: created.id,
      content: updatedContent,
    });
    assert.strictEqual(updated.id, created.id, "ID should remain the same");

    // Step 3: Get note and verify changes
    const retrieved = await client.callTool("get_note", { id: created.id });
    assert.ok(retrieved.content.includes("Updated Content"), "Content should be updated");
    assert.ok(retrieved.content.includes("modified"), "New content should be present");
  });

  test("CHAIN-006: create_template → apply_template → search → verify", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `chain-template-${testId}`;
    const uniqueMarker = `chain-marker-${testId}`;
    const templateContent = `# {{title}}\n\nGenerated with marker: ${uniqueMarker}`;

    // Step 1: Create template
    const template = await client.callTool("create_template", {
      name: templateName,
      content: templateContent,
    });
    assert.ok(template.id, "Template should be created");

    // Step 2: Apply template to create note
    const note = await client.callTool("apply_template", {
      template_id: template.id,
      variables: { title: "Generated Note" },
    });
    assert.ok(note.id, "Note should be created from template");
    cleanup.noteIds.push(note.id);

    // Allow indexing time
    await new Promise((resolve) => setTimeout(resolve, 100));

    // Step 3: Search for generated note
    const searchResults = await client.callTool("search_notes", {
      query: uniqueMarker,
    });

    const found = searchResults.find((n) => n.id === note.id);
    assert.ok(found, "Template-generated note should be found in search");

    // Step 4: Clean up template
    await client.callTool("delete_template", { id: template.id });
  });

  test("CHAIN-007: create_embedding_set → create_note → semantic_search workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const setName = `chain-embed-${testId}`;

    // Step 1: Create embedding set
    const embeddingSet = await client.callTool("create_embedding_set", {
      name: setName,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });
    assert.ok(embeddingSet.id, "Embedding set should be created");

    // Step 2: Create notes with related content
    const note1 = await client.callTool("create_note", {
      content: `# Machine Learning ${testId}\n\nDiscussing neural networks and AI.`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Deep Learning ${testId}\n\nExploring artificial intelligence models.`,
    });
    cleanup.noteIds.push(note2.id);

    // Step 3: Semantic search (may require embeddings to be generated)
    // This validates the workflow even if embeddings are async
    const semanticResults = await client.callTool("semantic_search", {
      query: "artificial intelligence",
      limit: 10,
    });

    assert.ok(Array.isArray(semanticResults), "Semantic search should return array");

    // Step 4: Clean up embedding set
    await client.callTool("delete_embedding_set", { id: embeddingSet.id });
  });

  test("CHAIN-008: create_concept_scheme → create_concepts → export_turtle workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const schemeTitle = `Chain SKOS Scheme ${testId}`;

    // Step 1: Create concept scheme
    const scheme = await client.callTool("create_concept_scheme", {
      title: schemeTitle,
      uri: `http://example.org/schemes/chain-${testId}`,
    });
    assert.ok(scheme.id, "Scheme should be created");

    // Step 2: Create concepts
    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent Concept",
    });

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Child Concept",
      broader: [concept1.id],
    });

    // Step 3: Export as Turtle
    const turtle = await client.callTool("export_concept_scheme_turtle", {
      scheme_id: scheme.id,
    });

    assert.ok(turtle, "Turtle export should succeed");
    assert.ok(typeof turtle === "string", "Turtle should be string");
    assert.ok(turtle.includes("Parent Concept"), "Should include concept labels");

    // Step 4: Clean up
    await client.callTool("delete_concept", { id: concept2.id });
    await client.callTool("delete_concept", { id: concept1.id });
    await client.callTool("delete_concept_scheme", { id: scheme.id });
  });

  test("CHAIN-009: create_notes → create_links → get_graph → traverse", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create network of notes
    const note1 = await client.callTool("create_note", {
      content: `# Central Note ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Connected Note A ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    const note3 = await client.callTool("create_note", {
      content: `# Connected Note B ${testId}`,
    });
    cleanup.noteIds.push(note3.id);

    // Step 2: Create links
    const link1 = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note2.id,
      link_type: "references",
    });

    const link2 = await client.callTool("create_link", {
      from_id: note1.id,
      to_id: note3.id,
      link_type: "references",
    });

    // Step 3: Get links for central note
    const links = await client.callTool("get_links", {
      note_id: note1.id,
    });

    assert.ok(Array.isArray(links), "Links should be array");
    assert.ok(links.length >= 2, "Should have at least 2 links");

    // Step 4: Traverse graph by getting connected notes
    const connectedIds = links.map((l) => (l.from_id === note1.id ? l.to_id : l.from_id));
    assert.ok(connectedIds.includes(note2.id), "Should be connected to note 2");
    assert.ok(connectedIds.includes(note3.id), "Should be connected to note 3");

    // Clean up links
    await client.callTool("delete_link", { id: link1.id });
    await client.callTool("delete_link", { id: link2.id });
  });

  test("CHAIN-010: create_collection → add_notes → export_markdown workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const collectionName = `export-collection-${testId}`;

    // Step 1: Create collection
    const collection = await client.callTool("create_collection", {
      name: collectionName,
    });
    cleanup.collectionIds.push(collection.id);

    // Step 2: Create and add notes
    const note1 = await client.callTool("create_note", {
      content: `# Export Note 1 ${testId}`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Export Note 2 ${testId}`,
    });
    cleanup.noteIds.push(note2.id);

    await client.callTool("add_to_collection", {
      collection_id: collection.id,
      note_id: note1.id,
    });

    await client.callTool("add_to_collection", {
      collection_id: collection.id,
      note_id: note2.id,
    });

    // Step 3: Export collection as markdown
    const exported = await client.callTool("export_collection_markdown", {
      collection_id: collection.id,
    });

    assert.ok(exported, "Export should succeed");
    assert.ok(typeof exported === "string", "Export should be string");
    assert.ok(exported.includes("Export Note 1"), "Should include note 1");
    assert.ok(exported.includes("Export Note 2"), "Should include note 2");
  });

  test("CHAIN-011: error recovery - create_note → fail → retry workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create valid note
    const validNote = await client.callTool("create_note", {
      content: `# Valid Note ${testId}`,
    });
    cleanup.noteIds.push(validNote.id);

    // Step 2: Attempt invalid operation (update non-existent note)
    const fakeId = MCPTestClient.uniqueId();
    const error = await client.callToolExpectError("update_note", {
      id: fakeId,
      content: "This should fail",
    });
    assert.ok(error.error, "Invalid operation should fail");

    // Step 3: Recover by updating valid note
    const recovered = await client.callTool("update_note", {
      id: validNote.id,
      content: `# Updated After Error ${testId}`,
    });

    assert.ok(recovered, "Should recover and update successfully");
    assert.strictEqual(recovered.id, validNote.id, "ID should match");
  });

  test("CHAIN-012: parallel workflows - multiple create → search operations", async () => {
    const testId = MCPTestClient.uniqueId();

    // Create multiple notes in parallel (simulating concurrent agent operations)
    const promises = [];
    const keywords = [];

    for (let i = 0; i < 5; i++) {
      const keyword = `parallel-${testId}-${i}`;
      keywords.push(keyword);
      promises.push(
        client.callTool("create_note", {
          content: `# Parallel Note ${i}\n\nKeyword: ${keyword}`,
        })
      );
    }

    const created = await Promise.all(promises);
    created.forEach((note) => {
      assert.ok(note.id, "Each parallel note should be created");
      cleanup.noteIds.push(note.id);
    });

    // Allow indexing time
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Search for each keyword
    const searchPromises = keywords.map((keyword) =>
      client.callTool("search_notes", { query: keyword })
    );

    const searchResults = await Promise.all(searchPromises);
    searchResults.forEach((results, i) => {
      assert.ok(Array.isArray(results), `Search ${i} should return array`);
      assert.ok(results.length > 0, `Search ${i} should find notes`);
    });
  });
});
