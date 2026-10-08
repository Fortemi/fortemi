import { strict as assert } from "node:assert";
import { test } from "node:test";
import tools from "../tools.js";
import { CORE_TOOLS } from "../constants/core-tools.js";
import {
  MEMORY_EXPORT_ENTITY_TYPES,
  MEMORY_EXPORT_PATH,
  buildMemoryExportRequest,
  exportMemorySnapshot,
} from "../lib/memory-export.js";

const tool = tools.find((candidate) => candidate.name === "export_memory_snapshot");

test("tool is read-only, full-mode, and mirrors the contract entity types", () => {
  assert.ok(tool, "export_memory_snapshot must be defined");
  assert.equal(tool.annotations.readOnlyHint, true);
  assert.equal(CORE_TOOLS.has("export_memory_snapshot"), false);
  assert.deepEqual(
    tool.inputSchema.properties.entity_types.items.enum,
    [...MEMORY_EXPORT_ENTITY_TYPES]
  );
  assert.equal(tool.inputSchema.additionalProperties, false);
});

test("mode defaults from since and cursors stay decimal strings", () => {
  assert.deepEqual(buildMemoryExportRequest({}), { mode: "full" });
  assert.deepEqual(buildMemoryExportRequest({ since: "18446744073709551615" }), {
    mode: "incremental",
    since: "18446744073709551615",
  });
  assert.deepEqual(
    buildMemoryExportRequest({ entity_types: ["note"], fields: { note: ["title"] } }),
    { mode: "full", entity_types: ["note"], fields: { note: ["title"] } }
  );
});

test("malformed requests are rejected before any API call", () => {
  for (const args of [
    { mode: "incremental" },
    { mode: "full", since: "12" },
    { since: "1e3" },
    { since: "-1" },
    { mode: "snapshot" },
    { entity_types: [] },
    { entity_types: ["tag"] },
    { fields: ["title"] },
  ]) {
    assert.throws(() => buildMemoryExportRequest(args), /invalid_memory_export_request/);
  }
});

test("export posts to the memory export route through the caller's client", async () => {
  const calls = [];
  const response = { manifest: { high_water_mark: "42" }, records: [] };
  const result = await exportMemorySnapshot(async (...call) => {
    calls.push(call);
    return response;
  }, { since: "7", entity_types: ["note_tag"] });
  assert.equal(result, response);
  assert.deepEqual(calls, [
    ["POST", MEMORY_EXPORT_PATH, { mode: "incremental", since: "7", entity_types: ["note_tag"] }],
  ]);
});
