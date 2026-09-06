import { LOAD_OPERATIONS } from './load-request-summary.mjs';
import { LOAD_PHASES, evaluateLoadRequests } from './evaluate-load-requests.mjs';
import { LOAD_TELEMETRY, evaluateLoadTelemetry } from './evaluate-load-telemetry.mjs';
import { canonicalJson, jsonDigest } from './canonical-json.mjs';
import { compareLoadUsd } from './load-attempt-budget.mjs';

const object = x => x !== null && typeof x === 'object' && !Array.isArray(x) && Object.getPrototypeOf(x) === Object.prototype;
const integer = (n, min, max, name) => { if (!Number.isSafeInteger(n) || n < min || n > max) throw Error(`invalid ${name}`); };
const keys = (x, expected, name) => { if (!object(x) || Object.keys(x).length !== expected.length || expected.some(k => !Object.hasOwn(x, k))) throw Error(`invalid keys: ${name}`); };
const id = x => typeof x === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$/.test(x);
const threshold = (operator, unit) => ({ operator, limit: null, unit });
const requestThresholds = () => ({ minimumSamples: 1000,
  latencyP50: threshold('lte', 'milliseconds'), latencyP95: threshold('lte', 'milliseconds'),
  latencyP99: threshold('lte', 'milliseconds'), throughputPerSecond: threshold('gte', 'operations/second'),
  errorRate: threshold('lte', 'ratio') });

/** Editable local authoring format. It is not an approval or receipt schema. */
export function loadConfigTemplate() {
  return { version: 1, defaults: {
    stage: 'calibration', repetitions: 1, seed: 1,
    phases: [0.5, 1, 2].map((ratePerSecond, i) => ({ name: `step-${i + 1}`, ratePerSecond, durationMs: 120000, windowMs: 120000 })),
    mix: Object.fromEntries(LOAD_OPERATIONS.map((op, i) => [op, [20, 25, 10, 10, 5, 5, 10, 5, 10][i]])),
    runner: { drainMs: 60000, maxConcurrency: 1, maxScheduleLagMs: 1000, pollMs: 10, safetyTimeoutMs: 1000 },
    cleanup: { timeoutMs: 300000, settleMs: 1000 },
    budgets: { maxLogicalOperations: 100000, maxHttpRequests: 1000000, maxProviderAttempts: 100000,
      maxEvidenceBytes: 67108864, maxEvidenceFiles: 10000, maxRunMs: 43200000 },
    provider: { maxCostUsd: null, priceRevision: null },
    operationBounds: Object.fromEntries(LOAD_OPERATIONS.map(op => [op, { httpRequests: null, providerAttempts: null, evidenceBytes: null, deadlineMs: null }])),
    requestPolicy: requestThresholds(),
    perOperation: Object.fromEntries(LOAD_OPERATIONS.map(op => [op, {}])),
    telemetry: { maxGapMs: 1000, maxAgeMs: 1000,
      thresholds: Object.fromEntries(Object.entries(LOAD_TELEMETRY).map(([metric, [unit, direction]]) => [metric, threshold(direction === 'lower' ? 'gte' : 'lte', unit)])) },
    environment: { target: null, generator: null, observer: null, runtimeRevision: null, fixtureDigest: null,
      runtimeTenants: ['load-a', 'load-b'], controlNamespace: 'load-control' },
  }, profiles: { calibration: {}, qualification: { stage: 'qualification', repetitions: 3,
    phases: LOAD_PHASES.map(name => ({ name, ratePerSecond: 2, durationMs: 120000, windowMs: 120000 })) } } };
}

function merge(base, patch, location = '', vocabulary = base) {
  if (!object(patch)) throw Error(`object override required: ${location}`);
  const result = structuredClone(base);
  for (const [key, value] of Object.entries(patch)) {
    if (!Object.hasOwn(vocabulary, key) || ['__proto__', 'constructor', 'prototype'].includes(key)) throw Error(`unknown configuration key: ${location}${key}`);
    const shape = location === 'perOperation.' ? requestThresholds() : vocabulary[key];
    if (object(shape)) {
      if (!object(value)) throw Error(`object override required: ${location}${key}`);
      result[key] = merge(object(base[key]) ? base[key] : {}, value, `${location}${key}.`, shape);
    } else result[key] = structuredClone(value);
  }
  return result;
}

