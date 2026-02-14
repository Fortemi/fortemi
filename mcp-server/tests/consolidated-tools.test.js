#!/usr/bin/env node

/**
 * Consolidated Tools Tests
 *
 * Tests the 6 discriminated-union consolidated tools that form the
 * agent-friendly core surface (issue #365):
 *   - capture_knowledge (create, bulk_create, from_template, upload)
 *   - search (text, spatial, temporal, spatial_temporal, federated)
 *   - record_provenance (location, named_location, device, file, note)
 *   - manage_tags (list, set, tag_concept, untag_concept, get_concepts)
 *   - manage_collection (list, create, get, update, delete, list_notes, move_note, export)
 *   - manage_concepts (search, autocomplete, get, get_full, stats, top)
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Consolidated Tools", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    for (const id of cleanup.collectionIds) {
      try { await client.callTool("delete_collection", { id }); } catch {}
    }
    await client.close();
  });

  // === capture_knowledge ===

  test("CK-001: capture_knowledge create action creates a note", async () => {
    const tag = MCPTestClient.testTag("ck", "create");
    const result = await client.callTool("capture_knowledge", {
      action: "create",
      content: `# Consolidated test note\n\nCreated via capture_knowledge.`,
      tags: [tag],
    });
    assert.ok(result.id, "Should return note ID");
    cleanup.noteIds.push(result.id);
  });

  test("CK-002: capture_knowledge bulk_create action creates multiple notes", async () => {
    const tag = MCPTestClient.testTag("ck", "bulk");
    const result = await client.callTool("capture_knowledge", {
      action: "bulk_create",
      notes: [
        { content: "Bulk note 1", tags: [tag] },
        { content: "Bulk note 2", tags: [tag] },
      ],
    });
    assert.ok(Array.isArray(result), "Should return array of results");
    assert.equal(result.length, 2, "Should create 2 notes");
    for (const r of result) cleanup.noteIds.push(r.id);
  });

  test("CK-003: capture_knowledge upload action returns curl command", async () => {
    const result = await client.callTool("capture_knowledge", {
      action: "upload",
      file_path: "/tmp/test-file.txt",
    });
    // Upload returns instructions with a curl command
    assert.ok(
      typeof result === "string" || (result && result.curl_command) || (result && result.upload_url),
      "Should return upload instructions"
    );
  });

  test("CK-004: capture_knowledge rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("capture_knowledge", { action: "invalid_action" }),
      (err) => {
        assert.ok(err.message.includes("Unknown capture_knowledge action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === search ===

  test("SRCH-001: search text action returns results", async () => {
    const result = await client.callTool("search", {
      action: "text",
      query: "test",
      limit: 5,
    });
    assert.ok(Array.isArray(result) || result.results !== undefined, "Should return search results");
  });

  test("SRCH-002: search federated action works", async () => {
    const result = await client.callTool("search", {
      action: "federated",
      query: "test",
      memories: ["public"],
      limit: 3,
    });
    assert.ok(result !== undefined, "Should return federated results");
  });

  test("SRCH-003: search spatial action accepts coordinates", async () => {
    const result = await client.callTool("search", {
      action: "spatial",
      latitude: 40.7128,
      longitude: -74.006,
      radius_km: 10,
      limit: 5,
    });
    // May return empty results but should not error
    assert.ok(result !== undefined, "Should return spatial results");
  });

  test("SRCH-004: search temporal action accepts date range", async () => {
    const result = await client.callTool("search", {
      action: "temporal",
      start: "2020-01-01T00:00:00Z",
      end: "2030-12-31T23:59:59Z",
      limit: 5,
    });
    assert.ok(result !== undefined, "Should return temporal results");
  });

  test("SRCH-005: search rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("search", { action: "bogus" }),
      (err) => {
        assert.ok(err.message.includes("Unknown search action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_tags ===

  test("MT-001: manage_tags list action returns tags", async () => {
    const result = await client.callTool("manage_tags", { action: "list" });
    assert.ok(Array.isArray(result), "Should return array of tags");
  });

  test("MT-002: manage_tags set action replaces note tags", async () => {
    // Create a note first
    const tag = MCPTestClient.testTag("mt", "set");
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Tag test note",
      tags: [tag],
    });
    cleanup.noteIds.push(note.id);

    const newTag = MCPTestClient.testTag("mt", "updated");
    const result = await client.callTool("manage_tags", {
      action: "set",
      note_id: note.id,
      tags: [newTag],
    });
    assert.ok(result.success, "Should return success");
  });

  test("MT-003: manage_tags rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_tags", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_tags action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_collection ===

  test("MC-001: manage_collection list action returns collections", async () => {
    const result = await client.callTool("manage_collection", { action: "list" });
    assert.ok(Array.isArray(result), "Should return array of collections");
  });

  test("MC-002: manage_collection create/get/update/delete lifecycle", async () => {
    // Create
    const name = `test-collection-${Date.now()}`;
    const created = await client.callTool("manage_collection", {
      action: "create",
      name,
      description: "Test collection",
    });
    assert.ok(created.id, "Should return collection ID");

    // Get
    const fetched = await client.callTool("manage_collection", {
      action: "get",
      id: created.id,
    });
    assert.equal(fetched.name, name, "Should return correct name");

    // Update
    const updated = await client.callTool("manage_collection", {
      action: "update",
      id: created.id,
      name: name + "-updated",
    });
    assert.ok(updated, "Should return updated collection");

    // Delete
    const deleted = await client.callTool("manage_collection", {
      action: "delete",
      id: created.id,
    });
    assert.ok(deleted.success || deleted.message, "Should confirm deletion");
  });

  test("MC-003: manage_collection move_note action moves a note", async () => {
    // Create collection and note
    const col = await client.callTool("manage_collection", {
      action: "create",
      name: `move-test-${Date.now()}`,
    });
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Move test note",
    });
    cleanup.noteIds.push(note.id);
    cleanup.collectionIds.push(col.id);

    const result = await client.callTool("manage_collection", {
      action: "move_note",
      note_id: note.id,
      collection_id: col.id,
    });
    assert.ok(result.success || result.note_id, "Should confirm move");
  });

  test("MC-004: manage_collection rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_collection", { action: "fly" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_collection action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_concepts ===

  test("MCO-001: manage_concepts search action returns results", async () => {
    const result = await client.callTool("manage_concepts", { action: "search" });
    // May return empty array but should succeed
    assert.ok(result !== undefined, "Should return concept results");
  });

  test("MCO-002: manage_concepts stats action returns governance stats", async () => {
    const result = await client.callTool("manage_concepts", { action: "stats" });
    assert.ok(result !== undefined, "Should return governance stats");
  });

  test("MCO-003: manage_concepts rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_concepts", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_concepts action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === record_provenance ===

  test("RP-001: record_provenance location action creates location", async () => {
    const result = await client.callTool("record_provenance", {
      action: "location",
      latitude: 40.7128,
      longitude: -74.006,
      source: "test",
    });
    assert.ok(result.id || result.location_id, "Should return location ID");
  });

  test("RP-002: record_provenance named_location action creates named location", async () => {
    const result = await client.callTool("record_provenance", {
      action: "named_location",
      name: `test-place-${Date.now()}`,
      location_type: "office",
      latitude: 51.5074,
      longitude: -0.1278,
    });
    assert.ok(result.id, "Should return named location ID");
  });

  test("RP-003: record_provenance rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("record_provenance", { action: "invalid" }),
      (err) => {
        assert.ok(err.message.includes("Unknown record_provenance action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === Tool schema validation ===

  test("SCHEMA-001: All 6 consolidated tools have action enum in schema", async () => {
    const tools = await client.listTools();
    const consolidated = ["capture_knowledge", "search", "record_provenance",
      "manage_tags", "manage_collection", "manage_concepts"];

    for (const name of consolidated) {
      const tool = tools.find(t => t.name === name);
      assert.ok(tool, `Consolidated tool ${name} should exist`);
      assert.ok(
        tool.inputSchema.properties?.action,
        `${name} should have action property in schema`
      );
      assert.ok(
        tool.inputSchema.properties.action.enum,
        `${name} action should have enum values`
      );
    }
  });
});
