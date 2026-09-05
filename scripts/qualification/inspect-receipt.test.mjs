import assert from 'node:assert/strict';
import test from 'node:test';
import { authority } from './test-fixtures.mjs';
import { inspectReceipt } from './inspect-receipt.mjs';
import { jsonDigest, receiptDigest } from './canonical-json.mjs';

function fixture() {
  const a = authority(), digest = `sha256:${'a'.repeat(64)}`;
  const outcome = { terminalState: 'unchanged', stateDigest: digest, mutationCount: 0, reasonCodes: [] };
  const r = { schemaVersion: '1.0.0', cellId: 'test-only', attemptId: '11111111-1111-4111-8111-111111111111',
    attemptNumber: 1, runNonce: 'a'.repeat(32), startedAt: a.validFrom, completedAt: a.validFrom,
    authorityValidFrom: a.validFrom, authorityValidUntil: a.validUntil, authorityRevision: jsonDigest(a),
    producer: { name: a.producers[0].name, revision: a.producers[0].revision },
    consumer: { name: a.consumers[0].name, revision: a.consumers[0].revision },
    plane: 'live-remote-persistence', profile: 'test-only', environmentDigest: jsonDigest(a.environment),
    fixtureDigest: digest, cleanDestination: true, cleanDestinationProvenance: digest,
    expected: structuredClone(outcome), actual: structuredClone(outcome),
    thresholds: a.thresholds.map(({ approverRole, ...t }, i) => ({ ...t, passed: true, authorityThresholdIndex: i })),
    authorityThresholdDigest: jsonDigest(a.thresholds), measurements: Object.fromEntries(a.thresholds.map(t => [t.metric, 0])),
    evidence: ['fixture', 'runtime-receipt', 'state-digest', 'telemetry', 'redaction', 'cleanup', 'approval', 'attestation'].map(artifactType => ({
      artifactType, path: `test-only/${artifactType}`, digest, producer: 'test-only', revision: 'a'.repeat(40) })),
    canonicalization: 'RFC8785-SHA256:exclude(receiptDigest,verifier.attestation)', receiptDigest: digest,
    acceptanceIds: ['DQ-TENANT-AC-001'], riskIds: ['DQ-R1'], approvalDigest: digest,
    verifier: { name: a.verifier.name, revision: a.verifier.revision, imageDigest: digest, independent: true, passed: true,
      verifiedAt: a.validFrom, attestation: { scheme: 'dsse', signerIdentityDigest: digest, signedReceiptDigest: digest,
        signature: 'test-only-invalid-signature' } }, verdict: 'PASS' };
  seal(r);
  return { a, r };
}
function seal(r) { r.receiptDigest = receiptDigest(r); r.verifier.attestation.signedReceiptDigest = r.receiptDigest; }

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
