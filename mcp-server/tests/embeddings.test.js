#!/usr/bin/env node

/**
 * MCP Embedding Sets Tests (Phase 7)
 *
 * Tests embedding set management via MCP tools:
 * - list_embedding_sets: List all embedding sets
 * - create_embedding_set: Create new embedding set with config
 * - get embedding set: Retrieve set by slug
 * - delete_embedding_set: Remove an embedding set
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 7: Embedding Sets", () => {
  let client;
  const cleanup = { embeddingSetIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Clean up embedding sets
    for (const id of cleanup.embeddingSetIds) {
      try {
        await client.callTool("delete_embedding_set", { id });
      } catch (e) {
        console.error(`Failed to delete embedding set ${id}:`, e.message);
      }
    }

    await client.close();
  });

  test("EMBED-001: list_embedding_sets returns array", async () => {
    const result = await client.callTool("list_embedding_sets");

    assert.ok(Array.isArray(result), "Result should be an array");
    // Default set should exist
    assert.ok(result.length >= 1, "At least default embedding set should exist");

    // Verify structure of first item
    if (result.length > 0) {
      const set = result[0];
      assert.ok(set.id, "Embedding set should have ID");
      assert.ok(set.name, "Embedding set should have name");
      assert.ok(set.slug, "Embedding set should have slug");
    }
  });

  test("EMBED-002: create_embedding_set with basic config", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-embed-${testId}`;

    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });

    assert.ok(result.id, "Embedding set should be created with ID");
    assert.ok(result.slug, "Embedding set should have slug");
    assert.strictEqual(result.name, name, "Name should match");
    assert.strictEqual(result.model, "nomic-embed-text:latest", "Model should match");
    assert.strictEqual(result.dimensions, 768, "Dimensions should match");

    cleanup.embeddingSetIds.push(result.id);
  });

  test("EMBED-003: get_embedding_set by slug", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-get-${testId}`;

    // Create embedding set
    const created = await client.callTool("create_embedding_set", {
      name,
      model: "all-minilm:latest",
      dimensions: 384,
    });
    cleanup.embeddingSetIds.push(created.id);

    // Retrieve by slug
    const retrieved = await client.callTool("get_embedding_set", {
      slug: created.slug,
    });

    assert.ok(retrieved, "Embedding set should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.slug, created.slug, "Slug should match");
    assert.strictEqual(retrieved.name, name, "Name should match");
  });

  test("EMBED-004: get_embedding_set by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-getbyid-${testId}`;

    // Create embedding set
    const created = await client.callTool("create_embedding_set", {
      name,
      model: "mxbai-embed-large:latest",
      dimensions: 1024,
    });
    cleanup.embeddingSetIds.push(created.id);

    // Retrieve by ID
    const retrieved = await client.callTool("get_embedding_set", {
      id: created.id,
    });

    assert.ok(retrieved, "Embedding set should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.name, name, "Name should match");
  });

  test("EMBED-005: delete_embedding_set removes set", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-delete-${testId}`;

    // Create embedding set
    const created = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });

    // Delete it
    await client.callTool("delete_embedding_set", { id: created.id });

    // Verify it's gone
    const error = await client.callToolExpectError("get_embedding_set", {
      id: created.id,
    });

    assert.ok(error.error, "Should return error for deleted set");
  });

  test("EMBED-006: create_embedding_set with MRL config", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-mrl-${testId}`;

    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
      matryoshka_dim: 64, // MRL dimension for efficient storage
    });

    assert.ok(result.id, "MRL embedding set should be created");
    assert.strictEqual(result.matryoshka_dim, 64, "MRL dimension should be stored");

    cleanup.embeddingSetIds.push(result.id);
  });

  test("EMBED-007: create_embedding_set with filter mode", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-filter-${testId}`;

    // Create filter set (shares embeddings with default)
    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
      filter_mode: true,
    });

    assert.ok(result.id, "Filter embedding set should be created");
    cleanup.embeddingSetIds.push(result.id);
  });

  test("EMBED-008: create_embedding_set with full mode and parent", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Get default embedding set ID
    const sets = await client.callTool("list_embedding_sets");
    const defaultSet = sets.find((s) => s.slug === "default");

    if (!defaultSet) {
      console.warn("Default embedding set not found, skipping test");
      return;
    }

    const name = `test-full-${testId}`;

    // Create full set with parent
    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
      filter_mode: false,
      parent_id: defaultSet.id,
    });

    assert.ok(result.id, "Full embedding set should be created");
    assert.strictEqual(result.filter_mode, false, "Should be full mode");

    cleanup.embeddingSetIds.push(result.id);
  });

  test("EMBED-009: list_embedding_sets includes created sets", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-list-${testId}`;

    // Create a new set
    const created = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });
    cleanup.embeddingSetIds.push(created.id);

    // List all sets
    const sets = await client.callTool("list_embedding_sets");

    // Find our created set
    const found = sets.find((s) => s.id === created.id);
    assert.ok(found, "Created set should appear in list");
    assert.strictEqual(found.name, name, "Name should match in list");
  });

  test("EMBED-010: create_embedding_set error - duplicate name", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-duplicate-${testId}`;

    // Create first set
    const first = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });
    cleanup.embeddingSetIds.push(first.id);

    // Try to create duplicate
    const error = await client.callToolExpectError("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
    });

    assert.ok(error.error, "Should return error for duplicate name");
    assert.ok(
      error.error.includes("exists") ||
        error.error.includes("duplicate") ||
        error.error.includes("unique"),
      "Error should mention conflict"
    );
  });

  test("EMBED-011: get_embedding_set error - non-existent slug", async () => {
    const fakeSlug = `non-existent-${MCPTestClient.uniqueId()}`;

    const error = await client.callToolExpectError("get_embedding_set", {
      slug: fakeSlug,
    });

    assert.ok(error.error, "Should return error for non-existent slug");
  });

  test("EMBED-012: create_embedding_set with description", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-desc-${testId}`;
    const description = "Test embedding set with custom description";

    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
      description,
    });

    assert.ok(result.id, "Embedding set should be created");
    assert.strictEqual(result.description, description, "Description should match");

    cleanup.embeddingSetIds.push(result.id);
  });

  test("EMBED-013: create_embedding_set with auto-embed rules", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-auto-${testId}`;

    const result = await client.callTool("create_embedding_set", {
      name,
      model: "nomic-embed-text:latest",
      dimensions: 768,
      auto_embed_on_create: true,
      auto_embed_on_update: true,
    });

    assert.ok(result.id, "Embedding set should be created");
    cleanup.embeddingSetIds.push(result.id);
  });
});
