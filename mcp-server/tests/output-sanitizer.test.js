import { strict as assert } from "node:assert";
import fs from "node:fs";
import { describe, test } from "node:test";
import {
  ACCESS_TOKEN_PLACEHOLDER,
  API_KEY_PLACEHOLDER,
  pushSafeAuthCurlHeader,
  sanitizeMcpOutput,
  sanitizeMcpText,
} from "../lib/output-sanitizer.js";

const SECRET_VALUES = [
  "SECRET_TEST_BEARER_DO_NOT_LEAK",
  "SECRET_TEST_API_KEY_DO_NOT_LEAK",
  "mm_at_secretBearerValue",
  "mm_rt_secretRefreshValue",
  "mm_key_secretApiKeyValue",
  "sk-live-secret",
  "client_secret=raw-client-secret",
  "token=raw-query-token",
  "user:password@internal.example",
];

function assertNoSecrets(value) {
  const serialized = typeof value === "string" ? value : JSON.stringify(value);
  for (const secret of SECRET_VALUES) {
    assert.doesNotMatch(serialized, new RegExp(secret.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
}

describe("MCP model-visible output sanitizer", () => {
  test("recursively redacts tokens, API keys, credential URLs, and control characters", () => {
    const output = sanitizeMcpOutput({
      curl_command:
        'curl -H "Authorization: Bearer SECRET_TEST_BEARER_DO_NOT_LEAK" "https://user:password@internal.example/api?token=raw-query-token"',
      nested: {
        api_key: "mm_key_secretApiKeyValue",
        provider_key: "sk-live-secret",
        text: "bad\u0000line client_secret=raw-client-secret",
      },
      list: ["Bearer mm_at_secretBearerValue", "refresh mm_rt_secretRefreshValue"],
    });

    assertNoSecrets(output);
    const serialized = JSON.stringify(output);
    assert.match(serialized, new RegExp(ACCESS_TOKEN_PLACEHOLDER.replace(/[<>]/g, "\\$&")));
    assert.match(serialized, new RegExp(API_KEY_PLACEHOLDER.replace(/[<>]/g, "\\$&")));
    assert.doesNotMatch(serialized, /\u0000/);
  });

  test("redacts error text before it is returned as MCP isError content", () => {
    const errorText = sanitizeMcpText(
      "API error: Authorization: Bearer SECRET_TEST_BEARER_DO_NOT_LEAK at https://user:password@internal.example/path?client_secret=raw-client-secret\r\n"
    );

    assertNoSecrets(errorText);
    assert.match(errorText, /Authorization: Bearer <ACCESS_TOKEN>/);
    assert.match(errorText, /client_secret=<REDACTED>/);
  });

  test("safe curl auth helper never interpolates runtime credentials", () => {
    const parts = ["curl -X POST"];
    pushSafeAuthCurlHeader(parts);

    assert.equal(parts[1], '-H "Authorization: Bearer <ACCESS_TOKEN>"');
    assertNoSecrets(parts.join(" "));
  });

  test("returned documentation source uses scanner-safe stream placeholders", () => {
    const source = fs.readFileSync(new URL("../index.js", import.meta.url), "utf8");

    assert.doesNotMatch(source, /Authorization: Bearer mm_at_/);
    assert.doesNotMatch(source, /token=mm_at_/);
    assert.match(source, /Authorization: Bearer <ACCESS_TOKEN>/);
    assert.match(source, /token=<STREAM_TOKEN>/);
  });
});
