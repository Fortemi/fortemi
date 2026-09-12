import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { controlsUrl, nativeUrl, loadControls, validateControls, exerciseControl } from './remote-negative-controls.mjs';

const bytes = readFileSync(nativeUrl), controls = JSON.parse(readFileSync(controlsUrl));
test('producer-owned negative corpus is explicitly injection and binds historical response bytes', () => {
  assert.equal(loadControls().controls.controls.length, 21);
});
for (const [name, mutate] of [
  ['missing case', c => c.controls.pop()],
  ['duplicate case identity', c => { c.controls[1].id = c.controls[0].id; }],
  ['live-server claim', c => { c.claims.liveFortemiServer = true; }],
  ['hosted-denial claim', c => { c.claims.hostedNoteDenial = true; }],
  ['wrong historical bytes', c => { c.basis.sha256 = '0'.repeat(64); }],
  ['unrecognized mode', c => { c.controls[0].mode = 'execute'; }],
  ['unrecognized route', c => { c.controls[0].stage = 'operator'; }],
  ['incorrect expected kind', c => { c.controls[0].expectedKind = 'transport'; }],
  ['unexpected execution field', c => { c.controls[0].command = 'not-executed'; }],
  ['oversized malformed body', c => { c.controls[0].rawBody = 'x'.repeat(16385); }],
  ['unsupported HTTP status', c => { c.controls.find(x => x.mode === 'problem').nativeStatus = 302; }],
  ['note denial relabeling', c => { c.controls.find(x => x.nativeStatus === 403).stage = 'note'; }],
]) test('rejects ' + name, () => { const changed = structuredClone(controls); mutate(changed); assert.throws(() => validateControls(changed, bytes)); });
test('rejects actual historical fixture-byte drift', () => assert.throws(() => validateControls(controls, Buffer.concat([bytes, Buffer.from(' ')]))));
test('published-package harness refuses unbounded execution before installation', () => {
  const result = spawnSync(process.execPath, [new URL('./verify-remote-negative-package.mjs', import.meta.url).pathname, '/unused', '/unused', '/unused'], {
    encoding: 'utf8', timeout: 5000, env: { ...process.env, FORTEMI_LOCAL_TEST_UNIT: 'invalid' },
  });
  assert.equal(result.status, 1); assert.match(result.stderr, /AssertionError/);
});
test('loopback verifier rejects a consumer that returns null on malformed success', async () => {
  class FakeError extends Error {}
  const broken = { RemoteBackendError: FakeError, createRemoteBackend: ({ baseUrl, fetchImpl }) => ({
    getNote: async id => { const response = await fetchImpl(`${baseUrl}/api/v1/notes/${id}`); await response.text(); return null; },
  }) };
  await assert.rejects(exerciseControl(broken, controls.controls[0], JSON.parse(bytes), 'getNote'), /must reject with typed error/);
});
