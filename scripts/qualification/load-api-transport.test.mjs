import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { createLoadApiTransport } from './load-api-transport.mjs';
async function fixture(t, handler) {
  const server = http.createServer(handler); await new Promise(r => server.listen(0, '127.0.0.1', r));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const observed = [];
  const config = { origin: `http://127.0.0.1:${server.address().port}`, token: 'test-secret', memory: 'test-namespace',
    allowedRequests: [{ method: 'POST', path: '/api/v1/notes/source-upsert' }, { method: 'GET', path: '/api/v1/search?q=test' }],
    maxRequestBytes: 100, maxResponseBytes: 100, timeoutMs: 1000, observe: async o => observed.push(o) };
  return { config, observed, api: createLoadApiTransport(config) };
}
test('sends explicit auth/namespace and bounded metadata without exposing token or body', async t => {
  const f = await fixture(t, (req, res) => {
    assert.equal(req.headers.authorization, 'Bearer test-secret'); assert.equal(req.headers['x-fortemi-memory'], 'test-namespace');
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{"accepted":true}');
  });
  assert.deepEqual(await f.api('POST', '/api/v1/notes/source-upsert', { synthetic: true }), { accepted: true });
  assert.equal(f.observed.length, 1); assert.match(f.observed[0].responseDigest, /^sha256:[a-f0-9]{64}$/);
  assert.ok(!JSON.stringify(f.observed).includes('test-secret'));
});
test('oversized or unapproved requests fail before network mutation', async t => {
  let calls = 0; const f = await fixture(t, (_, res) => { calls++; res.end(); });
  for (const args of [['POST', '/api/v1/notes/source-upsert', { x: 'a'.repeat(100) }],
    ['DELETE', '/api/v1/notes/source-upsert'], ['GET', '//example.invalid/path'], ['GET', '/api/v1/../admin']]) {
    await assert.rejects(f.api(...args));
  }
  assert.equal(calls, 0);
});
test('redirect is not followed', async t => {
  let calls = 0; const f = await fixture(t, (_, res) => { calls++; res.writeHead(302, { Location: '/redirect-target' }); res.end(); });
  await assert.rejects(f.api('GET', '/api/v1/search?q=test'), { code: 'REDIRECT_REJECTED' }); assert.equal(calls, 1);
});
test('streamed response bound is enforced without Content-Length', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.write('x'.repeat(101)); res.end(); });
  await assert.rejects(f.api('GET', '/api/v1/search?q=test'), { code: 'RESPONSE_BYTES_EXCEEDED' });
});
test('HTTP rejection retains status without leaking response content', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(403, { 'Content-Type': 'application/json' }); res.end('{"secret":"private"}'); });
  await assert.rejects(f.api('GET', '/api/v1/search?q=test'), e => e.code === 'HTTP_REJECTED' && e.status === 403 && !e.message.includes('private'));
});
test('caller abort cancels an unresolved response', async t => {
  const f = await fixture(t, () => {}), c = new AbortController();
  const request = f.api('GET', '/api/v1/search?q=test', null, { signal: c.signal }); c.abort();
  await assert.rejects(request, { code: 'REQUEST_ABORTED' });
});
test('malformed JSON and failed evidence observer cannot become success', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('not-json'); });
  await assert.rejects(f.api('GET', '/api/v1/search?q=test'), { code: 'INVALID_JSON_RESPONSE' });
  const api = createLoadApiTransport({ ...f.config, observe: async () => { throw Error('private observer detail'); } });
  await assert.rejects(api('GET', '/api/v1/search?q=test'), { code: 'TRANSPORT_OR_OBSERVER_FAILED' });
});
test('uses the existing dataset controller across the HTTP transport', async t => {
  const { readFileSync } = await import('node:fs');
  const { createDatasetExecutionController, previewDatasetExecution, sha256Digest } = await import('../../mcp-server/lib/dataset-execution.js');
  const input = JSON.parse(readFileSync(new URL('../../contracts/dataset-execution/1.0.0/fixtures/supported-request.json', import.meta.url)));
  let calls = 0;
  const f = await fixture(t, (_, res) => {
    calls++; res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ contract_version: '1.0.0', import_run_id: input.runId,
      batch_id: previewDatasetExecution(input).requestDigest, dry_run: false, outcome: 'committed',
      checkpoint: { contract: input.batch.checkpointAfter.contract, schemaVersion: input.batch.checkpointAfter.schemaVersion,
        opaque: input.batch.checkpointAfter.opaque, sequence: input.batch.checkpointAfter.sequence, planDigest: input.plan.planDigest },
      counts: { inserted: 1, unchanged: 0, versioned: 0, replaced: 0, conflict: 0, rejected: 0 },
      items: [{ index: 0, outcome: 'inserted', note_id: '018fd1a0-0000-7000-8000-000000001130',
        external_id_hash: sha256Digest('record-1'), content_digest: input.batch.mutations[0].digest }] }));
  });
  const controller = createDatasetExecutionController({ apiRequest: createLoadApiTransport({ ...f.config,
    maxRequestBytes: 100000, maxResponseBytes: 100000 }) });
  const result = await controller.handle({ ...input, action: 'execute' });
  assert.equal(calls, 1); assert.equal(result.verification, 'verified', JSON.stringify(result));
});
test('evidence observer cannot hang past transport timeout', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{}'); });
  const api = createLoadApiTransport({ ...f.config, timeoutMs: 30, observe: () => new Promise(() => {}) });
  await assert.rejects(api('GET', '/api/v1/search?q=test'), { code: 'REQUEST_ABORTED' });
});
test('preserves archive bytes and loss header, then carries existing base64 JSON import shape', async t => {
  const { gzipSync } = await import('node:zlib'); const archive = gzipSync('synthetic archive fixture');
  let imported = false;
  const f = await fixture(t, (req, res) => {
    if (req.method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'application/gzip', 'X-Fortemi-Shard-Loss-Report': '{"synthetic":true}' }); res.end(archive);
    } else {
      const chunks = []; req.on('data', d => chunks.push(d)); req.on('end', () => {
        const body = JSON.parse(Buffer.concat(chunks)); assert.deepEqual(Buffer.from(body.shard_base64, 'base64'), archive);
        imported = true; res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{}');
      });
    }
  });
  const api = createLoadApiTransport({ ...f.config, maxRequestBytes: 1000, maxResponseBytes: 1000,
    allowedRequests: [{ method: 'GET', path: '/api/v1/backup/knowledge-shard', responseKind: 'gzip-archive' },
      { method: 'POST', path: '/api/v1/backup/knowledge-shard/import' }] });
  const r = await api('GET', '/api/v1/backup/knowledge-shard'); assert.deepEqual(r.bytes, archive);
  assert.equal(r.shardLossReport, '{"synthetic":true}');
  await api('POST', '/api/v1/backup/knowledge-shard/import', { shard_base64: r.bytes.toString('base64') }); assert.equal(imported, true);
});
test('archive handling cannot be enabled by call options for a JSON-only route', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, { 'Content-Type': 'application/gzip' }); res.end('binary'); });
  await assert.rejects(f.api('GET', '/api/v1/search?q=test', null, { responseKind: 'gzip-archive' }), { code: 'JSON_RESPONSE_REQUIRED' });
});
for (const [headers, code] of [
  [{ 'Content-Type': 'application/json' }, 'ARCHIVE_MEDIA_TYPE_REQUIRED'],
  [{ 'Content-Type': 'application/gzip', 'Content-Encoding': 'gzip' }, 'ARCHIVE_CONTENT_ENCODING_REJECTED'],
  [{ 'Content-Type': 'application/gzip', 'X-Fortemi-Shard-Loss-Report': 'x'.repeat(4097) }, 'ARCHIVE_HEADER_LIMIT_EXCEEDED'],
]) test(`archive rejects ${code}`, async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, headers); res.end(); });
  const api = createLoadApiTransport({ ...f.config, allowedRequests: [{ method: 'GET', path: '/api/v1/backup/knowledge-shard', responseKind: 'gzip-archive' }] });
  await assert.rejects(api('GET', '/api/v1/backup/knowledge-shard'), { code });
});
test('archive streams remain subject to the response byte cap', async t => {
  const f = await fixture(t, (_, res) => { res.writeHead(200, { 'Content-Type': 'application/gzip' }); res.write(Buffer.alloc(101)); res.end(); });
  const api = createLoadApiTransport({ ...f.config, allowedRequests: [{ method: 'GET', path: '/api/v1/backup/knowledge-shard', responseKind: 'gzip-archive' }] });
  await assert.rejects(api('GET', '/api/v1/backup/knowledge-shard'), { code: 'RESPONSE_BYTES_EXCEEDED' });
});

