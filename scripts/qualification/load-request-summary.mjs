/** Offline request-observation reduction. No execution, approval or qualification. */
export const LOAD_OPERATIONS = Object.freeze(['ingest', 'query', 'lineage', 'materialization',
  'export', 'import', 'status', 'cancellation', 'retry']);
const outcomes = new Set(['succeeded', 'failed', 'timeout', 'rejected', 'cancelled', 'ambiguous']);
const finite = n => typeof n === 'number' && Number.isFinite(n) && n >= 0;
const quantile = (sorted, q) => sorted.length ? sorted[Math.ceil(q * sorted.length) - 1] : null;

/**
 * Times are milliseconds on one monotonic clock relative to the phase origin.
 * Schedule is retained separately from observations so dropped arrivals remain missing.
 * Outcome success must already be independently checked for correctness by the caller.
 */
export function summarizeLoadRequests({ durationMs, windowMs, drainMs, schedule, observations }) {
  if (![durationMs, windowMs].every(n => Number.isSafeInteger(n) && n > 0)
    || !Number.isSafeInteger(drainMs) || drainMs < 0
    || !Number.isSafeInteger(durationMs + drainMs)
    || durationMs % windowMs !== 0 || durationMs / windowMs > 10000) {
    throw new Error('invalid bounded phase/window/drain duration');
  }
  if (!Array.isArray(schedule) || !Array.isArray(observations)
    || schedule.length > 100000 || observations.length > 100000) throw new Error('request observation bound exceeded');
  const expected = new Map(), actual = new Map();
  for (const request of schedule) {
    if (!request || typeof request.id !== 'string' || !request.id.length || request.id.length > 128
      || expected.has(request.id) || !LOAD_OPERATIONS.includes(request.operation)
      || !finite(request.scheduledMs) || request.scheduledMs >= durationMs) throw new Error('invalid or duplicate scheduled request');
    expected.set(request.id, request);
  }
  for (const observation of observations) {
    const request = expected.get(observation?.id);
    if (!request || actual.has(observation.id) || !outcomes.has(observation.outcome)
      || !finite(observation.startedMs) || !finite(observation.finishedMs)
      || observation.startedMs < request.scheduledMs || observation.finishedMs < observation.startedMs
      || observation.finishedMs > durationMs + drainMs) throw new Error('invalid, unbound or duplicate observation');
    actual.set(observation.id, observation);
  }
  // Buckets are allocated once: never scan the full stream for each window.
  const makeBucket = (startMs, endMs) => ({ startMs, endMs,
    operations: Object.fromEntries(LOAD_OPERATIONS.map(operation => [operation,
      { scheduled: 0, missing: 0, outcomes: Object.fromEntries([...outcomes].map(o => [o, 0])),
        completedInWindow: 0, successfulInWindow: 0, latency: [], service: [] }])) });
  const windows = Array.from({ length: durationMs / windowMs }, (_, i) => makeBucket(i * windowMs, (i + 1) * windowMs));
  const phase = makeBucket(0, durationMs);
  for (const request of schedule) {
    const observation = actual.get(request.id);
    for (const bucket of [phase, windows[Math.floor(request.scheduledMs / windowMs)]]) {
      const row = bucket.operations[request.operation]; row.scheduled++;
      if (!observation) { row.missing++; continue; }
      row.outcomes[observation.outcome]++;
      row.latency.push(observation.finishedMs - request.scheduledMs);
      row.service.push(observation.finishedMs - observation.startedMs);
    }
    // Throughput follows completion time, not arrival cohort. Drain completions
    // remain visible in cohort outcomes but never inflate the offered phase rate.
    if (observation && observation.finishedMs < durationMs) {
      for (const bucket of [phase, windows[Math.floor(observation.finishedMs / windowMs)]]) {
        const row = bucket.operations[request.operation]; row.completedInWindow++;
        if (observation.outcome === 'succeeded') row.successfulInWindow++;
      }
    }
  }
  const finish = bucket => ({ startMs: bucket.startMs, endMs: bucket.endMs,
    operations: Object.fromEntries(Object.entries(bucket.operations).map(([operation, row]) => {
      row.latency.sort((a, b) => a - b); row.service.sort((a, b) => a - b);
      const observed = row.scheduled - row.missing;
      return [operation, { coverage: row.scheduled === 0 || row.missing ? 'MISSING' : 'COMPLETE',
        scheduled: row.scheduled, observed, missing: row.missing, outcomes: row.outcomes,
        completedInWindow: row.completedInWindow, successfulInWindow: row.successfulInWindow,
        offeredPerSecond: row.scheduled * 1000 / (bucket.endMs - bucket.startMs),
        throughputPerSecond: row.successfulInWindow * 1000 / (bucket.endMs - bucket.startMs),
        // Every nonsuccess is retained as an error here. An expected rejection is
        // a separate qualification cell, never silently removed from the denominator.
        errorRate: row.scheduled && !row.missing ? (observed - row.outcomes.succeeded) / row.scheduled : null,
        latencyP50: quantile(row.latency, 0.5), latencyP95: quantile(row.latency, 0.95),
        latencyP99: quantile(row.latency, 0.99), serviceP99: quantile(row.service, 0.99) }];
    })) });
  return { admitted: false, executionAuthorized: false, quantileMethod: 'nearest-rank',
    timeUnit: 'milliseconds', latencyAttribution: 'scheduled-arrival-cohort-including-drain',
    throughputAttribution: 'successful-completion-in-half-open-window-excluding-drain',
    phase: finish(phase), windows: windows.map(finish) };
}
