import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { authenticateReceipt } from './authenticate-receipt.mjs';
import { canonicalJson, parseCanonicalJson, jsonDigest } from './canonical-json.mjs';

export function artifactName(digest) {
  if (!/^sha256:[a-f0-9]{64}$/.test(digest)) throw new Error('invalid artifact digest');
  return digest.replace(':', '-');
}

/** Linux directory-fd anchoring prevents evidence path traversal and symlink
 * replacement. Evidence is opened read-only; no live service is contacted. */
export function verifyEvidence(authorityEnvelope, receiptEnvelope, trust, root, limits, now = Date.now()) {
  const authenticated = authenticateReceipt(authorityEnvelope, receiptEnvelope, trust, now);
  if (!authenticated.authenticated) return { ...authenticated, evidenceVerified: false, missing: [] };
  const errors = [], missing = [], { receipt } = authenticated;
  const envelopeDigest = jsonDigest(receiptEnvelope);
  // The detached envelope is indexed outside its own signed inventory.
  const inventory = [...receipt.evidence, { artifactType: 'detached-receipt',
    path: artifactName(envelopeDigest), digest: envelopeDigest, revision: receipt.verifier.revision }];
  let rootFd;
  let bytesRead = 0;
  let filesRead = 0;
  const artifacts = new Map();
  const result = () => ({ ...authenticated, valid: errors.length === 0 && missing.length === 0,
    admitted: false, evidenceVerified: errors.length === 0 && missing.length === 0, errors, missing, bytesRead, filesRead });
  try {
    if (process.platform !== 'linux') throw new Error('evidence reader requires Linux directory-fd support');
    for (const field of ['maxFileBytes', 'maxTotalBytes', 'maxFiles']) {
      if (!Number.isSafeInteger(limits?.[field]) || limits[field] <= 0) throw new Error(`positive bounded ${field} required`);
    }
    if (limits.maxFileBytes > limits.maxTotalBytes || limits.maxTotalBytes > 1024 ** 3 || limits.maxFiles > 10000) throw new Error('unsupported evidence budget');
    if (inventory.length > limits.maxFiles) throw new Error('evidence file count exceeds budget');
    rootFd = fs.openSync(root, fs.constants.O_RDONLY | fs.constants.O_DIRECTORY | fs.constants.O_NOFOLLOW);
    const paths = new Set();
    for (const entry of inventory) {
      if (!/^(?:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/.test(entry.revision)) { errors.push(`immutable evidence revision required: ${entry.artifactType}`); continue; }
      if (entry.path !== artifactName(entry.digest)) { errors.push(`non-content-addressed evidence path: ${entry.artifactType}`); continue; }
      if (paths.has(`${entry.artifactType}/${entry.path}`)) { errors.push('duplicate evidence role/path'); continue; }
      paths.add(`${entry.artifactType}/${entry.path}`);
      if (artifacts.has(entry.path)) continue;
      let fd;
      try {
        filesRead++;
        fd = fs.openSync(`/proc/self/fd/${rootFd}/${entry.path}`, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
        const before = fs.fstatSync(fd);
        if (!before.isFile()) throw new Error('evidence must be a regular file');
        if (before.size > limits.maxFileBytes || bytesRead + before.size > limits.maxTotalBytes) throw new Error('evidence byte budget exceeded');
        // Read at most the stated size plus a one-byte growth probe, never readFile.
        const bytes = Buffer.alloc(before.size);
        let offset = 0;
        while (offset < bytes.length) {
          const read = fs.readSync(fd, bytes, offset, bytes.length - offset, offset);
          if (!read) break;
          offset += read;
        }
        bytesRead += offset;
        const grew = fs.readSync(fd, Buffer.alloc(1), 0, 1, offset);
        const after = fs.fstatSync(fd);
        if (offset !== before.size || grew || before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs) throw new Error('evidence changed while reading');
        const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
        if (digest !== entry.digest) throw new Error('artifact digest mismatch');
        artifacts.set(entry.path, bytes);
      } catch (e) {
        if (e.code === 'ENOENT') missing.push(entry.path);
        else errors.push(`${entry.artifactType}: ${e.message}`);
      } finally { if (fd !== undefined) fs.closeSync(fd); }
    }
    const mandatory = ['fixture', 'approval', 'state-digest', 'telemetry', 'redaction', 'cleanup', 'runtime-receipt'];
    for (const role of mandatory) if (!receipt.evidence.some(e => e.artifactType === role)) missing.push(`role:${role}`);
    for (const [role, digest] of [['fixture', receipt.fixtureDigest], ['approval', receipt.approvalDigest]]) {
      if (!receipt.evidence.some(e => e.artifactType === role && e.digest === digest)) errors.push(`${role} reference missing from evidence inventory`);
    }
    if (!receipt.evidence.some(e => e.digest === receipt.cleanDestinationProvenance)) errors.push('clean destination provenance missing from inventory');
    const summaries = {
      'detached-receipt': receiptEnvelope,
      approval: authorityEnvelope,
      'state-digest': receipt.actual,
      telemetry: receipt.measurements,
      redaction: { attemptId: receipt.attemptId, runNonce: receipt.runNonce,
        verifierRevision: receipt.verifier.revision, findings: receipt.measurements.redactionFindings },
      cleanup: { attemptId: receipt.attemptId, runNonce: receipt.runNonce,
        verifierRevision: receipt.verifier.revision, cleanDestinationProvenance: receipt.cleanDestinationProvenance,
        outOfScope: receipt.measurements.cleanupOutOfScope },
    };
    for (const [role, expected] of Object.entries(summaries)) {
      const entries = inventory.filter(e => e.artifactType === role);
      if (entries.length > 1) errors.push(`ambiguous evidence summary: ${role}`);
      for (const entry of entries) {
        const bytes = artifacts.get(entry.path);
        if (!bytes) continue;
        try {
          if (canonicalJson(parseCanonicalJson(bytes)) !== canonicalJson(expected)) errors.push(`evidence summary mismatch: ${role}`);
          if (role !== 'approval' && entry.revision !== receipt.verifier.revision) errors.push(`summary verifier revision mismatch: ${role}`);
        } catch (e) { errors.push(`${role}: ${e.message}`); }
      }
    }
  } catch (e) { errors.push(e.message); }
  finally { if (rootFd !== undefined) fs.closeSync(rootFd); }
  return result();
}
