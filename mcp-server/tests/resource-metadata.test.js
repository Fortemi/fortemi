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
