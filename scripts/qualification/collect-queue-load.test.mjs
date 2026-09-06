import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { parseQueueLoad, collectQueueLoad } from './collect-queue-load.mjs';
import { createLoadApiTransport } from './load-api-transport.mjs';
const fixture = () => ({ pending: 2, delayed: 3, processing: 1, completed_last_hour: 4, failed_last_hour: 1, dead: 2, incompatible: 1, total: 15 });
test('keeps active backlog, terminal failures and incompatibility separate', () => {
  const r = parseQueueLoad(fixture()); assert.equal(r.queueDepth, 6); assert.equal(r.dead, 2); assert.equal(r.incompatible, 1);
  assert.deepEqual(r.missingMetrics, ['queueOldestSeconds']);
});
for (const [name, mutate] of [
  ['omitted delayed retries', x => delete x.delayed], ['negative count', x => x.pending = -1],
  ['fractional count', x => x.processing = 1.5], ['unsafe integer', x => x.total = 2 ** 53],
  ['overcounted components', x => x.total = 12], ['failed subset exceeds dead', x => x.failed_last_hour = 3],
  ['unknown schema field', x => x.oldest_age = 0],
]) test(`rejects ${name}`, () => { const x = fixture(); mutate(x); assert.throws(() => parseQueueLoad(x)); });
test('collects via scoped JSON transport, without inventing queue age', async t => {
  const server = http.createServer((req, res) => {
    assert.equal(req.url, '/api/v1/jobs/stats'); res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(fixture()));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const api = createLoadApiTransport({ origin: `http://127.0.0.1:${server.address().port}`, token: 'synthetic', memory: 'synthetic',
    allowedRequests: [{ method: 'GET', path: '/api/v1/jobs/stats' }], maxRequestBytes: 1024, maxResponseBytes: 1024, timeoutMs: 1000, observe: async () => {} });
  const r = await collectQueueLoad(api); assert.equal(r.queueDepth, 6); assert.equal(r.admitted, false);
  assert.ok(BigInt(r.endNs) >= BigInt(r.startNs));
});
