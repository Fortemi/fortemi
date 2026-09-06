# Configurable load experiments

The approved #1141 research design is implemented as a local configuration
compiler, bounded experiment orchestration, attempt accounting and state checks.
Use [config/load-profiles.json](config/load-profiles.json) to select parameters;
no script edits are needed. These tools do not admit qualification receipts.

## Edit, inspect, bind, run

Copy the example to an operator-owned file, edit its defaults or add named
profiles, then compile an inspectable artifact:

```bash
cp scripts/qualification/config/load-profiles.json /tmp/load-profiles.json
node scripts/qualification/compile-load-plan.mjs \
  --config /tmp/load-profiles.json --profile calibration \
  --set runner.maxConcurrency=2 \
  --set cleanup.settleMs=5000 \
  --output /tmp/load-plan.json
```

Precedence is **defaults → named profile → ordered `--set path=JSON` overrides**.
Common `requestPolicy` overrides apply to every operation; a later `perOperation`
override selects one operation. Arrays, such as `phases`, are replaced as a whole.
There is no environment-variable precedence or dynamic code loading. Unknown
keys, incomplete nested defaults, malformed units, duplicate overrides, zeroed
operation shares and impossible budgets reject. Existing output files are never
overwritten by the CLI. Use a new filename for each configuration revision.

The default calibration profile retains the approved proposal's 0.5/1/2 logical
operations per second, 120-second steps, 60-second drain and 300-second cleanup
callback limits. Site-dependent values are `null`: compilation lists them in
`missing`, retains them in the effective config and returns `ready: false`.
`ready: true` means authoring preflight succeeded, not that a target is qualified.

The qualification profile deliberately does not derive certification from the
short calibration schedule. Select the complete five-phase rates/windows and
sample floors before fresh trials. Its initial 1,000-sample floor is infeasible
with its placeholder schedule, so compilation rejects until a feasible plan is
selected. It never reduces floors or drops windows to make a run fit.

## Selected parameters

| Configuration | Meaning |
|---|---|
| `stage`, `repetitions`, `seed` | Rehearsal/calibration/qualification, trial count, deterministic allocation tie-break |
| `phases` | Ordered names, rates with up to three decimal places, duration and equal window size in milliseconds |
| `mix` | Positive relative weights for all nine operations |
| `runner` | Concurrency, schedule lag, polling, collector timeout and drain |
| `cleanup` | Per-callback cleanup/observer timeout and late-write settling interval |
| `operationBounds` | Maximum HTTP attempts, provider attempts, retained bytes and deadline per logical operation |
| `requestPolicy`, `perOperation` | Sample floors and explicit latency/throughput/error thresholds with operators and units |
| `telemetry` | Complete resource thresholds, maximum sample gap and age |
| `provider` | Exact decimal USD ceiling and explicit pricing revision |
| `budgets` | Logical entries per trial; whole-run HTTP/provider attempts, bytes, files and elapsed time |
| `environment` | Separate target/generator/observer identities, exact runtime revision, fixture digest, two runtime tenants and control namespace |

The effective config, full schedule, actual planned per-operation/window counts,
feasibility totals and SHA-256 digest are emitted in canonical JSON. Repeat trials
reuse the plan but retain distinct trial identities. Any selected-parameter change
changes the digest. Rates use fixed-point millirates rather than accumulated
floating-point increments. Schedule timestamps retain the original offered time.

The compiler enforces the existing 100,000 total logical-entry limit across a
trial's phases, 1,000 windows, telemetry frame limits and expanded whole-run
budgets. Per-operation byte estimates must include raw observations and verdicts;
compiler estimates do not prove complete byte overhead. The known publication
file count is also checked across every repetition before execution. The experiment
runner also charges every actual evidence write and rejects exhausted bytes/files.
Count polling and retries in HTTP bounds; a logical operation is not one HTTP call.

## Runtime integration

`runLoadExperiment(compiled, adapters)` validates and recompiles the selected
configuration before any callback. The fixture binding is
`jsonDigest({ fixtures, statePlan })`. Supply exactly one fixture per scheduled
ID with the matching operation. Fixtures use `createLoadWorkload`'s existing
controller and API interfaces. The state plan declares every namespace × surface,
baseline and expected final inventories, and the selected settling interval.

| Adapter | Required behavior |
|---|---|
| `authorize({digest,stage,environment,signal})` | Accept the exact selected tuple under the operator's existing authorization policy |
| `apiRequest(method,path,body,options)` | Selected bounded transport; options carry logical/request/attempt identities and cancellation signal |
| `verifyOutcome(...)` | Independently observe expected durable effects; retain referenced evidence and distinguish actual retry/cancellation from cached receipts |
| `checkSafety({policy,httpUsage,providerUsage,...})` | Check fresh complete runtime telemetry and target/generator health; false/error stops arrivals |
| `readTelemetry({trialIndex,phase,signal})` | Return complete phase frames using the existing telemetry evaluator's clock, unit and source contract |
| `readProviderUsage({timeMs,signal})` | Cumulative whole-run `{attempts,costUsd,priceRevision,complete,observedMs,sourceId,resetId}`; clock mapped to experiment elapsed milliseconds |
| `observeState({trialIndex,point,signal})` | Independently collect every declared namespace/surface, including empty ones |
| `cleanup({trialIndex,namespaces,signal})` | Remove only run-owned resources in the runtime namespaces; honor the fresh bounded cleanup signal |
| `record(bytes,{signal})` | Persist exact bytes and return their `{digest,bytes,path}` receipt; `createLoadEvidenceWriter` is the local implementation |