test('observes network failures and invalid responses exactly once with explicit attempt identities', async t => {
  for (const [handler, errorCode] of [
    [(req) => req.socket.destroy(), 'TRANSPORT_OR_OBSERVER_FAILED'],
    [(_, res) => { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('invalid'); }, 'INVALID_JSON_RESPONSE'],
    [(_, res) => { res.writeHead(302, { Location: '/elsewhere' }); res.end(); }, 'REDIRECT_REJECTED'],
  ]) {
    const f = await fixture(t, handler);
    await assert.rejects(f.api('GET', '/api/v1/search?q=test', null,
      { logicalOperationId: 'logical-1', requestId: 'request-1', attemptId: 'attempt-1' }), { code: errorCode });
    assert.equal(f.observed.length, 1);
    assert.equal(f.observed[0].errorCode, errorCode);
    assert.equal(f.observed[0].outcome, 'failed');
    assert.equal(f.observed[0].attemptId, 'attempt-1');
  }
});
test('aborted dispatched fetch is observed, pre-aborted calls are not dispatched', async t => {
  const f = await fixture(t, () => {}), c = new AbortController();
  const request = f.api('GET', '/api/v1/search?q=test', null, { signal: c.signal }); c.abort();
  await assert.rejects(request, { code: 'REQUEST_ABORTED' });
  assert.equal(f.observed.length, 1); assert.equal(f.observed[0].errorCode, 'REQUEST_ABORTED');
  await assert.rejects(f.api('GET', '/api/v1/search?q=test', null, { signal: c.signal }), { code: 'REQUEST_ABORTED' });
  assert.equal(f.observed.length, 1);
});
test('budget admission bounds failed attempts before further network dispatch', async t => {
  const { createLoadAttemptBudget } = await import('./load-attempt-budget.mjs');
  let calls = 0;
  const f = await fixture(t, (_, res) => { calls++; res.writeHead(503); res.end(); });
  const budget = createLoadAttemptBudget({ maxLogicalOperations: 1, maxRequests: 1, maxAttempts: 2, maxCostUsd: '0.2' });
  const api = createLoadApiTransport({ ...f.config, attemptBudget: budget });
  const options = { logicalOperationId: 'operation-1', requestId: 'request-1', reserveCostUsd: '0.1' };
  for (let n = 1; n <= 2; n++) await assert.rejects(api('GET', '/api/v1/search?q=test', null, { ...options, attemptId: `attempt-${n}` }), { code: 'HTTP_REJECTED' });
  await assert.rejects(api('GET', '/api/v1/search?q=test', null, { ...options, attemptId: 'attempt-3' }), { code: 'ATTEMPT_BUDGET_EXCEEDED' });
  assert.equal(calls, 2); assert.equal(f.observed.length, 2);
  assert.equal(budget.snapshot().chargedCostUsd, '0.2');
  assert.equal(budget.snapshot().unresolvedAttempts, 2);
});
