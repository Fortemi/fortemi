#!/usr/bin/env node

/**
 * MCP SKOS Concepts Tests (Phase 13)
 *
 * Tests W3C SKOS semantic tagging system via MCP tools:
 * - list_concept_schemes: List all concept schemes
 * - create_concept_scheme: Create new concept scheme
 * - get_concept_scheme: Retrieve scheme by ID
 * - create_concept: Create concept in scheme
 * - list_concepts: List concepts in scheme
 * - get_concept: Retrieve concept by ID
 * - delete_concept: Remove concept
 * - delete_concept_scheme: Remove scheme
 *
 * SKOS concepts provide hierarchical semantic tagging with
 * broader/narrower/related relationships.
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 13: SKOS Concepts", () => {
  let client;
  const cleanup = { schemeIds: [], conceptIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Clean up concepts first
    for (const id of cleanup.conceptIds) {
      try {
        await client.callTool("delete_concept", { id });
      } catch (e) {
        console.error(`Failed to delete concept ${id}:`, e.message);
      }
    }

    // Clean up schemes
    for (const id of cleanup.schemeIds) {
      try {
        await client.callTool("delete_concept_scheme", { id });
      } catch (e) {
        console.error(`Failed to delete concept scheme ${id}:`, e.message);
      }
    }

    await client.close();
  });

  test("SKOS-001: list_concept_schemes returns array", async () => {
    const result = await client.callTool("list_concept_schemes");

    assert.ok(Array.isArray(result), "Result should be an array");
    // May or may not have schemes depending on state
  });

  test("SKOS-002: create_concept_scheme with basic info", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `Test Scheme ${testId}`;

    const result = await client.callTool("create_concept_scheme", {
      title,
      description: "Test concept scheme for unit tests",
    });

    assert.ok(result.id, "Concept scheme should be created with ID");
    assert.strictEqual(result.title, title, "Title should match");
    assert.ok(result.description, "Description should be set");

    cleanup.schemeIds.push(result.id);
  });

  test("SKOS-003: get_concept_scheme by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `Test Get Scheme ${testId}`;

    // Create scheme
    const created = await client.callTool("create_concept_scheme", {
      title,
      description: "Get test",
    });
    cleanup.schemeIds.push(created.id);

    // Retrieve by ID
    const retrieved = await client.callTool("get_concept_scheme", {
      id: created.id,
    });

    assert.ok(retrieved, "Concept scheme should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.title, title, "Title should match");
  });

  test("SKOS-004: list_concept_schemes includes created scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `Test List Scheme ${testId}`;

    const created = await client.callTool("create_concept_scheme", {
      title,
    });
    cleanup.schemeIds.push(created.id);

    const schemes = await client.callTool("list_concept_schemes");

    const found = schemes.find((s) => s.id === created.id);
    assert.ok(found, "Created scheme should appear in list");
    assert.strictEqual(found.title, title, "Title should match in list");
  });

  test("SKOS-005: create_concept in scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const schemeTitle = `Concept Scheme ${testId}`;

    // Create scheme
    const scheme = await client.callTool("create_concept_scheme", {
      title: schemeTitle,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create concept in scheme
    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Test Concept",
      definition: "A test concept for unit testing",
    });

    assert.ok(result.id, "Concept should be created with ID");
    assert.strictEqual(result.pref_label, "Test Concept", "Pref label should match");
    assert.strictEqual(result.scheme_id, scheme.id, "Should belong to scheme");

    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-006: list_concepts in scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Create scheme
    const scheme = await client.callTool("create_concept_scheme", {
      title: `List Concepts Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create multiple concepts
    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept A",
    });
    cleanup.conceptIds.push(concept1.id);

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept B",
    });
    cleanup.conceptIds.push(concept2.id);

    // List concepts
    const concepts = await client.callTool("list_concepts", {
      scheme_id: scheme.id,
    });

    assert.ok(Array.isArray(concepts), "Concepts should be an array");
    assert.ok(concepts.length >= 2, "Should have at least 2 concepts");

    const foundA = concepts.find((c) => c.id === concept1.id);
    const foundB = concepts.find((c) => c.id === concept2.id);
    assert.ok(foundA, "Concept A should be in list");
    assert.ok(foundB, "Concept B should be in list");
  });

  test("SKOS-007: get_concept by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Create scheme and concept
    const scheme = await client.callTool("create_concept_scheme", {
      title: `Get Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const created = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Test Concept for Get",
      definition: "Test definition",
    });
    cleanup.conceptIds.push(created.id);

    // Retrieve concept
    const retrieved = await client.callTool("get_concept", {
      id: created.id,
    });

    assert.ok(retrieved, "Concept should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.pref_label, "Test Concept for Get", "Label should match");
  });

  test("SKOS-008: create_concept with alt_labels", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Alt Labels Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Primary Label",
      alt_labels: ["Alternative 1", "Alternative 2"],
    });

    assert.ok(result.id, "Concept should be created");
    assert.ok(result.alt_labels, "Should have alt labels");
    assert.ok(Array.isArray(result.alt_labels), "Alt labels should be array");
    assert.ok(result.alt_labels.includes("Alternative 1"), "Should include alt label 1");
    assert.ok(result.alt_labels.includes("Alternative 2"), "Should include alt label 2");

    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-009: create_concept with broader relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Hierarchy Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create parent concept
    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent Concept",
    });
    cleanup.conceptIds.push(parent.id);

    // Create child concept
    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Child Concept",
      broader: [parent.id],
    });
    cleanup.conceptIds.push(child.id);

    assert.ok(child.id, "Child concept should be created");
    assert.ok(child.broader, "Should have broader relationship");
    assert.ok(child.broader.includes(parent.id), "Should link to parent");
  });

  test("SKOS-010: create_concept with related relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Related Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create two related concepts
    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept 1",
    });
    cleanup.conceptIds.push(concept1.id);

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept 2",
      related: [concept1.id],
    });
    cleanup.conceptIds.push(concept2.id);

    assert.ok(concept2.id, "Concept 2 should be created");
    assert.ok(concept2.related, "Should have related concepts");
    assert.ok(concept2.related.includes(concept1.id), "Should link to concept 1");
  });

  test("SKOS-011: delete_concept removes concept", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Delete Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "To Be Deleted",
    });

    // Delete concept
    await client.callTool("delete_concept", { id: concept.id });

    // Verify it's gone
    const error = await client.callToolExpectError("get_concept", {
      id: concept.id,
    });

    assert.ok(error.error, "Should return error for deleted concept");
  });

  test("SKOS-012: delete_concept_scheme removes scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `To Be Deleted ${testId}`,
    });

    // Delete scheme
    await client.callTool("delete_concept_scheme", { id: scheme.id });

    // Verify it's gone
    const error = await client.callToolExpectError("get_concept_scheme", {
      id: scheme.id,
    });

    assert.ok(error.error, "Should return error for deleted scheme");
  });

  test("SKOS-013: create_concept_scheme with URI", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `URI Scheme ${testId}`;
    const uri = `http://example.org/schemes/${testId}`;

    const result = await client.callTool("create_concept_scheme", {
      title,
      uri,
    });

    assert.ok(result.id, "Scheme should be created");
    assert.strictEqual(result.uri, uri, "URI should match");

    cleanup.schemeIds.push(result.id);
  });

  test("SKOS-014: create_concept with notation", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Notation Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept with Notation",
      notation: "ABC-123",
    });

    assert.ok(result.id, "Concept should be created");
    assert.strictEqual(result.notation, "ABC-123", "Notation should match");

    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-015: update_concept modifies label", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Update Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Original Label",
    });
    cleanup.conceptIds.push(concept.id);

    // Update concept
    const updated = await client.callTool("update_concept", {
      id: concept.id,
      pref_label: "Updated Label",
    });

    assert.strictEqual(updated.pref_label, "Updated Label", "Label should be updated");

    // Verify via get
    const retrieved = await client.callTool("get_concept", {
      id: concept.id,
    });
    assert.strictEqual(retrieved.pref_label, "Updated Label", "Updated label should persist");
  });

  test("SKOS-016: create_concept error - invalid scheme_id", async () => {
    const fakeSchemeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("create_concept", {
      scheme_id: fakeSchemeId,
      pref_label: "Invalid Scheme Test",
    });

    assert.ok(error.error, "Should return error for invalid scheme");
  });

  test("SKOS-017: list_concepts empty for new scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Empty Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concepts = await client.callTool("list_concepts", {
      scheme_id: scheme.id,
    });

    assert.ok(Array.isArray(concepts), "Concepts should be an array");
    assert.strictEqual(concepts.length, 0, "New scheme should have no concepts");
  });

  test("SKOS-018: create_concept with multilingual labels", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Multilingual Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Computer",
      pref_label_lang: "en",
      alt_labels: ["Ordinateur", "Computadora"],
    });

    assert.ok(result.id, "Multilingual concept should be created");
    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-019: export_concept_scheme_turtle", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Export Scheme ${testId}`,
      uri: `http://example.org/schemes/export-${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create concept
    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Exportable Concept",
    });
    cleanup.conceptIds.push(concept.id);

    // Export as Turtle/RDF
    const turtle = await client.callTool("export_concept_scheme_turtle", {
      scheme_id: scheme.id,
    });

    assert.ok(turtle, "Should return turtle content");
    assert.ok(typeof turtle === "string", "Turtle should be string");
    assert.ok(turtle.includes("@prefix"), "Should include RDF prefixes");
  });

  test("SKOS-020: concept scheme has top concepts", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      title: `Top Concepts Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create top-level concept (no broader)
    const topConcept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Top Level Concept",
      top_concept: true,
    });
    cleanup.conceptIds.push(topConcept.id);

    // Get scheme and check top concepts
    const retrieved = await client.callTool("get_concept_scheme", {
      id: scheme.id,
    });

    assert.ok(retrieved.top_concepts, "Scheme should have top concepts");
    assert.ok(
      retrieved.top_concepts.includes(topConcept.id),
      "Top concept should be in list"
    );
  });
});
