import assert from "node:assert/strict";
import { describe, test } from "node:test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

import {
  canonicalJson,
  createDatasetExecutionController,
  DATASET_EXECUTION_CONTRACTS,
  DATASET_RESOURCE_POLICY,
  DatasetExecutionError,
  previewDatasetExecution,
  sha256Digest,
  verifyDatasetRunReceipt,
} from "../lib/dataset-execution.js";

const namespaceId = "018fd1a0-0000-7000-8000-000000001128";
const runId = "018fd1a0-0000-7000-8000-000000001129";
const fixtureRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../contracts/dataset-execution/1.0.0/fixtures");
const fixture = name => JSON.parse(fs.readFileSync(path.join(fixtureRoot, name), "utf8"));

function applyFixturePatch(document, operations) {
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

function request(content = "bounded synthetic dataset record") {
  const scope = {
    tenant: "current",
    dataset: namespaceId,
    sourceBinding: "fixture://dataset-execution/v1",
    stream: "records",
  };
  return {
    runId,
    contractVersions: { ...DATASET_EXECUTION_CONTRACTS },
    schemaVersions: {
      capability: "1.0.0",
      plan: "1.0.0",
      checkpoint: "1.0.0",
      lineage: "1.0.0",
      materialization: "1.0.0",
      receipt: "1.0.0",
      resourceEnvelope: "1.0.0",
    },
    negotiation: {
      contract: DATASET_EXECUTION_CONTRACTS.capability,
      required: [
        { id: "ingest.incremental", minimumVersion: "1.0.0" },
        { id: "mutation.upsert" },
        { id: "checkpoint.write" },
        { id: "transaction.atomic-batch", minimumLimits: { maxBatchRecords: 1 } },
        { id: "lineage.record" },
      ],
      optional: [{ id: "index.graph", fallback: ["index.lexical"] }],
    },
    plan: {
      contract: DATASET_EXECUTION_CONTRACTS.plan,
      schemaVersion: "1.0.0",
      planId: "dataset-plan-v1",
      planDigest: sha256Digest("plan-v1"),
      sourceRevision: "fixture-r1",
      configurationDigest: sha256Digest("configuration-v1"),
      transformationDigest: sha256Digest("transformation-v1"),
      destination: scope,
      mode: "incremental",
      rejectionPolicy: { mode: "fail-fast", maxRejectedRecords: 0 },
      reconciliation: { enabled: false, maxTombstones: 0 },
    },
    batch: {
      contract: DATASET_EXECUTION_CONTRACTS.plan,
      schemaVersion: "1.0.0",
      sequence: 1,
      mutations: [{
        operation: "upsert",
        logicalId: "record-1",
        revision: "r1",
        digest: sha256Digest(content),
        value: { content, title: "Synthetic fixture" },
      }],
      checkpointAfter: {
        contract: DATASET_EXECUTION_CONTRACTS.checkpoint,
        schemaVersion: "1.0.0",
        scope,
        opaque: "fixture-page-1",
        sequence: 1,
      },
    },
    resourceEnvelope: {
      ...DATASET_RESOURCE_POLICY,
      maxRecords: 1,
      maxInputBytes: 4096,
      maxRecordBytes: 2048,
      maxDurationMs: 5000,
      maxResults: 10,
    },
    profiles: {
      indexing: { id: "fortemi-note-materialization", version: "1.0.0" },
      retrieval: { id: "fortemi-note-retrieval", version: "1.0.0" },
      lineage: { id: "fortemi-source-identity", version: "1.0.0" },
    },
    inputSchemaDigest: sha256Digest("input-schema-v1"),
    outputSchemaDigest: sha256Digest("output-schema-v1"),
  };
}

function committedResponse(outcome = "committed") {
  return {
    contract_version: "1.0.0",
    import_run_id: runId,
    batch_id: "batch",
    dry_run: false,
    outcome,
    checkpoint: { sequence: 1 },
    counts: { inserted: 1, unchanged: 0, versioned: 0, replaced: 0, conflict: 0, rejected: 0 },
    items: [{
      index: 0,
      outcome: outcome === "duplicate" ? "unchanged" : "inserted",
      note_id: "018fd1a0-0000-7000-8000-000000001130",
      external_id_hash: sha256Digest("record-1"),
      content_digest: sha256Digest("bounded synthetic dataset record"),
    }],
  };
}

describe("versioned dataset execution MCP contract", () => {
  test("MCP initialization and default discovery advertise the versioned capability", { timeout: 10_000 }, async () => {
    const serverRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
    const child = spawn(process.execPath, ["index.js"], {
      cwd: serverRoot,
      env: { ...process.env, MCP_TRANSPORT: "stdio", MCP_TOOL_MODE: "core" },
      stdio: ["pipe", "pipe", "pipe"],
    });
    await new Promise((resolve, reject) => {
      let buffer = "";
      const timer = setTimeout(() => reject(new Error("MCP dataset discovery timed out")), 8_000);
      const fail = error => { clearTimeout(timer); child.kill(); reject(error); };
      child.once("error", fail);
      child.stderr.on("data", data => {
        const text = data.toString();
        if (/ERR_|fatal|exception/i.test(text)) fail(new Error(text));
      });
      child.stdout.on("data", data => {
        buffer += data.toString();
        const lines = buffer.split("\n");
        buffer = lines.pop();
        for (const line of lines) {
          if (!line.trim()) continue;
          const response = JSON.parse(line);
          if (response.id === 1) {
            assert.equal(response.result.capabilities.experimental.fortemiDatasetExecution.descriptor.contract, DATASET_EXECUTION_CONTRACTS.capability);
            child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
            child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} })}\n`);
          }
          if (response.id === 2) {
            assert.ok(response.result.tools.some(tool => tool.name === "manage_dataset_execution"));
            child.stdin.write(`${JSON.stringify({
              jsonrpc: "2.0",
              id: 3,
              method: "tools/call",
              params: { name: "manage_dataset_execution", arguments: { action: "preview", request: {} } },
            })}\n`);
          }
          if (response.id === 3) {
            const preview = JSON.parse(response.result.content[0].text);
            assert.equal(preview.accepted, false);
            assert.ok(preview.diagnostics.some(item => item.code === "PLAN_SCHEMA_UNSUPPORTED"));
            assert.equal(preview.noSideEffects, true);
            clearTimeout(timer);
            child.kill();
            resolve();
          }
        }
      });
      child.stdin.write(`${JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "dataset-contract-test", version: "1.0.0" } },
      })}\n`);
    });
  });

  test("canonical serialization and SHA-256 match the language-neutral vector", () => {
    const vector = fixture("canonical-vector.json");
    assert.equal(canonicalJson(vector.value), vector.canonicalUtf8);
    assert.equal(sha256Digest(vector.value), vector.digest);
  });

  test("preview negotiates a visible fallback without making an API call", async () => {
    let calls = 0;
    const controller = createDatasetExecutionController({ apiRequest: async () => { calls += 1; } });
    const result = await controller.handle({ action: "preview", ...request() });
    assert.equal(result.accepted, true);
    assert.equal(result.noSideEffects, true);
    assert.equal(result.negotiation.degradations[0].requested, "index.graph");
    assert.equal(result.negotiation.degradations[0].selected, "index.lexical");
    assert.equal(calls, 0);
  });

  test("preview fails closed for schema drift, unsupported capabilities, limits, and digest mismatch", () => {
    const input = request();
    input.schemaVersions.receipt = "2.0.0";
    input.negotiation.required.push({ id: "mutation.tombstone" });
    input.resourceEnvelope.maxConcurrency = 2;
    input.batch.mutations[0].digest = sha256Digest("different");
    const result = previewDatasetExecution(input);
    assert.equal(result.accepted, false);
    assert.deepEqual(new Set(result.diagnostics.map(item => item.code)), new Set([
      "SCHEMA_VERSION_UNSUPPORTED",
      "REQUIRED_CAPABILITY_MISSING",
      "RESOURCE_LIMIT_EXCEEDED",
      "CONTENT_DIGEST_MISMATCH",
    ]));
    assert.equal(result.noSideEffects, true);
  });

  test("execute delegates one bounded atomic request and emits a redacted verifiable receipt", async () => {
    const calls = [];
    const controller = createDatasetExecutionController({
      runtimeVersion: "2026.9.2",
      apiRequest: async (...args) => { calls.push(args); return committedResponse(); },
    });
    const input = request();
    const result = await controller.handle({ action: "execute", ...input });
    assert.equal(result.state, "degraded");
    assert.equal(result.verification, "verified");
    assert.equal(calls.length, 1);
    assert.equal(calls[0][0], "POST");
    assert.equal(calls[0][1], "/api/v1/notes/source-upsert");
    assert.equal(calls[0][2].source_namespace, `dataset:${namespaceId}`);
    assert.equal(calls[0][2].items.length, 1);
    assert.equal(result.receipt.effects[0].logicalIdDigest, sha256Digest("record-1"));
    assert.equal(result.receipt.capabilityDecision.runtime.version, "2026.9.2");
    assert.equal(result.receipt.bindings.sourceRevision, "fixture-r1");
    assert.equal(result.receipt.bindings.destinationDigest, sha256Digest(input.plan.destination));
    assert.equal(canonicalJson(result.receipt).includes("bounded synthetic dataset record"), false);
    assert.deepEqual(verifyDatasetRunReceipt(result.receipt), {
      contract: DATASET_EXECUTION_CONTRACTS.receipt,
      schemaVersion: "1.0.0",
      valid: true,
      errors: [],
      receiptDigest: result.receipt.receiptDigest,
    });
    assert.deepEqual(result.receipt, fixture("degraded-run-receipt.json"));
  });

  test("published request fixtures produce their stable negative reason codes", () => {
    const base = fixture("supported-request.json");
    for (const name of ["unsupported-schema-request.json", "unsupported-capability-request.json", "resource-limit-request.json", "tampered-content-request.json"]) {
      const vector = fixture(name);
      const result = previewDatasetExecution(applyFixturePatch(base, vector.patch));
      assert.equal(result.accepted, vector.expected.accepted, name);
      for (const code of vector.expected.reasonCodes) assert.ok(result.diagnostics.some(item => item.code === code), `${name}: ${code}`);
      assert.equal(result.noSideEffects, true, name);
    }
  });

  test("preview rejects unsupported execution policies, empty batches, duplicate identities, and invalid idempotency", () => {
    const input = request();
    input.plan.rejectionPolicy = { mode: "bounded-reject", maxRejectedRecords: 1 };
    input.plan.reconciliation = { enabled: true, maxTombstones: 1 };
    input.batch.idempotencyKey = "x".repeat(201);
    input.batch.mutations.push(structuredClone(input.batch.mutations[0]));
    input.resourceEnvelope.maxRecords = 2;
    const result = previewDatasetExecution(input);
    assert.deepEqual(new Set(result.diagnostics.map(item => item.code)), new Set([
      "REJECTION_POLICY_UNSUPPORTED",
      "RECONCILIATION_UNSUPPORTED",
      "IDEMPOTENCY_KEY_INVALID",
      "DUPLICATE_LOGICAL_ID",
    ]));
    const empty = request();
    empty.batch.mutations = [];
    assert.ok(previewDatasetExecution(empty).diagnostics.some(item => item.code === "BATCH_EMPTY"));
  });

  test("exact replay returns byte-equivalent receipt and conflicting run reuse fails", async () => {
    let calls = 0;
    const controller = createDatasetExecutionController({ apiRequest: async () => { calls += 1; return committedResponse(); } });
    const first = await controller.handle({ action: "execute", ...request() });
    const replay = await controller.handle({ action: "execute", ...request() });
    assert.equal(canonicalJson(first.receipt), canonicalJson(replay.receipt));
    assert.equal(calls, 1);
    await assert.rejects(
      controller.handle({ action: "execute", ...request("different canonical content") }),
      error => error instanceof DatasetExecutionError && error.code === "IDEMPOTENCY_CONFLICT",
    );
  });

  test("durable duplicate replay reconstructs the same receipt after MCP restart", async () => {
    const firstProcess = createDatasetExecutionController({ apiRequest: async () => committedResponse("committed") });
    const restartedProcess = createDatasetExecutionController({ apiRequest: async () => committedResponse("duplicate") });
    const first = await firstProcess.handle({ action: "execute", ...request() });
    const recovered = await restartedProcess.handle({ action: "execute", ...request() });
    assert.equal(canonicalJson(first.receipt), canonicalJson(recovered.receipt));
  });

  test("cancel records ambiguity and exact retry resolves against the durable journal", async () => {
    let attempt = 0;
    const controller = createDatasetExecutionController({
      apiRequest: async (_method, _path, _body, options) => {
        attempt += 1;
        if (attempt > 1) return committedResponse("duplicate");
        return new Promise((_resolve, reject) => {
          options.signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")), { once: true });
        });
      },
    });
    const pending = controller.handle({ action: "execute", ...request() });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal((await controller.handle({ action: "cancel", runId })).state, "cancellation_requested");
    assert.equal((await pending).state, "ambiguous");
    const resolved = await controller.handle({ action: "retry", runId });
    assert.equal(resolved.state, "degraded");
    assert.equal(resolved.verification, "verified");
    assert.equal(attempt, 2);
  });

  test("the negotiated duration aborts unresolved transport with a stable timeout receipt", async () => {
    const input = request();
    input.resourceEnvelope.maxDurationMs = 5;
    const controller = createDatasetExecutionController({
      apiRequest: async (_method, _path, _body, options) => new Promise((_resolve, reject) => {
        options.signal.addEventListener("abort", () => reject(options.signal.reason), { once: true });
      }),
    });
    const result = await controller.handle({ action: "execute", ...input });
    assert.equal(result.state, "ambiguous");
    assert.equal(result.verification, "unverifiable");
    assert.equal(result.receipt.diagnostics[0].code, "EXECUTION_TIMEOUT");
    assert.equal(verifyDatasetRunReceipt(result.receipt).valid, true);
  });

  test("the declared concurrency limit rejects a second active run without API side effects", async () => {
    let calls = 0;
    const controller = createDatasetExecutionController({
      apiRequest: async (_method, _path, _body, options) => {
        calls += 1;
        return new Promise((_resolve, reject) => options.signal.addEventListener("abort", () => reject(options.signal.reason), { once: true }));
      },
    });
    const first = controller.handle({ action: "execute", ...request() });
    await new Promise(resolve => setImmediate(resolve));
    const second = request();
    second.runId = "018fd1a0-0000-7000-8000-000000001131";
    await assert.rejects(
      controller.handle({ action: "execute", ...second }),
      error => error instanceof DatasetExecutionError && error.code === "CONCURRENCY_LIMIT_EXCEEDED",
    );
    assert.equal(calls, 1);
    await controller.handle({ action: "cancel", runId });
    await first;
  });

  test("status/checkpoint are bounded and archive is namespace-scoped and idempotent", async () => {
    const deleted = [];
    const controller = createDatasetExecutionController({
      apiRequest: async (method, path) => {
        if (method === "DELETE") { deleted.push(path); return null; }
        return committedResponse();
      },
    });
    await controller.handle({ action: "execute", ...request() });
    assert.equal((await controller.handle({ action: "status", runId })).state, "degraded");
    assert.equal((await controller.handle({ action: "checkpoint", runId })).checkpoint.sequence, 1);
    const resumed = await controller.handle({ action: "resume", runId });
    assert.equal(resumed.receipt.receiptDigest, (await controller.handle({ action: "status", runId })).receipt.receiptDigest);
    const first = await controller.handle({ action: "archive", runId });
    const second = await controller.handle({ action: "archive", runId });
    assert.deepEqual(first, second);
    assert.deepEqual(deleted, ["/api/v1/notes/018fd1a0-0000-7000-8000-000000001130"]);
    assert.equal(first.namespaceId, namespaceId);
    assert.equal(first.complete, true);
    assert.deepEqual(first.reasonCodes, []);
  });

  test("archive enforces the run duration and shared concurrency bound", async () => {
    const input = request();
    input.resourceEnvelope.maxDurationMs = 50;
    const controller = createDatasetExecutionController({
      apiRequest: async (method, _path, _body, options) => {
        if (method === "POST") return committedResponse();
        return new Promise((_resolve, reject) => options.signal.addEventListener("abort", () => reject(options.signal.reason), { once: true }));
      },
    });
    await controller.handle({ action: "execute", ...input });
    const cleanup = controller.handle({ action: "archive", runId });
    await new Promise(resolve => setImmediate(resolve));
    const second = request();
    second.runId = "018fd1a0-0000-7000-8000-000000001132";
    await assert.rejects(
      controller.handle({ action: "execute", ...second }),
      error => error instanceof DatasetExecutionError && error.code === "CONCURRENCY_LIMIT_EXCEEDED",
    );
    const result = await cleanup;
    assert.equal(result.complete, false);
    assert.deepEqual(result.reasonCodes, ["ARCHIVE_TIMEOUT"]);
    assert.equal(result.unresolved.length, 1);
  });

  test("receipt verification detects any bound-field mutation", async () => {
    const controller = createDatasetExecutionController({ apiRequest: async () => committedResponse() });
    const { receipt } = await controller.handle({ action: "execute", ...request() });
    const tampered = structuredClone(receipt);
    tampered.bindings.planDigest = sha256Digest("tampered");
    assert.deepEqual(verifyDatasetRunReceipt(tampered).errors, ["RECEIPT_DIGEST_MISMATCH"]);
    const inconsistent = structuredClone(receipt);
    inconsistent.verification = "failed";
    delete inconsistent.receiptDigest;
    inconsistent.receiptDigest = sha256Digest(inconsistent);
    assert.deepEqual(verifyDatasetRunReceipt(inconsistent).errors, ["RECEIPT_STATE_INCONSISTENT"]);
  });

  test("backend rejection yields an explicit failed receipt with no committed effects", async () => {
    const response = committedResponse("rejected");
    response.counts = { inserted: 0, unchanged: 0, versioned: 0, replaced: 0, conflict: 0, rejected: 1 };
    response.items[0].outcome = "rejected";
    response.items[0].reason_code = "synthetic_rejection";
    const controller = createDatasetExecutionController({ apiRequest: async () => response });
    const result = await controller.handle({ action: "execute", ...request() });
    assert.equal(result.state, "failed");
    assert.equal(result.verification, "failed");
    assert.deepEqual(result.receipt.counts, { attempted: 1, committed: 0, rejected: 1 });
    assert.equal(result.receipt.diagnostics[0].code, "SYNTHETIC_REJECTION");
  });
});
