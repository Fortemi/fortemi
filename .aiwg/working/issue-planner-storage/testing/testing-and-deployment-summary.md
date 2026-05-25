# Testing & Deployment Corpus — Summary Index

**Issue**: fortemi/fortemi#736 — Referenced storage mode + scan-and-ingest
**Phase**: Phase 3 — SDLC Corpus Generation
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

One-page index of the Phase 3 testing and deployment corpus.

---

## Documents Produced

### Testing

| Document | Purpose |
|---|---|
| `testing/test-strategy.md` | Master test strategy: scope, pyramid, test categories, fixtures, CI integration, coverage targets, mutation-testing posture |
| `testing/tenant-isolation-regression-suite.md` | TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10 — explicit attacker scenarios for cross-tenant isolation, path traversal, secret quarantine, mount failure modes |
| `testing/test-plan-construction.md` | Per-workstream test case inventory: 104 test cases across WS-1, WS-2, WS-3, WS-4, WS-6, WS-7, WS-8, WS-9, WS-10 plus 2 performance smoke benchmarks |

### Deployment

| Document | Purpose |
|---|---|
| `deployment/deployment-plan.md` | Bind mounts, env vars, schema migration (5 nullable columns), feature-flag rollout, rollback procedure, operational runbook stubs, new metrics + alerts |
| `deployment/operational-readiness-checklist.md` | 12-factor compliance, health checks (liveness/readiness/deep), SIGTERM graceful shutdown for scan worker, stateless-process discipline, structured logs, env-config validation, GATE-C2T pre-production checklist |

---

## TL;DR

- **104 test cases** across the 9 active workstreams (WS-5 deferred); pyramid is 39 unit / 47 integration / 3 e2e / 10 security regression / 2 performance smoke. Aggregate coverage floor of new code is **85%**; secret detection (WS-3) and security regression (WS-9) are gated at **90%** and **100% of TI-EXTSTORAGE-***.
- **Deployment is additive and feature-flagged**: new env var `FORTEMI_EXTERNAL_STORAGE_ENABLED` defaults OFF; soft rollback flips it back to OFF without data loss; the source-preservation invariant (Fortemi never writes/deletes user-owned source files) is verified at the trait layer, the Docker `:ro` mount layer, and the TI-EXTSTORAGE-9 regression test.
- **Operational readiness gate**: scan worker is stateless (state in PostgreSQL via existing `jobs` table), disposable (SIGTERM checkpoints mid-scan and resumes cleanly via BLAKE3 dedup), and instrumented (8 new Prometheus-style metrics, 5 new alerts including SourcePathUnreachable as critical).

---

## File Paths

Absolute paths for the deliverables produced by this phase:

- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/testing/test-strategy.md`
- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md`
- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/testing/test-plan-construction.md`
- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/deployment/deployment-plan.md`
- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/deployment/operational-readiness-checklist.md`
- `/home/roctinam/dev/fortemi/fortemi/.aiwg/working/issue-planner-storage/testing/testing-and-deployment-summary.md` (this document)

---

## References

- @.aiwg/working/issue-planner-storage/synthesis.md — Phase 2 synthesis (sole input)
- @.aiwg/working/issue-planner-storage/architecture/ — sibling Phase 3 architecture artifacts (separate generator)
- @.aiwg/working/issue-planner-storage/requirements/ — sibling Phase 3 requirements artifacts (separate generator)
- @.aiwg/working/issue-planner-storage/inception/ — sibling Phase 3 inception artifacts (separate generator)
