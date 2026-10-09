import { describe, test } from "node:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";

import {
  apiAuthorizationHeader,
  getSseMessageSessionId,
  sessionLogId,
  sessionPrincipalMatches,
} from "../lib/session-security.js";

describe("session principal binding", () => {
  for (const route of ["POST /", "GET /", "DELETE /", "POST /messages"]) {
    test(`${route} rejects user B's valid token on user A's session`, () => {
      const session = { principalFingerprint: "principal-a" };
      assert.equal(sessionPrincipalMatches(session, "principal-b"), false);
    });
  }

  test("matching principals may reuse the session", () => {
    assert.equal(sessionPrincipalMatches({ principalFingerprint: "principal-a" }, "principal-a"), true);
  });
});

describe("per-request token forwarding", () => {
  test("HTTP requests use the current request bearer, not the server API key", () => {
    assert.equal(
      apiAuthorizationHeader({ transport: "http", requestToken: "current-request-token", apiKey: "server-key" }),
      "Bearer current-request-token",
    );
  });

  test("HTTP user requests do not fall back to FORTEMI_API_KEY", () => {
    assert.equal(apiAuthorizationHeader({ transport: "http", requestToken: null, apiKey: "server-key" }), null);
  });

  test("stdio keeps the server API key behavior", () => {
    assert.equal(apiAuthorizationHeader({ transport: "stdio", requestToken: null, apiKey: "server-key" }), "Bearer server-key");
  });
});

describe("session id handling", () => {
  test("logs contain only truncated sha256 session ids", () => {
    const sessionId = "session-cleartext-123";
    const logged = sessionLogId(sessionId);
    assert.equal(logged, crypto.createHash("sha256").update(sessionId).digest("hex").slice(0, 12));
    assert.equal(logged.includes(sessionId), false);
  });

  test("SSE messages accept query session ids only behind MCP_LEGACY_SSE", () => {
    assert.equal(getSseMessageSessionId({ headerSessionId: "header-id", querySessionId: "query-id", legacySse: false }), "header-id");
    assert.equal(getSseMessageSessionId({ headerSessionId: null, querySessionId: "query-id", legacySse: false }), null);
    assert.equal(getSseMessageSessionId({ headerSessionId: null, querySessionId: "query-id", legacySse: true }), "query-id");
  });
});
