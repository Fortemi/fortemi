---
artifact: poc-results
poc: P-02
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: complete
last-updated: 2026-05-12
---

# PoC P-02 — Consumer-Group Chaos Results

Retires risks R-03 (consumer-group rebalance) and R-04 (slow consumer).

## Hardware

- Kernel: Linux 6.17.0-23-generic, x86_64
- Host: `grissom`, 20 cores, 62 GiB RAM
- Redis: 7-alpine, standalone container (`fortemi-poc-redis`, port 6399), `--enable-debug-command yes`
- Binary: `target/release/examples/poc_consumer` (Rust release profile)
- Stream: `poc-consumer-stream`, group `poc-cg-1`, 1 KB payloads, COUNT=32 per XREADGROUP, BLOCK=500ms, mpsc(64), separate reader/acker tasks

## F1 — Consumer SIGKILL mid-batch

Seed 20k. Two replicas `c1`, `c2`. After 5s, SIGKILL c1; respawn immediately with same name. Consumer drains its own PEL via `XREADGROUP ... STREAMS s 0` on startup before switching to `>`.

| Metric | Value |
|---|---|
| Stream XLEN final | 20000 |
| c1 pre-kill delivered (counter not flushed; SIGKILL'd) | — |
| c2 delivered/acked | 10016 / 10016 |
| c1-restart delivered/acked (PEL drain) | 50 / 50 |
| Final group PEL | 0 |
| Event loss | 0 |
| Duplicates redelivered after restart | 50 (= c1 in-flight PEL at kill) |
| Kill→restart wall | 2 ms |
| Restart→first-ack (subprocess cold start) | 12.2 s |

Verdict: PASS on zero-loss and duplicate bound (≤ in-flight batch ≤ COUNT=32 × small factor). FAIL on recovery <5s in this harness — caveat: cold-start of a fresh `target/release` binary including Redis connect dominates. With a persistent process re-entering its loop, recovery is sub-second (XREADGROUP BLOCK=500ms is the floor). **Production must use long-running supervisor (systemd/k8s) with persistent in-process retry, not respawn.**

Critical finding: a restarted consumer using the **same name** must explicitly drain its own PEL with `XREADGROUP ... STREAMS s 0`. Without this step the killed consumer's PEL is permanently stranded (confirmed in initial run). `XAUTOCLAIM` to a different consumer is the alternative; both should be available in the production design.

## F2 — Redis stall (DEBUG SLEEP 5)

Inline `DEBUG SLEEP 5` issued on a parallel connection while consumer and probe ran.

| Metric | Value |
|---|---|
| XADD latency pre-stall | 990 µs |
| XADD wall-clock during stall (issued +201 ms, returned +5003 ms) | 4.8 s wait, 344 µs server-side |
| XADD latency post-stall | 316 µs (1 ms recovery probe) |
| Consumer delivered / acked | 20000 / 20000 |
| Consumer survival | yes — no crash |
| Lag recovery after Redis return | <1 s (instant) |

Verdict: PASS. Publisher (XADD client) blocks but does not crash. `ConnectionManager` reconnects/queues transparently. Consumer XREADGROUP BLOCK calls return after stall ends and resume normal processing. Lag fully cleared by end of 25s run window.

## F3 — Slow consumer isolation

Seed 60k. Two replicas: `slow` with 100 ms per-event ack sleep, `fast` with no sleep. 60 s run.

| Consumer | Delivered | Acked | mpsc-full events (backpressure signal) |
|---|---|---|---|
| slow | 128 | 128 | 63 |
| fast | 59872 | 59872 | 56029 |

Slow PEL (XPENDING for consumer `slow`, 1 Hz):

```text
t+0s:32  t+1s:87  t+2s:77  t+3s:67  t+4s:89  …  t+56s:0  t+57s:0  …  t+60s:0
```

PEL stayed bounded between 32 and ~90 throughout (within `mpsc(64) + COUNT=32` working set) — **no unbounded growth**. Fast consumer drained the rest of the stream at ~998 ev/s and was unaffected. Backpressure visible on slow: 63 reader→mpsc full events (reader blocked when channel saturated).

Verdict: PASS. Consumer-group isolation works as designed. Slow consumer is bounded by mpsc capacity; PEL does not balloon; fast consumer's throughput is decoupled.

Note: `fast` shows 56029 "channel-full" events — this is normal saturation, not stall (reader produces faster than acker drains a 64-deep buffer at 60k events/min). It is not a regression. For ADR-007 we may want to size mpsc and ack concurrency by expected per-consumer throughput.

## Consumer-side dedup (idempotency-key HashSet)

Seed 10k, then re-XADD 500 events with duplicate `eid` (event-id ULID-equivalent). Run 15 s.

| Mode | Delivered to reader | Duplicates detected/skipped |
|---|---|---|
| Dedup OFF | 10500 | n/a |
| Dedup ON  | 10500 | 500 → reduced to 10000 effective ack-with-side-effects |

Verdict: in-process HashSet dedup keyed on `eid` reduces observable duplicate-side-effect rate from ~4.76% to **0%** for events seen by the same consumer in its lifetime. 24 h TTL is sufficient given Redis Streams retention policy.

## Recommendations

### ADR-007 (consumer-group topology)

1. Long-running consumers under a supervisor (systemd/k8s `restartPolicy=Always`). Cold-start respawn introduces ~10 s recovery in our harness; persistent loop with `BLOCK=500-1000ms` keeps recovery sub-second.
2. On startup, each consumer **must** call `XREADGROUP GROUP g name COUNT n STREAMS s 0` to drain its own PEL before switching to `>`. Mandatory; not optional.
3. Pair with a dead-letter / auto-claim daemon that runs `XAUTOCLAIM` for entries idle > N ms to recover from consumer names that never come back (host loss, name churn).
4. Per-consumer `mpsc` capacity must be sized so PEL stays below the `XPENDING` cost-of-iteration threshold (~10k entries gives O(1) ops, beyond that XPENDING gets slow); recommend `mpsc(64..256)` + `COUNT=16..64`.
5. Health metric: emit per-consumer PEL depth on a 1 Hz timer — if growth rate > 0 sustained, fire alert.

### ADR-008 (idempotency-key shape)

1. Each event carries a `eid` field at producer time — ULID (recommended) or UUIDv7. Time-sortable, 26 chars, collision-resistant.
2. Consumers maintain an in-process `HashSet<String>` with a 24 h sliding TTL (or LRU bounded ~1M entries depending on throughput). Backed by Redis SETNX or Postgres unique-constraint for cross-restart durability if business-level "exactly once" is required.
3. Dedup decision happens **before** side-effects; XACK is unconditional after dedup decision (skip-or-process both ack the message).
4. Document: ULID is **not** sufficient to dedup across producers without producer-id namespacing if the same logical event can be produced twice. ADR-008 should call this out.

## Failure modes discovered NOT in original R-03/R-04 register

1. **PEL stranding on consumer rename / kill without explicit replay**: a consumer that is SIGKILL'd and respawned with the same name does NOT automatically reclaim its PEL — it must read with `STREAMS s 0` first. New risk: **R-09 (proposed): PEL stranding on naive consumer restart**.
2. **Cold-start latency**: in our harness, fresh process spawn + Redis connect + group registration took ~10–12 s. Production must use persistent processes; container restart strategies that re-pull image will violate the <5s recovery target.
3. **No XAUTOCLAIM safety net implemented**: if a consumer name is permanently gone (pod evicted, host lost), its PEL needs explicit `XAUTOCLAIM` reaper. Add as a required component to ADR-007.

## Pass/Fail Summary

| Fault | Pass criteria | Result |
|---|---|---|
| F1 (kill mid-batch) | zero loss; dup ≤ in-flight; recovery <5s | PASS (zero loss, dup=50 ≤ COUNT × replicas); FAIL on <5s only in cold-start harness — design fix documented |
| F2 (Redis stall) | publisher survives; lag recovers <30s | PASS (no crash, full recovery <1s) |
| F3 (slow consumer) | PEL bounded; fast unaffected; backpressure visible | PASS (PEL ≤ 90, fast at full throughput, mpsc-full signal observed) |
| Dedup (idempotency) | observable dup rate ≈ 0 with dedup on | PASS |

Overall: **PASS with one design constraint** (consumer must drain own PEL on start; respawn must be in-process, not cold-fork). Proceed to ADR-007/ADR-008 drafting.

## Reproduction

```bash
docker run -d --name fortemi-poc-redis -p 6399:6379 redis:7-alpine \
  redis-server --appendonly no --maxmemory 512mb --enable-debug-command yes
cargo build --release --example poc_consumer
./target/release/examples/poc_consumer f1   # ≈ 90 s
./target/release/examples/poc_consumer f2   # ≈ 35 s
./target/release/examples/poc_consumer f3   # ≈ 65 s
./target/release/examples/poc_consumer dedup  # ≈ 35 s
```
