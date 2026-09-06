import test from 'node:test';
import assert from 'node:assert/strict';
import { jsonDigest } from './canonical-json.mjs';
import { compareLoadStateInventory, evaluateLoadState } from './load-state-oracle.mjs';

function fixture() {
  const namespaces = ['tenant-a', 'tenant-b', 'control'];
  const requiredSurfaces = ['records', 'jobs', 'blobs'];
  const expectedBaseline = namespaces.flatMap(namespace => requiredSurfaces.map(surface => ({ namespace, surface,
    resources: [{ id: 'existing', content: { namespace, surface, version: 1 } }] })));
  const expectedAfter = structuredClone(expectedBaseline);
  for (const row of expectedAfter) if (row.namespace !== 'control') row.resources.push({ id: 'created', content: { payload: row.surface } });
  const plan = { namespaces, requiredSurfaces, controlNamespaces: ['control'], expectedBaseline, expectedAfter, minimumSettleMs: 500 };
  return { plan, after: { observedAtMs: 100, inventory: structuredClone(expectedAfter) },
    cleanup: { observedAtMs: 200, inventory: structuredClone(expectedBaseline) },
    settled: { observedAtMs: 700, inventory: structuredClone(expectedBaseline) } };
}

test('complete expected state and both cleanup observations pass without admission', () => {
  const input = fixture(), original = structuredClone(input);
  const result = evaluateLoadState(input);
  assert.equal(result.status, 'PASS');
  assert.equal(result.admitted, false); assert.equal(result.executionAuthorized, false);
  assert.deepEqual(input, original);
});

for (const surface of ['records', 'jobs', 'blobs']) test(`falsification: omitted known ${surface} resource fails`, () => {
  const input = fixture();
  input.after.inventory.find(row => row.namespace === 'tenant-a' && row.surface === surface).resources.pop();
  const result = evaluateLoadState(input);
  assert.equal(result.status, 'FAIL');
  assert.ok(result.checks.after.failures.some(f => f.surface === surface && f.id === 'created' && f.reason === 'expected resource absent'));
});

test('falsification: late write fails after a clean first cleanup observation', () => {
  const input = fixture();
  input.settled.inventory[0].resources.push({ id: 'late', content: { state: 'written after cleanup' } });
  const result = evaluateLoadState(input);
  assert.equal(result.checks.cleanup.status, 'PASS'); assert.equal(result.checks.settled.status, 'FAIL');
  assert.equal(result.status, 'FAIL');
});

test('residual job fails even if later settling observation is clean', () => {
  const input = fixture(); input.cleanup.inventory[1].resources.push({ id: 'residual', content: { state: 'running' } });
  assert.equal(evaluateLoadState(input).checks.cleanup.status, 'FAIL');
});

for (const phase of ['after', 'cleanup', 'settled']) test(`control mutation in ${phase} cannot be hidden`, () => {
  const input = fixture(); input[phase].inventory.find(row => row.namespace === 'control').resources[0].content.version++;
  assert.equal(evaluateLoadState(input).checks[phase].status, 'FAIL');
});

test('control mutation cannot be authorized by rewriting expectedAfter', () => {
  const input = fixture(); input.plan.expectedAfter.find(row => row.namespace === 'control').resources[0].content.version++;
  assert.throws(() => evaluateLoadState(input), /control namespace expectations/);
});

test('absent snapshots and absent required surfaces remain MISSING', () => {
  const input = fixture(); delete input.cleanup; input.settled.inventory.pop();
  const result = evaluateLoadState(input);
  assert.equal(result.status, 'MISSING'); assert.equal(result.checks.cleanup.status, 'MISSING');
  assert.equal(result.checks.settled.missing[0].reason, 'required surface absent');
});

test('FAIL takes precedence while missing collection remains visible', () => {
  const input = fixture(); input.after.inventory[0].resources.pop(); delete input.settled;
  const result = evaluateLoadState(input);
  assert.equal(result.status, 'FAIL'); assert.equal(result.checks.settled.status, 'MISSING');
});

for (const [name, mutate] of [
  ['duplicate surfaces', rows => rows.push(structuredClone(rows[0]))],
  ['duplicate resource IDs', rows => rows[0].resources.push(structuredClone(rows[0].resources[0]))],
  ['unbound surface', rows => rows[0].surface = 'unknown'],
  ['unbound namespace', rows => rows[0].namespace = 'unknown'],
  ['missing invariant', rows => delete rows[0].resources[0].content],
  ['inconsistent digest', rows => rows[0].resources[0].digest = `sha256:${'0'.repeat(64)}`],
  ['noncanonical content', rows => rows[0].resources[0].content = { n: NaN }],
]) test(`invalid observed ${name} fails closed`, () => {
  const input = fixture(); mutate(input.after.inventory);
  assert.equal(evaluateLoadState(input).status, 'FAIL');
});

for (const [name, mutate] of [
  ['omitted expected surface', p => p.expectedAfter.pop()],
  ['omitted baseline surface', p => p.expectedBaseline.pop()],
  ['empty scope', p => p.requiredSurfaces = []],
  ['duplicate scope', p => p.namespaces.push(p.namespaces[0])],
  ['no control', p => p.controlNamespaces = []],
  ['unbound control', p => p.controlNamespaces = ['unknown']],
  ['no exercised namespace', p => p.controlNamespaces = [...p.namespaces]],
  ['zero settling interval', p => p.minimumSettleMs = 0],
  ['unbounded resource inventory', p => p.expectedBaseline[0].resources = new Array(100001)],
]) test(`invalid plan ${name} throws before considering missing evidence`, () => {
  const { plan } = fixture(); mutate(plan);
  assert.throws(() => evaluateLoadState({ plan }));
});

test('canonical content hashing ignores key and inventory ordering and accepts digest-only observations', () => {
  const input = fixture();
  for (const snapshot of [input.after, input.cleanup, input.settled]) {
    snapshot.inventory.reverse();
    for (const row of snapshot.inventory) row.resources = row.resources.reverse().map(resource => ({ id: resource.id,
      digest: jsonDigest(Object.fromEntries(Object.entries(resource.content).reverse())) }));
  }
  assert.equal(evaluateLoadState(input).status, 'PASS');
  assert.equal(compareLoadStateInventory(input.plan, input.plan.expectedBaseline, input.cleanup.inventory).status, 'PASS');
});

for (const [name, mutate] of [
  ['short settling interval', input => input.settled.observedAtMs = 699],
  ['reversed cleanup', input => input.cleanup.observedAtMs = 99],
  ['invalid timestamp', input => input.after.observedAtMs = NaN],
  ['missing timestamp', input => delete input.settled.observedAtMs],
]) test(`${name} fails even with matching state`, () => {
  const input = fixture(); mutate(input);
  assert.equal(evaluateLoadState(input).status, 'FAIL');
});

test('explicit empty surface inventories are complete and unexpected resources fail', () => {
  const input = fixture();
  for (const rows of [input.plan.expectedBaseline, input.plan.expectedAfter, input.after.inventory, input.cleanup.inventory, input.settled.inventory]) {
    rows[0].resources = [];
  }
  assert.equal(evaluateLoadState(input).status, 'PASS');
  input.after.inventory[0].resources.push({ id: 'unexpected', content: null });
  assert.equal(evaluateLoadState(input).status, 'FAIL');
});
