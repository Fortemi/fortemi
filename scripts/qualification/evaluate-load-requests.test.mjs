import test from 'node:test';
import assert from 'node:assert/strict';
import { LOAD_PHASES, evaluateLoadRequests } from './evaluate-load-requests.mjs';
import { LOAD_OPERATIONS } from './load-request-summary.mjs';
function fixture() {
  const plan = LOAD_PHASES.map(name => ({ name, durationMs: 2000, windowMs: 1000, drainMs: 1000,
    schedule: [0, 1000].flatMap(scheduledMs => LOAD_OPERATIONS.map(operation => ({ id: `${operation}-${scheduledMs}`, operation, scheduledMs }))),
    thresholds: Object.fromEntries(LOAD_OPERATIONS.map(op => [op, { minimumSamples: 1,
      ...Object.fromEntries(['latencyP50', 'latencyP95', 'latencyP99'].map(m => [m, { operator: 'lte', limit: 100, unit: 'milliseconds' }])),
      throughputPerSecond: { operator: 'gte', limit: 1, unit: 'operations/second' },
      errorRate: { operator: 'eq', limit: 0, unit: 'ratio' } }])) }));
  const evidence = Object.fromEntries(plan.map(p => [p.name, p.schedule.map(r => ({ id: r.id, startedMs: r.scheduledMs,
    finishedMs: r.scheduledMs + 10, outcome: 'succeeded' }))]));
  return { plan, evidence };
}
test('evaluates every required phase, operation and window without granting admission', () => {
  const { plan, evidence } = fixture(), r = evaluateLoadRequests(plan, evidence);
  assert.equal(r.requestChecksPass, true); assert.equal(r.admitted, false);
  assert.deepEqual(r.phases.map(p => p.name), LOAD_PHASES);
});
test('a passing phase median cannot hide a breached window', () => {
  const { plan, evidence } = fixture(); evidence.smoke[9].finishedMs = 1200;
  const r = evaluateLoadRequests(plan, evidence); assert.equal(r.requestChecksPass, false);
  const findings = r.phases[0].failures.filter(f => f.metric === 'latencyP50');
  assert.equal(findings.length, 1); assert.equal(findings[0].scope, 'window');
});
test('missing evidence and observed failures both survive aggregation', () => {
  const { plan, evidence } = fixture(); evidence.smoke.pop(); evidence.smoke[0].outcome = 'timeout';
  const p = evaluateLoadRequests(plan, evidence).phases[0];
  assert.equal(p.status, 'FAIL'); assert.ok(p.missing.length); assert.ok(p.failures.length);
});
test('absent phase and empty operation/window are MISSING', () => {
  const { plan, evidence } = fixture(); delete evidence.soak; evidence.average = [];
  const r = evaluateLoadRequests(plan, evidence);
  assert.equal(r.phases[1].status, 'MISSING'); assert.equal(r.phases[4].status, 'MISSING');
});
test('sample floor is enforced per window even when phase sample count passes', () => {
  const { plan, evidence } = fixture(); plan[0].thresholds.ingest.minimumSamples = 2;
  const p = evaluateLoadRequests(plan, evidence).phases[0]; assert.equal(p.status, 'MISSING');
  assert.ok(p.missing.every(m => m.scope === 'window'));
});
test('duplicate observations produce FAIL rather than disappear', () => {
  const { plan, evidence } = fixture(); evidence.spike.push(evidence.spike[0]);
  assert.equal(evaluateLoadRequests(plan, evidence).phases[3].status, 'FAIL');
});
for (const [name, mutate] of [
  ['phase omission', p => p.pop()], ['duplicate phase', p => p[1].name = 'smoke'],
  ['unknown phase', p => p[1].name = 'custom'],
  ['missing operation policy', p => delete p[0].thresholds.query],
  ['missing metric', p => delete p[0].thresholds.query.errorRate],
  ['wrong units', p => p[0].thresholds.query.latencyP99.unit = 'seconds'],
  ['inverted comparison', p => p[0].thresholds.query.latencyP99.operator = 'gte'],
  ['nonfinite limit', p => p[0].thresholds.query.latencyP99.limit = NaN],
  ['vacuous throughput', p => p[0].thresholds.query.throughputPerSecond.limit = 0],
  ['absent sample floor', p => delete p[0].thresholds.query.minimumSamples],
  ['invalid unsampled schedule', p => p[4].schedule[0].scheduledMs = -1],
  ['unbounded window count', p => { p[0].durationMs = 1001; p[0].windowMs = 1; }],
]) test(`rejects ${name} before examining missing evidence`, () => {
  const { plan } = fixture(); mutate(plan); assert.throws(() => evaluateLoadRequests(plan, {}));
});
test('rejects evidence for an undeclared phase', () => {
  const { plan, evidence } = fixture(); evidence.custom = []; assert.throws(() => evaluateLoadRequests(plan, evidence));
});
