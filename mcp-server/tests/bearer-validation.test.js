// Server-independent tests for MCP bearer validation (#1151): external OIDC tokens and Fortemi tokens.

import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { classifyBearer, mustReject, validateBearer } from "../lib/bearer-validation.js";

const EXTERNAL = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ0b2tlbi1zZW50aW5lbCJ9.c2ln";

function jsonResponse(status, body) {
  return { ok: status >= 200 && status < 300, status, json: async () => body };
}

function options(handler) {
  const calls = [];
  return {
    calls,
    apiBase: "http://api.internal:3000",
    clientId: "mcp-client",
    clientSecret: "mcp-secret",
    fetchImpl: async (url, init) => {
      calls.push({ url, init });
      return handler(url, init);
    },
  };
}

describe("classifyBearer", () => {
  test("separates Fortemi access, Fortemi refresh and external tokens by prefix", () => {
    assert.equal(classifyBearer("mm_at_abc"), "fortemi");
    assert.equal(classifyBearer("mm_key_abc"), "fortemi");
    assert.equal(classifyBearer("mm_rt_abc"), "refresh");
    assert.equal(classifyBearer(EXTERNAL), "external");
  });
});

describe("external OIDC tokens use the API's hosted verifier", () => {
  test("a verified token with mcp scope is accepted without local JWT checks", async () => {
    const opts = options(() =>
      jsonResponse(200, { active: true, token_class: "hosted_oidc", scope: "mcp read", tenant_bound: true, exp: 1900000000 }),
    );
    const result = await validateBearer(`Bearer ${EXTERNAL}`, opts);
    assert.equal(result.valid, true);
    assert.equal(result.kind, "external");
    assert.equal(opts.calls.length, 1);
    assert.equal(opts.calls[0].url, "http://api.internal:3000/api/v1/auth/token-info");
    assert.equal(opts.calls[0].init.headers.Authorization, `Bearer ${EXTERNAL}`);
    assert.equal(opts.calls[0].init.method, "GET");
  });

  for (const [name, status, expected] of [
    ["wrong issuer", 401, 401],
    ["wrong audience", 401, 401],
    ["expired token", 401, 401],
    ["unknown or inactive tenant", 403, 403],
    ["verifier outage", 503, 503],
  ]) {
    test(`${name} is rejected with ${expected}`, async () => {
      const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => jsonResponse(status, { title: "x" })));
      assert.equal(result.valid, false);
      assert.equal(result.status, expected);
      assert.equal(mustReject(result, false), true, "external failures never downgrade to anonymous");
    });
  }

  test("a verified token without mcp scope is rejected with 403", async () => {
    const result = await validateBearer(
      `Bearer ${EXTERNAL}`,
      options(() => jsonResponse(200, { active: true, token_class: "hosted_oidc", scope: "read write" })),
    );
    assert.deepEqual([result.valid, result.status, result.reason], [false, 403, "insufficient_scope"]);
  });

  test("a non-hosted token class from the verifier is rejected", async () => {
    const result = await validateBearer(
      `Bearer ${EXTERNAL}`,
      options(() => jsonResponse(200, { active: true, token_class: "api_key", scope: "mcp" })),
    );
    assert.deepEqual([result.valid, result.status], [false, 401]);
  });

  test("network failures report the verifier as unavailable", async () => {
    const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => { throw new TypeError("fetch failed"); }));
    assert.deepEqual([result.valid, result.status, result.reason], [false, 503, "verifier_unreachable"]);
  });

  test("results never echo the token in reasons", async () => {
    const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => jsonResponse(401, {})));
    assert.ok(!JSON.stringify({ ...result, token: undefined }).includes("token-sentinel"));
    assert.ok(!JSON.stringify(result).includes(EXTERNAL));
  });
});

describe("refresh tokens are never bearer credentials", () => {
  test("a Fortemi refresh token is rejected without calling the API", async () => {
    const opts = options(() => { throw new Error("must not be called"); });
    const result = await validateBearer("Bearer mm_rt_secret", opts);
    assert.deepEqual([result.valid, result.status, result.reason], [false, 401, "not_an_access_token"]);
    assert.equal(opts.calls.length, 0);
    assert.equal(mustReject(result, false), true);
  });

  test("introspection reporting a refresh token type is rejected", async () => {
    const result = await validateBearer(
      "Bearer mm_at_disguised",
      options(() => jsonResponse(200, { active: true, token_type: "refresh_token", scope: "mcp" })),
    );
    assert.deepEqual([result.valid, result.status], [false, 401]);
  });
});

describe("Fortemi-issued tokens keep self-hosted behavior", () => {
  test("active access tokens with mcp or read scope are accepted via introspection", async () => {
    const opts = options(() => jsonResponse(200, { active: true, token_type: "Bearer", scope: "read" }));
    const result = await validateBearer("Bearer mm_at_valid", opts);
    assert.equal(result.valid, true);
    assert.equal(opts.calls[0].url, "http://api.internal:3000/oauth/introspect");
    assert.equal(opts.calls[0].init.headers.Authorization, `Basic ${Buffer.from("mcp-client:mcp-secret").toString("base64")}`);
  });

  test("invalid Fortemi tokens only block requests when auth is required", async () => {
    const result = await validateBearer("Bearer mm_at_stale", options(() => jsonResponse(200, { active: false })));
    assert.equal(result.valid, false);
    assert.equal(mustReject(result, false), false);
    assert.equal(mustReject(result, true), true);
  });

  test("a missing or malformed header is unauthenticated", async () => {
    for (const header of [undefined, "", "Basic abc", "Bearer ", "Bearer    "]) {
      const result = await validateBearer(header, options(() => { throw new Error("unused"); }));
      assert.deepEqual([result.valid, result.status], [false, 401], String(header));
    }
  });
});
