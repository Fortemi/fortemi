import http from 'k6/http';
import execution from 'k6/execution';
import { Counter } from 'k6/metrics';

// Local synthetic reference only. Configuration is bounded again in this process.
const rate = Number(__ENV.REFERENCE_RATE), seconds = Number(__ENV.REFERENCE_SECONDS);
if (![rate, seconds].every(Number.isInteger) || rate < 1 || rate > 2 || seconds < 1 || seconds > 3
  || !/^http:\/\/127\.0\.0\.1:\d+\/[a-f0-9]+$/.test(__ENV.REFERENCE_URL)) throw new Error('invalid local reference configuration');
const started = new Counter('reference_started'), completed = new Counter('reference_completed');
const admitted = new Counter('reference_admitted'), boundaryRejected = new Counter('reference_boundary_rejected');
const failed = new Counter('reference_failed');
export const options = { scenarios: { reference: { executor: 'constant-arrival-rate', rate, timeUnit: '1s',
  duration: `${seconds}s`, preAllocatedVUs: 2, maxVUs: 2, gracefulStop: '1s' } },
  maxRedirects: 0, discardResponseBodies: true };
export default function () {
  started.add(1);
  // k6 may offer an iteration exactly at the duration endpoint. Keep that raw
  // offer visible, but dispatch only the explicitly selected half-open IDs.
  boundaryRejected.add(0);
  if (execution.scenario.iterationInTest >= rate * seconds) { boundaryRejected.add(1); return; }
  admitted.add(1);
  const response = http.get(__ENV.REFERENCE_URL, { timeout: '500ms', headers: {
    'X-Reference-Engine': 'k6', 'X-Reference-Id': String(execution.scenario.iterationInTest) } });
  completed.add(1); failed.add(response.status === 204 ? 0 : 1);
}
export function handleSummary(data) {
  const count = name => data.metrics[name]?.values?.count;
  return { [__ENV.REFERENCE_SUMMARY]: JSON.stringify({ started: count('reference_started'),
    admitted: count('reference_admitted'), boundaryRejected: count('reference_boundary_rejected'),
    completed: count('reference_completed'), failed: count('reference_failed'),
    dropped: count('dropped_iterations') ?? 0, httpRequests: count('http_reqs'), raw: data }, null, 2) };
}