function validateDefaults(value, shape, location = '') {
  keys(value, Object.keys(shape), location || 'defaults');
  for (const key of Object.keys(shape)) {
    if (location === 'perOperation.') merge({}, value[key], `${location}${key}.`, requestThresholds());
    else if (object(shape[key])) validateDefaults(value[key], shape[key], `${location}${key}.`);
  }
}

/** Defaults < named profile < explicit path=JSON overrides, applied in order.
 * Common-policy overrides update every operation; a later operation override wins.
 * No ambient environment and no implicit nested template defaults.
 */
export function resolveLoadConfig(document, profile, overrides = []) {
  keys(document, ['version', 'defaults', 'profiles'], 'document');
  if (document.version !== 1 || !object(document.profiles) || !id(profile)
    || !Object.hasOwn(document.profiles, profile) || Object.keys(document.profiles).length > 32) throw Error('unknown config version or profile');
  if (!Array.isArray(overrides) || overrides.length > 100) throw Error('too many overrides');
  const vocabulary = loadConfigTemplate().defaults;
  validateDefaults(document.defaults, vocabulary);
  for (const [name, patch] of Object.entries(document.profiles)) {
    if (!id(name)) throw Error('invalid profile name');
    merge(document.defaults, patch, '', vocabulary);
  }
  let config = merge(document.defaults, document.profiles[profile], '', vocabulary);
  const sparse = config.perOperation;
  config.perOperation = Object.fromEntries(LOAD_OPERATIONS.map(op => [op,
    merge(config.requestPolicy, sparse[op], `perOperation.${op}.`, requestThresholds())]));
  const seen = new Set();
  for (const expression of overrides) {
    if (typeof expression !== 'string' || expression.length > 65536) throw Error('bounded path=JSON override required');
    const equal = expression.indexOf('=');
    if (equal < 1) throw Error('path=JSON override required');
    const path = expression.slice(0, equal), parts = path.split('.');
    if (seen.has(path)) throw Error(`duplicate override: ${path}`);
    seen.add(path);
    let patch = JSON.parse(expression.slice(equal + 1));
    for (const part of [...parts].reverse()) {
      if (!part || ['__proto__', 'constructor', 'prototype'].includes(part)) throw Error(`unknown override: ${path}`);
      patch = { [part]: patch };
    }
    config = merge(config, patch);
    if (parts[0] === 'requestPolicy') for (const op of LOAD_OPERATIONS)
      config.perOperation[op] = merge(config.perOperation[op], patch.requestPolicy);
  }
  return config;
}

/** Pure preflight. Null selection values are reported, never invented.
 * maxLogicalOperations is per evaluator invocation (one trial); expanded HTTP,
 * provider, evidence-byte and elapsed-time budgets cover every repetition.
 * Evidence bytes include declared operation bounds and the compiled plan;
 * publication envelope bytes remain subject to the bounded writer. File counts
 * include the runner publication strategy, including raw and verdict records.
 */
