// W3C trace-context pass-through for MCP -> API calls (#1156).
//
// The MCP server does not export telemetry itself. It forwards a validated
// inbound `traceparent` (and `tracestate`) unchanged, so API spans join the
// caller's trace. Sources, in order: the tool call's `params._meta.traceparent`,
// then the HTTP request headers that carried the MCP message. Baggage is never
// forwarded and malformed values are dropped.

const TRACEPARENT_RE = /^00-(?!0{32})[0-9a-f]{32}-(?!0{16})[0-9a-f]{16}-[0-9a-f]{2}$/;
const TRACESTATE_MAX = 512;
// tracestate is printable ASCII list-members; reject anything else outright.
const TRACESTATE_RE = /^[\x20-\x7e]*$/;

export function isValidTraceparent(value) {
  return typeof value === "string" && TRACEPARENT_RE.test(value);
}

function cleanTracestate(value) {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > TRACESTATE_MAX || !TRACESTATE_RE.test(trimmed)) {
    return undefined;
  }
  return trimmed;
}

function headerValue(headers, name) {
  if (!headers) return undefined;
  if (typeof headers.get === "function") return headers.get(name) ?? undefined;
  const value = headers[name] ?? headers[name.toLowerCase()];
  return Array.isArray(value) ? value[0] : value;
}

/** Trace context from an HTTP header bag (Node IncomingHttpHeaders or Headers). */
export function extractTraceContext(headers) {
  const traceparent = headerValue(headers, "traceparent");
  if (!isValidTraceparent(traceparent)) return null;
  const tracestate = cleanTracestate(headerValue(headers, "tracestate"));
  return tracestate ? { traceparent, tracestate } : { traceparent };
}

/** Trace context from MCP request `_meta` (tool-call level), if present. */
export function extractMetaTraceContext(meta) {
  if (!meta || typeof meta !== "object") return null;
  return extractTraceContext({ traceparent: meta.traceparent, tracestate: meta.tracestate });
}

/** Return fetch options whose headers carry the trace context, if any. */
export function withTraceHeaders(options = {}, trace) {
  if (!trace || !isValidTraceparent(trace.traceparent)) return options;
  const extra = { traceparent: trace.traceparent };
  if (trace.tracestate) extra.tracestate = trace.tracestate;
  const headers = options.headers;
  if (headers && typeof headers.set === "function") {
    const copy = new Headers(headers);
    for (const [k, v] of Object.entries(extra)) copy.set(k, v);
    return { ...options, headers: copy };
  }
  return { ...options, headers: { ...(headers || {}), ...extra } };
}
