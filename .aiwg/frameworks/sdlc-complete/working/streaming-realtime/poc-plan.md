---
artifact: poc-plan
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: draft-v1
last-updated: 2026-05-11
---

# Streaming Cluster — PoC Plan (Phase 1)

Two PoCs to retire architectural risk before full Phase 1 commissioning. Both are time-boxed to **scope-bounded deliverables** (no wall-clock estimates per `no-time-estimates` rule).

## PoC P-01: Outbox publisher under load — does SKIP-LOCKED + UPDATE scale?

### Hypothesis to test
The proposed publisher design (poll `WHERE published_at IS NULL FOR UPDATE SKIP LOCKED LIMIT N` → XADD → `UPDATE SET published_at = NOW()`) sustains **10K events/sec for 60s** with **<1s p99 publisher lag** on the standard `matric-builder` CI hardware tier, without UPDATE hot-row contention degrading PG vacuum throughput.

### Why this is the right risk to retire first
R-01 (hot-row contention) is the highest critical risk and the prerequisite for ADR-005 (retention) and ADR-006 (trim policy). If the polling outbox doesn't scale, we pivot to push-outbox via logical replication slot (per C3, INDUCT pending re-auth).

### Method

1. Create disposable PG 18 + Redis 7 stack via `docker compose -f docker-compose.bundle.yml` (test database).
2. Build a minimal publisher binary in `crates/matric-jobs/examples/poc_outbox.rs` — no integration with API/MCP. Pure outbox→XADD loop.
3. Driver: synthesize event-insert load via direct SQL — 10K/sec sustained for 60s, payload ~1 KB JSONB.
4. Measure:
   - Publisher loop p50/p95/p99 latency (time from outbox INSERT commit to XADD return)
   - Publisher batch size distribution
   - PG vacuum queue depth + autovacuum runtime
   - Redis XLEN at end of run
   - PG hot-row contention via `pg_stat_user_tables.n_tup_upd` rate
5. Replicate with **partitioned outbox table** (weekly partitions via pg_partman) and compare.

### Pass criteria (objective, per `vague-discretion`)

- [ ] Publisher p99 lag ≤ 1000 ms over 60s sustained
- [ ] Zero event loss (XLEN equals INSERT count, modulo retention trim)
- [ ] Autovacuum keeps up (no manual VACUUM intervention)
- [ ] If partitioned variant beats unpartitioned by ≥30% on p99, that's the production choice and ADR-005 cites the PoC

### Fail handling

If unpartitioned PG doesn't hit pass criteria: ADR-005 mandates partitioning. If partitioned still doesn't: escalate to push-outbox (C3) PoC. Decision goes back to user before committing engineering effort.

### Artifact outputs
- `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/poc-results/p01-outbox-load.md`
- Recorded benchmark numbers + flame graphs
- Recommendation: partition vs not, ADR-005 input

---

## PoC P-02: Redis Streams consumer group round-trip with failure injection

### Hypothesis to test
A `XREADGROUP` consumer with bounded mpsc + `XAUTOCLAIM` recovery survives mid-batch process kill with **zero event loss** and **measurable but bounded duplicate delivery** (at-least-once, dedupable by ULID consumer-side).

### Why this PoC is needed
R-03 (consumer-group rebalance) and R-04 (slow consumer) are high-impact failure modes. Without measuring them, ADR-007 (consumer-group topology) is speculative.

### Method

1. Reuse PoC P-01 stack; pre-seed 100K events into a test stream.
2. Build a minimal consumer in `crates/matric-jobs/examples/poc_consumer.rs` — XREADGROUP loop + bounded `mpsc(64)` + acker task.
3. Driver injects faults:
   - **F1**: kill consumer mid-batch (`SIGKILL`); restart; measure recovery time and duplicate count
   - **F2**: redis-cli `DEBUG SLEEP 5` mid-stream; measure publisher backpressure
   - **F3**: slow consumer: sleep(100ms) per event in one of two replicas; measure PEL growth and other consumer's continued progress
4. Measure:
   - Duplicate delivery rate (must be bounded; ideally <1% of events)
   - Recovery latency (XAUTOCLAIM round-trip to resume position)
   - PEL depth time-series during F3
   - Bounded-channel pushback signal — does upstream stall correctly?

### Pass criteria

- [ ] F1: zero event loss; duplicates ≤ in-flight batch size; recovery <5s
- [ ] F2: publisher doesn't crash; lag recovers within 30s of Redis return
- [ ] F3: slow consumer's PEL grows linearly; fast consumer's throughput unaffected; backpressure signal observable
- [ ] Idempotency-key dedup at consumer reduces duplicate-observable rate to ~0

### Artifact outputs
- `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/poc-results/p02-consumer-chaos.md`
- ADR-007 (consumer-group topology) + ADR-008 (idempotency-key shape) inputs

---

## PoC sequencing

P-01 and P-02 are **independent** and can run in parallel by two engineers. Both must pass before #591/#593/#594 are commissioned for production implementation. If either fails, escalate to user for design pivot.

## Out of scope for PoC

- Performance under multi-tenant / multi-archive load → deferred to Phase 1 integration test suite
- TLS-encrypted Redis → deferred (operational concern, not architectural)
- Cross-region replication → not a Fortemi requirement
- LLM-as-stream-operator (Pie, C6) integration → Phase 2/3 PoCs filed separately when commissioned
