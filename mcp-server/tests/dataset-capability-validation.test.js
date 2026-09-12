import assert from "node:assert/strict";
import fs from "node:fs";
import { test } from "node:test";
import {
  buildDatasetExecutionDescriptor,
  createDatasetExecutionController,
  DATASET_EXECUTION_CONTRACTS,
  negotiateDatasetExecution,
  previewDatasetExecution,
} from "../lib/dataset-execution.js";
import { compareCapabilityVersions } from "../lib/dataset-capability-validation.js";

const contract = DATASET_EXECUTION_CONTRACTS.capability;
const supported = JSON.parse(fs.readFileSync(new URL(
  "../../contracts/dataset-execution/1.0.0/fixtures/supported-request.json", import.meta.url,
), "utf8"));

const malformed = [
  ["null", null],
  ["array", []],
  ["required object", { contract, required: {} }],
  ["required null", { contract, required: null }],
  ["optional string", { contract, required: [], optional: "bad" }],
  ["null requirement", { contract, required: [null] }],
  ["unknown field", { contract, required: [], unknown: "private-marker" }],
  ["unknown capability", { contract, required: [{ id: "private-marker" }] }],
  ["empty minimum", { contract, required: [{ id: "ingest.full", minimumVersion: "" }] }],
  ["invalid minimum", { contract, required: [{ id: "ingest.full", minimumVersion: "private-marker" }] }],
  ["unsafe core", { contract, required: [{ id: "ingest.full", minimumVersion: "9007199254740992.0.0" }] }],
  ["leading zero", { contract, required: [{ id: "ingest.full", minimumVersion: "01.0.0" }] }],
  ["negative limit", { contract, required: [{ id: "ingest.full", minimumLimits: { maxBatchRecords: -1 } }] }],
  ["unknown limit", { contract, required: [{ id: "ingest.full", minimumLimits: { private: 1 } }] }],
  ["malformed optional", { contract, required: [], optional: [{ id: "index.graph", minimumVersion: "", fallback: ["index.lexical"] }] }],
  ["invalid fallback", { contract, required: [], optional: [{ id: "index.graph", fallback: "index.lexical" }] }],
  ["duplicate fallback", { contract, required: [], optional: [{ id: "index.graph", fallback: ["index.lexical", "index.lexical"] }] }],
];

for (const [name, negotiation] of malformed) {
  test(`malformed negotiation rejects without dispatch: ${name}`, async () => {
    const input = { ...structuredClone(supported), negotiation: structuredClone(negotiation) };
    const before = structuredClone(input);
    const decision = negotiateDatasetExecution(input);
    assert.equal(decision.accepted, false);
    assert.deepEqual(decision.selected, []);
    assert.deepEqual(decision.degradations, []);
    assert.ok(decision.diagnostics.length > 0);
    assert.equal(JSON.stringify(decision).includes("private-marker"), false);
    const preview = previewDatasetExecution(input);
    assert.equal(preview.accepted, false);
    assert.equal(preview.noSideEffects, true);
    assert.equal(preview.requestDigest, undefined);
    let calls = 0;
    const controller = createDatasetExecutionController({ apiRequest: async () => { calls++; } });
    await assert.rejects(controller.handle({ ...input, action: "execute" }), { code: "DATASET_PLAN_UNSUPPORTED" });
    assert.equal(calls, 0);
    assert.deepEqual(input, before);
  });
}

test("omitted negotiation defaults remain read-only; explicit null does not default", () => {
  for (const input of [{}, { negotiation: {} }]) assert.equal(negotiateDatasetExecution(input).accepted, true);
  for (const input of [null, [], "bad", { negotiation: null }, { contractVersions: null }, { schemaVersions: [] }]) {
    assert.equal(negotiateDatasetExecution(input).accepted, false);
  }
});

test("stable offered version satisfies prerelease/build minima but rejects newer versions", () => {
  for (const [minimumVersion, accepted] of [["1.0.0-rc.1", true], ["1.0.0+001", true], ["1.0.1-alpha", false], ["1.1.0", false]]) {
    assert.equal(negotiateDatasetExecution({ negotiation: { contract, required: [{ id: "ingest.full", minimumVersion }] } }).accepted, accepted);
  }
});

test("valid optional mismatch preserves visible fallback", () => {
  const decision = negotiateDatasetExecution({ negotiation: { contract, required: [], optional: [{ id: "index.graph", fallback: ["index.lexical"] }] } });
  assert.equal(decision.accepted, true);
  assert.deepEqual(decision.selected, ["index.lexical"]);
  assert.equal(decision.degradations[0].reason, "unsupported");
});

const authorityRoot = new URL("../../contracts/dataset-execution/capability-validation/1.0.1/", import.meta.url);
const loadAuthority = name => JSON.parse(fs.readFileSync(new URL(name, authorityRoot), "utf8"));
for (const vector of loadAuthority("negotiation-vectors.json").versions) {
  test(`Core authority SemVer vector: ${vector.id}`, () => {
    const comparison = compareCapabilityVersions(vector.offered, vector.minimum);
    assert.equal(comparison !== null && comparison >= 0, vector.accepted);
    if (comparison !== null) {
      assert.equal(compareCapabilityVersions(vector.minimum, vector.offered) + comparison, 0);
      assert.equal(compareCapabilityVersions(vector.offered, vector.offered), 0);
    }
  });
}

// These vectors exercise request negotiation against the server's own descriptor.
// Descriptor-only vectors are not accepted input to this server API.
const requestVectors = new Set(["valid-current", "valid-compatible-revision", "null-request", "unknown-request-field", "invalid-limit", "unavailable-required", "next-contract-major"]);
for (const vector of loadAuthority("wire-vectors.json").cases.filter(item => requestVectors.has(item.id))) {
  test(`Core authority request vector with server descriptor: ${vector.id}`, () => {
    const decision = negotiateDatasetExecution({ negotiation: vector.request });
    assert.equal(decision.accepted, vector.valid && vector.accepted);
  });
}

test("invalid configured runtime versions never advertise a descriptor", () => {
  for (const version of ["", "01.0.0", "1.0.0\n", "9007199254740992.0.0", "1.0.0-01", "1.0.0+", "1.0.0+" + "a".repeat(256)]) {
    assert.throws(() => buildDatasetExecutionDescriptor(version), { code: "DESCRIPTOR_INVALID" });
  }
  assert.equal(buildDatasetExecutionDescriptor("1.0.0-rc.1+test").runtime.version, "1.0.0-rc.1+test");
});

test("MCP exact envelope revisions remain separate from compatible Core descriptor revisions", () => {
  for (const schemaVersions of [{ capability: "1.0.1" }, { capability: "1.0.0+build" }, { unknown: "1.0.0" }]) {
    assert.equal(negotiateDatasetExecution({ schemaVersions }).accepted, false);
  }
  assert.equal(negotiateDatasetExecution({ contractVersions: { unknown: "private-marker" } }).accepted, false);
});
