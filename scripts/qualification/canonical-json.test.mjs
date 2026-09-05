import assert from 'node:assert/strict';
import test from 'node:test';
import { canonicalJson, parseCanonicalJson, jsonDigest, receiptDigest } from './canonical-json.mjs';

test('UTF-16 sorting including numeric keys and supplementary characters', () => {
  assert.equal(canonicalJson({ '2': 2, '10': 10, '\ue000': 1, '😀': 2 }), '{"10":10,"2":2,"😀":2,"\ue000":1}');
});
test('ECMAScript numeric serialization and unchanged Unicode', () => {
  assert.equal(canonicalJson([-0, 1e30, 0.002, 1e-27, 'e\u0301']), '[0,1e+30,0.002,1e-27,"é"]');
});
for (const value of [NaN, Infinity, undefined, BigInt(1), new Date(), '\ud800', '\udc00', { '\ud800': 1 }, [, 1]]) {
  test(`rejects noncanonical value ${String(value)}`, () => assert.throws(() => canonicalJson(value)));
}
test('persisted parser rejects duplicates, whitespace, alternate encodings and invalid UTF-8', () => {
  for (const text of ['{"a":1,"a":2}', '{"a":1,"\\u0061":2}', '{ "a":1}', '{"a":1}\n', '\ufeff{"a":1}', '1e999', '"\\ud800"']) {
    assert.throws(() => parseCanonicalJson(Buffer.from(text)));
  }
  assert.throws(() => parseCanonicalJson(Buffer.from([0x22, 0xff, 0x22])));
  assert.deepEqual(parseCanonicalJson(Buffer.from('{"a":1}')), { a: 1 });
});
test('digest known vector and exactly specified receipt omissions', () => {
  assert.equal(jsonDigest({}), 'sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a');
  const r = { a: 1, receiptDigest: 'omit', verifier: { name: 'keep', attestation: { x: 1 } } };
  assert.equal(receiptDigest(r), jsonDigest({ a: 1, verifier: { name: 'keep' } }));
  r.verifier.attestation.x = 2; assert.equal(receiptDigest(r), jsonDigest({ a: 1, verifier: { name: 'keep' } }));
  r.verifier.name = 'changed'; assert.notEqual(receiptDigest(r), jsonDigest({ a: 1, verifier: { name: 'keep' } }));
});
