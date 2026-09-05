import { createHash } from 'node:crypto';

// RFC 8785 uses ECMAScript number serialization and UTF-16 property ordering.
// This implementation is deliberately separate from the runtime receipt writer.
export function canonicalJson(value) {
  if (value === null || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('nonfinite JSON number');
    return JSON.stringify(value);
  }
  if (typeof value === 'string') {
    for (let i = 0; i < value.length; i++) {
      const c = value.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff) {
        const next = value.charCodeAt(++i);
        if (!(next >= 0xdc00 && next <= 0xdfff)) throw new Error('unpaired Unicode surrogate');
      } else if (c >= 0xdc00 && c <= 0xdfff) throw new Error('unpaired Unicode surrogate');
    }
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    if (Object.keys(value).length !== value.length) throw new Error('non-JSON array');
    return `[${Array.from(value, canonicalJson).join(',')}]`;
  }
  if (value && Object.getPrototypeOf(value) === Object.prototype) {
    return `{${Object.keys(value).sort().map(k => `${canonicalJson(k)}:${canonicalJson(value[k])}`).join(',')}}`;
  }
  throw new Error('non-JSON value');
}

export function jsonDigest(value) {
  return `sha256:${createHash('sha256').update(canonicalJson(value), 'utf8').digest('hex')}`;
}

// Persisted admission inputs must already be canonical UTF-8. Comparing with
// the input bytes rejects duplicate keys before a lossy parse can be trusted.
export function parseCanonicalJson(bytes) {
  const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  const value = JSON.parse(text);
  if (canonicalJson(value) !== text) throw new Error('input must be exact canonical JSON bytes');
  return value;
}

export function receiptDigest(receipt) {
  const value = structuredClone(receipt);
  delete value.receiptDigest;
  if (value.verifier) delete value.verifier.attestation;
  return jsonDigest(value);
}
