import { authority } from './test-fixtures.mjs';
import { jsonDigest, receiptDigest } from './canonical-json.mjs';

export function fixture() {
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
export function seal(r) { r.receiptDigest = receiptDigest(r); r.verifier.attestation.signedReceiptDigest = r.receiptDigest; }

