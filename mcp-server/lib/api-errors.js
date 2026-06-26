const PROBLEM_JSON_RE = /(^|[/+])problem\+json\b/i;
const JSON_RE = /(^|[/+])json\b/i;

function parseJsonObject(text) {
  if (!text || !text.trim()) return null;
  try {
    const parsed = JSON.parse(text);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function appendProblemPart(parts, label, value) {
  if (typeof value === "string" && value.trim()) {
    parts.push(`${label}: ${value.trim()}`);
  }
}

/**
 * Format Fortemi API failures for MCP tool output without copying raw upstream
 * response bodies. RFC 9457 problem fields are the only upstream body fields
 * surfaced because Fortemi's API contract treats them as client-safe.
 */
export function formatFortemiApiError(status, statusText, contentType, responseText) {
  const normalizedContentType = contentType || "";
  const body = JSON_RE.test(normalizedContentType) || PROBLEM_JSON_RE.test(normalizedContentType)
    ? parseJsonObject(responseText)
    : null;

  if (body && typeof body.type === "string" && typeof body.title === "string") {
    const parts = [`API error ${status}: ${body.title.trim() || statusText || "Request failed"}`];
    appendProblemPart(parts, "detail", body.detail);
    appendProblemPart(parts, "type", body.type);
    appendProblemPart(parts, "request_id", body.request_id);
    return parts.join(" | ");
  }

  const reason = statusText && statusText.trim() ? statusText.trim() : "Request failed";
  return `API error ${status}: ${reason}`;
}

export function formatMcpReauthorizationError(status, statusText, contentType, responseText) {
  const formatted = formatFortemiApiError(status, statusText, contentType, responseText);
  return (
    "MCP server requires re-authorization (token expired). " +
    "Please obtain a new access token and reconnect. " +
    formatted
  );
}
