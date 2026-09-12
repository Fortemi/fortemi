import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { validateNativeOperations, validateNativeOperationsReceipt } from './verify-native-remote-operations.mjs';

const read = path => readFileSync(new URL(path, import.meta.url));
const bytes = read('../../contracts/openapi/fixtures/native-remote-operations.json');
const fixture = JSON.parse(bytes);
const script = read('./capture-native-remote-operations.mjs');
const historical = read('../../contracts/openapi/fixtures/remote-operations.json');
const receipt = JSON.parse(read('../../contracts/openapi/fixtures/native-remote-operations.receipt.json'));
const rebind = call => {
  call.rawBody = JSON.stringify(call.body);
  call.responseSha256 = createHash('sha256').update(call.rawBody).digest('hex');
};
test('published native operation capture and receipt agree', () => {
  assert.deepEqual(validateNativeOperationsReceipt(bytes, script, historical, receipt),
    {checks: 23, calls: 86, statuses: {200: 56, 201: 2, 204: 3, 400: 1, 401: 20, 404: 4}});
});
for (const [name, mutate] of [
  ['failed capture', f => { f.status = 'FAIL'; }],
  ['missing check', f => f.checks.pop()],
  ['missing request', f => f.calls.pop()],
  ['producer drift', f => { f.artifact.commit = '0'.repeat(40); }],
  ['consumer drift', f => { f.packageCommit = '0'.repeat(40); }],
  ['executable drift', f => { f.executableSha256 = '0'.repeat(64); }],
  ['no auth requirement', f => { f.health.capabilities.auth_required = false; }],
  ['dirty destination', f => { f.initialPhysicalNotes = 1; }],
  ['live API', f => { f.apiPidAbsent = false; }],
  ['unremoved install', f => { f.scratchRemoved = false; }],
  ['visible notes retained', f => { f.syntheticCleanup.visibleNotes = 2; }],
  ['missing tombstone', f => { f.syntheticCleanup.deletedNotes = 1; }],
  ['raw byte tampering', f => { f.calls[0].rawBody += ' '; }],
  ['decoded drift', f => { f.calls[0].body = {}; }],
  ['obsolete route', f => { f.calls[0].path = '/api/v1/tools/manage-note'; }],
  ['wrong method', f => { f.calls[0].method = 'PUT'; }],
  ['identity denial relabeled', f => { f.calls.find(c => c.status === 401).check = f.checks[0]; }],
  ['problem type drift', f => { const c = f.calls.find(c => c.status === 401); c.body.type = 'unknown'; rebind(c); }],
  ['create pipeline enabled', f => { f.calls.find(c => c.status === 201).requestBody.pipeline = ['embed']; }],
  ['create revision enabled', f => { f.calls.find(c => c.status === 201).requestBody.revision_mode = 'full'; }],
  ['wrong persisted create identity', f => { const c = f.calls.find(c => c.status === 201); c.body.id = f.syntheticCleanup.ids[1]; rebind(c); }],
  ['wrong star body', f => { f.calls.find(c => c.method === 'PATCH' && c.status === 200).requestBody.starred = false; }],
  ['restore query drift', f => { f.calls.find(c => c.path.includes('/restore?') && c.status === 200).path = '/api/v1/notes/other/restore'; }],
  ['fallback mislabeled', f => { f.semanticDegradation.effective_mode = 'semantic'; }],
  ['score projection drift', f => { f.searchProjection.result.hits[0].rank = -1; }],
  ['timestamp drift', f => { f.searchProjection.result.hits[0].note.createdAt = 'unknown'; }],
  ['historical fixture drift', f => { f.producerFixture.sha256 = '0'.repeat(64); }],
  ['credential material', f => { f.authorization = 'Bearer mm_key_fixture'; }],
]) test('rejects ' + name, () => {
  const changed = structuredClone(fixture); mutate(changed);
  assert.throws(() => validateNativeOperations(changed));
});
for (const [name, mutate] of [
  ['fixture digest', r => { r.fixtureSha256 = '0'.repeat(64); }],
  ['capture digest', r => { r.captureScriptSha256 = '0'.repeat(64); }],
  ['unit mismatch', r => { r.unit = 'fortemi-local-test-1000-other.service'; }],
  ['active job', r => { r.terminal.ActiveState = 'active'; }],
  ['failed exit', r => { r.terminal.ExecMainStatus = '1'; }],
  ['unbounded memory', r => { r.terminal.MemoryMax = 'infinity'; }],
  ['extra swap', r => { r.terminal.MemorySwapMax = '1'; }],
  ['missing cleanup', r => { r.cleanup.postgresRemoved = false; }],
  ['hosted denial claim', r => { r.claims.hostedRoleDenial = true; }],
  ['positive vector claim', r => { r.claims.positiveVectorRetrieval = true; }],
  ['parity claim', r => { r.claims.suiteParity = true; }],
]) test('rejects receipt ' + name, () => {
  const changed = structuredClone(receipt); mutate(changed);
  assert.throws(() => validateNativeOperationsReceipt(bytes, script, historical, changed));
});
test('rejects modified historical fixture bytes', () => {
  assert.throws(() => validateNativeOperationsReceipt(bytes, script, Buffer.from('{}'), receipt));
});
test('capture fails before setup outside the bounded runner', () => {
  const run = spawnSync(process.execPath, [new URL('./capture-native-remote-operations.mjs', import.meta.url).pathname,
    '/unused-api', '/unused-package', '/unused-output', '/unused-cache'],
  {encoding: 'utf8', timeout: 5000, env: {...process.env, FORTEMI_LOCAL_TEST_UNIT: 'invalid'}});
  assert.equal(run.status, 1); assert.match(run.stderr, /AssertionError/);
});
