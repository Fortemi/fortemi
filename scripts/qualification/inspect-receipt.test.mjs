import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectReceipt } from './inspect-receipt.mjs';

import { fixture, seal } from './receipt-test-fixtures.mjs';

test('internally consistent input with fake signature is never admitted', () => {
  const { a, r } = fixture();
  assert.deepEqual(inspectReceipt(r, a), { valid: true, admitted: false, errors: [] });
});
for (const [name, mutate] of [
  ['authority digest substitution', r => { r.authorityRevision = `sha256:${'b'.repeat(64)}`; }],
  ['environment substitution', r => { r.environmentDigest = `sha256:${'b'.repeat(64)}`; }],
  ['unapproved fixture', r => { r.fixtureDigest = `sha256:${'b'.repeat(64)}`; }],
  ['unapproved approval reference', r => { r.approvalDigest = `sha256:${'b'.repeat(64)}`; }],
  ['producer substitution', r => { r.producer.revision = 'b'.repeat(40); }],
  ['verifier substitution', r => { r.verifier.name = 'different'; }],
  ['reversed time', r => { r.completedAt = '2025-12-31T23:59:59Z'; }],
  ['postexpiry verification', r => { r.verifier.verifiedAt = '2027-01-01T00:00:00Z'; }],
  ['omitted metric', r => { r.thresholds.pop(); }],
  ['duplicate metric index', r => { r.thresholds[1] = r.thresholds[0]; }],
  ['missing measurement', r => { delete r.measurements.unauthorizedReads; }],
  ['false pass measurement', r => { r.measurements.unauthorizedReads = 1; }],
  ['lowered receipt threshold', r => { r.thresholds[0].limit = 2; }],
  ['different actual state', r => { r.actual.mutationCount = 1; }],
  ['missing cleanup inventory', r => { r.evidence = r.evidence.filter(e => e.artifactType !== 'cleanup'); }],
  ['shard profile confusion', r => { r.plane = 'knowledge-shard'; r.profile = 'full-v1'; }],
]) test(`rejects ${name} even after recomputing receipt digest`, () => {
  const { a, r } = fixture(); mutate(r); seal(r); assert.equal(inspectReceipt(r, a).valid, false);
});
test('rejects receipt tamper and mismatched attestation digest', () => {
  const { a, r } = fixture(); r.runNonce = 'b'.repeat(32);
  assert.equal(inspectReceipt(r, a).valid, false);
  seal(r); r.verifier.attestation.signedReceiptDigest = `sha256:${'b'.repeat(64)}`;
  assert.equal(inspectReceipt(r, a).valid, false);
});
test('retains truthful FAIL without promoting it to admission', () => {
  const { a, r } = fixture(); r.verdict = 'FAIL'; r.verifier.passed = false;
  r.measurements.unauthorizedReads = 1; r.thresholds[0].passed = false; seal(r);
  assert.deepEqual(inspectReceipt(r, a), { valid: true, admitted: false, errors: [] });
});
