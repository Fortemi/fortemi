import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { seal, trust, now } from './signed-test-fixtures.mjs';
import { verifyEvidence } from './verify-evidence.mjs';

import { prepared, limits } from './evidence-test-fixtures.mjs';

test('verifies complete content-addressed evidence without admitting receipt', t => {
  const p = prepared(t); const before = fs.readdirSync(p.root);
  const result = p.check(); assert.equal(result.evidenceVerified, true, result.errors.join(';'));
  assert.equal(result.admitted, false); assert.ok(result.bytesRead > 0);
  assert.deepEqual(fs.readdirSync(p.root), before);
});
test('missing file stays distinct from corrupted file', t => {
  const p = prepared(t), target = path.join(p.root, p.r.evidence[0].path);
  fs.unlinkSync(target); const missing = p.check();
  assert.equal(missing.evidenceVerified, false); assert.equal(missing.errors.length, 0);
  assert.deepEqual(missing.missing, [p.r.evidence[0].path]);
  fs.writeFileSync(target, 'corrupt'); const corrupt = p.check();
  assert.equal(corrupt.missing.length, 0); assert.ok(corrupt.errors.some(e => e.includes('digest mismatch')));
});
test('rejects traversal even with a valid signature', t => {
  const p = prepared(t); p.r.evidence[0].path = '../escape';
  assert.ok(p.check().errors.some(e => e.includes('non-content-addressed')));
});
test('rejects symlink to evidence outside the root', t => {
  const p = prepared(t); const file = path.join(p.root, p.r.evidence[0].path);
  const outside = fs.mkdtempSync(path.join(os.tmpdir(), 'qualification-outside-'));
  t.after(() => fs.rmSync(outside, { recursive: true, force: true }));
  const elsewhere = path.join(outside, 'evidence'); fs.renameSync(file, elsewhere); fs.symlinkSync(elsewhere, file);
  assert.equal(p.check().evidenceVerified, false);
});
test('rejects nonregular evidence and symlink root', t => {
  const p = prepared(t); const file = path.join(p.root, p.r.evidence[0].path);
  fs.unlinkSync(file); fs.mkdirSync(file); assert.ok(p.check().errors.some(e => e.includes('regular file')));
  const alias = path.join(p.root, 'alias'); fs.symlinkSync(p.root, alias);
  assert.equal(verifyEvidence(...seal(p.a, p.r), trust, alias, limits, now).evidenceVerified, false);
});
test('enforces file, total-byte and count budgets', t => {
  const p = prepared(t);
  for (const budget of [{ ...limits, maxFileBytes: 1 }, { ...limits, maxFileBytes: 5000, maxTotalBytes: 5000 },
    { ...limits, maxFiles: 1 }, { ...limits, maxTotalBytes: Infinity }, {}]) {
    const result = p.check(budget); assert.equal(result.evidenceVerified, false); assert.ok(result.errors.length);
  }
});
test('rejects independently signed but inconsistent redaction summary', t => {
  const p = prepared(t); p.r.evidence = p.r.evidence.filter(e => e.artifactType !== 'redaction');
  p.r.evidence.push(p.entry('redaction', { attemptId: p.r.attemptId, runNonce: p.r.runNonce,
    verifierRevision: p.r.verifier.revision, findings: 1 }));
  assert.ok(p.check().errors.includes('evidence summary mismatch: redaction'));
});
test('rejects approval substitution, unbound provenance and ambiguous summary', t => {
  const p = prepared(t); p.r.evidence = p.r.evidence.filter(e => e.artifactType !== 'approval');
  p.r.evidence.push(p.entry('approval', { fake: true }));
  assert.ok(p.check().errors.includes('approval reference missing from evidence inventory'));
  const q = prepared(t); q.r.evidence = q.r.evidence.filter(e => e.digest !== q.r.cleanDestinationProvenance);
  assert.ok(q.check().errors.includes('clean destination provenance missing from inventory'));
  const z = prepared(t); z.r.evidence.push(z.entry('telemetry', { unrelated: 1 }));
  assert.ok(z.check().errors.includes('ambiguous evidence summary: telemetry'));
});
test('authentication failure stops before filesystem access', t => {
  const p = prepared(t), [a, r] = seal(p.a, p.r); r.signatures[0].sig = Buffer.alloc(64).toString('base64');
  const result = verifyEvidence(a, r, trust, '/nonexistent-evidence-root', limits, now);
  assert.equal(result.authenticated, false); assert.ok(result.errors.some(e => e.includes('signature')));
});

test('rejects floating evidence revisions', t => {
  const p = prepared(t); p.r.evidence[0].revision = 'main';
  assert.ok(p.check().errors.some(e => e.includes('immutable evidence revision')));
});
