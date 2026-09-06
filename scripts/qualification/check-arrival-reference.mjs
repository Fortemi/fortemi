import { createServer } from 'node:http';
import { randomBytes } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { runLoadSchedule } from './run-load-schedule.mjs';

const exec = promisify(execFile);
const integer = value => Number.isSafeInteger(value) && value >= 0;
function policy({ rate = 2, durationSeconds = 2 } = {}) {
  if (!integer(rate) || rate < 1 || rate > 2 || !integer(durationSeconds) || durationSeconds < 1 || durationSeconds > 3) {
    throw new Error('reference rate must be 1..2 and durationSeconds 1..3');
  }
  return { rate, durationSeconds, expected: rate * durationSeconds };
}

/** Admitted count accounting only: no raw offered-count equivalence, latency
 * equivalence, saturation parity or qualification. Engine offered counts may
 * differ at the endpoint; k6 preserves raw starts and explicitly rejects IDs
 * outside Node's half-open scheduled window before sending any HTTP request.
 * k6 absence is MISSING; observed drops, failures or incomplete counts are FAIL.
 */
export function compareArrivalReference(config, node, k6, receipts) {
  const { expected } = policy(config), failures = [], missing = [];
  if (!node) missing.push('Node evidence absent');
  else if (node.abortReason || node.executionSettled !== true || !Array.isArray(node.observations)
    || node.observations.length !== expected || node.observations.some(row => row.outcome !== 'succeeded')
    || new Set(node.observations.map(row => row.id)).size !== expected
    || node.observations.some(row => !/^(0|[1-9]\d*)$/.test(row.id) || Number(row.id) >= expected)
    || node.undispatched?.length !== 0 || node.unresolved?.length !== 0) failures.push('Node arrival accounting mismatch');
  if (!k6 || ['started', 'admitted', 'boundaryRejected', 'completed', 'failed', 'dropped', 'httpRequests'].some(name => !integer(k6[name]))) {
    missing.push('k6 accounting evidence absent or incomplete');
  } else if (k6.started !== k6.completed + k6.boundaryRejected || k6.boundaryRejected > 1
    || k6.admitted !== expected || k6.completed !== expected || k6.httpRequests !== expected
    || k6.dropped !== 0 || k6.failed !== 0) failures.push('k6 arrival accounting mismatch');
  if (!Array.isArray(receipts)) missing.push('independent local request receipts absent');
  else for (const engine of ['node', 'k6']) {
    if (engine === 'k6' && !k6) continue;
    const rows = receipts.filter(row => row.engine === engine);
    if (rows.length !== expected || new Set(rows.map(row => row.id)).size !== expected
      || rows.some(row => !/^(0|[1-9]\d*)$/.test(row.id) || Number(row.id) >= expected)) failures.push(`${engine} local receipt mismatch`);
  }
  if (receipts?.some(row => !['node', 'k6'].includes(row.engine))) failures.push('unbound local receipt');
  return { status: failures.length ? 'FAIL' : missing.length ? 'MISSING' : 'PASS', expectedPerEngine: expected,
    failures, missing, admitted: false, qualificationPassed: false, scope: 'local-synthetic-admitted-arrival-accounting' };
}

/** Schedules at most twelve GETs against an ephemeral loopback-only service.
 * The service rejects requests above twenty; any boundary overrun fails comparison.
 * No configurable target URL; explicit outputDirectory retains raw evidence.
 * Uses caller-selected k6 executable without downloading or installing anything.
 */
export async function runArrivalReference(config = {}) {
  const selected = policy(config);
  if (typeof config.outputDirectory !== 'string' || !config.outputDirectory.length) throw new Error('outputDirectory required');
  const output = resolve(config.outputDirectory), summaryPath = resolve(output, 'k6-summary.json');
  await mkdir(output, { recursive: true });
  // Exclusive evidence file prevents a prior summary from satisfying this run.
  await writeFile(summaryPath, '', { flag: 'wx' });
  const receipts = [], nonce = randomBytes(12).toString('hex');
  let requests = 0;
  const server = createServer((request, response) => {
    requests++;
    const engine = request.headers['x-reference-engine'], id = request.headers['x-reference-id'];
    if (requests > 20 || request.method !== 'GET' || request.url !== `/${nonce}` || !['node', 'k6'].includes(engine)
      || typeof id !== 'string' || !/^(0|[1-9]\d*)$/.test(id)) { response.writeHead(400).end(); return; }
    receipts.push({ engine, id, receivedAt: new Date().toISOString() }); response.writeHead(204).end();
  });
  let node, k6 = null, k6Execution;
  try {
    await new Promise((resolveListen, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolveListen); });
    const url = `http://127.0.0.1:${server.address().port}/${nonce}`;
    node = await runLoadSchedule({ durationMs: selected.durationSeconds * 1000, drainMs: 1000, maxConcurrency: 2,
      maxScheduleLagMs: 1000, pollMs: 5, safetyTimeoutMs: 100, schedule: Array.from({ length: selected.expected }, (_, i) => ({
        id: String(i), operation: 'query', scheduledMs: i * 1000 / selected.rate })) }, {
      checkSafety: () => requests <= 20,
      execute: async (request, signal) => {
        const response = await fetch(url, { signal: AbortSignal.any([signal, AbortSignal.timeout(500)]), redirect: 'error',
          headers: { 'X-Reference-Engine': 'node', 'X-Reference-Id': request.id } });
        await response.arrayBuffer(); return response.status === 204 ? 'succeeded' : 'failed';
      },
    });
    try {
      const result = await exec(config.k6Path ?? 'k6', ['run', '--quiet', '--no-usage-report',
        fileURLToPath(new URL('./k6-arrival-reference.js', import.meta.url))], {
        timeout: 10000, maxBuffer: 1024 * 1024,
        // Do not inherit cloud/output/proxy configuration or unrelated credentials.
        env: { PATH: process.env.PATH ?? '/usr/bin:/bin', K6_NO_USAGE_REPORT: 'true',
          REFERENCE_URL: url, REFERENCE_RATE: String(selected.rate), REFERENCE_SECONDS: String(selected.durationSeconds), REFERENCE_SUMMARY: summaryPath },
      });
      k6Execution = { status: 'COMPLETE', stdout: result.stdout, stderr: result.stderr };
      k6 = JSON.parse(await readFile(summaryPath, 'utf8'));
    } catch (error) {
      k6Execution = { status: error.code === 'ENOENT' ? 'MISSING' : 'FAIL', code: String(error.code ?? 'invalid-summary'),
        stdout: error.stdout ?? '', stderr: error.stderr ?? '' };
    }
  } finally {
    server.closeAllConnections(); await new Promise(resolveClose => server.close(resolveClose));
  }
  const result = compareArrivalReference(selected, node, k6, receipts);
  if (k6Execution.status === 'FAIL') { result.failures.push('k6 execution failed'); result.status = 'FAIL'; }
  const report = { ...result, config: selected, node, k6, k6Execution, receipts,
    reference: 'https://grafana.com/docs/k6/latest/using-k6/scenarios/executors/constant-arrival-rate/' };
  await writeFile(resolve(output, 'comparison.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const config = JSON.parse(await readFile(process.argv[2], 'utf8'));
  const result = await runArrivalReference(config);
  console.log(JSON.stringify({ status: result.status, scope: result.scope, failures: result.failures, missing: result.missing }));
  if (result.status !== 'PASS') process.exitCode = 1;
}
