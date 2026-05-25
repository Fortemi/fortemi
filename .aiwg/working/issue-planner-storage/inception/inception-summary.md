# Inception Summary: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Inception (complete) → Elaboration (pending operator gate)
**Date**: 2026-05-21
**Source**: @.aiwg/working/issue-planner-storage/synthesis.md
**Inception artifacts directory**: `.aiwg/working/issue-planner-storage/inception/`

## Artifact Index

| Artifact | Path | What's in it |
|----------|------|-------------|
| Problem Statement | `problem-statement.md` | Operator's problem in their own words, why now (three 2025-2026 enabling shifts), stakeholder table (5 roles), constraints (no breaking changes, multi-tenant isolation preserved, fail-closed auth preserved, Fortemi never writes to user dirs), out-of-scope list (10 items pulled from synthesis §7) |
| Vision | `vision.md` | One-sentence vision, 6 measurable success criteria (each with a verifiable test), anti-vision section enumerating what success does NOT include (live watching, tree-sitter quality, remote storage, zero-config secret protection, web UI, source-path migration, overlap enforcement) |
| Risk Register v1 | `risk-register-v1.md` | Top 10 risks scored sev×lik with mitigation, owner-type, and workstream. Evidence tags carried from research streams (`established`/`emerging`/`speculative`). Retirement column marks which retire in Elaboration vs Construction vs operationally accepted. R-10 captures the deferred WS-5 (live watching) as a known unresolved risk requiring operator decision |
| Business Case Sketch | `business-case-sketch.md` | Value proposition (3 bullets), cost categories (development, maintenance, support, opportunity), alternatives considered (do nothing / external solution / sibling-trait split / per-blob mode as user concept), decision criteria (5 conditions for proceeding) |

## Most Important Inception Findings

1. **The architecture is additive, not a redesign.** Stream C source survey confirmed `StorageBackend::resolve_path()` and `extraction_handler.rs` path-access mode were designed in anticipation of this case. v1 is largely "finish what was started."
2. **Three risk hot-spots dominate** and all have established mitigations: secret leakage (R-1, mitigated by combined path + content denylist with quarantine), multi-tenant boundary (R-2, mitigated by canonicalization + allowlist), and Docker/NFS event reliability (R-4, retired for v1 by deferring live watching).
3. **Live watching deferral (WS-5) is the most consequential scope decision.** It is explicitly marked as the most important Phase 5 operator question (synthesis Q-1, risk R-10).

## Gate Criteria for Inception → Elaboration Transition

Per `.claude/rules/hitl-gates.md` (GATE-I2E), the following must be true before Elaboration begins. This gate is `mode: ALWAYS, timeout_action: block`.

- [ ] **Problem statement reviewed and accepted by operator.** The stakeholder table, constraints, and out-of-scope list reflect the operator's actual intent for #736.
- [ ] **Vision success criteria approved.** All 6 are measurable and reflect what "done" means for v1. Anti-vision is accepted as the explicit scope ceiling.
- [ ] **Risk register v1 reviewed.** Top-10 risks acknowledged, mitigation owners identified, retirement timeline accepted.
- [ ] **Operator answers synthesis §6 Phase 5 questions** OR explicitly accepts the recommended defaults for all eight (Q-1 through Q-8). Q-1 (live watching at v1?) is the highest-cost decision and the one most likely to alter scope. Recommended defaults from synthesis §6 are:
  - Q-1: Defer live watching entirely (A)
  - Q-2: Per-blob mode is system-only, not user-facing (A)
  - Q-3: No secret-scan opt-out in v1 (A)
  - Q-4: Extend `manage_archives` MCP tool; one new `rescan_archive` (A)
  - Q-5: Path allowlist mandatory when `FORTEMI_MULTI_TENANT=true`, optional otherwise (C)
  - Q-6: Multi-archive directory overlap allowed with warning (A)
  - Q-7: <10 min initial scan target for 10k-file repos on default CPU embedding (B)
  - Q-8: Lenient failure mode (warn-on-read, 503-on-write) as v1 default (A)
- [ ] **Business case decision criteria met.** Operator confirms the 5 conditions in `business-case-sketch.md` decision section, particularly (a) deferred-live-watching trade-off at Q-1 and (b) secret-scan mandatory-on default at Q-3.
- [ ] **Workstream decomposition (synthesis §4) accepted** as the basis for Elaboration-phase planning. WS-1 through WS-10 will become the issue-tree starting point in the Construction phase.

## Sign-Off Section

**Operator approval to proceed to Elaboration:**

- Approver name: ________________________
- Date: ________________________
- Decision: [ ] Approve — proceed to Elaboration   [ ] Reject — return for revision   [ ] Approve with revisions (specify below)
- Revisions / open items:

**Architecture designer acknowledgment (load-bearing decisions accepted):**

- Name: ________________________
- Date: ________________________
- Decisions 1-8 (synthesis §3) accepted as the v1 architectural baseline: [ ]
- Risk register v1 mitigations accepted as Elaboration-phase work: [ ]

**Security architect acknowledgment (R-1, R-2, R-7 mitigations accepted):**

- Name: ________________________
- Date: ________________________
- Threat model commitment for Elaboration: [ ]
- WS-9 multi-tenant security test suite scope accepted: [ ]