export function compileLoadConfig(document, profile, overrides = []) {
  const config = resolveLoadConfig(document, profile, overrides), missing = [], warnings = [];
  if (!['rehearsal', 'calibration', 'qualification'].includes(config.stage)) throw Error('invalid stage');
  integer(config.repetitions, 1, 10, 'repetitions'); integer(config.seed, 0, 4294967295, 'seed');
  keys(config.mix, LOAD_OPERATIONS, 'mix');
  for (const [op, n] of Object.entries(config.mix)) integer(n, 1, 10000, `mix.${op}`);
  const weight = Object.values(config.mix).reduce((a, b) => a + b, 0);
  keys(config.runner, ['drainMs', 'maxConcurrency', 'maxScheduleLagMs', 'pollMs', 'safetyTimeoutMs'], 'runner');
  for (const [k, cap] of Object.entries({ drainMs: 300000, maxConcurrency: 32, maxScheduleLagMs: 43200000, pollMs: 1000, safetyTimeoutMs: 10000 })) integer(config.runner[k], 1, cap, `runner.${k}`);
  keys(config.cleanup, ['timeoutMs', 'settleMs'], 'cleanup');
  integer(config.cleanup.timeoutMs, 1, 300000, 'cleanup.timeoutMs'); integer(config.cleanup.settleMs, 1, config.cleanup.timeoutMs, 'cleanup.settleMs');
  const caps = { maxLogicalOperations: 100000, maxHttpRequests: 10000000, maxProviderAttempts: 1000000,
    maxEvidenceBytes: 1024 ** 3, maxEvidenceFiles: 10000, maxRunMs: 43200000 };
  keys(config.budgets, Object.keys(caps), 'budgets');
  for (const [k, cap] of Object.entries(caps)) integer(config.budgets[k], 1, cap, `budgets.${k}`);
  keys(config.provider, ['maxCostUsd', 'priceRevision'], 'provider');
  if (config.provider.maxCostUsd === null) missing.push('provider.maxCostUsd');
  else compareLoadUsd(config.provider.maxCostUsd, '0');
  if (config.provider.priceRevision === null) missing.push('provider.priceRevision');
  else if (typeof config.provider.priceRevision !== 'string' || !config.provider.priceRevision.trim()
    || config.provider.priceRevision.length > 256) throw Error('invalid provider.priceRevision');
  keys(config.operationBounds, LOAD_OPERATIONS, 'operationBounds');
  for (const [op, bounds] of Object.entries(config.operationBounds)) {
    keys(bounds, ['httpRequests', 'providerAttempts', 'evidenceBytes', 'deadlineMs'], `operationBounds.${op}`);
    for (const [k, n] of Object.entries(bounds)) {
      if (n === null) missing.push(`operationBounds.${op}.${k}`);
      else integer(n, k === 'providerAttempts' ? 0 : 1, k === 'deadlineMs' ? 300000 : 10000000, `operationBounds.${op}.${k}`);
    }
    if (bounds.deadlineMs !== null && bounds.deadlineMs > config.runner.drainMs) throw Error(`deadline exceeds drain: ${op}`);
  }
  keys(config.environment, ['target', 'generator', 'observer', 'runtimeRevision', 'fixtureDigest', 'runtimeTenants', 'controlNamespace'], 'environment');
  for (const k of ['target', 'generator', 'observer', 'runtimeRevision', 'fixtureDigest']) {
    const value = config.environment[k];
    if (value === null) missing.push(`environment.${k}`);
    else if (typeof value !== 'string' || value.length > 256 || !value.length) throw Error(`invalid environment.${k}`);
  }
  const domains = ['target', 'generator', 'observer'].map(k => config.environment[k]).filter(x => x !== null);
  if (new Set(domains).size !== domains.length) throw Error('distinct target/generator/observer domains required');
  if (config.environment.runtimeRevision !== null && !/^[a-f0-9]{40}$/.test(config.environment.runtimeRevision)) throw Error('exact runtime revision required');
  if (config.environment.fixtureDigest !== null && !/^sha256:[a-f0-9]{64}$/.test(config.environment.fixtureDigest)) throw Error('fixture digest required');
  const tenants = config.environment.runtimeTenants;
  if (!Array.isArray(tenants) || tenants.length !== 2 || !tenants.every(id) || tenants[0] === tenants[1]
    || !id(config.environment.controlNamespace) || tenants.includes(config.environment.controlNamespace)) throw Error('two runtime tenants and separate control namespace required');
  keys(config.perOperation, LOAD_OPERATIONS, 'perOperation');
  const thresholds = structuredClone(config.perOperation);
  for (const [op, policy] of Object.entries(thresholds)) for (const [k, t] of Object.entries(policy)) {
    if (k !== 'minimumSamples' && t?.limit === null) { missing.push(`perOperation.${op}.${k}.limit`); t.limit = k === 'throughputPerSecond' ? 1 : 0; }
  }
  // Reuse normative local evaluator validation even for incomplete templates.
  const dummy = LOAD_PHASES.map(name => ({ name, durationMs: 1, windowMs: 1, drainMs: 0, schedule: [], thresholds }));
  evaluateLoadRequests(dummy, {});
  keys(config.telemetry, ['maxGapMs', 'maxAgeMs', 'thresholds'], 'telemetry');
  const telemetry = structuredClone(config.telemetry);
  for (const [metric, t] of Object.entries(telemetry.thresholds)) if (t?.limit === null) {
    missing.push(`telemetry.thresholds.${metric}.limit`); t.limit = LOAD_TELEMETRY[metric]?.[1] === 'lower' ? 1 : 0;
  }
  if (!Array.isArray(config.phases) || !config.phases.length || config.phases.length > 20) throw Error('bounded phases required');
  if (new Set(config.phases.map(p => p?.name)).size !== config.phases.length) throw Error('duplicate phase');
  if (config.stage === 'qualification' && (config.phases.length !== 5 || config.phases.some((p, i) => p.name !== LOAD_PHASES[i]))) throw Error('qualification requires all five ordered phases');
  let logical = 0, windows = 0, duration = 0, http = 0, provider = 0, evidenceBytes = 0;
  const phases = config.phases.map(p => {
    keys(p, ['name', 'ratePerSecond', 'durationMs', 'windowMs'], 'phase');
    if (!id(p.name)) throw Error('invalid phase name');
    integer(p.durationMs, 1, 43200000, 'phase.durationMs'); integer(p.windowMs, 1, p.durationMs, 'phase.windowMs');
    if (p.durationMs % p.windowMs) throw Error('window must divide duration');
    // Millirate fixed point avoids accumulating floating point arrival drift.
    const rateMilli = Math.round(p.ratePerSecond * 1000);
    if (typeof p.ratePerSecond !== 'number' || !Number.isFinite(p.ratePerSecond) || p.ratePerSecond !== rateMilli / 1000 || rateMilli < 1 || rateMilli > 10000000) throw Error('rate must be positive with at most three decimal places');
    const count = Math.ceil(p.durationMs * rateMilli / 1000000);
    logical += count; windows += p.durationMs / p.windowMs; duration += p.durationMs + config.runner.drainMs;
    if (logical > config.budgets.maxLogicalOperations || windows > 1000) throw Error('aggregate logical operation/window budget exceeded');
    evaluateLoadTelemetry({ durationMs: p.durationMs, ...telemetry }, []);
    if (Math.ceil(p.durationMs / telemetry.maxGapMs) + 1 > 10000) throw Error('telemetry frame budget infeasible');
    const balances = LOAD_OPERATIONS.map(() => 0), perWindow = Array.from({ length: p.durationMs / p.windowMs }, () => Object.fromEntries(LOAD_OPERATIONS.map(op => [op, 0])));
    const schedule = Array.from({ length: count }, (_, i) => {
      let best = config.seed % LOAD_OPERATIONS.length;
      for (let j = 0; j < balances.length; j++) balances[j] += config.mix[LOAD_OPERATIONS[j]];
      for (let j = 0; j < balances.length; j++) if (balances[j] > balances[best]) best = j;
      balances[best] -= weight;
      const operation = LOAD_OPERATIONS[best], scheduledMs = i * 1000000 / rateMilli;
      perWindow[Math.floor(scheduledMs / p.windowMs)][operation]++;
      const bound = config.operationBounds[operation];
      http += bound.httpRequests ?? 0; provider += bound.providerAttempts ?? 0; evidenceBytes += bound.evidenceBytes ?? 0;
      return { id: `${p.name}-${i}`, operation, scheduledMs };
    });
    for (const [window, counts] of perWindow.entries()) for (const op of LOAD_OPERATIONS) if (counts[op] < thresholds[op].minimumSamples) {
      const message = `${p.name}.window-${window}.${op}: scheduled ${counts[op]} < sample floor ${thresholds[op].minimumSamples}`;
      if (config.stage === 'qualification') throw Error(`infeasible sample floor: ${message}`);
      warnings.push(message);
    }
    return { ...p, ...config.runner, schedule, thresholds: structuredClone(config.perOperation), perWindow };
  });
  duration += config.cleanup.timeoutMs * 4 + config.cleanup.settleMs;
  const planBytes = Buffer.byteLength(canonicalJson({ config, phases }));
  evidenceBytes += planBytes;
  const totalEvidenceFiles = 3 + config.repetitions * (2 + 3 * phases.length + 2 * logical + http);
  if (totalEvidenceFiles > config.budgets.maxEvidenceFiles) throw Error('evidence file budget exceeded');
  if (http * config.repetitions > config.budgets.maxHttpRequests || provider * config.repetitions > config.budgets.maxProviderAttempts || evidenceBytes * config.repetitions > config.budgets.maxEvidenceBytes
    || duration * config.repetitions > config.budgets.maxRunMs) throw Error('expanded request/provider/evidence/time budget exceeded');
  const payload = { version: 1, profile, config, phases,
    feasibility: { perTrial: { logicalOperations: logical, httpRequests: http, providerAttempts: provider, evidenceBytes, durationMs: duration, windows },
      totalLogicalOperations: logical * config.repetitions, totalHttpRequests: http * config.repetitions,
      totalProviderAttempts: provider * config.repetitions, totalEvidenceFiles, totalEvidenceBytes: evidenceBytes * config.repetitions, totalDurationMs: duration * config.repetitions },
    missing, warnings, ready: missing.length === 0, admitted: false, executionAuthorized: false };
  return { ...payload, digest: jsonDigest(payload) };
}
