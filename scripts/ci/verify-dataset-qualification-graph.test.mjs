import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { validateGraph } from './verify-dataset-qualification-graph.mjs';

const graph = JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-qualification/1.0.0/readiness.json', import.meta.url)));
test('publication precedes children and closure follows them', () => {
  const order = validateGraph(graph);
  assert.ok(order.indexOf('authority-published') < order.indexOf('tenant'));
  assert.ok(order.indexOf('load') < order.indexOf('epic-closed'));
});
for (const [name, mutate, error] of [
  ['circular closure dependency', g => g.nodes.find(n => n.id === 'tenant').requires.push('epic-closed'), /cycle/],
  ['unknown dependency', g => g.nodes[0].requires.push('absent'), /unknown dependency/],
  ['duplicate node', g => g.nodes.push(g.nodes[0]), /duplicate node/],
  ['missing hosted isolation', g => { g.nodes.find(n => n.id === 'fault-hosted').requires = ['fault-nonhosted']; }, /bypasses tenant/],
  ['missing rollback certificate', g => { g.nodes.find(n => n.id === 'skew-rollback').requires = ['skew-midrun']; }, /bypasses restore/],
  ['missing load approval', g => { const n = g.nodes.find(n => n.id === 'load'); n.requires = n.requires.filter(x => x !== 'load-envelope-approved'); }, /bypasses load-envelope-approved/],
  ['omitted closure child', g => { const n = g.nodes.find(n => n.id === 'epic-closed'); n.requires = n.requires.filter(x => x !== 'load'); }, /omitted from closure/],
  ['consumer issue mismatch', g => { g.consumers[0].issue += '0'; }, /consumer issue missing/],
]) test(`rejects ${name}`, () => { const changed = structuredClone(graph); mutate(changed); assert.throws(() => validateGraph(changed), error); });
