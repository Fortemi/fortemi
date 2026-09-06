import { LOAD_OPERATIONS, summarizeLoadRequests } from './load-request-summary.mjs';
export const LOAD_PHASES = Object.freeze(['smoke', 'average', 'stress', 'spike', 'soak']);
const rules = Object.freeze({
  latencyP50: { unit: 'milliseconds', operators: ['lte', 'lt'] },
  latencyP95: { unit: 'milliseconds', operators: ['lte', 'lt'] },
  latencyP99: { unit: 'milliseconds', operators: ['lte', 'lt'] },
  throughputPerSecond: { unit: 'operations/second', operators: ['gte', 'gt'] },
  errorRate: { unit: 'ratio', operators: ['lte', 'lt', 'eq'] },
});
const compare = (a, op, b) => ({ lte: a <= b, lt: a < b, gte: a >= b, gt: a > b, eq: a === b })[op];
const keysEqual = (value, keys) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).length === keys.length && keys.every(k => Object.hasOwn(value, k));

/** Request metrics only. Caller must authenticate the plan and raw evidence separately. */
export function evaluateLoadRequests(plan, evidence) {
  if (!Array.isArray(plan) || plan.length !== LOAD_PHASES.length
    || new Set(plan.map(p => p?.name)).size !== LOAD_PHASES.length
    || plan.some(p => !LOAD_PHASES.includes(p?.name))) throw new Error('all five unique phases required');
  if (!evidence || typeof evidence !== 'object' || Array.isArray(evidence)
    || Object.keys(evidence).some(name => !LOAD_PHASES.includes(name))) throw new Error('unbound phase evidence');
  let requestCount = 0, windowCount = 0;
  // Validate the complete plan before examining results. Missing evidence cannot
  // hide invalid policy in a later phase.
  for (const p of plan) {
    if (![p.durationMs, p.windowMs].every(n => Number.isSafeInteger(n) && n > 0)
      || !Number.isSafeInteger(p.drainMs) || p.drainMs < 0
      || !Number.isSafeInteger(p.durationMs + p.drainMs) || p.durationMs % p.windowMs
      || !Array.isArray(p.schedule)) throw new Error('invalid phase schedule bounds');
    requestCount += p.schedule.length; windowCount += p.durationMs / p.windowMs;
    if (requestCount > 100000 || windowCount > 1000) throw new Error('load evaluation budget exceeded');
    if (!keysEqual(p.thresholds, LOAD_OPERATIONS)) throw new Error('thresholds required for every operation');
    for (const policy of Object.values(p.thresholds)) {
      if (!keysEqual(policy, ['minimumSamples', ...Object.keys(rules)])
        || !Number.isSafeInteger(policy.minimumSamples) || policy.minimumSamples < 1) throw new Error('explicit sample floor required');
      for (const [metric, rule] of Object.entries(rules)) {
        const t = policy[metric];
        if (!keysEqual(t, ['operator', 'limit', 'unit']) || !rule.operators.includes(t.operator)
          || t.unit !== rule.unit || typeof t.limit !== 'number' || !Number.isFinite(t.limit) || t.limit < 0
          || (metric === 'throughputPerSecond' && t.limit <= 0)
          || (metric === 'errorRate' && t.limit > 1)) throw new Error(`invalid request threshold: ${metric}`);
      }
    }
    // Validate IDs, operations and timestamps even when a phase has no evidence.
    summarizeLoadRequests({ ...p, observations: [] });
  }
  const phases = [];
  for (const name of LOAD_PHASES) {
    const p = plan.find(p => p.name === name);
    if (!Object.hasOwn(evidence, name)) { phases.push({ name, status: 'MISSING', reason: 'phase-evidence-missing' }); continue; }
    let summary;
    try { summary = summarizeLoadRequests({ ...p, observations: evidence[name] }); }
    catch (error) { phases.push({ name, status: 'FAIL', reason: 'invalid-observations', diagnostic: error.message }); continue; }
    const failures = [], missing = [];
    for (const [scope, buckets] of [['phase', [summary.phase]], ['window', summary.windows]]) {
      for (const bucket of buckets) for (const operation of LOAD_OPERATIONS) {
        const row = bucket.operations[operation], policy = p.thresholds[operation];
        const cell = { scope, startMs: bucket.startMs, endMs: bucket.endMs, operation };
        if (row.coverage !== 'COMPLETE' || row.observed < policy.minimumSamples) {
          missing.push({ ...cell, reason: row.missing ? 'missing-observations' : 'insufficient-samples',
            scheduled: row.scheduled, observed: row.observed, required: policy.minimumSamples });
          // Partial measurements cannot produce a passing comparison.
          continue;
        }
        for (const metric of Object.keys(rules)) {
          const t = policy[metric], actual = row[metric];
          if (!Number.isFinite(actual) || !compare(actual, t.operator, t.limit)) {
            failures.push({ ...cell, metric, actual, ...t });
          }
        }
      }
    }
    // Retain both kinds of finding. A failure never erases missing coverage.
    phases.push({ name, status: failures.length ? 'FAIL' : missing.length ? 'MISSING' : 'PASS', failures, missing });
  }
  return { admitted: false, executionAuthorized: false, scope: 'request-metrics-only',
    requestChecksPass: phases.every(p => p.status === 'PASS'), phases };
}
