import test from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as sleep } from 'node:timers/promises';
import { runLoadSchedule } from './run-load-schedule.mjs';
const plan = () => ({ durationMs: 100, drainMs: 50, maxConcurrency: 2, maxScheduleLagMs: 1000,
  pollMs: 2, safetyTimeoutMs: 50, schedule: Array.from({ length: 4 }, (_, i) => ({ id: String(i), operation: 'query', scheduledMs: 0 })) });
test('executes ordered arrivals with bounded concurrency and original schedule', async () => {
  let active = 0, peak = 0;
  const p = plan(); const r = await runLoadSchedule(p, { checkSafety: async () => true, execute: async request => {
    active++; peak = Math.max(peak, active); request.scheduledMs = 999; await sleep(2); active--; return 'succeeded';
  } });
  assert.equal(r.abortReason, null); assert.equal(r.observations.length, 4); assert.equal(peak, 2);
  assert.ok(r.schedule.every(s => s.scheduledMs === 0)); assert.equal(r.cleanupVerified, false);
});
test('failed preflight launches no workload', async () => {
  let calls = 0; const r = await runLoadSchedule(plan(), { checkSafety: async () => false, execute: async () => { calls++; } });
  assert.equal(calls, 0); assert.equal(r.abortReason, 'safety-rejected'); assert.equal(r.undispatched.length, 4);
});
test('hung safety collector times out and remains explicitly unresolved', async () => {
  const r = await runLoadSchedule(plan(), { checkSafety: () => new Promise(() => {}), execute: async () => 'succeeded' });
  assert.equal(r.abortReason, 'safety-timeout'); assert.equal(r.safetyUnresolved, true); assert.equal(r.executionSettled, false);
});
test('hung executors retain slots and cannot fabricate cleanup or observations', async () => {
  const p = plan(); p.durationMs = 30; p.maxScheduleLagMs = 10;
  const r = await runLoadSchedule(p, { checkSafety: async () => true, execute: () => new Promise(() => {}) });
  assert.ok(['schedule-lag', 'schedule-incomplete'].includes(r.abortReason));
  assert.equal(r.unresolved.length, 2); assert.equal(r.undispatched.length, 2); assert.deepEqual(r.observations, []);
  assert.equal(r.executionSettled, false);
});
test('adapter exceptions are ambiguous and stop further dispatch', async () => {
  const r = await runLoadSchedule(plan(), { checkSafety: async () => true, execute: async () => { throw Error('private error detail'); } });
  assert.equal(r.abortReason, 'executor-error'); assert.ok(r.observations.every(o => o.outcome === 'ambiguous'));
  assert.ok(!JSON.stringify(r).includes('private error detail'));
});
test('pre-aborted caller cannot invoke either adapter', async () => {
  const c = new AbortController(); c.abort(); let calls = 0;
  const r = await runLoadSchedule(plan(), { signal: c.signal, checkSafety: () => { calls++; }, execute: () => { calls++; } });
  assert.equal(calls, 0); assert.equal(r.abortReason, 'external-abort');
});
test('invalid or unordered schedules reject before callbacks', async () => {
  for (const mutate of [p => p.maxConcurrency = 33, p => p.schedule[0].scheduledMs = -1,
    p => p.schedule[1].id = '0', p => { p.schedule[0].scheduledMs = 10; }, p => p.drainMs = Infinity]) {
    const p = plan(); mutate(p); let calls = 0;
    await assert.rejects(runLoadSchedule(p, { execute: () => { calls++; }, checkSafety: () => { calls++; } })); assert.equal(calls, 0);
  }
});
test('late completion during an event-loop stall remains unresolved beyond the fixed drain bound', async () => {
  const p = plan(); p.durationMs = 40; p.drainMs = 20; p.maxConcurrency = 1;
  p.schedule = p.schedule.slice(0, 2);
  let finish, stalled = false;
  const r = await runLoadSchedule(p, {
    execute: () => new Promise(resolve => { finish = resolve; }),
    checkSafety: () => {
      if (finish && !stalled) {
        stalled = true;
        // Force completion delivery beyond the entire budget, without relying
        // on timer ordering or asserting an exact elapsed wall-clock duration.
        finish('succeeded');
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, p.durationMs + p.drainMs + 30);
      }
      return true;
    },
  });
  assert.equal(stalled, true);
  assert.equal(r.abortReason, 'drain-timeout');
  assert.deepEqual(r.observations, []);
  assert.deepEqual(r.unresolved, ['0']);
  assert.deepEqual(r.undispatched, ['1']);
  assert.equal(r.observations.length + r.unresolved.length + r.undispatched.length, p.schedule.length);
  assert.equal(r.executionSettled, false); assert.equal(r.qualificationPassed, false);
});
