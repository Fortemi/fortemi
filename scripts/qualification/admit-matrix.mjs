import fs from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { verifyEnvelope } from './dsse.mjs';
import { inspectAuthority } from './inspect-authority.mjs';
import { verifyEvidence } from './verify-evidence.mjs';
import { canonicalJson, jsonDigest, parseCanonicalJson } from './canonical-json.mjs';

const MAX_LEDGER_BYTES = 16 * 1024 * 1024;
const digestPattern = /^sha256:[a-f0-9]{64}$/;
const uuidPattern = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
function validateHistory(ledger) {
  if (ledger?.schemaVersion !== '1.0.0' || !Number.isSafeInteger(ledger.sequence) || ledger.sequence < 0
    || !Array.isArray(ledger.attempts) || ledger.attempts.length > 10000) throw new Error('invalid or oversized admission ledger');
  const ids = new Set(), nonces = new Set(), highest = new Map();
  for (const a of ledger.attempts) {
    if (!digestPattern.test(a.authorityRevision) || !digestPattern.test(a.receiptDigest) || !digestPattern.test(a.receiptEnvelopeDigest) || !uuidPattern.test(a.attemptId)
      || typeof a.cellId !== 'string' || !a.cellId || !/^[a-f0-9]{32,128}$/.test(a.runNonce)
      || !Number.isSafeInteger(a.attemptNumber) || a.attemptNumber < 1 || !['PASS', 'FAIL', 'MISSING'].includes(a.verdict)) throw new Error('invalid ledger attempt');
    const cell = `${a.authorityRevision}/${a.cellId}`, id = a.attemptId.toLowerCase();
    if (ids.has(id) || nonces.has(a.runNonce) || (highest.get(cell) ?? 0) >= a.attemptNumber) throw new Error('inconsistent replay history');
    ids.add(id); nonces.add(a.runNonce); highest.set(cell, a.attemptNumber);
  }
  return { ids, nonces, highest };
}

/** Runs under the store lock. Reports only the exact submitted authority. */
function evaluate(request, ledger, now) {
  const history = validateHistory(ledger);
  const { authorityEnvelope, receiptEnvelopes, trust, evidenceRoot, limits } = request;
  for (const field of ['maxFileBytes', 'maxTotalBytes', 'maxFiles']) {
    if (!Number.isSafeInteger(limits?.[field]) || limits[field] <= 0) throw new Error(`positive bounded ${field} required`);
  }
  if (limits.maxFileBytes > limits.maxTotalBytes || limits.maxTotalBytes > 1024 ** 3 || limits.maxFiles > 10000) throw new Error('unsupported evidence budget');
  const approved = verifyEnvelope(authorityEnvelope, 'authority', trust.authorityKeys, trust.authorityThreshold ?? 1);
  const authority = approved.document;
  const checked = inspectAuthority(authority);
  if (authority.schemaVersion !== '2.0.0' || !checked.valid) throw new Error(`invalid v2 authority: ${checked.errors.join(';')}`);
  if (!Number.isFinite(now) || now < Date.parse(authority.validFrom) || now > Date.parse(authority.validUntil)) throw new Error('authority outside validity window');
  if (!Array.isArray(receiptEnvelopes) || receiptEnvelopes.length > 10000) throw new Error('bounded receipt batch required');
  const revision = jsonDigest(authority), rows = new Map(authority.cells.map(c => [c.cellId, {
    cellId: c.cellId, plane: c.plane, profile: c.profile, supported: c.supported,
    status: c.supported ? 'MISSING' : 'UNSUPPORTED', checkVerdict: 'MISSING', diagnostics: ['no verified receipt submitted'],
  }]));
  const pending = new Map(), seen = new Set(), globalErrors = [];
  let bytesRead = 0;
  let filesRead = 0;
  for (const envelope of receiptEnvelopes) {
    let candidate;
    try { candidate = verifyEnvelope(envelope, 'receipt', trust.verifierKeys, trust.verifierThreshold ?? 1).document; }
    catch (e) { globalErrors.push(e.message); continue; }
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) { globalErrors.push('receipt payload must be an object'); continue; }
    const row = rows.get(candidate.cellId);
    if (!row || candidate.authorityRevision !== revision) { globalErrors.push('receipt outside declared authority/cell'); continue; }
    if (seen.has(candidate.cellId)) {
      pending.delete(candidate.cellId); delete row.receiptDigest; delete row.receiptEnvelopeDigest; row.checkVerdict = 'FAIL'; row.status = row.supported ? 'FAIL' : 'UNSUPPORTED'; row.diagnostics = ['multiple receipts submitted for one cell']; continue;
    }
    seen.add(candidate.cellId);
    const remaining = limits?.maxTotalBytes - bytesRead;
    const verification = verifyEvidence(authorityEnvelope, envelope, trust, evidenceRoot,
      { ...limits, maxFileBytes: Math.min(limits.maxFileBytes, remaining), maxTotalBytes: remaining, maxFiles: limits.maxFiles - filesRead }, now);
    bytesRead += verification.bytesRead ?? 0;
    filesRead += verification.filesRead ?? 0;
    row.diagnostics = [...verification.errors, ...verification.missing.map(m => `missing: ${m}`)];
    if (!verification.evidenceVerified) {
      row.checkVerdict = verification.errors.length ? 'FAIL' : 'MISSING';
    } else {
      const r = verification.receipt, cellKey = `${revision}/${r.cellId}`;
      if (history.ids.has(r.attemptId.toLowerCase()) || history.nonces.has(r.runNonce)
        || (history.highest.get(cellKey) ?? 0) >= r.attemptNumber) {
        row.checkVerdict = 'FAIL'; row.diagnostics.push('replayed identity/nonce or nonmonotonic attempt');
      } else {
        history.ids.add(r.attemptId.toLowerCase()); history.nonces.add(r.runNonce); history.highest.set(cellKey, r.attemptNumber);
        row.checkVerdict = r.verdict; row.receiptDigest = r.receiptDigest; row.receiptEnvelopeDigest = jsonDigest(envelope);
        pending.set(r.cellId, { authorityRevision: revision, cellId: r.cellId, attemptId: r.attemptId,
          attemptNumber: r.attemptNumber, runNonce: r.runNonce, receiptDigest: r.receiptDigest, receiptEnvelopeDigest: row.receiptEnvelopeDigest, verdict: r.verdict });
      }
    }
    row.status = row.supported ? row.checkVerdict : 'UNSUPPORTED';
  }
  const cells = [...rows.values()];
  return { attempts: [...pending.values()], report: { schemaVersion: '1.0.0', authorityRevision: revision,
    authorityEnvelopeDigest: jsonDigest(authorityEnvelope),
    trustDigest: jsonDigest(trust), evaluatedAt: new Date(now).toISOString(), committed: true,
    allRequiredChecksPass: globalErrors.length === 0 && cells.every(c => c.checkVerdict === 'PASS'),
    cells, errors: globalErrors, bytesRead, filesRead, suiteClaim: 'NO-GO' } };
}

