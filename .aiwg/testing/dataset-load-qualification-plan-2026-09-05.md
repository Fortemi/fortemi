# DQ-LOAD execution and measurement plan

Status: proposed; not an execution approval or a qualification receipt.
Owner: [Fortemi #1141](https://git.integrolabs.net/Fortemi/fortemi/issues/1141).
Authority owner: [#1136](https://git.integrolabs.net/Fortemi/fortemi/issues/1136).

## Scope and readiness

Certify only the exact signed topology, workload, revision tuple and resource
limits exercised. Keep the static AIWG index, Knowledge Shard transfer and live
persistence separate. A shard cell must identify core-v1, record-v1 or full-v1;
its load observation cannot replace the corresponding conformance matrix.
Suite NO-GO remains in force.

Before scheduling any load, retain accepted dependency receipts for tenant
isolation (#1137) and fault recovery (#1138), the runner-capacity disposition
(#1072), and the hosted inference resilience disposition (#1099). All four
trackers were open when this plan was written. Their issue status alone cannot
replace inspection of their exact revision-bound evidence. Do not silently
remove hosted or provider cases to bypass these dependencies.

Use a disposable, dedicated environment with synthetic content, separate
storage, a UUID run namespace, and an independent control namespace. Do not use
the live host or a shared CI runner as a stress target. The signed inventory
must name CPU allocation, RAM, disk size and free-space floor, filesystem,
PostgreSQL/WAL configuration, blob storage, queues, connection pool, inference
provider or emulator, network limits and telemetry collectors. Provider emulation
is explicitly a representativeness limitation; it cannot qualify real-provider
quota, price or failure behavior.

Pin immutable producer, consumer, fixture, verifier and schema revisions.
Coordinate the exact consumer tuple with
[AIWG #2242](https://git.integrolabs.net/roctinam/aiwg/issues/2242), and with
[React #412](https://git.integrolabs.net/Fortemi/fortemi-react/issues/412) and
[HotM #231](https://git.integrolabs.net/Fortemi/HotM/issues/231) when exercised.
Obtain reciprocal links before execution. Existing runtime evidence is not
DQ-LOAD qualification.

## Workload proposal for operator review

Use a deterministic seed and a fixed 100-operation scheduling cycle:

| Operation | Operations per cycle | Required state assertion |
|---|---:|---|
| Ingest | 20 | Expected durable effects and receipt identity |
| Query | 25 | Correct tenant-scoped result set |
| Lineage | 10 | Exact expected edges and no cross-tenant references |
| Materialization | 10 | Expected derived state and freshness |
| Export | 5 | Declared profile, counts and artifact-byte integrity |
| Import | 5 | Validated input, exact effects in a clean destination |
| Status | 10 | Attempt/checkpoint identity and terminal classification |
| Cancellation | 5 | Declared cancellation outcome, bounded outstanding work |
| Retry | 10 | Exact retry without duplicate effects; changed retry rejected |

This mix is a proposed scheduling choice, not an assertion that it represents
production. Record the operator's rationale, observed production distribution
if available, and deviations. Generate tenant, payload size, record count,
blob size and query-selectivity distributions before signing the fixture set.
Bounded maximum payloads and resource demand must be included, not only averages.
Do not substitute status polling for durable work to reach a throughput target.

Each phase must exercise the full mix. Keep separate results for each operation,
phase, tenant and size class in retained evidence, without placing tenant or
content identifiers in metric labels.

| Phase | Schedule | Exit requirement |
|---|---|---|
| Smoke | Smallest approved rate and concurrency | Every operation and collector works; no missing evidence |
| Average | Approved representative arrival rate and concurrency | All window and phase thresholds pass |
| Stress | Approved steps up to the signed maximum | Every step passes; never search beyond the maximum |
| Spike | Approved baseline-to-peak transition and hold | Peak bounds and recovery-to-baseline thresholds pass |
| Soak | Approved steady rate and fixed duration | No sustained growth, freshness or correctness failure |

The operator must supply numeric rate, concurrency, duration, ramp, sample
interval, window size and recovery deadline for each phase. Warmup is declared
in advance, bounded, recorded and subject to all safety ceilings. No adaptive
extension, repeat-until-pass, dropped windows or post-run threshold adjustment.
A rejected unsupported tuple is reported as UNSUPPORTED with its rejection
verdict; it cannot count as a successful workload operation.

## Measurement specification: DQ-LOAD-AC-001 and 002

Every row below needs an explicit operator, numeric limit and unit approved
before execution. No numeric capacity or recovery target is inferred from a
unit test or selected after observing the run. Retain raw measurements alongside
all derived summaries. Missing, stale, nonfinite, truncated or discontinuous
observations prevent PASS.

| Measurement | Definition and required source |
|---|---|
| latencyP50/P95/P99 | Client monotonic time from scheduled arrival to terminal response; include queueing; per operation and phase, plus each signed window; record timeout latency and outcome |
| throughput | Correctly completed logical operations per wall-clock second; retries/status calls cannot inflate durable-operation throughput; retain offered and achieved rates |
| errorRate | Unexpected failed logical operations divided by attempted logical operations; separately retain expected rejection, timeout, cancellation and ambiguous counts |
| rssBytes | Maximum process/cgroup RSS including declared workers, with explicit accounting scope |
| cpuPercent | CPU usage against signed allocated cores; declare normalization and maximum/window aggregation |
| storageGrowthBytes | Database/filesystem growth from baseline, maximum over the phase; do not hide a peak with later compaction |
| walGrowthBytes | Retained WAL bytes and generated WAL delta; declare both limits and collection method |
| blobGrowthBytes | Logical and physical object bytes, including staging and failed-attempt objects |
| queueDepth | Maximum outstanding items; retain oldest-item age separately |
| poolUtilizationPercent | Used/configured connections; also retain acquisition timeout and wait counts |
| lockWaitSeconds | Maximum lock-wait duration and timeout/deadlock counts |
| indexFreshnessSeconds | Time from durable commit to independently observable expected derived state |
| providerCalls | Attempted and completed upstream calls, including retries and abandoned streams |
| providerCost | Cumulative cost from pinned model/pricing/account policy; missing billing dimensions fail coverage |
| recovery | Time to approved post-spike baseline and terminal-state convergence, with zero unclassified outcomes |
| correctness | Zero unauthorized reads/mutations, duplicate effects, canonical mismatches and limit-plus-one mutations |
| redaction and cleanup | Zero sensitive-data findings and zero out-of-namespace cleanup effects; independent retained verification |

Latency windows with no completions cannot report zero latency or pass by
omission. Preserve the offered-load schedule to expose coordinated omission.
Do not average percentiles. Recompute summaries from retained observations,
record the quantile method and sample count, and apply every approved comparison
to every required window. A single breached window fails the cell even when the
whole-run average passes. An empty traffic class or missing collector is MISSING.

## Limit-plus-one matrix

Inventory every enforced integer request/resource limit in the pinned producer
and exercised consumers. For each, record its source, exact accepted maximum,
unit, rejection layer, stable diagnostic and whether it is an admission limit
or an operational abort ceiling. Test boundary and boundary-plus-one inputs
with otherwise identical valid fixtures. Include record count, payload bytes,
blob bytes, batch size, concurrency/queue admission and provider quota wherever
they are actually enforced. A proposed limit with no enforcement is a gap,
not an assumed rejection case.

The independent observer must establish before/after canonical state, durable
journal/job/blob/checkpoint effects and provider side effects. Zero changed rows
alone is insufficient. Every rejection must match its approved diagnostic and
show zero mutation. Distinguish isolated fixture setup/cleanup from the request
under test. Do not try to validate disk exhaustion or a monetary ceiling by
consuming the resource past its safety boundary; exercise the admission gate
using bounded fixture controls in the approved isolated environment.

## Abort and teardown: DQ-LOAD-AC-003

Use an independent watchdog and a maximum run deadline. Before start, approve
numeric ceilings for free bytes/inodes, WAL/blob growth, RSS/CPU, queue/pool/lock
pressure, provider calls/cost, telemetry age and total elapsed time. Reserve
capacity for teardown; reject start if that reserve is unavailable. Check both
absolute ceilings and growth rates needed to avoid exhausting the reserve
between observations. A watchdog or collector failure stops new arrivals.

On any ceiling breach: stop arrivals, cancel or classify in-flight operations,
retain the failed measurement and abort reason, enforce the approved drain
deadline, and invoke only the namespace-scoped teardown procedure. Record the
elapsed abort and teardown durations against their separate approved limits.
Verify control-namespace state is unchanged and the run namespace has no active
jobs, blobs, temporary objects or provider work. Incomplete cleanup is a failed
qualification with retained residual inventory, not permission to delete broadly.

## Artifact and implementation gaps

Use the candidate detached authority/receipt design described in
[the qualification ADR](../../docs/architecture/adr/ADR-dataset-qualification-admission.md).
Bind this plan's finalized workload, topology, metric definitions, phase
schedule, limits and diagnostics into immutable approved fixture artifacts.
Retain per-cell raw observations, measurement summaries, representativeness,
redaction, abort and cleanup evidence, signed by the independent verifier.

The current candidate authority enumerates many aggregate load metrics, but
its admission tool does not establish full phase/window coverage or inspect
raw load observations. Provider-call ceilings exist in the environment budget;
this does not itself prove measured provider-call coverage. Separate abort,
teardown and recovery bounds must also be explicitly represented and checked.
Do not relabel successful signature/file verification as successful load testing.
Any schema revision needed for these fields must follow the authority and
consumer versioning policy rather than silently editing the existing snapshot.

The approved research direction now has a configurable plan compiler, integrated
bounded experiment lifecycle, attempt accounting, deterministic inventory oracle
and explicit k6 arrival reference. See the
[configuration guide](../../scripts/qualification/README.md) and
[accepted design](../../docs/architecture/adr/ADR-configurable-load-experiments.md).
Selected parameters are editable profiles and ordered overrides; every effective
value and schedule is digest-bound before execution. These implementations do not
supply the missing live adapter evidence.

Remaining integration: actual SQLx/provider measurement coverage, independent live
state/telemetry adapters, dedicated resource allocation and watchdog deployment,
complete limit fixtures, operational cleanup verification, prerequisite receipts
and the final approved envelope. Any incompatible signed-envelope change still
requires a new contract version and all declared consumer gates.

Review sequence: complete source-backed limit/collector inventory, prepare the
executable driver and verifier with synthetic regression fixtures, finalize the
actual environment and proposed numeric values from its constraints, then
present the complete immutable authority for operator approval. Execution and
issue closure remain gated on dependencies, approval and actual passing receipts.

## Approved configurable implementation validation

The operator approved the research recommendations and requested easy parameter
reconfiguration. The implementation provides strict defaults/profile/override
precedence, explicit null selections, deterministic allocation, per-window sample
feasibility, complete repeated-run budgets, fresh configuration digests and bounded
runtime evidence checks. Candidate 2.0.0 remains unchanged.

355 qualification/readiness tests passed with no skips. A real loopback-only k6
v1.7.1 reference first exposed an endpoint mismatch (Node four offers, k6 five).
The explicitly bounded reference now reconciles five raw k6 starts, one boundary
rejection before HTTP and four admitted/completed requests, matching four Node
requests and eight independent loopback receipts. Both experiments and the binary
provenance are retained in
[load-arrival-reference-2026-09-05](load-arrival-reference-2026-09-05/).
This establishes local admitted-arrival accounting, not identical raw engine
scheduling, target capacity or qualification. No Fortemi load was executed.
