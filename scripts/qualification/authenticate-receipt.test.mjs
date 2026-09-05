import assert from 'node:assert/strict';
import test from 'node:test';
import { jsonDigest } from './canonical-json.mjs';
import { inspectAuthority } from './inspect-authority.mjs';
import { authenticateReceipt } from './authenticate-receipt.mjs';

import { v2, seal, trust, now, approver, impostor } from './signed-test-fixtures.mjs';

test('detached signatures authenticate without circular digest or admitting evidence', () => {
  const { a, r } = v2();
  const [ae, re] = seal(a, r);
  const result = authenticateReceipt(ae, re, trust, now);
  assert.equal(result.authenticated, true, result.errors.join(';'));
  assert.equal(result.admitted, false);
  assert.equal(result.receipt.authorityRevision, jsonDigest(a));
  assert.equal(result.receipt.approvalDigest, jsonDigest(ae));
});
for (const [name, mutate] of [
  ['unknown cell', (a, r) => { r.cellId = 'another'; }],
  ['cell plane substitution', (a, r) => { r.plane = 'static-index'; }],
  ['cell expected-state substitution', (a, r) => { r.expected.mutationCount = 1; r.actual.mutationCount = 1; }],
  ['acceptance ID substitution', (a, r) => { r.acceptanceIds = ['DQ-FAULT-AC-001']; }],
  ['schema substitution', (a, r) => { r.receiptSchemaDigest = `sha256:${'f'.repeat(64)}`; }],
  ['wrong verifier key in authority', a => { a.verifier.signerKeyDigest = impostor.pin.digest; }],
  ['duplicate cells', a => { a.cells.push(a.cells[0]); }],
  ['unsupported cell allowing writes', a => { a.cells[0].supported = false; }],
]) test(`rejects signed ${name}`, () => {
  const { a, r } = v2(); mutate(a, r);
  assert.equal(authenticateReceipt(...seal(a, r), trust, now).authenticated, false);
});
test('rejects unsigned self-approval and incorrect signature roles', () => {
  const { a, r } = v2();
  assert.equal(authenticateReceipt(...seal(a, r, impostor), trust, now).authenticated, false);
  assert.equal(authenticateReceipt(...seal(a, r, approver, impostor), trust, now).authenticated, false);
  a.verifier.signerKeyDigest = approver.pin.digest;
  assert.equal(authenticateReceipt(...seal(a, r, approver, approver), { authorityKeys: [approver.pin], verifierKeys: [approver.pin] }, now).authenticated, false);
});
test('rejects expiry, future authority and invalid trusted time', () => {
  const { a, r } = v2(); const envelopes = seal(a, r);
  for (const clock of [Date.parse('2025-01-01T00:00:00Z'), Date.parse('2027-01-01T00:00:00Z'), NaN]) {
    assert.equal(authenticateReceipt(...envelopes, trust, clock).authenticated, false);
  }
});
test('v2 rejects embedded approval/attestation fields', () => {
  const { a, r } = v2(); a.approvals = [];
  assert.equal(inspectAuthority(a).valid, false); delete a.approvals;
  r.verifier.attestation = { arbitrary: true };
  assert.equal(authenticateReceipt(...seal(a, r), trust, now).authenticated, false);
});
