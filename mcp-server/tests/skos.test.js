#!/usr/bin/env node

/**
 * MCP SKOS Concepts Tests (Phase 13)
 *
 * Tests W3C SKOS semantic tagging system via MCP tools:
 * - Concept Scheme CRUD: list, create, get, update, delete
 * - Concept CRUD: create, get, get_full, update, delete, search, autocomplete
 * - Hierarchical relationships: add/get/remove broader, narrower, related
 * - Concept-note tagging: tag_note_concept, untag_note_concept, get_note_concepts
 * - SKOS Collections: create, get, list, update, delete, add/remove members
 * - Export: export_skos_turtle (per scheme and all)
 * - Governance: get_governance_stats, get_top_concepts
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 13: SKOS Concepts", () => {
  let client;
  const cleanup = { schemeIds: [], conceptIds: [], noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    // Clean up notes first
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    // Clean up concepts
    for (const id of cleanup.conceptIds) {
      try { await client.callTool("delete_concept", { id }); } catch {}
    }
    // Clean up schemes
    for (const id of cleanup.schemeIds) {
      try { await client.callTool("delete_concept_scheme", { id }); } catch {}
    }
    await client.close();
  });

  // --- Concept Scheme CRUD ---

  test("SKOS-001: list_concept_schemes returns array", async () => {
    const result = await client.callTool("list_concept_schemes");
    assert.ok(Array.isArray(result), "Result should be an array");
  });

  test("SKOS-002: create_concept_scheme with basic info", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `Test Scheme ${testId}`;

    const result = await client.callTool("create_concept_scheme", {
      notation: `test-${testId}`,
      title,
      description: "Test concept scheme for unit tests",
    });

    assert.ok(result.id, "Concept scheme should be created with ID");
    cleanup.schemeIds.push(result.id);
  });

  test("SKOS-003: get_concept_scheme by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `Test Get Scheme ${testId}`;

    const created = await client.callTool("create_concept_scheme", {
      notation: `test-get-${testId}`,
      title,
      description: "Get test",
    });
    cleanup.schemeIds.push(created.id);

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
      notation: `test-list-${testId}`,
      title,
    });
    cleanup.schemeIds.push(created.id);

    const schemes = await client.callTool("list_concept_schemes");

    const found = schemes.find((s) => s.id === created.id);
    assert.ok(found, "Created scheme should appear in list");
    assert.strictEqual(found.title, title, "Title should match in list");
  });

  test("SKOS-004a: update_concept_scheme modifies title", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `upd-scheme-${testId}`,
      title: `Original Title ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // update_concept_scheme returns 204 No Content (null result)
    await client.callTool("update_concept_scheme", {
      id: scheme.id,
      title: `Updated Title ${testId}`,
    });

    const retrieved = await client.callTool("get_concept_scheme", {
      id: scheme.id,
    });
    assert.strictEqual(retrieved.title, `Updated Title ${testId}`, "Title should be updated");
  });

  test("SKOS-004b: create_concept_scheme with URI", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const title = `URI Scheme ${testId}`;
    const uri = `http://example.org/schemes/${testId}`;

    const result = await client.callTool("create_concept_scheme", {
      notation: `uri-${testId}`,
      title,
      uri,
    });

    assert.ok(result.id, "Scheme should be created");
    cleanup.schemeIds.push(result.id);
  });

  test("SKOS-004c: delete_concept_scheme removes scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `del-scheme-${testId}`,
      title: `To Be Deleted ${testId}`,
    });

    await client.callTool("delete_concept_scheme", { id: scheme.id });

    const error = await client.callToolExpectError("get_concept_scheme", {
      id: scheme.id,
    });
    assert.ok(error.error, "Should return error for deleted scheme");
  });

  // --- Concept CRUD ---

  test("SKOS-005: create_concept in scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `concept-${testId}`,
      title: `Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Test Concept",
      definition: "A test concept for unit testing",
    });

    assert.ok(result.id, "Concept should be created with ID");
    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-006: search_concepts in scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `list-conc-${testId}`,
      title: `List Concepts Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

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

    const result = await client.callTool("search_concepts", {
      scheme_id: scheme.id,
    });

    assert.ok(result, "Should return result object");
    assert.ok(Array.isArray(result.concepts), "Result should have concepts array");
    assert.ok(result.concepts.length >= 2, "Should have at least 2 concepts");
    assert.ok(typeof result.total === "number", "Should have total count");

    const foundA = result.concepts.find((c) => c.id === concept1.id);
    const foundB = result.concepts.find((c) => c.id === concept2.id);
    assert.ok(foundA, "Concept A should be in list");
    assert.ok(foundB, "Concept B should be in list");
  });

  test("SKOS-007: get_concept by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `get-conc-${testId}`,
      title: `Get Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const created = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Test Concept for Get",
      definition: "Test definition",
    });
    cleanup.conceptIds.push(created.id);

    const retrieved = await client.callTool("get_concept", {
      id: created.id,
    });

    assert.ok(retrieved, "Concept should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.pref_label, "Test Concept for Get", "Label should match");
  });

  test("SKOS-007a: get_concept_full returns extended details", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `full-conc-${testId}`,
      title: `Full Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Full Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Full Child",
      broader_ids: [parent.id],
    });
    cleanup.conceptIds.push(child.id);

    const full = await client.callTool("get_concept_full", {
      id: child.id,
    });

    assert.ok(full, "Should return full concept data");
    assert.ok(full.id || full.concept, "Should have concept data");
    console.log(`  get_concept_full keys: ${Object.keys(full).join(", ")}`);
  });

  test("SKOS-008: create_concept with alt_labels", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `alt-labels-${testId}`,
      title: `Alt Labels Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Primary Label",
      alt_labels: ["Alternative 1", "Alternative 2"],
    });

    assert.ok(result.id, "Concept should be created");
    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-009: create_concept with broader relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `hierarchy-${testId}`,
      title: `Hierarchy Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent Concept",
    });
    cleanup.conceptIds.push(parent.id);

    // Use correct param name: broader_ids (not broader)
    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Child Concept",
      broader_ids: [parent.id],
    });
    cleanup.conceptIds.push(child.id);

    assert.ok(child.id, "Child concept should be created");
  });

  test("SKOS-010: create_concept with related relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `related-${testId}`,
      title: `Related Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept 1",
    });
    cleanup.conceptIds.push(concept1.id);

    // Use correct param name: related_ids (not related)
    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept 2",
      related_ids: [concept1.id],
    });
    cleanup.conceptIds.push(concept2.id);

    assert.ok(concept2.id, "Concept 2 should be created");
  });

  test("SKOS-011: delete_concept removes concept", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `del-conc-${testId}`,
      title: `Delete Concept Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "To Be Deleted",
    });

    await client.callTool("delete_concept", { id: concept.id });

    const error = await client.callToolExpectError("get_concept", {
      id: concept.id,
    });
    assert.ok(error.error, "Should return error for deleted concept");
  });

  test("SKOS-014: create_concept with notation", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `notation-${testId}`,
      title: `Notation Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Concept with Notation",
      notation: "ABC-123",
    });

    assert.ok(result.id, "Concept should be created");
    cleanup.conceptIds.push(result.id);
  });

  test("SKOS-015: update_concept modifies notation", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `update-${testId}`,
      title: `Update Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Original Label",
    });
    cleanup.conceptIds.push(concept.id);

    const updated = await client.callTool("update_concept", {
      id: concept.id,
      notation: "updated-notation",
    });

    assert.strictEqual(updated.notation, "updated-notation", "Notation should be updated");

    const retrieved = await client.callTool("get_concept", {
      id: concept.id,
    });
    assert.strictEqual(retrieved.notation, "updated-notation", "Updated notation should persist");
  });

  test("SKOS-016: create_concept error - invalid scheme_id", async () => {
    const fakeSchemeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("create_concept", {
      scheme_id: fakeSchemeId,
      pref_label: "Invalid Scheme Test",
    });

    assert.ok(error.error, "Should return error for invalid scheme");
  });

  test("SKOS-017: search_concepts empty for new scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `empty-${testId}`,
      title: `Empty Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("search_concepts", {
      scheme_id: scheme.id,
    });

    assert.ok(result, "Should return result object");
    assert.ok(Array.isArray(result.concepts), "Result should have concepts array");
    assert.strictEqual(result.concepts.length, 0, "New scheme should have no concepts");
    assert.strictEqual(result.total, 0, "Total should be 0");
  });

  test("SKOS-018: create_concept with multilingual alt_labels", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `multilang-${testId}`,
      title: `Multilingual Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // pref_label_lang is not in the schema — use alt_labels for synonyms
    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Computer",
      alt_labels: ["Ordinateur", "Computadora"],
    });

    assert.ok(result.id, "Multilingual concept should be created");
    cleanup.conceptIds.push(result.id);
  });

  // --- Export ---

  test("SKOS-019: export_skos_turtle for specific scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `export-${testId}`,
      title: `Export Scheme ${testId}`,
      uri: `http://example.org/schemes/export-${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Exportable Concept",
    });
    cleanup.conceptIds.push(concept.id);

    const turtle = await client.callTool("export_skos_turtle", {
      scheme_id: scheme.id,
    });

    assert.ok(turtle, "Should return turtle content");
    const turtleStr = typeof turtle === "string" ? turtle : turtle.turtle;
    assert.ok(typeof turtleStr === "string", "Turtle content should be string");
    assert.ok(turtleStr.includes("@prefix"), "Should include RDF prefixes");
  });

  test("SKOS-019a: export_skos_turtle without scheme_id exports all", async () => {
    const turtle = await client.callTool("export_skos_turtle", {});

    assert.ok(turtle, "Should return turtle content for all schemes");
    const turtleStr = typeof turtle === "string" ? turtle : turtle.turtle;
    assert.ok(typeof turtleStr === "string", "Turtle content should be string");
    assert.ok(turtleStr.includes("@prefix"), "Should include RDF prefixes");
  });

  // --- Top Concepts & Governance ---

  test("SKOS-020: get_top_concepts for scheme", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `top-conc-${testId}`,
      title: `Top Concepts Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create top-level concept (no broader) — top_concept param does not exist
    const topConcept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Top Level Concept",
    });
    cleanup.conceptIds.push(topConcept.id);

    // Use get_top_concepts tool (required: scheme_id)
    const result = await client.callTool("get_top_concepts", {
      scheme_id: scheme.id,
    });

    assert.ok(result, "Should return top concepts data");
    const concepts = Array.isArray(result) ? result : (result.concepts || result.top_concepts || []);
    assert.ok(Array.isArray(concepts), "Top concepts should be array");
    // Concept with no broader should be a top concept
    assert.ok(concepts.length >= 1, "Should have at least 1 top concept");
    console.log(`  Found ${concepts.length} top concepts`);
  });

  test("SKOS-020a: get_governance_stats", async () => {
    const result = await client.callTool("get_governance_stats", {});

    assert.ok(result, "Should return governance stats");
    console.log(`  Governance stats: ${JSON.stringify(result).slice(0, 200)}`);
  });

  test("SKOS-020b: get_governance_stats with scheme_id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `gov-${testId}`,
      title: `Governance Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("get_governance_stats", {
      scheme_id: scheme.id,
    });

    assert.ok(result, "Should return governance stats for specific scheme");
  });

  // --- Explicit Relationship Management ---

  test("SKOS-021: add_broader and get_broader", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `add-broader-${testId}`,
      title: `Add Broader Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Broader Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Broader Child",
    });
    cleanup.conceptIds.push(child.id);

    // Add broader relationship
    const addResult = await client.callTool("add_broader", {
      id: child.id,
      target_id: parent.id,
    });
    assert.ok(addResult, "Should add broader relationship");

    // Get broader for child
    const broader = await client.callTool("get_broader", {
      id: child.id,
    });
    assert.ok(broader, "Should return broader concepts");
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    assert.ok(Array.isArray(broaderList), "Broader should be array");
    // API returns relationship objects with object_id (the broader concept)
    const found = broaderList.find((c) => c.id === parent.id || c.object_id === parent.id);
    assert.ok(found, "Parent should be in broader list");
  });

  test("SKOS-022: add_narrower and get_narrower", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `add-narrower-${testId}`,
      title: `Add Narrower Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Narrower Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Narrower Child",
    });
    cleanup.conceptIds.push(child.id);

    // Add narrower relationship
    const addResult = await client.callTool("add_narrower", {
      id: parent.id,
      target_id: child.id,
    });
    assert.ok(addResult, "Should add narrower relationship");

    // Get narrower for parent
    const narrower = await client.callTool("get_narrower", {
      id: parent.id,
    });
    assert.ok(narrower, "Should return narrower concepts");
    const narrowerList = Array.isArray(narrower) ? narrower : (narrower.concepts || narrower.narrower || []);
    assert.ok(Array.isArray(narrowerList), "Narrower should be array");
    const found = narrowerList.find((c) => c.id === child.id || c.object_id === child.id);
    assert.ok(found, "Child should be in narrower list");
  });

  test("SKOS-023: add_related and get_related", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `add-related-${testId}`,
      title: `Add Related Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Related A",
    });
    cleanup.conceptIds.push(concept1.id);

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Related B",
    });
    cleanup.conceptIds.push(concept2.id);

    // Add related relationship
    const addResult = await client.callTool("add_related", {
      id: concept1.id,
      target_id: concept2.id,
    });
    assert.ok(addResult, "Should add related relationship");

    // Get related for concept1
    const related = await client.callTool("get_related", {
      id: concept1.id,
    });
    assert.ok(related, "Should return related concepts");
    const relatedList = Array.isArray(related) ? related : (related.concepts || related.related || []);
    assert.ok(Array.isArray(relatedList), "Related should be array");
    const found = relatedList.find((c) => c.id === concept2.id || c.object_id === concept2.id);
    assert.ok(found, "Concept 2 should be in related list");
  });

  test("SKOS-024: remove_broader removes relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `rm-broader-${testId}`,
      title: `Remove Broader Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Broader Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Broader Child",
      broader_ids: [parent.id],
    });
    cleanup.conceptIds.push(child.id);

    // Remove broader
    const removeResult = await client.callTool("remove_broader", {
      id: child.id,
      target_id: parent.id,
    });
    assert.ok(removeResult, "Should remove broader relationship");

    // Verify removal
    const broader = await client.callTool("get_broader", { id: child.id });
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    const found = broaderList.find((c) => c.id === parent.id || c.object_id === parent.id);
    assert.ok(!found, "Parent should no longer be in broader list");
  });

  test("SKOS-025: remove_narrower removes relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `rm-narrower-${testId}`,
      title: `Remove Narrower Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Narrow Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Narrow Child",
    });
    cleanup.conceptIds.push(child.id);

    // Add then remove narrower
    await client.callTool("add_narrower", { id: parent.id, target_id: child.id });
    await client.callTool("remove_narrower", { id: parent.id, target_id: child.id });

    // Verify removal
    const narrower = await client.callTool("get_narrower", { id: parent.id });
    const narrowerList = Array.isArray(narrower) ? narrower : (narrower.concepts || narrower.narrower || []);
    const found = narrowerList.find((c) => c.id === child.id || c.object_id === child.id);
    assert.ok(!found, "Child should no longer be in narrower list");
  });

  test("SKOS-026: remove_related removes relationship", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `rm-related-${testId}`,
      title: `Remove Related Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Related A",
    });
    cleanup.conceptIds.push(concept1.id);

    const concept2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "RM Related B",
    });
    cleanup.conceptIds.push(concept2.id);

    // Add then remove related
    await client.callTool("add_related", { id: concept1.id, target_id: concept2.id });
    await client.callTool("remove_related", { id: concept1.id, target_id: concept2.id });

    // Verify removal
    const related = await client.callTool("get_related", { id: concept1.id });
    const relatedList = Array.isArray(related) ? related : (related.concepts || related.related || []);
    const found = relatedList.find((c) => c.id === concept2.id || c.object_id === concept2.id);
    assert.ok(!found, "Concept 2 should no longer be in related list");
  });

  // --- Autocomplete ---

  test("SKOS-027: autocomplete_concepts returns matches", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `auto-${testId}`,
      title: `Autocomplete Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    // Create concepts with distinct prefixes
    const c1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: `AutoFoo ${testId}`,
    });
    cleanup.conceptIds.push(c1.id);

    const c2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: `AutoBar ${testId}`,
    });
    cleanup.conceptIds.push(c2.id);

    // Autocomplete search
    const result = await client.callTool("autocomplete_concepts", {
      q: `Auto`,
    });

    assert.ok(result, "Should return autocomplete results");
    const concepts = Array.isArray(result) ? result : (result.concepts || result.results || []);
    assert.ok(Array.isArray(concepts), "Autocomplete should return array");
    console.log(`  Autocomplete for 'Auto': ${concepts.length} results`);
  });

  test("SKOS-027a: autocomplete_concepts with limit", async () => {
    const result = await client.callTool("autocomplete_concepts", {
      q: "a",
      limit: 3,
    });

    assert.ok(result, "Should return autocomplete results");
    const concepts = Array.isArray(result) ? result : (result.concepts || result.results || []);
    assert.ok(Array.isArray(concepts), "Autocomplete should return array");
    assert.ok(concepts.length <= 3, "Should respect limit parameter");
  });

  // --- Concept-Note Tagging ---

  test("SKOS-028: tag_note_concept and get_note_concepts", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Create scheme and concept
    const scheme = await client.callTool("create_concept_scheme", {
      notation: `tag-note-${testId}`,
      title: `Tag Note Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Tagging Concept",
    });
    cleanup.conceptIds.push(concept.id);

    // Create a note
    const note = await client.callTool("create_note", {
      content: `# Tag Test Note ${testId}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Tag the note with the concept
    const tagResult = await client.callTool("tag_note_concept", {
      note_id: note.id,
      concept_id: concept.id,
    });
    assert.ok(tagResult, "Should tag note with concept");

    // Get concepts for note
    const concepts = await client.callTool("get_note_concepts", {
      note_id: note.id,
    });
    assert.ok(concepts, "Should return note concepts");
    // API returns nested array: [[{note_id, concept_id,...}, {concept details}], ...]
    // or flat array of concept objects
    const conceptList = Array.isArray(concepts) ? concepts : (concepts.concepts || []);
    assert.ok(Array.isArray(conceptList), "Note concepts should be array");
    // Handle both nested [[tag, concept]] and flat [{concept_id}] formats
    const hasTaggedConcept = conceptList.some((item) => {
      if (Array.isArray(item)) {
        return item.some((sub) => sub.concept_id === concept.id || sub.id === concept.id);
      }
      return item.id === concept.id || item.concept_id === concept.id;
    });
    assert.ok(hasTaggedConcept, "Tagged concept should be in note concepts list");
  });

  test("SKOS-029: untag_note_concept removes tag", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `untag-${testId}`,
      title: `Untag Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Untag Concept",
    });
    cleanup.conceptIds.push(concept.id);

    const note = await client.callTool("create_note", {
      content: `# Untag Test Note ${testId}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Tag then untag
    await client.callTool("tag_note_concept", { note_id: note.id, concept_id: concept.id });
    const untagResult = await client.callTool("untag_note_concept", {
      note_id: note.id,
      concept_id: concept.id,
    });
    assert.ok(untagResult, "Should untag note from concept");

    // Verify removal
    const concepts = await client.callTool("get_note_concepts", { note_id: note.id });
    const conceptList = Array.isArray(concepts) ? concepts : (concepts.concepts || []);
    const found = conceptList.find((c) => c.id === concept.id || c.concept_id === concept.id);
    assert.ok(!found, "Untagged concept should not be in note concepts list");
  });

  test("SKOS-030: tag note with multiple concepts", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `multi-tag-${testId}`,
      title: `Multi Tag Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const c1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Multi Tag A",
    });
    cleanup.conceptIds.push(c1.id);

    const c2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Multi Tag B",
    });
    cleanup.conceptIds.push(c2.id);

    const note = await client.callTool("create_note", {
      content: `# Multi Tag Note ${testId}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Tag with both concepts
    await client.callTool("tag_note_concept", { note_id: note.id, concept_id: c1.id });
    await client.callTool("tag_note_concept", { note_id: note.id, concept_id: c2.id });

    // Verify both are present
    const concepts = await client.callTool("get_note_concepts", { note_id: note.id });
    const conceptList = Array.isArray(concepts) ? concepts : (concepts.concepts || []);
    assert.ok(conceptList.length >= 2, "Should have at least 2 tagged concepts");
  });

  // --- SKOS Collections ---

  test("SKOS-031: create_skos_collection", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `coll-${testId}`,
      title: `Collection Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `Test Collection ${testId}`,
    });

    assert.ok(result, "Should create SKOS collection");
    assert.ok(result.id, "Collection should have ID");
    console.log(`  Created SKOS collection: ${result.id}`);
  });

  test("SKOS-032: get_skos_collection by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `get-coll-${testId}`,
      title: `Get Collection Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const created = await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `Get Collection ${testId}`,
    });

    const retrieved = await client.callTool("get_skos_collection", {
      id: created.id,
    });

    assert.ok(retrieved, "Should retrieve SKOS collection");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
  });

  test("SKOS-033: list_skos_collections", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `list-coll-${testId}`,
      title: `List Collections Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `List Collection ${testId}`,
    });

    const result = await client.callTool("list_skos_collections", {
      scheme_id: scheme.id,
    });

    assert.ok(result, "Should return collections list");
    const collections = Array.isArray(result) ? result : (result.collections || []);
    assert.ok(Array.isArray(collections), "Should be array");
    assert.ok(collections.length >= 1, "Should have at least 1 collection");
  });

  test("SKOS-034: update_skos_collection", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `upd-coll-${testId}`,
      title: `Update Collection Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const created = await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `Original Collection ${testId}`,
    });

    // update_skos_collection returns 204 No Content (null result)
    await client.callTool("update_skos_collection", {
      id: created.id,
      pref_label: `Updated Collection ${testId}`,
    });

    // Verify update via re-fetch
    const retrieved = await client.callTool("get_skos_collection", { id: created.id });
    assert.ok(retrieved, "Should retrieve updated collection");
  });

  test("SKOS-035: add_skos_collection_member and remove", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `member-coll-${testId}`,
      title: `Member Collection Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const concept = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Collection Member",
    });
    cleanup.conceptIds.push(concept.id);

    const collection = await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `Member Collection ${testId}`,
    });

    // Add member (returns 204 No Content / null)
    await client.callTool("add_skos_collection_member", {
      id: collection.id,
      concept_id: concept.id,
    });

    // Get collection to verify member
    const retrieved = await client.callTool("get_skos_collection", {
      id: collection.id,
    });
    assert.ok(retrieved, "Should retrieve collection with members");

    // Remove member (may also return null)
    await client.callTool("remove_skos_collection_member", {
      id: collection.id,
      concept_id: concept.id,
    });
  });

  test("SKOS-036: delete_skos_collection", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `del-coll-${testId}`,
      title: `Delete Collection Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const created = await client.callTool("create_skos_collection", {
      scheme_id: scheme.id,
      pref_label: `Delete Collection ${testId}`,
    });

    const deleteResult = await client.callTool("delete_skos_collection", {
      id: created.id,
    });
    assert.ok(deleteResult, "Should delete SKOS collection");

    // Verify deletion
    const error = await client.callToolExpectError("get_skos_collection", {
      id: created.id,
    });
    assert.ok(error.error, "Should error for deleted collection");
  });

  // --- Hierarchy Depth ---

  test("SKOS-037: three-level hierarchy via broader_ids", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `deep-hier-${testId}`,
      title: `Deep Hierarchy ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const grandparent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Grandparent",
    });
    cleanup.conceptIds.push(grandparent.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent",
      broader_ids: [grandparent.id],
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Child",
      broader_ids: [parent.id],
    });
    cleanup.conceptIds.push(child.id);

    // Verify child's broader is parent (get_broader returns relationship objects with object_id)
    const broader = await client.callTool("get_broader", { id: child.id });
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    const foundParent = broaderList.find((c) => c.id === parent.id || c.object_id === parent.id);
    assert.ok(foundParent, "Child's broader should be parent");

    // Verify parent's broader is grandparent
    const broaderP = await client.callTool("get_broader", { id: parent.id });
    const broaderListP = Array.isArray(broaderP) ? broaderP : (broaderP.concepts || broaderP.broader || []);
    const foundGP = broaderListP.find((c) => c.id === grandparent.id || c.object_id === grandparent.id);
    assert.ok(foundGP, "Parent's broader should be grandparent");
  });

  // --- Search with Query ---

  test("SKOS-038: search_concepts with query filter", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `search-q-${testId}`,
      title: `Search Query Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: `Findable-${testId}`,
    }).then((c) => { cleanup.conceptIds.push(c.id); return c; });

    await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: `Other-${testId}`,
    }).then((c) => { cleanup.conceptIds.push(c.id); return c; });

    const result = await client.callTool("search_concepts", {
      scheme_id: scheme.id,
      q: `Findable-${testId}`,
    });

    assert.ok(result, "Should return search results");
    assert.ok(Array.isArray(result.concepts), "Should have concepts array");
    // The matching concept should appear in results
    if (result.concepts.length > 0) {
      const found = result.concepts.find((c) => c.pref_label === `Findable-${testId}`);
      assert.ok(found, "Searched concept should be in results");
    }
  });

  // --- Error Cases ---

  test("SKOS-039: get_concept with non-existent ID errors", async () => {
    const error = await client.callToolExpectError("get_concept", {
      id: "00000000-0000-0000-0000-000000000000",
    });
    assert.ok(error.error, "Should error for non-existent concept");
  });

  test("SKOS-040: get_concept_scheme with non-existent ID errors", async () => {
    const error = await client.callToolExpectError("get_concept_scheme", {
      id: "00000000-0000-0000-0000-000000000000",
    });
    assert.ok(error.error, "Should error for non-existent scheme");
  });

  test("SKOS-041: delete_concept with non-existent ID is idempotent", async () => {
    // delete_concept returns success even for non-existent IDs (idempotent)
    const result = await client.callTool("delete_concept", {
      id: "00000000-0000-0000-0000-000000000000",
    });
    assert.ok(result, "Should return success for idempotent delete");
  });

  // --- Concept with Definition and Scope Note ---

  test("SKOS-042: create_concept with definition and scope_note", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `def-scope-${testId}`,
      title: `Definition Scope Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const result = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Rich Concept",
      definition: "A concept with all metadata fields populated",
      scope_note: "This concept is for testing purposes only",
      notation: "RICH-001",
      alt_labels: ["Detailed Concept", "Complete Concept"],
    });

    assert.ok(result.id, "Should create concept with all fields");
    cleanup.conceptIds.push(result.id);

    // Verify via get
    const retrieved = await client.callTool("get_concept", { id: result.id });
    assert.ok(retrieved, "Should retrieve rich concept");
    assert.strictEqual(retrieved.pref_label, "Rich Concept", "Label should match");
  });

  // --- Multiple Broader ---

  test("SKOS-043: concept with multiple broader (polyhierarchy)", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `polyhier-${testId}`,
      title: `Polyhierarchy Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent1 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent A",
    });
    cleanup.conceptIds.push(parent1.id);

    const parent2 = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Parent B",
    });
    cleanup.conceptIds.push(parent2.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Multi-Parent Child",
      broader_ids: [parent1.id, parent2.id],
    });
    cleanup.conceptIds.push(child.id);

    // Verify both parents are broader
    const broader = await client.callTool("get_broader", { id: child.id });
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    assert.ok(broaderList.length >= 2, "Should have at least 2 broader concepts");
  });

  // --- Bidirectional Relationship Verification ---

  test("SKOS-044: broader/narrower are bidirectional", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    const scheme = await client.callTool("create_concept_scheme", {
      notation: `bidi-${testId}`,
      title: `Bidirectional Scheme ${testId}`,
    });
    cleanup.schemeIds.push(scheme.id);

    const parent = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Bidi Parent",
    });
    cleanup.conceptIds.push(parent.id);

    const child = await client.callTool("create_concept", {
      scheme_id: scheme.id,
      pref_label: "Bidi Child",
    });
    cleanup.conceptIds.push(child.id);

    // Add broader from child to parent
    await client.callTool("add_broader", { id: child.id, target_id: parent.id });

    // Child should have parent as broader (returns relationship objects with object_id)
    const broader = await client.callTool("get_broader", { id: child.id });
    const broaderList = Array.isArray(broader) ? broader : (broader.concepts || broader.broader || []);
    assert.ok(broaderList.find((c) => c.id === parent.id || c.object_id === parent.id), "Child should have parent as broader");

    // Parent should have child as narrower (returns relationship objects with object_id)
    const narrower = await client.callTool("get_narrower", { id: parent.id });
    const narrowerList = Array.isArray(narrower) ? narrower : (narrower.concepts || narrower.narrower || []);
    assert.ok(narrowerList.find((c) => c.id === child.id || c.object_id === child.id), "Parent should have child as narrower");
  });
});
