import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeLoadRequests, LOAD_OPERATIONS } from './load-request-summary.mjs';
const base = () => ({ durationMs: 2000, windowMs: 1000, drainMs: 1000,
  schedule: [{ id: 'a', operation: 'ingest', scheduledMs: 0 }, { id: 'b', operation: 'ingest', scheduledMs: 1000 }],
  observations: [{ id: 'a', startedMs: 900, finishedMs: 1100, outcome: 'succeeded' },
    { id: 'b', startedMs: 1000, finishedMs: 2500, outcome: 'timeout' }] });
test('retains scheduling delay, failures, drain and completion-time throughput', () => {
  const r = summarizeLoadRequests(base());
  assert.equal(r.phase.operations.ingest.latencyP99, 1500);
  assert.equal(r.phase.operations.ingest.serviceP99, 1500);
  assert.equal(r.windows[0].operations.ingest.latencyP99, 1100);
  assert.equal(r.windows[0].operations.ingest.serviceP99, 200);
  assert.equal(r.windows[0].operations.ingest.throughputPerSecond, 0);
  assert.equal(r.windows[1].operations.ingest.throughputPerSecond, 1);
  assert.equal(r.phase.operations.ingest.throughputPerSecond, 0.5);
  assert.equal(r.phase.operations.ingest.errorRate, 0.5);
  assert.equal(r.admitted, false);
});
test('missing observations and empty operations cannot become zero-error successes', () => {
  const b = base(); b.observations.pop(); const r = summarizeLoadRequests(b);
  assert.equal(r.phase.operations.ingest.coverage, 'MISSING');
  assert.equal(r.phase.operations.ingest.errorRate, null);
  assert.equal(r.phase.operations.query.coverage, 'MISSING');
  assert.equal(r.phase.operations.query.latencyP99, null);
  assert.deepEqual(Object.keys(r.phase.operations), LOAD_OPERATIONS);
});
test('nearest rank derives whole-phase percentile from raw values, not window percentiles', () => {
  const b = base(); b.schedule = []; b.observations = [];
  for (let i = 0; i < 100; i++) {
    b.schedule.push({ id: String(i), operation: 'query', scheduledMs: 0 });
    b.observations.push({ id: String(i), startedMs: 0, finishedMs: i + 1, outcome: 'succeeded' });
  }
  const r = summarizeLoadRequests(b).phase.operations.query;
  assert.deepEqual([r.latencyP50, r.latencyP95, r.latencyP99], [50, 95, 99]);
});
for (const [name, mutate] of [
  ['duplicate schedule', b => b.schedule.push(b.schedule[0])],
  ['duplicate observation', b => b.observations.push(b.observations[0])],
  ['unbound observation', b => b.observations[0].id = 'unknown'],
  ['negative scheduling delay', b => b.observations[1].startedMs = 999],
  ['backwards completion', b => b.observations[0].finishedMs = 800],
  ['unbounded drain', b => b.drainMs = Infinity],
  ['nonfinite timestamp', b => b.observations[0].startedMs = NaN],
  ['late observation', b => b.observations[0].finishedMs = 3001],
  ['unknown operation', b => b.schedule[0].operation = 'bogus'],
  ['unknown outcome', b => b.observations[0].outcome = 'passed'],
  ['partial final window', b => b.windowMs = 1500],
  ['too many windows', b => { b.durationMs = 10001; b.windowMs = 1; }],
  ['too many requests', b => b.schedule = new Array(100001)],
]) test(`rejects ${name}`, () => { const b = base(); mutate(b); assert.throws(() => summarizeLoadRequests(b)); });
test('phase-end completion is drain, and a window-boundary completion enters the next window', () => {
  const b = base(); b.observations[0].finishedMs = 1000; b.observations[1].finishedMs = 2000;
  b.observations[1].outcome = 'succeeded'; const r = summarizeLoadRequests(b);
  assert.equal(r.windows[0].operations.ingest.successfulInWindow, 0);
  assert.equal(r.windows[1].operations.ingest.successfulInWindow, 1);
  assert.equal(r.phase.operations.ingest.outcomes.succeeded, 2);
  assert.equal(r.phase.operations.ingest.successfulInWindow, 1);
});
