// Server-independent tests for MCP startup configuration (#1171): API URL resolution and auth posture.

import { describe, test } from "node:test";
import assert from "node:assert/strict";

import {
  loadStartupConfig,
  parseStrictBool,
  resolveApiBase,
  resolveAuthPolicy,
  resolvePublicUrl,
  StartupConfigError,
} from "../lib/startup-config.js";

describe("resolveApiBase", () => {
  test("MATRIC_API_URL wins over FORTEMI_URL and the local layout", () => {
    const env = { MATRIC_API_URL: "http://api.internal:3000/", FORTEMI_URL: "http://other:3000", MCP_API_LAYOUT: "bundle" };
    assert.deepEqual(resolveApiBase(env), { apiBase: "http://api.internal:3000", source: "MATRIC_API_URL" });
  });

  test("FORTEMI_URL is used when MATRIC_API_URL is unset", () => {
    assert.deepEqual(resolveApiBase({ FORTEMI_URL: "http://localhost:3000" }), {
      apiBase: "http://localhost:3000",
      source: "FORTEMI_URL",
    });
  });

  test("the bundle and sidecar layouts default to the local API", () => {
    assert.equal(resolveApiBase({ MCP_API_LAYOUT: "bundle" }).apiBase, "http://127.0.0.1:3000");
    assert.equal(resolveApiBase({ MCP_API_LAYOUT: "sidecar", MCP_LOCAL_API_PORT: "3900" }).apiBase, "http://127.0.0.1:3900");
  });

  test("with nothing set it fails instead of defaulting to a hosted URL", () => {
    assert.throws(() => resolveApiBase({}), StartupConfigError);
    assert.throws(() => resolveApiBase({ FORTEMI_URL: "   " }), /No Fortemi API URL configured/);
  });

  test("ISSUER_URL alone is not an API base", () => {
    assert.throws(() => resolveApiBase({ ISSUER_URL: "https://memory.example.com" }), StartupConfigError);
  });

  test("rejects malformed values", () => {
    assert.throws(() => resolveApiBase({ FORTEMI_URL: "not a url" }), /not a valid URL/);
    assert.throws(() => resolveApiBase({ FORTEMI_URL: "ftp://api:21" }), /http or https/);
    assert.throws(() => resolveApiBase({ FORTEMI_URL: "http://user:pw@api:3000" }), /credentials/);
    assert.throws(() => resolveApiBase({ MCP_API_LAYOUT: "cloud" }), /MCP_API_LAYOUT/);
    assert.throws(() => resolveApiBase({ MCP_API_LAYOUT: "bundle", MCP_LOCAL_API_PORT: "99999" }), /MCP_LOCAL_API_PORT/);
  });

  test("no resolution path yields fortemi.com", () => {
    const candidates = [
      { FORTEMI_URL: "http://localhost:3000" },
      { MATRIC_API_URL: "http://127.0.0.1:3000" },
      { MCP_API_LAYOUT: "bundle" },
    ];
    for (const env of candidates) assert.doesNotMatch(resolveApiBase(env).apiBase, /fortemi\.com/);
  });
});

describe("resolvePublicUrl", () => {
  test("prefers ISSUER_URL and otherwise uses the API base", () => {
    assert.equal(resolvePublicUrl({ ISSUER_URL: "https://memory.example.com/" }, "http://api:3000"), "https://memory.example.com");
    assert.equal(resolvePublicUrl({}, "http://api:3000"), "http://api:3000");
  });
});

describe("resolveAuthPolicy", () => {
  test("requires auth by default", () => {
    assert.deepEqual(resolveAuthPolicy({}), { requireAuth: true, anonymous: false });
    assert.deepEqual(resolveAuthPolicy({ REQUIRE_AUTH: "1" }), { requireAuth: true, anonymous: false });
  });

  test("REQUIRE_AUTH=false without the acknowledgment refuses to start", () => {
    assert.throws(() => resolveAuthPolicy({ REQUIRE_AUTH: "false" }), /I_UNDERSTAND_NO_AUTH=true/);
    assert.throws(() => resolveAuthPolicy({ REQUIRE_AUTH: "false", I_UNDERSTAND_NO_AUTH: "false" }), StartupConfigError);
  });

  test("anonymous mode needs the explicit pairing", () => {
    assert.deepEqual(resolveAuthPolicy({ REQUIRE_AUTH: "false", I_UNDERSTAND_NO_AUTH: "true" }), {
      requireAuth: false,
      anonymous: true,
    });
  });

  test("multi-tenant never runs anonymous", () => {
    assert.throws(
      () => resolveAuthPolicy({ REQUIRE_AUTH: "false", I_UNDERSTAND_NO_AUTH: "true", FORTEMI_MULTI_TENANT: "true" }),
      /FORTEMI_MULTI_TENANT/
    );
  });

  test("security booleans are strict", () => {
    assert.throws(() => resolveAuthPolicy({ REQUIRE_AUTH: "yes" }), /REQUIRE_AUTH must be one of/);
    assert.throws(() => parseStrictBool("X", "maybe", false), StartupConfigError);
    assert.equal(parseStrictBool("X", undefined, true), true);
  });
});

