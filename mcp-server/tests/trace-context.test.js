import { test } from "node:test";
import assert from "node:assert/strict";
import {
  extractMetaTraceContext,
  extractTraceContext,
  isValidTraceparent,
  withTraceHeaders,
} from "../lib/trace-context.js";

const VALID = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

test("traceparent grammar is strict", () => {
  assert.equal(isValidTraceparent(VALID), true);
  assert.equal(isValidTraceparent(VALID.toUpperCase()), false);
  assert.equal(isValidTraceparent("00-00000000000000000000000000000000-00f067aa0ba902b7-01"), false);
  assert.equal(isValidTraceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"), false);
  assert.equal(isValidTraceparent("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"), false);
  assert.equal(isValidTraceparent(`${VALID}\r\nX-Injected: 1`), false);
  assert.equal(isValidTraceparent(undefined), false);
});

test("extracts from node headers and drops baggage and bad tracestate", () => {
  assert.deepEqual(
    extractTraceContext({ traceparent: VALID, tracestate: "vendor=1", baggage: "tenant=x" }),
    { traceparent: VALID, tracestate: "vendor=1" }
  );
  assert.deepEqual(extractTraceContext({ traceparent: VALID, tracestate: "a=\u0001" }), {
    traceparent: VALID,
  });
  assert.equal(extractTraceContext({ traceparent: "garbage" }), null);
  assert.equal(extractTraceContext(undefined), null);
  assert.deepEqual(extractTraceContext(new Headers({ traceparent: VALID })), { traceparent: VALID });
});

test("reads tool-call _meta", () => {
  assert.deepEqual(extractMetaTraceContext({ traceparent: VALID }), { traceparent: VALID });
  assert.equal(extractMetaTraceContext({ traceparent: "nope" }), null);
  assert.equal(extractMetaTraceContext(null), null);
});

test("withTraceHeaders adds headers without dropping existing ones", () => {
  const plain = withTraceHeaders({ method: "GET", headers: { Authorization: "Bearer t" } }, {
    traceparent: VALID,
    tracestate: "v=1",
  });
  assert.deepEqual(plain.headers, { Authorization: "Bearer t", traceparent: VALID, tracestate: "v=1" });
  const fetchHeaders = withTraceHeaders({ headers: new Headers({ a: "1" }) }, { traceparent: VALID });
  assert.equal(fetchHeaders.headers.get("a"), "1");
  assert.equal(fetchHeaders.headers.get("traceparent"), VALID);
  const untouched = { headers: { a: "1" } };
  assert.equal(withTraceHeaders(untouched, null), untouched);
  assert.equal(withTraceHeaders(untouched, { traceparent: "bad" }), untouched);
});
