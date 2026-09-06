const fields = ['pending', 'delayed', 'processing', 'completed_last_hour', 'failed_last_hour', 'dead', 'incompatible', 'total'];

/** Decode the current QueueStats response; unknown or inconsistent revisions reject. */
export function parseQueueLoad(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length !== fields.length
    || fields.some(k => !Object.hasOwn(value, k) || !Number.isSafeInteger(value[k]) || value[k] < 0)) throw new Error('invalid queue statistics');
  const active = BigInt(value.pending) + BigInt(value.delayed) + BigInt(value.processing);
  const accounted = active + BigInt(value.dead) + BigInt(value.incompatible) + BigInt(value.completed_last_hour);
  if (accounted > BigInt(value.total) || value.failed_last_hour > value.dead) throw new Error('inconsistent queue counts');
  return { queueDepth: Number(active), incompatible: value.incompatible, dead: value.dead,
    snapshot: { ...value }, missingMetrics: ['queueOldestSeconds'] };
}

/** Scope is the existing API's job repository, not inferred tenant membership. */
export async function collectQueueLoad(apiRequest, signal) {
  if (typeof apiRequest !== 'function') throw new Error('explicit API adapter required');
  const startNs = process.hrtime.bigint().toString();
  const raw = await apiRequest('GET', '/api/v1/jobs/stats', null, { signal });
  return { admitted: false, executionAuthorized: false, startNs, endNs: process.hrtime.bigint().toString(),
    scope: 'api-job-repository', ...parseQueueLoad(raw) };
}
