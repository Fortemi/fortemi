#!/usr/bin/env node

/**
 * Consolidated Tools Tests
 *
 * Tests the 13 discriminated-union consolidated tools that form the
 * agent-friendly core surface (issue #365, #441):
 *   - capture_knowledge (create, bulk_create, from_template, upload)
 *   - search (text, spatial, temporal, spatial_temporal, federated)
 *   - record_provenance (location, named_location, device, file, note)
 *   - manage_tags (list, set, tag_concept, untag_concept, get_concepts)
 *   - manage_collection (list, create, get, update, delete, list_notes, move_note, export)
 *   - manage_concepts (search, autocomplete, get, get_full, stats, top, list_schemes, create_scheme, get_scheme, update_scheme, delete_scheme)
 *   - manage_attachments (list, upload, get, download, delete)
 *   - manage_embeddings (list, get, create, update, delete, list_members, add_members, remove_member, refresh)
 *   - manage_archives (list, create, get, update, delete, set_default, stats, clone)
 *   - manage_encryption (generate_keypair, get_address, encrypt, decrypt, list_recipients, verify_address, keyset ops)
 *   - manage_backups (export_shard, import_shard, snapshot, restore, list, get_info, get_metadata, update_metadata, download_archive, upload_archive, swap, download_memory)
 *   - manage_jobs (list, get, create, stats, pending_count, extraction_stats)
 *   - manage_inference (list_models, get_embedding_config, list_embedding_configs)
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Consolidated Tools", () => {
  let client;
  const cleanup = { noteIds: [], collectionIds: [], archiveNames: [], keysetNames: [], schemeIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    // Restore default archive to public before cleanup
    try { await client.callTool("manage_archives", { action: "set_default", name: "public" }); } catch {}
    try { await client.callTool("select_memory", { name: "public" }); } catch {}

    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    for (const id of cleanup.collectionIds) {
      try { await client.callTool("delete_collection", { id }); } catch {}
    }
    for (const name of cleanup.archiveNames.reverse()) {
      try { await client.callTool("manage_archives", { action: "delete", name }); } catch {}
    }
    for (const name of cleanup.keysetNames) {
      try { await client.callTool("manage_encryption", { action: "delete_keyset", name }); } catch {}
    }
    for (const id of cleanup.schemeIds) {
      try { await client.callTool("manage_concepts", { action: "delete_scheme", scheme_id: id, force: true }); } catch {}
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
    // Upload requires a note_id — create a note first
    const tag = MCPTestClient.testTag("ck", "upload");
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Upload test note",
      tags: [tag],
    });
    cleanup.noteIds.push(note.id);

    const result = await client.callTool("capture_knowledge", {
      action: "upload",
      note_id: note.id,
      filename: "/tmp/test-file.txt",
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
      lat: 40.7128,
      lon: -74.006,
      radius: 10000,
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

  test("SRCH-005: search spatial_temporal action combines both", async () => {
    const result = await client.callTool("search", {
      action: "spatial_temporal",
      lat: 40.7128,
      lon: -74.006,
      radius: 50000,
      start: "2020-01-01T00:00:00Z",
      end: "2030-12-31T23:59:59Z",
      limit: 5,
    });
    assert.ok(result !== undefined, "Should return spatial-temporal results");
  });

  test("SRCH-006: search rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("search", { action: "bogus" }),
      (err) => {
        assert.ok(err.message.includes("Unknown search action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  test("SRCH-012: search text action without query returns error", async () => {
    await assert.rejects(
      () => client.callTool("search", { action: "text" }),
      (err) => {
        assert.ok(err.message.includes("query") && err.message.includes("required"),
          `Should mention query is required, got: ${err.message}`);
        return true;
      }
    );
  });

  test("SRCH-013: search federated action without query returns error", async () => {
    await assert.rejects(
      () => client.callTool("search", { action: "federated", memories: ["all"] }),
      (err) => {
        assert.ok(err.message.includes("query") && err.message.includes("required"),
          `Should mention query is required, got: ${err.message}`);
        return true;
      }
    );
  });

  test("SRCH-014: search spatial action without lat/lon returns error", async () => {
    await assert.rejects(
      () => client.callTool("search", { action: "spatial" }),
      (err) => {
        assert.ok(err.message.includes("lat") && err.message.includes("required"),
          `Should mention lat is required, got: ${err.message}`);
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

  test("MT-003: manage_tags get_concepts action returns note concepts", async () => {
    // Use a note we already created
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Concept tag test note",
      tags: [MCPTestClient.testTag("mt", "concepts")],
    });
    cleanup.noteIds.push(note.id);

    const result = await client.callTool("manage_tags", {
      action: "get_concepts",
      note_id: note.id,
    });
    // May be empty array but should not error
    assert.ok(Array.isArray(result) || result !== undefined, "Should return concepts array or response");
  });

  test("MT-004: manage_tags rejects invalid action", async () => {
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
    await client.callTool("manage_collection", {
      action: "update",
      id: created.id,
      name: name + "-updated",
    });
    // Verify update took effect
    const afterUpdate = await client.callTool("manage_collection", {
      action: "get",
      id: created.id,
    });
    assert.equal(afterUpdate.name, name + "-updated", "Name should be updated");

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

  test("MCO-003: manage_concepts autocomplete action works", async () => {
    const result = await client.callTool("manage_concepts", {
      action: "autocomplete",
      q: "test",
      limit: 5,
    });
    // May return empty but should not error
    assert.ok(result !== undefined, "Should return autocomplete results");
  });

  test("MCO-004: manage_concepts rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_concepts", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_concepts action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  test("MCO-005: manage_concepts list_schemes returns schemes", async () => {
    const result = await client.callTool("manage_concepts", { action: "list_schemes" });
    assert.ok(Array.isArray(result), "Should return array of schemes");
    // Default scheme should always exist
    const defaultScheme = result.find(s => s.notation === "default");
    assert.ok(defaultScheme, "Default scheme should exist");
    assert.strictEqual(defaultScheme.is_system, true, "Default scheme should be system");
  });

  test("MCO-006: manage_concepts create_scheme + get_scheme + update_scheme + delete_scheme lifecycle", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const notation = `test-${testId}`;

    // Create
    const created = await client.callTool("manage_concepts", {
      action: "create_scheme",
      notation,
      title: `Test Scheme ${testId}`,
      description: "Scheme created by consolidated tools test",
    });
    assert.ok(created.id, "Should return created scheme ID");
    cleanup.schemeIds.push(created.id);

    // Get
    const fetched = await client.callTool("manage_concepts", {
      action: "get_scheme",
      scheme_id: created.id,
    });
    assert.strictEqual(fetched.notation, notation, "Notation should match");
    assert.strictEqual(fetched.title, `Test Scheme ${testId}`, "Title should match");

    // Update
    const updated = await client.callTool("manage_concepts", {
      action: "update_scheme",
      scheme_id: created.id,
      title: `Updated Scheme ${testId}`,
      description: "Updated description",
    });
    assert.ok(updated.success || updated.title, "Update should succeed");

    // Verify update
    const refetched = await client.callTool("manage_concepts", {
      action: "get_scheme",
      scheme_id: created.id,
    });
    assert.strictEqual(refetched.title, `Updated Scheme ${testId}`, "Title should be updated");

    // Delete
    const deleted = await client.callTool("manage_concepts", {
      action: "delete_scheme",
      scheme_id: created.id,
      force: true,
    });
    assert.ok(deleted.success, "Delete should succeed");
    cleanup.schemeIds = cleanup.schemeIds.filter(id => id !== created.id);
  });

  // === record_provenance ===

  test("RP-001: record_provenance location action creates location", async () => {
    const result = await client.callTool("record_provenance", {
      action: "location",
      latitude: 40.7128,
      longitude: -74.006,
      source: "user_manual",
      confidence: "medium",
    });
    assert.ok(result.id || result.location_id, "Should return location ID");
  });

  test("RP-002: record_provenance named_location action creates named location", async () => {
    const result = await client.callTool("record_provenance", {
      action: "named_location",
      name: `test-place-${Date.now()}`,
      location_type: "poi",
      latitude: 51.5074,
      longitude: -0.1278,
    });
    assert.ok(result.id, "Should return named location ID");
  });

  test("RP-003: record_provenance device action creates device", async () => {
    const result = await client.callTool("record_provenance", {
      action: "device",
      device_make: "TestCo",
      device_model: `TestModel-${MCPTestClient.uniqueId().slice(0, 8)}`,
    });
    assert.ok(result.id, "Should return device ID");
  });

  test("RP-004: record_provenance note action creates note provenance", async () => {
    // Need a note and location first
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Provenance test note",
      tags: [MCPTestClient.testTag("rp", "note")],
    });
    cleanup.noteIds.push(note.id);

    const loc = await client.callTool("record_provenance", {
      action: "location",
      latitude: 35.6762,
      longitude: 139.6503,
      source: "user_manual",
      confidence: "high",
    });

    const result = await client.callTool("record_provenance", {
      action: "note",
      note_id: note.id,
      location_id: loc.id || loc.location_id,
      capture_time_start: "2026-01-01T00:00:00Z",
      time_source: "user_manual",
      time_confidence: "high",
    });
    assert.ok(result.id, "Should return note provenance ID");
  });

  test("RP-005: record_provenance rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("record_provenance", { action: "invalid" }),
      (err) => {
        assert.ok(err.message.includes("Unknown record_provenance action") || err.code, "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_archives ===

  test("MA-001: manage_archives list action returns archives", async () => {
    const result = await client.callTool("manage_archives", { action: "list" });
    assert.ok(Array.isArray(result), "Should return array of archives");
    assert.ok(result.length >= 1, "Should have at least the default archive");
    const defaultArchive = result.find(a => a.is_default);
    assert.ok(defaultArchive, "Should have a default archive");
  });

  test("MA-002: manage_archives create/get/update/stats/delete lifecycle", async () => {
    const name = `ct-arch-${MCPTestClient.uniqueId().slice(0, 8)}`;

    // Create
    const created = await client.callTool("manage_archives", {
      action: "create",
      name,
      description: "Consolidated tool test archive",
    });
    assert.ok(created.id || created.name, "Should return archive id or name");
    cleanup.archiveNames.push(name);

    // Get
    const fetched = await client.callTool("manage_archives", { action: "get", name });
    assert.strictEqual(fetched.name, name, "Should return correct name");
    assert.strictEqual(fetched.description, "Consolidated tool test archive", "Should have description");

    // Update
    const updateResult = await client.callTool("manage_archives", {
      action: "update",
      name,
      description: "Updated description",
    });
    assert.ok(updateResult.success, "Should return success");

    // Verify update
    const afterUpdate = await client.callTool("manage_archives", { action: "get", name });
    assert.strictEqual(afterUpdate.description, "Updated description", "Description should be updated");

    // Stats
    const stats = await client.callTool("manage_archives", { action: "stats", name });
    assert.ok(stats.note_count !== undefined, "Should have note_count");
    assert.ok(stats.size_bytes !== undefined, "Should have size_bytes");

    // Delete
    const deleted = await client.callTool("manage_archives", { action: "delete", name });
    assert.ok(deleted.success, "Should confirm deletion");
    // Remove from cleanup since we already deleted
    cleanup.archiveNames = cleanup.archiveNames.filter(n => n !== name);
  });

  test("MA-003: manage_archives clone action deep-copies archive", async () => {
    const sourceName = `ct-src-${MCPTestClient.uniqueId().slice(0, 8)}`;
    const cloneName = `ct-cln-${MCPTestClient.uniqueId().slice(0, 8)}`;

    // Create source
    await client.callTool("manage_archives", {
      action: "create",
      name: sourceName,
      description: "Source for clone",
    });
    cleanup.archiveNames.push(sourceName);

    // Clone
    const cloned = await client.callTool("manage_archives", {
      action: "clone",
      name: sourceName,
      new_name: cloneName,
      description: "Cloned archive",
    });
    assert.ok(cloned.id || cloned.name, "Should return cloned archive info");
    cleanup.archiveNames.push(cloneName);

    // Verify clone exists
    const fetched = await client.callTool("manage_archives", { action: "get", name: cloneName });
    assert.strictEqual(fetched.name, cloneName, "Clone should exist with correct name");
  });

  test("MA-004: manage_archives set_default action changes default", async () => {
    const name = `ct-def-${MCPTestClient.uniqueId().slice(0, 8)}`;
    await client.callTool("manage_archives", { action: "create", name });
    cleanup.archiveNames.push(name);

    // Set as default
    const result = await client.callTool("manage_archives", { action: "set_default", name });
    assert.ok(result.success, "Should confirm default set");

    // Verify
    const archives = await client.callTool("manage_archives", { action: "list" });
    const newDefault = archives.find(a => a.is_default);
    assert.strictEqual(newDefault.name, name, "New default should be our archive");

    // Restore
    await client.callTool("manage_archives", { action: "set_default", name: "public" });
  });

  test("MA-005: manage_archives rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_archives", { action: "fly" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_archives action"), "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_encryption ===

  test("ME-001: manage_encryption generate_keypair creates keypair", async () => {
    const result = await client.callTool("manage_encryption", {
      action: "generate_keypair",
      passphrase: "test-passphrase-12chars",
    });
    assert.ok(result.address, "Should return address");
    assert.ok(result.public_key, "Should return public key");
    assert.ok(result.encrypted_private_key, "Should return encrypted private key");
    assert.ok(result.address.startsWith("mm:"), "Address should start with mm:");
  });

  test("ME-002: manage_encryption get_address derives address from key", async () => {
    // Generate a keypair first
    const keypair = await client.callTool("manage_encryption", {
      action: "generate_keypair",
      passphrase: "test-passphrase-12chars",
    });

    const result = await client.callTool("manage_encryption", {
      action: "get_address",
      public_key: keypair.public_key,
    });
    assert.ok(result.address, "Should return derived address");
    assert.strictEqual(result.address, keypair.address, "Derived address should match original");
  });

  test("ME-003: manage_encryption encrypt/decrypt roundtrip", async () => {
    const keypair = await client.callTool("manage_encryption", {
      action: "generate_keypair",
      passphrase: "roundtrip-pass-12ch",
    });

    // Encrypt
    const plaintext = Buffer.from("Hello from consolidated test!").toString("base64");
    const encrypted = await client.callTool("manage_encryption", {
      action: "encrypt",
      plaintext,
      recipient_keys: [keypair.public_key],
    });
    assert.ok(encrypted.ciphertext, "Should return ciphertext");

    // List recipients
    const recipients = await client.callTool("manage_encryption", {
      action: "list_recipients",
      ciphertext: encrypted.ciphertext,
    });
    assert.ok(Array.isArray(recipients.recipients), "Should return recipients array");
    assert.ok(recipients.recipients.includes(keypair.address), "Should include our address");

    // Decrypt
    const decrypted = await client.callTool("manage_encryption", {
      action: "decrypt",
      ciphertext: encrypted.ciphertext,
      encrypted_private_key: keypair.encrypted_private_key,
      passphrase: "roundtrip-pass-12ch",
    });
    assert.ok(decrypted.plaintext, "Should return plaintext");
    const decoded = Buffer.from(decrypted.plaintext, "base64").toString("utf-8");
    assert.strictEqual(decoded, "Hello from consolidated test!", "Decrypted content should match");
  });

  test("ME-004: manage_encryption verify_address validates format", async () => {
    const keypair = await client.callTool("manage_encryption", {
      action: "generate_keypair",
      passphrase: "verify-test-12chars",
    });

    const result = await client.callTool("manage_encryption", {
      action: "verify_address",
      address: keypair.address,
    });
    assert.ok(result, "Should return verification result");
  });

  test("ME-005: manage_encryption keyset lifecycle", async () => {
    const keysetName = `ct-ks-${MCPTestClient.uniqueId().slice(0, 8)}`;
    cleanup.keysetNames.push(keysetName);

    // Create keyset
    const created = await client.callTool("manage_encryption", {
      action: "create_keyset",
      name: keysetName,
      passphrase: "keyset-test-12chars",
    });
    assert.ok(created.address, "Should return keyset address");
    assert.strictEqual(created.name, keysetName, "Should return keyset name");

    // List keysets
    const keysets = await client.callTool("manage_encryption", { action: "list_keysets" });
    assert.ok(Array.isArray(keysets), "Should return array");
    const found = keysets.find(k => k.name === keysetName);
    assert.ok(found, "Created keyset should appear in list");

    // Set active
    const activated = await client.callTool("manage_encryption", {
      action: "set_active_keyset",
      name: keysetName,
    });
    assert.ok(activated.success, "Should confirm activation");

    // Get active
    const active = await client.callTool("manage_encryption", { action: "get_active_keyset" });
    assert.strictEqual(active.name, keysetName, "Active keyset should match");

    // Delete keyset
    const deleted = await client.callTool("manage_encryption", {
      action: "delete_keyset",
      name: keysetName,
    });
    assert.ok(deleted.success, "Should confirm deletion");
    cleanup.keysetNames = cleanup.keysetNames.filter(n => n !== keysetName);
  });

  test("ME-006: manage_encryption rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_encryption", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_encryption action"), "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_backups ===

  test("MB-001: manage_backups list action returns backups", async () => {
    const result = await client.callTool("manage_backups", { action: "list" });
    // May return empty array or object with files
    assert.ok(result !== undefined, "Should return backup list");
  });

  test("MB-002: manage_backups snapshot action creates backup", async () => {
    const result = await client.callTool("manage_backups", {
      action: "snapshot",
      name: `ct-snap-${MCPTestClient.uniqueId().slice(0, 8)}`,
      title: "Consolidated tool test backup",
      description: "Test snapshot",
    });
    assert.ok(result, "Should return snapshot result");
  });

  test("MB-003: manage_backups export_shard returns curl command", async () => {
    const result = await client.callTool("manage_backups", {
      action: "export_shard",
      include: "notes,tags",
    });
    assert.ok(result.download_url, "Should return download URL");
    assert.ok(result.curl_command, "Should return curl command");
    assert.ok(result.suggested_filename, "Should return suggested filename");
  });

  test("MB-004: manage_backups import_shard returns curl command", async () => {
    const result = await client.callTool("manage_backups", {
      action: "import_shard",
      file_path: "/tmp/test-shard.tar.gz",
      dry_run: true,
    });
    assert.ok(result.upload_url, "Should return upload URL");
    assert.ok(result.curl_command, "Should return curl command");
  });

  test("MB-005: manage_backups download_memory returns curl command", async () => {
    const result = await client.callTool("manage_backups", {
      action: "download_memory",
      name: "public",
    });
    assert.ok(result.download_url, "Should return download URL");
    assert.ok(result.curl_command, "Should return curl command");
    assert.ok(result.suggested_filename, "Should return suggested filename");
  });

  test("MB-006: manage_backups swap with nonexistent file errors", async () => {
    await assert.rejects(
      () => client.callTool("manage_backups", {
        action: "swap",
        filename: "nonexistent-backup.tar.gz",
      }),
      (err) => {
        assert.ok(err.message, "Should return error for nonexistent file");
        return true;
      }
    );
  });

  test("MB-007: manage_backups rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_backups", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_backups action"), "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_jobs ===

  test("MJ-001: manage_jobs stats action returns queue statistics", async () => {
    const result = await client.callTool("manage_jobs", { action: "stats" });
    assert.ok(result !== undefined, "Should return queue stats");
  });

  test("MJ-002: manage_jobs list action returns jobs", async () => {
    const result = await client.callTool("manage_jobs", { action: "list", limit: 5 });
    assert.ok(Array.isArray(result), "Should return array of jobs");
  });

  test("MJ-003: manage_jobs list with status filter", async () => {
    const result = await client.callTool("manage_jobs", {
      action: "list",
      status: "completed",
      limit: 3,
    });
    assert.ok(Array.isArray(result), "Should return filtered jobs");
  });

  test("MJ-004: manage_jobs pending_count action returns count", async () => {
    const result = await client.callTool("manage_jobs", { action: "pending_count" });
    assert.ok(result !== undefined, "Should return pending count");
  });

  test("MJ-005: manage_jobs extraction_stats action returns stats", async () => {
    const result = await client.callTool("manage_jobs", { action: "extraction_stats" });
    assert.ok(result !== undefined, "Should return extraction stats");
  });

  test("MJ-006: manage_jobs create action queues a job", async () => {
    // Create a test note first
    const note = await client.callTool("capture_knowledge", {
      action: "create",
      content: "Test note for manage_jobs UAT",
      revision_mode: "none",
    });
    const noteId = note.id;
    cleanup.noteIds.push(noteId);

    const result = await client.callTool("manage_jobs", {
      action: "create",
      note_id: noteId,
      job_type: "embedding",
      deduplicate: true,
    });
    assert.ok(result, "Should return job creation result");
  });

  test("MJ-007: manage_jobs get action retrieves job details", async () => {
    // Get a recent job from list
    const jobs = await client.callTool("manage_jobs", { action: "list", limit: 1 });
    if (Array.isArray(jobs) && jobs.length > 0) {
      const job = await client.callTool("manage_jobs", { action: "get", id: jobs[0].id });
      assert.ok(job, "Should return job details");
      assert.ok(job.id, "Job should have id");
    }
  });

  test("MJ-008: manage_jobs rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_jobs", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_jobs action"), "Should mention unknown action");
        return true;
      }
    );
  });

  // === manage_inference ===

  test("MI-001: manage_inference list_models action returns models", async () => {
    const result = await client.callTool("manage_inference", { action: "list_models" });
    assert.ok(result !== undefined, "Should return model information");
  });

  test("MI-002: manage_inference get_embedding_config action returns config", async () => {
    const result = await client.callTool("manage_inference", { action: "get_embedding_config" });
    assert.ok(result !== undefined, "Should return embedding config");
  });

  test("MI-003: manage_inference list_embedding_configs action returns configs", async () => {
    const result = await client.callTool("manage_inference", { action: "list_embedding_configs" });
    assert.ok(result !== undefined, "Should return embedding configs list");
  });

  test("MI-004: manage_inference rejects invalid action", async () => {
    await assert.rejects(
      () => client.callTool("manage_inference", { action: "nope" }),
      (err) => {
        assert.ok(err.message.includes("Unknown manage_inference action"), "Should mention unknown action");
        return true;
      }
    );
  });

  // === Tool schema validation ===

  test("SCHEMA-001: All 13 consolidated tools have action enum in schema", async () => {
    const tools = await client.listTools();
    const consolidated = [
      "capture_knowledge", "search", "record_provenance",
      "manage_tags", "manage_collection", "manage_concepts",
      "manage_attachments", "manage_embeddings",
      "manage_archives", "manage_encryption", "manage_backups",
      "manage_jobs", "manage_inference",
    ];

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
