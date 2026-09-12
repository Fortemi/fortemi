import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const checks = [
  'clean authenticated destination and capabilities',
  'create both synthetic notes through published adapter',
  'authenticated FTS q AND-tags and actual EnhancedSearchHit projection',
  'AND tags exclude nonmatching notes',
  'empty FTS results preserve returned-hit total',
  'bounded FTS limit and rank order',
  'semantic report explicitly retains unavailable-inference FTS fallback',
  'array-only semantic rejects fallback',
  'hybrid explicitly reports FTS degradation',
  'invalid and unsupported intents reject before HTTP',
  'actual producer rejects legacy missing-q request',
  'missing identity cannot search or dispatch advertised mutations',
  'invalid identity cannot search or dispatch advertised mutations',
  'authenticated star persists expected state',
  'authenticated unstar persists expected state',
  'authenticated archive persists expected state',
  'authenticated unarchive persists expected state',
  'authenticated content update persists through composed read',
  'authenticated tag update persists and changes search membership',
  'authenticated delete yields authoritative absence',
  'authenticated restore preserves content and identity',
  'mutation404 is typed failure not false success',
  'synthetic lifecycle cleanup verifies both tombstones and no visible notes',
];
const statuses = {200: 56, 201: 2, 204: 3, 400: 1, 401: 20, 404: 4};

export function validateNativeOperations(fixture) {
  assert.equal(fixture.schemaVersion, 'fortemi.native-remote-operations.v1');
  assert.equal(fixture.status, 'PASS');
  assert.equal(fixture.artifact.commit, 'e91c595a896275f835cb7ed1aef173cb26056206');
  assert.equal(fixture.artifact.version, '2026.9.9');
  assert.equal(fixture.artifact.kind, 'published-native-linux-amd64');
  assert.equal(fixture.artifact.sha256, '19b98d48d7c92e8e9d3d7d514fd60a8817f144926c1a4270adf6e0a052c39bc9');
  assert.equal(fixture.executableSha256, fixture.artifact.sha256);
  assert.equal(fixture.packageCommit, '9f74c0bab0cce5ef8e2433e8944a4efda5575eb4');
  assert.equal(fixture.packageVersion, '2026.9.4');
  assert.equal(fixture.packageSha256, '4a126d59bc18af4fb981d5bdbf19a761402e246b7d85cf3e6445de4eacf92897');
  assert.equal(fixture.health.capabilities.auth_required, true);
  assert.equal(fixture.initialPhysicalNotes, 0);
  assert.equal(fixture.migrations, 139);
  assert.equal(fixture.apiPidAbsent, true);
  assert.equal(fixture.apiExit.code, 0);
  assert.equal(fixture.scratchRemoved, true);
  assert.match(fixture.scope, /personal AllowAllPolicy/);
  assert.match(fixture.scope, /no positive vector retrieval/);
  assert.deepEqual(fixture.checks, checks);
  assert.equal(fixture.calls.length, 86);
  assert.equal(fixture.syntheticCleanup.visibleNotes, 0);
  assert.equal(fixture.syntheticCleanup.physicalNotes, 2);
  assert.equal(fixture.syntheticCleanup.deletedNotes, 2);
  assert.equal(fixture.syntheticCleanup.ids.length, 2);
  assert.equal(new Set(fixture.syntheticCleanup.ids).size, 2);
  for (const [status, count] of Object.entries(statuses)) {
    assert.equal(fixture.calls.filter(call => call.status === Number(status)).length, count);
  }
  for (const call of fixture.calls) {
    assert.ok(checks.includes(call.check));
    assert.ok(['GET', 'POST', 'PATCH', 'DELETE'].includes(call.method));
    assert.match(call.path, /^\/api\/v1\/(?:notes(?:[/?]|$)|search\?)/);
    assert.equal(typeof call.rawBody, 'string');
    assert.ok(Buffer.byteLength(call.rawBody) <= 1048576);
    assert.equal(hash(call.rawBody), call.responseSha256);
    let body; try { body = JSON.parse(call.rawBody); } catch { body = call.rawBody; }
    assert.deepEqual(call.body, body);
    if ([401, 404].includes(call.status)) {
      assert.equal(call.contentType, 'application/problem+json');
      assert.equal(call.body.status, call.status);
      assert.equal(call.body.type, 'https://fortemi.com/problems/' + (call.status === 401 ? 'unauthorized' : 'not-found'));
    }
  }
  const creates = fixture.calls.filter(call => call.method === 'POST' && call.path === '/api/v1/notes' && call.status === 201);
  assert.equal(creates.length, 2);
  assert.deepEqual(creates.map(call => call.body.id), fixture.syntheticCleanup.ids);
  for (const call of creates) {
    assert.equal(call.requestBody.revision_mode, 'none');
    assert.deepEqual(call.requestBody.pipeline, []);
  }
  const first = fixture.syntheticCleanup.ids[0];
  const patches = fixture.calls.filter(call => call.method === 'PATCH' && call.status === 200);
  assert.equal(patches.length, 6);
  for (const call of patches) assert.equal(call.path, '/api/v1/notes/' + first);
  assert.deepEqual(patches.slice(0, 4).map(call => call.requestBody),
    [{starred: true}, {starred: false}, {archived: true}, {archived: false}]);
  assert.deepEqual(patches[4].requestBody, {content: 'PACKAGE UPDATED', revision_mode: 'none'});
  assert.equal(patches[5].requestBody.tags.includes('updated'), true);
  assert.equal(fixture.calls.filter(call => call.method === 'POST' &&
    call.path === '/api/v1/notes/' + first + '/restore?revision_mode=none' && call.status === 200).length, 1);
  for (const identity of ['missing', 'invalid']) {
    const calls = fixture.calls.filter(call => call.check === identity + ' identity cannot search or dispatch advertised mutations' && call.status === 401);
    assert.equal(calls.length, 10);
    assert.deepEqual(calls.map(call => call.method), ['GET', 'GET', 'POST', 'PATCH', 'PATCH', 'PATCH', 'PATCH', 'PATCH', 'DELETE', 'POST']);
  }
  assert.deepEqual(fixture.semanticDegradation, {code: 'embedding_request_failed', effective_mode: 'fts'});
  const projected = fixture.searchProjection, raw = projected.raw, hit = projected.result.hits[0];
  assert.equal(hit.note.id, raw.note_id); assert.equal(hit.rank, raw.score);
  assert.equal(hit.snippet, raw.snippet); assert.equal(hit.note.title, raw.title);
  assert.deepEqual(hit.note.tags, raw.tags);
  for (const timestamp of [hit.note.createdAt, hit.note.updatedAt]) {
    assert.ok(Number.isFinite(Date.parse(timestamp))); assert.match(timestamp, /(?:Z|\+00:00)$/);
  }
  assert.equal(projected.result.totalKind, 'returned-hits');
  assert.equal(projected.result.degraded, false);
  assert.equal(fixture.producerFixture.commit, '9588dea2d20fe16086da26f078723f74c7c51327');
  assert.equal(fixture.producerFixture.sha256, 'c086a4ec2f02fb3e1c12b25c93427dbe720a4527b6eadef6cd3244b568397d52');
  assert.equal(fixture.producerFixture.historical, true);
  assert.ok(!/mm_key_|"authorization"\s*:/i.test(JSON.stringify(fixture)));
  return {checks: checks.length, calls: fixture.calls.length, statuses};
}

