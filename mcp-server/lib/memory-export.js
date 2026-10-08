// Point-in-time memory export (`memory-export/1.0.0`, Fortemi #1157).
//
// The tool forwards a validated request to `POST /api/v1/memory/export`. The
// API owns snapshot consistency, high-water marks, and hashing; this module
// only shapes the request so malformed cursors fail before a network call.

export const MEMORY_EXPORT_PATH = "/api/v1/memory/export";

export const MEMORY_EXPORT_ENTITY_TYPES = Object.freeze([
  "collection",
  "note",
  "note_original",
  "note_revised_current",
  "note_tag",
  "link",
]);

const MARK_PATTERN = /^[0-9]{1,20}$/;

function invalid(message) {
  const error = new Error(`invalid_memory_export_request: ${message}`);
  error.code = "invalid_memory_export_request";
  return error;
}

/** Build the REST request body from MCP tool arguments. */
export function buildMemoryExportRequest(args = {}) {
  const mode = args.mode ?? (args.since === undefined ? "full" : "incremental");
  if (mode !== "full" && mode !== "incremental") {
    throw invalid("mode must be 'full' or 'incremental'");
  }
  const body = { mode };
  if (args.since !== undefined && args.since !== null) {
    const since = String(args.since);
    if (!MARK_PATTERN.test(since)) {
      throw invalid("since must be a decimal high_water_mark from a previous export");
    }
    body.since = since;
  }
  if (mode === "incremental" && body.since === undefined) {
    throw invalid("incremental export requires since");
  }
  if (mode === "full" && body.since !== undefined) {
    throw invalid("full export does not accept since");
  }
  if (args.entity_types !== undefined) {
    if (!Array.isArray(args.entity_types) || args.entity_types.length === 0) {
      throw invalid("entity_types must be a non-empty array");
    }
    const unknown = args.entity_types.filter((type) => !MEMORY_EXPORT_ENTITY_TYPES.includes(type));
    if (unknown.length > 0) {
      throw invalid(`unknown entity type ${unknown[0]}`);
    }
    body.entity_types = [...args.entity_types];
  }
  if (args.fields !== undefined) {
    if (args.fields === null || typeof args.fields !== "object" || Array.isArray(args.fields)) {
      throw invalid("fields must be an object of entity type to field names");
    }
    body.fields = args.fields;
  }
  return body;
}

/** Run the export through the caller's API client (keeps auth and memory headers). */
export async function exportMemorySnapshot(apiRequest, args) {
  return apiRequest("POST", MEMORY_EXPORT_PATH, buildMemoryExportRequest(args));
}
