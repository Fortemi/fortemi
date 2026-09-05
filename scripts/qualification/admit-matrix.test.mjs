import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { prepared, limits } from './evidence-test-fixtures.mjs';
import { trust, now } from './signed-test-fixtures.mjs';
import { admitMatrix } from './admit-matrix.mjs';
import { canonicalJson } from './canonical-json.mjs';

function context(t, transform) {
  const p = prepared(t, transform), store = fs.mkdtempSync(path.join(os.tmpdir(), 'qualification-store-'));
  t.after(() => fs.rmSync(store, { recursive: true, force: true }));
  const request = () => { const [authorityEnvelope, receiptEnvelope] = p.envelopes();
    return { authorityEnvelope, receiptEnvelopes: [receiptEnvelope], trust, evidenceRoot: p.root, storeRoot: store, limits }; };
  return { ...p, store, request, ledger: () => JSON.parse(fs.readFileSync(path.join(store, 'ledger.json'))) };
}
test('atomically stores verified exact-cell result and replay history', t => {
  const p = context(t), report = admitMatrix({ ...p.request(), initialize: true }, now);
  assert.equal(report.committed, true); assert.equal(report.allRequiredChecksPass, true);
  assert.equal(report.cells[0].status, 'PASS'); assert.equal(report.suiteClaim, 'NO-GO');
  assert.equal(p.ledger().attempts.length, 1); assert.deepEqual(p.ledger().lastReport, report);
  assert.deepEqual(fs.readdirSync(p.store), ['ledger.json']);
  const repeated = admitMatrix(p.request(), now);
  assert.equal(repeated.allRequiredChecksPass, false); assert.equal(repeated.cells[0].status, 'FAIL');
  assert.match(repeated.cells[0].diagnostics.join(';'), /replayed/); assert.equal(p.ledger().attempts.length, 1);
});
test('missing and corrupt evidence stay distinct and do not consume an attempt', t => {
  const p = context(t), target = path.join(p.root, p.r.evidence[0].path);
  fs.unlinkSync(target);
  const missing = admitMatrix({ ...p.request(), initialize: true }, now);
  assert.equal(missing.cells[0].status, 'MISSING'); assert.equal(p.ledger().attempts.length, 0);
  fs.writeFileSync(target, 'corrupt');
  const corrupt = admitMatrix(p.request(), now);
  assert.equal(corrupt.cells[0].status, 'FAIL'); assert.equal(p.ledger().attempts.length, 0);
});
test('duplicate submissions cannot select the best receipt or be admitted', t => {
  const p = context(t), request = p.request(); request.receiptEnvelopes.push(request.receiptEnvelopes[0]);
  const report = admitMatrix({ ...request, initialize: true }, now);
  assert.equal(report.cells[0].status, 'FAIL'); assert.equal(p.ledger().attempts.length, 0);
});
test('unsubmitted supported and unsupported cells remain distinguishable', t => {
  const p = context(t); const c = structuredClone(p.a.cells[0]);
  c.cellId = 'unsupported'; c.supported = false; c.expected.terminalState = 'rejected'; p.a.cells.push(c);
  const request = p.request(); request.receiptEnvelopes = [];
  const report = admitMatrix({ ...request, initialize: true }, now);
  assert.deepEqual(report.cells.map(c => [c.status, c.checkVerdict]), [['MISSING', 'MISSING'], ['UNSUPPORTED', 'MISSING']]);
  assert.equal(report.allRequiredChecksPass, false);
});
test('failed observations are recorded without being qualified', t => {
  const p = context(t); p.r.verdict = 'FAIL'; p.r.verifier.passed = false;
  const report = admitMatrix({ ...p.request(), initialize: true }, now);
  assert.equal(report.cells[0].status, 'FAIL'); assert.equal(report.allRequiredChecksPass, false);
  assert.equal(p.ledger().attempts[0].verdict, 'FAIL');
});
test('missing store, corrupted store and reset attempts fail closed', t => {
  const p = context(t);
  assert.throws(() => admitMatrix(p.request(), now), /ENOENT/);
  admitMatrix({ ...p.request(), initialize: true }, now);
  assert.throws(() => admitMatrix({ ...p.request(), initialize: true }, now), /already initialized/);
  const ledger = path.join(p.store, 'ledger.json'); fs.writeFileSync(ledger, '{}');
  assert.throws(() => admitMatrix(p.request(), now), /ledger/);
  assert.equal(fs.readFileSync(ledger, 'utf8'), '{}');
});
test('existing lock blocks concurrent writers and is preserved', t => {
  const p = context(t); const lock = path.join(p.store, 'admission.lock'); fs.writeFileSync(lock, 'another process');
  assert.throws(() => admitMatrix({ ...p.request(), initialize: true }, now), /EEXIST/);
  assert.equal(fs.readFileSync(lock, 'utf8'), 'another process');
  assert.equal(fs.existsSync(path.join(p.store, 'ledger.json')), false);
});
test('rejects overlapping roots and ledger symlinks', t => {
  const p = context(t); assert.throws(() => admitMatrix({ ...p.request(), storeRoot: p.root, initialize: true }, now), /separate/);
  fs.symlinkSync(path.join(p.root, p.r.evidence[0].path), path.join(p.store, 'ledger.json'));
  assert.throws(() => admitMatrix(p.request(), now));
});
test('bad signature and budgets never become a pass or bypass ledger integrity', t => {
  const p = context(t), request = p.request(); request.receiptEnvelopes[0].signatures[0].sig = Buffer.alloc(64).toString('base64');
  const report = admitMatrix({ ...request, initialize: true }, now);
  assert.equal(report.allRequiredChecksPass, false); assert.ok(report.errors.length);
  assert.equal(p.ledger().attempts.length, 0);
  assert.throws(() => admitMatrix({ ...p.request(), limits: {} }, now), /bounded/);
  const ledger = p.ledger(); ledger.attempts = [{ bad: true }]; fs.writeFileSync(path.join(p.store, 'ledger.json'), canonicalJson(ledger));
  assert.throws(() => admitMatrix(p.request(), now), /ledger attempt/);
});

