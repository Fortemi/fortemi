import test from 'node:test';
import assert from 'node:assert/strict';
import { loadConfigTemplate, compileLoadConfig } from './load-config.mjs';
import { runLoadExperiment } from './run-load-experiment.mjs';
import { LOAD_TELEMETRY } from './evaluate-load-telemetry.mjs';
import { jsonDigest } from './canonical-json.mjs';
import { createHash } from 'node:crypto';
import fs from 'node:fs';

function setup() {
  const document = loadConfigTemplate(), c = document.defaults;
  c.provider = { maxCostUsd: '0', priceRevision: 'synthetic-no-provider' };
  c.stage = 'rehearsal'; c.phases = [{ name: 'one', ratePerSecond: 10, durationMs: 100, windowMs: 100 }];
  c.mix.query = 10000;
  Object.assign(c.runner, { drainMs: 100, pollMs: 1, safetyTimeoutMs: 100 });
  c.cleanup = { timeoutMs: 100, settleMs: 2 };
  c.telemetry.maxGapMs = 100; c.telemetry.maxAgeMs = 100;
  for (const bound of Object.values(c.operationBounds)) Object.assign(bound, { httpRequests: 1, providerAttempts: 0, evidenceBytes: 10000, deadlineMs: 50 });
  c.requestPolicy.minimumSamples = 1;
  for (const [metric, t] of Object.entries(c.requestPolicy)) if (metric !== 'minimumSamples') t.limit = metric === 'throughputPerSecond' ? 0.001 : 1;
  for (const t of Object.values(c.telemetry.thresholds)) t.limit = 1;
  const namespaces = [...c.environment.runtimeTenants, c.environment.controlNamespace];
  const baseline = namespaces.flatMap(namespace => ['records', 'jobs', 'blobs'].map(surface => ({ namespace, surface, resources: [] })));
  const statePlan = { namespaces, controlNamespaces: [c.environment.controlNamespace], requiredSurfaces: ['records', 'jobs', 'blobs'],
    expectedBaseline: baseline, expectedAfter: baseline, minimumSettleMs: c.cleanup.settleMs };
  const fixtures = [{ id: 'one-0', operation: 'query', path: '/api/v1/search?q=synthetic' }];
  Object.assign(c.environment, { target: 'target', generator: 'generator', observer: 'observer', runtimeRevision: 'a'.repeat(40),
    fixtureDigest: jsonDigest({ fixtures, statePlan }) });
  const records = [], calls = [];
  const adapters = { fixtures, statePlan,
    authorize: async () => true,
    apiRequest: async () => { calls.push('api'); return {}; },
    verifyOutcome: async () => ({ verified: true, outcome: 'succeeded', evidenceDigest: `sha256:${'b'.repeat(64)}` }),
    checkSafety: async () => true,
    readProviderUsage: async ({ timeMs }) => ({ attempts: 0, costUsd: '0', priceRevision: 'synthetic-no-provider', complete: true, observedMs: timeMs, sourceId: 'synthetic-provider', resetId: 'initial' }),
    observeState: async ({ point }) => { calls.push(point); return structuredClone(baseline); },
    cleanup: async ({ namespaces: owned, signal }) => { calls.push('delete'); assert.equal(signal.aborted, false); assert.deepEqual(owned, c.environment.runtimeTenants); },
    readTelemetry: async () => [0, 100].map(timeMs => ({ timeMs, values: Object.fromEntries(Object.entries(LOAD_TELEMETRY).map(([metric, [unit, direction]]) => [metric, { value: direction === 'lower' ? 1 : 0, unit, observedMs: timeMs }])) })),
    record: async bytes => { records.push(JSON.parse(bytes)); return { path: 'synthetic', digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`, bytes: bytes.length }; },
  };
  return { document, compile: () => compileLoadConfig(document, 'calibration'), adapters, records, calls };
}
test('integrates selected plan, bounded requests, telemetry, state, cleanup and evidence without admission', async () => {
  const f = setup(), result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials[0].abortReason, null);
  assert.equal(result.trials[0].phases[0].result.observations.length, 1);
  assert.equal(result.httpUsage.attempts, 1);
  assert.equal(result.cleanupVerified, true);
  assert.equal(result.qualificationPassed, false); assert.equal(result.admitted, false);
  assert.deepEqual(f.calls, ['before', 'api', 'after', 'delete', 'cleanup', 'settled']);
  assert.ok(f.records.some(r => r.kind === 'run-result'));
});
test('repetitions reset the workload and inventory while budgets count the whole run', async () => {
  const f = setup(); f.document.defaults.repetitions = 2;
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials.length, 2); assert.equal(result.httpUsage.logicalOperations, 2);
  assert.equal(f.calls.filter(c => c === 'delete').length, 2);
});
test('edited compiled parameters and fixture inventories reject before adapters', async () => {
  for (const mutate of [p => { p.config.runner.maxConcurrency = 2; }, p => { p.ready = true; p.missing = []; p.phases[0].schedule = []; }]) {
    const f = setup(), p = f.compile(); mutate(p); let called = false;
    f.adapters.authorize = () => { called = true; };
    await assert.rejects(runLoadExperiment(p, f.adapters)); assert.equal(called, false);
  }
  const f = setup(); f.adapters.fixtures[0].path = '/api/v1/search?q=other';
  await assert.rejects(runLoadExperiment(f.compile(), f.adapters), /fixture\/state/); assert.deepEqual(f.calls, []);
});
test('baseline mismatch invokes no workload or destructive cleanup', async () => {
  const f = setup(); f.adapters.observeState = async () => [];
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials[0].abortReason, 'baseline-mismatch');
  assert.equal(result.cleanupVerified, false); assert.deepEqual(f.calls, []);
});
test('missing telemetry stops the trial but still verifies cleanup', async () => {
  const f = setup(); f.adapters.readTelemetry = async () => [];
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials[0].abortReason, 'telemetry-not-passing');
  assert.ok(f.calls.includes('delete')); assert.equal(result.trials[0].phases[0].telemetry.status, 'MISSING');
});
test('evidence failure after dispatch stops work and enters cleanup', async () => {
  const f = setup(), record = f.adapters.record;
  f.adapters.record = bytes => JSON.parse(bytes).kind === 'http-dispatch' ? Promise.reject(Error('disk full')) : record(bytes);
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.ok(result.trials[0].abortReason); assert.ok(f.calls.includes('delete'));
  assert.ok(!f.calls.includes('api')); assert.equal(result.qualificationPassed, false);
});
test('late writes and unchanged control namespace checks cannot be bypassed', async () => {
  const f = setup(), observe = f.adapters.observeState;
  f.adapters.observeState = async input => {
    const inventory = await observe(input);
    if (input.point === 'settled') inventory[0].resources.push({ id: 'late', content: {} });
    return inventory;
  };
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials[0].state.status, 'FAIL'); assert.equal(result.cleanupVerified, false);
});
test('hung cleanup remains unresolved and never becomes verified', async () => {
  const f = setup(); f.adapters.cleanup = () => new Promise(() => {});
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.cleanupVerified, false);
  assert.ok(result.unresolved.some(k => k.endsWith(':cleanup')));
  assert.ok(result.failures.some(f => f.reason === 'cleanup-failed-or-unsettled'));
});
test('provider evidence gaps and cumulative budget breaches prevent dispatch', async () => {
  for (const usage of [{ complete: false, attempts: 0, costUsd: '0', priceRevision: 'synthetic-no-provider' },
    { complete: true, attempts: 0, costUsd: '0.000000000000000001', priceRevision: 'synthetic-no-provider' },
    { complete: true, attempts: 100001, costUsd: '0', priceRevision: 'synthetic-no-provider' }]) {
    const f = setup(); f.adapters.readProviderUsage = async ({ timeMs }) => ({ ...usage, observedMs: timeMs, sourceId: 'synthetic-provider', resetId: 'initial' });
    const result = await runLoadExperiment(f.compile(), f.adapters);
    assert.ok(!f.calls.includes('api'));
    assert.equal(result.trials[0].phases[0].result.abortReason, 'safety-rejected');
  }
});
test('final evidence failure updates unresolved callbacks, counters and cleanup verdict', async () => {
  const f = setup(), record = f.adapters.record;
  f.adapters.record = bytes => JSON.parse(bytes).kind === 'run-result' ? new Promise(() => {}) : record(bytes);
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.ok(result.unresolved.some(k => k.endsWith(':record')));
  assert.equal(result.cleanupVerified, false);
  assert.ok(result.failures.some(f => f.reason === 'result-evidence-failed'));
  assert.ok(result.attemptedEvidenceFiles > result.artifacts.length);
});
test('dispatch evidence and transport retain the same attempt identities', async () => {
  const f = setup(); let actual;
  f.adapters.apiRequest = async (_method, _path, _body, options) => { actual = options; return {}; };
  await runLoadExperiment(f.compile(), f.adapters);
  const event = f.records.find(r => r.kind === 'http-dispatch').value;
  assert.equal(event.attemptId, actual.attemptId); assert.equal(event.requestId, actual.requestId);
  assert.equal(event.logicalOperationId, actual.logicalOperationId);
  assert.notEqual(event.budgetLogicalOperationId, actual.logicalOperationId);
});
test('cancellation during an operation still invokes cleanup using a fresh signal', async () => {
  const f = setup(), controller = new AbortController(); f.adapters.signal = controller.signal;
  f.adapters.apiRequest = async () => { f.calls.push('api'); controller.abort(); throw Error('cancel'); };
  f.adapters.verifyOutcome = async () => ({ verified: true, outcome: 'cancelled', evidenceDigest: `sha256:${'b'.repeat(64)}` });
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.ok(f.calls.includes('delete')); assert.ok(result.trials[0].abortReason);
});

test('qualification stops after the first failing request window and retains missing later phases', async () => {
  const f = setup(), c = f.document.defaults;
  c.stage = 'qualification'; c.repetitions = 2;
  c.phases = ['smoke', 'average', 'stress', 'spike', 'soak'].map(name => ({ name, durationMs: 450, windowMs: 450, ratePerSecond: 20 }));
  for (const op of Object.keys(c.mix)) c.mix[op] = 1;
  c.requestPolicy.latencyP50.limit = 0;
  const input = JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-execution/1.0.0/fixtures/supported-request.json', import.meta.url)));
  const noteId = '018fd1a0-0000-7000-8000-000000001130';
  const make = request => {
    switch(request.operation) {
      case 'ingest': case 'retry': return { ...request, input };
      case 'status': case 'cancellation': return { ...request, runId: input.runId };
      case 'lineage': return { ...request, path: `/api/v1/notes/${noteId}/provenance` };
      case 'materialization': return { ...request, noteId, body: {} };
      case 'export': return { ...request, path: '/api/v1/backup/knowledge-shard?profile=core-v1' };
      case 'import': return { ...request, profile: 'core-v1', body: { shard_base64: 'c3ludGhldGlj' } };
      default: return { ...request, path: '/api/v1/search?q=synthetic' };
    }
  };
  f.adapters.fixtures = f.compile().phases.flatMap(p => p.schedule.map(make));
  c.environment.fixtureDigest = jsonDigest({ fixtures: f.adapters.fixtures, statePlan: f.adapters.statePlan });
  f.adapters.readTelemetry = async () => [0, 90, 180, 270, 360, 450].map(timeMs => ({ timeMs, values: Object.fromEntries(Object.entries(LOAD_TELEMETRY).map(([metric, [unit, direction]]) => [metric, { value: direction === 'lower' ? 1 : 0, unit, observedMs: timeMs }])) }));
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials.length, 1);
  assert.equal(result.trials[0].abortReason, 'request-not-passing');
  assert.equal(result.trials[0].phases.length, 1);
  assert.equal(result.trials[0].requests.phases[1].status, 'MISSING');
});

test('hung safety adapter retains unsettled phase and prevents a verified cleanup claim', async () => {
  const f = setup(); let checks = 0;
  f.adapters.checkSafety = () => ++checks === 1 ? true : new Promise(() => {});
  const result = await runLoadExperiment(f.compile(), f.adapters);
  assert.equal(result.trials[0].phases[0].result.safetyUnresolved, true);
  assert.ok(f.calls.includes('delete'));
  assert.equal(result.cleanupVerified, false);
});
