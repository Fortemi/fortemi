import { AsyncLocalStorage } from 'node:async_hooks';
import { createHash, randomUUID } from 'node:crypto';
import { createDatasetExecutionController } from '../../mcp-server/lib/dataset-execution.js';
import { LOAD_OPERATIONS } from './load-request-summary.mjs';
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const verdicts = new Set(['succeeded', 'failed', 'timeout', 'rejected', 'cancelled', 'ambiguous']);

/** Routes the approved workload fixture through existing runtime interfaces. */
export function createLoadWorkload({ fixtures, apiRequest, runtimeVersion, verifyOutcome, recordObservation }) {
  if (!Array.isArray(fixtures) || !fixtures.length || fixtures.length > 100000
    || typeof apiRequest !== 'function' || typeof verifyOutcome !== 'function' || typeof recordObservation !== 'function'
    || typeof runtimeVersion !== 'string' || !runtimeVersion.length) throw new Error('explicit workload adapters and fixtures required');
  const frozen = structuredClone(fixtures), byId = new Map();
  for (const f of frozen) {
    if (!f || typeof f.id !== 'string' || !f.id.length || f.id.length > 128 || byId.has(f.id)
      || !LOAD_OPERATIONS.includes(f.operation)) throw new Error('invalid workload fixture identity');
    if (['ingest', 'retry'].includes(f.operation)) {
      if (!f.input || !uuid.test(f.input.runId)) throw new Error('bound execution input required');
    } else if (['status', 'cancellation'].includes(f.operation)) {
      if (!uuid.test(f.runId)) throw new Error('bound lifecycle run required');
    } else if (f.operation === 'materialization') {
      if (!uuid.test(f.noteId) || !f.body || typeof f.body !== 'object' || Array.isArray(f.body)) throw new Error('bound materialization request required');
    } else if (f.operation === 'import') {
      if (!['core-v1', 'record-v1', 'full-v1'].includes(f.profile) || !f.body || typeof f.body.shard_base64 !== 'string') throw new Error('named import profile and fixture required');
    } else {
      if (typeof f.path !== 'string' || f.path.length > 4096 || f.path.includes('\\')) throw new Error('bounded workload path required');
      const url = new URL(f.path, 'https://scope.invalid');
      if (url.origin !== 'https://scope.invalid' || url.hash || `${url.pathname}${url.search}` !== f.path) throw new Error('invalid workload path');
      if (f.operation === 'query' && url.pathname !== '/api/v1/search') throw new Error('query route mismatch');
      if (f.operation === 'lineage') {
        const match = /^\/api\/v1\/notes\/([^/]+)\/(provenance|links)$/.exec(url.pathname);
        if (!match || !uuid.test(match[1])) throw new Error('lineage route mismatch');
      }
      if (f.operation === 'export' && (url.pathname !== '/api/v1/backup/knowledge-shard'
        || url.searchParams.getAll('profile').length !== 1 || !['core-v1', 'record-v1', 'full-v1'].includes(url.searchParams.get('profile')))) throw new Error('named export profile required');
    }
    byId.set(f.id, f);
  }
  const context = new AsyncLocalStorage();
  const scopedApiRequest = (method, path, body, options = {}) => {
    const current = context.getStore();
    if (!current) throw new Error('logical operation context required');
    const signals = [current.signal, options.signal].filter(Boolean);
    // Each dispatch gets a distinct HTTP request and attempt identity. This does
    // not add retries or change the controller's existing receipt-cache behavior.
    const identity = { logicalOperationId: current.logicalOperationId,
      requestId: randomUUID(), attemptId: randomUUID() };
    current.attempts.push(identity);
    return apiRequest(method, path, body, { ...options, ...identity,
      signal: signals.length ? AbortSignal.any(signals) : undefined });
  };
  const controller = createDatasetExecutionController({ runtimeVersion, apiRequest: scopedApiRequest });
  return async function execute(request, signal) {
    const original = byId.get(request?.id);
    if (!original || original.operation !== request.operation) throw new Error('scheduled fixture binding mismatch');
    if (signal?.aborted) return 'cancelled';
    const fixture = structuredClone(original), startNs = process.hrtime.bigint().toString();
    // Keep ordinary fixture IDs legible; arbitrary existing fixture IDs map to
    // deterministic bounded identifiers accepted by the transport and budget.
    const logicalOperationId = /^[A-Za-z0-9_.:-]{1,128}$/.test(request.id) ? request.id
      : `fixture:${createHash('sha256').update(request.id).digest('hex')}`;
    const attempts = [];
    let result, failure;
    try {
      result = await context.run({ signal, logicalOperationId, attempts }, async () => {
        switch (fixture.operation) {
          case 'ingest':
          case 'retry': return controller.handle({ ...fixture.input, action: 'execute' });
          case 'status': return controller.handle({ action: 'status', runId: fixture.runId });
          case 'cancellation': return controller.handle({ action: 'cancel', runId: fixture.runId });
          case 'materialization': return scopedApiRequest('POST', `/api/v1/notes/${fixture.noteId}/reprocess`, fixture.body, { signal });
          case 'import': return scopedApiRequest('POST', '/api/v1/backup/knowledge-shard/import', fixture.body, { signal });
          default: return scopedApiRequest('GET', fixture.path, null, { signal });
        }
      });
    } catch (error) {
      failure = { code: typeof error?.code === 'string' && /^[A-Z0-9_]{1,80}$/.test(error.code) ? error.code : 'WORKLOAD_EXECUTION_ERROR',
        status: Number.isInteger(error?.status) && error.status >= 100 && error.status <= 599 ? error.status : null };
    }
    const observation = { fixtureId: fixture.id, logicalOperationId, attempts: structuredClone(attempts), operation: fixture.operation, startNs,
      endNs: process.hrtime.bigint().toString(), ...(failure ? { failure } : { result }) };
    // Raw outcomes must be retained before a verifier can classify them. These
    // callbacks must live in the approved bounded worker/verifier environment.
    await recordObservation(structuredClone(observation));
    const verdict = await verifyOutcome({ fixture: structuredClone(original), observation: structuredClone(observation), signal });
    const verificationEndNs = process.hrtime.bigint().toString();
    const verified = verdict?.verified === true && verdicts.has(verdict.outcome)
      && /^sha256:[a-f0-9]{64}$/.test(verdict.evidenceDigest || '');
    await recordObservation({ fixtureId: fixture.id, logicalOperationId, attempts: structuredClone(attempts),
      operation: fixture.operation, acknowledgementEndNs: observation.endNs, verificationEndNs,
      // Completion means the independent verifier finished a valid classification,
      // which can be a verified failure. It is never inferred from HTTP success.
      completionNs: verified ? verificationEndNs : null, verification: structuredClone(verdict) });
    if (!verified) return 'ambiguous';
    return verdict.outcome;
  };
}
