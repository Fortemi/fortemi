import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_RESOURCE_DOCUMENTATION_URL,
  buildProtectedResourceMetadata,
  resolveResourceDocumentationUrl,
} from "../lib/resource-metadata.js";

test("protected-resource metadata advertises curated consumer documentation", () => {
  const metadata = buildProtectedResourceMetadata({
    resource: "https://memory.example.com/mcp",
    authorizationServer: "https://memory.example.com",
  });

  assert.deepEqual(metadata, {
    resource: "https://memory.example.com/mcp",
    authorization_servers: ["https://memory.example.com"],
    bearer_methods_supported: ["header"],
    scopes_supported: ["mcp"],
    resource_documentation: DEFAULT_RESOURCE_DOCUMENTATION_URL,
  });
  assert.doesNotMatch(metadata.resource_documentation, /\/docs$/);
});

test("resource documentation override accepts local HTTP URLs", () => {
  assert.equal(
    resolveResourceDocumentationUrl("http://localhost:8080/consumer-api"),
    "http://localhost:8080/consumer-api",
  );
});

test("resource documentation rejects unsafe URL forms", () => {
  for (const value of [
    "/relative/docs",
    "file:///srv/private/docs",
    "https://operator:secret@example.com/docs",
  ]) {
    assert.throws(() => resolveResourceDocumentationUrl(value));
  }
});

test("RFC 9728 metadata covers self-issued and external-issuer profiles (#1151)", () => {
  const selfIssued = buildProtectedResourceMetadata({
    resource: "https://memory.example.com/mcp",
    authorizationServer: "https://memory.example.com",
  });
  assert.equal(selfIssued.resource, "https://memory.example.com/mcp");
  assert.deepEqual(selfIssued.authorization_servers, ["https://memory.example.com"]);

  // External issuer: ISSUER_URL is the IdP realm and MCP_RESOURCE_URI equals
  // FORTEMI_AUTH_AUDIENCE, so clients obtain tokens the API's verifier accepts.
  const external = buildProtectedResourceMetadata({
    resource: "https://memory.example.com",
    authorizationServer: "https://idp.example.com/realms/acme",
  });
  assert.equal(external.resource, "https://memory.example.com");
  assert.deepEqual(external.authorization_servers, ["https://idp.example.com/realms/acme"]);
  assert.deepEqual(external.scopes_supported, ["mcp"]);
  assert.deepEqual(external.bearer_methods_supported, ["header"]);
});
