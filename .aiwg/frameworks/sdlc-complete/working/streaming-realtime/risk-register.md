---
artifact: risk-register
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: draft-v1
last-updated: 2026-05-11
---

# Streaming Cluster — Risk Register (Phase 1)

Scored on standard 1–5 likelihood × 1–5 impact. Owners are role labels; assignment happens at commissioning.

## Critical (L×I ≥ 16)

| ID | Risk | L | I | L×I | Mitigation | Owner | Refs |
|---|---|---|---|---|---|---|---|
| R-01 | **Outbox row-update hot-row contention** — `UPDATE event_outbox SET published_at = NOW()` becomes the bottleneck under load. Symptoms: publisher CPU pinned, lag grows, PG vacuum unable to keep up. | 3 | 5 | 15 | Partition outbox by week (REF-324 pattern). Consider push-outbox via logical replication (C3/INDUCT-#filed-after-reauth) as Phase 3 alternative if observed. PoC P-01 must measure this. | DB architect | REF-324, C3 |
| R-02 | **Dual-write inconsistency during shadow-mode rollout** — `OUTBOX_FANOUT=shadow` writes events twice (old broadcast + new outbox). If old path emits but outbox txn fails, consumers under shadow see drift. | 3 | 5 | 15 | Outbox write happens *inside* the same PG transaction as the data write — atomicity guarantees consistency. Shadow consumers compare event-id streams; differential alerts on drift. Cutover gates on zero drift for 1 week. | Reliability eng | REF-322, REF-330 |

## High (L×I 10–15)

| ID | Risk | L | I | L×I | Mitigation | Owner | Refs |
|---|---|---|---|---|---|---|---|
| R-03 | **Consumer-group rebalance loses pending events on Redis restart** — Redis is not the source of truth, but consumer groups have in-memory PEL state that survives only if Redis persistence is configured. | 3 | 4 | 12 | Redis AOF + `XAUTOCLAIM` on consumer reconnect. Publisher republishes from outbox if event-id not seen by consumer within SLA. Document RTO/RPO in runbook. | Reliability eng | REF-328, C2 |
| R-04 | **Slow consumer blocks publisher progress** — webhook dispatcher hits unreachable external endpoint; PEL grows; XLEN grows; outbox lag grows. | 4 | 3 | 12 | Separate consumer groups per service (sse/ws/webhook). Webhook dispatcher uses bounded retry + DLQ stream (`webhook-dlq`). Publisher is decoupled from any single consumer. | Backend eng | C4, C5 |
| R-05 | **Idempotency-key collisions** — Two events with the same `(aggregate_id, version)` get published; consumers see duplicates. | 2 | 4 | 8 | ADR-008 mandates ULID per outbox row as primary idempotency key. Consumers maintain dedup TTL window (Redis `SET NX EX`, 24h). | Backend eng | C1, C2 |
| R-06 | **Event payload PII leakage** — Event JSONB contains note titles, user identifiers; Redis stream payloads are not encrypted at rest by default in the bundle. | 3 | 4 | 12 | Security review per CLAUDE.md §Auth. Bundle deployment doc explicitly states Redis is not PII-safe without TLS+ACL. Add ADR for payload-shape minimization (event-id refs PG, not full body) where feasible. | Security architect | CLAUDE.md `REQUIRE_AUTH` |

## Medium (L×I 5–9)

| ID | Risk | L | I | L×I | Mitigation | Owner | Refs |
|---|---|---|---|---|---|---|---|
| R-07 | **Stream retention vs replay needs conflict** — Aggressive `MAXLEN` trimming loses events; lenient trimming bloats Redis memory. | 3 | 3 | 9 | ADR-006 decides per-stream trim policy. Time-windowed trim (last 7 days) for SSE/WS; longer for webhook (matches retry budget). | DB architect | REF-328 |
| R-08 | **`tokio::Notify` wake signal lost during publisher restart** — Outbox rows inserted while publisher is down don't wake the new publisher immediately. | 4 | 2 | 8 | Publisher always polls on startup before waiting on notify. Polling fallback every 5s caps worst-case latency. | Backend eng | jobs.rs:268 |
| R-09 | **Multi-memory archive routing complexity** — Per-archive events need correct schema context on consumer side (CLAUDE.md Multi-Memory section). | 3 | 3 | 9 | `archive_id` column in outbox + consumer-side `SET LOCAL search_path` per event. Test with 3+ archives. | Backend eng | CLAUDE.md Multi-Memory |
| R-10 | **CI test infrastructure load** — Integration tests need PG+Redis+pgvector containers per run; chaos tests need fault injection. | 3 | 3 | 9 | Reuse existing `matric-builder` runner stack. Chaos via `toxiproxy` or `tc netem`. | Test eng | CLAUDE.md Testing |

## Low (L×I ≤ 4)

| ID | Risk | L | I | L×I | Mitigation | Owner |
|---|---|---|---|---|---|---|
| R-11 | Health metric cardinality explosion (event-type × consumer-group). | 2 | 2 | 4 | Cap to top-N event types; bucket rest. |
| R-12 | Docker bundle entrypoint sequencing — Redis not ready when publisher starts. | 2 | 2 | 4 | Existing healthcheck dependency in docker-compose.bundle.yml. |

## Cross-phase risks (deferred but tracked)

| ID | Risk | Phase | Refs |
|---|---|---|---|
| R-13 | LLM side-effects (incremental NER on stream) cannot be exactly-once. | 2/3 | REF-563 (gap remains structural even after C6 closes it for the inference-loop level) |
| R-14 | MRL-aware partial re-embed has no literature backing (#605 GAP NOTE). | 2 | filed-#605 |
| R-15 | Drift-Adapter integration (#605 impl) interacts with speculative decoding (C7) — unstudied. | 3 | C7 |
