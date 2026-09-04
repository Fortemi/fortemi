import crypto from "node:crypto";

export const DATASET_EXECUTION_CONTRACTS = Object.freeze({
  capability: "fortemi.dataset-execution-capabilities/v1",
  plan: "fortemi.dataset-ingest/v1",
  checkpoint: "fortemi.dataset-ingest/v1",
  lineage: "fortemi.dataset-lineage/v1",
  materialization: "fortemi.dataset-materialization-profile/v1",
  receipt: "fortemi.dataset-run-receipt/v1",
  resourceEnvelope: "fortemi.dataset-resource-envelope/v1",
});

export const DATASET_EXECUTION_SCHEMA_VERSIONS = Object.freeze({
  capability: "1.0.0",
  plan: "1.0.0",
  checkpoint: "1.0.0",
  lineage: "1.0.0",
  materialization: "1.0.0",
  receipt: "1.0.0",
  resourceEnvelope: "1.0.0",
});

export const DATASET_RESOURCE_POLICY = Object.freeze({
  contract: DATASET_EXECUTION_CONTRACTS.resourceEnvelope,
  schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.resourceEnvelope,
  maxRecords: 500,
  maxInputBytes: 16 * 1024 * 1024,
  maxRecordBytes: 4 * 1024 * 1024,
  maxDurationMs: 120_000,
  maxConcurrency: 1,
  maxTraversalDepth: 8,
  maxResults: 1_000,
  allowOutboundNetwork: false,
});

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const DIGEST = /^sha256:[a-f0-9]{64}$/;
const SEMVER = /^1\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;
const SUPPORTED_CAPABILITIES = new Map([
  ["ingest.full", { version: "1.0.0", status: "experimental" }],
  ["ingest.snapshot", { version: "1.0.0", status: "experimental" }],
  ["ingest.incremental", { version: "1.0.0", status: "experimental" }],
  ["schema.inspect", { version: "1.0.0", status: "supported" }],
  ["identity.stable-revision", { version: "1.0.0", status: "supported" }],
  ["identity.record", { version: "1.0.0", status: "supported" }],
  ["mutation.upsert", { version: "1.0.0", status: "supported" }],
  ["checkpoint.read", { version: "1.0.0", status: "experimental" }],
  ["checkpoint.write", { version: "1.0.0", status: "experimental" }],
  ["execution.cancel", { version: "1.0.0", status: "experimental" }],
  ["rejection.record", { version: "1.0.0", status: "supported" }],
  ["index.lexical", { version: "1.0.0", status: "experimental" }],
  ["lineage.dataset", { version: "1.0.0", status: "experimental" }],
  ["lineage.record", { version: "1.0.0", status: "experimental" }],
  ["transaction.atomic-batch", { version: "1.0.0", status: "supported" }],
  ["pagination.cursor", { version: "1.0.0", status: "supported" }],
  ["ordering.deterministic", { version: "1.0.0", status: "supported" }],
]);

const KNOWN_CAPABILITIES = new Set([
  "ingest.full", "ingest.snapshot", "ingest.incremental", "ingest.stream",
  "schema.inspect", "identity.stable-revision", "identity.record",
  "mutation.upsert", "mutation.tombstone", "mutation.reconcile",
  "checkpoint.read", "checkpoint.write", "execution.cancel", "rejection.record",
  "index.lexical", "index.chunk", "index.vector", "index.hybrid", "index.rerank",
  "index.graph", "index.community", "lineage.dataset", "lineage.record",
  "lineage.field", "lineage.relationship-evidence", "transaction.atomic-batch",
  "privacy.pre-materialization-filter", "pagination.cursor", "ordering.deterministic",
]);

export class DatasetExecutionError extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "DatasetExecutionError";
    this.code = code;
    this.details = details;
  }
}

function clone(value) {
  return structuredClone(value);
}

function finiteJson(value, path = "$") {
  if (typeof value === "number" && !Number.isFinite(value)) {
    throw new DatasetExecutionError("CANONICAL_VALUE_INVALID", `Non-finite number at ${path}`);
  }
  if (value === undefined || typeof value === "function" || typeof value === "symbol" || typeof value === "bigint") {
    throw new DatasetExecutionError("CANONICAL_VALUE_INVALID", `Unsupported JSON value at ${path}`);
  }
  if (Array.isArray(value)) value.forEach((item, index) => finiteJson(item, `${path}/${index}`));
  else if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) finiteJson(item, `${path}/${key}`);
  }
}

