import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 17: OAuth Authentication", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("OAUTH-001: Register OAuth client", async () => {
    const result = await client.callTool("oauth_register_client", {
      client_name: `Test Client ${MCPTestClient.uniqueId()}`,
      grant_types: ["client_credentials"],
      scope: "read",
    });
    assert.ok(result, "Should register client");
    assert.ok(
      result.client_id || result.clientId,
      "Should return client_id"
    );
  });

  test("OAUTH-002: Create API key", async () => {
    const result = await client.callTool("create_api_key", {
      name: `Test Key ${MCPTestClient.uniqueId()}`,
      scope: "read",
    });
    assert.ok(result, "Should create API key");
    assert.ok(result.api_key || result.key, "Should return API key value");
  });

  test("OAUTH-003: List API keys", async () => {
    const result = await client.callTool("list_api_keys", {});
    assert.ok(result, "Should return API keys list");
  });

  test("OAUTH-004: Revoke API key", async () => {
    // Create then revoke
    const created = await client.callTool("create_api_key", {
      name: `Revoke Test ${MCPTestClient.uniqueId()}`,
      scope: "read",
    });
    assert.ok(created, "Should create key");

    const keyId = created.id || created.key_id;
    if (keyId) {
      const result = await client.callTool("revoke_api_key", { id: keyId });
      assert.ok(result, "Should revoke key");
    }
  });
});
