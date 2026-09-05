import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import test from 'node:test';
import { fixture } from './receipt-test-fixtures.mjs';
import { canonicalJson, jsonDigest, receiptDigest } from './canonical-json.mjs';
import { PAYLOAD_TYPES, publicKeyDigest } from './dsse.mjs';
import { schemaDigestsV2, inspectAuthority } from './inspect-authority.mjs';
import { authenticateReceipt } from './authenticate-receipt.mjs';

function keys() {
  const k = generateKeyPairSync('ed25519'); const pem = k.publicKey.export({ type: 'spki', format: 'pem' });
  return { ...k, pin: { publicKeyPem: pem, digest: publicKeyDigest(pem) } };
}
const approver = keys(), verifier = keys(), impostor = keys();
function signed(document, role, key) {
  const payload = Buffer.from(canonicalJson(document)), payloadType = PAYLOAD_TYPES[role];
  const data = Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(payloadType)} ${payloadType} ${payload.length} `), payload]);
  return { payloadType, payload: payload.toString('base64'), signatures: [{ sig: sign(null, data, key.privateKey).toString('base64') }] };
}
function v2() {
  const { a, r } = fixture();
  a.schemaVersion = '2.0.0'; delete a.approvals;
  a.schemaDigests = { ...schemaDigestsV2 }; a.verifier.signerKeyDigest = verifier.pin.digest;
  a.cells = [{ cellId: r.cellId, plane: r.plane, profile: r.profile, supported: true,
    producer: a.producers[0], consumer: a.consumers[0], expected: structuredClone(r.expected),
    acceptanceIds: r.acceptanceIds, riskIds: r.riskIds }];
  r.schemaVersion = '2.0.0'; delete r.verifier.attestation;
  r.evidence = r.evidence.filter(e => e.artifactType !== 'attestation');
  r.canonicalization = 'RFC8785-SHA256:exclude(receiptDigest)';
  r.authoritySchemaDigest = schemaDigestsV2.authority; r.receiptSchemaDigest = schemaDigestsV2.receipt;
  return { a, r };
}
function seal(a, r, authorityKey = approver, receiptKey = verifier) {
  r.authorityRevision = jsonDigest(a);
  const ae = signed(a, 'authority', authorityKey); r.approvalDigest = jsonDigest(ae);
  r.receiptDigest = receiptDigest(r);
  return [ae, signed(r, 'receipt', receiptKey)];
}
const trust = { authorityKeys: [approver.pin], verifierKeys: [verifier.pin] };
const now = Date.parse('2026-01-01T12:00:00Z');

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
