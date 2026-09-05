import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { v2, seal, trust, now } from './signed-test-fixtures.mjs';
import { canonicalJson, jsonDigest } from './canonical-json.mjs';
import { artifactName, verifyEvidence } from './verify-evidence.mjs';

export const limits = { maxFileBytes: 1024 * 1024, maxTotalBytes: 8 * 1024 * 1024, maxFiles: 20 };
export function prepared(t, transform = () => {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'qualification-evidence-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const { a, r } = v2();
  transform(a, r);
  const entry = (artifactType, value, raw = false) => {
    const bytes = raw ? Buffer.from(value) : Buffer.from(canonicalJson(value));
    const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
    const relative = artifactName(digest); fs.writeFileSync(path.join(root, relative), bytes);
    return { artifactType, path: relative, digest, producer: 'test-only', revision: r.verifier.revision };
  };
  const source = entry('fixture', 'synthetic fixture only', true);
  const provenance = entry('fixture', { namespace: 'test-only', synthetic: true });
  a.fixtureDigests = [source.digest]; r.fixtureDigest = source.digest;
  a.environment.cleanDestinationProvenance = provenance.digest; r.cleanDestinationProvenance = provenance.digest;
  r.environmentDigest = jsonDigest(a.environment);
  const [approval] = seal(a, r);
  r.evidence = [source, provenance, entry('approval', approval), entry('runtime-receipt', 'test-only observation', true),
    entry('state-digest', r.actual), entry('telemetry', r.measurements),
    entry('redaction', { attemptId: r.attemptId, runNonce: r.runNonce, verifierRevision: r.verifier.revision, findings: r.measurements.redactionFindings }),
    entry('cleanup', { attemptId: r.attemptId, runNonce: r.runNonce, verifierRevision: r.verifier.revision,
      cleanDestinationProvenance: r.cleanDestinationProvenance, outOfScope: r.measurements.cleanupOutOfScope })];
  const envelopes = () => { const pair = seal(a, r); entry('detached-receipt', pair[1]); return pair; };
  envelopes();
  return { a, r, root, entry, envelopes, check: (budget = limits) => verifyEvidence(...envelopes(), trust, root, budget, now) };
}
