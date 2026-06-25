---
artifact: commissioning-brief
project: fortemi
cluster: streaming-realtime
epic: fortemi/fortemi#586
status: ready-for-review
last-updated: 2026-05-11
audience: project owner (roctinam)
purpose: enable commissioning decision for Phase 1 work
---

# Streaming Cluster — Commissioning Brief

This brief summarizes what's ready to commission, in what order, with what agent mix. Quoting the AIWG `no-time-estimates` rule, effort is expressed as **scope count + agent count + parallelism + pass estimate** rather than wall-clock.

## TL;DR

- **PoCs P-01 + P-02 complete** (2026-05-12). P-02 ✅ PASS. P-01 ⚠️ CONDITIONAL — architecture sound, but the original 10K/s SLA was edge-tier-impossible. Revised SLA is **tiered** (see feature-plan §2.4). High-end titan run (P-01b) pending.
- **Recommend commissioning Phase 1 Slate B** (#590, #591) once titan P-01b confirms high-end SLA ceiling. Slates C/D/E hold for the revised SLA + new risks (R-09 writer backpressure, R-10 PEL stranding, R-11 cold-start).
- **Phase 2 and Phase 3** stay backlog-only until Phase 1 has 2-week production soak data.
- C3 induction filed as `section9/research-papers#606` after user re-auth.

## PoC verdict matrix

| PoC | Status | Headline finding | Artifact |
|---|---|---|---|
| P-01 outbox load | ⚠️ Conditional | 10K/s edge SLA unachievable; revised to tiered (edge: 1K/s stock, 2.5K/s tuned; high-end TBD) | `poc-results/p01-outbox-load.md` |
| P-02 consumer chaos | ✅ Pass | F1/F2/F3 pass; consumer-side dedup → 0% observable dup rate; supervised-process requirement surfaced | `poc-results/p02-consumer-chaos.md` |
| P-01b titan high-end | ⏳ Dispatching | Sets the high-end ceiling for the tiered SLA | `poc-results/p01b-titan-load.md` (pending) |

## New risks added post-PoC

- **R-09** Writer-side backpressure undesigned (Critical, L=4 I=4)
- **R-10** PEL stranding on consumer restart (High, L=4 I=3)
- **R-11** Cold-start latency dominates F1 recovery (Medium)

See `risk-register.md` for mitigations.

## Deliverables produced this session

| Artifact | Location | Status |
|---|---|---|
| Research synthesis (v2) | `.aiwg/frameworks/research-complete/working/streaming-realtime/research-synthesis.md` | Draft, citing REF-280..330 + 8 new candidates |
| Feature plan (v2) | `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/feature-plan.md` | Draft, Phase 1 detailed; Phase 2/3 sketched |
| Risk register | `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/risk-register.md` | 15 risks (2 critical, 4 high, 4 medium, 2 low, 3 cross-phase) |
| PoC plan | `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/poc-plan.md` | 2 PoCs scoped with objective pass criteria |
| Commissioning brief (this doc) | `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/commissioning-brief.md` | Ready for your review |
| INDUCT issues in research-papers | #597 parent + #598/599/600/601/602/603/604 (children C1, C2, C4, C5, C6, C7, C8) + #605 GAP NOTE | Filed |
| INDUCT issue C3 (Dudycz push-outbox) | — | **Blocked, needs re-auth** |

## Commissioning slate (Phase 1)

### Slate A — PoCs (commission first)

| Work item | Scope | Agent mix | Parallelism | Pass est. |
|---|---|---|---|---|
| **P-01** Outbox publisher load test | Build minimal `poc_outbox.rs`; 10K events/sec for 60s; partitioned vs not | 1 backend eng (impl) + 1 db architect (review) | parallel with P-02 | 1 pass + 1 review cycle |
| **P-02** Consumer chaos test | Build minimal `poc_consumer.rs`; fault injection F1/F2/F3 | 1 backend eng (impl) + 1 reliability eng (review) | parallel with P-01 | 1 pass + 1 review cycle |

**Gate**: both PoCs PASS → proceed to Slate B. If either FAILS → escalate to user.

### Slate B — Foundation (commission after PoCs pass)

| Issue | Scope | Agent mix | Sequencing | Pass est. |
|---|---|---|---|---|
| **#590** Enable redis streams feature | `Cargo.toml` feature flag toggle | 1 rust eng | parallel w/ #591 | 1 pass |
| **#591** event_outbox migration | New migration; schema with ULID, partitioning, indexes | 1 db architect (author) + 1 rust eng (impl) + 1 security architect (PII review) + 1 test eng (migration test) | parallel w/ #590 | 1 design + 1 review cycle |
| **#592** Outbox insert helpers | `outbox_insert(tx, ...)` API; migrate existing emitters | 1 rust eng (impl) + 1 backend eng (review) | after #591 | 1 design + 1 review cycle |

### Slate C — Publisher (after Slate B)

| Issue | Scope | Agent mix | Sequencing | Pass est. |
|---|---|---|---|---|
| **#593** Outbox-to-Redis publisher | Tokio task; batch loop; failure handling; trim policy | 1 rust eng + 1 reliability eng (review) + 1 db architect (review) + 1 test eng (chaos tests) | after #592 | 1 design + 2 review cycles (complexity) |

### Slate D — Consumers + observability (after Slate C; parallelizable internally)

| Issue | Scope | Agent mix | Sequencing |
|---|---|---|---|
| **#594** SSE consumer migration | XREADGROUP; bounded channel; XAUTOCLAIM | 1 rust eng + 1 reliability eng | parallel within slate |
| **#595** WS + webhook consumer | Per-service groups; DLQ for webhook | 1 rust eng + 1 reliability eng | parallel within slate |
| **#596** Health metrics | Prometheus gauges; `/api/v1/health/streaming` | 1 backend eng + 1 reliability eng | parallel within slate |

### Slate E — ADR backlog (parallel with B/C/D)

| ADR | Subject | Author | Reviewers |
|---|---|---|---|
| ADR-005 | Outbox retention policy | db architect | reliability + backend |
| ADR-006 | XADD MAXLEN trimming strategy | reliability eng | db + backend |
| ADR-007 | Consumer-group topology | reliability eng | backend + db |
| ADR-008 | Idempotency-key shape | backend eng | db + reliability |

Each ADR follows the AIWG multi-agent pattern: Primary Author → Parallel Reviewers (3) → Synthesizer → Archive at `.aiwg/frameworks/sdlc-complete/working/streaming-realtime/adrs/`.

## Cumulative scope

- **PoCs**: 2 scope items
- **Phase 1 issues**: 7 (#590..#596)
- **Phase 1 ADRs**: 4 (005..008)
- **Total Phase 1 scope items**: 13
- **Unique agent roles**: 6 (rust eng, backend eng, db architect, reliability eng, security architect, test eng)
- **Critical paths**: #591 → #592 → #593 → {#594, #595} (sequential)
- **Parallel batches**: PoCs (2-way), Slate D (3-way), ADR batch (4-way)

## What this brief does NOT do

- Authorize work — that's your call
- Lock in agent assignments — those happen per-issue at commissioning time via the AIWG multi-agent pattern (Primary Author → Parallel Reviewers → Synthesizer)
- Estimate wall-clock — see `no-time-estimates` rule
- Commit to Phase 2/3 scope — explicitly deferred until Phase 1 production soak data exists

## Decision points for your review

1. **Approve the 2 PoCs**? (P-01 outbox load, P-02 consumer chaos) — recommend yes.
2. **Approve Slate B/C/D/E** *contingent on PoC pass*? — recommend yes, with re-confirmation after PoC results.
3. **Re-authorize INDUCT issue C3** (Dudycz push-outbox) in `section9/research-papers`? — recommend yes; the issue body was already drafted and would have been #-after-605.
4. **Where to commit these `.aiwg/` artifacts**? Currently in `.aiwg/frameworks/{sdlc-complete,research-complete}/working/streaming-realtime/`. The prior orchestration (epic #586) referenced `.aiwg/research/streaming-realtime/` and `.aiwg/architecture/streaming-realtime/` paths that never landed — recommend committing under the current multi-framework layout.

## Pointer to the rest

- Risk register has 15 items, prioritized — review before greenlighting #593 in particular (R-01 is the long pole)
- PoC plan has objective pass criteria, no vague-discretion conditions
- Research synthesis grounds every claim in REF-XXX or the 8 new candidates (C1..C8)
- Locked ADR-001..004 are still authoritative; ADR files themselves need re-emission (only the issue comment captured the decisions)
