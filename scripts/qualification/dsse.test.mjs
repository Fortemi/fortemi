import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import test from 'node:test';
import { canonicalJson } from './canonical-json.mjs';
import { PAYLOAD_TYPES, preAuthEncoding, publicKeyDigest, verifyEnvelope } from './dsse.mjs';

function keyPair() {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const publicKeyPem = publicKey.export({ format: 'pem', type: 'spki' });
  return { privateKey, pin: { publicKeyPem, digest: publicKeyDigest(publicKeyPem) } };
}
const first = keyPair(), second = keyPair(), outsider = keyPair();
function envelope(keys = [first], text = canonicalJson({ schemaVersion: '2.0.0', example: 'test only' }), kind = 'receipt') {
  const payload = Buffer.from(text), type = PAYLOAD_TYPES[kind];
  // Construct the protocol string separately from the implementation under test.
  const message = Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(type)} ${type} ${payload.length} `), payload]);
  return { payloadType: type, payload: payload.toString('base64'), signatures: keys.map(k => ({ sig: sign(null, message, k.privateKey).toString('base64') })) };
}
test('DSSE reference PAE vector and UTF-8 byte lengths', () => {
  assert.equal(preAuthEncoding('http://example.com/HelloWorld', Buffer.from('hello world')).toString(), 'DSSEv1 29 http://example.com/HelloWorld 11 hello world');
  assert.equal(preAuthEncoding('é', Buffer.from('😀')).toString(), 'DSSEv1 2 é 4 😀');
});
test('valid detached signature authenticates exact canonical document', () => {
  const result = verifyEnvelope(envelope(), 'receipt', [first.pin]);
  assert.deepEqual(result.document, { schemaVersion: '2.0.0', example: 'test only' });
  assert.deepEqual(result.signerDigests, [first.pin.digest]);
  assert.equal(Object.hasOwn(result, 'admitted'), false);
});
test('supports URL-safe base64 and ignores forged key hint', () => {
  const e = envelope(); e.payload = Buffer.from(e.payload, 'base64').toString('base64url');
  e.signatures[0].sig = Buffer.from(e.signatures[0].sig, 'base64').toString('base64url');
  e.signatures[0].keyid = outsider.pin.digest;
  assert.deepEqual(verifyEnvelope(e, 'receipt', [first.pin]).signerDigests, [first.pin.digest]);
});
test('key rotation requires current pins; removed signer cannot authenticate', () => {
  assert.throws(() => verifyEnvelope(envelope(), 'receipt', [second.pin]), /threshold/);
  assert.equal(verifyEnvelope(envelope([second]), 'receipt', [second.pin]).signerDigests[0], second.pin.digest);
});
test('distinct threshold cannot be satisfied by duplicate signatures', () => {
  const duplicate = envelope(); duplicate.signatures.push(duplicate.signatures[0]);
  assert.throws(() => verifyEnvelope(duplicate, 'receipt', [first.pin, second.pin], 2), /threshold/);
  assert.equal(verifyEnvelope(envelope([first, second]), 'receipt', [first.pin, second.pin], 2).signerDigests.length, 2);
});
for (const [name, mutate] of [
  ['payload tamper', e => { e.payload = Buffer.from('{}').toString('base64'); }],
  ['payload type substitution', e => { e.payloadType = PAYLOAD_TYPES.authority; }],
  ['invalid signature', e => { e.signatures[0].sig = Buffer.alloc(64).toString('base64'); }],
  ['malformed base64', e => { e.payload += '!'; }],
  ['empty signature set', e => { e.signatures = []; }],
  ['signature limit', e => { e.signatures = Array(33).fill(e.signatures[0]); }],
  ['embedded public key', e => { e.publicKey = first.pin.publicKeyPem; }],
]) test(`rejects ${name}`, () => {
  const e = envelope(); mutate(e); assert.throws(() => verifyEnvelope(e, 'receipt', [first.pin]));
});
test('rejects valid signatures on duplicate-key or noncanonical JSON', () => {
  for (const text of ['{"a":1,"a":2}', '{ "a":1}', '"\\ud800"']) assert.throws(() => verifyEnvelope(envelope([first], text), 'receipt', [first.pin]));
});
test('rejects key substitution, duplicate pins, missing trust and invalid thresholds', () => {
  const e = envelope();
  assert.throws(() => verifyEnvelope(e, 'receipt', [{ ...first.pin, digest: second.pin.digest }]), /pin/);
  assert.throws(() => verifyEnvelope(e, 'receipt', [first.pin, first.pin]), /pin/);
  assert.throws(() => verifyEnvelope(e, 'receipt', []));
  assert.throws(() => verifyEnvelope(e, 'receipt', [first.pin], 0));
  assert.throws(() => verifyEnvelope(e, 'receipt', [first.pin], 2));
  assert.throws(() => verifyEnvelope(e, 'unknown', [first.pin]));
});
