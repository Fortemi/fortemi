import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createLoadWorkload } from './load-workload.mjs';
import { sha256Digest } from '../../mcp-server/lib/dataset-execution.js';
const input = () => JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-execution/1.0.0/fixtures/supported-request.json', import.meta.url)));
const noteId = '018fd1a0-0000-7000-8000-000000001130';
const evidenceDigest = `sha256:${'a'.repeat(64)}`;
function fixture() {
  const i = input(), records = [], calls = [];
  const fixtures = [ { id: 'i', operation: 'ingest', input: i }, { id: 'r', operation: 'retry', input: i },
    { id: 's', operation: 'status', runId: i.runId }, { id: 'c', operation: 'cancellation', runId: i.runId },
    { id: 'q', operation: 'query', path: '/api/v1/search?q=synthetic' },
    { id: 'l', operation: 'lineage', path: `/api/v1/notes/${noteId}/provenance` },
    { id: 'm', operation: 'materialization', noteId, body: {} },
    { id: 'e', operation: 'export', path: '/api/v1/backup/knowledge-shard?profile=core-v1' },
    { id: 'b', operation: 'import', profile: 'core-v1', body: { shard_base64: 'c3ludGhldGlj' } } ];
  const options = { fixtures, runtimeVersion: '2026.9.3', recordObservation: async o => records.push(o),
    verifyOutcome: async () => ({ verified: true, outcome: 'succeeded', evidenceDigest }),
    apiRequest: async (method, path, body) => {
      calls.push({ method, path, body });
      if (path.endsWith('source-upsert')) return { contract_version: '1.0.0', import_run_id: i.runId,
        batch_id: body.batch_id, dry_run: false, outcome: 'committed', checkpoint: body.checkpoint,
        counts: { inserted: 1, unchanged: 0, versioned: 0, replaced: 0, conflict: 0, rejected: 0 },
        items: [{ index: 0, outcome: 'inserted', note_id: noteId, external_id_hash: sha256Digest('record-1'), content_digest: i.batch.mutations[0].digest }] };
      return { synthetic: true };
    } };
  return { fixtures, options, records, calls };
}
test('routes all nine operations through existing interfaces and retains observations before verdicts', async () => {
  const f = fixture(), run = createLoadWorkload(f.options);
  for (const request of f.fixtures) assert.equal(await run(request), 'succeeded');
  assert.equal(f.records.length, 18); assert.equal(f.calls.filter(c => c.path.endsWith('source-upsert')).length, 1);
  assert.equal(f.records[0].result.verification, 'verified');
  assert.equal(f.records[2].result.receipt.requestDigest, f.records[0].result.receipt.requestDigest);
  assert.equal(f.calls.find(c => c.path.endsWith('/reprocess')).method, 'POST');
});
test('HTTP success without independent verified evidence is ambiguous', async () => {
  const f = fixture(); f.options.verifyOutcome = async () => ({ verified: false, outcome: 'succeeded', evidenceDigest });
  assert.equal(await createLoadWorkload(f.options)(f.fixtures[4]), 'ambiguous');
});
test('wrong operation labels cannot turn status into a query measurement', async () => {
  const f = fixture(); await assert.rejects(createLoadWorkload(f.options)({ id: 's', operation: 'query' })); assert.equal(f.calls.length, 0);
});
test('configured fixtures cannot be rewritten after construction', async () => {
  const f = fixture(), run = createLoadWorkload(f.options); f.fixtures[4].path = '/api/v1/jobs/pause';
  await run({ id: 'q', operation: 'query' }); assert.equal(f.calls[0].path, '/api/v1/search?q=synthetic');
});
test('raw evidence failure prevents a successful classification', async () => {
  const f = fixture(); let verified = false;
  f.options.recordObservation = async () => { throw Error('evidence unavailable'); };
  f.options.verifyOutcome = async () => { verified = true; };
  await assert.rejects(createLoadWorkload(f.options)(f.fixtures[4])); assert.equal(verified, false);
});
test('backend errors are redacted and still require independent classification', async () => {
  const f = fixture(); f.options.apiRequest = async () => { throw Error('secret body'); };
  f.options.verifyOutcome = async ({ observation }) => { assert.equal(observation.failure.code, 'WORKLOAD_EXECUTION_ERROR'); return { verified: false }; };
  assert.equal(await createLoadWorkload(f.options)(f.fixtures[4]), 'ambiguous'); assert.ok(!JSON.stringify(f.records).includes('secret body'));
});
test('unsupported routes and unnamed shard profile reject at construction', () => {
  for (const change of [f => f.fixtures[4].path = '/api/v1/jobs/pause', f => f.fixtures[7].path = '/api/v1/backup/knowledge-shard',
    f => f.fixtures[5].path = `/api/v1/notes/${noteId}`, f => f.fixtures[8].profile = 'generic']) {
    const f = fixture(); change(f); assert.throws(() => createLoadWorkload(f.options));
  }
});
test('unverified verdicts are retained and malformed evidence cannot count as success', async () => {
  for (const verdict of [undefined, { verified: false }, { verified: true, outcome: 'succeeded', evidenceDigest: 'missing' },
    { verified: true, outcome: 'PASS', evidenceDigest }]) {
    const f = fixture(); f.options.verifyOutcome = async () => verdict;
    assert.equal(await createLoadWorkload(f.options)(f.fixtures[4]), 'ambiguous');
    assert.equal(f.records.length, 2);
    assert.deepEqual(f.records[1].verification, verdict);
  }
});
test('failure to retain the verifier verdict prevents successful classification', async () => {
  const f = fixture(); let count = 0;
  f.options.recordObservation = async () => { if (++count === 2) throw Error('write failed'); };
  await assert.rejects(createLoadWorkload(f.options)(f.fixtures[4]), /write failed/);
});
test('aborted requests do not reach the runtime', async () => {
  const f = fixture(), controller = new AbortController(); controller.abort();
  assert.equal(await createLoadWorkload(f.options)(f.fixtures[0], controller.signal), 'cancelled');
  assert.equal(f.calls.length, 0);
});
test('concurrent execution retains each request abort signal', async () => {
  const f = fixture(), ingest = new AbortController(), query = new AbortController();
  const originalApi = f.options.apiRequest, seen = [];
  f.options.apiRequest = async (method, path, body, options) => {
    seen.push({ path, signal: options.signal });
    await new Promise(resolve => setImmediate(resolve));
    return originalApi(method, path, body);
  };
  const run = createLoadWorkload(f.options);
  await Promise.all([run(f.fixtures[0], ingest.signal), run(f.fixtures[4], query.signal)]);
  ingest.abort();
  assert.equal(seen.find(c => c.path.endsWith('source-upsert')).signal.aborted, true);
  assert.equal(seen.find(c => c.path.startsWith('/api/v1/search')).signal.aborted, false);
});
