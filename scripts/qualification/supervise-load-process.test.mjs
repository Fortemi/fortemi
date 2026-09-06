import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createHash } from 'node:crypto';
import { superviseLoadProcess } from './supervise-load-process.mjs';
function fixture(t, code) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dq-watchdog-')); t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const entrypoint = path.join(root, 'worker.mjs'); fs.writeFileSync(entrypoint, code);
  return { entrypoint, entryDigest: `sha256:${createHash('sha256').update(code).digest('hex')}`, input: {},
    durationMs: 1000, graceMs: 50, maxOutputBytes: 1024, heapMiB: 32, cwd: root };
}
test('captures bounded child output and exit without asserting cleanup', async t => {
  const p = fixture(t, "process.stdin.resume(); process.stdin.on('end',()=>console.log('done'));");
  const r = await superviseLoadProcess(p); assert.equal(r.exitCode, 0); assert.equal(r.reason, null);
  assert.equal(r.processGroupGone, true); assert.match(r.captured.toString(), /done/); assert.equal(r.cleanupVerified, false);
});
test('external watchdog terminates event-loop-blocking child', async t => {
  const p = fixture(t, 'while (true) {}'); p.durationMs = 100;
  const r = await superviseLoadProcess(p); assert.equal(r.reason, 'deadline'); assert.equal(r.processGroupGone, true);
});
test('SIGTERM-resistant child is killed after grace', async t => {
  const p = fixture(t, "process.on('SIGTERM',()=>{}); setInterval(()=>{},10);"); p.durationMs = 150;
  const r = await superviseLoadProcess(p); assert.equal(r.reason, 'deadline'); assert.equal(r.processGroupGone, true);
});
test('output budget stops a flooding child and bounds retained bytes', async t => {
  const p = fixture(t, "setInterval(()=>process.stdout.write('x'.repeat(10000)),1);");
  const r = await superviseLoadProcess(p); assert.equal(r.reason, 'output-limit'); assert.equal(r.captured.length, 1024);
});
test('digest mismatch and forbidden injection options reject before spawn', async t => {
  const p = fixture(t, "throw Error('must not run');");
  await assert.rejects(superviseLoadProcess({ ...p, entryDigest: 'sha256:bad' }));
  await assert.rejects(superviseLoadProcess({ ...p, environment: { NODE_OPTIONS: '--inspect' } }));
});
test('pre-aborted supervisor never spawns', async t => {
  const p = fixture(t, "throw Error('must not run');"), c = new AbortController(); c.abort();
  const r = await superviseLoadProcess({ ...p, signal: c.signal }); assert.equal(r.spawned, false);
});
test('executes verified snapshot after original entrypoint is replaced', async t => {
  const p = fixture(t, "process.stdin.resume(); process.stdin.on('end',()=>console.log('verified-original'));");
  const pending = superviseLoadProcess(p);
  fs.writeFileSync(p.entrypoint, "console.log('replacement');");
  const r = await pending; assert.match(r.captured.toString(), /verified-original/);
  assert.ok(!r.captured.toString().includes('replacement'));
  assert.deepEqual(fs.readdirSync(p.cwd), ['worker.mjs']);
});
