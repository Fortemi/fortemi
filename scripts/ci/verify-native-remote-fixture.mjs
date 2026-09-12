import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const checks = [
  'empty authenticated list', 'nonempty authenticated list', 'note identity UTC tags',
  'directional relationship reads', 'composed existing note remains accessible',
  'seeded nonempty provenance and revised content', 'authenticated authoritative not-found',
  'missing identity', 'invalid identity',
  'personal AllowAllPolicy permits authenticated MCP-scoped note read',
  'producer operator inventory denies non-admin identity',
  'real producer500 from reversible private database fault',
  'real rate limit429 reaches installed consumer',
];
const problems = new Map([[401, 'unauthorized'], [403, 'forbidden'], [404, 'not-found'], [429, 'rate-limit-exceeded'], [500, 'internal-error']]);

export function validateNativeRemoteFixture(fixture) {
  assert.equal(fixture.schemaVersion, 'fortemi.native-remote-fixture.v1');
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
  assert.equal(fixture.syntheticPhysicalNotesBeforeClusterRemoval, 2);
  assert.equal(fixture.apiPidAbsent, true);
  assert.equal(fixture.apiExit.code, 0);
  assert.equal(fixture.scratchRemoved, true);
  assert.equal(fixture.databaseFaultRestored, true);
  assert.match(fixture.scope, /personal AllowAllPolicy/);
  assert.match(fixture.denialBoundary, /producer-only operator inventory, not Core getNote\/getNoteFull denial/);
  assert.match(fixture.boundedUnit, /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
  assert.deepEqual(fixture.checks, checks);
  assert.ok(Array.isArray(fixture.calls) && fixture.calls.length >= 30 && fixture.calls.length <= 128, 'bounded call inventory');
  const counts = {};
  for (const call of fixture.calls) {
    assert.ok(checks.includes(call.check) || call.check === 'fixture setup', 'named capture group');
    assert.ok(['GET', 'POST'].includes(call.method), 'read fixture method inventory');
    assert.match(call.path, /^\/api\/v1\/(notes(?:[/?]|$)|operator\/openapi\.yaml$)/);
    assert.equal(typeof call.rawBody, 'string');
    assert.ok(Buffer.byteLength(call.rawBody) <= 1048576, 'bounded response');
    assert.equal(hash(call.rawBody), call.responseSha256, 'raw response digest');
    assert.ok(JSON.stringify(JSON.parse(call.rawBody)) === JSON.stringify(call.body), 'raw/decoded response agreement');
    assert.ok([200, 201, ...problems.keys()].includes(call.status), 'known captured status');
    counts[call.status] = (counts[call.status] ?? 0) + 1;
    if (problems.has(call.status)) {
      assert.equal(call.contentType, 'application/problem+json');
      assert.equal(call.body.status, call.status);
      assert.equal(call.body.type, 'https://fortemi.com/problems/' + problems.get(call.status));
      for (const field of ['title', 'detail', 'request_id']) assert.ok(typeof call.body[field] === 'string' && call.body[field].length > 0 && call.body[field].length <= 256, 'bounded problem field');
      if (call.status === 403) {
        assert.equal(call.path, '/api/v1/operator/openapi.yaml', '403 is operator-only, not note denial');
        assert.equal(call.check, 'producer operator inventory denies non-admin identity');
      } else assert.match(call.path, /^\/api\/v1\/notes\/[a-f0-9-]{36}$/);
      if (call.status === 429) assert.match(call.retryAfter ?? '', /^[1-9][0-9]*$/);
      if (call.status === 500) assert.equal(call.body.detail, 'An internal error occurred.');
    }
  }
  for (const status of [401, 404, 429, 500]) assert.ok(counts[status] >= 2, 'both public read methods need error capture');
  assert.equal(counts[403], 1);
  assert.ok(!/mm_key_|"authorization"\s*:/i.test(JSON.stringify(fixture)), 'no authentication material');
  return { checks: checks.length, calls: fixture.calls.length, statuses: counts };
}

export function validateNativeRemoteReceipt(fixtureBytes, scriptBytes, receipt) {
  assert.equal(receipt.schemaVersion, 'fortemi.native-remote-receipt.v1');
  assert.equal(receipt.fixturePath, 'contracts/openapi/fixtures/native-remote-auth.json');
  assert.equal(receipt.fixtureSha256, hash(fixtureBytes), 'fixture receipt digest');
  assert.equal(receipt.captureScriptSha256, hash(scriptBytes), 'capture script receipt digest');
  const fixture = JSON.parse(fixtureBytes);
  assert.equal(fixture.probeSha256, receipt.captureScriptSha256);
  assert.equal(receipt.unit, fixture.boundedUnit);
  assert.equal(receipt.terminal.ActiveState, 'inactive');
  assert.equal(receipt.terminal.Result, 'success');
  assert.equal(receipt.terminal.ExecMainStatus, '0');
  assert.equal(receipt.terminal.MemoryMax, '8589934592');
  assert.equal(receipt.terminal.MemorySwapMax, '0');
  assert.equal(receipt.terminal.CPUQuotaPerSecUSec, '2s');
  for (const field of ['cgroupAbsent', 'apiPidAbsent', 'postmasterPidAbsent', 'postgresStopped', 'postgresRemoved']) assert.equal(receipt.cleanup[field], true, 'verified cleanup ' + field);
  assert.equal(receipt.claims.hostedDeniedNote, false);
  assert.equal(receipt.claims.inference, false);
  assert.equal(receipt.claims.suiteParity, false);
  return validateNativeRemoteFixture(fixture);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = new URL('../../', import.meta.url);
  const result = validateNativeRemoteReceipt(
    readFileSync(new URL('contracts/openapi/fixtures/native-remote-auth.json', root)),
    readFileSync(new URL('scripts/ci/capture-native-remote-fixture.mjs', root)),
    JSON.parse(readFileSync(new URL('contracts/openapi/fixtures/native-remote-auth.receipt.json', root))),
  );
  console.log(JSON.stringify({ status: 'PASS', ...result }));
}
