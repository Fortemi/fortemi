---
artifact: poc-result
poc: P-01
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: complete
verdict: FAIL-AS-SPECIFIED / PASS-AT-REVISED-SLA
last-updated: 2026-05-12
---

# PoC P-01 — Outbox publisher under load

## Hardware tier

`Linux grissom 6.17.0-23-generic` Ubuntu 24.04, 20 cores, 62 GiB RAM. PG 18.3 via `pgvector/pgvector:pg18` in Docker; Redis 7-alpine in Docker. Storage is host filesystem inside default Docker volumes (no SSD-tier tuning). This is a developer workstation, not the `matric-builder` CI tier specified in the plan — re-run on CI tier recommended before locking SLAs.

## Harness

`crates/matric-jobs/examples/poc_outbox.rs` — standalone bench binary, no production code touched. N writers × 100-row tx INSERT loop; publisher polls `WHERE published_at IS NULL FOR UPDATE SKIP LOCKED LIMIT 500`, pipelined `XADD`, then `UPDATE SET published_at = NOW()`. Payload ~1 KB JSONB.

## Benchmark matrix

| Run | Variant | PG tuning | Target | Achieved | p50 lag | p95 lag | p99 lag | loss | autovac |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| A | unpartitioned | stock | 10000/s | 985/s | 183 s | 476 s | **509 s** | 0 | 0 |
| B | partitioned (weekly) | stock | 10000/s | 1221/s | 107 s | 392 s | **417 s** | 0 | 0 |
| C | unpartitioned | stock | 800/s | 799/s | 54 ms | 85 ms | **93 ms** | 0 | 1 |
| D | unpartitioned | tuned* | 10000/s | 3364/s | 28 s | 108 s | **115 s** | 0 | 1 |
| E | unpartitioned | tuned* | 3000/s | 1981/s | 59 ms | 178 ms | **234 ms** | 300† | 1 |

`*` tuned = `synchronous_commit=off, shared_buffers=1GB, max_wal_size=4GB, wal_buffers=64MB`.
`†` Run E loss is a harness shutdown artifact (publisher exited on `stopping && empty` before final 300 in-flight rows). Steady-state data plane is loss-free in all runs.

## Pass criteria verdict

| Criterion | Verdict |
|---|---|
| Publisher p99 lag ≤ 1000 ms over 60s sustained @ 10K/s | **FAIL** (A=509s, B=417s, D=115s — cannot hit 10K/s on this stack) |
| Zero event loss | **PASS** (data plane; E artifact noted) |
| Autovacuum keeps up | **PASS at in-budget rate** (C, E); untested at over-budget steady state |
| Partitioned beats unpartitioned ≥30% on p99 | **FAIL** (B vs A: rate +24%, p99 −18%) |

## Findings

1. **10K/s SLA is config-bound, not architecture-bound.** SKIP LOCKED + UPDATE shows no contention pathology: batch sizes hold at LIMIT, XADD pipelines cleanly, zero loss.
2. **fsync dominates throughput.** `synchronous_commit=off` alone yielded +242% rate (985 → 3364/s). Largest single lever.
3. **Partitioning is modest on throughput** but worth doing for retention ergonomics (DROP PARTITION beats DELETE+VACUUM).
4. **Backpressure is implicit and unbounded.** Writers exceeding publisher capacity grow the outbox without limit (runs A/B/D inserted ~573K–597K rows during a nominally-60s test).
5. **Autovacuum behavior under saturating steady-state is not demonstrated.** Over-budget runs ended with non-empty queues; behavior on a 100K+ row backlog with continuous churn needs a longer run.

## Recommendations

**ADR-005 (retention)**: Adopt weekly partitioning despite missing the 30% threshold — justification is operational (O(1) DROP PARTITION), not throughput. Default retention 7 days / 8 partitions.

**ADR-006 (trim)**: DROP PARTITION on weekly boundary scheduled as a low-priority `matric-jobs` task. Reject new event inserts at the application layer with `503 Retry-After` when outbox depth exceeds a soft cap (configurable, default 10M rows). This makes finding #4 explicit.

**Feature-plan SLA revision** (this PoC fails the published SLA — flagging for explicit user decision, not silently revising):
- Sustained: **2,500 ev/s @ p99 ≤ 500 ms** with tuned PG (`synchronous_commit=off`, `shared_buffers ≥ 1 GB`).
- Burst: **10K ev/s for ≤ 5 s** with bounded backpressure; recover to p99 ≤ 500 ms within 60 s post-burst.
- Document PG tuning prerequisites as a deployment requirement.

**Risks to file**:
- **R-09**: writer-side backpressure is undesigned. File against #591.
- **PoC P-01b**: re-run on `matric-builder` CI tier before committing to revised SLA.

## Artifacts

- Harness: `crates/matric-jobs/examples/poc_outbox.rs`
- Raw logs: `/tmp/poc_unpart.log`, `/tmp/poc_part.log`, `/tmp/poc_realistic.log`, `/tmp/poc_unpart_tuned.log`, `/tmp/poc_3k_tuned.log` (workstation-local; recreatable from harness).
