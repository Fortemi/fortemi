/** Offline comparison of resource observations; not a collector or execution gate. */
export const LOAD_TELEMETRY = Object.freeze({
  rssBytes: ['bytes', 'upper'], cpuPercent: ['percent', 'upper'],
  storageGrowthBytes: ['bytes', 'upper'], walGrowthBytes: ['bytes', 'upper'],
  walGeneratedBytes: ['bytes', 'counter'], blobGrowthBytes: ['bytes', 'upper'],
  queueDepth: ['count', 'upper'], queueOldestSeconds: ['seconds', 'upper'],
  poolUtilizationPercent: ['percent', 'upper'], poolTimeouts: ['count', 'counter'],
  lockWaitSeconds: ['seconds', 'upper'], deadlocks: ['count', 'counter'],
  indexFreshnessSeconds: ['seconds', 'upper'], providerCalls: ['count', 'counter'],
  providerCost: ['USD', 'counter'], freeBytes: ['bytes', 'lower'], freeInodes: ['count', 'lower'],
});
const exactKeys = (o, keys) => o && typeof o === 'object' && !Array.isArray(o)
  && Object.keys(o).length === keys.length && keys.every(k => Object.hasOwn(o, k));
const positiveInteger = n => Number.isSafeInteger(n) && n > 0;
const compare = (value, op, limit) => ({ lte: value <= limit, lt: value < limit,
  gte: value >= limit, gt: value > limit, eq: value === limit })[op];

/** Policy and observations must later be bound to the independently approved run. */
export function evaluateLoadTelemetry(policy, frames) {
  if (!policy || !positiveInteger(policy.durationMs) || !positiveInteger(policy.maxGapMs)
    || !positiveInteger(policy.maxAgeMs) || policy.maxGapMs > policy.durationMs
    || policy.maxAgeMs > policy.maxGapMs || !exactKeys(policy.thresholds, Object.keys(LOAD_TELEMETRY))) {
    throw new Error('complete bounded telemetry policy required');
  }
  for (const [metric, [unit, direction]] of Object.entries(LOAD_TELEMETRY)) {
    const t = policy.thresholds[metric];
    const operators = direction === 'lower' ? ['gte', 'gt'] : ['lte', 'lt', 'eq'];
    if (!exactKeys(t, ['operator', 'limit', 'unit']) || t.unit !== unit || !operators.includes(t.operator)
      || typeof t.limit !== 'number' || !Number.isFinite(t.limit) || t.limit < 0
      || (direction === 'lower' && t.limit === 0)
      || (['bytes', 'count'].includes(unit) && !Number.isSafeInteger(t.limit))
      || (unit === 'percent' && t.limit > 100)) throw new Error(`invalid telemetry threshold: ${metric}`);
  }
  if (!Array.isArray(frames) || frames.length > 10000) throw new Error('telemetry frame budget exceeded');
  const failures = [], missing = [], last = new Map();
  let failureCount = 0, missingCount = 0, previousTime = -1;
  const fail = detail => { failureCount++; if (failures.length < 1000) failures.push(detail); };
  const absent = detail => { missingCount++; if (missing.length < 1000) missing.push(detail); };
  if (!frames.length || frames[0]?.timeMs !== 0) absent({ reason: 'initial-frame-missing' });
  for (const frame of frames) {
    if (!frame || !Number.isSafeInteger(frame.timeMs) || frame.timeMs < 0 || frame.timeMs > policy.durationMs
      || frame.timeMs <= previousTime || !frame.values || typeof frame.values !== 'object' || Array.isArray(frame.values)
      || Object.keys(frame.values).some(m => !Object.hasOwn(LOAD_TELEMETRY, m))) throw new Error('invalid telemetry frame');
    if (previousTime >= 0 && frame.timeMs - previousTime > policy.maxGapMs) absent({ timeMs: frame.timeMs, reason: 'frame-gap' });
    previousTime = frame.timeMs;
    for (const [metric, [unit, direction]] of Object.entries(LOAD_TELEMETRY)) {
      const v = Object.hasOwn(frame.values, metric) ? frame.values[metric] : undefined;
      const detail = { timeMs: frame.timeMs, metric };
      if (v === undefined) { absent({ ...detail, reason: 'metric-missing' }); continue; }
      if (!exactKeys(v, ['value', 'unit', 'observedMs']) || v.unit !== unit
        || typeof v.value !== 'number' || !Number.isFinite(v.value) || v.value < 0
        || (['bytes', 'count'].includes(unit) && !Number.isSafeInteger(v.value))
        || (unit === 'percent' && v.value > 100)
        || !Number.isSafeInteger(v.observedMs) || v.observedMs < 0 || v.observedMs > frame.timeMs) {
        fail({ ...detail, reason: 'invalid-observation' }); continue;
      }
      if (frame.timeMs - v.observedMs > policy.maxAgeMs) absent({ ...detail, reason: 'stale-observation' });
      const prior = last.get(metric);
      if (prior && (v.observedMs < prior.observedMs || (v.observedMs === prior.observedMs && v.value !== prior.value))) {
        fail({ ...detail, reason: 'inconsistent-observation-clock' });
      }
      if (prior && direction === 'counter' && v.value < prior.value) fail({ ...detail, reason: 'counter-reset' });
      last.set(metric, v);
      const t = policy.thresholds[metric];
      if (!compare(v.value, t.operator, t.limit)) fail({ ...detail, reason: 'threshold-breach', actual: v.value, ...t });
    }
  }
  if (previousTime !== policy.durationMs) absent({ reason: 'terminal-frame-missing' });
  return { admitted: false, executionAuthorized: false, scope: 'resource-observations-only',
    status: failureCount ? 'FAIL' : missingCount ? 'MISSING' : 'PASS',
    telemetryChecksPass: !failureCount && !missingCount, failureCount, missingCount,
    detailsTruncated: failureCount > failures.length || missingCount > missing.length, failures, missing };
}
