#!/usr/bin/env node
import fs from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(new URL('../../mcp-server/package.json', import.meta.url));
const Ajv = require('ajv/dist/2020.js');
// Planning schemas use conditional subschemas whose types/required properties
// are inherited from the root. Keep schema keyword checking and format validation.
const ajv = new Ajv({ strictTypes: false, strictRequired: false, allErrors: true });
require('ajv-formats')(ajv);
const schema = JSON.parse(fs.readFileSync(new URL('../../contracts/dataset-qualification/1.0.0/schemas/authority.schema.json', import.meta.url)));
const validate = ajv.compile(schema);
const revision = /^(?:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/;
const zeroMetrics = new Set(['unauthorizedReads', 'unauthorizedMutations', 'redactionFindings',
  'cleanupOutOfScope', 'unclassifiedOutcomes', 'duplicateEffects', 'canonicalMismatches',
  'rejectedTupleMutations', 'limitPlusOneMutations']);
const rates = new Set(['supportedTuplePassRate', 'unsupportedPrewriteRejectRate']);

/** Structural and policy checks only; never an approval or a PASS receipt. */
export function inspectAuthority(authority) {
  const errors = [];
  if (!validate(authority)) {
    return { valid: false, executionAuthorized: false, errors: validate.errors.map(e => `${e.instancePath}: ${e.message}`) };
  }
  const from = Date.parse(authority.validFrom), until = Date.parse(authority.validUntil);
  if (!Number.isFinite(from) || !Number.isFinite(until) || from >= until) errors.push('authority validity window must increase');
  const components = [...authority.producers, ...authority.consumers, authority.verifier];
  for (const component of components) {
    if (!revision.test(component.revision)) errors.push(`immutable revision required: ${component.name}`);
    if (!/^[\w.-]+\/[\w.-]+$/.test(component.repository)) errors.push(`owner/repository required: ${component.name}`);
  }
  for (const [role, list] of [['producer', authority.producers], ['consumer', authority.consumers]]) {
    if (new Set(list.map(c => `${c.repository}/${c.name}`)).size !== list.length) errors.push(`duplicate ${role} identity`);
  }
  for (const field of ['approvals', 'fixtureDigests']) {
    if (new Set(authority[field]).size !== authority[field].length) errors.push(`duplicate ${field}`);
  }
  const seen = new Set();
  for (const threshold of authority.thresholds) {
    const { metric, operator, limit, unit } = threshold;
    if (seen.has(metric)) errors.push(`duplicate threshold: ${metric}`);
    seen.add(metric);
    if (!Number.isFinite(limit) || limit < 0) errors.push(`invalid limit: ${metric}`);
    if (zeroMetrics.has(metric) && (operator !== 'eq' || limit !== 0 || unit !== 'count')) errors.push(`zero-count invariant required: ${metric}`);
    if (rates.has(metric) && (operator !== 'eq' || limit !== 1 || unit !== 'ratio')) errors.push(`complete tuple coverage required: ${metric}`);
    if (['rpoSeconds', 'rtoSeconds'].includes(metric) && (!['lt', 'lte'].includes(operator) || unit !== 'seconds')) errors.push(`recovery upper bound required: ${metric}`);
  }
  if (!seen.has('redactionFindings') || !seen.has('cleanupOutOfScope')) errors.push('redaction and cleanup thresholds required for every qualification type');
  return { valid: errors.length === 0, executionAuthorized: false, errors };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('usage: node scripts/qualification/inspect-authority.mjs AUTHORITY.json');
  const result = inspectAuthority(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
  console.log(JSON.stringify(result, null, 2));
  if (!result.valid) process.exitCode = 1;
}
