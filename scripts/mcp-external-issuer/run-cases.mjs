// Exercises MCP-over-HTTP with externally issued OIDC tokens against a live hosted stack (#1151).
//
// Tokens are minted by the disposable fixture issuer, kept in memory only, and scanned for in
// the API and MCP logs at the end. Usage (driven by scripts/test-mcp-external-issuer.sh):
//   node run-cases.mjs <mcpUrl> <audience> <activeTenant> <suspendedTenant> <log>...
// Requires FORTEMI_TEST_ISSUER and FORTEMI_TEST_CA (read by fixture-issuer-client.mjs).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { issuerRequest } from "../../crates/matric-api/src/scoped_search_tests/fixture-issuer-client.mjs";

const [mcpUrl, audience, activeTenant, suspendedTenant, ...logs] = process.argv.slice(2);
assert.ok(mcpUrl && audience && activeTenant && suspendedTenant && logs.length >= 2, "usage");
const issuer = process.env.FORTEMI_TEST_ISSUER;
const presented = [];

async function mint(body) {
  const { token } = await issuerRequest("/fixture-token", { audience, scope: "mcp read write", ...body });
  presented.push(token);
  return token;
}

function remember(token) {
  presented.push(token);
  return token;
}

const initialize = (id = 1) => ({
  jsonrpc: "2.0", id, method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "mcp-external-issuer-e2e", version: "1" } },
});

async function mcpPost(body, { token, session } = {}) {
  const headers = { "content-type": "application/json", accept: "application/json, text/event-stream" };
  if (token !== undefined) headers.authorization = `Bearer ${token}`;
  if (session) headers["mcp-session-id"] = session;
  const response = await fetch(mcpUrl + "/", { method: "POST", headers, body: JSON.stringify(body), signal: AbortSignal.timeout(15000) });
  const text = await response.text();
  return { response, text };
}

/** Parse a JSON or single-event SSE JSON-RPC reply. */
function rpcResult(text) {
  const data = text.trimStart().startsWith("{")
    ? text
    : text.split("\n").filter((line) => line.startsWith("data:")).map((line) => line.slice(5)).join("");
  return JSON.parse(data);
}

const results = [];
function record(name, status, expected, extra = "") {
  results.push({ name, status, expected });
  console.log(`[mcp-oidc-it] ${name}: HTTP ${status}${extra}`);
}

// --- Accepted: external token, configured audience, mcp scope, active tenant ----------------
{
  const token = await mint({ tenant: activeTenant, kind: "valid" });
  const init = await mcpPost(initialize(), { token });
  assert.equal(init.response.status, 200, `valid initialize: ${init.text.slice(0, 200)}`);
  const session = init.response.headers.get("mcp-session-id");
  assert.ok(session, "initialize returned no MCP session id");
  assert.ok(rpcResult(init.text).result?.serverInfo, "initialize result lacks serverInfo");
  await mcpPost({ jsonrpc: "2.0", method: "notifications/initialized" }, { token, session });
  const call = await mcpPost(
    { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "list_notes", arguments: { limit: 1 } } },
    { token, session },
  );
  assert.equal(call.response.status, 200, "tools/call status");
  const reply = rpcResult(call.text);
  assert.ok(reply.result, `tools/call failed: ${call.text.slice(0, 300)}`);
  assert.notEqual(reply.result.isError, true, `list_notes errored through the API: ${call.text.slice(0, 300)}`);
  record("valid external token (initialize + tools/call list_notes)", 200, [200]);
}

// --- Rejected ------------------------------------------------------------------------------
const rejected = [
  ["wrong issuer", () => mint({ tenant: activeTenant, kind: "wrong-issuer" }), [401]],
  ["wrong audience", () => mint({ tenant: activeTenant, audience: "https://other-resource.fortemi.invalid/" }), [401]],
  ["unknown tenant", () => mint({ tenant: activeTenant, kind: "unknown-tenant" }), [403]],
  ["inactive (suspended) tenant", () => mint({ tenant: suspendedTenant }), [403]],
  ["missing mcp scope", () => mint({ tenant: activeTenant, scope: "read write" }), [403]],
  ["expired token", () => mint({ tenant: activeTenant, kind: "expired" }), [401]],
  ["refresh token (IdP JWT refresh token)", () => mint({ tenant: activeTenant, kind: "refresh-jwt" }), [401]],
  ["refresh token (opaque IdP refresh token)", async () => remember(`opaque-refresh-${Date.now().toString(36)}-e2e`), [401]],
  ["refresh token (Fortemi mm_rt_)", async () => remember(`mm_rt_e2eRefreshSentinel${Date.now().toString(36)}`), [401]],
  ["missing bearer", async () => undefined, [401]],
];
for (const [name, tokenFor, expected] of rejected) {
  const token = await tokenFor();
  const { response, text } = await mcpPost(initialize(), { token });
  const challenge = response.headers.get("www-authenticate") || "";
  record(name, response.status, expected, challenge ? ` (${challenge.split(",").slice(0, 2).join(",")})` : "");
  assert.ok(expected.includes(response.status), `${name}: expected ${expected}, got ${response.status}`);
  assert.ok(challenge.startsWith("Bearer "), `${name}: missing WWW-Authenticate challenge`);
  assert.ok(challenge.includes("resource_metadata="), `${name}: challenge lacks resource_metadata`);
  if (name === "missing mcp scope") {
    assert.ok(challenge.includes('error="insufficient_scope"') && challenge.includes('scope="mcp"'), "403 challenge");
  } else {
    // Asking for the mcp scope again cannot fix an issuer, audience, expiry or tenant failure.
    assert.ok(!challenge.includes("insufficient_scope"), `${name}: challenge must not claim insufficient_scope`);
  }
  if (token) assert.ok(!text.includes(token) && !challenge.includes(token), `${name}: token echoed in response`);
  assert.equal(response.headers.get("mcp-session-id"), null, `${name}: rejected request created a session`);
}

// --- Protected-resource metadata for the external-issuer profile ----------------------------
{
  const response = await fetch(`${mcpUrl}/.well-known/oauth-protected-resource`, { signal: AbortSignal.timeout(5000) });
  assert.equal(response.status, 200);
  const metadata = await response.json();
  assert.equal(metadata.resource, audience, "resource must be the configured audience");
  assert.deepEqual(metadata.authorization_servers, [issuer], "authorization_servers must be the external issuer");
  assert.ok(metadata.scopes_supported.includes("mcp"));
  assert.deepEqual(metadata.bearer_methods_supported, ["header"]);
  console.log(`[mcp-oidc-it] protected-resource metadata: resource=${metadata.resource} authorization_servers=${metadata.authorization_servers}`);
}

// --- No token material in logs --------------------------------------------------------------
await new Promise((resolve) => setTimeout(resolve, 500));
const fragments = presented.flatMap((token) => [token, ...token.split(".").filter((part) => part.length >= 16)]);
for (const file of logs) {
  const content = readFileSync(file, "utf8");
  assert.ok(content.length > 0, `${file} is empty`);
  for (const fragment of fragments) assert.ok(!content.includes(fragment), `token material leaked into ${file}`);
}
console.log(`[mcp-oidc-it] ${presented.length} presented tokens absent from ${logs.length} logs`);
console.log(JSON.stringify({ cases: results }));
