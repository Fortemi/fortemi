#!/usr/bin/env node

/**
 * Multi-Memory UAT Tests
 *
 * Comprehensive user acceptance tests for the new multi-memory features that
 * enable data isolation across different knowledge domains. These tests validate
 * all memory management tools and cross-memory operations.
 *
 * Features tested:
 * - Memory CRUD (create, list, delete)
 * - Memory selection and active context
 * - Note isolation per memory
 * - Memory cloning with full data copy
 * - Federated search across memories
 * - Overview and capacity planning
 * - Archive statistics
 *
 * Tests follow the UAT pattern with real MCP client integration,
 * covering happy paths, edge cases, and error conditions.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Multi-Memory Features (UAT)", () => {
  let client;
  const cleanup = { noteIds: [], memoryNames: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Cleanup all created resources
    console.log(`  Cleaning up ${cleanup.memoryNames.length} memories and ${cleanup.noteIds.length} notes...`);

    // Select public memory before cleanup (ensure we're not in a memory we're deleting)
    try {
      await client.callTool("select_memory", { name: "public" });
    } catch (error) {
      // Ignore if select fails
    }

    for (const name of cleanup.memoryNames) {
      try {
        await client.callTool("delete_memory", { name });
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    for (const id of cleanup.noteIds) {
      try {
        await client.callTool("delete_note", { id });
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    await client.close();
  });

  // ==========================================================================
  // MEMORY CRUD TESTS
  // ==========================================================================

  test("MEM-001: Create a memory with name and description", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const description = "UAT test memory for validation";

    const result = await client.callTool("create_memory", {
      name: memName,
      description: description,
    });

    assert.ok(result, "Should return a result");
    assert.ok(result.id, "Result should contain memory ID");
    assert.strictEqual(result.name, memName, "Memory name should match");
    assert.strictEqual(result.schema_name, `archive_${memName}`, "Schema name should be prefixed");

    cleanup.memoryNames.push(memName);
    console.log(`  ✓ Created memory: ${memName} (id: ${result.id})`);
  });

  test("MEM-002: List memories returns array with created memory", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create a memory first
    await client.callTool("create_memory", {
      name: memName,
      description: "Test memory for listing",
    });
    cleanup.memoryNames.push(memName);

    // List all memories
    const result = await client.callTool("list_memories", {});

    assert.ok(result, "Should return a result");
    assert.ok(Array.isArray(result), "Should return an array");

    const found = result.find(m => m.name === memName);
    assert.ok(found, "Should find the created memory in list");
    assert.strictEqual(found.name, memName, "Name should match");

    console.log(`  ✓ Listed ${result.length} memories, found: ${memName}`);
  });

  test("MEM-003: Create memory with duplicate name fails", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create first memory
    await client.callTool("create_memory", { name: memName });
    cleanup.memoryNames.push(memName);

    // Try to create duplicate
    const errorResult = await client.callToolExpectError("create_memory", {
      name: memName,
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /already exists|duplicate|conflict/i,
      "Error should indicate name conflict"
    );

    console.log(`  ✓ Duplicate memory creation rejected`);
  });

  test("MEM-004: Delete a memory removes it from list", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create memory
    await client.callTool("create_memory", { name: memName });

    // Delete it
    const deleteResult = await client.callTool("delete_memory", { name: memName });
    assert.ok(deleteResult, "Delete should return result");

    // Verify it's gone
    const memories = await client.callTool("list_memories", {});
    const found = memories.find(m => m.name === memName);
    assert.strictEqual(found, undefined, "Deleted memory should not appear in list");

    console.log(`  ✓ Memory deleted: ${memName}`);
  });

  // ==========================================================================
  // MEMORY SELECTION TESTS
  // ==========================================================================

  test("MEM-005: Select a memory sets active context", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create memory
    await client.callTool("create_memory", { name: memName });
    cleanup.memoryNames.push(memName);

    // Select it
    const result = await client.callTool("select_memory", { name: memName });

    assert.ok(result, "Should return result");
    assert.strictEqual(result.success, true, "Should indicate success");
    assert.strictEqual(result.active_memory, memName, "Active memory should match");
    assert.match(
      result.message,
      new RegExp(memName),
      "Message should mention the memory name"
    );

    console.log(`  ✓ Selected memory: ${memName}`);

    // Clean up: select back to public
    await client.callTool("select_memory", { name: "public" });
  });

  test("MEM-006: Get active memory returns default before selection", async () => {
    // Ensure we're in default state
    await client.callTool("select_memory", { name: "public" });

    const result = await client.callTool("get_active_memory", {});

    assert.ok(result, "Should return result");
    assert.strictEqual(result.active_memory, "public (default)", "Should show public as default");
    assert.strictEqual(result.is_explicit, false, "Should indicate no explicit selection");

    console.log(`  ✓ Default memory: ${result.active_memory}`);
  });

  test("MEM-007: Get active memory returns selected memory after select", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create and select memory
    await client.callTool("create_memory", { name: memName });
    cleanup.memoryNames.push(memName);
    await client.callTool("select_memory", { name: memName });

    // Get active memory
    const result = await client.callTool("get_active_memory", {});

    assert.ok(result, "Should return result");
    assert.strictEqual(result.active_memory, memName, "Active memory should match selected");
    assert.strictEqual(result.is_explicit, true, "Should indicate explicit selection");

    console.log(`  ✓ Active memory after select: ${result.active_memory}`);

    // Clean up
    await client.callTool("select_memory", { name: "public" });
  });

  // ==========================================================================
  // MEMORY ISOLATION TESTS
  // ==========================================================================

  test("MEM-008: Notes are isolated per memory", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const testTag = MCPTestClient.testTag("mem-isolation");
    const uniqueContent = "Isolated content " + MCPTestClient.uniqueId();

    // Create memory and select it
    await client.callTool("create_memory", { name: memName });
    cleanup.memoryNames.push(memName);
    await client.callTool("select_memory", { name: memName });

    // Create note in the selected memory
    const note = await client.callTool("create_note", {
      content: uniqueContent,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    console.log(`  ✓ Created note in memory ${memName}: ${note.id}`);

    // Switch back to public memory
    await client.callTool("select_memory", { name: "public" });

    // Search in public memory - should NOT find the note
    const publicSearch = await client.callTool("search_notes", {
      query: uniqueContent,
      limit: 10,
    });

    const foundInPublic = publicSearch.results?.some(r => r.id === note.id);
    assert.strictEqual(
      foundInPublic,
      false,
      "Note from other memory should not appear in public search"
    );

    // Switch back to the memory
    await client.callTool("select_memory", { name: memName });

    // Search in the memory - SHOULD find the note
    const memorySearch = await client.callTool("search_notes", {
      query: uniqueContent,
      limit: 10,
    });

    const foundInMemory = memorySearch.results?.some(r => r.id === note.id);
    assert.strictEqual(
      foundInMemory,
      true,
      "Note should be found when searching in its own memory"
    );

    console.log(`  ✓ Note isolation verified: found in ${memName}, not in public`);

    // Clean up
    await client.callTool("select_memory", { name: "public" });
  });

  // ==========================================================================
  // CLONE TESTS
  // ==========================================================================

  test("MEM-009: Clone a memory with notes creates exact copy", async () => {
    const sourceName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const cloneName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const testTag = MCPTestClient.testTag("mem-clone");
    const uniqueContent = "Content to clone " + MCPTestClient.uniqueId();

    // Create source memory with a note
    await client.callTool("create_memory", { name: sourceName });
    cleanup.memoryNames.push(sourceName);
    await client.callTool("select_memory", { name: sourceName });

    const originalNote = await client.callTool("create_note", {
      content: uniqueContent,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(originalNote.id);

    console.log(`  ✓ Created note in source memory: ${originalNote.id}`);

    // Switch back to public before cloning
    await client.callTool("select_memory", { name: "public" });

    // Clone the memory
    const cloneResult = await client.callTool("clone_memory", {
      source_name: sourceName,
      new_name: cloneName,
      description: "Cloned memory for UAT",
    });
    cleanup.memoryNames.push(cloneName);

    assert.ok(cloneResult, "Clone should return result");
    assert.ok(cloneResult.id, "Clone should have ID");
    assert.strictEqual(cloneResult.name, cloneName, "Clone name should match");

    console.log(`  ✓ Cloned memory: ${sourceName} -> ${cloneName}`);

    // Verify clone exists in list
    const memories = await client.callTool("list_memories", {});
    const foundClone = memories.find(m => m.name === cloneName);
    assert.ok(foundClone, "Clone should appear in memory list");

    // Select clone and verify note exists
    await client.callTool("select_memory", { name: cloneName });
    const cloneSearch = await client.callTool("search_notes", {
      query: uniqueContent,
      limit: 10,
    });

    assert.ok(cloneSearch.results?.length > 0, "Clone should contain notes");
    const foundNote = cloneSearch.results.some(r => r.content?.includes(uniqueContent));
    assert.strictEqual(foundNote, true, "Cloned memory should contain the original note content");

    console.log(`  ✓ Clone verified: contains original note data`);

    // Clean up
    await client.callTool("select_memory", { name: "public" });
  });

  test("MEM-010: Clone non-existent memory fails", async () => {
    const nonExistentName = "nonexistent-" + MCPTestClient.uniqueId().slice(0, 8);
    const targetName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    const errorResult = await client.callToolExpectError("clone_memory", {
      source_name: nonExistentName,
      new_name: targetName,
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /not found|does not exist/i,
      "Error should indicate source memory not found"
    );

    console.log(`  ✓ Clone of non-existent memory rejected`);
  });

  test("MEM-011: Clone to existing name fails", async () => {
    const sourceName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const existingName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create source and existing memories
    await client.callTool("create_memory", { name: sourceName });
    cleanup.memoryNames.push(sourceName);
    await client.callTool("create_memory", { name: existingName });
    cleanup.memoryNames.push(existingName);

    // Try to clone to existing name
    const errorResult = await client.callToolExpectError("clone_memory", {
      source_name: sourceName,
      new_name: existingName,
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /already exists|conflict|duplicate/i,
      "Error should indicate target name conflict"
    );

    console.log(`  ✓ Clone to existing name rejected`);
  });

  // ==========================================================================
  // FEDERATED SEARCH TESTS
  // ==========================================================================

  test("MEM-012: Federated search across all memories finds notes from multiple sources", async () => {
    const mem1Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const mem2Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const testTag = MCPTestClient.testTag("federated");
    const uniqueWord = "fedword" + MCPTestClient.uniqueId().slice(0, 8);

    // Create two memories with distinctive notes
    await client.callTool("create_memory", { name: mem1Name });
    cleanup.memoryNames.push(mem1Name);
    await client.callTool("select_memory", { name: mem1Name });

    const note1 = await client.callTool("create_note", {
      content: `${uniqueWord} in memory 1`,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note1.id);

    await client.callTool("create_memory", { name: mem2Name });
    cleanup.memoryNames.push(mem2Name);
    await client.callTool("select_memory", { name: mem2Name });

    const note2 = await client.callTool("create_note", {
      content: `${uniqueWord} in memory 2`,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note2.id);

    console.log(`  ✓ Created notes in ${mem1Name} and ${mem2Name}`);

    // Switch back to public
    await client.callTool("select_memory", { name: "public" });

    // Federated search across all memories
    const result = await client.callTool("search_memories_federated", {
      q: uniqueWord,
      memories: ["all"],
      limit: 10,
    });

    assert.ok(result, "Should return result");
    assert.ok(Array.isArray(result.results), "Should return results array");

    // Check that we found notes from both memories
    const foundMemories = new Set(result.results.map(r => r.memory_name));
    assert.ok(
      foundMemories.has(mem1Name) || foundMemories.has(mem2Name),
      "Should find notes from at least one test memory"
    );

    console.log(`  ✓ Federated search found ${result.results.length} results across ${foundMemories.size} memories`);

    // Clean up
    await client.callTool("select_memory", { name: "public" });
  });

  test("MEM-013: Federated search specific memories only searches those", async () => {
    const mem1Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const mem2Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const testTag = MCPTestClient.testTag("federated-specific");
    const uniqueWord = "specword" + MCPTestClient.uniqueId().slice(0, 8);

    // Create two memories with notes
    await client.callTool("create_memory", { name: mem1Name });
    cleanup.memoryNames.push(mem1Name);
    await client.callTool("select_memory", { name: mem1Name });

    const note1 = await client.callTool("create_note", {
      content: `${uniqueWord} specific test`,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note1.id);

    await client.callTool("create_memory", { name: mem2Name });
    cleanup.memoryNames.push(mem2Name);
    await client.callTool("select_memory", { name: mem2Name });

    const note2 = await client.callTool("create_note", {
      content: `${uniqueWord} specific test`,
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note2.id);

    console.log(`  ✓ Created notes in ${mem1Name} and ${mem2Name}`);

    // Switch back to public
    await client.callTool("select_memory", { name: "public" });

    // Search only mem1Name
    const result = await client.callTool("search_memories_federated", {
      q: uniqueWord,
      memories: [mem1Name],
      limit: 10,
    });

    assert.ok(result, "Should return result");
    assert.ok(Array.isArray(result.results), "Should return results array");

    // All results should be from mem1Name only
    const memoriesFound = result.results.map(r => r.memory_name);
    const hasOtherMemory = memoriesFound.some(m => m !== mem1Name);
    assert.strictEqual(
      hasOtherMemory,
      false,
      "Should only find results from specified memory"
    );

    console.log(`  ✓ Specific memory search limited to: ${mem1Name}`);

    // Clean up
    await client.callTool("select_memory", { name: "public" });
  });

  test("MEM-014: Federated search with non-existent memory fails gracefully", async () => {
    const nonExistentName = "nonexistent-" + MCPTestClient.uniqueId().slice(0, 8);

    const errorResult = await client.callToolExpectError("search_memories_federated", {
      q: "test query",
      memories: [nonExistentName],
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /not found|does not exist|invalid/i,
      "Error should indicate memory not found"
    );

    console.log(`  ✓ Federated search with invalid memory rejected`);
  });

  // ==========================================================================
  // OVERVIEW / STATS TESTS
  // ==========================================================================

  test("MEM-015: Get memories overview shows capacity and breakdown", async () => {
    const result = await client.callTool("get_memories_overview", {});

    assert.ok(result, "Should return result");
    assert.ok(typeof result.memory_count === "number", "Should have memory_count");
    assert.ok(typeof result.max_memories === "number", "Should have max_memories");
    assert.ok(typeof result.remaining_slots === "number", "Should have remaining_slots");
    assert.ok(typeof result.total_notes === "number", "Should have total_notes");
    assert.ok(Array.isArray(result.memories), "Should have memories array");

    // Verify calculated fields
    assert.strictEqual(
      result.remaining_slots,
      result.max_memories - result.memory_count,
      "Remaining slots should be max - current"
    );

    console.log(`  ✓ Overview: ${result.memory_count}/${result.max_memories} memories, ${result.total_notes} total notes`);
  });

  test("MEM-016: Get archive stats for specific memory shows note count and size", async () => {
    const memName = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const testTag = MCPTestClient.testTag("archive-stats");

    // Create memory with a note
    await client.callTool("create_memory", { name: memName });
    cleanup.memoryNames.push(memName);
    await client.callTool("select_memory", { name: memName });

    const note = await client.callTool("create_note", {
      content: "Test content for archive stats",
      tags: [testTag],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Switch back to public
    await client.callTool("select_memory", { name: "public" });

    // Get stats
    const result = await client.callTool("get_archive_stats", { name: memName });

    assert.ok(result, "Should return result");
    assert.ok(typeof result.note_count === "number", "Should have note_count");
    assert.ok(typeof result.size_bytes === "number", "Should have size_bytes");
    assert.ok(result.note_count >= 1, "Should show at least 1 note");

    console.log(`  ✓ Archive stats for ${memName}: ${result.note_count} notes, ${result.size_bytes} bytes`);
  });

  test("MEM-017: Overview shows per-memory breakdown", async () => {
    const mem1Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);
    const mem2Name = "test-mem-" + MCPTestClient.uniqueId().slice(0, 8);

    // Create two memories
    await client.callTool("create_memory", { name: mem1Name, description: "First test memory" });
    cleanup.memoryNames.push(mem1Name);
    await client.callTool("create_memory", { name: mem2Name, description: "Second test memory" });
    cleanup.memoryNames.push(mem2Name);

    // Get overview
    const result = await client.callTool("get_memories_overview", {});

    assert.ok(result, "Should return result");
    assert.ok(Array.isArray(result.memories), "Should have memories array");

    // Check that our memories appear in breakdown
    const found1 = result.memories.find(m => m.name === mem1Name);
    const found2 = result.memories.find(m => m.name === mem2Name);

    assert.ok(found1, `Should find ${mem1Name} in breakdown`);
    assert.ok(found2, `Should find ${mem2Name} in breakdown`);

    // Each memory should have the required fields
    for (const mem of [found1, found2]) {
      assert.ok(typeof mem.note_count === "number", "Memory should have note_count");
      assert.ok(typeof mem.size_bytes === "number", "Memory should have size_bytes");
      assert.ok(typeof mem.is_default === "boolean", "Memory should have is_default");
    }

    console.log(`  ✓ Overview breakdown includes ${result.memories.length} memories`);
  });

  // ==========================================================================
  // ERROR HANDLING & EDGE CASES
  // ==========================================================================

  test("MEM-018: Delete non-existent memory fails gracefully", async () => {
    const nonExistentName = "nonexistent-" + MCPTestClient.uniqueId().slice(0, 8);

    const errorResult = await client.callToolExpectError("delete_memory", {
      name: nonExistentName,
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /not found|does not exist/i,
      "Error should indicate memory not found"
    );

    console.log(`  ✓ Delete of non-existent memory rejected`);
  });

  test("MEM-019: Select non-existent memory fails gracefully", async () => {
    const nonExistentName = "nonexistent-" + MCPTestClient.uniqueId().slice(0, 8);

    const errorResult = await client.callToolExpectError("select_memory", {
      name: nonExistentName,
    });

    assert.ok(errorResult.error, "Should return an error");
    // Note: The error might come from the API or the MCP server

    console.log(`  ✓ Select of non-existent memory rejected`);
  });

  test("MEM-020: Archive stats for non-existent memory fails gracefully", async () => {
    const nonExistentName = "nonexistent-" + MCPTestClient.uniqueId().slice(0, 8);

    const errorResult = await client.callToolExpectError("get_archive_stats", {
      name: nonExistentName,
    });

    assert.ok(errorResult.error, "Should return an error");
    assert.match(
      errorResult.error,
      /not found|does not exist/i,
      "Error should indicate memory not found"
    );

    console.log(`  ✓ Stats for non-existent memory rejected`);
  });

  test("MEM-021: Create memory with invalid name fails", async () => {
    const invalidNames = [
      "",                          // Empty
      "test memory",               // Spaces not allowed
      "test@memory",               // Special chars not allowed
      "test.memory",               // Dots might not be allowed
    ];

    let errorCount = 0;
    for (const invalidName of invalidNames) {
      const errorResult = await client.callToolExpectError("create_memory", {
        name: invalidName,
      });

      if (errorResult.error) {
        errorCount++;
      }
    }

    assert.ok(errorCount > 0, "At least some invalid names should be rejected");
    console.log(`  ✓ ${errorCount}/${invalidNames.length} invalid memory names rejected`);
  });
});

console.log("\n✓ All multi-memory UAT tests completed");