test('verified rejection satisfies its check without qualifying an unsupported tuple', t => {
  const p = context(t, (a, r) => {
    a.cells[0].supported = false; a.cells[0].expected.terminalState = 'rejected';
    r.expected.terminalState = 'rejected'; r.actual.terminalState = 'rejected';
  });
  const report = admitMatrix({ ...p.request(), initialize: true }, now);
  assert.equal(report.allRequiredChecksPass, true);
  assert.equal(report.cells[0].status, 'UNSUPPORTED'); assert.equal(report.cells[0].checkVerdict, 'PASS');
  assert.equal(report.cells[0].supported, false); assert.equal(report.suiteClaim, 'NO-GO');
});

test('fresh identity cannot bypass attempt ordering; next attempt retains prior evidence', t => {
  const p = context(t); admitMatrix({ ...p.request(), initialize: true }, now);
  p.r.attemptId = '22222222-2222-4222-8222-222222222222'; p.r.runNonce = 'b'.repeat(32);
  p.r.evidence = p.r.evidence.filter(e => !['redaction', 'cleanup'].includes(e.artifactType));
  p.r.evidence.push(p.entry('redaction', { attemptId: p.r.attemptId, runNonce: p.r.runNonce,
    verifierRevision: p.r.verifier.revision, findings: p.r.measurements.redactionFindings }));
  p.r.evidence.push(p.entry('cleanup', { attemptId: p.r.attemptId, runNonce: p.r.runNonce,
    verifierRevision: p.r.verifier.revision, cleanDestinationProvenance: p.r.cleanDestinationProvenance,
    outOfScope: p.r.measurements.cleanupOutOfScope }));
  assert.equal(admitMatrix(p.request(), now).allRequiredChecksPass, false);
  p.r.attemptNumber = 2;
  assert.equal(admitMatrix(p.request(), now).allRequiredChecksPass, true);
  assert.deepEqual(p.ledger().attempts.map(a => a.attemptNumber), [1, 2]);
});

test('failure before atomic replacement preserves previous ledger and releases owned lock', t => {
  const p = context(t); admitMatrix({ ...p.request(), initialize: true }, now);
  const before = fs.readFileSync(path.join(p.store, 'ledger.json'));
  const rename = fs.renameSync;
  try {
    fs.renameSync = () => { throw new Error('injected replacement failure'); };
    assert.throws(() => admitMatrix(p.request(), now), /injected replacement failure/);
  } finally { fs.renameSync = rename; }
  assert.deepEqual(fs.readFileSync(path.join(p.store, 'ledger.json')), before);
  assert.deepEqual(fs.readdirSync(p.store), ['ledger.json']);
});

test('two real processes cannot both admit the same attempt', async t => {
  const p = context(t), config = path.join(p.store, 'request.json');
  fs.writeFileSync(config, JSON.stringify({ ...p.request(), initialize: true }));
  const script = `import fs from 'node:fs'; import { admitMatrix } from ${JSON.stringify(new URL('./admit-matrix.mjs', import.meta.url).href)};
    try { const r = admitMatrix(JSON.parse(fs.readFileSync(process.argv[1])), Number(process.argv[2])); if (!r.allRequiredChecksPass) process.exitCode = 2; }
    catch (e) { if (e.code === 'EEXIST' || e.message === 'store already initialized') process.exitCode = 3; else throw e; }`;
  const run = () => new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['--input-type=module', '-e', script, config, String(now)], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = ''; child.stderr.on('data', data => { stderr += data; });
    child.on('error', reject); child.on('exit', code => { if (![0, 3].includes(code)) reject(new Error(stderr || `exit ${code}`)); else resolve(code); });
  });
  const codes = await Promise.all([run(), run()]);
  assert.deepEqual(codes.sort(), [0, 3]); assert.equal(p.ledger().attempts.length, 1);
});
