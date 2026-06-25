# Flowgate: Streaming Data Capture & Realtime Processing

**Track**: fortemi/streaming-realtime
**Current Phase**: Elaboration (Research → Architecture)
**Gate**: Elaboration Entry Gate
**Date**: 2026-04-08

---

## Gate Criteria Assessment

### Inception Deliverables (Required for Elaboration Entry)

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Problem validated | PASS | Batch-oriented AI pipeline creates latency; existing event output infrastructure underutilized |
| Vision documented | PASS | Hardware-tiered streaming architecture with progressive enhancement |
| Key risks identified | PASS | 5 risks documented with mitigations in feature-plan.md |
| Architecture sketch | PASS | Data flow diagram + component breakdown in feature-plan.md |
| Stakeholder alignment | PASS | Hardware tier strategy reviewed and accepted (2026-04-09) |
| Research complete | PASS | 51 sources across 4 domains; synthesis document complete |
| Sources filed for induction | PASS | 51 issues filed in roctinam/research-papers (#211-#261) |
| Architecture decisions | PASS | 4 ADRs accepted (2026-04-09) |

### Gate Decision

**PASS** — All criteria met. Architecture decisions accepted 2026-04-09:
1. ADR-001: Direct redis-rs streams (no SeaStreamer)
2. ADR-002: Manual outbox pattern (no CDC tooling)
3. ADR-003: Redis Streams (no NATS)
4. ADR-004: Selective realtime on edge (wake-up + SSE + FTS)

---

## Elaboration Phase Plan

### Objective
Produce a baselined architecture for streaming data capture with implementation-ready specifications for Phase 0 and Phase 1.

### Deliverables

1. **Architecture Decision Records (ADRs)** -- COMPLETE
   - [x] ADR-001: Stream processing backend → Direct redis-rs
   - [x] ADR-002: CDC approach → Manual outbox
   - [x] ADR-003: Message broker → Redis Streams
   - [x] ADR-004: Edge tier realtime scope → Selective (wake-up + SSE + FTS)

2. **Technical Specifications**
   - [ ] Event outbox table schema + migration
   - [ ] Redis Streams consumer group design
   - [ ] PgListener integration specification
   - [ ] Incremental embedding change detection algorithm
   - [ ] Dynamic GPU batching algorithm

3. **Proof of Concepts**
   - [ ] PoC: PgListener wake-up in job worker (Phase 0)
   - [ ] PoC: Redis Streams XADD/XREADGROUP round-trip
   - [ ] PoC: SeaStreamer with Redis backend in Fortemi context

4. **Test Strategy**
   - [ ] Integration tests for event flow (PG → Redis Stream → consumer)
   - [ ] Performance benchmarks: wake-up latency, fan-out throughput
   - [ ] Hardware tier simulation tests

### Agents Required

| Role | Responsibility |
|------|---------------|
| Architecture Designer | ADRs, component design |
| Systems Engineer | PoC implementation, integration specs |
| Test Architect | Test strategy, benchmark design |
| Security Architect | Event data exposure review, auth for streams |

### Risks to Retire in Elaboration

| Risk | Retirement Strategy |
|------|-------------------|
| SeaStreamer fitness | PoC with Redis backend |
| Redis Streams memory on edge | Benchmark with MAXLEN trimming |
| PgListener reliability | PoC with reconnection handling |
| Multi-memory stream routing | Design review with schema-per-archive |

---

## Next Steps

1. ~~File feature epic issue in `fortemi/fortemi`~~ — DONE (#586)
2. ~~ADR drafting~~ — DONE (4 ADRs accepted)
3. Implement Phase 0 (PgListener wake-up — lowest risk, immediate value)
4. Implement Phase 1 (Redis Streams event bus + outbox)
5. File Phase 0 and Phase 1 implementation issues in fortemi/fortemi

---

## Track History

| Date | Event |
|------|-------|
| 2026-04-08 | Research initiated — 4 parallel research agents |
| 2026-04-08 | Research complete — 51 sources across 4 domains |
| 2026-04-08 | 51 issues filed in roctinam/research-papers (#211-#261) |
| 2026-04-08 | Research synthesis + feature plan + gate check created |
| 2026-04-08 | Feature epic filed: fortemi/fortemi#586 |
| 2026-04-09 | 4 ADRs accepted: redis-rs, outbox, Redis Streams, selective edge realtime |
| 2026-04-09 | Gate upgraded: CONDITIONAL PASS → PASS |
| 2026-04-09 | Feature plan + gate check updated with decisions |
| 2026-04-08 | Flowgate track opened — Elaboration entry (conditional) |
