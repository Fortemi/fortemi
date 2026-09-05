#!/usr/bin/env node
import fs from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { canonicalJson } from './canonical-json.mjs';

const require = createRequire(new URL('../../mcp-server/package.json', import.meta.url));
const Ajv = require('ajv/dist/2020.js');
// Planning schemas use conditional subschemas whose types/required properties
// are inherited from the root. Keep schema keyword checking and format validation.
const ajv = new Ajv({ strictTypes: false, strictRequired: false, allErrors: true });
require('ajv-formats')(ajv);
const validators = new Map(['1.0.0', '2.0.0'].map(version => [version, ajv.compile(JSON.parse(fs.readFileSync(new URL(`../../contracts/dataset-qualification/${version}/schemas/authority.schema.json`, import.meta.url))))]));
export const schemaDigestsV2 = Object.freeze(Object.fromEntries(['authority', 'receipt'].map(name => [name,
  `sha256:${createHash('sha256').update(fs.readFileSync(new URL(`../../contracts/dataset-qualification/2.0.0/schemas/${name}.schema.json`, import.meta.url))).digest('hex')}`])));
const revision = /^(?:[a-f0-9]{40}|sha256:[a-f0-9]{64})$/;
const zeroMetrics = new Set(['unauthorizedReads', 'unauthorizedMutations', 'redactionFindings',
  'cleanupOutOfScope', 'unclassifiedOutcomes', 'duplicateEffects', 'canonicalMismatches',
  'rejectedTupleMutations', 'limitPlusOneMutations']);
const rates = new Set(['supportedTuplePassRate', 'unsupportedPrewriteRejectRate']);

/** Structural and policy checks only; never an approval or a PASS receipt. */
export function inspectAuthority(authority) {
  const errors = [];
  const validate = validators.get(authority?.schemaVersion);
  if (!validate) return { valid: false, executionAuthorized: false, errors: ['unsupported authority schema'] };
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
  for (const field of authority.schemaVersion === '1.0.0' ? ['approvals', 'fixtureDigests'] : ['fixtureDigests']) {
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
  if (authority.schemaVersion === '2.0.0') {
    if (canonicalJson(authority.schemaDigests) !== canonicalJson(schemaDigestsV2)) errors.push('authority schema digest mismatch');
    const cellIds = new Set();
    for (const cell of authority.cells) {
      if (cellIds.has(cell.cellId)) errors.push('duplicate cell identity');
      cellIds.add(cell.cellId);
      for (const [role, components] of [['producer', authority.producers], ['consumer', authority.consumers]]) {
        if (!components.some(c => canonicalJson(c) === canonicalJson(cell[role]))) errors.push(`unbound cell ${role}`);
      }
      if (cell.plane === 'knowledge-shard') {
        if (!['core-v1', 'record-v1', 'full-v1'].includes(cell.profile) || cell.profile !== authority.contractTuple.knowledgeShardProfile) errors.push('cell shard profile mismatch');
      } else if (['core-v1', 'record-v1', 'full-v1'].includes(cell.profile) || authority.contractTuple.knowledgeShardProfile !== 'not-applicable') errors.push('cell plane mismatch');
      if (!cell.supported && (cell.expected.terminalState !== 'rejected' || cell.expected.mutationCount !== 0)) errors.push('unsupported cell must reject before mutation');
      if (cell.acceptanceIds.some(id => !id.startsWith(`DQ-${authority.qualificationType.toUpperCase()}-AC-`))) errors.push('cell acceptance type mismatch');
    }
  }
  return { valid: errors.length === 0, executionAuthorized: false, errors };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw new Error('usage: node scripts/qualification/inspect-authority.mjs AUTHORITY.json');
  const result = inspectAuthority(JSON.parse(fs.readFileSync(process.argv[2], 'utf8')));
  console.log(JSON.stringify(result, null, 2));
  if (!result.valid) process.exitCode = 1;
}
