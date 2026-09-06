import test from 'node:test';
import assert from 'node:assert/strict';
import { LOAD_TELEMETRY, evaluateLoadTelemetry } from './evaluate-load-telemetry.mjs';
function fixture() {
  const policy = { durationMs: 1000, maxGapMs: 500, maxAgeMs: 100,
    thresholds: Object.fromEntries(Object.entries(LOAD_TELEMETRY).map(([m, [unit, direction]]) => [m,
      { operator: direction === 'lower' ? 'gte' : 'lte', limit: direction === 'lower' ? 10 : 100, unit }])) };
  const frames = [0, 500, 1000].map(timeMs => ({ timeMs,
    values: Object.fromEntries(Object.entries(LOAD_TELEMETRY).map(([m, [unit]]) => [m, { unit, value: 50, observedMs: timeMs }])) }));
  return { policy, frames };
}
test('checks all metrics throughout the retained interval without granting admission', () => {
  const { policy, frames } = fixture(); const r = evaluateLoadTelemetry(policy, frames);
  assert.equal(r.telemetryChecksPass, true); assert.equal(r.admitted, false);
});
for (const [name, mutate, reason] of [
  ['transient peak', f => f[1].values.rssBytes.value = 101, 'threshold-breach'],
  ['free-space exhaustion', f => f[1].values.freeBytes.value = 9, 'threshold-breach'],
  ['provider counter reset', f => f[2].values.providerCalls.value = 49, 'counter-reset'],
  ['wrong units', f => f[0].values.rssBytes.unit = 'MiB', 'invalid-observation'],
  ['nonfinite sample', f => f[0].values.rssBytes.value = NaN, 'invalid-observation'],
  ['fractional count', f => f[0].values.queueDepth.value = 0.5, 'invalid-observation'],
  ['future timestamp', f => f[0].values.rssBytes.observedMs = 1, 'invalid-observation'],
  ['clock reversal', f => f[2].values.rssBytes.observedMs = 499, 'inconsistent-observation-clock'],
  ['same timestamp changed value', f => { f[1].values.rssBytes.observedMs = 0; f[1].values.rssBytes.value = 51; }, 'inconsistent-observation-clock'],
]) test(`fails ${name}`, () => {
  const { policy, frames } = fixture(); mutate(frames); const r = evaluateLoadTelemetry(policy, frames);
  assert.equal(r.status, 'FAIL'); assert.ok(r.failures.some(f => f.reason === reason));
});
for (const [name, mutate, reason] of [
  ['start coverage', f => f.shift(), 'initial-frame-missing'],
  ['end coverage', f => f.pop(), 'terminal-frame-missing'],
  ['interior coverage', f => f.splice(1, 1), 'frame-gap'],
  ['metric coverage', f => delete f[1].values.providerCost, 'metric-missing'],
  ['stale individual metric', f => f[1].values.providerCost.observedMs = 0, 'stale-observation'],
]) test(`marks missing ${name}`, () => {
  const { policy, frames } = fixture(); mutate(frames); const r = evaluateLoadTelemetry(policy, frames);
  assert.equal(r.status, 'MISSING'); assert.ok(r.missing.some(f => f.reason === reason));
});
test('retains missing coverage together with observed failure', () => {
  const { policy, frames } = fixture(); delete frames[1].values.providerCost; frames[1].values.freeBytes.value = 1;
  const r = evaluateLoadTelemetry(policy, frames); assert.ok(r.missingCount); assert.ok(r.failureCount);
});
for (const [name, mutate] of [
  ['missing policy metric', p => delete p.thresholds.providerCost],
  ['inverted disk ceiling', p => p.thresholds.freeBytes.operator = 'lte'],
  ['zero reserve', p => p.thresholds.freeBytes.limit = 0],
  ['wrong policy units', p => p.thresholds.providerCost.unit = 'EUR'],
  ['unbounded age', p => p.maxAgeMs = Infinity],
  ['age exceeding cadence', p => p.maxAgeMs = 501],
]) test(`rejects ${name}`, () => {
  const { policy, frames } = fixture(); mutate(policy); assert.throws(() => evaluateLoadTelemetry(policy, frames));
});
test('duplicate frames, extra metrics and excessive frame counts reject', () => {
  for (const mutate of [f => f.push(f[0]), f => f[0].values.extra = {}, f => f.push(...new Array(10000))]) {
    const { policy, frames } = fixture(); mutate(frames); assert.throws(() => evaluateLoadTelemetry(policy, frames));
  }
});
