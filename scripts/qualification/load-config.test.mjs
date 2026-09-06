import test from 'node:test';
import assert from 'node:assert/strict';
import { loadConfigTemplate, resolveLoadConfig, compileLoadConfig } from './load-config.mjs';
import { LOAD_OPERATIONS } from './load-request-summary.mjs';

function selected() {
  const document = loadConfigTemplate(), d = document.defaults;
  document.profiles.qualification.phases.forEach(p => { p.ratePerSecond = 1; });
  Object.assign(d.environment, { target: 'target', generator: 'generator', observer: 'observer',
    runtimeRevision: 'a'.repeat(40), fixtureDigest: `sha256:${'b'.repeat(64)}` });
  for (const bound of Object.values(d.operationBounds)) Object.assign(bound,
    { httpRequests: 2, providerAttempts: 1, evidenceBytes: 100, deadlineMs: 1000 });
  d.provider = { maxCostUsd: '0', priceRevision: 'synthetic-no-provider' };
  d.requestPolicy.minimumSamples = 1;
  for (const [metric, value] of Object.entries(d.requestPolicy)) if (metric !== 'minimumSamples') value.limit = metric === 'throughputPerSecond' ? 0.001 : 1;
  for (const value of Object.values(d.telemetry.thresholds)) value.limit = 1;
  return document;
}

test('incomplete template compiles honestly without mutating or inventing selections', () => {
  const document = loadConfigTemplate(), before = structuredClone(document);
  const result = compileLoadConfig(document, 'calibration');
  assert.equal(result.ready, false);
  assert.equal(result.executionAuthorized, false);
  assert.equal(result.admitted, false);
  assert.ok(result.missing.includes('environment.target'));
  assert.ok(result.missing.includes('perOperation.ingest.latencyP99.limit'));
  assert.equal(result.config.perOperation.ingest.latencyP99.limit, null);
  assert.ok(result.warnings.length);
  assert.deepEqual(document, before);
});

test('nested omissions and unknown sparse policy keys never acquire template defaults', () => {
  for (const mutate of [d => delete d.defaults.runner.pollMs,
    d => delete d.defaults.requestPolicy.latencyP99.unit,
    d => { d.defaults.perOperation.ingest.latencyP999 = {}; },
    d => { d.profiles.unused = { runner: { typo: 1 } }; }]) {
    const d = loadConfigTemplate(); mutate(d);
    assert.throws(() => resolveLoadConfig(d, 'calibration'));
  }
});

test('sparse defaults and profile patches preserve independent common and specific values', () => {
  const d = selected();
  d.defaults.perOperation.ingest = { latencyP99: { limit: 20 } };
  d.profiles.calibration = { requestPolicy: { latencyP95: { limit: 10 } },
    perOperation: { ingest: { latencyP50: { limit: 3 } } } };
  const c = resolveLoadConfig(d, 'calibration');
  assert.equal(c.perOperation.ingest.latencyP99.limit, 20);
  assert.equal(c.perOperation.ingest.latencyP50.limit, 3);
  assert.equal(c.perOperation.ingest.latencyP95.limit, 10);
  assert.equal(c.perOperation.query.latencyP99.limit, 1);
  const overridden = resolveLoadConfig(d, 'calibration', [
    'requestPolicy.latencyP99.limit=30', 'perOperation.ingest.latencyP99.limit=40']);
  assert.equal(overridden.perOperation.ingest.latencyP99.limit, 40);
  assert.equal(overridden.perOperation.query.latencyP99.limit, 30);
});

test('digest is deterministic and binds changed selected parameters and sparse overrides', () => {
  const d = selected(), a = compileLoadConfig(d, 'calibration');
  assert.equal(a.digest, compileLoadConfig(d, 'calibration').digest);
  assert.notEqual(a.digest, compileLoadConfig(d, 'calibration', ['runner.maxConcurrency=2']).digest);
  assert.notEqual(a.digest, compileLoadConfig(d, 'calibration', ['perOperation.ingest.latencyP99.limit=2']).digest);
  assert.equal(a.ready, true);
});

test('override typos, malformed replacements, duplicate paths and prototype keys reject', () => {
  for (const overrides of [['runner.maxConcurency=2'], ['runner={"typo":2}'],
    ['runner=null'], ['requestPolicy.latencyP99.unit="seconds"'],
    ['runner.pollMs=1', 'runner.pollMs=2'], ['__proto__.polluted=true']])
    assert.throws(() => compileLoadConfig(selected(), 'calibration', overrides));
});

test('fixed point arrivals retain fractional rates without accumulated drift', () => {
  const d = selected();
  d.defaults.phases = [{ name: 'fractional', ratePerSecond: 0.333, durationMs: 120000, windowMs: 60000 }];
  const p = compileLoadConfig(d, 'calibration').phases[0];
  assert.equal(p.schedule.length, 40);
  assert.equal(p.schedule[39].scheduledMs, 39 * 1000000 / 333);
  assert.equal(p.perWindow.reduce((n, window) => n + Object.values(window).reduce((a,b) => a+b,0),0),40);
  d.defaults.phases[0].ratePerSecond = 0.3333;
  assert.throws(() => compileLoadConfig(d, 'calibration'), /three decimal/);
});

