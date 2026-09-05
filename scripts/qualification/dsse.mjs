import { createHash, createPublicKey, verify } from 'node:crypto';
import { parseCanonicalJson } from './canonical-json.mjs';

export const PAYLOAD_TYPES = Object.freeze({
  authority: 'application/vnd.fortemi.dataset-qualification.authority.v2+json',
  receipt: 'application/vnd.fortemi.dataset-qualification.receipt.v2+json',
});
const MAX_PAYLOAD_BYTES = 8 * 1024 * 1024;
const MAX_SIGNERS = 32;

export function preAuthEncoding(payloadType, payload) {
  const type = Buffer.from(payloadType, 'utf8');
  return Buffer.concat([Buffer.from(`DSSEv1 ${type.length} `), type,
    Buffer.from(` ${payload.length} `), payload]);
}

function exactKeys(value, required, optional = []) {
  if (!value || Object.getPrototypeOf(value) !== Object.prototype
    || required.some(k => !Object.hasOwn(value, k))
    || Object.keys(value).some(k => !required.includes(k) && !optional.includes(k))) {
    throw new Error('invalid envelope or key structure');
  }
}

function decodeBase64(value, maxBytes) {
  if (typeof value !== 'string' || value.length > Math.ceil(maxBytes / 3) * 4
    || !/^[A-Za-z0-9+/_-]*={0,2}$/.test(value)) throw new Error('invalid base64');
  const normalized = value.replaceAll('-', '+').replaceAll('_', '/');
  if ((/[+\/]/.test(value) && /[-_]/.test(value)) || normalized.length % 4 === 1) throw new Error('invalid base64');
  const bytes = Buffer.from(normalized, 'base64');
  const canonical = bytes.toString('base64');
  if (bytes.length > maxBytes || (normalized !== canonical && normalized !== canonical.replace(/=+$/, ''))) throw new Error('invalid base64');
  return bytes;
}

export function publicKeyDigest(publicKeyPem) {
  if (typeof publicKeyPem !== 'string' || !publicKeyPem.startsWith('-----BEGIN PUBLIC KEY-----')) throw new Error('public SPKI PEM required');
  const key = createPublicKey(publicKeyPem);
  if (key.asymmetricKeyType !== 'ed25519') throw new Error('Ed25519 public key required');
  return `sha256:${createHash('sha256').update(key.export({ format: 'der', type: 'spki' })).digest('hex')}`;
}

/** Authenticate one payload against caller-supplied pins, not keys in evidence.
 * This establishes signature provenance only, not qualification admission. */
export function verifyEnvelope(envelope, kind, trustedKeys, requiredSignatures = 1) {
  if (!Object.hasOwn(PAYLOAD_TYPES, kind)) throw new Error('unsupported qualification payload kind');
  exactKeys(envelope, ['payloadType', 'payload', 'signatures']);
  if (envelope.payloadType !== PAYLOAD_TYPES[kind]) throw new Error('unexpected payload type');
  if (!Array.isArray(trustedKeys) || trustedKeys.length === 0 || trustedKeys.length > MAX_SIGNERS) throw new Error('bounded external key pins required');
  if (!Number.isInteger(requiredSignatures) || requiredSignatures < 1 || requiredSignatures > trustedKeys.length) throw new Error('invalid signature threshold');
  if (!Array.isArray(envelope.signatures) || envelope.signatures.length === 0 || envelope.signatures.length > MAX_SIGNERS) throw new Error('bounded signatures required');
  const pins = new Map();
  for (const pin of trustedKeys) {
    exactKeys(pin, ['digest', 'publicKeyPem']);
    const digest = publicKeyDigest(pin.publicKeyPem);
    if (digest !== pin.digest || pins.has(digest)) throw new Error('mismatched or duplicate key pin');
    pins.set(digest, createPublicKey(pin.publicKeyPem));
  }
  const payload = decodeBase64(envelope.payload, MAX_PAYLOAD_BYTES);
  const signed = preAuthEncoding(envelope.payloadType, payload);
  const accepted = new Set();
  for (const signature of envelope.signatures) {
    exactKeys(signature, ['sig'], ['keyid']);
    if (signature.keyid !== undefined && typeof signature.keyid !== 'string') throw new Error('invalid key hint');
    const bytes = decodeBase64(signature.sig, 64);
    if (bytes.length !== 64) throw new Error('invalid Ed25519 signature length');
    // keyid is an unauthenticated hint and cannot grant trust or add a signer.
    for (const [digest, key] of pins) if (verify(null, signed, key, bytes)) accepted.add(digest);
  }
  if (accepted.size < requiredSignatures) throw new Error('signature threshold not met');
  // Parse the exact authenticated bytes once; never reload the envelope payload.
  return { document: parseCanonicalJson(payload), signerDigests: [...accepted].sort() };
}
