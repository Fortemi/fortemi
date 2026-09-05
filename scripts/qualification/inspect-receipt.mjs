import fs from 'node:fs';
import { createRequire } from 'node:module';
import { inspectAuthority } from './inspect-authority.mjs';
import { jsonDigest, receiptDigest, canonicalJson } from './canonical-json.mjs';

const require = createRequire(new URL('../../mcp-server/package.json', import.meta.url));
const Ajv = require('ajv/dist/2020.js');
const ajv = new Ajv({ strictTypes: false, strictRequired: false, allErrors: true });
require('ajv-formats')(ajv);
const validate = ajv.compile(JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-qualification/1.0.0/schemas/receipt.schema.json', import.meta.url))));
const compare = { eq: (a, b) => a === b, lte: (a, b) => a <= b, lt: (a, b) => a < b, gte: (a, b) => a >= b, gt: (a, b) => a > b };

/** Validity here is internal consistency only, never independent verification. */
export function inspectReceipt(receipt, authority) {
  const errors = [];
  const result = () => ({ valid: errors.length === 0, admitted: false, errors });
  const authorityResult = inspectAuthority(authority);
  if (!authorityResult.valid) { errors.push(...authorityResult.errors.map(e => `authority: ${e}`)); return result(); }
  if (!validate(receipt)) { errors.push(...validate.errors.map(e => `${e.instancePath}: ${e.message}`)); return result(); }
  try {
    for (const [field, expected] of Object.entries({ authorityRevision: jsonDigest(authority),
      environmentDigest: jsonDigest(authority.environment), authorityThresholdDigest: jsonDigest(authority.thresholds),
      cleanDestinationProvenance: authority.environment.cleanDestinationProvenance,
      authorityValidFrom: authority.validFrom, authorityValidUntil: authority.validUntil,
      receiptDigest: receiptDigest(receipt) })) {
      if (receipt[field] !== expected) errors.push(`binding mismatch: ${field}`);
    }
  } catch (e) { errors.push(`canonicalization: ${e.message}`); return result(); }
  if (!authority.fixtureDigests.includes(receipt.fixtureDigest)) errors.push('unapproved fixture digest');
  if (!authority.approvals.includes(receipt.approvalDigest)) errors.push('unapproved approval reference');
  for (const [role, components] of [['producer', authority.producers], ['consumer', authority.consumers]]) {
    if (!components.some(c => c.name === receipt[role].name && c.revision === receipt[role].revision)) errors.push(`unbound ${role}`);
  }
  for (const field of ['name', 'revision', 'imageDigest']) {
    if (receipt.verifier[field] !== authority.verifier[field]) errors.push(`unbound verifier ${field}`);
  }
  if (receipt.verifier.attestation.signedReceiptDigest !== receipt.receiptDigest) errors.push('attestation digest mismatch');
  const times = [authority.validFrom, receipt.startedAt, receipt.completedAt, receipt.verifier.verifiedAt, authority.validUntil].map(Date.parse);
  if (times.some((t, i) => !Number.isFinite(t) || (i > 0 && t < times[i - 1]))) errors.push('receipt outside ordered authority window');
  if (receipt.thresholds.length !== authority.thresholds.length) errors.push('threshold coverage mismatch');
  const seen = new Set();
  for (const threshold of receipt.thresholds) {
    const expected = authority.thresholds[threshold.authorityThresholdIndex];
    if (seen.has(threshold.authorityThresholdIndex)) errors.push('duplicate threshold index');
    seen.add(threshold.authorityThresholdIndex);
    if (!expected || ['metric', 'operator', 'limit', 'unit'].some(k => threshold[k] !== expected[k])) {
      errors.push(`threshold binding mismatch: ${threshold.metric}`); continue;
    }
    const measured = receipt.measurements[threshold.metric];
    if (typeof measured !== 'number' || !Number.isFinite(measured) || measured < 0) errors.push(`measurement missing or invalid: ${threshold.metric}`);
    else if (compare[threshold.operator](measured, threshold.limit) !== threshold.passed) errors.push(`threshold result mismatch: ${threshold.metric}`);
  }
  if (receipt.verdict === 'PASS' && canonicalJson(receipt.expected) !== canonicalJson(receipt.actual)) errors.push('PASS expected/actual mismatch');
  const profile = authority.contractTuple.knowledgeShardProfile;
  if (receipt.plane === 'knowledge-shard' ? receipt.profile !== profile : profile !== 'not-applicable') errors.push('shard plane/profile binding mismatch');
  // Artifact bytes, signer trust, replay history, exact cell scope and independent
  // state/cleanup verification must still be checked by the admission engine.
  return result();
}