test('qualification proves every operation/window and retains all repetitions', () => {
  const plan = compileLoadConfig(selected(), 'qualification');
  assert.equal(plan.ready, true);
  assert.equal(plan.phases.length, 5);
  assert.equal(plan.feasibility.totalLogicalOperations, plan.feasibility.perTrial.logicalOperations * 3);
  for (const p of plan.phases) for (const window of p.perWindow)
    for (const op of LOAD_OPERATIONS) assert.ok(window[op] >= p.thresholds[op].minimumSamples);
  const d = selected(); d.defaults.requestPolicy.minimumSamples = 1000;
  assert.throws(() => compileLoadConfig(d, 'qualification'), /infeasible sample floor/);
});

test('aggregate evaluator cap applies across phases, without silently reducing rates or floors', () => {
  const d = selected();
  d.profiles.qualification.phases.forEach(p => { p.ratePerSecond = 200; });
  assert.throws(() => compileLoadConfig(d, 'qualification'), /aggregate logical/);
  assert.equal(d.profiles.qualification.phases[0].ratePerSecond, 200);
});

test('expanded budgets apply to complete repeated run and reject impossible telemetry coverage', () => {
  const d = selected(); d.defaults.budgets.maxHttpRequests = 3000;
  assert.throws(() => compileLoadConfig(d, 'qualification'), /expanded/);
  const long = selected(); long.defaults.phases[0].durationMs = 12000000;
  long.defaults.phases[0].windowMs = 12000000;
  assert.throws(() => compileLoadConfig(long, 'calibration'), /telemetry frame/);
});

test('required operations, phases, metric directions and disjoint domains cannot be diluted', () => {
  for (const mutate of [d => { d.defaults.mix.ingest = 0; },
    d => { d.profiles.qualification.phases.pop(); },
    d => { d.defaults.telemetry.thresholds.freeBytes.operator = 'lte'; },
    d => { d.defaults.environment.observer = d.defaults.environment.generator; },
    d => { d.defaults.perOperation.ingest.minimumSamples = 0; }]) {
    const d = selected(); mutate(d);
    assert.throws(() => compileLoadConfig(d, 'qualification'));
  }
});

test('expanded effective configuration recompiles identically for runner binding', () => {
  const d = selected();
  d.defaults.perOperation.ingest = { latencyP99: { limit: 3 } };
  const a = compileLoadConfig(d, 'calibration', ['runner.maxConcurrency=2']);
  const b = compileLoadConfig({ version: 1, defaults: a.config, profiles: { calibration: {} } }, 'calibration');
  assert.equal(b.digest, a.digest);
});

test('provider cap and price revision require explicit exact decimal selections', () => {
  const incomplete = compileLoadConfig(loadConfigTemplate(), 'calibration');
  assert.ok(incomplete.missing.includes('provider.maxCostUsd'));
  assert.ok(incomplete.missing.includes('provider.priceRevision'));
  for (const value of [0, -1, '-1', '1e-3', '01', '0.0000000000000000001', 'NaN']) {
    const d = selected(); d.defaults.provider.maxCostUsd = value;
    assert.throws(() => compileLoadConfig(d, 'calibration'));
  }
  for (const revision of ['', ' ', 'x'.repeat(257), 1]) {
    const d = selected(); d.defaults.provider.priceRevision = revision;
    assert.throws(() => compileLoadConfig(d, 'calibration'));
  }
  const d = selected(); d.defaults.provider.maxCostUsd = '0.000000000000000001';
  const a = compileLoadConfig(d, 'calibration');
  assert.equal(a.config.provider.maxCostUsd, '0.000000000000000001');
  assert.notEqual(a.digest, compileLoadConfig(d, 'calibration', ['provider.priceRevision="revision-2"']).digest);
});

test('duration reserves baseline, after, cleanup and settled observation callbacks', () => {
  const d = selected(), plan = compileLoadConfig(d, 'calibration');
  const expected = d.defaults.phases.reduce((sum, p) => sum + p.durationMs + d.defaults.runner.drainMs, 0)
    + 4 * d.defaults.cleanup.timeoutMs + d.defaults.cleanup.settleMs;
  assert.equal(plan.feasibility.perTrial.durationMs, expected);
});

test('publication file count is preflighted across all phases and repetitions', () => {
  const d = selected(), plan = compileLoadConfig(d, 'qualification');
  const perTrial = plan.feasibility.perTrial;
  const expected = 3 + 3 * (2 + 3 * 5 + 2 * perTrial.logicalOperations + perTrial.httpRequests);
  assert.equal(plan.feasibility.totalEvidenceFiles, expected);
  d.defaults.budgets.maxEvidenceFiles = expected - 1;
  assert.throws(() => compileLoadConfig(d, 'qualification'), /evidence file budget/);
  d.defaults.budgets.maxEvidenceFiles = expected;
  assert.equal(compileLoadConfig(d, 'qualification').ready, true);
});
