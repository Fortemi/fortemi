import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { validateNativeRemoteFixture, validateNativeRemoteReceipt } from './verify-native-remote-fixture.mjs';

const fixtureBytes = readFileSync(new URL('../../contracts/openapi/fixtures/native-remote-auth.json', import.meta.url));
const fixture = JSON.parse(fixtureBytes);
const scriptBytes = readFileSync(new URL('./capture-native-remote-fixture.mjs', import.meta.url));
const receipt = JSON.parse(readFileSync(new URL('../../contracts/openapi/fixtures/native-remote-auth.receipt.json', import.meta.url)));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const rebind = call => { call.rawBody = JSON.stringify(call.body); call.responseSha256 = hash(call.rawBody); };

test('pinned real capture and terminal cleanup receipt agree', () => {
  const result = validateNativeRemoteReceipt(fixtureBytes, scriptBytes, receipt);
  assert.equal(result.checks, 13);
  assert.equal(result.calls, 86);
});
for (const [name, mutate] of [
  ['missing check', f => f.checks.pop()],
  ['failed run', f => { f.status = 'FAIL'; }],
  ['wrong producer', f => { f.artifact.commit = '0'.repeat(40); }],
  ['wrong consumer', f => { f.packageSha256 = '0'.repeat(64); }],
  ['raw body corruption', f => { f.calls[0].rawBody += ' '; }],
  ['decoded body drift', f => { f.calls[0].body = {}; }],
  ['missing auth requirement', f => { f.health.capabilities.auth_required = false; }],
  ['dirty destination', f => { f.initialPhysicalNotes = 1; }],
  ['database fault left active', f => { f.databaseFaultRestored = false; }],
  ['child still alive', f => { f.apiPidAbsent = false; }],
  ['credential in capture', f => { f.authorization = 'Bearer mm_key_fixture'; }],
  ['operator403 relabeled as note denial', f => { f.calls.find(c => c.status === 403).path = '/api/v1/notes/00000000-0000-0000-0000-000000000000'; }],
  ['problem status drift', f => { const c = f.calls.find(c => c.status === 500); c.body.status = 200; rebind(c); }],
  ['private internal error content', f => { const c = f.calls.find(c => c.status === 500); c.body.detail = 'private database state'; rebind(c); }],
  ['missing retry-after', f => { f.calls.find(c => c.status === 429).retryAfter = null; }],
  ['missing composed error', f => { let removed = false; f.calls = f.calls.filter(c => c.status !== 401 || (!removed && (removed = true))); }],
]) test('rejects ' + name, () => { const changed = structuredClone(fixture); mutate(changed); assert.throws(() => validateNativeRemoteFixture(changed)); });
for (const [name, mutate] of [
  ['fixture digest drift', r => { r.fixtureSha256 = '0'.repeat(64); }],
  ['capture script drift', r => { r.captureScriptSha256 = '0'.repeat(64); }],
  ['nonterminal job', r => { r.terminal.ActiveState = 'active'; }],
  ['signal termination', r => { r.terminal.ExecMainStatus = '15'; }],
  ['unbounded memory', r => { r.terminal.MemoryMax = 'infinity'; }],
  ['cluster not removed', r => { r.cleanup.postgresRemoved = false; }],
  ['unsupported hosted claim', r => { r.claims.hostedDeniedNote = true; }],
]) test('rejects receipt ' + name, () => { const changed = structuredClone(receipt); mutate(changed); assert.throws(() => validateNativeRemoteReceipt(fixtureBytes, scriptBytes, changed)); });
test('capture refuses execution without the owned bounded fixture', () => {
  const run = spawnSync(process.execPath, [new URL('./capture-native-remote-fixture.mjs', import.meta.url).pathname, '/unused-api', '/unused-package', '/unused-output', '/unused-cache'], { encoding: 'utf8', timeout: 5000, env: { ...process.env, FORTEMI_LOCAL_TEST_UNIT: 'invalid' } });
  assert.equal(run.status, 1);
  assert.match(run.stderr, /AssertionError/);
});
