import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { loadControls } from './remote-negative-controls.mjs';
import { validateNegativeReceipt } from './verify-remote-negative-receipt.mjs';
const receipt = JSON.parse(readFileSync(new URL('../../contracts/openapi/fixtures/remote-negative-package.receipt.json', import.meta.url)));
const script = readFileSync(new URL('./verify-remote-negative-package.mjs', import.meta.url));
const helper = readFileSync(new URL('./remote-negative-controls.mjs', import.meta.url));
const verify = r => validateNegativeReceipt(r, loadControls().sha256, script, helper);
test('historical controlled-fault receipt binds exact published package, corpus and harness', () => assert.equal(verify(receipt).checks, 31));
for (const [name, mutate] of [
  ['missing operation', r => r.checks.pop()],
  ['wrong package', r => { r.packageSha256 = '0'.repeat(64); }],
  ['helper drift', r => { r.helperSha256 = '0'.repeat(64); }],
  ['script drift', r => { r.scriptSha256 = '0'.repeat(64); }],
  ['corpus drift', r => { r.fixtureSha256 = '0'.repeat(64); }],
  ['false live-server claim', r => { r.claims.liveFortemiServer = true; }],
  ['wrong error kind', r => { r.checks[0].actualKind = 'transport'; }],
  ['null result disguised as error', r => { r.checks[0].actualKind = null; }],
  ['listener not closed', r => { r.checks[0].listenerClosed = false; }],
  ['install scratch not removed', r => { r.scratchRemoved = false; }],
  ['retry', r => { r.checks[0].injected = 2; }],
  ['credentials in request', r => { r.checks[0].requests[0].authorization = 'synthetic'; }],
]) test('rejects ' + name, () => { const changed = structuredClone(receipt); mutate(changed); assert.throws(() => verify(changed)); });
