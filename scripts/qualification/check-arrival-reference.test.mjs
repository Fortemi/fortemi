import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { compareArrivalReference, runArrivalReference } from './check-arrival-reference.mjs';

function fixture() {
  return [{ rate: 2, durationSeconds: 2 }, { abortReason: null, executionSettled: true, unresolved: [], undispatched: [],
    observations: Array.from({ length: 4 }, (_, i) => ({ id: String(i), outcome: 'succeeded' })) },
  { started: 4, admitted: 4, boundaryRejected: 0, completed: 4, failed: 0, dropped: 0, httpRequests: 4 },
  ['node', 'k6'].flatMap(engine => Array.from({ length: 4 }, (_, i) => ({ engine, id: String(i) })))];
}
test('equal successful arrival counts require independent receipt agreement without admission', () => {
  const r = compareArrivalReference(...fixture());
  assert.equal(r.status, 'PASS'); assert.equal(r.admitted, false); assert.equal(r.qualificationPassed, false);
});
for (const [name, mutate] of [
  ['k6 dropped iteration', f => { f[2].started--; f[2].completed--; f[2].dropped++; }],
  ['k6 failed request', f => f[2].failed++],
  ['extra request attempt', f => f[2].httpRequests++],
  ['unreconciled raw start', f => f[2].started++],
  ['excessive boundary rejection', f => { f[2].started += 2; f[2].boundaryRejected = 2; }],
  ['incorrect admitted count', f => f[2].admitted--],
  ['missing independent request', f => f[3].pop()],
  ['duplicated request ID', f => f[3][7].id = '0'],
  ['unbound engine', f => f[3][0].engine = 'unknown'],
  ['unresolved Node request', f => f[1].unresolved.push('0')],
  ['Node abort', f => f[1].abortReason = 'schedule-lag'],
]) test(`${name} fails reference comparison`, () => {
  const f = fixture(); mutate(f); assert.equal(compareArrivalReference(...f).status, 'FAIL');
});
test('missing or incomplete k6 evidence is MISSING', () => {
  const f = fixture(); f[2] = null; f[3] = f[3].filter(row => row.engine === 'node');
  assert.equal(compareArrivalReference(...f).status, 'MISSING');
  const g = fixture(); delete g[2].dropped;
  assert.equal(compareArrivalReference(...g).status, 'MISSING');
});
test('one raw endpoint offer is retained and reconciled without HTTP dispatch', () => {
  const f = fixture(); f[2].started++; f[2].boundaryRejected++;
  const result = compareArrivalReference(...f);
  assert.equal(result.status, 'PASS');
  assert.equal(result.scope, 'local-synthetic-admitted-arrival-accounting');
  assert.equal(f[2].started, 5); assert.equal(f[3].length, 8);
});
test('invalid or absent boundary counters cannot establish reconciliation', () => {
  for (const value of [undefined, -1, NaN, 0.5, '0']) {
    const f = fixture(); f[2].boundaryRejected = value;
    assert.equal(compareArrivalReference(...f).status, 'MISSING');
  }
});
test('unsafe reference rate and duration reject', () => {
  for (const config of [{ rate: 3 }, { durationSeconds: 4 }, { rate: 0 }, { durationSeconds: 0 }, { rate: 1.5 }]) {
    assert.throws(() => compareArrivalReference(config));
  }
});
test('absent executable produces actual MISSING receipt and does not reuse output', async () => {
  const outputDirectory = await mkdtemp(join(tmpdir(), 'arrival-reference-test-'));
  try {
    const config = { rate: 1, durationSeconds: 1, outputDirectory, k6Path: join(outputDirectory, 'absent-k6') };
    const result = await runArrivalReference(config);
    assert.equal(result.status, 'MISSING'); assert.equal(result.k6Execution.status, 'MISSING');
    assert.equal(result.receipts.length, 1); assert.equal(result.receipts[0].engine, 'node');
    assert.equal(JSON.parse(await readFile(join(outputDirectory, 'comparison.json'))).status, 'MISSING');
    await assert.rejects(runArrivalReference(config), /EEXIST/);
  } finally { await rm(outputDirectory, { recursive: true, force: true }); }
});
