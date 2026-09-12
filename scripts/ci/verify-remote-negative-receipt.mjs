import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { loadControls } from './remote-negative-controls.mjs';

export function validateNegativeReceipt(receipt, fixtureSha256, scriptBytes, helperBytes) {
  const { controls } = loadControls();
  const hash = bytes => createHash('sha256').update(bytes).digest('hex');
  assert.equal(receipt.schemaVersion, 'fortemi.remote-negative-package.v1');
  assert.equal(receipt.status, 'PASS');
  assert.equal(receipt.packageVersion, '2026.9.4');
  assert.equal(receipt.packageCommit, '9f74c0bab0cce5ef8e2433e8944a4efda5575eb4');
  assert.equal(receipt.packageSha256, '4a126d59bc18af4fb981d5bdbf19a761402e246b7d85cf3e6445de4eacf92897');
  assert.equal(receipt.fixtureSha256, fixtureSha256);
  assert.equal(receipt.basisSha256, controls.basis.sha256);
  assert.equal(receipt.scriptSha256, hash(scriptBytes));
  assert.equal(receipt.helperSha256, hash(helperBytes));
  assert.equal(receipt.classification, controls.classification);
  assert.deepEqual(receipt.claims, controls.claims);
  assert.equal(receipt.boundary, controls.boundary);
  assert.equal(receipt.scratchRemoved, true);
  assert.match(receipt.unit, /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
  const expected = controls.controls.flatMap(control => (control.stage === 'note' ? ['getNote', 'getNoteFull'] : ['getNoteFull']).map(method => `${control.id}/${method}`));
  assert.deepEqual(receipt.checks.map(c => `${c.id}/${c.readMethod}`), expected);
  for (const check of receipt.checks) {
    const control = controls.controls.find(c => c.id === check.id);
    assert.equal(check.expectedKind, control.expectedKind);
    assert.equal(check.actualKind, control.expectedKind);
    assert.equal(check.injected, 1);
    assert.equal(check.listenerClosed, true);
    assert.equal(check.noteReadableBeforeEnrichmentFault, control.stage !== 'note');
    assert.equal(check.classification, controls.classification.replace('producer-owned-', ''));
    assert.equal(check.transport, 'real-private-loopback-http');
    assert.ok(check.requests.length >= 1 && check.requests.length <= 8);
    for (const request of check.requests) {
      assert.deepEqual(Object.keys(request).sort(), ['method', 'path']);
      assert.equal(request.method, 'GET');
      assert.match(request.path, /^\/api\/v1\/notes\/[a-f0-9-]+(?:\/(links|concepts|provenance))?$/);
    }
    for (const response of check.responses) {
      assert.ok(check.requests.some(r => r.path === response.path));
      assert.match(response.responseSha256, /^[a-f0-9]{64}$/);
      assert.ok(response.bytes >= 0 && response.bytes <= 16384);
    }
    if (control.mode === 'problem') {
      assert.equal(check.status, control.nativeStatus);
      assert.equal(check.problemCode, ({ 401: 'unauthorized', 403: 'forbidden', 404: 'not-found', 429: 'rate-limit-exceeded', 500: 'internal-error' })[control.nativeStatus]);
      assert.equal(check.retryAfterSeconds, control.nativeStatus === 429 ? 60 : undefined);
    }
    if (['proxy404', 'mismatched404'].includes(control.mode)) {
      assert.equal(check.status, 404); assert.equal(check.problemCode, undefined);
    }
  }
  return { checks: receipt.checks.length, fixtureSha256, classification: receipt.classification };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const receipt = JSON.parse(readFileSync(new URL('../../contracts/openapi/fixtures/remote-negative-package.receipt.json', import.meta.url)));
  console.log(JSON.stringify(validateNegativeReceipt(receipt, loadControls().sha256,
    readFileSync(new URL('./verify-remote-negative-package.mjs', import.meta.url)), readFileSync(new URL('./remote-negative-controls.mjs', import.meta.url)))));
}
