import { verifyEnvelope } from './dsse.mjs';
import { inspectReceipt } from './inspect-receipt.mjs';
import { jsonDigest } from './canonical-json.mjs';

/** Validate detached authority approval and independent receipt signatures.
 * Trust is supplied by the operator, outside both evidence documents. */
export function authenticateReceipt(authorityEnvelope, receiptEnvelope, trust, now = Date.now()) {
  try {
    if (!Number.isFinite(now)) throw new Error('trusted current time required');
    const approved = verifyEnvelope(authorityEnvelope, 'authority', trust.authorityKeys, trust.authorityThreshold ?? 1);
    const verified = verifyEnvelope(receiptEnvelope, 'receipt', trust.verifierKeys, trust.verifierThreshold ?? 1);
    const authority = approved.document, receipt = verified.document;
    if (authority.schemaVersion !== '2.0.0' || receipt.schemaVersion !== '2.0.0') throw new Error('detached signatures require schema 2.0.0');
    const checked = inspectReceipt(receipt, authority);
    if (!checked.valid) return { ...checked, authenticated: false };
    if (!verified.signerDigests.includes(authority.verifier.signerKeyDigest)) throw new Error('receipt not signed by authority verifier');
    if (approved.signerDigests.some(key => verified.signerDigests.includes(key))) throw new Error('authority and verifier signers must be separate');
    if (now < Date.parse(authority.validFrom) || now > Date.parse(authority.validUntil)
      || Date.parse(receipt.verifier.verifiedAt) > now) throw new Error('authority expired, not yet valid, or verification in future');
    if (receipt.approvalDigest !== jsonDigest(authorityEnvelope)) throw new Error('detached approval digest mismatch');
    return { valid: true, authenticated: true, admitted: false, errors: [], authority, receipt,
      authoritySigners: approved.signerDigests, verifierSigners: verified.signerDigests };
  } catch (e) {
    return { valid: false, authenticated: false, admitted: false, errors: [e.message] };
  }
}
