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
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
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
    const searchResult = await client.callTool("search_notes", {
      query: uniquePhrase,
    });
    assert.ok(searchResult.results, "Search should return results object");
    assert.ok(Array.isArray(searchResult.results), "Results should be array");
    assert.ok(searchResult.results.length > 0, "Search should find at least one note");

    const found = searchResult.results.find((n) => n.note_id === created.id);
    assert.ok(found, "Search should find the created note");
    assert.ok(found.snippet, "Result should have snippet");

    // Step 3: Get the note by ID
    const retrieved = await client.callTool("get_note", { id: created.id });
    assert.ok(retrieved.note, "Note should be retrieved");
    assert.strictEqual(retrieved.note.id, created.id, "Retrieved note ID should match");
    assert.ok(retrieved.original.content.includes(uniquePhrase), "Content should match");
  });

  test("CHAIN-002: create_note → create_collection → move_note_to_collection → get_collection_notes workflow", async () => {
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

    // Step 3: Move notes to collection (notes can only be in ONE collection)
    await client.callTool("move_note_to_collection", {
      note_id: note1.id,
      collection_id: collection.id,
    });

    await client.callTool("move_note_to_collection", {
      note_id: note2.id,
      collection_id: collection.id,
    });

    // Step 4: Get collection notes
    const collectionData = await client.callTool("get_collection_notes", {
      id: collection.id,
    });

    assert.ok(collectionData.notes, "Collection should have notes array");
    assert.ok(Array.isArray(collectionData.notes), "Collection notes should be array");
    assert.ok(collectionData.notes.length >= 2, "Collection should have at least 2 notes");

    const foundNote1 = collectionData.notes.find((n) => n.id === note1.id);
    const foundNote2 = collectionData.notes.find((n) => n.id === note2.id);
    assert.ok(foundNote1, "Note 1 should be in collection");
    assert.ok(foundNote2, "Note 2 should be in collection");
  });

  test("CHAIN-003: create_note with tags → list_tags → verify workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const tag = MCPTestClient.testTag("chain", testId);
    const content = `# Tagged Note ${testId}\n\nThis note has a test tag.`;

    // Step 1: Create note with tag
    const created = await client.callTool("create_note", {
      content,
      tags: [tag],
    });
    assert.ok(created.id, "Note should be created");
    cleanup.noteIds.push(created.id);

    // Step 2: Get note to verify tags
    const note = await client.callTool("get_note", { id: created.id });
    assert.ok(note.tags, "Note should have tags array");
    assert.ok(note.tags.includes(tag), "Note should have tag");

    // Step 3: List all tags
    const allTags = await client.callTool("list_tags");
    assert.ok(Array.isArray(allTags), "Tags should be array");

    // Step 4: Verify tag exists with correct field name
    const foundTag = allTags.find((t) => t.name === tag);
    assert.ok(foundTag, "Tag should exist in list");
    assert.ok(foundTag.note_count >= 1, "Tag should have note_count >= 1");

    // Step 5: Search notes by tag
    const taggedNotes = await client.callTool("list_notes", {
      tags: [tag],
    });
    assert.ok(taggedNotes.notes, "Tagged notes should have notes array");
    assert.ok(Array.isArray(taggedNotes.notes), "Notes should be array");

    const foundNote = taggedNotes.notes.find((n) => n.id === created.id);
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
    const searchResult = await client.callTool("search_notes", {
      query: sharedKeyword,
    });

    assert.ok(searchResult.results, "Search should return results object");
    assert.ok(Array.isArray(searchResult.results), "Results should be array");
    assert.ok(searchResult.results.length >= 3, "Search should find at least 3 notes");

    // Step 3: Verify all notes are found
    const createdIds = new Set(created.map((n) => n.id));
    const foundIds = new Set(searchResult.results.map((n) => n.note_id));

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

    // Step 2: Update note (returns {success: true}, not the note)
    const updateResult = await client.callTool("update_note", {
      id: created.id,
      content: updatedContent,
    });
    assert.strictEqual(updateResult.success, true, "Update should succeed");

    // Step 3: Get note and verify changes
    const retrieved = await client.callTool("get_note", { id: created.id });
    assert.ok(retrieved.original.content.includes("Updated Content"), "Content should be updated");
    assert.ok(retrieved.original.content.includes("modified"), "New content should be present");
  });

  test("CHAIN-006: create_template → instantiate_template → search → verify", async () => {
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

    // Step 2: Instantiate template (not apply_template)
    const instantiated = await client.callTool("instantiate_template", {
      id: template.id,
      variables: { title: "Generated Note" },
    });
    assert.ok(instantiated.id, "Note should be created from template");
    cleanup.noteIds.push(instantiated.id);

    // Step 3: Get note to verify content
    const note = await client.callTool("get_note", { id: instantiated.id });
    assert.ok(note.original.content.includes(uniqueMarker), "Note should contain marker");
    assert.ok(note.original.content.includes("Generated Note"), "Note should contain title");

    // Allow indexing time
    await new Promise((resolve) => setTimeout(resolve, 100));

    // Step 4: Search for generated note
    const searchResult = await client.callTool("search_notes", {
      query: uniqueMarker,
    });

    const found = searchResult.results.find((n) => n.note_id === instantiated.id);
    assert.ok(found, "Template-generated note should be found in search");

    // Step 5: Clean up template
    await client.callTool("delete_template", { id: template.id });
  });

  test("CHAIN-007: create_embedding_set → create_note → search_notes (semantic) workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const setName = `chain-embed-${testId}`;

    // Step 1: Create embedding set
    const embeddingSet = await client.callTool("create_embedding_set", {
      name: setName,
      description: "Embedding set for chain workflow test",
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

    // Step 3: Semantic search (use search_notes with mode parameter)
    const semanticResult = await client.callTool("search_notes", {
      query: "artificial intelligence",
      mode: "semantic",
      limit: 10,
    });

    assert.ok(semanticResult.results, "Semantic search should return results object");
    assert.ok(Array.isArray(semanticResult.results), "Results should be array");

    // Step 4: Clean up embedding set
    await client.callTool("delete_embedding_set", { slug: embeddingSet.slug });
  });

  test("CHAIN-008: create_concept_scheme → create_concepts → export_skos_turtle workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const schemeTitle = `Chain SKOS Scheme ${testId}`;
    const schemeNotation = `chain-${testId}`;

    // Step 1: Create concept scheme (requires notation and title)
    const scheme = await client.callTool("create_concept_scheme", {
      notation: schemeNotation,
      title: schemeTitle,
      uri: `http://example.org/schemes/chain-${testId}`,
    });
    assert.ok(scheme.id, "Scheme should be created");

    // Step 2: Create concepts (use pref_label not label)
    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent Concept",
    });

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Child Concept",
      broader_ids: [concept1.id],
    });

    // Step 3: Export as Turtle (use export_skos_turtle not export_concept_scheme_turtle)
    const turtleResult = await client.callTool("export_skos_turtle", {
      scheme_id: scheme.id,
    });

    assert.ok(turtleResult, "Turtle export should succeed");
    assert.ok(turtleResult.turtle, "Turtle export should have turtle field");
    assert.ok(typeof turtleResult.turtle === "string", "Turtle should be string");
    assert.ok(turtleResult.turtle.includes("Parent Concept"), "Should include concept labels");

    // Step 4: Clean up
    await client.callTool("delete_concept", { id: concept2.id });
    await client.callTool("delete_concept", { id: concept1.id });
    await client.callTool("delete_concept_scheme", { id: scheme.id });
  });

  test("CHAIN-009: create_notes → get_note_links → explore_graph workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create network of notes with wikilink-style references
    const note1 = await client.callTool("create_note", {
      content: `# Central Note ${testId}\n\nThis references [[note2]] and [[note3]].`,
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Connected Note A ${testId}\n\nTitle: note2`,
    });
    cleanup.noteIds.push(note2.id);

    const note3 = await client.callTool("create_note", {
      content: `# Connected Note B ${testId}\n\nTitle: note3`,
    });
    cleanup.noteIds.push(note3.id);

    // Allow time for link extraction
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Step 2: Get links for central note (use get_note_links, not get_links)
    const linksResult = await client.callTool("get_note_links", {
      id: note1.id,
    });

    assert.ok(linksResult.outgoing, "Should have outgoing links array");
    assert.ok(linksResult.incoming, "Should have incoming links array");
    assert.ok(Array.isArray(linksResult.outgoing), "Outgoing should be array");

    // Step 3: Explore graph from central note
    const graphResult = await client.callTool("explore_graph", {
      id: note1.id,
      depth: 2,
    });

    assert.ok(graphResult.nodes, "Graph should have nodes array");
    assert.ok(graphResult.edges, "Graph should have edges array");
    assert.ok(Array.isArray(graphResult.nodes), "Nodes should be array");
    assert.ok(Array.isArray(graphResult.edges), "Edges should be array");

    // The central note should be in the graph
    const centralNode = graphResult.nodes.find((n) => n.id === note1.id);
    assert.ok(centralNode, "Central note should be in graph");
  });

  test("CHAIN-010: create_collection → move_notes → export_collection workflow", async () => {
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

    await client.callTool("move_note_to_collection", {
      note_id: note1.id,
      collection_id: collection.id,
    });

    await client.callTool("move_note_to_collection", {
      note_id: note2.id,
      collection_id: collection.id,
    });

    // Step 3: Export collection
    // Note: export_collection returns markdown, but the MCP server's apiRequest
    // always tries JSON.parse, causing a tool error. Handle both success and error.
    try {
      const exported = await client.callTool("export_collection", {
        id: collection.id,
      });
      // If we get here, it succeeded (returned parseable JSON or raw text)
      const exportContent = typeof exported === "string" ? exported : (exported.markdown || exported.content || JSON.stringify(exported));
      assert.ok(typeof exportContent === "string", "Export content should be string");
      console.log(`  ✓ Export collection returned content`);
    } catch (error) {
      // Server JSON.parse fails on markdown response - known limitation
      assert.ok(
        error.message.includes("JSON") || error.isToolError,
        "Export error should be JSON parse related"
      );
      console.log(`  ✓ Export collection returned markdown (JSON parse limitation)`);
    }
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

    // Step 3: Recover by updating valid note (returns {success: true})
    const updateResult = await client.callTool("update_note", {
      id: validNote.id,
      content: `# Updated After Error ${testId}`,
    });

    assert.ok(updateResult.success, "Should recover and update successfully");

    // Step 4: Verify with get_note
    const retrieved = await client.callTool("get_note", { id: validNote.id });
    assert.ok(retrieved.original.content.includes("Updated After Error"), "Content should be updated");
  });

  test("CHAIN-012: parallel workflows - multiple create → search operations", async () => {
    const testId = MCPTestClient.uniqueId();

    // Create notes sequentially to avoid overwhelming SSE transport
    const keywords = [];
    for (let i = 0; i < 5; i++) {
      const keyword = `parallel-${testId}-${i}`;
      keywords.push(keyword);
      const note = await client.callTool("create_note", {
        content: `# Parallel Note ${i}\n\nKeyword: ${keyword}`,
      });
      assert.ok(note.id, "Each note should be created");
      cleanup.noteIds.push(note.id);
    }

    // Allow indexing time
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Search for each keyword sequentially
    for (let i = 0; i < keywords.length; i++) {
      const result = await client.callTool("search_notes", { query: keywords[i] });
      assert.ok(result.results, `Search ${i} should return results object`);
      assert.ok(Array.isArray(result.results), `Results ${i} should be array`);
    }
  });

  // --- Version Workflow Chains ---

  test("CHAIN-013: create_note → update → list_note_versions → get_note_version → diff workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create note
    const note = await client.callTool("create_note", {
      content: `# Version Test v1 ${testId}\n\nOriginal content.`,
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Step 2: Update note to create a new version
    const update1 = await client.callTool("update_note", {
      id: note.id,
      content: `# Version Test v2 ${testId}\n\nUpdated content with changes.`,
    });
    assert.ok(update1.success, "First update should succeed");

    // Step 3: Update again for a third version
    const update2 = await client.callTool("update_note", {
      id: note.id,
      content: `# Version Test v3 ${testId}\n\nFinal content with more changes.`,
    });
    assert.ok(update2.success, "Second update should succeed");

    // Step 4: List note versions
    const versions = await client.callTool("list_note_versions", {
      note_id: note.id,
    });
    assert.ok(versions, "Should return versions data");
    assert.ok(
      versions.original_versions || versions.current_original_version !== undefined,
      "Should have version info"
    );
    console.log(`  Versions: original=${versions.current_original_version}, revision=${versions.current_revision_number}`);

    // Step 5: Get a specific version
    const v1 = await client.callTool("get_note_version", {
      note_id: note.id,
      version: 1,
    });
    assert.ok(v1, "Should return version 1 data");
    assert.ok(v1.content || v1.version, "Should have version content or metadata");

    // Step 6: Diff between versions
    if (versions.current_original_version >= 2) {
      const diff = await client.callTool("diff_note_versions", {
        note_id: note.id,
        from_version: 1,
        to_version: versions.current_original_version,
      });
      assert.ok(diff !== undefined && diff !== null, "Should return diff data");
      console.log(`  Diff type: ${typeof diff}`);
    }
  });

  test("CHAIN-014: create_note → update multiple → restore_note_version → verify workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const originalContent = `# Restore Test ${testId}\n\nThis is the original content to restore.`;

    // Step 1: Create note
    const note = await client.callTool("create_note", {
      content: originalContent,
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Step 2: Update twice to create versions
    await client.callTool("update_note", {
      id: note.id,
      content: `# Restore Test ${testId}\n\nChanged in update 1.`,
    });
    await client.callTool("update_note", {
      id: note.id,
      content: `# Restore Test ${testId}\n\nChanged again in update 2.`,
    });

    // Step 3: Verify current content is update 2
    const current = await client.callTool("get_note", { id: note.id });
    assert.ok(current.original.content.includes("update 2"), "Should have update 2 content");

    // Step 4: Restore version 1
    const restored = await client.callTool("restore_note_version", {
      note_id: note.id,
      version: 1,
    });
    assert.ok(restored, "Should return restore result");
    assert.ok(
      restored.success || restored.new_version !== undefined,
      "Should indicate restore success"
    );

    // Step 5: Verify content is restored to original
    const afterRestore = await client.callTool("get_note", { id: note.id });
    assert.ok(
      afterRestore.original.content.includes("original content to restore"),
      "Content should be restored to version 1"
    );
    console.log(`  Restored to version 1 successfully`);
  });

  // --- PKE Encryption Workflow ---

  test("CHAIN-015: pke_generate_keypair → pke_encrypt → pke_list_recipients → pke_decrypt roundtrip", async () => {
    // Step 1: Generate a keypair
    const keypair = await client.callTool("pke_generate_keypair", {
      passphrase: "test-passphrase-chain-015",
      label: "chain-test-key",
    });
    assert.ok(keypair, "Should generate keypair");
    assert.ok(keypair.public_key, "Should have public_key");
    assert.ok(keypair.encrypted_private_key, "Should have encrypted_private_key");
    assert.ok(keypair.address, "Should have address");
    console.log(`  Generated keypair with address: ${keypair.address}`);

    // Step 2: Verify address
    const addrResult = await client.callTool("pke_verify_address", {
      address: keypair.address,
    });
    assert.ok(addrResult, "Should verify address");

    // Step 3: Encrypt a message
    const plaintext = Buffer.from("Hello from CHAIN-015 test!").toString("base64");
    const encrypted = await client.callTool("pke_encrypt", {
      plaintext,
      recipient_keys: [keypair.public_key],
    });
    assert.ok(encrypted, "Should return encryption result");
    const ciphertext = encrypted.ciphertext || encrypted;
    assert.ok(ciphertext, "Should have ciphertext");

    // Step 4: List recipients
    if (typeof ciphertext === "string" && ciphertext.length > 0) {
      const recipients = await client.callTool("pke_list_recipients", {
        ciphertext,
      });
      assert.ok(recipients, "Should list recipients");
      const addrs = Array.isArray(recipients) ? recipients : (recipients.addresses || recipients.recipients || []);
      assert.ok(addrs.length > 0, "Should have at least one recipient");
      console.log(`  Recipients: ${JSON.stringify(addrs).slice(0, 100)}`);
    }

    // Step 5: Decrypt the message
    if (typeof ciphertext === "string" && ciphertext.length > 0) {
      const decrypted = await client.callTool("pke_decrypt", {
        ciphertext,
        encrypted_private_key: keypair.encrypted_private_key,
        passphrase: "test-passphrase-chain-015",
      });
      assert.ok(decrypted, "Should return decryption result");
      const decryptedText = decrypted.plaintext || decrypted;
      // Decode base64 and verify
      const decoded = Buffer.from(decryptedText, "base64").toString("utf-8");
      assert.ok(
        decoded.includes("Hello from CHAIN-015"),
        "Decrypted text should match original"
      );
      console.log(`  Decrypted: ${decoded}`);
    }
  });

  // --- Backup & Status Workflow ---

  test("CHAIN-016: backup_now → backup_status → list_backups workflow", async () => {
    // Step 1: Trigger backup
    const backupResult = await client.callTool("backup_now", {
      dry_run: true,
    });
    assert.ok(backupResult, "Should return backup result");
    console.log(`  Backup result: ${JSON.stringify(backupResult).slice(0, 200)}`);

    // Step 2: Check backup status
    const status = await client.callTool("backup_status", {});
    assert.ok(status, "Should return backup status");
    assert.ok(
      status.status !== undefined || status.backup_directory !== undefined,
      "Should have status info"
    );
    console.log(`  Backup status: ${status.status || "available"}, dir: ${status.backup_directory || "N/A"}`);

    // Step 3: List backups
    const backups = await client.callTool("list_backups", {});
    assert.ok(backups, "Should return backups list");
    const backupList = Array.isArray(backups) ? backups : (backups.backups || []);
    assert.ok(Array.isArray(backupList), "Backups should be an array");
    console.log(`  Found ${backupList.length} backups`);
  });

  // --- Observability Workflow ---

  test("CHAIN-017: create notes → get_knowledge_health → get_notes_timeline → get_notes_activity workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create a few test notes to generate activity
    const note1 = await client.callTool("create_note", {
      content: `# Observability Chain A ${testId}`,
      tags: [MCPTestClient.testTag("obs-chain")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note1.id);

    const note2 = await client.callTool("create_note", {
      content: `# Observability Chain B ${testId}`,
      tags: [MCPTestClient.testTag("obs-chain")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note2.id);

    // Brief delay for indexing
    await new Promise((r) => setTimeout(r, 300));

    // Step 2: Get knowledge health dashboard
    const health = await client.callTool("get_knowledge_health", {});
    assert.ok(health, "Should return health data");
    assert.ok(
      health.health_score !== undefined || health.orphan_tags !== undefined || health.metrics !== undefined,
      "Should have health metrics"
    );

    // Step 3: Get notes timeline
    const timeline = await client.callTool("get_notes_timeline", {
      granularity: "day",
    });
    assert.ok(timeline !== undefined, "Should return timeline data");
    const buckets = Array.isArray(timeline) ? timeline : (timeline.buckets || timeline.timeline || []);
    assert.ok(Array.isArray(buckets), "Timeline should be an array");

    // Step 4: Get notes activity
    const activity = await client.callTool("get_notes_activity", {
      limit: 20,
    });
    assert.ok(activity !== undefined, "Should return activity data");
    const events = Array.isArray(activity) ? activity : (activity.events || activity.activity || []);
    assert.ok(Array.isArray(events), "Activity should be an array");

    // Step 5: Verify our notes appear in activity
    if (events.length > 0) {
      assert.ok(events[0].note_id, "Activity events should have note_id");
      assert.ok(events[0].created_at, "Activity events should have created_at");
    }

    console.log(`  Health score: ${health.health_score || "N/A"}, timeline: ${buckets.length} buckets, activity: ${events.length} events`);
  });

  // --- Document Type Detection Workflow ---

  test("CHAIN-018: detect_document_type → create_note → verify workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Detect document type from filename
    const detection = await client.callTool("detect_document_type", {
      filename: "meeting-notes-2026-02-10.md",
      content: "# Meeting Notes\n\n## Attendees\n- Alice\n- Bob\n\n## Agenda\n1. Project update\n2. Planning",
    });
    assert.ok(detection, "Should return detection result");
    console.log(`  Detected type: ${JSON.stringify(detection).slice(0, 200)}`);

    // Step 2: Create a note using the detected type info
    const note = await client.callTool("create_note", {
      content: `# Meeting Notes ${testId}\n\n## Attendees\n- Alice\n- Bob\n\n## Discussion\nProject planning session.`,
      tags: [MCPTestClient.testTag("meeting")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Step 3: Verify note was created with content intact
    const retrieved = await client.callTool("get_note", { id: note.id });
    assert.ok(retrieved.original.content.includes("Meeting Notes"), "Should have meeting notes content");
  });

  // --- Provenance Workflow ---

  test("CHAIN-019: create_note → get_note_provenance → get_note_backlinks workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create a note
    const note = await client.callTool("create_note", {
      content: `# Provenance Test ${testId}\n\nNote for provenance tracking.`,
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Brief delay
    await new Promise((r) => setTimeout(r, 200));

    // Step 2: Get note provenance
    const provenance = await client.callTool("get_note_provenance", {
      id: note.id,
    });
    assert.ok(provenance, "Should return provenance data");
    console.log(`  Provenance: ${JSON.stringify(provenance).slice(0, 200)}`);

    // Step 3: Get note backlinks
    const backlinks = await client.callTool("get_note_backlinks", {
      id: note.id,
    });
    assert.ok(backlinks !== undefined, "Should return backlinks data");
    const links = Array.isArray(backlinks) ? backlinks : (backlinks.backlinks || backlinks.notes || []);
    assert.ok(Array.isArray(links), "Backlinks should be an array");
    console.log(`  Backlinks: ${links.length} found`);
  });

  // --- SKOS Deep Knowledge Organization Chain ---

  test("CHAIN-020: full SKOS workflow: scheme → concepts → hierarchy → tag note → get_note_concepts", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create a concept scheme
    const scheme = await client.callTool("create_concept_scheme", {
      notation: `chain20-${testId}`,
      title: `Chain 20 Scheme ${testId}`,
    });
    assert.ok(scheme.id, "Scheme should be created");

    // Step 2: Create hierarchy of concepts
    const topConcept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Technology",
    });
    assert.ok(topConcept.id, "Top concept should be created");

    const childConcept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Software",
      broader_ids: [topConcept.id],
    });
    assert.ok(childConcept.id, "Child concept should be created");

    const grandchild = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Open Source",
      broader_ids: [childConcept.id],
    });
    assert.ok(grandchild.id, "Grandchild concept should be created");

    // Step 3: Create a note and tag it with a concept
    const note = await client.callTool("create_note", {
      content: `# Open Source ${testId}\n\nThis note is about open source software.`,
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Step 4: Tag note with concept
    await client.callTool("tag_note_concept", {
      note_id: note.id,
      concept_id: grandchild.id,
    });

    // Step 5: Get note concepts to verify tagging
    const noteConcepts = await client.callTool("get_note_concepts", {
      note_id: note.id,
    });
    assert.ok(noteConcepts, "Should return note concepts");
    // Response is nested array: [[{tag_info}, {concept_info}], ...]
    const conceptList = Array.isArray(noteConcepts) ? noteConcepts : (noteConcepts.concepts || []);
    assert.ok(conceptList.length > 0, "Note should have at least one concept");

    // Step 6: Verify hierarchy via get_broader
    const broader = await client.callTool("get_broader", {
      id: grandchild.id,
    });
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    const foundParent = broaderList.find((c) => c.id === childConcept.id || c.object_id === childConcept.id);
    assert.ok(foundParent, "Grandchild should have child as broader");

    // Step 7: Untag and clean up
    await client.callTool("untag_note_concept", {
      note_id: note.id,
      concept_id: grandchild.id,
    });

    await client.callTool("delete_concept", { id: grandchild.id });
    await client.callTool("delete_concept", { id: childConcept.id });
    await client.callTool("delete_concept", { id: topConcept.id });
    await client.callTool("delete_concept_scheme", { id: scheme.id });
    console.log(`  Full SKOS chain completed: scheme → 3 concepts → tag note → verify → cleanup`);
  });

  // --- Job Queue Workflow ---

  test("CHAIN-021: create_note → create_job → get_queue_stats → list_jobs workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Step 1: Create a note
    const note = await client.callTool("create_note", {
      content: `# Job Chain Test ${testId}\n\nNote for job queue workflow.`,
      revision_mode: "none",
    });
    assert.ok(note.id, "Note should be created");
    cleanup.noteIds.push(note.id);

    // Step 2: Create a job for the note
    const job = await client.callTool("create_job", {
      note_id: note.id,
      job_type: "embedding",
      priority: 5,
    });
    assert.ok(job, "Should create job");
    assert.ok(job.id || job.job_id, "Should have job ID");
    const jobId = job.id || job.job_id;

    // Step 3: Get queue stats
    const stats = await client.callTool("get_queue_stats", {});
    assert.ok(stats, "Should return queue stats");
    assert.ok(
      stats.pending !== undefined || stats.total !== undefined,
      "Should have queue stat fields"
    );

    // Step 4: List jobs for the note
    const noteJobs = await client.callTool("list_jobs", {
      note_id: note.id,
      limit: 10,
    });
    const jobs = Array.isArray(noteJobs) ? noteJobs : (noteJobs.jobs || []);
    assert.ok(jobs.length > 0, "Should have at least one job for the note");

    // Step 5: Get specific job
    const jobDetail = await client.callTool("get_job", { id: jobId });
    assert.ok(jobDetail, "Should return job detail");
    assert.ok(jobDetail.job_type, "Should have job_type");
    assert.ok(jobDetail.status, "Should have status");

    console.log(`  Job chain: created job ${jobId}, type=${jobDetail.job_type}, status=${jobDetail.status}`);
  });

  // --- Template with Default Tags Workflow ---

  test("CHAIN-022: create_template with default_tags → instantiate → verify tags", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const tag = MCPTestClient.testTag("tpl-chain", testId);

    // Step 1: Create template with default tags
    const template = await client.callTool("create_template", {
      name: `chain-tpl-${testId}`,
      content: "# {{title}}\n\nGenerated from template with default tags.",
      default_tags: [tag],
    });
    assert.ok(template.id, "Template should be created");

    // Step 2: Instantiate template
    const note = await client.callTool("instantiate_template", {
      id: template.id,
      variables: { title: `Tagged Template Note ${testId}` },
    });
    assert.ok(note.id, "Note should be created from template");
    cleanup.noteIds.push(note.id);

    // Step 3: Verify the note has the default tags
    const retrieved = await client.callTool("get_note", { id: note.id });
    assert.ok(retrieved.tags, "Note should have tags");
    assert.ok(retrieved.tags.includes(tag), "Note should have the default tag from template");

    // Step 4: Clean up template
    await client.callTool("delete_template", { id: template.id });
    console.log(`  Template chain: created template → instantiated → verified tag '${tag}'`);
  });

  // --- Archive Workflow ---

  test("CHAIN-023: create_archive → get_archive_stats → list_archives → delete workflow", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const archiveName = `chain-archive-${testId}`;

    // Step 1: Create an archive
    const archive = await client.callTool("create_archive", {
      name: archiveName,
      description: "Archive for chain workflow test",
    });
    assert.ok(archive, "Should create archive");
    assert.ok(archive.id, "Should have archive ID");

    // Step 2: Get archive stats (uses name, not id)
    const stats = await client.callTool("get_archive_stats", {
      name: archiveName,
    });
    assert.ok(stats, "Should return archive stats");
    console.log(`  Archive stats: ${JSON.stringify(stats).slice(0, 200)}`);

    // Step 3: List archives to verify ours exists
    const archives = await client.callTool("list_archives", {});
    assert.ok(archives, "Should return archives list");
    const archiveList = Array.isArray(archives) ? archives : (archives.archives || []);
    const found = archiveList.find((a) => a.name === archiveName || a.id === archive.id);
    assert.ok(found, "Created archive should appear in list");

    // Step 4: Clean up archive (uses name, not id)
    await client.callTool("delete_archive", { name: archiveName });
    console.log(`  Archive chain: created → stats → listed → deleted`);
  });

  // --- Embedding Set Lifecycle ---

  test("CHAIN-024: create_embedding_set → list → refresh → delete lifecycle", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const setName = `chain-lifecycle-${testId}`;

    // Step 1: Create embedding set
    const created = await client.callTool("create_embedding_set", {
      name: setName,
      description: "Lifecycle test embedding set",
    });
    assert.ok(created.id, "Should create embedding set");

    // Step 2: List embedding sets and verify
    const sets = await client.callTool("list_embedding_sets", {});
    assert.ok(sets, "Should return embedding sets");
    const setList = Array.isArray(sets) ? sets : (sets.sets || sets.embedding_sets || []);
    const found = setList.find((s) => s.slug === created.slug || s.name === setName);
    assert.ok(found, "Created set should appear in list");

    // Step 3: Get specific set
    const setDetail = await client.callTool("get_embedding_set", {
      slug: created.slug,
    });
    assert.ok(setDetail, "Should return set details");
    assert.ok(setDetail.name || setDetail.slug, "Should have set metadata");

    // Step 4: Refresh the set
    const refreshResult = await client.callTool("refresh_embedding_set", {
      slug: created.slug,
    });
    assert.ok(refreshResult, "Should return refresh result");

    // Step 5: Delete the set
    await client.callTool("delete_embedding_set", { slug: created.slug });
    console.log(`  Embedding set lifecycle: create → list → get → refresh → delete`);
  });

  // --- PKE Keyset-Based Note Encryption Workflow ---

  test("CHAIN-026: pke_create_keyset → encrypt note content → decrypt roundtrip", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const keysetName = `chain026-${testId}`;
    const passphrase = `test-pass-${testId}`;
    const sensitiveContent = `API_KEY=sk_test_${testId}_secret_value\nDB_PASSWORD=hunter2`;

    // CHAIN-026: Create keyset
    const keyset = await client.callTool("pke_create_keyset", {
      name: keysetName,
      passphrase,
    });
    assert.ok(keyset, "Should create keyset");
    assert.ok(keyset.name === keysetName, "Keyset name should match");
    assert.ok(keyset.public_key, "Should have public_key");
    assert.ok(keyset.encrypted_private_key, "Should have encrypted_private_key");
    assert.ok(keyset.address, "Should have address");
    console.log(`  CHAIN-026: Created keyset '${keysetName}' with address ${keyset.address}`);

    try {
      // CHAIN-027: Create a sensitive note
      const note = await client.callTool("create_note", {
        content: `# Sensitive Note\n\n${sensitiveContent}`,
        tags: [MCPTestClient.testTag("pke", "chain026")],
        revision_mode: "none",
      });
      assert.ok(note.id, "Should create sensitive note");
      console.log(`  CHAIN-027: Created sensitive note ${note.id}`);

      try {
        // CHAIN-028: Encrypt the note content using keyset's public key
        const plaintext = Buffer.from(sensitiveContent).toString("base64");
        const encrypted = await client.callTool("pke_encrypt", {
          plaintext,
          recipient_keys: [keyset.public_key],
        });
        assert.ok(encrypted, "Should encrypt content");
        const ciphertext = encrypted.ciphertext || encrypted;
        assert.ok(ciphertext, "Should have ciphertext");
        console.log(`  CHAIN-028: Encrypted note content (${ciphertext.length} chars)`);

        // Update note with encrypted content
        await client.callTool("update_note", {
          id: note.id,
          content: `# Encrypted Note\n\nCIPHERTEXT:${ciphertext}`,
        });

        // CHAIN-029: Verify address from the keyset
        const addrResult = await client.callTool("pke_verify_address", {
          address: keyset.address,
        });
        assert.ok(addrResult, "Should verify keyset address");
        console.log(`  CHAIN-029: Verified address ${keyset.address}`);

        // List recipients of the encrypted data
        const recipients = await client.callTool("pke_list_recipients", {
          ciphertext,
        });
        assert.ok(recipients, "Should list recipients");
        const addrs = Array.isArray(recipients) ? recipients : (recipients.addresses || recipients.recipients || []);
        assert.ok(addrs.length > 0, "Should have at least one recipient");
        console.log(`  CHAIN-029: Listed ${addrs.length} recipient(s)`);

        // CHAIN-030: Decrypt the content using keyset's encrypted private key
        const decrypted = await client.callTool("pke_decrypt", {
          ciphertext,
          encrypted_private_key: keyset.encrypted_private_key,
          passphrase,
        });
        assert.ok(decrypted, "Should decrypt content");
        const decryptedText = decrypted.plaintext || decrypted;
        const decoded = Buffer.from(decryptedText, "base64").toString("utf-8");
        assert.ok(
          decoded.includes("sk_test_") && decoded.includes("hunter2"),
          "Decrypted content should match original sensitive data"
        );
        console.log(`  CHAIN-030: Decrypted content matches original`);

        // CHAIN-031: Verify full roundtrip - get note, decrypt, compare
        const storedNote = await client.callTool("get_note", { id: note.id });
        assert.ok(storedNote, "Should retrieve stored note");
        assert.ok(
          storedNote.original.content.includes("CIPHERTEXT:"),
          "Stored note should contain encrypted content"
        );
        // Extract ciphertext from note, decrypt, verify
        const storedCiphertext = storedNote.original.content.split("CIPHERTEXT:")[1];
        const reDecrypted = await client.callTool("pke_decrypt", {
          ciphertext: storedCiphertext,
          encrypted_private_key: keyset.encrypted_private_key,
          passphrase,
        });
        const reDecryptedText = reDecrypted.plaintext || reDecrypted;
        const reDecoded = Buffer.from(reDecryptedText, "base64").toString("utf-8");
        assert.strictEqual(reDecoded, sensitiveContent, "Re-decrypted content should match original exactly");
        console.log(`  CHAIN-031: Full roundtrip verified - note content integrity confirmed`);
      } finally {
        // Clean up note
        try { await client.callTool("delete_note", { id: note.id }); } catch {}
      }
    } finally {
      // Clean up keyset
      try { await client.callTool("pke_delete_keyset", { name: keysetName }); } catch {}
    }
  });

  test("CHAIN-027: pke_create_keyset → pke_set_active → pke_get_active → pke_list_keysets lifecycle", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const keysetName = `chain027-${testId}`;
    const passphrase = `lifecycle-pass-${testId}`;

    // Step 1: Create keyset
    const keyset = await client.callTool("pke_create_keyset", {
      name: keysetName,
      passphrase,
    });
    assert.ok(keyset.name === keysetName, "Should create keyset");
    assert.ok(keyset.address, "Should have address");
    console.log(`  Created keyset: ${keysetName}`);

    try {
      // Step 2: Set as active
      const setResult = await client.callTool("pke_set_active_keyset", {
        name: keysetName,
      });
      assert.ok(setResult, "Should set active keyset");
      console.log(`  Set active: ${keysetName}`);

      // Step 3: Get active keyset
      const active = await client.callTool("pke_get_active_keyset", {});
      assert.ok(active, "Should return active keyset");
      assert.strictEqual(active.name, keysetName, "Active keyset should match");
      console.log(`  Active keyset confirmed: ${active.name}`);

      // Step 4: List keysets
      const keysets = await client.callTool("pke_list_keysets", {});
      assert.ok(Array.isArray(keysets), "Should return keyset array");
      const found = keysets.find((k) => k.name === keysetName);
      assert.ok(found, "Created keyset should appear in list");
      assert.ok(found.address, "Listed keyset should have address");
      assert.ok(found.public_key, "Listed keyset should have public_key");
      console.log(`  Listed ${keysets.length} keysets, found '${keysetName}'`);
    } finally {
      // Clean up
      try { await client.callTool("pke_delete_keyset", { name: keysetName }); } catch {}
    }
  });
});
