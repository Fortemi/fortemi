---
artifact: feature-plan
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: draft-v2
last-updated: 2026-05-11
references:
  - research-synthesis: ../../research-complete/working/streaming-realtime/research-synthesis.md
---

# Streaming Cluster — Feature Plan (v2)

## 1. Scope

Replace Fortemi's batch-oriented job processing with event-driven, incremental pipelines. Scope locked to **Phase 1 (Redis Streams + outbox)** for immediate commissioning. Phases 2–3 sequenced as follow-ons.

ADR-001..004 are locked. This plan is execution-level: components, sequencing, gating, and verification.

## 2. Phase 1 — Redis Streams Event Bus (commissioning candidate)

### 2.1 Construction order (dependency graph)

```
#590 superseded (raw redis::cmd stream operations compile without typed streams feature)
       #591 event_outbox migration ─┐
          └─> #592 outbox helpers ──┤
                └─> #593 publisher ─┴─> #594 SSE consumer
                                        #595 WS+webhook consumer
                                        #596 outbound bus health metrics
```

`#590` is closed as superseded: the current Redis Stream connector uses raw
`redis::cmd(...)` operations, and `cargo check -p matric-jobs --examples`
passes without enabling `redis::streams::*`. `#591` is the durable foundation.
`#592` depends on `#591`. `#593` depends on `#592`. `#594`, `#595`, and `#596`
are parallelizable once `#593` lands.

### 2.2 Component design (engineering team to refine)