export function validateNativeOperationsReceipt(fixtureBytes, scriptBytes, historicalBytes, receipt) {
  assert.equal(receipt.schemaVersion, 'fortemi.native-remote-operations-receipt.v1');
  assert.equal(receipt.fixturePath, 'contracts/openapi/fixtures/native-remote-operations.json');
  assert.equal(receipt.captureScriptPath, 'scripts/ci/capture-native-remote-operations.mjs');
  assert.equal(receipt.fixtureSha256, hash(fixtureBytes));
  assert.equal(receipt.captureScriptSha256, hash(scriptBytes));
  const fixture = JSON.parse(fixtureBytes);
  assert.equal(fixture.probeSha256, receipt.captureScriptSha256);
  assert.equal(hash(historicalBytes), fixture.producerFixture.sha256);
  assert.equal(receipt.unit, fixture.boundedUnit);
  assert.equal(receipt.terminal.unit, receipt.unit);
  assert.match(receipt.unit, /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
  for (const [field, value] of Object.entries({ActiveState: 'inactive', Result: 'success',
    ExecMainStatus: '0', MemoryMax: '8589934592', MemoryHigh: '6442450944',
    MemorySwapMax: '0', CPUQuotaPerSecUSec: '2s', TasksMax: '256', RuntimeMaxUSec: '5min'})) {
    assert.equal(receipt.terminal[field], value);
  }
  for (const field of ['cgroupAbsent', 'apiPidAbsent', 'postmasterPidAbsent', 'postgresStopped', 'postgresRemoved']) {
    assert.equal(receipt.cleanup[field], true);
  }
  for (const field of ['realHttp', 'publishedConsumer', 'personalRequiredAuthentication', 'allAdvertisedMutations']) assert.equal(receipt.claims[field], true);
  for (const field of ['hostedRoleDenial', 'positiveVectorRetrieval', 'suiteParity']) assert.equal(receipt.claims[field], false);
  return validateNativeOperations(fixture);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = new URL('../../', import.meta.url);
  const result = validateNativeOperationsReceipt(
    readFileSync(new URL('contracts/openapi/fixtures/native-remote-operations.json', root)),
    readFileSync(new URL('scripts/ci/capture-native-remote-operations.mjs', root)),
    readFileSync(new URL('contracts/openapi/fixtures/remote-operations.json', root)),
    JSON.parse(readFileSync(new URL('contracts/openapi/fixtures/native-remote-operations.receipt.json', root))));
  console.log(JSON.stringify({status: 'PASS', ...result}));
}
