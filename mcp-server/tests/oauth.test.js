import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 17: OAuth Authentication", () => {
  let client;
  const cleanup = { noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  // --- Session & System Info ---

  test("OAUTH-001: System info accessible with auth", async () => {
    const result = await client.callTool("get_system_info", {});
    assert.ok(result, "Should return system info");
    assert.ok(
      result.versions || result.version || result.server || result.api_version,
      "Should have version or server info"
    );
    console.log(`  Server status: ${result.status}, version: ${result.versions?.release || "N/A"}`);
  });

  // --- Authenticated CRUD Operations ---

  test("OAUTH-002: Authenticated write - create_note", async () => {
    const result = await client.callTool("create_note", {
      content: `# OAuth Write Test ${MCPTestClient.uniqueId()}\n\nCreated via authenticated MCP session.`,
      tags: [MCPTestClient.testTag("oauth-write")],
      revision_mode: "none",
    });
    assert.ok(result.id, "Should create note via authenticated session");
    cleanup.noteIds.push(result.id);
    console.log(`  Created note: ${result.id}`);
  });

  test("OAUTH-003: Authenticated read - get_note", async () => {
    const note = await client.callTool("create_note", {
      content: `# OAuth Read Test ${MCPTestClient.uniqueId()}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    const result = await client.callTool("get_note", { id: note.id });
    assert.ok(result, "Should retrieve note via authenticated session");
    assert.ok(result.note || result.id, "Should have note data");
  });

  test("OAUTH-004: Authenticated update - update_note", async () => {
    const note = await client.callTool("create_note", {
      content: `# OAuth Update Test ${MCPTestClient.uniqueId()}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    const result = await client.callTool("update_note", {
      id: note.id,
      content: `# OAuth Updated ${MCPTestClient.uniqueId()}`,
    });
    assert.ok(result.success === true || result.id, "Should update note via authenticated session");
  });

  test("OAUTH-005: Authenticated delete - delete_note", async () => {
    const note = await client.callTool("create_note", {
      content: `# OAuth Delete Test ${MCPTestClient.uniqueId()}`,
      revision_mode: "none",
    });

    const result = await client.callTool("delete_note", { id: note.id });
    assert.ok(result, "Should delete note via authenticated session");

    // Verify deletion
    const error = await client.callToolExpectError("get_note", { id: note.id });
    assert.ok(error.error, "Deleted note should not be retrievable");
  });

  test("OAUTH-006: Authenticated purge - purge_note", async () => {
    const note = await client.callTool("create_note", {
      content: `# OAuth Purge Test ${MCPTestClient.uniqueId()}`,
      revision_mode: "none",
    });

    // Soft delete first
    await client.callTool("delete_note", { id: note.id });

    // Purge permanently
    const result = await client.callTool("purge_note", { id: note.id });
    assert.ok(result, "Should purge note via authenticated session");
  });

  // --- Search Operations ---

  test("OAUTH-007: Authenticated search - search_notes", async () => {
    const uniquePhrase = `oauth-search-${MCPTestClient.uniqueId()}`;
    const note = await client.callTool("create_note", {
      content: `# OAuth Search ${uniquePhrase}`,
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await new Promise((r) => setTimeout(r, 200));

    const result = await client.callTool("search_notes", {
      query: uniquePhrase,
    });
    assert.ok(result.results, "Should return search results via authenticated session");
    assert.ok(Array.isArray(result.results), "Results should be array");
  });

  // --- System Operations ---

  test("OAUTH-008: Authenticated backup_status", async () => {
    const result = await client.callTool("backup_status", {});
    assert.ok(result, "Should return backup status via authenticated session");
    assert.ok(result.status !== undefined, "Should have status field");
    console.log(`  Backup status: ${result.status}`);
  });

  test("OAUTH-009: Authenticated memory_info", async () => {
    const result = await client.callTool("memory_info", {});
    assert.ok(result, "Should return memory info via authenticated session");
    assert.ok(
      result.summary || result.storage || result.total_notes !== undefined,
      "Should have memory info data"
    );
  });

  // --- Error Handling ---

  test("OAUTH-010: Error on invalid parameters", async () => {
    const error = await client.callToolExpectError("get_note", {
      id: "00000000-0000-0000-0000-000000000000",
    });
    assert.ok(error.error, "Should return error for non-existent note ID");
  });

  // --- API Key Management ---

  test("OAUTH-011: Create API key", async () => {
    const result = await client.callTool("create_api_key", {
      name: `Test Key ${MCPTestClient.uniqueId()}`,
      scope: "read",
    });
    assert.ok(result, "Should create API key");
    assert.ok(result.api_key || result.key, "Should return API key value");
  });

  test("OAUTH-012: List API keys", async () => {
    const result = await client.callTool("list_api_keys", {});
    assert.ok(result, "Should return API keys list");
    const keys = Array.isArray(result) ? result : (result.keys || result.api_keys || []);
    assert.ok(Array.isArray(keys), "API keys should be an array");
    console.log(`  Found ${keys.length} API keys`);
  });

  test("OAUTH-013: revoke_api_key not exposed via MCP", async () => {
    // revoke_api_key was removed from MCP tools (issue #315).
    // It is an admin-only operation available via REST API directly.
    const error = await client.callToolExpectError("revoke_api_key", {
      id: MCPTestClient.uniqueId(),
    });
    assert.ok(error.error, "revoke_api_key should not be available as MCP tool");
  });
});
