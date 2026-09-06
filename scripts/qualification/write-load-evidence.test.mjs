import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createHash } from 'node:crypto';
import { createLoadEvidenceWriter } from './write-load-evidence.mjs';
function fixture(t, overrides = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dq-artifacts-'));
  const writer = createLoadEvidenceWriter(root, { maxFileBytes: 100, maxTotalBytes: 200, maxFiles: 4, ...overrides });
  t.after(() => { writer.close(); fs.rmSync(root, { recursive: true, force: true }); }); return { root, writer };
}
test('publishes exact immutable-addressed bytes and verifies idempotent existing artifacts', t => {
  const { root, writer } = fixture(t), bytes = Buffer.from('{"raw":true}');
  const a = writer.write(bytes), b = writer.write(bytes); assert.deepEqual(a, b);
  assert.equal(a.digest, `sha256:${createHash('sha256').update(bytes).digest('hex')}`);
  assert.deepEqual(fs.readFileSync(path.join(root, a.path)), bytes);
  assert.equal(fs.statSync(path.join(root, a.path)).mode & 0o222, 0);
  assert.deepEqual(fs.readdirSync(root), [a.path]); assert.deepEqual(writer.usage(), { attemptedFiles: 2, attemptedBytes: bytes.length * 2 });
});
test('conflicting bytes and symlinks are never overwritten', t => {
  const { root, writer } = fixture(t), bytes = Buffer.from('good');
  const name = `sha256-${createHash('sha256').update(bytes).digest('hex')}`;
  fs.writeFileSync(path.join(root, name), 'evil'); assert.throws(() => writer.write(bytes));
  assert.equal(fs.readFileSync(path.join(root, name), 'utf8'), 'evil');
  fs.unlinkSync(path.join(root, name)); fs.symlinkSync('/dev/null', path.join(root, name));
  assert.throws(() => writer.write(bytes)); assert.equal(fs.readlinkSync(path.join(root, name)), '/dev/null');
  assert.deepEqual(fs.readdirSync(root), [name]);
});
test('rejects oversized, excessive and closed writes', t => {
  const { writer } = fixture(t, { maxFileBytes: 2, maxTotalBytes: 3, maxFiles: 2 });
  assert.throws(() => writer.write(Buffer.alloc(3))); writer.write(Buffer.from('ab'));
  assert.throws(() => writer.write(Buffer.from('ab'))); writer.write(Buffer.from('c'));
  assert.throws(() => writer.write(Buffer.alloc(0))); writer.close(); assert.throws(() => writer.write(Buffer.from('x')));
});
test('directory replacement cannot redirect writes', t => {
  const { root, writer } = fixture(t), moved = `${root}-moved`;
  fs.renameSync(root, moved); fs.mkdirSync(root);
  t.after(() => fs.rmSync(moved, { recursive: true, force: true }));
  const a = writer.write(Buffer.from('anchored')); assert.ok(fs.existsSync(path.join(moved, a.path)));
  assert.deepEqual(fs.readdirSync(root), []);
});
test('failure before publication removes only its temporary file', t => {
  const { root, writer } = fixture(t), original = fs.linkSync;
  fs.writeFileSync(path.join(root, 'sentinel'), 'keep');
  try { fs.linkSync = () => { throw new Error('injected publication failure'); }; assert.throws(() => writer.write(Buffer.from('x'))); }
  finally { fs.linkSync = original; }
  assert.deepEqual(fs.readdirSync(root), ['sentinel']); assert.equal(writer.usage().attemptedFiles, 1);
});
test('published bytes are accepted through existing signed evidence verification', async t => {
  const { prepared } = await import('./evidence-test-fixtures.mjs');
  const f = prepared(t), writer = createLoadEvidenceWriter(f.root, { maxFileBytes: 1000, maxTotalBytes: 2000, maxFiles: 2 });
  t.after(() => writer.close());
  const artifact = writer.write(Buffer.from('synthetic raw load observations'));
  f.r.evidence = f.r.evidence.filter(e => e.artifactType !== 'runtime-receipt');
  f.r.evidence.push({ artifactType: 'runtime-receipt', path: artifact.path, digest: artifact.digest,
    producer: 'test-only', revision: f.r.verifier.revision });
  f.envelopes(); assert.equal(f.check().evidenceVerified, true);
});
