import { generateKeyPairSync, sign } from 'node:crypto';
import { fixture } from './receipt-test-fixtures.mjs';
import { canonicalJson, jsonDigest, receiptDigest } from './canonical-json.mjs';
import { PAYLOAD_TYPES, publicKeyDigest } from './dsse.mjs';
import { schemaDigestsV2 } from './inspect-authority.mjs';

function keys() {
  const k = generateKeyPairSync('ed25519'); const pem = k.publicKey.export({ type: 'spki', format: 'pem' });
  return { ...k, pin: { publicKeyPem: pem, digest: publicKeyDigest(pem) } };
}
export const approver = keys(), verifier = keys(), impostor = keys();
export function signed(document, role, key) {
  const payload = Buffer.from(canonicalJson(document)), payloadType = PAYLOAD_TYPES[role];
  const data = Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(payloadType)} ${payloadType} ${payload.length} `), payload]);
  return { payloadType, payload: payload.toString('base64'), signatures: [{ sig: sign(null, data, key.privateKey).toString('base64') }] };
}
export function v2() {
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
export function seal(a, r, authorityKey = approver, receiptKey = verifier) {
  r.authorityRevision = jsonDigest(a);
  const ae = signed(a, 'authority', authorityKey); r.approvalDigest = jsonDigest(ae);
  r.receiptDigest = receiptDigest(r);
  return [ae, signed(r, 'receipt', receiptKey)];
}
export const trust = { authorityKeys: [approver.pin], verifierKeys: [verifier.pin] };
export const now = Date.parse('2026-01-01T12:00:00Z');

