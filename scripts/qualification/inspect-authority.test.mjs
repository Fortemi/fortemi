import assert from 'node:assert/strict';
import test from 'node:test';
import { inspectAuthority } from './inspect-authority.mjs';

import { authority } from './test-fixtures.mjs';

for (const type of ['tenant', 'fault', 'restore', 'skew', 'load']) {
  test(`${type} well-formed synthetic input never authorizes execution`, () => {
    assert.deepEqual(inspectAuthority(authority(type)), { valid: true, executionAuthorized: false, errors: [] });
  });
  test(`${type} rejects removal of each required metric`, () => {
    const input = authority(type);
    for (const t of input.thresholds) {
      const changed = structuredClone(input); changed.thresholds = changed.thresholds.filter(x => x.metric !== t.metric);
      assert.equal(inspectAuthority(changed).valid, false, t.metric);
    }
  });
}
for (const [name, mutate] of [
  ['floating producer revision', a => { a.producers[0].revision = 'main'; }],
  ['unknown field', a => { a.approved = true; }],
  ['missing approval reference', a => { a.approvals = []; }],
  ['duplicate approval', a => { a.approvals.push(a.approvals[0]); }],
  ['duplicate producer', a => { a.producers.push(a.producers[0]); }],
  ['invalid time', a => { a.validFrom = 'yesterday'; }],
  ['reversed window', a => { a.validUntil = a.validFrom; }],
  ['weakened zero invariant', a => { a.thresholds[0].limit = 1; }],
  ['wrong unit', a => { a.thresholds[0].unit = 'percent'; }],
  ['duplicate metric with altered approval', a => { a.thresholds.push({ ...a.thresholds[0], approverRole: 'another' }); }],
  ['nonfinite threshold', a => { a.thresholds[0].limit = Infinity; }],
  ['unsupported schema', a => { a.schemaVersion = '2.0.0'; }],
  ['invalid shard tuple', a => { a.contractTuple.knowledgeShardProfile = 'full-v1'; }],
]) test(`rejects ${name}`, () => { const a = authority(); mutate(a); assert.equal(inspectAuthority(a).valid, false); });
