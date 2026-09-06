# DQ-LOAD runtime inventory and request observations

Owner: [#1141](https://git.integrolabs.net/Fortemi/fortemi/issues/1141).
Source inspected: Fortemi `c8ca46ba`.
This is a partial source inventory, not a qualified capacity envelope.

## Dataset execution admission limits

Source: [dataset-execution.js](../../mcp-server/lib/dataset-execution.js),
`DATASET_RESOURCE_POLICY`, `validateResourceEnvelope` and execution admission.

| Limit | Current maximum | Rejection or runtime behavior |
|---|---:|---|
| Records | 500 | Envelope above policy: RESOURCE_LIMIT_EXCEEDED; payload above requested bound: RECORD_LIMIT_EXCEEDED |
| Total canonical record bytes | 16777216 | INPUT_BYTES_EXCEEDED for payload above requested bound |
| Single canonical record bytes | 4194304 | RECORD_BYTES_EXCEEDED for payload above requested bound |
| Declared duration | 120000 ms | Envelope above policy rejected; runtime timer aborts request and requires outcome reconciliation |
| Active dataset runs | 1 per MCP process | CONCURRENCY_LIMIT_EXCEEDED; does not establish cross-process admission |
| Declared traversal depth | 8 | RESOURCE_LIMIT_EXCEEDED above policy; not proof every traversal runtime path enforces depth |
| Declared results | 1000 | RESOURCE_LIMIT_EXCEEDED above policy; not proof every query runtime path enforces result limits |
| Outbound network | false | OUTBOUND_NETWORK_UNSUPPORTED when requested |

Record byte checks use canonical JSON UTF-8 bytes, not source file size. Requested
bounds can be below policy limits. Test both policy-envelope rejection and actual
payload rejection separately. Duration expiry is an operational abort, not a
pre-mutation admission boundary; it cannot satisfy a zero-mutation claim merely
because the caller received a timeout. Process-local concurrency must not be
reported as a global deployment limit. Other workload interfaces still require
source-backed inventories; these values cannot be applied to every API route.

## Offline request reduction

[summarizeLoadRequests](../../scripts/qualification/load-request-summary.mjs)
accepts a retained arrival schedule separately from terminal observations.
Each schedule entry has an opaque ID, one of nine operation names and a monotonic
`scheduledMs`. Each observation binds that ID to `startedMs`, `finishedMs` and
one of succeeded, failed, timeout, rejected, cancelled or ambiguous. All times
share one monotonic clock and phase origin. The caller supplies phase duration,
window duration and maximum drain duration, all in milliseconds.

The reducer reports all nine operation classes in every window and in the full
phase. Missing arrivals/observations remain MISSING, with an unavailable error
rate; empty cohorts have null percentiles. Latencies include scheduled queueing
delay and terminal failures. Nearest-rank percentiles are recomputed from raw
values, never averaged from window summaries. Service p99 is separately retained
so it cannot hide queueing delay in end-to-end p99.

Latency and outcome summaries follow arrival cohorts including bounded drain.
Successful throughput follows completions in half-open time windows, excluding
post-phase drain. Thus a late completion can affect a later window's throughput
while its latency remains attributed to its arrival window. All nonsuccess
outcomes count in the error numerator; expected rejections require separate
qualification cells. The reducer never assumes a successful HTTP response is a
correct durable operation: independent outcome classification remains upstream.

Input bounds are 100000 requests, 100000 observations and 10000 windows. Duplicate,
unbound, invalid or chronologically inconsistent events reject. This is an
in-memory API; the caller must bound files before parsing, verify artifact
signatures/revisions and bind the schedule and observations to the approved
phase. The reducer always returns admitted:false and executionAuthorized:false.
It does not check operator thresholds or independently establish observation
provenance, resource telemetry, tenant state, abort/cleanup or full-phase coverage.

## Collector gaps

The inspected API source includes event telemetry and specialized subsystem
metrics. That is not evidence of a complete DQ-LOAD collector suite. Still map
and implement: cgroup CPU/RSS, filesystem/inodes, PostgreSQL size/WAL/pool/locks,
blob staging, queue age/depth, independently observed freshness, provider call
and billing counters, and watchdog/cleanup timing. Every collector needs its
source revision, timebase, unit, aggregation semantics and stale-data policy.

Next: finish the API and provider limit inventory, bind raw-request reduction
to an approved phase/window threshold evaluator, and build bounded driver and
collector adapters. The execution plan's dependency and approval gates remain
in force; this utility and its synthetic tests do not qualify #1141.

## Request threshold evaluation

[evaluateLoadRequests](../../scripts/qualification/evaluate-load-requests.mjs)
consumes a separate five-phase plan and observations keyed by phase name. Each
plan phase contains name, durationMs, windowMs, drainMs, retained schedule and
thresholds for every operation. Each operation policy contains minimumSamples,
latencyP50/P95/P99, throughputPerSecond and errorRate. Each threshold has an
explicit operator, limit and unit. Latency upper bounds use milliseconds;
throughput lower bounds use operations/second and must be positive; error bounds
use a ratio. Missing policy, incorrect units, inverted comparisons and nonfinite
limits reject before observations are considered.

The same approved policy applies to the phase and each of its windows; a window
cannot be excluded after results are known. Sample floors also apply to every
window. Raw input is reduced internally rather than accepting caller-supplied
percentiles. Phase reports preserve both failed comparisons and missing coverage.
An invalid observation stream produces FAIL; absent phase evidence or incomplete
samples produce MISSING. Reports use canonical phase order irrespective of input
order. The total plan is bounded to 100000 scheduled requests and 1000 windows.

`requestChecksPass` applies only to these request metrics. `admitted` and
`executionAuthorized` remain false. This module does not authenticate the plan,
verify resource/provider measurements, enforce the driver schedule, or qualify
limit-plus-one, topology, recovery, redaction or cleanup. The caller must bind
these inputs to signed immutable evidence and supply the remaining checks.
The test fixtures' numeric values are synthetic regression values, not proposed
production thresholds. Driver and collector integration remain outstanding.

## Resource observation evaluation

[evaluateLoadTelemetry](../../scripts/qualification/evaluate-load-telemetry.mjs)
checks a bounded interval of telemetry frames independently of request summaries.
The policy requires durationMs, maxGapMs, maxAgeMs and a threshold for each metric
in LOAD_TELEMETRY. Each frame has a monotonic timeMs and metric observations with
value, unit and observedMs. Each collector's own timestamp is checked, so a fresh
frame timestamp cannot conceal an old provider or storage reading.

Required fields cover RSS, allocated-core-normalized CPU percent, storage growth,
retained/generated WAL, blob growth, queue depth/age, pool utilization/timeouts,
lock waits/deadlocks, freshness, cumulative provider calls/cost and free bytes/
inodes. Bytes/counts must be safe integers; percentages use a 0–100 scale.
Provider cost uses USD under a separately pinned pricing policy. Cumulative
counters cannot reset during an interval; snapshots such as retained WAL may
shrink, but every observed peak is still compared. Baseline normalization and
counter provenance remain the collector's responsibility.

The first frame must be at zero and the final frame at the declared duration;
interior gaps and individual observation ages are bounded by policy. A missing
metric or stale reading is MISSING, while an invalid value, inconsistent clock,
counter reset or breached threshold is FAIL. Both finding counts are retained
when both occur. Reports retain at most 1000 details of each kind and explicitly
flag truncation; total counts remain authoritative. The input limit is 10000
frames. No interpolation or whole-run average can erase an observed violation.

`telemetryChecksPass` means only that retained resource observations satisfy the
supplied policy. The evaluator does not collect measurements, authenticate them,
or enforce a live abort. Between-sample behavior is unproven; select collection
cadence and independent watchdog reserves accordingly. Actual baseline capture,
collector adapters, authority binding, phase orchestration, growth-rate safety,
abort/recovery deadlines and independent cleanup remain required for #1141.

## Read-only Linux adapter

[collectLinuxLoad](../../scripts/qualification/collect-linux-load.mjs) reads
explicitly selected process smaps_rollup RSS, cgroup-v2 cpu.stat usage, and
filesystem free bytes/inodes. Kernel interface reads are capped at 64 KiB;
nonregular files and final-component symlinks reject. At most 32 explicit PIDs
are accepted. Process start identities are checked around each RSS read to
reject PID reuse. The cgroup directory is anchored by an open descriptor and
its filesystem type is checked. No process is spawned or terminated and no
service, namespace or filesystem is modified.

Raw samples retain monotonic acquisition timestamps, process start identities
and cgroup device/inode. CPU percent is derived from two observations of the
same cgroup, normalized by the operator's allocated-core count; counter/clock
resets reject and values are never clamped. The adapter uses the kernel's
[cgroup-v2 CPU interface](https://docs.kernel.org/admin-guide/cgroup-v2.html)
and [proc memory reporting](https://docs.kernel.org/filesystems/proc.html).

Summed RSS can count shared pages more than once. Cgroup CPU includes the
selected cgroup's hierarchy, whereas RSS covers only the supplied PIDs. The
operator/verifier must bind both scopes to the signed topology, account for
worker churn and check membership separately. Device/inode is not a substitute
for machine/boot identity across separate runs. Filesystem availability is a
snapshot for the supplied path, not a reservation or proof of isolated storage.
Snapshots cannot establish between-sample peaks or cross-process atomicity.

This adapter deliberately returns raw timestamps, not fabricated time-zero
telemetry frames. The orchestrator must establish a baseline, preserve each
collector's acquisition time, map it to the phase clock and reject stale input.
Database/WAL/blob, queue, lock, freshness and provider collectors remain absent;
they must not be filled with zeros. The local kernel-interface regression test
exercises only collection in this environment and is not a load qualification.

## Bounded scheduling mechanism

[runLoadSchedule](../../scripts/qualification/run-load-schedule.mjs) executes an
ordered retained schedule through caller-supplied execute and checkSafety
adapters. The caller must authorize the exact plan and adapters before invocation;
this module has no endpoints, credential discovery or dynamic code loading.
Each request retains its original scheduledMs even when concurrency is saturated.
Excessive scheduling delay stops dispatch; saturation cannot rewrite the workload
into an easier closed-loop schedule. The full schedule and undispatched IDs remain
in the result, alongside terminal observations and unresolved request IDs.

The plan supplies durationMs, drainMs, maxConcurrency, maxScheduleLagMs, pollMs and
safetyTimeoutMs. Implementation ceilings are 12 hours, 5 minutes drain, 32 active
calls, 100000 arrivals, 1 second polling and 10 seconds per safety callback. These
are mechanism bounds, not approved capacity or SLO values. A true safety result
is required before dispatch and on subsequent polling, including normal drain.
A safety exception, false result or timeout stops new arrivals. Executor exceptions
are ambiguous rather than assumed to have produced no effect; private adapter
error text is not copied into receipts.

Abort uses a shared AbortSignal. Cooperative adapters must honor it and report
outcomes only after checking their actual effects. A hung executor retains its
slot; elapsed drain produces unresolved IDs and executionSettled:false. Results
are copied before return so a late promise cannot rewrite a retained report.
Cleanup is always unverified at this layer. Expected cancellation must be
classified independently; absence of an exception is not correctness evidence.

This in-process scheduler cannot interrupt blocking JavaScript, terminate an
uncooperative adapter or prove cancellation of remote work. After an abort it
stops further safety callbacks and relies on the independent external watchdog
and bounded teardown to control residual work. The OS-level watchdog, actual
runtime adapters, durable receipt writer, state observer and namespace cleanup
must be supplied before this mechanism can support a qualification run. Tests
use local synthetic callbacks only; no deployed service or provider is targeted.

## Existing API transport integration

[createLoadApiTransport](../../scripts/qualification/load-api-transport.mjs)
implements the apiRequest signature accepted by the existing
createDatasetExecutionController. Configuration explicitly supplies an origin,
authentication token, memory namespace, exact method/path permissions, request/
response byte bounds, timeout and evidence observer. No environment credentials
or default namespace are discovered. Redirects and requests outside the configured
scope reject; request-size failures occur before fetch. Response limits apply to
streamed bytes as well as Content-Length. Caller cancellation and a fixed timeout
cover the network operation and metadata observer.

Successful JSON responses are returned to the existing controller, which retains
its own storage receipt/checkpoint checks. Transport success alone is not a
successful operation classification. HTTP errors expose only stable transport
codes and status, not private response bodies. Metadata retains timing, status,
byte counts and a response digest without token, URL query, namespace or content.
The full independent state/content proof is still required. Network failures,
oversized responses and missing metadata can occur after mutation; classify them
as ambiguous until independently reconciled, never as zero-mutation rejections.

The local integration regression runs the actual dataset controller against a
synthetic HTTP server using the existing supported-request fixture and a bound
storage response. It verifies transport/controller compatibility only; it does
not use PostgreSQL or qualify durable execution. Binary export/import and SSE
are not supported by this JSON transport and still require bounded adapters.
The origin and credentials must be approved and bound to the isolated topology
before runtime use. Same-origin URL checks are not network/DNS isolation proof.

## PostgreSQL 18 monitoring adapter

[collectPostgresLoad](../../scripts/qualification/collect-postgres-load.mjs)
runs the fixed [snapshot SQL](../../scripts/qualification/load-postgres-snapshot.sql)
through psql with an explicit connection object. It does not discover deployed
credentials. It ignores psql startup files, disables password prompts, enforces
read-only transactions and statement/lock/connect timeouts, and caps subprocess
output and elapsed time. Private connection/server errors are replaced by a
stable unavailable-observation error.

The snapshot records server/database identities and statistics-reset timestamps,
database size, deadlocks, connections, waiting locks, oldest observed lock wait,
WAL generated and WAL retained. A monitoring role must have the required
statistics and WAL-directory privileges; the adapter requires visible complete
statistics and rejects unavailable wait timestamps. The regression creates a
small disposable PostgreSQL 18 cluster on a private Unix socket, grants pg_monitor
to its test observer, reads the snapshot and tears down that cluster. It never
connects to the configured application database.

PostgreSQL [statistics can lag and are cached within a transaction](https://www.postgresql.org/docs/18/monitoring-stats.html).
Every collection uses a new bounded read-only transaction. Lock waits use
[pg_locks.waitstart](https://www.postgresql.org/docs/18/view-pg-locks.html), not
query start time. WAL observations cover the entire cluster; database-size and
deadlock observations cover the connected database. Connection count is not
application pool utilization, and statistics alone do not prove real-time
resource ceilings. Sampling cadence must account for statistics lag.

postgresLoadDelta derives growth from a retained baseline only when database,
server-start and reset identities agree, rejecting cumulative counter decreases.
Retained database/WAL size can shrink; reported growth is floored at zero while
raw snapshots remain retained. Apply comparisons to every sample, not only the
last one. Environment identity and topology still require external binding;
these fields alone do not distinguish two unrelated clusters with copied state.

Remaining application-specific gaps include SQLx pool utilization/timeouts,
queue age, blob staging, independent freshness and provider billing. Source
inventory found log_pool_metrics in crates/matric-db/src/pool.rs and queue_stats
in crates/matric-db/src/jobs.rs, exposed at GET /api/v1/jobs/stats. Existing pool
logs and queue counts do not yet supply the full required collector matrix.
