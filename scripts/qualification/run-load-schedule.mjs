import { performance } from 'node:perf_hooks';
import { LOAD_OPERATIONS } from './load-request-summary.mjs';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const outcomes = new Set(['succeeded', 'failed', 'timeout', 'rejected', 'cancelled', 'ambiguous']);
const positive = n => Number.isSafeInteger(n) && n > 0;

/**
 * Mechanism only. The caller must authorize the exact plan and adapter before
 * invoking this function. No endpoint, credentials or dynamic code loading.
 */
export async function runLoadSchedule(plan, { execute, checkSafety, signal }) {
  if (!plan || !positive(plan.durationMs) || plan.durationMs > 43200000
    || !positive(plan.drainMs) || plan.drainMs > 300000
    || !positive(plan.maxConcurrency) || plan.maxConcurrency > 32
    || !positive(plan.maxScheduleLagMs) || !positive(plan.pollMs) || plan.pollMs > 1000
    || !positive(plan.safetyTimeoutMs) || plan.safetyTimeoutMs > 10000
    || !Array.isArray(plan.schedule) || plan.schedule.length > 100000
    || typeof execute !== 'function' || typeof checkSafety !== 'function') throw new Error('bounded execution plan and adapters required');
  const schedule = structuredClone(plan.schedule), ids = new Set();
  let prior = -1;
  for (const r of schedule) {
    if (!r || typeof r.id !== 'string' || !r.id.length || r.id.length > 128 || ids.has(r.id)
      || !LOAD_OPERATIONS.includes(r.operation) || !Number.isFinite(r.scheduledMs)
      || r.scheduledMs < 0 || r.scheduledMs >= plan.durationMs || r.scheduledMs < prior) throw new Error('invalid ordered arrival schedule');
    ids.add(r.id); prior = r.scheduledMs;
  }
  // Copy all options before any caller callback can mutate the supplied plan.
  const policy = { ...plan, schedule };
  const controller = new AbortController(), pending = new Map(), observations = [];
  let abortReason = null, index = 0, safetyUnresolved = false, safetyChecks = 0;
  const stop = reason => { if (!abortReason) { abortReason = reason; controller.abort(new Error(reason)); } };
  const externalAbort = () => stop('external-abort');
  if (signal?.aborted) externalAbort();
  signal?.addEventListener('abort', externalAbort, { once: true });
  const origin = performance.now(), now = () => performance.now() - origin;
  let completionDeadline = policy.durationMs + policy.drainMs;
  async function safety(deadline = policy.durationMs) {
    if (abortReason) return;
    let timer;
    safetyChecks++;
    const timeoutMs = Math.min(policy.safetyTimeoutMs, Math.max(1, deadline - now()));
    try {
      const observation = Promise.resolve().then(() => checkSafety(controller.signal));
      const timeout = new Promise((_, reject) => { timer = setTimeout(() => {
        safetyUnresolved = true; reject(new Error('safety-timeout'));
      }, timeoutMs); });
      const ok = await Promise.race([observation, timeout]);
      if (ok !== true) stop('safety-rejected');
    } catch { stop(safetyUnresolved ? 'safety-timeout' : 'safety-error'); }
    finally { clearTimeout(timer); }
  }
  function dispatch(request) {
    const startedMs = now();
    pending.set(request.id, { startedMs });
    const complete = (outcome, failureReason) => {
      const finishedMs = now();
      // An event-loop stall can deliver an executor completion before the drain
      // timer runs. Keep that dispatched ID unresolved rather than accepting an
      // observation outside the approved phase/drain interval.
      if (finishedMs > completionDeadline) { stop('drain-timeout'); return; }
      observations.push({ id: request.id, startedMs, finishedMs, outcome });
      pending.delete(request.id);
      if (failureReason) stop(failureReason);
    };
    Promise.resolve().then(() => execute(structuredClone(request), controller.signal)).then(result => {
      const outcome = typeof result === 'string' && outcomes.has(result) ? result : 'ambiguous';
      complete(outcome, outcome === 'ambiguous' ? 'ambiguous-outcome' : null);
    }, () => {
      complete('ambiguous', 'executor-error');
    });
  }
  try {
    while (!abortReason && now() < policy.durationMs) {
      await safety();
      if (abortReason || now() >= policy.durationMs) break;
      // A schedule entry remains queued with its original arrival timestamp.
      // Saturation never rewrites arrivals or fabricates an observation.
      while (index < schedule.length && schedule[index].scheduledMs <= now()) {
        if (now() - schedule[index].scheduledMs > policy.maxScheduleLagMs) { stop('schedule-lag'); break; }
        if (pending.size >= policy.maxConcurrency) break;
        dispatch(schedule[index++]);
      }
      if (!abortReason) await delay(Math.min(policy.pollMs, Math.max(1, policy.durationMs - now())));
    }
    if (!abortReason && index !== schedule.length) stop('schedule-incomplete');
    const drainDeadline = Math.min(now() + policy.drainMs, policy.durationMs + policy.drainMs);
    completionDeadline = drainDeadline;
    while (pending.size && now() < drainDeadline) {
      await safety(drainDeadline);
      await delay(Math.min(policy.pollMs, Math.max(1, drainDeadline - now())));
    }
    if (pending.size) stop('drain-timeout');
    return { admitted: false, qualificationPassed: false, abortReason, safetyChecks, safetyUnresolved,
      elapsedMs: now(), schedule: structuredClone(schedule), observations: structuredClone(observations),
      undispatched: schedule.slice(index).map(r => r.id), unresolved: [...pending.keys()],
      executionSettled: !pending.size && !safetyUnresolved, cleanupVerified: false };
  } finally { signal?.removeEventListener('abort', externalAbort); }
}