/** Deterministic UTF-8 JSON used by both fixtures and live receipt digests. */
export function canonicalJson(value) {
  finiteJson(value);
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.entries(value)
    .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
    .map(([key, item]) => `${JSON.stringify(key)}:${canonicalJson(item)}`)
    .join(",")}}`;
}

export function sha256Digest(value) {
  const bytes = typeof value === "string" ? value : canonicalJson(value);
  return `sha256:${crypto.createHash("sha256").update(bytes, "utf8").digest("hex")}`;
}

function compareSemver(left, right) {
  const parse = value => {
    const match = /^(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(value || "");
    return match ? match.slice(1, 4).map(Number) : null;
  };
  const a = parse(left);
  const b = parse(right);
  if (!a || !b) return null;
  for (let index = 0; index < 3; index++) {
    if (a[index] !== b[index]) return a[index] > b[index] ? 1 : -1;
  }
  return 0;
}

function descriptorLimit(id) {
  if (id.startsWith("ingest.") || id === "mutation.upsert" || id === "transaction.atomic-batch") {
    return {
      maxInputBytes: DATASET_RESOURCE_POLICY.maxInputBytes,
      maxRecordBytes: DATASET_RESOURCE_POLICY.maxRecordBytes,
      maxBatchRecords: DATASET_RESOURCE_POLICY.maxRecords,
      maxConcurrency: DATASET_RESOURCE_POLICY.maxConcurrency,
    };
  }
  if (id.startsWith("index.") || id.startsWith("lineage.")) {
    return {
      maxPageSize: DATASET_RESOURCE_POLICY.maxResults,
      maxTraversalDepth: DATASET_RESOURCE_POLICY.maxTraversalDepth,
      maxConcurrency: DATASET_RESOURCE_POLICY.maxConcurrency,
    };
  }
  return undefined;
}

export function buildDatasetExecutionDescriptor(runtimeVersion = "0.0.0") {
  return {
    contract: DATASET_EXECUTION_CONTRACTS.capability,
    schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.capability,
    runtime: {
      id: "fortemi-server-mcp",
      version: runtimeVersion,
      plane: "live-remote-persistence",
      dataClass: "remote-persistence",
      maturity: "alpha",
    },
    guarantees: {
      transaction: "atomic-batch",
      isolation: "serializable",
      durability: "wal",
      availability: "single-host",
      ordering: "backend-cursor",
    },
    capabilities: [...SUPPORTED_CAPABILITIES].map(([id, declaration]) => ({
      id,
      ...declaration,
      ...(descriptorLimit(id) ? { limits: descriptorLimit(id) } : {}),
      evidence: ["fortemi-server-dataset-execution-v1"],
    })),
    evidence: [{
      id: "fortemi-server-dataset-execution-v1",
      kind: "fixture",
      uri: "fortemi://contracts/dataset-execution/1.0.0/conformance",
    }],
  };
}

function diagnostic(code, message, extra = {}) {
  return { code, message, ...extra };
}

function checkVersions(input, diagnostics) {
  const requested = input.contractVersions || {};
  for (const [name, contract] of Object.entries(DATASET_EXECUTION_CONTRACTS)) {
    if (requested[name] !== undefined && requested[name] !== contract) {
      diagnostics.push(diagnostic("CONTRACT_MAJOR_UNSUPPORTED", `Unsupported ${name} contract`, { path: `/contractVersions/${name}` }));
    }
  }
  const versions = input.schemaVersions || {};
  for (const [name, version] of Object.entries(versions)) {
    if (!(name in DATASET_EXECUTION_SCHEMA_VERSIONS) || !SEMVER.test(version) || !version.startsWith("1.")) {
      diagnostics.push(diagnostic("SCHEMA_VERSION_UNSUPPORTED", `Unsupported ${name} schema version`, { path: `/schemaVersions/${name}` }));
    }
  }
}

function checkRequirement(requirement) {
  if (!requirement || !KNOWN_CAPABILITIES.has(requirement.id)) {
    return { ok: false, reason: "unsupported", diagnostic: diagnostic("REQUIRED_CAPABILITY_MISSING", "Unknown or unsupported capability", { capability: requirement?.id }) };
  }
  const declaration = SUPPORTED_CAPABILITIES.get(requirement.id);
  if (!declaration) {
    return { ok: false, reason: "unsupported", diagnostic: diagnostic("REQUIRED_CAPABILITY_MISSING", `Capability ${requirement.id} is unsupported`, { capability: requirement.id }) };
  }
  if (requirement.minimumVersion && compareSemver(declaration.version, requirement.minimumVersion) < 0) {
    return { ok: false, reason: "version-insufficient", diagnostic: diagnostic("CAPABILITY_VERSION_INSUFFICIENT", `Capability ${requirement.id} does not satisfy ${requirement.minimumVersion}`, { capability: requirement.id }) };
  }
  const limits = descriptorLimit(requirement.id) || {};
  for (const [key, value] of Object.entries(requirement.minimumLimits || {})) {
    if (!Number.isSafeInteger(value) || value < 0 || !Number.isSafeInteger(limits[key]) || limits[key] < value) {
      return { ok: false, reason: "limit-insufficient", diagnostic: diagnostic("CAPABILITY_LIMIT_INSUFFICIENT", `${key} does not satisfy ${value}`, { capability: requirement.id, path: `/minimumLimits/${key}` }) };
    }
  }
  return { ok: true };
}

export function negotiateDatasetExecution(input, runtimeVersion = "0.0.0") {
  const descriptor = buildDatasetExecutionDescriptor(runtimeVersion);
  const diagnostics = [];
  const selected = [];
  const degradations = [];
  checkVersions(input, diagnostics);
  const request = input.negotiation || {};
  if (request.contract !== undefined && request.contract !== DATASET_EXECUTION_CONTRACTS.capability) {
    diagnostics.push(diagnostic("CONTRACT_MAJOR_UNSUPPORTED", "Unsupported negotiation contract", { path: "/negotiation/contract" }));
  }
  for (const requirement of request.required || []) {
    const result = checkRequirement(requirement);
    if (result.ok) selected.push(requirement.id);
    else diagnostics.push(result.diagnostic);
  }
  for (const requirement of request.optional || []) {
    const result = checkRequirement(requirement);
    if (result.ok) {
      selected.push(requirement.id);
      continue;
    }
    const fallback = (requirement.fallback || []).find(id => SUPPORTED_CAPABILITIES.has(id));
    if (fallback) selected.push(fallback);
    degradations.push({
      requested: requirement.id,
      ...(fallback ? { selected: fallback } : {}),
      reason: result.reason,
      changedGuarantees: [fallback ? `${requirement.id} replaced by ${fallback}` : `${requirement.id} omitted`],
    });
  }
  return {
    contract: DATASET_EXECUTION_CONTRACTS.capability,
    schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.capability,
    accepted: diagnostics.length === 0,
    runtime: descriptor.runtime,
    selected: [...new Set(selected)],
    degradations,
    diagnostics,
    decisionDigest: sha256Digest({ selected: [...new Set(selected)], degradations, diagnostics }),
  };
}

function validateResourceEnvelope(envelope, records, diagnostics) {
  if (!envelope || envelope.contract !== DATASET_EXECUTION_CONTRACTS.resourceEnvelope || !SEMVER.test(envelope.schemaVersion || "")) {
    diagnostics.push(diagnostic("RESOURCE_ENVELOPE_UNSUPPORTED", "A v1 resource envelope is required", { path: "/resourceEnvelope" }));
    return;
  }
  const bounded = ["maxRecords", "maxInputBytes", "maxRecordBytes", "maxDurationMs", "maxConcurrency", "maxTraversalDepth", "maxResults"];
  for (const key of bounded) {
    const value = envelope[key];
    if (!Number.isSafeInteger(value) || value < 0 || value > DATASET_RESOURCE_POLICY[key]) {
      diagnostics.push(diagnostic("RESOURCE_LIMIT_EXCEEDED", `${key} exceeds server policy`, { path: `/resourceEnvelope/${key}`, limit: DATASET_RESOURCE_POLICY[key] }));
    }
  }
  if (Number.isSafeInteger(envelope.maxDurationMs) && envelope.maxDurationMs < 1) {
    diagnostics.push(diagnostic("RESOURCE_LIMIT_EXCEEDED", "maxDurationMs must allow at least one millisecond", { path: "/resourceEnvelope/maxDurationMs", limit: DATASET_RESOURCE_POLICY.maxDurationMs }));
  }
  if (envelope.maxConcurrency !== 1) {
    diagnostics.push(diagnostic("RESOURCE_LIMIT_EXCEEDED", "maxConcurrency must be exactly one for the v1 live projection", { path: "/resourceEnvelope/maxConcurrency", limit: 1 }));
  }
  if (envelope.allowOutboundNetwork !== false) {
    diagnostics.push(diagnostic("OUTBOUND_NETWORK_UNSUPPORTED", "Dataset execution does not permit outbound network access", { path: "/resourceEnvelope/allowOutboundNetwork" }));
  }
  if (records.length > envelope.maxRecords) diagnostics.push(diagnostic("RECORD_LIMIT_EXCEEDED", "Record count exceeds requested resource envelope"));
  const encoded = records.map(record => Buffer.byteLength(canonicalJson(record), "utf8"));
  if (encoded.some(bytes => bytes > envelope.maxRecordBytes)) diagnostics.push(diagnostic("RECORD_BYTES_EXCEEDED", "A record exceeds maxRecordBytes"));
  if (encoded.reduce((sum, value) => sum + value, 0) > envelope.maxInputBytes) diagnostics.push(diagnostic("INPUT_BYTES_EXCEEDED", "Records exceed maxInputBytes"));
}

function validatePlanAndBatch(plan, batch, diagnostics) {
  if (!plan || plan.contract !== DATASET_EXECUTION_CONTRACTS.plan || !SEMVER.test(plan.schemaVersion || "")) {
    diagnostics.push(diagnostic("PLAN_SCHEMA_UNSUPPORTED", "A fortemi.dataset-ingest/v1 plan is required", { path: "/plan" }));
    return;
  }
  if (!batch || batch.contract !== DATASET_EXECUTION_CONTRACTS.plan || !SEMVER.test(batch.schemaVersion || "")) {
    diagnostics.push(diagnostic("BATCH_SCHEMA_UNSUPPORTED", "A fortemi.dataset-ingest/v1 batch is required", { path: "/batch" }));
    return;
  }
  for (const [key, value] of Object.entries({ planDigest: plan.planDigest, configurationDigest: plan.configurationDigest, transformationDigest: plan.transformationDigest })) {
    if (!DIGEST.test(value || "")) diagnostics.push(diagnostic("DIGEST_INVALID", `${key} must be a sha256 digest`, { path: `/plan/${key}` }));
  }
  if (typeof plan.planId !== "string" || plan.planId.length === 0) diagnostics.push(diagnostic("PLAN_ID_INVALID", "planId is required", { path: "/plan/planId" }));
  if (typeof plan.sourceRevision !== "string" || plan.sourceRevision.length === 0) diagnostics.push(diagnostic("SOURCE_REVISION_INVALID", "sourceRevision is required", { path: "/plan/sourceRevision" }));
  if (!plan.destination || !UUID.test(plan.destination.dataset || "")) {
    diagnostics.push(diagnostic("NAMESPACE_UUID_REQUIRED", "plan.destination.dataset must be a non-nil UUID namespace", { path: "/plan/destination/dataset" }));
  }
  if (!["full", "snapshot", "incremental"].includes(plan.mode)) diagnostics.push(diagnostic("INGEST_MODE_UNSUPPORTED", "Unsupported ingest mode", { path: "/plan/mode" }));
  if (plan.rejectionPolicy?.mode !== "fail-fast" || plan.rejectionPolicy?.maxRejectedRecords !== 0) {
    diagnostics.push(diagnostic("REJECTION_POLICY_UNSUPPORTED", "The v1 live projection supports fail-fast atomic batches only", { path: "/plan/rejectionPolicy" }));
  }
  if (plan.reconciliation?.enabled !== false || plan.reconciliation?.maxTombstones !== 0) {
    diagnostics.push(diagnostic("RECONCILIATION_UNSUPPORTED", "The v1 live projection does not perform reconciliation or tombstones", { path: "/plan/reconciliation" }));
  }
  if (!Number.isSafeInteger(batch.sequence) || batch.sequence < 1) diagnostics.push(diagnostic("BATCH_SEQUENCE_INVALID", "Batch sequence must be a positive integer", { path: "/batch/sequence" }));
  if (!Array.isArray(batch.mutations) || batch.mutations.length === 0) diagnostics.push(diagnostic("BATCH_EMPTY", "At least one mutation is required", { path: "/batch/mutations" }));
  if (batch.idempotencyKey !== undefined && (typeof batch.idempotencyKey !== "string" || batch.idempotencyKey.length < 1 || batch.idempotencyKey.length > 200)) {
    diagnostics.push(diagnostic("IDEMPOTENCY_KEY_INVALID", "idempotencyKey must contain 1 to 200 characters", { path: "/batch/idempotencyKey" }));
  }
  if (!batch.checkpointAfter || batch.checkpointAfter.contract !== DATASET_EXECUTION_CONTRACTS.checkpoint || batch.checkpointAfter.sequence !== batch.sequence) {
    diagnostics.push(diagnostic("CHECKPOINT_INVALID", "checkpointAfter must be a matching v1 checkpoint", { path: "/batch/checkpointAfter" }));
  }
  const scope = canonicalJson(plan.destination || {});
  for (const [name, checkpoint] of [["checkpointBefore", batch.checkpointBefore], ["checkpointAfter", batch.checkpointAfter]]) {
    if (checkpoint && canonicalJson(checkpoint.scope || {}) !== scope) diagnostics.push(diagnostic("CHECKPOINT_SCOPE_MISMATCH", `${name} belongs to a different destination`, { path: `/batch/${name}/scope` }));
  }
  const logicalIds = new Set();
  for (const [index, mutation] of (batch.mutations || []).entries()) {
    if (mutation.operation !== "upsert") diagnostics.push(diagnostic("MUTATION_UNSUPPORTED", "Only upsert mutations are supported", { path: `/batch/mutations/${index}/operation` }));
    if (!mutation.logicalId || !mutation.revision || !DIGEST.test(mutation.digest || "")) diagnostics.push(diagnostic("MUTATION_INVALID", "Mutation identity, revision, and digest are required", { path: `/batch/mutations/${index}` }));
    const content = typeof mutation.value === "string" ? mutation.value : mutation.value?.content;
    if (typeof content !== "string" || content.length === 0) diagnostics.push(diagnostic("MUTATION_VALUE_INVALID", "Upsert value must be a non-empty string or contain non-empty content", { path: `/batch/mutations/${index}/value` }));
    else if (sha256Digest(content) !== mutation.digest) diagnostics.push(diagnostic("CONTENT_DIGEST_MISMATCH", "Mutation digest does not match content", { path: `/batch/mutations/${index}/digest` }));
    if (logicalIds.has(mutation.logicalId)) diagnostics.push(diagnostic("DUPLICATE_LOGICAL_ID", "A batch may contain each logicalId only once", { path: `/batch/mutations/${index}/logicalId` }));
    logicalIds.add(mutation.logicalId);
  }
}

export function previewDatasetExecution(input, runtimeVersion = "0.0.0") {
  const records = input.batch?.mutations || [];
  const negotiation = negotiateDatasetExecution(input, runtimeVersion);
  const diagnostics = [...negotiation.diagnostics];
  validateResourceEnvelope(input.resourceEnvelope, records, diagnostics);
  validatePlanAndBatch(input.plan, input.batch, diagnostics);
  if (input.runId !== undefined && !UUID.test(input.runId)) diagnostics.push(diagnostic("RUN_UUID_INVALID", "runId must be a UUID", { path: "/runId" }));
  if (!DIGEST.test(input.inputSchemaDigest || "") || !DIGEST.test(input.outputSchemaDigest || "")) {
    diagnostics.push(diagnostic("SCHEMA_DIGEST_INVALID", "inputSchemaDigest and outputSchemaDigest are required sha256 digests"));
  }
  const accepted = diagnostics.length === 0;
  const requestDigest = accepted ? sha256Digest({
    contractVersions: input.contractVersions || {},
    schemaVersions: input.schemaVersions || {},
    negotiation: input.negotiation || {},
    plan: input.plan,
    batch: input.batch,
    resourceEnvelope: input.resourceEnvelope,
    profiles: input.profiles || {},
  }) : undefined;
  return {
    contract: DATASET_EXECUTION_CONTRACTS.plan,
    schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.plan,
    mode: "preview",
    accepted,
    negotiation: { ...negotiation, diagnostics: [] },
    diagnostics,
    ...(requestDigest ? { requestDigest, idempotencyKey: input.batch.idempotencyKey || requestDigest } : {}),
    counts: {
      attempted: records.length,
      upserts: records.filter(item => item.operation === "upsert").length,
      tombstones: records.filter(item => item.operation === "tombstone").length,
    },
    noSideEffects: true,
  };
}

function sourceRequest(input, requestDigest) {
  const plan = input.plan;
  const batch = input.batch;
  const namespaceId = plan.destination.dataset;
  return {
    source_namespace: `dataset:${namespaceId}`,
    source_id: sha256Digest({ sourceBinding: plan.destination.sourceBinding, stream: plan.destination.stream, partition: plan.destination.partition || "" }),
    source_schema_version: plan.schemaVersion,
    import_run_id: input.runId,
    batch_id: batch.idempotencyKey || requestDigest,
    workspace_id: sha256Digest({ tenant: plan.destination.tenant, dataset: namespaceId }),
    checkpoint: {
      contract: batch.checkpointAfter.contract,
      schemaVersion: batch.checkpointAfter.schemaVersion,
      opaque: batch.checkpointAfter.opaque,
      sequence: batch.checkpointAfter.sequence,
      planDigest: plan.planDigest,
    },
    dry_run: false,
    policy: "version",
    items: batch.mutations.map(mutation => {
      const value = typeof mutation.value === "string" ? { content: mutation.value } : mutation.value;
      return {
        external_id: mutation.logicalId,
        content: value.content,
        content_digest: mutation.digest,
        ...(value.title ? { title: value.title } : {}),
        format: value.format || "markdown",
        metadata: {
          ...(value.metadata && typeof value.metadata === "object" ? value.metadata : {}),
          dataset_execution: {
            namespace_digest: sha256Digest(namespaceId),
            logical_id_digest: sha256Digest(mutation.logicalId),
            revision: mutation.revision,
            plan_digest: plan.planDigest,
          },
        },
      };
    }),
  };
}

function receiptPayload(input, preview, response, state, verification, diagnostics = []) {
  const items = response?.items || [];
  const effects = input.batch.mutations.map((mutation, index) => ({
    operation: mutation.operation,
    logicalIdDigest: items[index]?.external_id_hash || sha256Digest(mutation.logicalId),
    revision: mutation.revision,
    digest: mutation.digest,
    outcome: ["inserted", "versioned", "replaced", "unchanged"].includes(items[index]?.outcome)
      ? "committed"
      : items[index]?.outcome || "unverifiable",
  }));
  const committed = effects.filter(item => item.outcome === "committed").length;
  const rejected = effects.filter(item => ["conflict", "rejected", "unverifiable"].includes(item.outcome)).length;
  const outputDigest = sha256Digest(effects);
  const payload = {
    contract: DATASET_EXECUTION_CONTRACTS.receipt,
    schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.receipt,
    runId: input.runId,
    namespaceId: input.plan.destination.dataset,
    idempotencyKey: preview.idempotencyKey,
    requestDigest: preview.requestDigest,
    bindings: {
      planId: input.plan.planId,
      sourceRevision: input.plan.sourceRevision,
      mode: input.plan.mode,
      destinationDigest: sha256Digest(input.plan.destination),
      planDigest: input.plan.planDigest,
      configurationDigest: input.plan.configurationDigest,
      transformationDigest: input.plan.transformationDigest,
      inputSchemaDigest: input.inputSchemaDigest,
      outputSchemaDigest: input.outputSchemaDigest,
      inputDigest: sha256Digest(input.batch.mutations),
      outputDigest,
      negotiationDigest: preview.negotiation.decisionDigest,
      resourceEnvelopeDigest: sha256Digest(input.resourceEnvelope),
    },
    contracts: clone(DATASET_EXECUTION_CONTRACTS),
    schemas: clone(DATASET_EXECUTION_SCHEMA_VERSIONS),
    capabilityDecision: {
      accepted: preview.negotiation.accepted,
      runtime: clone(preview.negotiation.runtime),
      selected: preview.negotiation.selected,
      degradations: preview.negotiation.degradations,
    },
    profiles: clone(input.profiles || {}),
    checkpoint: {
      ...(input.batch.checkpointBefore ? { before: clone(input.batch.checkpointBefore) } : {}),
      after: clone(input.batch.checkpointAfter),
    },
    resourceEnvelope: clone(input.resourceEnvelope),
    counts: { attempted: effects.length, committed, rejected },
    effects,
    state,
    verification,
    diagnostics,
    redaction: {
      sourceContentIncluded: false,
      logicalIdentifiersIncluded: false,
      connectionDetailsIncluded: false,
    },
  };
  return { ...payload, receiptDigest: sha256Digest(payload) };
}

export function verifyDatasetRunReceipt(receipt) {
  const errors = [];
  if (!receipt || receipt.contract !== DATASET_EXECUTION_CONTRACTS.receipt) errors.push("RECEIPT_CONTRACT_UNSUPPORTED");
  if (!receipt || !SEMVER.test(receipt.schemaVersion || "")) errors.push("RECEIPT_SCHEMA_UNSUPPORTED");
  if (!receipt || !DIGEST.test(receipt.receiptDigest || "")) errors.push("RECEIPT_DIGEST_INVALID");
  if (receipt) {
    const { receiptDigest, ...payload } = receipt;
    if (DIGEST.test(receiptDigest || "") && sha256Digest(payload) !== receiptDigest) errors.push("RECEIPT_DIGEST_MISMATCH");
    if ((receipt.counts?.committed || 0) + (receipt.counts?.rejected || 0) !== receipt.counts?.attempted) errors.push("RECEIPT_COUNTS_INCONSISTENT");
    if (receipt.effects?.length !== receipt.counts?.attempted) errors.push("RECEIPT_EFFECTS_INCONSISTENT");
    if (receipt.redaction?.sourceContentIncluded !== false || receipt.redaction?.connectionDetailsIncluded !== false) errors.push("RECEIPT_REDACTION_INVALID");
    if (["committed", "degraded"].includes(receipt.state) && receipt.verification !== "verified") errors.push("RECEIPT_STATE_INCONSISTENT");
    if (receipt.state === "ambiguous" && receipt.verification !== "unverifiable") errors.push("RECEIPT_STATE_INCONSISTENT");
    if (receipt.state === "failed" && receipt.verification !== "failed") errors.push("RECEIPT_STATE_INCONSISTENT");
    if (!receipt.capabilityDecision?.runtime?.id || !receipt.capabilityDecision?.runtime?.version) errors.push("RECEIPT_RUNTIME_UNBOUND");
  }
  return {
    contract: DATASET_EXECUTION_CONTRACTS.receipt,
    schemaVersion: DATASET_EXECUTION_SCHEMA_VERSIONS.receipt,
    valid: errors.length === 0,
    errors,
    ...(receipt?.receiptDigest ? { receiptDigest: receipt.receiptDigest } : {}),
  };
}

function publicRun(run) {
  return {
    runId: run.runId,
    state: run.state,
    verification: run.verification,
    attempt: run.attempt,
    ...(run.preview?.idempotencyKey ? { idempotencyKey: run.preview.idempotencyKey } : {}),
    ...(run.preview?.requestDigest ? { requestDigest: run.preview.requestDigest } : {}),
    ...(run.receipt ? { receipt: clone(run.receipt) } : {}),
    ...(run.diagnostics?.length ? { diagnostics: clone(run.diagnostics) } : {}),
    ...(run.archive ? { archive: clone(run.archive) } : {}),
  };
}

export function createDatasetExecutionController({ apiRequest, runtimeVersion = "0.0.0", now = () => Date.now() }) {
  if (typeof apiRequest !== "function") throw new TypeError("apiRequest is required");
  const runs = new Map();
  let activeRuns = 0;

  async function execute(input, retry = false) {
    const preview = previewDatasetExecution(input, runtimeVersion);
    if (!preview.accepted) throw new DatasetExecutionError("DATASET_PLAN_UNSUPPORTED", "Dataset plan was rejected before execution", { preview });
    if (!UUID.test(input.runId || "")) throw new DatasetExecutionError("RUN_UUID_INVALID", "execute requires a caller-supplied UUID runId");
    if (!DIGEST.test(input.inputSchemaDigest || "") || !DIGEST.test(input.outputSchemaDigest || "")) {
      throw new DatasetExecutionError("SCHEMA_DIGEST_INVALID", "inputSchemaDigest and outputSchemaDigest are required sha256 digests");
    }
    const prior = runs.get(input.runId);
    if (prior && !retry) {
      if (prior.preview?.requestDigest !== preview.requestDigest) throw new DatasetExecutionError("IDEMPOTENCY_CONFLICT", "runId was reused with different canonical content");
      if (prior.receipt) return publicRun(prior);
      if (prior.state === "running") throw new DatasetExecutionError("RUN_ALREADY_ACTIVE", "Run is already active");
    }
    if (prior && retry && prior.preview?.requestDigest !== preview.requestDigest) throw new DatasetExecutionError("IDEMPOTENCY_CONFLICT", "Retry content differs from the original run");
    if (activeRuns >= DATASET_RESOURCE_POLICY.maxConcurrency) {
      throw new DatasetExecutionError("CONCURRENCY_LIMIT_EXCEEDED", "The dataset execution concurrency limit is active", { limit: DATASET_RESOURCE_POLICY.maxConcurrency });
    }
    const abortController = new AbortController();
    const run = {
      runId: input.runId,
      state: "running",
      verification: "pending",
      attempt: (prior?.attempt || 0) + 1,
      preview,
      input: clone(input),
      abortController,
      startedAt: now(),
      diagnostics: [],
    };
    runs.set(input.runId, run);
    activeRuns += 1;
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      abortController.abort(new DOMException("dataset execution timed out", "TimeoutError"));
    }, input.resourceEnvelope.maxDurationMs);
    timeout.unref?.();
    try {
      const response = await apiRequest("POST", "/api/v1/notes/source-upsert", sourceRequest(input, preview.requestDigest), { signal: abortController.signal });
      const rejected = response?.outcome === "rejected" || (response?.counts?.rejected || 0) > 0 || (response?.counts?.conflict || 0) > 0;
      run.state = rejected ? "failed" : (preview.negotiation.degradations.length ? "degraded" : "committed");
      run.verification = rejected ? "failed" : "verified";
      run.response = response;
      run.noteIds = (response?.items || []).map(item => item.note_id).filter(Boolean);
      run.receipt = receiptPayload(input, preview, response, run.state, run.verification,
        (response?.items || []).flatMap(item => item.reason_code ? [diagnostic(item.reason_code.toUpperCase(), "Record rejected by storage contract", { item: item.index })] : []));
      return publicRun(run);
    } catch (error) {
      const cancelled = abortController.signal.aborted;
      run.state = cancelled ? "ambiguous" : "failed";
      run.verification = cancelled ? "unverifiable" : "failed";
      const interruptedCode = timedOut ? "EXECUTION_TIMEOUT" : "COMMIT_OUTCOME_AMBIGUOUS";
      const interruptedMessage = timedOut
        ? "The negotiated duration elapsed; retry exact content to resolve the durable outcome"
        : "Cancellation interrupted transport; retry exact content to resolve the durable outcome";
      run.diagnostics = [diagnostic(cancelled ? interruptedCode : "EXECUTION_FAILED", cancelled ? interruptedMessage : "Execution failed before a verified receipt")];
      run.receipt = receiptPayload(input, preview, null, run.state, run.verification, run.diagnostics);
      if (!cancelled) throw error;
      return publicRun(run);
    } finally {
      clearTimeout(timeout);
      activeRuns -= 1;
    }
  }

  return {
    descriptor: buildDatasetExecutionDescriptor(runtimeVersion),
    async handle(input = {}) {
      switch (input.action) {
        case "capabilities":
          return {
            descriptor: buildDatasetExecutionDescriptor(runtimeVersion),
            contracts: clone(DATASET_EXECUTION_CONTRACTS),
            schemaVersions: clone(DATASET_EXECUTION_SCHEMA_VERSIONS),
            resourcePolicy: clone(DATASET_RESOURCE_POLICY),
            profiles: {
              indexing: { id: "fortemi-note-materialization", version: "1.0.0", maturity: "experimental" },
              retrieval: { id: "fortemi-note-retrieval", version: "1.0.0", maturity: "experimental" },
              lineage: { id: "fortemi-source-identity", version: "1.0.0", maturity: "experimental" },
            },
          };
        case "preview":
          return previewDatasetExecution(input, runtimeVersion);
        case "execute":
          return execute(input);
        case "status": {
          const run = runs.get(input.runId);
          if (!run) throw new DatasetExecutionError("RUN_NOT_FOUND", "No run is known in this MCP process");
          return publicRun(run);
        }
        case "checkpoint": {
          const run = runs.get(input.runId);
          if (!run) throw new DatasetExecutionError("RUN_NOT_FOUND", "No run is known in this MCP process");
          return { runId: run.runId, state: run.state, checkpoint: clone(run.input.batch.checkpointAfter), receiptDigest: run.receipt?.receiptDigest };
        }
        case "cancel": {
          const run = runs.get(input.runId);
          if (!run) throw new DatasetExecutionError("RUN_NOT_FOUND", "No run is known in this MCP process");
          if (run.state === "running") {
            run.abortController.abort();
            return { runId: run.runId, state: "cancellation_requested", terminal: false };
          }
          return { runId: run.runId, state: run.state, terminal: true, changed: false };
        }
        case "resume":
        case "retry": {
          const run = runs.get(input.runId);
          if (!run) throw new DatasetExecutionError("RUN_NOT_FOUND", "No run is available to retry");
          if (run.state === "running") throw new DatasetExecutionError("RUN_ALREADY_ACTIVE", "Run is already active");
          if (["committed", "degraded"].includes(run.state)) return publicRun(run);
          return execute(run.input, true);
        }
        case "verify":
          return verifyDatasetRunReceipt(input.receipt || runs.get(input.runId)?.receipt);
        case "archive": {
          const run = runs.get(input.runId);
          if (!run) throw new DatasetExecutionError("RUN_NOT_FOUND", "No run is available to archive");
          if (!["committed", "degraded", "failed", "ambiguous"].includes(run.state)) throw new DatasetExecutionError("RUN_NOT_TERMINAL", "Only terminal runs can be archived");
          if (run.archive?.complete) return clone(run.archive);
          if (activeRuns >= DATASET_RESOURCE_POLICY.maxConcurrency) {
            throw new DatasetExecutionError("CONCURRENCY_LIMIT_EXCEEDED", "The dataset execution concurrency limit is active", { limit: DATASET_RESOURCE_POLICY.maxConcurrency });
          }
          activeRuns += 1;
          const archiveAbort = new AbortController();
          let timedOut = false;
          const archiveTimeout = setTimeout(() => {
            timedOut = true;
            archiveAbort.abort(new DOMException("dataset archive timed out", "TimeoutError"));
          }, run.input.resourceEnvelope.maxDurationMs);
          archiveTimeout.unref?.();
          let archived = 0;
          let alreadyArchived = 0;
          const unresolved = [];
          const noteIds = run.noteIds || [];
          try {
            for (let index = 0; index < noteIds.length; index += 1) {
              const noteId = noteIds[index];
              if (timedOut) {
                unresolved.push(...noteIds.slice(index).map(sha256Digest));
                break;
              }
              try {
                await apiRequest("DELETE", `/api/v1/notes/${encodeURIComponent(noteId)}`, null, { signal: archiveAbort.signal });
                archived += 1;
              } catch (error) {
                if (timedOut) {
                  unresolved.push(...noteIds.slice(index).map(sha256Digest));
                  break;
                }
                if (/404|not found/i.test(error.message || "")) alreadyArchived += 1;
                else unresolved.push(sha256Digest(noteId));
              }
            }
          } finally {
            clearTimeout(archiveTimeout);
            activeRuns -= 1;
          }
          run.archive = {
            namespaceId: run.input.plan.destination.dataset,
            archived,
            alreadyArchived,
            unresolved,
            complete: unresolved.length === 0,
            reasonCodes: timedOut ? ["ARCHIVE_TIMEOUT"] : [],
          };
          return clone(run.archive);
        }
        default:
          throw new DatasetExecutionError("ACTION_UNSUPPORTED", "Valid actions: capabilities, preview, execute, status, checkpoint, cancel, resume, retry, verify, archive");
      }
    },
  };
}