**`event_outbox` table (issue #591)**
- Same PG transaction as data write → at-least-once delivery guarantee
- Schema sketch (engineering to formalize): `(id ULID PK, aggregate_type, aggregate_id, event_type, payload JSONB, occurred_at TIMESTAMPTZ, published_at TIMESTAMPTZ NULL, attempt_count INT DEFAULT 0)`
- Lives in `public` schema (shared, not per-archive) — see CLAUDE.md Multi-Memory section; events MAY reference archive-scoped aggregates by FK + `archive_id` column
- Partition by week (`pg_partman`-style; cite REF-324) — keeps trimming O(DROP TABLE)
- Index strategy: `(published_at NULLS FIRST, occurred_at)` for publisher scan

**Outbox insert helpers (#592)**
- Single `outbox_insert(tx, OutboxEvent)` taking the same `&mut Transaction` as the data write
- Migration of existing event emitters (currently writing direct to SSE channel) to write via outbox; old path removed after `#594`/`#595` complete
- Compile-time enforcement: SSE/WS emitters become private; only outbox publisher reaches the wire

**Publisher task (#593)**
- Single-process Tokio task; polls outbox `WHERE published_at IS NULL ORDER BY occurred_at LIMIT N FOR UPDATE SKIP LOCKED`
- Per-batch loop: read → `XADD stream * payload event_id ...` to per-event-type Redis stream → `UPDATE event_outbox SET published_at = NOW()` → commit
- Failure handling: on Redis error, leave `published_at` NULL, increment `attempt_count`, exponential backoff at task level; on PG error, abort batch
- Stream trimming: `XADD ... MAXLEN ~ 100000` per stream (approximate trim, per REF-328 trade-off notes)
- Wake signal: `tokio::Notify` from `outbox_insert` to avoid polling latency (mirrors existing job worker pattern)

**Consumer migration (#594, #595)**
- SSE handler: replace internal broadcast channel with `XREADGROUP` loop in dedicated consumer group `sse-fanout`
- WS dispatcher: separate consumer group `ws-fanout` (allows independent scaling, separate XPENDING tracking)
- Webhook dispatcher: separate consumer group `webhook-dispatch` (different reliability SLO — must survive process restart)
- Pending message handling: XPENDING + XCLAIM for stalled consumers on restart

**Health metrics (#596)**
- Prometheus gauge: outbox lag = `count WHERE published_at IS NULL`
- Prometheus gauge: stream depth per type via `XLEN`
- Prometheus gauge: oldest-pending-age per consumer group via `XPENDING` summary
- Alerts on lag > 1000 events, oldest-pending > 30s
- Surface on `/api/v1/health/streaming` endpoint

### 2.3 Backwards-compatibility & migration

- `REQUIRE_AUTH=false` deployments must not break: outbox writes are additive
- Existing in-process SSE channel kept live in parallel during #594 rollout (`OUTBOX_FANOUT=shadow` mode → write to both, consume from old)
- Cutover via `OUTBOX_FANOUT=primary` env flag; rollback = flip back; `OUTBOX_FANOUT=only` removes old code (separate cleanup PR after 1 week of soak)

### 2.4 Test strategy

- **Unit**: outbox insert helpers under property-based tests (transactionality invariant: outbox row exists ⟺ data row exists)
- **Integration**: full publisher loop on test PG + test Redis; verify at-least-once delivery, idempotent re-publish on `published_at` rollback
- **Chaos**: kill publisher mid-batch; verify no event loss, no double-publish detectable by consumer-side idempotency
- **Load (tiered SLA — revised post-P-01 2026-05-12)**:

  | Tier | Sustained | Burst (≤5s) | PG config | Status |
  |---|---|---|---|---|
  | Edge (stock) | **1K events/sec** @ p99 ≤ 1s | 2.5K/s | default `synchronous_commit=on` | measured P-01 |
  | Edge (tuned) | **2.5K events/sec** @ p99 ≤ 500ms | 5K/s | `synchronous_commit=off`, `shared_buffers ≥ 1GB` | measured P-01 |
  | Mid-tier | ~5K/s (extrapolated) | TBD | tuned | not yet measured |
  | High-end (titan) | target **10K+ events/sec** @ p99 ≤ 1s | 20K/s | tuned | TBD pending P-01b |

  REF-324 (RudderStack 100K/sec partitioned PG) remains the theoretical ceiling under aggressive partitioning + dedicated hardware
- **Failover**: Redis restart mid-stream; verify XCLAIM recovery; consumer-group resume from last ack
- Per CLAUDE.md: **NO `#[ignore]`, NO `SKIP_INTEGRATION_TESTS`**. All tests run in CI.

### 2.5 Open ADRs (engineering team to author during commissioning)

- **ADR-005**: Outbox retention policy (drop-after-N-days vs keep-forever) — REF-324/330 trade-off
- **ADR-006**: XADD MAXLEN trimming strategy — fixed ring vs approximate vs time-windowed
- **ADR-007**: Consumer-group topology — per-service vs per-worker
- **ADR-008**: Idempotency-key shape — ULID-per-event vs `(aggregate_id, version)`

## 3. Phase 2 — Incremental AI Pipeline (deferred)

Sequenced behind Phase 1 stabilization (≥2 weeks production observation). Scope locked to: #597 content-hash tracking → #598 incremental embed skip → #599 incremental graph linking → #600 dynamic GPU batching → #602 batch coalescing. #601 (event-driven extraction) deferred to high-end-tier-only deployment.

Key risk: content-hash gating must be Matryoshka-aware once MRL embeddings ship (current code path uses MRL for storage savings — see CLAUDE.md). Add ADR-009 (MRL-aware skip).

## 4. Phase 3 — Advanced Streaming (deferred)

Sequenced behind Phase 2. Encompasses #603 (CDC eval), #604 (live semantic search via streaming vector index), #605 (Drift-Adapter), #606 (streaming KG). REF-563 GAP NOTE is a structural risk for #606 — design defensively (at-least-once + idempotent LLM side-effects; no exactly-once promise).

## 5. Hardware-tier capability matrix (from ADR-004)

| Capability | Edge (6-8GB) | Mid (12-16GB) | High-end (24GB+) |
|---|:---:|:---:|:---:|
| Event-driven wake-up (Phase 0) | ✅ | ✅ | ✅ |
| Redis Streams + outbox (Phase 1) | ✅ | ✅ | ✅ |
| FTS index updates | ✅ realtime | ✅ realtime | ✅ realtime |
| SSE/WS/webhook fan-out | ✅ realtime | ✅ realtime | ✅ realtime |
| Embedding refresh | batch (existing) | ✅ incremental | ✅ incremental |
| Graph linking | batch (existing) | ✅ incremental | ✅ incremental |
| NER/extraction | batch | batch | ✅ streaming |
| Live semantic search | — | — | ✅ |

## 6. Effort decomposition (agent-oriented units)

Per the AIWG `no-time-estimates` rule, no wall-clock estimates given.

**Phase 1 atomic scope count:** 6 active issues (#591..#596). #590 is closed
as superseded/obsolete. Foundation (#591), helpers + publisher (#592, #593)
remain sequential. Consumer-side (#594, #595, #596) is parallelizable after
#593.

**Agent mix per commissioning:**
- **Primary Author**: `architecture-designer` or `backend-engineer` for #591/#592/#593 design docs
- **Implementer**: `rust-engineer` (per issue, in dependency order)
- **Parallel Reviewers**: `database-architect` (schema), `reliability-engineer` (failure modes), `test-engineer` (chaos tests), `security-architect` (event payload PII review)
- **Synthesizer**: `documentation-synthesizer` for final ADRs

**Pass estimate to quality gate:** 1 design pass + 1 review-revision cycle expected per issue. #593 (publisher) likely needs 2 cycles due to failure-mode complexity.

## 7. Quality gates

**Phase 1 exit criteria** (all must be objectively verifiable):

- [ ] All 7 issues closed with merged PRs
- [ ] ADR-005..008 emitted, reviewed, accepted
- [ ] Integration test suite green in CI (no skipped tests, no ignored failures)
- [ ] Chaos test scenario set executed: kill-publisher, kill-redis, kill-consumer — zero event loss measurable
- [ ] Load test: 10K events/sec for 60s with publisher lag < 1s p99
- [ ] 2-week production soak under shadow mode shows zero delta in outcome vs legacy path
- [ ] Health metrics surfaced and dashboard documented in `docs/`
- [ ] Existing tests still pass (no regression in non-streaming flows)
