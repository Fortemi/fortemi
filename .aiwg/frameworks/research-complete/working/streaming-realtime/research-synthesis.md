---
artifact: research-synthesis
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: draft-v2
last-updated: 2026-05-11
authors:
  - claude-opus-4-7 (orchestrator)
---

# Streaming Cluster — Research Synthesis (v2)

## 1. Purpose

This document is the literature backbone for Fortemi epic #586 (Streaming Data Capture & Realtime Processing). It cites the existing `roctinam/research-papers` corpus (REF-XXX) where applicable and flags genuine gaps. Phase 1 (Redis Streams + outbox) is the immediate construction target; Phases 2–3 (incremental AI pipeline, advanced streaming) are sequenced behind it.

Architectural decisions are locked by ADR-001..004 (see #586 comment, 2026-04-09). This synthesis does **not** revisit those decisions — it grounds the *execution* with literature support and surfaces residual risks.

## 2. Architectural priors (locked)

| ADR | Decision | Authority in corpus |
|---|---|---|
| 001 | Direct `redis-rs` streams API; no SeaStreamer abstraction layer | REF-327 (`redis-rs` Streams module), REF-328 (Redis Streams docs) — capability proven; YAGNI applied |
| 002 | Manual outbox; not WAL-replication CDC for Phase 1 | REF-322 (Postgres-as-Message-Bus outbox), REF-329 (Brandur/Stripe Unified Log), REF-330 (Redis event sourcing lessons); CDC alternatives REF-294 (Sequin) / REF-295 (PG CDC taxonomy) / REF-297 (pgwire-replication) retained as Phase 3 options |
| 003 | Redis Streams; not Kafka / NATS | REF-293 (Redis vs Kafka vs NATS), REF-304 (microservices comparison), REF-329 (Brandur unified log) |
| 004 | Edge tier = realtime wake-up + SSE/WS + FTS only; embed/link/extract stay batch | REF-313 (memory-aware batching), REF-315/316 (edge AI quant + DVFS), REF-690 (LLM latency 10–1000× too slow for realtime on constrained GPU); informs the hardware-tier capability matrix in #586 |

## 3. Phase-by-phase literature map

### 3.1 Phase 0 — Event-driven wake-up *(already implemented per #586 comment 2026-04-09)*

Verified against REF-320 (Postgres LISTEN/NOTIFY scaling cliff, `NotifyQueueLock` global contention at ~10K writers) and REF-321 (PgDog proxy decoupling client fan-out). The existing `tokio::Notify` mechanism in `crates/matric-jobs/src/worker.rs:299-312` is **strictly superior** for Fortemi's single-process architecture:

- In-process signal: <1ms wake latency (vs LISTEN/NOTIFY network round-trip)
- No global lock contention (REF-320's failure mode is not reachable)
- All insertion paths already call `notify.notify_waiters()` (`jobs.rs:268, 320, 359`)

**No change required.** REF-320/321 remain as guard-rails should we ever multi-process the worker.

### 3.2 Phase 1 — Outbox + Redis Streams event bus *(immediate construction target)*

**Corpus coverage is strong:**

- **Outbox pattern**: REF-322 (PG-as-message-bus), REF-329 (Stripe Unified Log — direct production-scale precedent), REF-330 (Redis event sourcing 1-year retrospective) — together describe the exact pattern: write to PG in same transaction as data change, background publisher promotes to Redis Stream, marks consumed.
- **Redis Streams operational model**: REF-327 (`redis-rs` API surface — XCLAIM/XPENDING/consumer groups), REF-328 (official docs — MAXLEN/MINID trimming, exactly-once-per-group semantics).
- **Scaling reference points**: REF-324 (RudderStack 100K events/sec on partitioned PG SKIP LOCKED — sets a lower bound on what PG can sustain *before* needing Redis fan-out); REF-329 (Stripe-scale validation of hybrid PG+Redis).
- **Failure-mode literature**: REF-330 (lessons from 1-year production — outbox lag, retry storms, dual-write hazards).

**Genuine gaps surfaced for new induction** (subject to the parallel web-research agent's confirmation):

| Gap | Why corpus is thin |
|---|---|
| Observability/health-metrics design for outbox publisher lag (XLEN, XPENDING, oldest-unconsumed-age) | REF-329/330 are anecdotal; no formal observability spec |
| Idempotency-key conventions at outbox boundary | REF-322 mentions at-least-once but doesn't prescribe key design |
| Backpressure when consumer (e.g., embed worker) is slower than producer | REF-681 (Reactive Streams 2015) is the theory; need 2024–2026 practice in async Rust / tokio |

### 3.3 Phase 2 — Incremental AI pipeline

**Corpus coverage:**

- **Incremental embedding**: REF-318 (Drift-Adapter — zero-downtime model upgrades, 95-99% recall, <10µs overhead, 100× cost reduction) directly informs #605. REF-317 (VectraFlow — streaming vector search) informs #604.
- **Incremental KG construction**: REF-310 (iText2KG — Document Distiller + blueprint), REF-311 (IncRML — RML mappings + CDC + LDES, 11–57× faster than full regen) → both inform #599 and #606.
- **Incremental document summarization**: REF-308 (ACL 2024) → adjacent prior art for streaming-rewriting patterns.
- **Dynamic GPU batching**: REF-313 (memory-aware batching) directly informs #600; complemented by Aegaeon GPU pooling (SOSP 2025, indexed in 1578) and the LLM-serving prior art (REF-101 Orca continuous batching, REF-102 vLLM PagedAttention, REF-276 LLM serving survey 2024).

**Gaps surfaced:**

- Content-hash-aware skip rules for embedding (#597, #598) — REF-318 covers model-version invalidation; corpus is thin on *content-version* SimHash/MinHash gating.
- Matryoshka-aware partial re-embed (low-dim quick refresh + high-dim deferred) — corpus has no MRL-specific paper for streaming.

### 3.4 Phase 3 — Advanced streaming

**Corpus coverage:**

- **CDC alternatives** for #603: REF-294 (Sequin — 6.8× faster than Debezium, single Docker), REF-295 (CDC taxonomy), REF-297 (`pgwire-replication` pure-Rust WAL consumer), REF-323 (ElectricSQL durable streams — WAL Shapes, HTTP offset tracking).
- **Live semantic search** for #604: REF-317 (VectraFlow — streaming vector search as operator).
- **Drift-Adapter** for #605: REF-318 directly named.
- **Streaming KG** for #606: REF-310 / REF-311 directly named; REF-291 / REF-292 (stream-reasoning, RDF stream processing) as theoretical scaffolding.

**Critical open question — REF-563 GAP NOTE:**
> *"Documents absence of systems-community work treating LLM inference as a first-class stream operator with event-time/watermark/exactly-once semantics."*

This gap is **structural**, not just a missing paper. Phase 3 #606 (streaming KG via incremental NER) bumps directly into it. The parallel web-research agent is searching SOSP/OSDI/EuroSys/CIDR 2025 specifically to see if anyone closed it; if not, we accept the gap and design defensively (no exactly-once promise for LLM-emitted side-effects; mark them at-least-once + idempotent).

## 4. Foundational theory (background)

For implementers reading this fresh, the theoretical foundation is laid by:

- **REF-280** (Dataflow Model, Akidau/Google VLDB 2015) — what/where/when/how decomposition of unbounded streams. The mental model for Phase 1+ design.
- **REF-282** (Asynchronous Barrier Snapshotting, Carbone 2015) — Chandy-Lamport refinement for distributed snapshots. Relevant if we ever add stateful stream operators.
- **REF-286** (One SQL to Rule Them All, Begoli/Akidau SIGMOD 2019) — stream-table duality. Justifies the "outbox is a table that doubles as a log" view.
- **REF-287** (Watermarks, Begoli VLDB 2021) — watermark semantics, completeness vs latency trade-off. Becomes load-bearing if we add event-time windowing in Phase 3.
- **REF-676** (Kreps "The Log", 2013) — offset-based replay; foundational for consumer-group resumability.
- **REF-681** (Reactive Streams Spec, 2015) — credit-based demand protocol; theoretical anchor for Phase 1 backpressure design.

## 5. Citable claims for SDLC artifacts (quick lookup)

| Claim in feature plan / ADR | REF |
|---|---|
| "PG LISTEN/NOTIFY has a scaling cliff at ~10K writers" | REF-320 |
| "Outbox + transactional event capture is the proven CDC-light pattern" | REF-322, REF-329 |
| "Redis Streams provides consumer groups with at-least-once-per-group" | REF-328 |
| "RudderStack ran 100K events/sec on partitioned SKIP LOCKED PG queues" | REF-324 |
| "Drift-Adapter shows 95-99% recall with <10µs overhead during embedding model upgrades" | REF-318 |
| "Stream-table duality makes outbox-tables and Redis-streams interchangeable views" | REF-286 |
| "Memory-aware admission control beats fixed-N GPU semaphores under variable VRAM" | REF-313 |
| "LLM inference as a watermark-aware stream operator is an open research question" | REF-563 GAP NOTE |

## 6. Open questions deferred to design ADRs

1. **Outbox retention** — how long do we keep consumed rows? (REF-330 retrospective: months caused queue bloat; weekly partition drop preferred per REF-324)
2. **XADD MAXLEN trimming policy** — fixed-length ring buffer, approximate trim (`~`), or time-windowed? (REF-328 documents trade-offs; needs a Fortemi-specific decision)
3. **Consumer-group naming** — one group per service (sse/ws/webhook) vs one group per worker pool? Affects rebalancing semantics. (REF-327/328)
4. **Idempotency-key shape** — `(event_type, aggregate_id, version)` vs ULID per event? Cross-cuts #594/#595 handlers and #598/#599 idempotent consumers.

These four are the ADR backlog for the engineering team once commissioned.

## 7. Provenance & limitations

- Corpus IDs (REF-XXX) drawn from `roctinam/research-papers/INDEX.md` (fetched 2026-05-11).
- ADRs cited as locked are from #586 comment by `roctibot` 2026-04-09; **note**: the prior orchestration referenced ADR-001..004 files at `.aiwg/architecture/streaming-realtime/`, but those files were never committed to this working tree. The accepted decisions are recorded only in the issue comment. Recommend re-emitting them as proper ADR files under `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/adrs/` during commissioning.
- New 2026 sources from the parallel web-research pass will be appended in §8 once the agent returns and INDUCT issues are filed.

## 8. New candidate sources (research-agent 2026-05-11)

Eight candidates surfaced. Gap-3 (content-hash-aware incremental embedding) returned no peer-reviewed 2025 result — recorded as a residual GAP NOTE candidate rather than padded with weak sources.

### 8.1 Gap area 1 — Outbox at scale (3 candidates)

| # | Title | Year | Source | Grade | Phase |
|---|---|---|---|---|---|
| C1 | **Transactional Outbox Pattern: From Theory to Production** (Pionovskyi) | 2025 | `npiontko.pro` | B | 1 |
| C2 | **Building Reliable Agents with the Transactional Outbox Pattern and Redis Streams** (Redis Inc.) | 2025 | `dev.to/redis` | B | 1 |
| C3 | **Push-based Outbox Pattern with Postgres Logical Replication** (Dudycz) | 2024–2025 | `event-driven.io` | B | 1 |

**Why these fill genuine gaps beyond REF-322 / REF-329 / REF-330:**
- C1: 2025 operational refinements (dispatcher poisoning, SKIP-LOCKED concurrency with Redis downstream, idempotency-key retention math) absent from corpus.
- C2: First-party Redis-authored guidance — XAUTOCLAIM rebalancing, PEL depth as health signal, idempotency-key dedup at consumer. Directly informs #593 (publisher) and #596 (metrics).
- C3: Push-outbox via logical replication slot — eliminates the `published_at UPDATE` hot-row contention. Becomes a Phase-3 alternative (#603 CDC eval).

### 8.2 Gap area 2 — Backpressure in async AI pipelines (2 candidates)

| # | Title | Year | Source | Grade | Phase |
|---|---|---|---|---|---|
| C4 | **Backpressure Patterns for LLM Pipelines: Why Exponential Backoff Isn't Enough** (Tian Pan) | 2026 | `tianpan.co` | B | 2 |
| C5 | **Backpressure by Design in 2025: Concurrency Limits, Admission Control, Queueing Patterns** | 2025 | `debugg.ai` | B | 1 |

**Why these fill genuine gaps beyond REF-681:**
- C4: First applied write-up tying token-aware shedding to credit-based demand; documents the 2023→2025 token-distribution drift (sub-100-tok queries 80%→20%) that breaks naive backoff/retry.
- C5: Implementation pattern catalog (admission gates in front of bounded channels) tuned for AI worker stalls.

### 8.3 Gap area 3 — Content-hash-aware incremental embedding (0 candidates — residual GAP)

**No peer-reviewed 2025 result.** Strongest adjacent work (SemHash, pgai Vectorizer) is library/tooling, not research. **Recommendation**: file a GAP NOTE in `roctinam/research-papers` companion to REF-563 — "no 2025 peer-reviewed work on chunk-level SHA/SimHash embed-skip with Matryoshka-aware partial invalidation; topic remains open." Continue with engineering-only design for #597/#598, no literature dependency.

### 8.4 Gap area 4 — LLM inference as stream operator (3 candidates, closes REF-563)

| # | Title | Year | Source | Grade | Phase |
|---|---|---|---|---|---|
| C6 | **Pie: A Programmable Serving System for Emerging LLM Applications** | 2025 | SOSP 2025 | **A** | 2 |
| C7 | **TurboSpec: Closed-loop Speculation Control for Optimizing LLM Serving Goodput** | 2024–2025 | arXiv:2406.14066 | A- | 3 |
| C8 | **AdaSpec: Adaptive Speculative Decoding for Fast, SLO-Aware LLM Serving** | 2025 | arXiv:2503.05096 | B | 3 |

**Why these fill the REF-563 GAP NOTE:**
- C6 (Pie, SOSP 2025) — **Grade A peer-reviewed**. Decomposes LLM generation loop into programmable handlers (embed/forward/sample/KV-page alloc/free) exposed via WASM "inferlets". This is the **first system that exposes the LLM forward pass as a programmable stage** — the missing primitive for watermark / exactly-once hooks. Directly addresses the structural gap REF-563 documented.
- C7 (TurboSpec) — Closed-loop control-theoretic admission for speculative decoding under variable load. Maps onto "speculative re-execution under model swap" for #605 Drift-Adapter integration.
- C8 (AdaSpec) — SLO-aware adaptive K per request. Watermark-friendly admission knob.

### 8.5 Action: INDUCT issues to file

One **parent cluster issue** in `roctinam/research-papers` + **8 child INDUCT issues** (C1..C8) + **1 residual GAP NOTE** for area 3. Cross-links: REF-563 (the existing GAP NOTE Pie closes), REF-322/329/330 (outbox parents), REF-681 (backpressure parent), REF-318 (Drift-Adapter — pair with C7).