Provider snapshots must be complete, fresh, monotonic, bound to the selected
pricing revision and stable source/reset identities. Missing cost is not zero.
Observed cap breaches stop new arrivals. **Dispatch-time provider reservation must
also be integrated at the actual provider boundary**: use
`createLoadAttemptBudget` with exact decimal reservations and reconciliation.
Polling snapshots cannot prevent already dispatched remote work from overshooting.
The helper is process-local; it does not establish a distributed provider quota.

The lifecycle persists plan/fixtures, verifies baseline, executes phases with
periodic safety checks, evaluates telemetry and request windows, observes effects,
performs bounded cleanup and re-observes after settling. A failed qualification
window stops subsequent phases/trials. Baseline mismatch launches no work and does
not delete pre-existing state. Evidence failure, cancellation and telemetry loss
after dispatch still enter cleanup. Hung callbacks remain explicitly unresolved.
Final evidence failure prevents a verified cleanup claim.

Use the existing external `superviseLoadProcess` or equivalent dedicated supervisor
for the whole worker, with the configured time limit and CPU/memory/IO controls.
JavaScript timers cannot stop synchronous stalls or uncooperative remote work.
Do not serialize a large expanded plan into its 1 MiB stdin; supply a bounded,
digest-verified artifact loader in the pinned worker. Pin the complete runtime
revision/imported modules, not only the entrypoint. The observer/cleanup actor must
remain independent after worker death. The library's injected callbacks and local
tests alone do not establish that operational independence.

## Evidence and compatibility

Raw workload records preserve acknowledgement timestamps, logical IDs and each
HTTP attempt; verifier records add independently verified completion timestamps.
The transport records failed and abandoned dispatched attempts as well as complete
responses. Late scheduler completions outside the fixed phase/drain envelope stay
unresolved. They cannot disappear into successful latency samples.

`load-state-oracle.mjs` checks deterministic identities/content digests on every
required surface, unchanged control state, cleanup residuals and late writes.
Missing collections are MISSING; a collected inventory omitting a known resource
is FAIL. Collection scope, physical blob coverage, RLS identity and real provider
instrumentation must still be established by the selected adapters and receipts.

This authoring format is local version 1, separate from detached qualification
candidate 2.0.0. No existing authority/receipt schema, signature, shard profile or
consumer snapshot is changed. Bind the compiled artifact into the approved tuple
before live execution. An incompatible authority change still requires versioning
and all declared consumer gates under #1136 and AIWG #2242.

Approval of the research recommendations is recorded. Remaining live integration
work includes complete SQLx/provider measurement at the source, selected isolated
resources, real observer/cleanup adapters, prerequisite receipts and final
certification measurements. These tools always return `admitted: false` and
`qualificationPassed: false`; final admission belongs to the independent verifier.
Suite NO-GO remains until the executable cross-repository gates pass.

## Validation

```bash
node --test scripts/qualification/*.test.mjs
node scripts/ci/verify-dataset-qualification-graph.mjs
```

Tests include parameter/digest changes, infeasible coverage, repeated-run budgets,
concurrent reservation exhaustion, failed attempts, oracle falsification, telemetry
loss, cancellation, late writes, hung cleanup and final evidence publication failure.
See the [research proposal](../../.aiwg/research/reports/issue-1141-load-qualification-research-proposal-2026-09-05.md)
for source evidence and the remaining runtime qualification matrix.

## Local k6 reference

`check-arrival-reference.mjs` creates its own ephemeral loopback probe. It accepts
`{ "rate": 2, "durationSeconds": 2, "k6Path": "/path/to/k6", "outputDirectory": "/new/output/path" }`
as a JSON CLI input. Rates are bounded to 1–2/second and durations to 1–3 seconds.
It never accepts a Fortemi target URL. An unavailable executable is MISSING.

The k6 adapter retains raw starts and separately rejects IDs outside the Node
half-open schedule before HTTP. Its comparison requires exact admitted IDs/counts,
completion and independent probe receipts, with all boundary rejections reconciled.
The original raw-count mismatch is preserved alongside the bounded comparison in
[the validation record](../../.aiwg/testing/load-arrival-reference-2026-09-05/).
The pinned local v1.7.1 binary was checked against official release checksums.
This is a limited generator accounting check, not a latency or overload benchmark.
