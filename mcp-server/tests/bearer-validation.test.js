// Server-independent tests for MCP bearer validation (#1151/#1193).

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";

import {
  classifyBearer,
  createTokenInfoCache,
  mustReject,
  validateBearer,
} from "../lib/bearer-validation.js";

function jwt(claims) {
  const encoded = (value) => Buffer.from(JSON.stringify(value)).toString("base64url");
  return `${encoded({ alg: "RS256" })}.${encoded(claims)}.sig`;
}

const EXTERNAL = jwt({ iss: "https://idp.example.test/realms/acme", sub: "user-a" });

function jsonResponse(status, body) {
  return { ok: status >= 200 && status < 300, status, json: async () => body };
}

function options(handler, extra = {}) {
  const calls = [];
  return {
    calls,
    apiBase: "http://api.internal:3000",
    cache: createTokenInfoCache(),
    fetchImpl: async (url, init) => {
      calls.push({ url, init });
      return handler(url, init);
    },
    ...extra,
  };
}

function fingerprint(iss, sub) {
  return crypto.createHash("sha256").update(`${iss}\n${sub}`).digest("hex");
}

describe("classifyBearer", () => {
  test("separates Fortemi access, PAT, Fortemi refresh and external tokens by prefix", () => {
    assert.equal(classifyBearer("mm_at_abc"), "fortemi");
    assert.equal(classifyBearer("mm_key_abc"), "fortemi");
    assert.equal(classifyBearer("mm_pat_abc"), "fortemi");
    assert.equal(classifyBearer("mm_rt_abc"), "refresh");
    assert.equal(classifyBearer(EXTERNAL), "external");
  });
});

describe("all bearer classes use token-info", () => {
  test("a verified external token with mcp scope is accepted and fingerprinted", async () => {
    const opts = options(() =>
      jsonResponse(200, { active: true, token_class: "hosted_oidc", scope: "mcp read", exp: 1900000000 }),
    );
    const result = await validateBearer(`Bearer ${EXTERNAL}`, opts);
    assert.equal(result.valid, true);
    assert.equal(result.kind, "external");
    assert.equal(result.forwardToken, EXTERNAL);
    assert.equal(result.principalFingerprint, fingerprint("https://idp.example.test/realms/acme", "user-a"));
    assert.equal(opts.calls.length, 1);
    assert.equal(opts.calls[0].url, "http://api.internal:3000/api/v1/auth/token-info");
    assert.equal(opts.calls[0].init.headers.Authorization, `Bearer ${EXTERNAL}`);
    assert.equal(opts.calls[0].init.method, "GET");
  });

  for (const tokenClass of ["hosted_oidc", "oauth_access_token", "api_key", "pat"]) {
    test(`a read-only ${tokenClass} credential gets insufficient_scope`, async () => {
      const token = tokenClass === "hosted_oidc" ? EXTERNAL : "mm_at_readonly";
      const result = await validateBearer(
        `Bearer ${token}`,
        options(() => jsonResponse(200, { active: true, token_class: tokenClass, scope: "read" })),
      );
      assert.deepEqual([result.valid, result.status, result.reason], [false, 403, "insufficient_scope"]);
      assert.equal(mustReject(result, true), true);
    });
  }

  test("PAT token-info ids produce a user-stable session fingerprint", async () => {
    const result = await validateBearer(
      "Bearer mm_pat_secret",
      options(() => jsonResponse(200, { active: true, token_class: "pat", scope: "mcp", pat_id: "pat-123" })),
    );
    assert.equal(result.valid, true);
    assert.equal(result.principalFingerprint, crypto.createHash("sha256").update("pat\npat-123").digest("hex"));
  });

  test("a token rejected by token-info is never returned for forwarding", async () => {
    const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => jsonResponse(401, {})));
    assert.equal(result.valid, false);
    assert.equal(result.forwardToken, undefined);
    assert.equal(mustReject(result, false), true);
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
      assert.equal(mustReject(result, false), true, "invalid credentials never downgrade to anonymous");
    });
  }

  test("network failures report the verifier as unavailable", async () => {
    const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => { throw new TypeError("fetch failed"); }));
    assert.deepEqual([result.valid, result.status, result.reason], [false, 503, "verifier_unreachable"]);
  });

  test("results never echo the token in reasons", async () => {
    const result = await validateBearer(`Bearer ${EXTERNAL}`, options(() => jsonResponse(401, {})));
    assert.ok(!JSON.stringify({ ...result, token: undefined }).includes("user-a"));
    assert.ok(!JSON.stringify(result).includes(EXTERNAL));
  });
});

describe("refresh and malformed credentials", () => {
  test("a Fortemi refresh token is rejected without calling the API", async () => {
    const opts = options(() => { throw new Error("must not be called"); });
    const result = await validateBearer("Bearer mm_rt_secret", opts);
    assert.deepEqual([result.valid, result.status, result.reason], [false, 401, "not_an_access_token"]);
    assert.equal(opts.calls.length, 0);
    assert.equal(mustReject(result, false), true);
  });

  test("a missing or malformed header is unauthenticated", async () => {
    for (const header of [undefined, "", "Basic abc", "Bearer ", "Bearer    "]) {
      const result = await validateBearer(header, options(() => { throw new Error("unused"); }));
      assert.deepEqual([result.valid, result.status], [false, 401], String(header));
    }
  });
});

describe("token-info cache", () => {
  test("successful results cache for min(exp - now, 60 seconds)", async () => {
    let now = 1_000_000;
    const cache = createTokenInfoCache({ now: () => now });
    const opts = options(
      () => jsonResponse(200, { active: true, token_class: "hosted_oidc", scope: "mcp", exp: 1060 }),
      { cache, now: () => now },
    );
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, true);
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, true);
    assert.equal(opts.calls.length, 1);
    now += 60_001;
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, true);
    assert.equal(opts.calls.length, 2);
  });

  test("failed token-info results cache for at most five seconds", async () => {
    let now = 1_000_000;
    const cache = createTokenInfoCache({ now: () => now });
    const opts = options(() => jsonResponse(401, {}), { cache, now: () => now });
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, false);
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, false);
    assert.equal(opts.calls.length, 1);
    now += 5_001;
    assert.equal((await validateBearer(`Bearer ${EXTERNAL}`, opts)).valid, false);
    assert.equal(opts.calls.length, 2);
  });
});

describe("RFC 8693 token exchange", () => {
  test("exchanges the inbound token and forwards only the API-audience access token", async () => {
    const opts = options((url) => {
      if (url.endsWith("/api/v1/auth/token-info")) {
        return jsonResponse(200, { active: true, token_class: "hosted_oidc", scope: "mcp" });
      }
      return jsonResponse(200, { access_token: "api-audience-token" });
    }, {
      tokenExchange: {
        enabled: true,
        clientId: "mcp-exchanger",
        clientSecret: "secret",
        tokenEndpoint: "https://idp.example.test/token",
        audience: "https://api.example.test",
      },
    });
    const result = await validateBearer(`Bearer ${EXTERNAL}`, opts);
    assert.equal(result.valid, true);
    assert.equal(result.token, EXTERNAL);
    assert.equal(result.forwardToken, "api-audience-token");
    assert.equal(opts.calls.length, 2);
    assert.equal(opts.calls[1].url, "https://idp.example.test/token");
    assert.match(String(opts.calls[1].init.body), /grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange/);
    assert.match(String(opts.calls[1].init.body), /audience=https%3A%2F%2Fapi.example.test/);
  });
});
