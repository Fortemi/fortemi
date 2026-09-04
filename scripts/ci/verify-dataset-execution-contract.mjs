#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  createDatasetExecutionController,
  previewDatasetExecution,
  sha256Digest,
  verifyDatasetRunReceipt,
} from "../../mcp-server/lib/dataset-execution.js";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const contractRoot = path.join(repositoryRoot, "contracts/dataset-execution/1.0.0");
const fixtures = path.join(contractRoot, "fixtures");
const load = relative => JSON.parse(fs.readFileSync(path.join(contractRoot, relative), "utf8"));

function fileDigest(relative) {
  return crypto.createHash("sha256").update(fs.readFileSync(path.join(contractRoot, relative))).digest("hex");
}

function applyPatch(document, operations) {
  const output = structuredClone(document);
  for (const operation of operations) {
    const segments = operation.path.slice(1).split("/");
    const leaf = segments.pop();
    let target = output;
    for (const segment of segments) target = target[segment];
    if (operation.op === "add" && leaf === "-") target.push(operation.value);
    else target[leaf] = operation.value;
  }
  return output;
}

const manifest = load("manifest.json");
for (const entry of manifest.files) assert.equal(fileDigest(entry.path), entry.sha256, `${entry.path} digest`);

const requestSchema = load("request.schema.json");
const receiptSchema = load("run-receipt.schema.json");
assert.equal(requestSchema.$schema, "https://json-schema.org/draft/2020-12/schema");
assert.equal(receiptSchema.$schema, "https://json-schema.org/draft/2020-12/schema");
assert.equal(requestSchema.additionalProperties, false);
assert.equal(receiptSchema.additionalProperties, false);

const vector = load("fixtures/canonical-vector.json");
assert.equal(canonicalJson(vector.value), vector.canonicalUtf8);
assert.equal(sha256Digest(vector.value), vector.digest);

const supported = load("fixtures/supported-request.json");
const preview = previewDatasetExecution(supported, "2026.9.2");
assert.equal(preview.accepted, true);
assert.equal(preview.noSideEffects, true);

for (const name of ["unsupported-schema-request.json", "unsupported-capability-request.json", "resource-limit-request.json", "tampered-content-request.json"]) {
  const fixture = load(`fixtures/${name}`);
  const result = previewDatasetExecution(applyPatch(supported, fixture.patch), "2026.9.2");
  assert.equal(result.accepted, fixture.expected.accepted, name);
  for (const code of fixture.expected.reasonCodes) assert.ok(result.diagnostics.some(item => item.code === code), `${name}: ${code}`);
  assert.equal(result.noSideEffects, true, name);
}

const expectedReceipt = load("fixtures/degraded-run-receipt.json");
assert.equal(verifyDatasetRunReceipt(expectedReceipt).valid, true);
const response = {
  contract_version: "1.0.0",
  import_run_id: supported.runId,
  batch_id: "fixture-batch",
  dry_run: false,
  outcome: "committed",
  checkpoint: { sequence: 1 },
  counts: { inserted: 1, unchanged: 0, versioned: 0, replaced: 0, conflict: 0, rejected: 0 },
  items: [{
    index: 0,
    outcome: "inserted",
    note_id: "018fd1a0-0000-7000-8000-000000001130",
    external_id_hash: sha256Digest("record-1"),
    content_digest: supported.batch.mutations[0].digest,
  }],
};
const controller = createDatasetExecutionController({ runtimeVersion: "2026.9.2", apiRequest: async () => response });
const actual = await controller.handle({ action: "execute", ...supported });
assert.deepEqual(actual.receipt, expectedReceipt);

const tampered = structuredClone(expectedReceipt);
tampered.bindings.planDigest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
assert.deepEqual(verifyDatasetRunReceipt(tampered).errors, ["RECEIPT_DIGEST_MISMATCH"]);

assert.ok(fs.existsSync(fixtures));
console.log(`dataset execution contract: ${manifest.files.length} files verified; positive, degradation, schema, capability, resource, tamper, and cross-runtime receipt vectors pass`);
