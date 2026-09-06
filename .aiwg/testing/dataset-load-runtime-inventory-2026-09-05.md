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
