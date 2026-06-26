import { strict as assert } from "node:assert";
import { describe, test } from "node:test";
import {
  formatFortemiApiError,
  formatMcpReauthorizationError,
} from "../lib/api-errors.js";

describe("Fortemi API problem formatting", () => {
  test("surfaces RFC 9457 problem fields without legacy envelope text", () => {
    const message = formatFortemiApiError(
      403,
      "Forbidden",
      "application/problem+json",
      JSON.stringify({
        type: "https://fortemi.com/problems/forbidden",
        title: "Forbidden",
        status: 403,
        detail: "Access denied.",
        request_id: "req_01JZSAFE",
      })
    );

    assert.equal(
      message,
      "API error 403: Forbidden | detail: Access denied. | type: https://fortemi.com/problems/forbidden | request_id: req_01JZSAFE"
    );
    assert.doesNotMatch(message, /error_description/);
  });

  test("does not copy legacy JSON error bodies into MCP tool errors", () => {
    const message = formatFortemiApiError(
      500,
      "Internal Server Error",
      "application/json",
      JSON.stringify({
        error: "database failed at postgresql://user:secret@db/internal",
        stderr: "/var/lib/fortemi/private/path",
      })
    );

    assert.equal(message, "API error 500: Internal Server Error");
    assert.doesNotMatch(message, /postgresql:\/\//);
    assert.doesNotMatch(message, /secret/);
    assert.doesNotMatch(message, /\/var\/lib/);
  });

  test("does not copy raw text upstream errors into MCP tool errors", () => {
    const message = formatFortemiApiError(
      502,
      "Bad Gateway",
      "text/plain",
      "provider http://localhost:11434 failed with token sk-live-secret"
    );

    assert.equal(message, "API error 502: Bad Gateway");
    assert.doesNotMatch(message, /localhost/);
    assert.doesNotMatch(message, /sk-live-secret/);
  });

  test("keeps reauthorization guidance while suppressing raw upstream body", () => {
    const message = formatMcpReauthorizationError(
      401,
      "Unauthorized",
      "application/json",
      JSON.stringify({
        error: "unauthorized",
        error_description: "token bearer-secret expired against internal issuer",
      })
    );

    assert.match(message, /requires re-authorization/);
    assert.match(message, /API error 401: Unauthorized/);
    assert.doesNotMatch(message, /bearer-secret/);
    assert.doesNotMatch(message, /internal issuer/);
    assert.doesNotMatch(message, /error_description/);
  });
});