describe("loadStartupConfig", () => {
  test("stdio does not apply the HTTP auth policy", () => {
    const config = loadStartupConfig({ FORTEMI_URL: "http://localhost:3000", REQUIRE_AUTH: "false" });
    assert.equal(config.transport, "stdio");
    assert.equal(config.auth.requireAuth, false);
  });

  test("http applies the auth policy", () => {
    assert.throws(
      () => loadStartupConfig({ MCP_TRANSPORT: "http", FORTEMI_URL: "http://localhost:3000", REQUIRE_AUTH: "false" }),
      StartupConfigError
    );
    const config = loadStartupConfig({ MCP_TRANSPORT: "http", MCP_API_LAYOUT: "bundle" });
    assert.equal(config.auth.requireAuth, true);
    assert.equal(config.publicUrl, "http://127.0.0.1:3000");
  });

  test("http refuses MCP resource/audience drift for external audiences", () => {
    assert.throws(
      () => loadStartupConfig({
        MCP_TRANSPORT: "http",
        FORTEMI_URL: "http://localhost:3000",
        MCP_RESOURCE_URI: "https://memory.example.com/mcp",
        FORTEMI_AUTH_AUDIENCE: "https://memory.example.com",
      }),
      /MCP_RESOURCE_URI must exactly match/
    );

    const config = loadStartupConfig({
      MCP_TRANSPORT: "http",
      FORTEMI_URL: "http://localhost:3000",
      MCP_RESOURCE_URI: "https://memory.example.com/mcp",
      FORTEMI_AUTH_AUDIENCES: "https://memory.example.com, https://memory.example.com/mcp",
    });
    assert.deepEqual(config.resource.acceptedAudiences, [
      "https://memory.example.com",
      "https://memory.example.com/mcp",
    ]);
  });

  test("http refuses non-https MCP_RESOURCE_URI outside explicit local development", () => {
    assert.throws(
      () => loadStartupConfig({
        MCP_TRANSPORT: "http",
        FORTEMI_URL: "http://localhost:3000",
        MCP_RESOURCE_URI: "http://memory.example.com/mcp",
        FORTEMI_AUTH_AUDIENCE: "http://memory.example.com/mcp",
      }),
      /must use https/
    );
    const config = loadStartupConfig({
      MCP_TRANSPORT: "http",
      FORTEMI_URL: "http://localhost:3000",
      MCP_RESOURCE_URI: "http://localhost:3001/mcp",
      FORTEMI_AUTH_AUDIENCE: "http://localhost:3001/mcp",
      FORTEMI_ALLOW_LOCAL_ISSUER: "true",
    });
    assert.equal(config.resource.resourceUri, "http://localhost:3001/mcp");
  });

  test("token exchange fails closed until fully configured", () => {
    const base = {
      MCP_TRANSPORT: "http",
      FORTEMI_URL: "http://localhost:3000",
      MCP_RESOURCE_URI: "https://mcp.example.com/mcp",
      FORTEMI_AUTH_AUDIENCES: "https://mcp.example.com/mcp,https://api.example.com",
      MCP_TOKEN_EXCHANGE: "true",
    };
    assert.throws(() => loadStartupConfig(base), /MCP_TOKEN_EXCHANGE_CLIENT_ID/);
    assert.throws(
      () => loadStartupConfig({ ...base, MCP_TOKEN_EXCHANGE_CLIENT_ID: "mcp-client" }),
      /CLIENT_SECRET/
    );
    assert.throws(
      () => loadStartupConfig({
        ...base,
        MCP_TOKEN_EXCHANGE_CLIENT_ID: "mcp-client",
        MCP_TOKEN_EXCHANGE_CLIENT_SECRET: "secret",
      }),
      /TOKEN_ENDPOINT/
    );
    assert.throws(
      () => loadStartupConfig({
        ...base,
        MCP_TOKEN_EXCHANGE_CLIENT_ID: "mcp-client",
        MCP_TOKEN_EXCHANGE_CLIENT_SECRET: "secret",
        MCP_TOKEN_EXCHANGE_TOKEN_ENDPOINT: "https://idp.example.com/token",
      }),
      /EXCHANGE_AUDIENCE/
    );
    const config = loadStartupConfig({
      ...base,
      MCP_TOKEN_EXCHANGE_CLIENT_ID: "mcp-client",
      MCP_TOKEN_EXCHANGE_CLIENT_SECRET: "secret",
      MCP_TOKEN_EXCHANGE_TOKEN_ENDPOINT: "https://idp.example.com/token",
      MCP_TOKEN_EXCHANGE_AUDIENCE: "https://api.example.com",
    });
    assert.equal(config.tokenExchange.enabled, true);
    assert.equal(config.tokenExchange.audience, "https://api.example.com");
  });

  test("rejects an unknown transport", () => {
    assert.throws(() => loadStartupConfig({ MCP_TRANSPORT: "ws", FORTEMI_URL: "http://localhost:3000" }), /MCP_TRANSPORT/);
  });
});
