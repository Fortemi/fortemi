import { performance } from 'node:perf_hooks';
import { compileLoadConfig } from './load-config.mjs';
import { canonicalJson, jsonDigest } from './canonical-json.mjs';
import { createLoadWorkload } from './load-workload.mjs';
import { createLoadAttemptBudget, compareLoadUsd } from './load-attempt-budget.mjs';
import { runLoadSchedule } from './run-load-schedule.mjs';
import { evaluateLoadRequests } from './evaluate-load-requests.mjs';
import { summarizeLoadRequests } from './load-request-summary.mjs';
import { evaluateLoadTelemetry } from './evaluate-load-telemetry.mjs';
import { compareLoadStateInventory, evaluateLoadState } from './load-state-oracle.mjs';

/** Caller-selected adapters run in an externally supervised, pinned environment.
 * This orchestration never authenticates signatures or admits qualification.
 * Bind {fixtures,statePlan} using environment.fixtureDigest. Configuration and
 * observations are emitted through a bounded record callback; its returned receipt
 * must identify the exact content-addressed bytes. No dynamic adapter loading.
 */
export async function runLoadExperiment(compiled, adapters) {
  const selected = structuredClone(compiled);
  const { digest, ...payload } = selected;
  if (jsonDigest(payload) !== digest) throw Error('compiled plan digest mismatch');
  const rebuilt = compileLoadConfig({ version: 1, defaults: selected.config, profiles: { [selected.profile]: {} } }, selected.profile);
  if (rebuilt.digest !== digest || !selected.ready) throw Error('complete validated configuration required');
  const config = selected.config;
  for (const name of ['authorize', 'apiRequest', 'verifyOutcome', 'checkSafety', 'observeState', 'cleanup', 'readTelemetry', 'readProviderUsage', 'record']) {
    if (typeof adapters?.[name] !== 'function') throw Error(`explicit adapter required: ${name}`);
  }
  const fixtures = structuredClone(adapters.fixtures), statePlan = structuredClone(adapters.statePlan);
  if (jsonDigest({ fixtures, statePlan }) !== config.environment.fixtureDigest) throw Error('fixture/state plan digest mismatch');
  const expected = selected.phases.flatMap(p => p.schedule);
  if (!Array.isArray(fixtures) || fixtures.length !== expected.length) throw Error('exact fixture inventory required');
  const byId = new Map(fixtures.map(f => [f.id, f]));
  if (byId.size !== fixtures.length || expected.some(r => byId.get(r.id)?.operation !== r.operation)) throw Error('schedule/fixture binding mismatch');
  if (jsonDigest([...statePlan.namespaces].sort()) !== jsonDigest([...config.environment.runtimeTenants, config.environment.controlNamespace].sort())
    || jsonDigest(statePlan.controlNamespaces) !== jsonDigest([config.environment.controlNamespace])
    || statePlan.minimumSettleMs !== config.cleanup.settleMs) throw Error('state scope/settling configuration mismatch');
  evaluateLoadState({ plan: statePlan }); // Validate all expectations before any callback.
  createLoadWorkload({ fixtures, apiRequest: adapters.apiRequest, runtimeVersion: config.environment.runtimeRevision,
    verifyOutcome: adapters.verifyOutcome, recordObservation: () => {} }); // Validate routes and payloads before authorization.

  const origin = performance.now(), now = () => Math.floor(performance.now() - origin);
  const unresolved = new Set(), trials = [], artifacts = [], failures = [];
  let attemptedBytes = 0, attemptedFiles = 0, serial = 0, evidenceFailed = false, runtimeBudgetBreached = false;
  let providerUsage = null;
  const httpBudget = createLoadAttemptBudget({ maxLogicalOperations: config.budgets.maxLogicalOperations * config.repetitions,
    maxRequests: config.budgets.maxHttpRequests, maxAttempts: config.budgets.maxHttpRequests });
  async function bounded(label, timeoutMs, fn, parentSignal) {
    const remaining = config.budgets.maxRunMs - now();
    if (remaining <= 0) throw Error('run-deadline');
    timeoutMs = Math.min(timeoutMs, remaining);
    const controller = new AbortController();
    const relay = () => controller.abort(parentSignal.reason);
    if (parentSignal?.aborted) relay();
    parentSignal?.addEventListener('abort', relay, { once: true });
    const key = `${++serial}:${label}`; unresolved.add(key);
    let timer;
    const work = Promise.resolve().then(() => fn(controller.signal)).finally(() => unresolved.delete(key));
    try {
      return await Promise.race([work, new Promise((_, reject) => {
        timer = setTimeout(() => { controller.abort(); reject(Error(`${label}-timeout`)); }, timeoutMs);
      })]);
    } finally { clearTimeout(timer); parentSignal?.removeEventListener('abort', relay); }
  }
  async function record(kind, value) {
    try {
      const bytes = Buffer.from(canonicalJson({ kind, value }));
      attemptedBytes += bytes.length; attemptedFiles++;
      if (attemptedBytes > config.budgets.maxEvidenceBytes || attemptedFiles > config.budgets.maxEvidenceFiles) throw Error('evidence-budget-exceeded');
      const artifact = await bounded('record', config.runner.safetyTimeoutMs, signal => adapters.record(bytes, { signal }));
      if (artifact?.digest !== jsonDigest({ kind, value }) || artifact.bytes !== bytes.length) throw Error('evidence-receipt-mismatch');
      artifacts.push({ kind, ...artifact });
    } catch (error) { evidenceFailed = true; throw error; }
  }
  async function providerSafety(signal) {
    const usage = await adapters.readProviderUsage({ signal, timeMs: now() });
    if (!usage || usage.complete !== true
      || !Number.isSafeInteger(usage.observedMs) || usage.observedMs < 0 || usage.observedMs > now()
      || now() - usage.observedMs > config.telemetry.maxAgeMs
      || typeof usage.sourceId !== 'string' || !usage.sourceId.length || usage.sourceId.length > 256
      || typeof usage.resetId !== 'string' || !usage.resetId.length || usage.resetId.length > 256
      || (providerUsage && (usage.sourceId !== providerUsage.sourceId || usage.resetId !== providerUsage.resetId)) || !Number.isSafeInteger(usage.attempts) || usage.attempts < 0
      || usage.priceRevision !== config.provider.priceRevision
      || usage.attempts > config.budgets.maxProviderAttempts
      || compareLoadUsd(usage.costUsd, config.provider.maxCostUsd) > 0
      || (providerUsage && (usage.attempts < providerUsage.attempts || compareLoadUsd(usage.costUsd, providerUsage.costUsd) < 0))) return false;
    providerUsage = structuredClone(usage);
    return true;
  }
  const authorize = await bounded('authorization', config.runner.safetyTimeoutMs,
    signal => adapters.authorize({ digest, stage: config.stage, environment: structuredClone(config.environment), signal }), adapters.signal);
  if (authorize !== true || adapters.signal?.aborted) throw Error('run authorization required for exact compiled digest');
  await record('compiled-plan', selected);
  await record('fixture-state-plan', { fixtures, statePlan });
  for (let trialIndex = 0; trialIndex < config.repetitions; trialIndex++) {
    const trial = { trialIndex, phases: [], baseline: null, state: null, cleanupAttempted: false, abortReason: null };
    trials.push(trial);
    let after, clean, settled, workloadStarted = false;
    const phaseEvidence = {}, phaseContext = { name: null };
    const counts = new Map();
    const execute = createLoadWorkload({ fixtures, runtimeVersion: config.environment.runtimeRevision,
      apiRequest: async (method, path, body, options = {}) => {
        const operation = byId.get(options.logicalOperationId)?.operation;
        if (!operation) throw Error('unbound HTTP operation');
        const count = (counts.get(options.logicalOperationId) ?? 0) + 1;
        counts.set(options.logicalOperationId, count);
        if (count > config.operationBounds[operation].httpRequests) { runtimeBudgetBreached = true; throw Error('operation HTTP budget exceeded'); }
        let identity;
        try { identity = httpBudget.reserve({ logicalOperationId: `${trialIndex}:${options.logicalOperationId}`,
          requestId: options.requestId, attemptId: options.attemptId }); }
        catch (error) { runtimeBudgetBreached = true; throw error; }
        await record('http-dispatch', { trialIndex, phase: phaseContext.name, ...identity, budgetLogicalOperationId: identity.logicalOperationId, logicalOperationId: options.logicalOperationId });
        return adapters.apiRequest(method, path, body, options);
      }, verifyOutcome: adapters.verifyOutcome,
      recordObservation: observation => record('operation', { trialIndex, phase: phaseContext.name, observation }) });
    try {
      const baseline = await bounded('baseline', config.cleanup.timeoutMs, signal => adapters.observeState({ trialIndex, point: 'before', signal }));
      trial.baseline = compareLoadStateInventory(statePlan, statePlan.expectedBaseline, baseline);
      await record('baseline', { trialIndex, observedAtMs: now(), inventory: baseline, check: trial.baseline });
      if (trial.baseline.status !== 'PASS') throw Error('baseline-mismatch');
      for (const phase of selected.phases) {
        phaseContext.name = phase.name;
        if (adapters.signal?.aborted) throw Error('external-abort');
        if (now() + phase.durationMs + phase.drainMs + config.cleanup.timeoutMs * 3 + config.cleanup.settleMs > config.budgets.maxRunMs) throw Error('run-time-reserve-exhausted');
        const result = await runLoadSchedule(phase, {
          signal: adapters.signal,
          checkSafety: async signal => !evidenceFailed && !runtimeBudgetBreached && await providerSafety(signal)
            && await adapters.checkSafety({ trialIndex, phase: phase.name, policy: structuredClone(config.telemetry),
              httpUsage: httpBudget.snapshot(), providerUsage: structuredClone(providerUsage), signal }),
          execute: (request, signal) => {
            workloadStarted = true;
            return bounded('operation', config.operationBounds[request.operation].deadlineMs,
              async boundedSignal => {
                const outcome = await execute(request, boundedSignal);
                if (evidenceFailed || runtimeBudgetBreached) throw Error('evidence-or-budget-failed');
                return outcome;
              }, signal);
          },
        });
        phaseEvidence[phase.name] = result.observations;
        trial.phases.push({ name: phase.name, result });
        await record('phase', { trialIndex, name: phase.name, result });
        const frames = await bounded('telemetry', config.runner.safetyTimeoutMs,
          signal => adapters.readTelemetry({ trialIndex, phase: phase.name, signal }));
        const telemetry = evaluateLoadTelemetry({ durationMs: phase.durationMs, ...config.telemetry }, frames);
        trial.phases.at(-1).telemetry = telemetry;
        await record('telemetry', { trialIndex, phase: phase.name, frames, check: telemetry });
        if (result.abortReason || !result.executionSettled) throw Error(result.abortReason ?? 'execution-unsettled');
        if (!telemetry.telemetryChecksPass) throw Error('telemetry-not-passing');
        if (!await bounded('provider', config.runner.safetyTimeoutMs, providerSafety)) throw Error('provider-not-passing');
        await record('provider', { trialIndex, phase: phase.name, usage: providerUsage });
        if (config.stage === 'qualification') {
          const check = evaluateLoadRequests(selected.phases, phaseEvidence).phases.find(p => p.name === phase.name);
          trial.phases.at(-1).requests = check;
          if (check.status !== 'PASS') throw Error('request-not-passing');
        }
      }
    } catch (error) {
      // Keep a bounded local diagnostic; adapter messages may contain private data.
      trial.abortReason = ['baseline-mismatch', 'external-abort', 'run-time-reserve-exhausted', 'telemetry-not-passing', 'provider-not-passing', 'request-not-passing'].includes(error.message)
        ? error.message : 'execution-or-evidence-failure';
    } finally {
      if (workloadStarted) {
        try {
          const inventory = await bounded('after', config.cleanup.timeoutMs, signal => adapters.observeState({ trialIndex, point: 'after', signal }));
          after = { observedAtMs: now(), inventory };
        } catch { failures.push({ trialIndex, reason: 'after-observation-failed' }); }
        // Cleanup uses a fresh signal even when workload cancellation was requested.
        trial.cleanupAttempted = true;
        try {
          await bounded('cleanup', config.cleanup.timeoutMs, async signal => {
            await adapters.cleanup({ trialIndex, namespaces: [...config.environment.runtimeTenants], signal });
            const inventory = await adapters.observeState({ trialIndex, point: 'cleanup', signal });
            clean = { observedAtMs: now(), inventory };
          });
        } catch { failures.push({ trialIndex, reason: 'cleanup-failed-or-unsettled' }); }
        if (clean) {
          await new Promise(resolve => setTimeout(resolve, config.cleanup.settleMs));
          try {
            const inventory = await bounded('settled', config.cleanup.timeoutMs, signal => adapters.observeState({ trialIndex, point: 'settled', signal }));
            settled = { observedAtMs: now(), inventory };
          } catch { failures.push({ trialIndex, reason: 'settled-observation-failed' }); }
        }
        trial.state = evaluateLoadState({ plan: statePlan, after, cleanup: clean, settled });
        try { await record('state', { trialIndex, after: after ?? null, cleanup: clean ?? null, settled: settled ?? null, check: trial.state }); }
        catch { failures.push({ trialIndex, reason: 'state-evidence-failed' }); }
      }
    }
    try {
      if (config.stage === 'qualification') trial.requests = evaluateLoadRequests(selected.phases, phaseEvidence);
      else trial.requests = { scope: 'calibration-or-rehearsal-only', requestChecksPass: false,
        phases: selected.phases.map(p => ({ name: p.name, summary: summarizeLoadRequests({ ...p, observations: phaseEvidence[p.name] ?? [] }) })) };
    } catch { trial.abortReason ||= 'invalid-request-observations'; failures.push({ trialIndex, reason: 'invalid-request-observations' }); }
    if (trial.abortReason || trial.state?.status !== 'PASS' || failures.length || unresolved.size) break;
  }
  const result = { configDigest: digest, stage: config.stage, admitted: false, qualificationPassed: false,
    trials, failures, httpUsage: httpBudget.snapshot(), unresolved: [...unresolved],
    cleanupVerified: false, runtimeBudgetBreached, providerUsage,
    attemptedEvidenceBytes: attemptedBytes, attemptedEvidenceFiles: attemptedFiles, artifacts };
  try { await record('run-result', { ...result, artifacts: [...artifacts] }); }
  catch { result.failures.push({ reason: 'result-evidence-failed' }); }
  result.unresolved = [...unresolved];
  result.attemptedEvidenceBytes = attemptedBytes; result.attemptedEvidenceFiles = attemptedFiles;
  result.cleanupVerified = trials.length > 0 && trials.every(t => t.state?.status === 'PASS' && t.phases.every(p => p.result.executionSettled))
    && !unresolved.size && !failures.length && !evidenceFailed;
  return result;
}