/** Persist replay history and its report as one atomic ledger replacement.
 * storeRoot must be operator-controlled and separate from untrusted evidence. */
export function admitMatrix(request, now = Date.now()) {
  let rootFd, lockFd, temp;
  try {
    if (process.platform !== 'linux') throw new Error('admission store requires Linux');
    const store = fs.realpathSync(request.storeRoot), evidence = fs.realpathSync(request.evidenceRoot);
    if (store === evidence || store.startsWith(evidence + path.sep) || evidence.startsWith(store + path.sep)) throw new Error('store and evidence roots must be separate');
    rootFd = fs.openSync(request.storeRoot, fs.constants.O_RDONLY | fs.constants.O_DIRECTORY | fs.constants.O_NOFOLLOW);
    const base = `/proc/self/fd/${rootFd}`;
    lockFd = fs.openSync(`${base}/admission.lock`, fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW, 0o600);
    fs.writeFileSync(lockFd, canonicalJson({ pid: process.pid, token: randomUUID(), createdAt: new Date().toISOString() }));
    let ledger;
    try {
      const fd = fs.openSync(`${base}/ledger.json`, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
      try {
        const stat = fs.fstatSync(fd);
        if (!stat.isFile() || stat.size > MAX_LEDGER_BYTES) throw new Error('invalid ledger file');
        ledger = parseCanonicalJson(fs.readFileSync(fd));
      } finally { fs.closeSync(fd); }
      if (request.initialize === true) throw new Error('store already initialized');
    } catch (e) {
      if (e.code !== 'ENOENT' || request.initialize !== true) throw e;
      ledger = { schemaVersion: '1.0.0', sequence: 0, attempts: [] };
    }
    const evaluated = evaluate(request, ledger, now);
    const next = { schemaVersion: '1.0.0', sequence: ledger.sequence + 1,
      attempts: [...ledger.attempts, ...evaluated.attempts], lastReport: evaluated.report };
    validateHistory(next);
    const bytes = Buffer.from(canonicalJson(next));
    if (bytes.length > MAX_LEDGER_BYTES) throw new Error('admission ledger capacity reached');
    temp = `${base}/ledger-${randomUUID()}.tmp`;
    const fd = fs.openSync(temp, fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW, 0o600);
    try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
    fs.renameSync(temp, `${base}/ledger.json`); temp = undefined; fs.fsyncSync(rootFd);
    return evaluated.report;
  } finally {
    if (temp) fs.unlinkSync(temp);
    if (lockFd !== undefined) { fs.closeSync(lockFd); fs.unlinkSync(`/proc/self/fd/${rootFd}/admission.lock`); }
    if (rootFd !== undefined) fs.closeSync(rootFd);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('usage: node scripts/qualification/admit-matrix.mjs OPERATOR_REQUEST.json');
  const report = admitMatrix(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
  console.log(JSON.stringify(report, null, 2));
  if (!report.allRequiredChecksPass) process.exitCode = 1;
}
